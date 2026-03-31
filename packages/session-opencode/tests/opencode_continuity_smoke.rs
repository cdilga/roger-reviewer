#![cfg(unix)]

use roger_app_core::{
    ContinuityQuality, HarnessAdapter, LaunchAction, LaunchIntent, ResumeBundle,
    ResumeBundleProfile, ReviewTarget, Surface,
};
use roger_session_opencode::{OpenCodeAdapter, OpenCodeSessionPath};
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
        objective: Some("resume review".to_owned()),
        launch_profile_id: Some("profile-opencode".to_owned()),
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
        provider: "opencode".to_owned(),
        continuity_quality: ContinuityQuality::Degraded,
        stage_summary: "resume after compacted session".to_owned(),
        unresolved_finding_ids: vec!["finding-1".to_owned()],
        outbound_draft_ids: vec![],
        attention_summary: "awaiting_resume".to_owned(),
        artifact_refs: vec!["artifact-1".to_owned()],
    }
}

fn write_stub_binary(reopen_fails: bool) -> (TempDir, PathBuf) {
    let dir = tempdir().expect("tempdir");
    let script_path = dir.path().join("opencode-stub");
    let script = if reopen_fails {
        r#"#!/bin/sh
if [ "$1" = "--session" ]; then
  exit 1
fi
if [ "$1" = "export" ]; then
  echo "{}"
  exit 0
fi
exit 0
"#
    } else {
        r#"#!/bin/sh
if [ "$1" = "--session" ]; then
  exit 0
fi
if [ "$1" = "export" ]; then
  echo "{}"
  exit 0
fi
exit 0
"#
    };

    fs::write(&script_path, script).expect("write stub");
    let mut permissions = fs::metadata(&script_path).expect("metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&script_path, permissions).expect("chmod");

    (dir, script_path)
}

#[test]
fn link_session_reopens_existing_locator_when_direct_reopen_is_usable() {
    let (_stub_dir, binary_path) = write_stub_binary(false);
    let adapter = OpenCodeAdapter::with_binary(binary_path.to_string_lossy().to_string());
    let target = sample_target(42);
    let intent = sample_intent(LaunchAction::ResumeReview);

    let locator = adapter
        .start_session(&target, &intent)
        .expect("start locator");
    let linkage = adapter
        .link_session(
            &target,
            &intent,
            Some(&locator),
            Some(&sample_bundle(target.clone())),
        )
        .expect("link session");

    assert_eq!(linkage.path, OpenCodeSessionPath::ReopenedByLocator);
    assert_eq!(linkage.locator.session_id, locator.session_id);
    assert_eq!(linkage.continuity_quality, ContinuityQuality::Usable);
}

#[test]
fn link_session_reseeds_when_reopen_is_unavailable_and_preserves_review_target() {
    let (_stub_dir, binary_path) = write_stub_binary(true);
    let adapter = OpenCodeAdapter::with_binary(binary_path.to_string_lossy().to_string());
    let target = sample_target(42);
    let intent = sample_intent(LaunchAction::ResumeReview);

    let stale_locator = adapter
        .start_session(&target, &intent)
        .expect("start locator");
    let linkage = adapter
        .link_session(
            &target,
            &intent,
            Some(&stale_locator),
            Some(&sample_bundle(target.clone())),
        )
        .expect("reseed fallback");

    assert_eq!(linkage.path, OpenCodeSessionPath::ReseededFromBundle);
    assert!(linkage.locator.session_id.starts_with("oc-reseed-"));
    assert_eq!(linkage.continuity_quality, ContinuityQuality::Degraded);

    let reseeded_target = OpenCodeAdapter::review_target_from_locator(&linkage.locator)
        .expect("reseeded locator should contain a review target");
    assert_eq!(reseeded_target, target);
}
