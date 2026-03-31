#![cfg(unix)]

use roger_app_core::{
    ContinuityQuality, HarnessAdapter, LaunchAction, LaunchIntent, ResumeBundle,
    ResumeBundleProfile, ReviewTarget, RogerCommandId, Surface,
};
use roger_cli::{CliRuntime, HarnessCommandInvocation, run, run_harness_command};
use roger_session_opencode::OpenCodeAdapter;
use roger_storage::{CreateReviewSession, CreateSessionLaunchBinding, LaunchSurface, RogerStore};
use serde_json::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use tempfile::{TempDir, tempdir};

fn sample_target(pr_number: u64) -> ReviewTarget {
    ReviewTarget {
        repository: "owner/repo".to_owned(),
        pull_request_number: pr_number,
        base_ref: "main".to_owned(),
        head_ref: format!("feature-{pr_number}"),
        base_commit: "aaa".to_owned(),
        head_commit: "bbb".to_owned(),
    }
}

fn sample_launch_intent(action: LaunchAction) -> LaunchIntent {
    LaunchIntent {
        action,
        source_surface: Surface::Cli,
        objective: Some("cli smoke".to_owned()),
        launch_profile_id: Some("profile-open-pr".to_owned()),
        cwd: Some("/tmp/repo".to_owned()),
        worktree_root: None,
    }
}

fn dropout_bundle(target: ReviewTarget) -> ResumeBundle {
    ResumeBundle {
        schema_version: 1,
        profile: ResumeBundleProfile::DropoutControl,
        review_target: target,
        launch_intent: sample_launch_intent(LaunchAction::ResumeReview),
        provider: "opencode".to_owned(),
        continuity_quality: ContinuityQuality::Usable,
        stage_summary: "awaiting explicit return".to_owned(),
        unresolved_finding_ids: vec!["finding-1".to_owned()],
        outbound_draft_ids: vec![],
        attention_summary: "awaiting_return".to_owned(),
        artifact_refs: vec!["artifact-dropout".to_owned()],
    }
}

fn run_rr(args: &[&str], runtime: &CliRuntime) -> roger_cli::CliRunResult {
    let argv = args
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    run(&argv, runtime)
}

fn run_harness(
    command_id: RogerCommandId,
    provider: &str,
    runtime: &CliRuntime,
    pr: Option<u64>,
) -> roger_cli::CliRunResult {
    run_harness_command(
        &HarnessCommandInvocation {
            provider: provider.to_owned(),
            command_id,
            repo: None,
            pr,
            session_id: None,
            robot: true,
        },
        runtime,
    )
}

fn parse_robot_payload(stdout: &str) -> Value {
    serde_json::from_str(stdout).expect("robot payload json")
}

fn init_repo(temp: &TempDir) -> PathBuf {
    let repo = temp.path().join("repo");
    fs::create_dir_all(&repo).expect("create repo dir");

    let init = Command::new("git")
        .arg("-C")
        .arg(&repo)
        .arg("init")
        .output()
        .expect("git init");
    assert!(init.status.success(), "git init failed");

    let remote = Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args([
            "remote",
            "add",
            "origin",
            "https://github.com/owner/repo.git",
        ])
        .output()
        .expect("git remote add");
    assert!(remote.status.success(), "git remote add failed");

    repo
}

fn write_stub_binary(reopen_fails: bool) -> (TempDir, PathBuf) {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("opencode-stub");
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

    fs::write(&path, script).expect("write stub binary");
    let mut perms = fs::metadata(&path).expect("metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).expect("chmod stub binary");
    (dir, path)
}

fn seed_session_with_provider(
    runtime: &CliRuntime,
    provider: &str,
    pr_number: u64,
    session_id: &str,
) {
    let target = sample_target(pr_number);
    let store = RogerStore::open(&runtime.store_root).expect("open store");
    store
        .create_review_session(CreateReviewSession {
            id: session_id,
            review_target: &target,
            provider,
            session_locator: None,
            resume_bundle_artifact_id: None,
            continuity_state: "awaiting_user_input",
            attention_state: "awaiting_user_input",
            launch_profile_id: Some("profile-open-pr"),
        })
        .expect("create session");

    let binding_id = format!("binding-{session_id}");
    store
        .put_session_launch_binding(CreateSessionLaunchBinding {
            id: &binding_id,
            session_id,
            repo_locator: &target.repository,
            review_target: Some(&target),
            surface: LaunchSurface::Cli,
            launch_profile_id: Some("profile-open-pr"),
            ui_target: Some("cli"),
            instance_preference: Some("reuse_if_possible"),
            cwd: Some("/tmp/repo"),
            worktree_root: None,
        })
        .expect("create binding");
}

#[test]
fn shell_commands_work_without_extension_on_blessed_opencode_path() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let review = run_rr(&["review", "--pr", "42", "--robot"], &runtime);
    assert_eq!(review.exit_code, 0, "{}", review.stderr);

    let status = run_rr(&["status", "--robot"], &runtime);
    assert_eq!(status.exit_code, 0, "{}", status.stderr);
    let status_payload = parse_robot_payload(&status.stdout);
    assert_eq!(status_payload["outcome"], "complete");
    assert_eq!(status_payload["data"]["target"]["pull_request"], 42);

    let findings = run_rr(&["findings", "--robot"], &runtime);
    assert_eq!(findings.exit_code, 0, "{}", findings.stderr);
    let findings_payload = parse_robot_payload(&findings.stdout);
    assert!(findings_payload["outcome"] == "empty" || findings_payload["outcome"] == "complete");

    let refresh = run_rr(&["refresh", "--pr", "42", "--robot"], &runtime);
    assert_eq!(refresh.exit_code, 0, "{}", refresh.stderr);

    let resume = run_rr(&["resume", "--pr", "42", "--robot"], &runtime);
    assert_eq!(resume.exit_code, 0, "{}", resume.stderr);

    let ret = run_rr(&["return", "--pr", "42", "--robot"], &runtime);
    assert_eq!(ret.exit_code, 0, "{}", ret.stderr);
}

