use tempfile::tempdir;

use roger_app_core::{
    ContinuityQuality, LaunchAction, LaunchIntent, ProviderContinuityCapability,
    ResumeAttemptOutcome, ResumeBundle, ResumeBundleProfile, ResumeDecisionReason,
    ResumeSessionState, ResumeStrategy, ReviewTarget, SessionLocator, Surface,
    decide_resume_strategy,
};
use roger_storage::{CreateReviewSession, Result, RogerStore};

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

fn sample_bundle(target: ReviewTarget) -> ResumeBundle {
    ResumeBundle {
        schema_version: 1,
        profile: ResumeBundleProfile::ReseedResume,
        review_target: target,
        launch_intent: LaunchIntent {
            action: LaunchAction::ResumeReview,
            source_surface: Surface::Cli,
            objective: Some("resume opencode session".to_owned()),
            launch_profile_id: Some("profile-open-pr".to_owned()),
            cwd: Some("/tmp/repo".to_owned()),
            worktree_root: None,
        },
        provider: "opencode".to_owned(),
        continuity_quality: ContinuityQuality::Degraded,
        stage_summary: "stale locator detected".to_owned(),
        unresolved_finding_ids: vec!["finding-1".to_owned()],
        outbound_draft_ids: vec!["draft-1".to_owned()],
        attention_summary: "needs_resume".to_owned(),
        artifact_refs: vec!["artifact-1".to_owned()],
    }
}

#[test]
fn opencode_stale_locator_reseeds_without_losing_review_target_identity() -> Result<()> {
    let temp = tempdir()?;
    let root = temp.path().join("profile");
    let target = sample_target();

    {
        let store = RogerStore::open(&root)?;
        let bundle = sample_bundle(target.clone());
        store.store_resume_bundle("resume-bundle-1", &bundle)?;

        store.create_review_session(CreateReviewSession {
            id: "session-1",
            review_target: &target,
            provider: "opencode",
            session_locator: Some(&SessionLocator {
                provider: "opencode".to_owned(),
                session_id: "stale-session".to_owned(),
                invocation_context_json: "{\"cwd\":\"/tmp/repo\"}".to_owned(),
                captured_at: 111,
                last_tested_at: Some(112),
            }),
            resume_bundle_artifact_id: Some("resume-bundle-1"),
            continuity_state: "awaiting_resume",
            attention_state: "awaiting_user_input",
            launch_profile_id: Some("profile-open-pr"),
        })?;
    }

    let reopened = RogerStore::open(&root)?;
    let session = reopened
        .review_session("session-1")?
        .expect("session must exist");
    let state = ResumeSessionState {
        locator_present: session.session_locator.is_some(),
        resume_bundle_present: session.resume_bundle_artifact_id.is_some(),
    };

    let decision = decide_resume_strategy(
        ProviderContinuityCapability::ReopenByLocator,
        &state,
        ResumeAttemptOutcome::ReopenUnavailable,
    );
    assert_eq!(decision.strategy, ResumeStrategy::ReseedFromBundle);
    assert_eq!(
        decision.reason_code,
        ResumeDecisionReason::ReopenUnavailableNeedsReseed
    );

    let bundle_id = session
        .resume_bundle_artifact_id
        .as_deref()
        .expect("resume bundle artifact id must be present");
    let loaded_bundle = reopened.load_resume_bundle(bundle_id)?;
    assert_eq!(loaded_bundle.provider, "opencode");
    assert_eq!(loaded_bundle.review_target, target);
    assert_eq!(loaded_bundle.review_target, session.review_target);

    Ok(())
}

#[test]
fn opencode_missing_harness_state_without_resume_bundle_fails_closed() {
    let decision = decide_resume_strategy(
        ProviderContinuityCapability::ReopenByLocator,
        &ResumeSessionState {
            locator_present: false,
            resume_bundle_present: false,
        },
        ResumeAttemptOutcome::MissingHarnessState,
    );

    assert_eq!(decision.strategy, ResumeStrategy::FailClosed);
    assert_eq!(decision.continuity_quality, ContinuityQuality::Unusable);
    assert_eq!(
        decision.reason_code,
        ResumeDecisionReason::MissingHarnessStateWithoutBundle
    );
}
