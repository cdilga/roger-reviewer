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

const SESSION_ID: &str = "session-dropout-1";
const BUNDLE_ID: &str = "resume-bundle-dropout-1";
const BINDING_ID: &str = "binding-return-cli-1";

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

fn dropout_control_bundle(target: ReviewTarget) -> ResumeBundle {
    ResumeBundle {
        schema_version: 1,
        profile: ResumeBundleProfile::DropoutControl,
        review_target: target,
        launch_intent: LaunchIntent {
            action: LaunchAction::ResumeReview,
            source_surface: Surface::Cli,
            objective: Some("continue in plain opencode and return safely".to_owned()),
            launch_profile_id: Some("profile-open-pr".to_owned()),
            cwd: Some("/tmp/repo".to_owned()),
            worktree_root: None,
        },
        provider: "opencode".to_owned(),
        continuity_quality: ContinuityQuality::Usable,
        stage_summary: "dropout to bare harness requested".to_owned(),
        unresolved_finding_ids: vec!["finding-1".to_owned()],
        outbound_draft_ids: vec![],
        attention_summary: "awaiting_rr_return".to_owned(),
        artifact_refs: vec!["artifact-dropout-control".to_owned()],
    }
}

fn seed_dropout_session(store: &RogerStore, target: &ReviewTarget) -> Result<()> {
    store.store_resume_bundle(BUNDLE_ID, &dropout_control_bundle(target.clone()))?;

    store.create_review_session(CreateReviewSession {
        id: SESSION_ID,
        review_target: target,
        provider: "opencode",
        session_locator: Some(&SessionLocator {
            provider: "opencode".to_owned(),
            session_id: "oc-bare-session-1".to_owned(),
            invocation_context_json: r#"{"mode":"bare_harness","entrypoint":"rr return"}"#
                .to_owned(),
            captured_at: 111,
            last_tested_at: Some(112),
        }),
        resume_bundle_artifact_id: Some(BUNDLE_ID),
        continuity_state: "dropout_control_written",
        attention_state: "awaiting_return",
        launch_profile_id: Some("profile-open-pr"),
    })?;

    store.put_session_launch_binding(CreateSessionLaunchBinding {
        id: BINDING_ID,
        session_id: SESSION_ID,
        repo_locator: &target.repository,
        review_target: Some(target),
        surface: LaunchSurface::Cli,
        launch_profile_id: None,
        ui_target: Some("cli"),
        instance_preference: Some("reuse_if_possible"),
        cwd: Some("/tmp/repo"),
        worktree_root: None,
    })?;

    Ok(())
}

#[test]
fn dropout_control_bundle_survives_restart_with_target_intact() -> Result<()> {
    let temp = tempdir()?;
    let root = temp.path().join("profile");
    let target = sample_target();

    {
        let store = RogerStore::open(&root)?;
        seed_dropout_session(&store, &target)?;
    }

    let reopened = RogerStore::open(&root)?;
    let session = reopened
        .review_session(SESSION_ID)?
        .expect("session must exist");
    let bundle = reopened.load_resume_bundle(BUNDLE_ID)?;

    assert_eq!(bundle.profile, ResumeBundleProfile::DropoutControl);
    assert_eq!(bundle.provider, "opencode");
    assert_eq!(bundle.review_target, target);
    assert_eq!(bundle.review_target, session.review_target);

    Ok(())
}

#[test]
fn rr_return_rebinds_to_the_correct_session_after_restart() -> Result<()> {
    let temp = tempdir()?;
    let root = temp.path().join("profile");
    let target = sample_target();

    {
        let store = RogerStore::open(&root)?;
        seed_dropout_session(&store, &target)?;
    }

    let reopened = RogerStore::open(&root)?;
    let resolution = reopened.resolve_resume_ledger(
        ResolveSessionLaunchBinding {
            surface: LaunchSurface::Cli,
            repo_locator: &target.repository,
            review_target: Some(&target),
            ui_target: Some("cli"),
            instance_preference: Some("reuse_if_possible"),
        },
        ProviderContinuityCapability::ReopenByLocator,
        ResumeAttemptOutcome::ReopenedUsable,
    )?;

    match resolution {
        ResumeLedgerResolution::Resolved(ledger) => {
            assert_eq!(ledger.binding.id, BINDING_ID);
            assert_eq!(ledger.binding.session_id, SESSION_ID);
            assert_eq!(ledger.session.id, SESSION_ID);
            assert_eq!(ledger.session.provider, "opencode");
            assert_eq!(ledger.decision.strategy, ResumeStrategy::ReopenExisting);
            assert_eq!(
                ledger.decision.reason_code,
                ResumeDecisionReason::LocatorReopenedUsable
            );
            assert_eq!(
                ledger
                    .resume_bundle
                    .as_ref()
                    .map(|bundle| bundle.profile.clone()),
                Some(ResumeBundleProfile::DropoutControl)
            );
        }
        other => panic!("expected resolved rr return rebind, got {other:?}"),
    }

    Ok(())
}

#[test]
fn rr_return_falls_back_to_reseed_when_locator_is_unavailable() -> Result<()> {
    let temp = tempdir()?;
    let root = temp.path().join("profile");
    let target = sample_target();

    {
        let store = RogerStore::open(&root)?;
        seed_dropout_session(&store, &target)?;
    }

    let reopened = RogerStore::open(&root)?;
    let resolution = reopened.resolve_resume_ledger(
        ResolveSessionLaunchBinding {
            surface: LaunchSurface::Cli,
            repo_locator: &target.repository,
            review_target: Some(&target),
            ui_target: Some("cli"),
            instance_preference: Some("reuse_if_possible"),
        },
        ProviderContinuityCapability::ReopenByLocator,
        ResumeAttemptOutcome::ReopenUnavailable,
    )?;

    match resolution {
        ResumeLedgerResolution::Resolved(ledger) => {
            assert_eq!(ledger.session.id, SESSION_ID);
            assert_eq!(ledger.decision.strategy, ResumeStrategy::ReseedFromBundle);
            assert_eq!(
                ledger.decision.reason_code,
                ResumeDecisionReason::ReopenUnavailableNeedsReseed
            );
            assert_eq!(
                ledger.decision.continuity_quality,
                ContinuityQuality::Degraded
            );
            let bundle = ledger
                .resume_bundle
                .expect("resume bundle must be available");
            assert_eq!(bundle.profile, ResumeBundleProfile::DropoutControl);
            assert_eq!(bundle.review_target, target);
        }
        other => panic!("expected reseed resolution for unavailable locator, got {other:?}"),
    }

    Ok(())
}