#[test]
fn resume_blocks_with_picker_when_repo_match_is_ambiguous() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    assert_eq!(
        run_rr(&["review", "--pr", "42", "--robot"], &runtime).exit_code,
        0
    );
    assert_eq!(
        run_rr(&["review", "--pr", "43", "--robot"], &runtime).exit_code,
        0
    );

    let resume = run_rr(&["resume", "--robot"], &runtime);
    assert_eq!(resume.exit_code, 3, "{}", resume.stderr);
    let payload = parse_robot_payload(&resume.stdout);
    assert_eq!(payload["outcome"], "blocked");
    assert!(
        payload["data"]["reason"]
            .as_str()
            .expect("reason")
            .contains("multiple repo-local sessions")
    );
    assert_eq!(
        payload["data"]["candidates"]
            .as_array()
            .expect("candidate list")
            .len(),
        2
    );
}

#[test]
fn resume_stale_locator_falls_back_to_resumebundle_reseed() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_ok_stub_dir, ok_bin) = write_stub_binary(false);
    let (_fail_stub_dir, fail_bin) = write_stub_binary(true);

    let stable_runtime = CliRuntime {
        cwd: repo.clone(),
        store_root: temp.path().join("roger-store"),
        opencode_bin: ok_bin.to_string_lossy().to_string(),
    };

    let review = run_rr(&["review", "--pr", "42", "--robot"], &stable_runtime);
    assert_eq!(review.exit_code, 0, "{}", review.stderr);

    let stale_runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: fail_bin.to_string_lossy().to_string(),
    };

    let resume = run_rr(&["resume", "--pr", "42", "--robot"], &stale_runtime);
    assert_eq!(resume.exit_code, 5, "{}", resume.stderr);

    let payload = parse_robot_payload(&resume.stdout);
    assert_eq!(payload["outcome"], "degraded");
    assert_eq!(payload["data"]["resume_path"], "reseeded_from_bundle");
}

#[test]
fn review_blocks_truthfully_for_non_blessed_provider() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let review = run_rr(
        &["review", "--pr", "42", "--provider", "gemini", "--robot"],
        &runtime,
    );
    assert_eq!(review.exit_code, 3, "{}", review.stderr);

    let payload = parse_robot_payload(&review.stdout);
    assert_eq!(payload["outcome"], "blocked");
    assert_eq!(payload["data"]["provider"], "gemini");
    assert_eq!(payload["data"]["supported_provider"], "opencode");
    assert!(
        payload["repair_actions"]
            .as_array()
            .expect("repair actions")
            .iter()
            .any(|action| action
                .as_str()
                .expect("repair action string")
                .contains("--provider opencode"))
    );
}

