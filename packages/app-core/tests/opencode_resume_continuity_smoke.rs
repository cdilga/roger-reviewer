use roger_app_core::{
    ContinuityQuality, LaunchAction, LaunchIntent, ProviderContinuityCapability,
    ResumeAttemptOutcome, ResumeBundle, ResumeBundleProfile, ResumeDecisionReason,
    ResumeSessionState, ResumeStrategy, ReviewTarget, Surface, decide_resume_strategy,
};

fn sample_target() -> ReviewTarget {
    ReviewTarget {
        repository: "owner/repo".to_owned(),
        pull_request_number: 42,
        base_ref: "main".to_owned(),
        head_ref: "feature".to_owned(),
        base_commit: "abc123".to_owned(),
        head_commit: "def456".to_owned(),
    }
}

fn sample_bundle() -> ResumeBundle {
    ResumeBundle {
        schema_version: 1,
        profile: ResumeBundleProfile::ReseedResume,
        review_target: sample_target(),
        launch_intent: LaunchIntent {
            action: LaunchAction::ResumeReview,
            source_surface: Surface::Cli,
            objective: Some("resume blocked review".to_owned()),
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
        artifact_refs: vec!["artifact-1".to_owned()],
    }
}

#[test]
fn locator_reopen_uses_existing_session_when_usable() {
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
    assert_eq!(
        decision.reason_code,
        ResumeDecisionReason::LocatorReopenedUsable
    );
}

#[test]
fn direct_reopen_unavailable_reseeds_when_bundle_exists() {
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
    assert_eq!(
        decision.reason_code,
        ResumeDecisionReason::ReopenUnavailableNeedsReseed
    );
}

#[test]
fn reseed_path_preserves_review_target_identity() {
    let bundle = sample_bundle();

    let decision = decide_resume_strategy(
        ProviderContinuityCapability::ReopenByLocator,
        &ResumeSessionState {
            locator_present: true,
            resume_bundle_present: true,
        },
        ResumeAttemptOutcome::ReopenUnavailable,
    );
    assert_eq!(decision.strategy, ResumeStrategy::ReseedFromBundle);

    let encoded = serde_json::to_string(&bundle).expect("serialize bundle");
    let decoded: ResumeBundle = serde_json::from_str(&encoded).expect("deserialize bundle");

    assert_eq!(decoded.review_target, bundle.review_target);
    assert_eq!(decoded.review_target.repository, "owner/repo");
    assert_eq!(decoded.review_target.pull_request_number, 42);
    assert_eq!(decoded.provider, "opencode");
}
