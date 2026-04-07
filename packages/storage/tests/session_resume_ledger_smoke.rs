use tempfile::tempdir;

use roger_app_core::{
    ContinuityQuality, LaunchAction, LaunchIntent, ProviderContinuityCapability,
    ResumeAttemptOutcome, ResumeBundle, ResumeBundleProfile, ResumeDecisionReason, ResumeStrategy,
    ReviewTarget, SessionLocator, Surface,
};
use roger_storage::{
    CreateReviewSession, CreateSessionLaunchBinding, LaunchSurface, ResolveSessionLaunchBinding,
    Result, ResumeLedgerResolution, RogerStore,
};

fn sample_target() -> ReviewTarget {
    ReviewTarget {
        repository: "owner/repo".to_owned(),
        pull_request_number: 42,
        base_ref: "main".to_owned(),
        head_ref: "feature".to_owned(),
        base_commit: "deadbeef".to_owned(),
        head_commit: "feedface".to_owned(),
    }
}

fn sample_resume_bundle() -> ResumeBundle {
    ResumeBundle {
        schema_version: 1,
        profile: ResumeBundleProfile::ReseedResume,
        review_target: sample_target(),
        launch_intent: LaunchIntent {
            action: LaunchAction::ResumeReview,
            source_surface: Surface::Cli,
            objective: Some("resume review".to_owned()),
            launch_profile_id: Some("profile-open-pr".to_owned()),
            cwd: Some("/tmp/repo".to_owned()),
            worktree_root: None,
        },
        provider: "opencode".to_owned(),
        continuity_quality: ContinuityQuality::Degraded,
        stage_summary: "follow-up pending".to_owned(),
        unresolved_finding_ids: vec!["finding-1".to_owned()],
        outbound_draft_ids: vec!["draft-1".to_owned()],
        attention_summary: "awaiting_user_input".to_owned(),
        artifact_refs: vec!["artifact-inline".to_owned()],
    }
}

#[test]
fn resume_ledger_reopens_when_locator_is_usable_after_restart() -> Result<()> {
    let temp = tempdir()?;
    let root = temp.path().join("profile");

    {
        let store = RogerStore::open(&root)?;
        store.store_resume_bundle("resume-bundle-1", &sample_resume_bundle())?;
        store.create_review_session(CreateReviewSession {
            id: "session-1",
            review_target: &sample_target(),
            provider: "opencode",
            session_locator: Some(&SessionLocator {
                provider: "opencode".to_owned(),
                session_id: "oc-123".to_owned(),
                invocation_context_json: "{\"cwd\":\"/tmp/repo\"}".to_owned(),
                captured_at: 100,
                last_tested_at: Some(101),
            }),
            resume_bundle_artifact_id: Some("resume-bundle-1"),
            continuity_state: "awaiting_resume",
            attention_state: "awaiting_user_input",
            launch_profile_id: Some("profile-open-pr"),
        })?;
        store.put_session_launch_binding(CreateSessionLaunchBinding {
            id: "binding-cli",
            session_id: "session-1",
            repo_locator: "owner/repo",
            review_target: Some(&sample_target()),
            surface: LaunchSurface::Cli,
            launch_profile_id: Some("profile-open-pr"),
            ui_target: Some("cli"),
            instance_preference: Some("reuse_if_possible"),
            cwd: Some("/tmp/repo"),
            worktree_root: None,
        })?;
    }

    let reopened = RogerStore::open(&root)?;
    let resolution = reopened.resolve_resume_ledger(
        ResolveSessionLaunchBinding {
            explicit_session_id: None,
            surface: LaunchSurface::Cli,
            repo_locator: "owner/repo",
            review_target: Some(&sample_target()),
            ui_target: Some("cli"),
            instance_preference: Some("reuse_if_possible"),
        },
        ProviderContinuityCapability::ReopenByLocator,
        ResumeAttemptOutcome::ReopenedUsable,
    )?;

    assert!(
        matches!(
            &resolution,
            ResumeLedgerResolution::Resolved(record)
                if record.binding.session_id == "session-1"
                    && record.decision.strategy == ResumeStrategy::ReopenExisting
                    && record.decision.reason_code
                        == ResumeDecisionReason::LocatorReopenedUsable
                    && record
                        .resume_bundle
                        .as_ref()
                        .expect("resume bundle present")
                        .review_target
                        == sample_target()
        ),
        "expected resolved resume ledger, got {resolution:?}"
    );

    Ok(())
}