#[test]
fn bounded_provider_outputs_are_truthful_for_status_resume_refresh_and_return() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };
    seed_session_with_provider(&runtime, "gemini", 42, "session-gemini-1");

    let status = run_rr(&["status", "--pr", "42", "--robot"], &runtime);
    assert_eq!(status.exit_code, 0, "{}", status.stderr);
    let status_payload = parse_robot_payload(&status.stdout);
    assert_eq!(status_payload["outcome"], "complete");
    assert_eq!(status_payload["data"]["session"]["provider"], "gemini");
    assert_eq!(
        status_payload["data"]["session"]["resume_mode"],
        "bounded_provider"
    );
    assert_eq!(status_payload["data"]["continuity"]["tier"], "tier_a");
    assert!(
        status_payload["warnings"]
            .as_array()
            .expect("status warnings")
            .iter()
            .any(|warning| warning
                .as_str()
                .expect("warning string")
                .contains("bounded support"))
    );

    let resume = run_rr(&["resume", "--pr", "42", "--robot"], &runtime);
    assert_eq!(resume.exit_code, 3, "{}", resume.stderr);
    let resume_payload = parse_robot_payload(&resume.stdout);
    assert_eq!(resume_payload["outcome"], "blocked");
    assert_eq!(resume_payload["data"]["provider"], "gemini");

    let refresh = run_rr(&["refresh", "--pr", "42", "--robot"], &runtime);
    assert_eq!(refresh.exit_code, 3, "{}", refresh.stderr);
    let refresh_payload = parse_robot_payload(&refresh.stdout);
    assert_eq!(refresh_payload["outcome"], "blocked");
    assert_eq!(refresh_payload["data"]["provider"], "gemini");

    let ret = run_rr(&["return", "--pr", "42", "--robot"], &runtime);
    assert_eq!(ret.exit_code, 3, "{}", ret.stderr);
    let return_payload = parse_robot_payload(&ret.stdout);
    assert_eq!(return_payload["outcome"], "blocked");
    assert_eq!(return_payload["data"]["provider"], "gemini");
}

#[test]
fn return_reports_truthful_rebind_path_after_dropout_style_state() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let target = sample_target(42);
    let adapter = OpenCodeAdapter::with_binary(runtime.opencode_bin.clone());
    let locator = adapter
        .start_session(&target, &sample_launch_intent(LaunchAction::StartReview))
        .expect("start locator");

    let store = RogerStore::open(&runtime.store_root).expect("open store");
    store
        .store_resume_bundle("bundle-dropout-1", &dropout_bundle(target.clone()))
        .expect("store bundle");
    store
        .create_review_session(CreateReviewSession {
            id: "session-dropout-1",
            review_target: &target,
            provider: "opencode",
            session_locator: Some(&locator),
            resume_bundle_artifact_id: Some("bundle-dropout-1"),
            continuity_state: "awaiting_return",
            attention_state: "awaiting_return",
            launch_profile_id: Some("profile-open-pr"),
        })
        .expect("create session");
    store
        .put_session_launch_binding(CreateSessionLaunchBinding {
            id: "binding-dropout-1",
            session_id: "session-dropout-1",
            repo_locator: &target.repository,
            review_target: Some(&target),
            surface: LaunchSurface::Cli,
            launch_profile_id: Some("profile-open-pr"),
            ui_target: Some("cli"),
            instance_preference: Some("reuse_if_possible"),
            cwd: Some("/tmp/repo"),
            worktree_root: None,
        })
        .expect("create binding");

    let ret = run_rr(&["return", "--pr", "42", "--robot"], &runtime);
    assert_eq!(ret.exit_code, 0, "{}", ret.stderr);

    let payload = parse_robot_payload(&ret.stdout);
    assert_eq!(payload["outcome"], "complete");
    assert_eq!(payload["data"]["return_path"], "rebound_existing_session");
}

#[test]
fn harness_status_routes_to_same_core_cli_operation() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let review = run_rr(&["review", "--pr", "42", "--robot"], &runtime);
    assert_eq!(review.exit_code, 0, "{}", review.stderr);

    let harness_status = run_harness(RogerCommandId::RogerStatus, "opencode", &runtime, Some(42));
    assert_eq!(harness_status.exit_code, 0, "{}", harness_status.stderr);

    let payload = parse_robot_payload(&harness_status.stdout);
    assert_eq!(payload["schema_id"], "rr.robot.status.v1");
    assert_eq!(payload["outcome"], "complete");
    assert_eq!(payload["data"]["target"]["pull_request"], 42);
}

#[test]
fn harness_command_falls_back_truthfully_when_provider_binding_is_absent() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let result = run_harness(RogerCommandId::RogerReturn, "gemini", &runtime, Some(42));
    assert_eq!(result.exit_code, 3, "{}", result.stderr);

    let payload = parse_robot_payload(&result.stdout);
    assert_eq!(payload["schema_id"], "rr.robot.harness_command.v1");
    assert_eq!(payload["outcome"], "blocked");
    assert_eq!(payload["data"]["command_id"], "roger-return");
    assert_eq!(payload["data"]["fallback_cli_command"], "rr return");
    assert!(
        payload["repair_actions"]
            .as_array()
            .expect("repair actions")
            .iter()
            .any(|action| action
                .as_str()
                .expect("repair action")
                .contains("rr return"))
    );
}
