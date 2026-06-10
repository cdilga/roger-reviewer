use std::fs;
use std::path::PathBuf;

use tempfile::tempdir;

use roger_app_core::{
    ContinuityQuality, LaunchAction, LaunchIntent, ProviderContinuityCapability,
    ResumeAttemptOutcome, ResumeBundle, ResumeBundleProfile, ResumeDecisionReason,
    ResumeSessionState, ResumeStrategy, ReviewTarget, SessionLocator, Surface,
    decide_resume_strategy,
};
use roger_session_copilot::{parse_transcript_capture_fixture, resolve_transcript_capture};
use roger_storage::{CreateReviewSession, Result, RogerStore};

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/fixture_copilot_transcript_capture")
}

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

fn sample_bundle(target: ReviewTarget, artifact_refs: Vec<String>) -> ResumeBundle {
    ResumeBundle {
        schema_version: 1,
        profile: ResumeBundleProfile::ReseedResume,
        review_target: target,
        launch_intent: LaunchIntent {
            action: LaunchAction::ResumeReview,
            source_surface: Surface::Cli,
            objective: Some("resume copilot review".to_owned()),
            launch_profile_id: Some("profile-open-pr".to_owned()),
            cwd: Some("/tmp/repo".to_owned()),
            worktree_root: None,
        },
        provider: "copilot".to_owned(),
        continuity_quality: ContinuityQuality::Degraded,
        stage_summary: "stale locator detected; reseed required".to_owned(),
        unresolved_finding_ids: vec!["finding-1".to_owned()],
        outbound_draft_ids: vec!["draft-1".to_owned()],
        attention_summary: "needs_resume".to_owned(),
        artifact_refs,
    }
}

#[test]
fn copilot_resume_bundle_persists_transcript_refs_and_degrades_to_reseed_only() -> Result<()> {
    let fixture_raw = fs::read_to_string(fixture_root().join("copilot_transcript_refs.json"))?;
    let fixture = parse_transcript_capture_fixture(&fixture_raw).expect("parse transcript fixture");

    for record in fixture.records {
        let temp = tempdir()?;
        let root = temp.path().join(&record.id);
        let target = sample_target();
        let bundle = sample_bundle(target.clone(), record.artifact_refs.clone());

        {
            let store = RogerStore::open(&root)?;
            store.store_resume_bundle("resume-bundle-1", &bundle)?;
            store.create_review_session(CreateReviewSession {
                id: "session-1",
                review_target: &target,
                provider: "copilot",
                session_locator: Some(&SessionLocator {
                    provider: "copilot".to_owned(),
                    session_id: record.session_id.clone(),
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
        let bundle_id = session
            .resume_bundle_artifact_id
            .as_deref()
            .expect("resume bundle id");
        let loaded_bundle = reopened.load_resume_bundle(bundle_id)?;

        assert_eq!(
            loaded_bundle.provider, "copilot",
            "provider drift for {}",
            record.id
        );
        assert_eq!(
            loaded_bundle.review_target, target,
            "target drift for {}",
            record.id
        );
        assert_eq!(
            loaded_bundle.artifact_refs, record.artifact_refs,
            "artifact refs drift for {}",
            record.id
        );

        let resolution = resolve_transcript_capture(&fixture_root(), &loaded_bundle.artifact_refs)
            .expect("resolve transcript capture");
        assert_eq!(
            resolution.raw_capture_present, record.expected.raw_capture_present,
            "raw capture mismatch for {}",
            record.id
        );
        assert_eq!(
            resolution.transcript_lookup_state, record.expected.transcript_lookup_state,
            "transcript state mismatch for {}",
            record.id
        );
        assert_eq!(
            resolution.reason_code, record.expected.reason_code,
            "reason code mismatch for {}",
            record.id
        );

        let decision = decide_resume_strategy(
            ProviderContinuityCapability::ReseedOnly,
            &ResumeSessionState {
                locator_present: session.session_locator.is_some(),
                resume_bundle_present: true,
            },
            ResumeAttemptOutcome::ReopenUnavailable,
        );
        assert_eq!(decision.strategy, ResumeStrategy::ReseedFromBundle);
        assert_eq!(decision.continuity_quality, ContinuityQuality::Degraded);
        assert_eq!(
            decision.reason_code,
            ResumeDecisionReason::ProviderLimitedNeedsReseed
        );
    }

    Ok(())
}