#[test]
fn stale_locator_reseeds_with_target_identity_preserved() -> Result<()> {
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path())?;

    store.store_resume_bundle("resume-bundle-1", &sample_resume_bundle())?;
    store.create_review_session(CreateReviewSession {
        id: "session-1",
        review_target: &sample_target(),
        provider: "opencode",
        session_locator: Some(&SessionLocator {
            provider: "opencode".to_owned(),
            session_id: "oc-stale".to_owned(),
            invocation_context_json: "{\"cwd\":\"/tmp/repo\"}".to_owned(),
            captured_at: 100,
            last_tested_at: Some(101),
        }),
        resume_bundle_artifact_id: Some("resume-bundle-1"),
        continuity_state: "awaiting_resume",
        attention_state: "awaiting_user_input",
        launch_profile_id: Some("profile-open-pr"),
    })?;
    store.put_session_launch_binding(CreateSessionLaunchBinding {
        id: "binding-tui",
        session_id: "session-1",
        repo_locator: "owner/repo",
        review_target: Some(&sample_target()),
        surface: LaunchSurface::Tui,
        launch_profile_id: Some("profile-open-pr"),
        ui_target: Some("tui"),
        instance_preference: Some("always_new"),
        cwd: Some("/tmp/repo"),
        worktree_root: None,
    })?;

    let resolution = store.resolve_resume_ledger(
        ResolveSessionLaunchBinding {
            explicit_session_id: None,
            surface: LaunchSurface::Tui,
            repo_locator: "owner/repo",
            review_target: Some(&sample_target()),
            ui_target: Some("tui"),
            instance_preference: Some("always_new"),
        },
        ProviderContinuityCapability::ReopenByLocator,
        ResumeAttemptOutcome::ReopenUnavailable,
    )?;

    assert!(
        matches!(
            &resolution,
            ResumeLedgerResolution::Resolved(record)
                if record.decision.strategy == ResumeStrategy::ReseedFromBundle
                    && record.decision.reason_code
                        == ResumeDecisionReason::ReopenUnavailableNeedsReseed
                    && record
                        .resume_bundle
                        .as_ref()
                        .expect("resume bundle present")
                        .review_target
                        == sample_target()
        ),
        "expected reseed decision, got {resolution:?}"
    );

    Ok(())
}

