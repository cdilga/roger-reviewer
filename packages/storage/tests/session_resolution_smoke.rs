use tempfile::tempdir;

use roger_app_core::ReviewTarget;
use roger_storage::{
    CreateReviewSession, CreateSessionLaunchBinding, LaunchSurface, ResolveSessionReentry, Result,
    RogerStore, SessionFinderQuery, SessionReentryResolution,
};

fn target(repository: &str, pull_request_number: u64) -> ReviewTarget {
    ReviewTarget {
        repository: repository.to_owned(),
        pull_request_number,
        base_ref: "main".to_owned(),
        head_ref: format!("feature-{pull_request_number}"),
        base_commit: "aaa".to_owned(),
        head_commit: "bbb".to_owned(),
    }
}

#[test]
fn single_strong_repo_local_match_resumes_safely() -> Result<()> {
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path())?;

    let review_target = target("owner/repo", 42);
    store.create_review_session(CreateReviewSession {
        id: "session-1",
        review_target: &review_target,
        provider: "opencode",
        session_locator: None,
        resume_bundle_artifact_id: None,
        continuity_state: "awaiting_resume",
        attention_state: "awaiting_user_input",
        launch_profile_id: None,
    })?;
    store.put_session_launch_binding(CreateSessionLaunchBinding {
        id: "binding-cli",
        session_id: "session-1",
        repo_locator: "owner/repo",
        review_target: Some(&review_target),
        surface: LaunchSurface::Cli,
        launch_profile_id: None,
        ui_target: Some("cli"),
        instance_preference: Some("reuse_if_possible"),
        cwd: Some("/tmp/repo"),
        worktree_root: None,
    })?;

    let resolution = store.resolve_session_reentry(ResolveSessionReentry {
        explicit_session_id: None,
        repository: Some("owner/repo".to_owned()),
        pull_request_number: Some(42),
        source_surface: LaunchSurface::Cli,
        ui_target: Some("cli".to_owned()),
        instance_preference: Some("reuse_if_possible".to_owned()),
    })?;

    assert!(
        matches!(
            &resolution,
            SessionReentryResolution::Resolved { session, binding }
                if session.id == "session-1"
                    && session.review_target == review_target
                    && binding.as_ref().expect("binding").id == "binding-cli"
        ),
        "expected resolved re-entry, got {resolution:?}"
    );

    Ok(())
}

#[test]
fn ambiguous_or_missing_repo_matches_require_picker_instead_of_guessing() -> Result<()> {
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path())?;

    for (session_id, pr) in [("session-1", 42_u64), ("session-2", 43_u64)] {
        let review_target = target("owner/repo", pr);
        store.create_review_session(CreateReviewSession {
            id: session_id,
            review_target: &review_target,
            provider: "opencode",
            session_locator: None,
            resume_bundle_artifact_id: None,
            continuity_state: "awaiting_resume",
            attention_state: "awaiting_user_input",
            launch_profile_id: None,
        })?;
    }

    let ambiguous = store.resolve_session_reentry(ResolveSessionReentry {
        explicit_session_id: None,
        repository: Some("owner/repo".to_owned()),
        pull_request_number: None,
        source_surface: LaunchSurface::Cli,
        ui_target: Some("cli".to_owned()),
        instance_preference: Some("reuse_if_possible".to_owned()),
    })?;
    assert!(
        matches!(
            &ambiguous,
            SessionReentryResolution::PickerRequired { reason, candidates }
                if reason.contains("multiple repo-local sessions") && candidates.len() == 2
        ),
        "expected picker-required ambiguity, got {ambiguous:?}"
    );

    let missing = store.resolve_session_reentry(ResolveSessionReentry {
        explicit_session_id: None,
        repository: Some("owner/other".to_owned()),
        pull_request_number: None,
        source_surface: LaunchSurface::Cli,
        ui_target: Some("cli".to_owned()),
        instance_preference: Some("reuse_if_possible".to_owned()),
    })?;
    assert!(
        matches!(
            &missing,
            SessionReentryResolution::PickerRequired { reason, candidates }
                if reason.contains("no repo-local sessions") && candidates.is_empty()
        ),
        "expected picker-required missing-case, got {missing:?}"
    );

    Ok(())
}

