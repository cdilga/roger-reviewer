#![cfg(unix)]

use roger_app_core::{
    ContinuityQuality, FindingsBoundaryInput, FindingsBoundaryState, HarnessAdapter, LaunchAction,
    LaunchIntent, ResumeBundle, ResumeBundleProfile, ResumeDecisionReason, ReviewTarget, Surface,
    validate_structured_findings_boundary,
};
use roger_session_gemini::{GeminiAdapter, GeminiSessionPath};
use roger_storage::{ArtifactBudgetClass, CreateReviewRun, CreateReviewSession, RogerStore};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use tempfile::{TempDir, tempdir};

fn sample_target(pr_number: u64) -> ReviewTarget {
    ReviewTarget {
        repository: "owner/repo".to_owned(),
        pull_request_number: pr_number,
        base_ref: "main".to_owned(),
        head_ref: "feature".to_owned(),
        base_commit: "aaa".to_owned(),
        head_commit: "bbb".to_owned(),
    }
}

fn sample_intent(action: LaunchAction) -> LaunchIntent {
    LaunchIntent {
        action,
        source_surface: Surface::Cli,
        objective: Some("provider acceptance".to_owned()),
        launch_profile_id: Some("profile-gemini".to_owned()),
        cwd: Some("/tmp/repo".to_owned()),
        worktree_root: None,
    }
}

fn sample_bundle(target: ReviewTarget) -> ResumeBundle {
    ResumeBundle {
        schema_version: 1,
        profile: ResumeBundleProfile::ReseedResume,
        review_target: target,
        launch_intent: sample_intent(LaunchAction::ResumeReview),
        provider: "gemini".to_owned(),
        continuity_quality: ContinuityQuality::Degraded,
        stage_summary: "reseed required".to_owned(),
        unresolved_finding_ids: vec!["finding-1".to_owned()],
        outbound_draft_ids: vec![],
        attention_summary: "awaiting_resume".to_owned(),
        artifact_refs: vec!["artifact-raw".to_owned()],
    }
}

fn write_stub_binary() -> (TempDir, PathBuf) {
    let dir = tempdir().expect("tempdir");
    let script_path = dir.path().join("gemini-stub");
    let script = r#"#!/bin/sh
if [ "$1" = "export" ]; then
  echo "{\"provider\":\"gemini\",\"raw\":\"output\"}"
  exit 0
fi
exit 0
"#;

    fs::write(&script_path, script).expect("write stub");
    let mut permissions = fs::metadata(&script_path).expect("metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&script_path, permissions).expect("chmod");
    (dir, script_path)
}

#[test]
fn accept_gemini_reseeds_and_reports_bounded_tier_a_reasoning() {
    let (_stub_dir, binary_path) = write_stub_binary();
    let adapter = GeminiAdapter::with_binary(binary_path.to_string_lossy().to_string());
    let target = sample_target(42);
    let intent = sample_intent(LaunchAction::ResumeReview);
    let bundle = sample_bundle(target.clone());

    let stale_locator = adapter
        .start_session(&target, &intent)
        .expect("start locator");
    let linkage = adapter
        .link_session(&target, &intent, Some(&stale_locator), Some(&bundle))
        .expect("gemini should reseed from ResumeBundle");

    assert_eq!(linkage.path, GeminiSessionPath::ReseededFromBundle);
    assert_eq!(linkage.continuity_quality, ContinuityQuality::Degraded);
    assert_eq!(
        linkage.decision.expect("resume decision").reason_code,
        ResumeDecisionReason::ProviderLimitedNeedsReseed
    );
}

#[test]
fn accept_gemini_rejects_unsupported_deeper_capabilities() {
    let (_stub_dir, binary_path) = write_stub_binary();
    let adapter = GeminiAdapter::with_binary(binary_path.to_string_lossy().to_string());
    let target = sample_target(42);
    let locator = adapter
        .start_session(&target, &sample_intent(LaunchAction::StartReview))
        .expect("start locator");
    let bundle = sample_bundle(target);

    assert!(
        adapter
            .reopen_by_locator(&locator)
            .expect_err("reopen must be unsupported for tier-a")
            .to_string()
            .contains("tier-a")
    );
    assert!(
        adapter
            .open_in_bare_harness_mode(&locator, &bundle)
            .expect_err("dropout must be unsupported for tier-a")
            .to_string()
            .contains("tier-a")
    );
    assert!(
        adapter
            .return_to_roger_session(&locator)
            .expect_err("rr return must be unsupported for tier-a")
            .to_string()
            .contains("tier-a")
    );
}

#[test]
fn accept_gemini_captures_raw_and_structured_inputs_in_ledger()
-> Result<(), Box<dyn std::error::Error>> {
    let (_stub_dir, binary_path) = write_stub_binary();
    let adapter = GeminiAdapter::with_binary(binary_path.to_string_lossy().to_string());
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path())?;
    let target = sample_target(42);
    let locator = adapter.start_session(&target, &sample_intent(LaunchAction::StartReview))?;

    store.create_review_session(CreateReviewSession {
        id: "session-gemini-accept-1",
        review_target: &target,
        provider: "gemini",
        session_locator: Some(&locator),
        resume_bundle_artifact_id: Some("resume-bundle-gemini-accept-1"),
        continuity_state: "review_launched",
        attention_state: "awaiting_user_input",
        launch_profile_id: Some("profile-gemini"),
    })?;
    store.create_review_run(CreateReviewRun {
        id: "run-gemini-accept-1",
        session_id: "session-gemini-accept-1",
        run_kind: "explore",
        repo_snapshot: "git:feedface",
        continuity_quality: "degraded",
        session_locator_artifact_id: None,
    })?;

    let raw_output = adapter.capture_raw_output(&locator)?;
    let raw_bytes = raw_output.as_bytes();
    store.store_artifact(
        "artifact-raw-gemini-accept-1",
        ArtifactBudgetClass::ColdArtifact,
        "application/json",
        raw_bytes,
    )?;

    let bundle = sample_bundle(target.clone());
    store.store_resume_bundle("resume-bundle-gemini-accept-1", &bundle)?;

    let boundary = validate_structured_findings_boundary(FindingsBoundaryInput {
        raw_output_artifact_id: Some("artifact-raw-gemini-accept-1"),
        pack_json: Some(
            r#"{
                "schema_version": "structured_findings_pack/v1",
                "stage": "explore",
                "findings": [
                    {
                        "fingerprint": "fp-gemini-accept-1",
                        "title": "Potential null-check gap",
                        "normalized_summary": "Gemini observed a guard that may not hold.",
                        "severity": "medium",
                        "confidence": "medium",
                        "code_evidence": []
                    }
                ]
            }"#,
        ),
        repair_attempt: 0,
        retry_budget: 1,
    });
    assert_eq!(boundary.state, FindingsBoundaryState::Structured);
    assert_eq!(
        boundary
            .refresh_candidates()
            .expect("structured findings should produce refresh candidates")
            .len(),
        1
    );

    let reopened = RogerStore::open(temp.path())?;
    assert_eq!(
        reopened.artifact_bytes("artifact-raw-gemini-accept-1")?,
        raw_bytes
    );
    let loaded_bundle = reopened.load_resume_bundle("resume-bundle-gemini-accept-1")?;
    assert_eq!(loaded_bundle.provider, "gemini");
    assert_eq!(loaded_bundle.review_target, target);

    Ok(())
}
