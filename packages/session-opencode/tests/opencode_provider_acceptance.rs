#![cfg(unix)]

use roger_app_core::{
    ContinuityQuality, HarnessAdapter, LaunchAction, LaunchIntent, ResumeAttemptOutcome,
    ResumeBundle, ResumeBundleProfile, ResumeDecisionReason, ReviewTarget, Surface,
};
use roger_session_opencode::{
    OpenCodeAdapter, OpenCodeReturnPath, OpenCodeSessionPath, dropout_to_plain_opencode,
    rr_return_to_roger_session,
};
use roger_storage::{
    CreateReviewSession, CreateSessionLaunchBinding, LaunchSurface, ResolveSessionLaunchBinding,
    RogerStore,
};
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

fn dropout_bundle(target: ReviewTarget) -> ResumeBundle {
    let mut bundle = sample_bundle(target);
    bundle.profile = ResumeBundleProfile::DropoutControl;
    bundle
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
fn accept_opencode_locator_reopen_path() {
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
fn accept_opencode_stale_locator_reseeds_from_bundle() {
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
    assert_eq!(linkage.continuity_quality, ContinuityQuality::Degraded);
    assert_eq!(
        linkage.decision.expect("resume decision").reason_code,
        ResumeDecisionReason::ReopenUnavailableNeedsReseed
    );

    let reseeded_target = OpenCodeAdapter::review_target_from_locator(&linkage.locator)
        .expect("reseeded locator should keep review target identity");
    assert_eq!(reseeded_target, target);
}

#[test]
fn accept_opencode_fails_closed_without_bundle_when_reopen_fails() {
    let (_stub_dir, binary_path) = write_stub_binary(true);
    let adapter = OpenCodeAdapter::with_binary(binary_path.to_string_lossy().to_string());
    let target = sample_target(42);
    let intent = sample_intent(LaunchAction::ResumeReview);
    let stale_locator = adapter
        .start_session(&target, &intent)
        .expect("start locator");

    let err = adapter
        .link_session(&target, &intent, Some(&stale_locator), None)
        .expect_err("resume must fail closed without ResumeBundle");

    assert!(err.to_string().contains("no ResumeBundle is available"));
}

#[test]
fn accept_opencode_dropout_and_rr_return_rebinds_session() {
    let (_stub_dir, binary_path) = write_stub_binary(false);
    let adapter = OpenCodeAdapter::with_binary(binary_path.to_string_lossy().to_string());
    let target = sample_target(42);
    let locator = adapter
        .start_session(&target, &sample_intent(LaunchAction::StartReview))
        .expect("start locator");
    let bundle = dropout_bundle(target.clone());

    let transition =
        dropout_to_plain_opencode(&adapter, &locator, &bundle).expect("dropout should succeed");
    assert_eq!(
        transition.control_bundle.profile,
        ResumeBundleProfile::DropoutControl
    );

    let temp = tempdir().expect("tempdir");
    let root = temp.path().join("profile");
    {
        let store = RogerStore::open(&root).expect("open store");
        store
            .store_resume_bundle("bundle-1", &bundle)
            .expect("store bundle");
        store
            .create_review_session(CreateReviewSession {
                id: "session-1",
                review_target: &target,
                provider: "opencode",
                session_locator: Some(&locator),
                resume_bundle_artifact_id: Some("bundle-1"),
                continuity_state: "awaiting_return",
                attention_state: "awaiting_return",
                launch_profile_id: Some("profile-opencode"),
            })
            .expect("create session");
        store
            .put_session_launch_binding(CreateSessionLaunchBinding {
                id: "binding-1",
                session_id: "session-1",
                repo_locator: &target.repository,
                review_target: Some(&target),
                surface: LaunchSurface::Cli,
                launch_profile_id: None,
                ui_target: Some("cli"),
                instance_preference: Some("reuse_if_possible"),
                cwd: Some("/tmp/repo"),
                worktree_root: None,
            })
            .expect("put binding");
    }

    let reopened = RogerStore::open(&root).expect("reopen store");
    let outcome = rr_return_to_roger_session(
        &adapter,
        &reopened,
        ResolveSessionLaunchBinding {
            surface: LaunchSurface::Cli,
            repo_locator: &target.repository,
            review_target: Some(&target),
            ui_target: Some("cli"),
            instance_preference: Some("reuse_if_possible"),
        },
        ResumeAttemptOutcome::ReopenedUsable,
    )
    .expect("rr return should rebind existing session");

    assert_eq!(outcome.session_id, "session-1");
    assert_eq!(outcome.path, OpenCodeReturnPath::ReboundExistingSession);
    assert_eq!(
        outcome.decision.reason_code,
        ResumeDecisionReason::LocatorReopenedUsable
    );
}