#[test]
fn explicit_pr_no_match_returns_no_match_picker_without_cross_pr_candidates() -> Result<()> {
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path())?;

    let existing_target = target("owner/repo", 123);
    store.create_review_session(CreateReviewSession {
        id: "session-123",
        review_target: &existing_target,
        provider: "opencode",
        session_locator: None,
        resume_bundle_artifact_id: None,
        continuity_state: "awaiting_resume",
        attention_state: "awaiting_user_input",
        launch_profile_id: None,
    })?;

    let resolution = store.resolve_session_reentry(ResolveSessionReentry {
        explicit_session_id: None,
        repository: Some("owner/repo".to_owned()),
        pull_request_number: Some(2),
        source_surface: LaunchSurface::Cli,
        ui_target: Some("cli".to_owned()),
        instance_preference: Some("reuse_if_possible".to_owned()),
    })?;

    assert!(
        matches!(
            &resolution,
            SessionReentryResolution::PickerRequired { reason, candidates }
                if reason.contains("no matching repo-local session found for pull request 2")
                    && candidates.is_empty()
        ),
        "expected no-match picker response, got {resolution:?}"
    );

    Ok(())
}

#[test]
fn global_session_finder_can_filter_across_repos_and_attention_states() -> Result<()> {
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path())?;

    let rows = [
        ("session-1", "owner/repo-a", 1_u64, "awaiting_user_input"),
        ("session-2", "owner/repo-b", 2_u64, "review_launched"),
        ("session-3", "owner/repo-c", 3_u64, "awaiting_user_input"),
    ];
    for (session_id, repo, pr, attention_state) in rows {
        let review_target = target(repo, pr);
        store.create_review_session(CreateReviewSession {
            id: session_id,
            review_target: &review_target,
            provider: "opencode",
            session_locator: None,
            resume_bundle_artifact_id: None,
            continuity_state: "awaiting_resume",
            attention_state,
            launch_profile_id: None,
        })?;
    }

    let filtered = store.session_finder(SessionFinderQuery {
        repository: None,
        pull_request_number: None,
        attention_states: vec!["awaiting_user_input".to_owned()],
        limit: 10,
    })?;

    assert_eq!(filtered.len(), 2);
    assert!(
        filtered
            .iter()
            .all(|entry| entry.attention_state == "awaiting_user_input")
    );
    assert!(
        filtered
            .iter()
            .any(|entry| entry.repository == "owner/repo-a")
    );
    assert!(
        filtered
            .iter()
            .any(|entry| entry.repository == "owner/repo-c")
    );

    Ok(())
}

#[test]
fn same_pr_multi_instance_requires_picker_across_cli_and_extension_surfaces() -> Result<()> {
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path())?;

    let review_target = target("owner/repo", 42);
    for session_id in ["session-a", "session-b"] {
        store.create_review_session(CreateReviewSession {
            id: session_id,
            review_target: &review_target,
            provider: "opencode",
            session_locator: None,
            resume_bundle_artifact_id: None,
            continuity_state: "awaiting_resume",
            attention_state: "awaiting_user_input",
            launch_profile_id: None,
        })?;
        let binding_id = format!("binding-{session_id}");
        store.put_session_launch_binding(CreateSessionLaunchBinding {
            id: &binding_id,
            session_id,
            repo_locator: &review_target.repository,
            review_target: Some(&review_target),
            surface: LaunchSurface::Cli,
            launch_profile_id: None,
            ui_target: Some("cli"),
            instance_preference: Some("reuse_if_possible"),
            cwd: Some("/tmp/repo"),
            worktree_root: None,
        })?;
    }

    for (surface, ui_target) in [
        (LaunchSurface::Cli, "cli"),
        (LaunchSurface::Extension, "extension"),
        (LaunchSurface::Bridge, "tui"),
    ] {
        let resolution = store.resolve_session_reentry(ResolveSessionReentry {
            explicit_session_id: None,
            repository: Some(review_target.repository.clone()),
            pull_request_number: Some(review_target.pull_request_number),
            source_surface: surface,
            ui_target: Some(ui_target.to_owned()),
            instance_preference: Some("reuse_if_possible".to_owned()),
        })?;

        assert!(
            matches!(
                &resolution,
                SessionReentryResolution::PickerRequired { reason, candidates }
                    if reason.contains("ambiguous repo-local session match")
                        && candidates.len() == 2
                        && candidates.iter().all(
                            |entry| entry.repository == "owner/repo"
                                && entry.pull_request_number == 42
                        )
            ),
            "expected picker-required for {surface:?}, got {resolution:?}"
        );
    }

    Ok(())
}

