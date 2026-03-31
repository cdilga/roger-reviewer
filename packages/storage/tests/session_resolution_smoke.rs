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

    match resolution {
        SessionReentryResolution::Resolved { session, binding } => {
            assert_eq!(session.id, "session-1");
            assert_eq!(session.review_target, review_target);
            assert_eq!(binding.expect("binding").id, "binding-cli");
        }
        other => panic!("expected resolved re-entry, got {other:?}"),
    }

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
    match ambiguous {
        SessionReentryResolution::PickerRequired { reason, candidates } => {
            assert!(reason.contains("multiple repo-local sessions"));
            assert_eq!(candidates.len(), 2);
        }
        other => panic!("expected picker-required ambiguity, got {other:?}"),
    }

    let missing = store.resolve_session_reentry(ResolveSessionReentry {
        explicit_session_id: None,
        repository: Some("owner/other".to_owned()),
        pull_request_number: None,
        source_surface: LaunchSurface::Cli,
        ui_target: Some("cli".to_owned()),
        instance_preference: Some("reuse_if_possible".to_owned()),
    })?;
    match missing {
        SessionReentryResolution::PickerRequired { reason, candidates } => {
            assert!(reason.contains("no repo-local sessions"));
            assert!(candidates.is_empty());
        }
        other => panic!("expected picker-required missing-case, got {other:?}"),
    }

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