#[test]
fn missing_harness_state_fails_closed_without_resume_bundle() -> Result<()> {
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path())?;

    store.create_review_session(CreateReviewSession {
        id: "session-1",
        review_target: &sample_target(),
        provider: "opencode",
        session_locator: Some(&SessionLocator {
            provider: "opencode".to_owned(),
            session_id: "oc-missing-state".to_owned(),
            invocation_context_json: "{\"cwd\":\"/tmp/repo\"}".to_owned(),
            captured_at: 100,
            last_tested_at: Some(101),
        }),
        resume_bundle_artifact_id: None,
        continuity_state: "awaiting_resume",
        attention_state: "awaiting_user_input",
        launch_profile_id: Some("profile-open-pr"),
    })?;
    store.put_session_launch_binding(CreateSessionLaunchBinding {
        id: "binding-bridge",
        session_id: "session-1",
        repo_locator: "owner/repo",
        review_target: Some(&sample_target()),
        surface: LaunchSurface::Bridge,
        launch_profile_id: Some("profile-open-pr"),
        ui_target: Some("tui"),
        instance_preference: Some("reuse_if_possible"),
        cwd: Some("/tmp/repo"),
        worktree_root: Some("/tmp/repo/.worktrees/pr-42"),
    })?;

    let resolution = store.resolve_resume_ledger(
        ResolveSessionLaunchBinding {
            explicit_session_id: None,
            surface: LaunchSurface::Bridge,
            repo_locator: "owner/repo",
            review_target: Some(&sample_target()),
            ui_target: Some("tui"),
            instance_preference: Some("reuse_if_possible"),
        },
        ProviderContinuityCapability::ReopenByLocator,
        ResumeAttemptOutcome::MissingHarnessState,
    )?;

    assert!(
        matches!(
            &resolution,
            ResumeLedgerResolution::Resolved(record)
                if record.decision.strategy == ResumeStrategy::FailClosed
                    && record.decision.reason_code
                        == ResumeDecisionReason::MissingHarnessStateWithoutBundle
                    && record.resume_bundle.is_none()
                    && record.session.review_target == sample_target()
        ),
        "expected fail-closed decision, got {resolution:?}"
    );

    Ok(())
}

#[test]
fn cross_surface_bindings_resolve_to_same_durable_session_state() -> Result<()> {
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path())?;

    store.store_resume_bundle("resume-bundle-1", &sample_resume_bundle())?;
    store.create_review_session(CreateReviewSession {
        id: "session-1",
        review_target: &sample_target(),
        provider: "opencode",
        session_locator: Some(&SessionLocator {
            provider: "opencode".to_owned(),
            session_id: "oc-shared".to_owned(),
            invocation_context_json: "{\"cwd\":\"/tmp/repo\"}".to_owned(),
            captured_at: 100,
            last_tested_at: Some(101),
        }),
        resume_bundle_artifact_id: Some("resume-bundle-1"),
        continuity_state: "awaiting_resume",
        attention_state: "awaiting_user_input",
        launch_profile_id: Some("profile-open-pr"),
    })?;

    for (binding_id, surface, ui_target, instance) in [
        (
            "binding-cli",
            LaunchSurface::Cli,
            "cli",
            "reuse_if_possible",
        ),
        ("binding-tui", LaunchSurface::Tui, "tui", "always_new"),
        (
            "binding-bridge",
            LaunchSurface::Bridge,
            "tui",
            "reuse_if_possible",
        ),
    ] {
        store.put_session_launch_binding(CreateSessionLaunchBinding {
            id: binding_id,
            session_id: "session-1",
            repo_locator: "owner/repo",
            review_target: Some(&sample_target()),
            surface,
            launch_profile_id: Some("profile-open-pr"),
            ui_target: Some(ui_target),
            instance_preference: Some(instance),
            cwd: Some("/tmp/repo"),
            worktree_root: None,
        })?;
    }

    for (surface, ui_target, instance) in [
        (LaunchSurface::Cli, "cli", "reuse_if_possible"),
        (LaunchSurface::Tui, "tui", "always_new"),
        (LaunchSurface::Bridge, "tui", "reuse_if_possible"),
    ] {
        let resolution = store.resolve_resume_ledger(
            ResolveSessionLaunchBinding {
                explicit_session_id: None,
                surface,
                repo_locator: "owner/repo",
                review_target: Some(&sample_target()),
                ui_target: Some(ui_target),
                instance_preference: Some(instance),
            },
            ProviderContinuityCapability::ReopenByLocator,
            ResumeAttemptOutcome::ReopenedUsable,
        )?;

        assert!(
            matches!(
                &resolution,
                ResumeLedgerResolution::Resolved(record)
                    if record.session.id == "session-1"
                        && record.binding.session_id == "session-1"
                        && record.session.review_target == sample_target()
            ),
            "expected resolved binding for surface, got {resolution:?}"
        );
    }

    Ok(())
}