#[test]
fn stale_target_binding_requires_picker_across_cli_extension_and_no_status_bridge() -> Result<()> {
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path())?;

    let review_target = target("owner/repo", 42);
    store.create_review_session(CreateReviewSession {
        id: "session-stale",
        review_target: &review_target,
        provider: "opencode",
        session_locator: None,
        resume_bundle_artifact_id: None,
        continuity_state: "awaiting_resume",
        attention_state: "awaiting_user_input",
        launch_profile_id: None,
    })?;

    let stale_bound_target = target("owner/repo", 99);
    for (binding_id, surface, ui_target) in [
        ("binding-cli", LaunchSurface::Cli, Some("cli")),
        (
            "binding-extension",
            LaunchSurface::Extension,
            Some("extension"),
        ),
        ("binding-bridge", LaunchSurface::Bridge, None),
    ] {
        store.put_session_launch_binding(CreateSessionLaunchBinding {
            id: binding_id,
            session_id: "session-stale",
            repo_locator: &review_target.repository,
            review_target: Some(&stale_bound_target),
            surface,
            launch_profile_id: None,
            ui_target,
            instance_preference: Some("reuse_if_possible"),
            cwd: Some("/tmp/repo"),
            worktree_root: None,
        })?;
    }

    for (surface, ui_target) in [
        (LaunchSurface::Cli, Some("cli")),
        (LaunchSurface::Extension, Some("extension")),
        (LaunchSurface::Bridge, None),
    ] {
        let resolution = store.resolve_session_reentry(ResolveSessionReentry {
            explicit_session_id: None,
            repository: Some(review_target.repository.clone()),
            pull_request_number: Some(review_target.pull_request_number),
            source_surface: surface,
            ui_target: ui_target.map(str::to_owned),
            instance_preference: Some("reuse_if_possible".to_owned()),
        })?;

        assert!(
            matches!(
                &resolution,
                SessionReentryResolution::PickerRequired { reason, candidates }
                    if reason.contains("launch binding is stale")
                        && candidates.len() == 1
                        && candidates[0].session_id == "session-stale"
                        && candidates[0].repository == "owner/repo"
                        && candidates[0].pull_request_number == 42
            ),
            "expected stale picker-required for {surface:?}, got {resolution:?}"
        );
    }

    Ok(())
}

#[test]
fn global_session_picker_reentry_preserves_target_identity() -> Result<()> {
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path())?;

    let rows = [
        ("session-a", "owner/repo-a", 1_u64, "awaiting_user_input"),
        ("session-b", "owner/repo-b", 2_u64, "review_launched"),
        ("session-c", "owner/repo-c", 3_u64, "awaiting_approval"),
    ];
    for (session_id, repo, pr, attention_state) in rows {
        let review_target = target(repo, pr);
        store.create_review_session(CreateReviewSession {
            id: session_id,
            review_target: &review_target,
            provider: "opencode",
            session_locator: None,
            resume_bundle_artifact_id: None,
            continuity_state: "awaiting_resume",
            attention_state,
            launch_profile_id: None,
        })?;
    }

    let picker = store.resolve_session_reentry(ResolveSessionReentry {
        explicit_session_id: None,
        repository: None,
        pull_request_number: None,
        source_surface: LaunchSurface::Extension,
        ui_target: Some("extension".to_owned()),
        instance_preference: Some("reuse_if_possible".to_owned()),
    })?;

    assert!(
        matches!(
            &picker,
            SessionReentryResolution::PickerRequired { reason, .. }
                if reason.contains("global session finder required")
        ),
        "expected global picker, got {picker:?}"
    );
    let selected = if let SessionReentryResolution::PickerRequired { candidates, .. } = picker {
        candidates
            .into_iter()
            .find(|entry| entry.repository == "owner/repo-b")
            .expect("repo-b candidate")
    } else {
        unreachable!("global picker assertion above should have held")
    };

    let resolved = store.resolve_session_reentry(ResolveSessionReentry {
        explicit_session_id: Some(selected.session_id.clone()),
        repository: None,
        pull_request_number: None,
        source_surface: LaunchSurface::Extension,
        ui_target: Some("extension".to_owned()),
        instance_preference: Some("reuse_if_possible".to_owned()),
    })?;

    assert!(
        matches!(
            &resolved,
            SessionReentryResolution::Resolved { session, .. }
                if session.id == selected.session_id
                    && session.review_target.repository == selected.repository
                    && session.review_target.pull_request_number == selected.pull_request_number
        ),
        "expected explicit-session resolution, got {resolved:?}"
    );

    Ok(())
}
