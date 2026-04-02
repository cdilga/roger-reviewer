#![cfg(unix)]

use roger_app_core::{
    ContinuityQuality, HarnessAdapter, HarnessCommandBinding, LaunchAction, LaunchIntent,
    ResumeBundle, ResumeBundleProfile, ReviewTarget, RogerCommand, RogerCommandId,
    RogerCommandInvocationSurface, RogerCommandRouteStatus, Surface, route_harness_command,
};
use roger_cli::{CliRuntime, HarnessCommandInvocation, run, run_harness_command};
use roger_session_opencode::OpenCodeAdapter;
use roger_storage::{CreateReviewSession, CreateSessionLaunchBinding, LaunchSurface, RogerStore};
use serde_json::Value;
use std::collections::HashMap;
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

fn parse_toon_payload(stdout: &str) -> Value {
    toon_format::decode_default(stdout).expect("robot payload toon")
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("packages parent")
        .parent()
        .expect("workspace root")
        .to_path_buf()
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

fn write_probe_binary() -> (TempDir, PathBuf, PathBuf) {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("opencode-probe");
    let marker = dir.path().join("invoked.log");
    let script = format!(
        r#"#!/bin/sh
echo "$@" >> "{marker}"
if [ "$1" = "export" ]; then
  echo "{{}}"
  exit 0
fi
exit 0
"#,
        marker = marker.to_string_lossy()
    );

    fs::write(&path, script).expect("write probe binary");
    let mut perms = fs::metadata(&path).expect("metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).expect("chmod probe binary");
    (dir, path, marker)
}

#[test]
fn help_forms_exit_cleanly_for_quickstart_probe() {
    let temp = tempdir().expect("tempdir");
    let runtime = CliRuntime {
        cwd: temp.path().to_path_buf(),
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };

    for args in [&["help"][..], &["--help"][..], &["-h"][..]] {
        let result = run_rr(args, &runtime);
        assert_eq!(result.exit_code, 0, "args={args:?} stderr={}", result.stderr);
        assert!(
            result.stdout.contains("Usage:"),
            "args={args:?} stdout={}",
            result.stdout
        );
        assert!(
            result.stderr.trim().is_empty(),
            "args={args:?} stderr={}",
            result.stderr
        );
    }
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

fn seed_session_for_finder(
    runtime: &CliRuntime,
    session_id: &str,
    repository: &str,
    pr_number: u64,
    attention_state: &str,
) {
    let target = ReviewTarget {
        repository: repository.to_owned(),
        pull_request_number: pr_number,
        base_ref: "main".to_owned(),
        head_ref: format!("feature-{pr_number}"),
        base_commit: "aaa".to_owned(),
        head_commit: "bbb".to_owned(),
    };
    let store = RogerStore::open(&runtime.store_root).expect("open store");
    store
        .create_review_session(CreateReviewSession {
            id: session_id,
            review_target: &target,
            provider: "opencode",
            session_locator: None,
            resume_bundle_artifact_id: None,
            continuity_state: "awaiting_resume",
            attention_state,
            launch_profile_id: Some("profile-open-pr"),
        })
        .expect("create session");
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
fn repeated_review_reuses_resume_bundle_artifact_for_duplicate_digest() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let first = run_rr(&["review", "--pr", "42", "--robot"], &runtime);
    assert_eq!(first.exit_code, 0, "{}", first.stderr);
    let first_payload = parse_robot_payload(&first.stdout);
    let first_bundle_id = first_payload["data"]["resume_bundle_artifact_id"]
        .as_str()
        .expect("first bundle id");

    let second = run_rr(&["review", "--pr", "42", "--robot"], &runtime);
    assert_eq!(second.exit_code, 0, "{}", second.stderr);
    let second_payload = parse_robot_payload(&second.stdout);
    let second_bundle_id = second_payload["data"]["resume_bundle_artifact_id"]
        .as_str()
        .expect("second bundle id");

    assert_eq!(
        first_bundle_id, second_bundle_id,
        "duplicate resume-bundle payload digest should reuse the existing artifact id"
    );
    assert!(
        !second.stderr.contains("UNIQUE constraint failed: artifacts.digest"),
        "review path should avoid duplicate artifact digest failures: {}",
        second.stderr
    );
}

#[test]
fn status_repo_pr_resolution_matches_live_session_picker_and_explicit_session() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let review = run_rr(
        &["review", "--pr", "11", "--repo", "owner/repo", "--robot"],
        &runtime,
    );
    assert_eq!(review.exit_code, 0, "{}", review.stderr);
    let review_payload = parse_robot_payload(&review.stdout);
    let session_id = review_payload["data"]["session_id"]
        .as_str()
        .expect("review session id")
        .to_owned();

    let sessions = run_rr(
        &["sessions", "--repo", "owner/repo", "--pr", "11", "--robot"],
        &runtime,
    );
    assert_eq!(sessions.exit_code, 0, "{}", sessions.stderr);
    let sessions_payload = parse_robot_payload(&sessions.stdout);
    let listed_session = sessions_payload["data"]["items"]
        .as_array()
        .expect("sessions items")
        .first()
        .expect("single session entry");
    assert_eq!(
        listed_session["session_id"]
            .as_str()
            .expect("listed session id"),
        session_id
    );

    let resume = run_rr(
        &[
            "resume",
            "--repo",
            "owner/repo",
            "--pr",
            "11",
            "--dry-run",
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(resume.exit_code, 0, "{}", resume.stderr);
    let resume_payload = parse_robot_payload(&resume.stdout);
    assert_eq!(
        resume_payload["data"]["session_id"]
            .as_str()
            .expect("resume session id"),
        session_id
    );

    let status = run_rr(
        &["status", "--repo", "owner/repo", "--pr", "11", "--robot"],
        &runtime,
    );
    assert_eq!(status.exit_code, 0, "{}", status.stderr);
    let status_payload = parse_robot_payload(&status.stdout);
    assert_eq!(status_payload["outcome"], "complete");
    assert_eq!(
        status_payload["data"]["session"]["id"]
            .as_str()
            .expect("status session id"),
        session_id
    );

    let status_by_session = run(
        &[
            "status".to_owned(),
            "--session".to_owned(),
            session_id.clone(),
            "--robot".to_owned(),
        ],
        &runtime,
    );
    assert_eq!(
        status_by_session.exit_code, 0,
        "{}",
        status_by_session.stderr
    );
    let status_by_session_payload = parse_robot_payload(&status_by_session.stdout);
    assert_eq!(
        status_by_session_payload["data"]["session"]["id"]
            .as_str()
            .expect("explicit status session id"),
        session_id
    );
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
fn resume_robot_mode_suppresses_stale_locator_reopen_attempts() {
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
    assert_eq!(resume.exit_code, 0, "{}", resume.stderr);

    let payload = parse_robot_payload(&resume.stdout);
    assert_eq!(payload["outcome"], "complete");
    assert_eq!(payload["data"]["mode"], "robot_non_interactive");
    assert_eq!(payload["data"]["launch_suppressed"], true);
    assert_eq!(
        payload["data"]["reason_code"],
        "interactive_launch_suppressed_for_robot_mode"
    );
    assert_eq!(payload["data"]["command"], "resume");
}

#[test]
fn robot_resume_and_refresh_do_not_launch_interactive_provider_paths() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_probe_dir, probe_bin, marker_path) = write_probe_binary();

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: probe_bin.to_string_lossy().to_string(),
    };
    seed_session_with_provider(&runtime, "opencode", 42, "session-opencode-robot-1");

    let resume = run_rr(
        &["resume", "--session", "session-opencode-robot-1", "--robot"],
        &runtime,
    );
    assert_eq!(resume.exit_code, 0, "{}", resume.stderr);
    let resume_payload = parse_robot_payload(&resume.stdout);
    assert_eq!(resume_payload["outcome"], "complete");
    assert_eq!(resume_payload["data"]["mode"], "robot_non_interactive");
    assert_eq!(resume_payload["data"]["launch_suppressed"], true);
    assert_eq!(
        resume_payload["data"]["reason_code"],
        "interactive_launch_suppressed_for_robot_mode"
    );
    assert_eq!(resume_payload["data"]["command"], "resume");

    let refresh = run_rr(
        &[
            "refresh",
            "--session",
            "session-opencode-robot-1",
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(refresh.exit_code, 0, "{}", refresh.stderr);
    let refresh_payload = parse_robot_payload(&refresh.stdout);
    assert_eq!(refresh_payload["outcome"], "complete");
    assert_eq!(refresh_payload["data"]["mode"], "robot_non_interactive");
    assert_eq!(refresh_payload["data"]["launch_suppressed"], true);
    assert_eq!(
        refresh_payload["data"]["reason_code"],
        "interactive_launch_suppressed_for_robot_mode"
    );
    assert_eq!(refresh_payload["data"]["command"], "refresh");

    assert!(
        !marker_path.exists(),
        "provider binary should not be invoked for robot resume/refresh"
    );
}

#[test]
fn review_blocks_truthfully_for_unsupported_provider() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let review = run_rr(
        &["review", "--pr", "42", "--provider", "claude", "--robot"],
        &runtime,
    );
    assert_eq!(review.exit_code, 3, "{}", review.stderr);

    let payload = parse_robot_payload(&review.stdout);
    assert_eq!(payload["outcome"], "blocked");
    assert_eq!(payload["data"]["provider"], "claude");
    assert_eq!(
        payload["data"]["supported_providers"],
        Value::Array(vec![
            Value::String("opencode".to_owned()),
            Value::String("codex".to_owned())
        ])
    );
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
fn review_blocks_truthfully_for_gemini_until_provider_launch_support_is_exposed() {
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
    assert_eq!(
        payload["data"]["supported_providers"],
        Value::Array(vec![
            Value::String("opencode".to_owned()),
            Value::String("codex".to_owned())
        ])
    );
    assert!(
        payload["repair_actions"]
            .as_array()
            .expect("repair actions")
            .iter()
            .any(|action| action
                .as_str()
                .expect("repair action string")
                .contains("--provider codex"))
    );
}

#[test]
fn codex_review_and_resume_are_truthful_tier_a_degraded_paths() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let review = run_rr(
        &["review", "--pr", "42", "--provider", "codex", "--robot"],
        &runtime,
    );
    assert_eq!(review.exit_code, 5, "{}", review.stderr);

    let review_payload = parse_robot_payload(&review.stdout);
    assert_eq!(review_payload["outcome"], "degraded");
    assert_eq!(review_payload["data"]["provider"], "codex");
    assert_eq!(review_payload["data"]["session_path"], "started_fresh");
    assert_eq!(review_payload["data"]["continuity_quality"], "degraded");
    assert!(
        review_payload["warnings"]
            .as_array()
            .expect("warnings")
            .iter()
            .any(|warning| warning.as_str().expect("warning text").contains("tier-a"))
    );

    let resume = run_rr(&["resume", "--pr", "42", "--robot"], &runtime);
    assert_eq!(resume.exit_code, 5, "{}", resume.stderr);

    let resume_payload = parse_robot_payload(&resume.stdout);
    assert_eq!(resume_payload["outcome"], "degraded");
    assert_eq!(resume_payload["data"]["provider"], "codex");
    assert_eq!(
        resume_payload["data"]["resume_path"],
        "reseeded_from_bundle"
    );
    assert_eq!(resume_payload["data"]["continuity_quality"], "degraded");
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

#[test]
fn harness_return_stale_locator_matches_cli_degraded_semantics() {
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

    let harness_return = run_harness(
        RogerCommandId::RogerReturn,
        "opencode",
        &stale_runtime,
        Some(42),
    );
    assert_eq!(harness_return.exit_code, 5, "{}", harness_return.stderr);

    let payload = parse_robot_payload(&harness_return.stdout);
    assert_eq!(payload["schema_id"], "rr.robot.return.v1");
    assert_eq!(payload["outcome"], "degraded");
    assert_eq!(payload["data"]["return_path"], "reseeded_session");
}

#[test]
fn sessions_lists_filters_and_compacts_with_explicit_follow_on_hints() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    seed_session_for_finder(
        &runtime,
        "session-a",
        "owner/repo-a",
        101,
        "awaiting_user_input",
    );
    seed_session_for_finder(
        &runtime,
        "session-b",
        "owner/repo-b",
        202,
        "review_launched",
    );
    seed_session_for_finder(
        &runtime,
        "session-c",
        "owner/repo-c",
        303,
        "awaiting_user_input",
    );

    let sessions = run_rr(&["sessions", "--robot"], &runtime);
    assert_eq!(sessions.exit_code, 0, "{}", sessions.stderr);
    let payload = parse_robot_payload(&sessions.stdout);
    assert_eq!(payload["schema_id"], "rr.robot.sessions.v1");
    assert_eq!(payload["outcome"], "complete");
    assert_eq!(payload["data"]["count"], 3);
    assert_eq!(payload["data"]["truncated"], false);
    let items = payload["data"]["items"].as_array().expect("session items");
    assert_eq!(items.len(), 3);
    for item in items {
        assert!(item.get("session_id").is_some());
        assert!(item.get("repo").is_some());
        assert!(item["target"].get("repository").is_some());
        assert!(item["target"].get("pull_request").is_some());
        assert!(item.get("attention_state").is_some());
        assert!(item.get("updated_at").is_some());
        assert_eq!(
            item["follow_on"]["requires_explicit_session"].as_bool(),
            Some(true)
        );
        assert!(
            item["follow_on"]["resume_command"]
                .as_str()
                .expect("resume command")
                .contains("--session ")
        );
    }

    let compact_filtered = run_rr(
        &[
            "sessions",
            "--attention",
            "awaiting_user_input",
            "--limit",
            "1",
            "--robot",
            "--robot-format",
            "compact",
        ],
        &runtime,
    );
    assert_eq!(compact_filtered.exit_code, 0, "{}", compact_filtered.stderr);
    let compact_payload = parse_robot_payload(&compact_filtered.stdout);
    assert_eq!(compact_payload["schema_id"], "rr.robot.sessions.v1");
    assert_eq!(compact_payload["robot_format"], "compact");
    assert_eq!(compact_payload["data"]["count"], 1);
    assert_eq!(compact_payload["data"]["truncated"], true);
    assert_eq!(
        compact_payload["data"]["items"]
            .as_array()
            .expect("compact items")
            .len(),
        1
    );
    assert!(
        compact_payload["data"]["items"][0]
            .get("session_id")
            .is_some()
    );
    assert!(compact_payload["data"]["items"][0].get("repo").is_some());
    assert!(
        compact_payload["data"]["items"][0]
            .get("pull_request")
            .is_some()
    );
    assert_eq!(
        compact_payload["data"]["items"][0]["attention_state"],
        "awaiting_user_input"
    );
}

#[test]
fn search_reports_truthful_degraded_mode_and_stable_robot_fields() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let search = run_rr(&["search", "--query", "stale draft", "--robot"], &runtime);
    assert_eq!(search.exit_code, 5, "{}", search.stderr);
    let payload = parse_robot_payload(&search.stdout);
    assert_eq!(payload["schema_id"], "rr.robot.search.v1");
    assert_eq!(payload["outcome"], "degraded");
    assert_eq!(payload["data"]["query"], "stale draft");
    assert_eq!(payload["data"]["mode"], "lexical_only");
    assert!(payload["data"]["items"].is_array());
    assert!(payload["data"]["count"].is_number());
    assert!(payload["data"]["truncated"].is_boolean());
    assert!(
        payload["data"]["degraded_reasons"]
            .as_array()
            .expect("degraded reasons")
            .iter()
            .any(|reason| reason
                .as_str()
                .expect("degraded reason")
                .contains("lexical"))
    );

    let compact = run_rr(
        &[
            "search",
            "--query",
            "stale draft",
            "--robot",
            "--robot-format",
            "compact",
        ],
        &runtime,
    );
    assert_eq!(compact.exit_code, 5, "{}", compact.stderr);
    let compact_payload = parse_robot_payload(&compact.stdout);
    assert_eq!(compact_payload["schema_id"], "rr.robot.search.v1");
    assert_eq!(compact_payload["robot_format"], "compact");
    assert!(compact_payload["data"]["items"].is_array());
}

#[test]
fn robot_docs_surfaces_schema_inventory_and_blocks_unknown_topics() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let schemas = run_rr(&["robot-docs", "schemas", "--robot"], &runtime);
    assert_eq!(schemas.exit_code, 0, "{}", schemas.stderr);
    let payload = parse_robot_payload(&schemas.stdout);
    assert_eq!(payload["schema_id"], "rr.robot.robot_docs.v1");
    assert_eq!(payload["outcome"], "complete");
    assert_eq!(payload["data"]["topic"], "schemas");
    assert_eq!(payload["data"]["version"], "0.1.0");
    let items = payload["data"]["items"].as_array().expect("schema items");
    assert!(
        items
            .iter()
            .any(|item| item["schema_id"] == "rr.robot.sessions.v1")
    );
    assert!(
        items
            .iter()
            .any(|item| item["schema_id"] == "rr.robot.search.v1")
    );

    let compact = run_rr(
        &[
            "robot-docs",
            "commands",
            "--robot",
            "--robot-format",
            "compact",
        ],
        &runtime,
    );
    assert_eq!(compact.exit_code, 0, "{}", compact.stderr);
    let compact_payload = parse_robot_payload(&compact.stdout);
    assert_eq!(compact_payload["schema_id"], "rr.robot.robot_docs.v1");
    assert_eq!(compact_payload["robot_format"], "compact");
    assert_eq!(compact_payload["data"]["topic"], "commands");

    let blocked = run_rr(&["robot-docs", "unknown-topic", "--robot"], &runtime);
    assert_eq!(blocked.exit_code, 3, "{}", blocked.stderr);
    let blocked_payload = parse_robot_payload(&blocked.stdout);
    assert_eq!(blocked_payload["schema_id"], "rr.robot.robot_docs.v1");
    assert_eq!(blocked_payload["outcome"], "blocked");
    assert_eq!(
        blocked_payload["data"]["reason_code"],
        "unknown_robot_docs_topic"
    );
}

#[test]
fn bridge_pack_extension_emits_checksum_artifacts_in_smoke() {
    let temp = tempdir().expect("tempdir");
    let runtime = CliRuntime {
        cwd: workspace_root(),
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };
    let output_dir = temp.path().join("pack-output");
    let result = run(
        &[
            "bridge".to_owned(),
            "pack-extension".to_owned(),
            "--output-dir".to_owned(),
            output_dir.to_string_lossy().to_string(),
            "--robot".to_owned(),
        ],
        &runtime,
    );
    assert_eq!(result.exit_code, 0, "{}", result.stderr);
    let payload = parse_robot_payload(&result.stdout);
    assert_eq!(payload["outcome"], "complete");
    assert_eq!(payload["data"]["subcommand"], "pack-extension");
    assert_eq!(payload["data"]["installs_browser_extension"], false);
    let package_dir = PathBuf::from(
        payload["data"]["package_dir"]
            .as_str()
            .expect("package_dir should be present"),
    );
    assert!(package_dir.join("SHA256SUMS").exists());
    assert!(package_dir.join("asset-manifest.json").exists());
}

#[test]
fn bridge_install_uninstall_is_failure_closed_and_reports_asset_checksums_in_smoke() {
    let temp = tempdir().expect("tempdir");
    let bridge_binary = temp.path().join("rr-bridge");
    fs::write(&bridge_binary, b"#!/bin/sh\nexit 0\n").expect("write mock bridge binary");

    let runtime = CliRuntime {
        cwd: workspace_root(),
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };
    let install_root = temp.path().join("install-root");

    let blocked = run(
        &[
            "bridge".to_owned(),
            "install".to_owned(),
            "--bridge-binary".to_owned(),
            bridge_binary.to_string_lossy().to_string(),
            "--install-root".to_owned(),
            install_root.to_string_lossy().to_string(),
            "--robot".to_owned(),
        ],
        &runtime,
    );
    assert_eq!(blocked.exit_code, 3, "{}", blocked.stderr);
    let blocked_payload = parse_robot_payload(&blocked.stdout);
    assert_eq!(blocked_payload["outcome"], "blocked");
    assert_eq!(
        blocked_payload["data"]["reason_code"],
        "extension_id_required"
    );

    let install = run(
        &[
            "bridge".to_owned(),
            "install".to_owned(),
            "--extension-id".to_owned(),
            "abcdefghijklmnopabcdefghijklmnop".to_owned(),
            "--bridge-binary".to_owned(),
            bridge_binary.to_string_lossy().to_string(),
            "--install-root".to_owned(),
            install_root.to_string_lossy().to_string(),
            "--robot".to_owned(),
        ],
        &runtime,
    );
    assert_eq!(install.exit_code, 0, "{}", install.stderr);
    let install_payload = parse_robot_payload(&install.stdout);
    assert_eq!(install_payload["outcome"], "complete");
    let assets = install_payload["data"]["assets"]
        .as_array()
        .expect("assets array");
    assert!(!assets.is_empty());
    assert!(assets.iter().all(|asset| {
        asset["sha256"]
            .as_str()
            .is_some_and(|checksum| checksum.len() == 64)
    }));

    let uninstall = run(
        &[
            "bridge".to_owned(),
            "uninstall".to_owned(),
            "--install-root".to_owned(),
            install_root.to_string_lossy().to_string(),
            "--robot".to_owned(),
        ],
        &runtime,
    );
    assert_eq!(uninstall.exit_code, 0, "{}", uninstall.stderr);
    let uninstall_payload = parse_robot_payload(&uninstall.stdout);
    assert_eq!(uninstall_payload["outcome"], "complete");
    assert!(
        uninstall_payload["data"]["removed"]
            .as_array()
            .expect("removed list")
            .len()
            >= 1
    );
}

#[test]
fn partial_harness_binding_fails_closed_with_rr_fallback() {
    let bindings = vec![HarnessCommandBinding {
        provider: "opencode".to_owned(),
        command_id: RogerCommandId::RogerStatus,
        provider_command_syntax: "/roger-status".to_owned(),
        capability_requirements: vec!["supports_roger_commands".to_owned()],
    }];

    let command = RogerCommand {
        command_id: RogerCommandId::RogerReturn,
        review_session_id: Some("session-1".to_owned()),
        review_run_id: None,
        args: HashMap::new(),
        invocation_surface: RogerCommandInvocationSurface::HarnessCommand,
        provider: "opencode".to_owned(),
    };

    let routed = route_harness_command(&command, &bindings);
    assert_eq!(routed.status, RogerCommandRouteStatus::FallbackRequired);
    assert_eq!(routed.next_action.fallback_cli_command, "rr return");
    assert!(
        routed
            .next_action
            .session_finder_hint
            .expect("session finder hint")
            .contains("--session <id>")
    );
}

#[test]
fn status_and_findings_support_toon_robot_format() {
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

    let status = run_rr(
        &["status", "--pr", "42", "--robot", "--robot-format", "toon"],
        &runtime,
    );
    assert_eq!(status.exit_code, 0, "{}", status.stderr);
    let status_payload = parse_toon_payload(&status.stdout);
    assert_eq!(status_payload["schema_id"], "rr.robot.status.v1");
    assert_eq!(status_payload["robot_format"], "toon");
    assert_eq!(status_payload["outcome"], "complete");

    let findings = run_rr(
        &[
            "findings",
            "--pr",
            "42",
            "--robot",
            "--robot-format",
            "toon",
        ],
        &runtime,
    );
    assert_eq!(findings.exit_code, 0, "{}", findings.stderr);
    let findings_payload = parse_toon_payload(&findings.stdout);
    assert_eq!(findings_payload["schema_id"], "rr.robot.findings.v1");
    assert_eq!(findings_payload["robot_format"], "toon");
}

#[test]
fn toon_is_rejected_outside_status_and_findings_in_this_slice() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let review = run_rr(
        &["review", "--pr", "42", "--robot", "--robot-format", "toon"],
        &runtime,
    );
    assert_eq!(review.exit_code, 2);
    assert!(
        review
            .stderr
            .contains("toon format is only supported for status/findings in this slice"),
        "{}",
        review.stderr
    );
}
