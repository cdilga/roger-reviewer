use tempfile::tempdir;

use roger_app_core::{
    ContinuityQuality, LaunchAction, LaunchIntent, ProviderContinuityCapability,
    ResumeAttemptOutcome, ResumeBundle, ResumeBundleProfile, ResumeSessionState, ResumeStrategy,
    ReviewTarget, SessionLocator, Surface, decide_resume_strategy,
};
use roger_storage::{CreateReviewSession, Result, RogerStore, StorageLayout};
use rusqlite::Connection;

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

fn sample_resume_bundle(review_target: ReviewTarget) -> ResumeBundle {
    ResumeBundle {
        schema_version: 1,
        profile: ResumeBundleProfile::ReseedResume,
        review_target,
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
        outbound_draft_ids: vec![],
        attention_summary: "awaiting_user_input".to_owned(),
        artifact_refs: vec![],
    }
}

#[test]
fn opencode_resume_smoke_prefers_reopen_when_locator_is_usable() {
    let decision = decide_resume_strategy(
        ProviderContinuityCapability::ReopenByLocator,
        &ResumeSessionState {
            locator_present: true,
            resume_bundle_present: true,
        },
        ResumeAttemptOutcome::ReopenedUsable,
    );

    assert_eq!(decision.strategy, ResumeStrategy::ReopenExisting);
    assert_eq!(decision.continuity_quality, ContinuityQuality::Usable);
}

#[test]
fn opencode_resume_smoke_reseeds_without_losing_review_target_identity() -> Result<()> {
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path())?;
    let target = sample_target();
    let bundle = sample_resume_bundle(target.clone());

    store.store_resume_bundle("resume-bundle-1", &bundle)?;
    let session = store.create_review_session(CreateReviewSession {
        id: "session-1",
        review_target: &target,
        provider: "opencode",
        session_locator: Some(&SessionLocator {
            provider: "opencode".to_owned(),
            session_id: "stale-session-id".to_owned(),
            invocation_context_json: "{\"cwd\":\"/tmp/repo\"}".to_owned(),
            captured_at: 111,
            last_tested_at: Some(112),
        }),
        resume_bundle_artifact_id: Some("resume-bundle-1"),
        continuity_state: "awaiting_resume",
        attention_state: "awaiting_user_input",
        launch_profile_id: None,
    })?;

    let decision = decide_resume_strategy(
        ProviderContinuityCapability::ReopenByLocator,
        &ResumeSessionState {
            locator_present: true,
            resume_bundle_present: true,
        },
        ResumeAttemptOutcome::ReopenUnavailable,
    );
    assert_eq!(decision.strategy, ResumeStrategy::ReseedFromBundle);
    assert_eq!(decision.continuity_quality, ContinuityQuality::Degraded);

    let loaded_bundle = store.load_resume_bundle("resume-bundle-1")?;
    assert_eq!(loaded_bundle.provider, "opencode");
    assert_eq!(session.provider, "opencode");
    assert_eq!(loaded_bundle.review_target, session.review_target);
    assert_eq!(loaded_bundle.review_target, target);

    Ok(())
}

#[test]
fn opencode_resume_smoke_recovers_resume_bundle_after_restart_and_compaction() -> Result<()> {
    let temp = tempdir()?;
    let target = sample_target();
    let bundle = sample_resume_bundle(target.clone());

    {
        let store = RogerStore::open(temp.path())?;
        store.store_resume_bundle("resume-bundle-1", &bundle)?;
        store.create_review_session(CreateReviewSession {
            id: "session-1",
            review_target: &target,
            provider: "opencode",
            session_locator: Some(&SessionLocator {
                provider: "opencode".to_owned(),
                session_id: "stale-after-restart".to_owned(),
                invocation_context_json: "{\"cwd\":\"/tmp/repo\"}".to_owned(),
                captured_at: 111,
                last_tested_at: Some(112),
            }),
            resume_bundle_artifact_id: Some("resume-bundle-1"),
            continuity_state: "awaiting_resume",
            attention_state: "awaiting_user_input",
            launch_profile_id: None,
        })?;
    }

    let layout = StorageLayout::under(temp.path());
    let conn = Connection::open(&layout.db_path)?;
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE); VACUUM;")?;

    let reopened = RogerStore::open(temp.path())?;
    let restored_bundle = reopened.load_resume_bundle("resume-bundle-1")?;
    let restored_session = reopened
        .review_session("session-1")?
        .expect("session exists");

    let decision = decide_resume_strategy(
        ProviderContinuityCapability::ReopenByLocator,
        &ResumeSessionState {
            locator_present: true,
            resume_bundle_present: true,
        },
        ResumeAttemptOutcome::ReopenUnavailable,
    );

    assert_eq!(
        decision.strategy,
        ResumeStrategy::ReseedFromBundle,
        "continuity layer regression: stale locator after restart+compaction must reseed"
    );
    assert_eq!(
        restored_bundle.review_target, target,
        "continuity layer regression: resume bundle lost review target identity after restart"
    );
    assert_eq!(
        restored_session.review_target, target,
        "continuity layer regression: session target drifted after restart+compaction"
    );

    Ok(())
}
