#![cfg(unix)]

use roger_app_core::{
    ContinuityQuality, HarnessAdapter, HarnessCommandBinding, LaunchAction, LaunchIntent,
    ResumeBundle, ResumeBundleProfile, ReviewTarget, RogerCommand, RogerCommandId,
    RogerCommandInvocationSurface, RogerCommandRouteStatus, Surface, route_harness_command,
};
use roger_bridge::{BridgeLaunchIntent, BridgePreflight, BridgeResponse, handle_bridge_intent};
use roger_cli::{CliRuntime, HarnessCommandInvocation, run, run_harness_command};
use roger_session_opencode::OpenCodeAdapter;
use roger_storage::{CreateReviewSession, CreateSessionLaunchBinding, LaunchSurface, RogerStore};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;
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

fn run_rr_process(args: &[&str], runtime: &CliRuntime) -> std::process::Output {
    if let Ok(rr_bin) = std::env::var("CARGO_BIN_EXE_rr") {
        return Command::new(rr_bin)
            .args(args)
            .current_dir(&runtime.cwd)
            .env("RR_STORE_ROOT", &runtime.store_root)
            .env("RR_OPENCODE_BIN", &runtime.opencode_bin)
            .output()
            .expect("run rr process via CARGO_BIN_EXE_rr");
    }

    let workspace = workspace_root();
    let local_rr = workspace.join("target/debug/rr");
    if local_rr.exists() {
        return Command::new(local_rr)
            .args(args)
            .current_dir(&runtime.cwd)
            .env("RR_STORE_ROOT", &runtime.store_root)
            .env("RR_OPENCODE_BIN", &runtime.opencode_bin)
            .output()
            .expect("run rr process via target/debug/rr");
    }

    let mut cmd = Command::new("cargo");
    cmd.arg("run")
        .arg("-q")
        .arg("-p")
        .arg("roger-cli")
        .arg("--bin")
        .arg("rr")
        .arg("--");
    cmd.args(args)
        .current_dir(workspace)
        .env("RR_STORE_ROOT", &runtime.store_root)
        .env("RR_OPENCODE_BIN", &runtime.opencode_bin);
    cmd.output().expect("run rr process via cargo run fallback")
}

fn run_rr_process_with_stdin(
    args: &[&str],
    runtime: &CliRuntime,
    stdin_bytes: &[u8],
) -> std::process::Output {
    let mut cmd = if let Ok(rr_bin) = std::env::var("CARGO_BIN_EXE_rr") {
        let mut cmd = Command::new(rr_bin);
        cmd.current_dir(&runtime.cwd)
            .env("RR_STORE_ROOT", &runtime.store_root)
            .env("RR_OPENCODE_BIN", &runtime.opencode_bin);
        cmd
    } else {
        let workspace = workspace_root();
        let local_rr = workspace.join("target/debug/rr");
        if local_rr.exists() {
            let mut cmd = Command::new(local_rr);
            cmd.current_dir(&runtime.cwd)
                .env("RR_STORE_ROOT", &runtime.store_root)
                .env("RR_OPENCODE_BIN", &runtime.opencode_bin);
            cmd
        } else {
            let mut cmd = Command::new("cargo");
            cmd.arg("run")
                .arg("-q")
                .arg("-p")
                .arg("roger-cli")
                .arg("--bin")
                .arg("rr")
                .arg("--")
                .current_dir(workspace)
                .env("RR_STORE_ROOT", &runtime.store_root)
                .env("RR_OPENCODE_BIN", &runtime.opencode_bin);
            cmd
        }
    };

    cmd.args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().expect("spawn rr process");
    {
        let stdin = child.stdin.as_mut().expect("child stdin");
        stdin
            .write_all(stdin_bytes)
            .expect("write native messaging request");
    }
    child
        .wait_with_output()
        .expect("wait for rr process output")
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

fn encode_native_intent(intent: &BridgeLaunchIntent) -> Vec<u8> {
    let json = serde_json::to_vec(intent).expect("serialize native intent");
    let len = json.len() as u32;
    let mut wire = Vec::with_capacity(4 + json.len());
    wire.extend_from_slice(&len.to_le_bytes());
    wire.extend_from_slice(&json);
    wire
}

fn decode_native_response(stdout: &[u8]) -> BridgeResponse {
    assert!(
        stdout.len() >= 4,
        "native host output missing length prefix: {} bytes",
        stdout.len()
    );
    let len = u32::from_le_bytes([stdout[0], stdout[1], stdout[2], stdout[3]]) as usize;
    assert_eq!(
        stdout.len(),
        4 + len,
        "native host output length prefix mismatch"
    );
    serde_json::from_slice(&stdout[4..]).expect("decode native host response payload")
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("packages parent")
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn extension_pack_test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn write_guided_profile_discovery_state(runtime: &CliRuntime, browser: &str, extension_id: &str) {
    let package_dir = workspace_root().join("target/bridge/extension/roger-extension-unpacked");
    let preferences_path = runtime
        .store_root
        .join("bridge/browser-profiles")
        .join(browser)
        .join("Default/Secure Preferences");
    fs::create_dir_all(
        preferences_path
            .parent()
            .expect("profile preferences parent directory"),
    )
    .expect("create profile preferences parent");
    let preferences = serde_json::json!({
        "extensions": {
            "settings": {
                extension_id: {
                    "path": package_dir.to_string_lossy().to_string()
                }
            }
        }
    });
    fs::write(
        &preferences_path,
        serde_json::to_vec_pretty(&preferences).expect("serialize preferences"),
    )
    .expect("write secure preferences");
}

fn register_extension_identity_via_bridge(runtime: &CliRuntime, browser: &str, extension_id: &str) {
    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _env_guard = ENV_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("env lock");

    let previous_store_root = std::env::var_os("RR_STORE_ROOT");
    // SAFETY: tests serialize RR_STORE_ROOT mutation via ENV_LOCK and restore it before return.
    unsafe {
        std::env::set_var("RR_STORE_ROOT", &runtime.store_root);
    }

    let response = handle_bridge_intent(
        &BridgeLaunchIntent {
            action: "register_extension_identity".to_owned(),
            owner: "roger".to_owned(),
            repo: "roger-reviewer".to_owned(),
            pr_number: 0,
            head_ref: None,
            instance: None,
            extension_id: Some(extension_id.to_owned()),
            browser: Some(browser.to_owned()),
        },
        &BridgePreflight {
            roger_binary_found: false,
            roger_data_dir_exists: false,
            gh_available: false,
        },
        Path::new("rr"),
    );

    match previous_store_root {
        Some(value) => {
            // SAFETY: tests serialize RR_STORE_ROOT mutation via ENV_LOCK and restore it before return.
            unsafe {
                std::env::set_var("RR_STORE_ROOT", value);
            }
        }
        None => {
            // SAFETY: tests serialize RR_STORE_ROOT mutation via ENV_LOCK and restore it before return.
            unsafe {
                std::env::remove_var("RR_STORE_ROOT");
            }
        }
    }

    assert!(
        response.ok,
        "bridge registration intent failed: {} / {:?}",
        response.message, response.guidance
    );
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
        assert_eq!(
            result.exit_code, 0,
            "args={args:?} stderr={}",
            result.stderr
        );
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

#[test]
fn rr_binary_accepts_native_host_registration_intents_via_stdio_envelope() {
    let temp = tempdir().expect("tempdir");
    let runtime = CliRuntime {
        cwd: temp.path().to_path_buf(),
        store_root: temp.path().join(".roger"),
        opencode_bin: "opencode".to_owned(),
    };

    let intent = BridgeLaunchIntent {
        action: "register_extension_identity".to_owned(),
        owner: "roger".to_owned(),
        repo: "roger-reviewer".to_owned(),
        pr_number: 0,
        head_ref: None,
        instance: None,
        extension_id: Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned()),
        browser: Some("chrome".to_owned()),
    };
    let output = run_rr_process_with_stdin(&[], &runtime, &encode_native_intent(&intent));

    assert!(
        output.status.success(),
        "native host registration should exit cleanly: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response = decode_native_response(&output.stdout);
    assert!(response.ok, "response should be ok: {:?}", response);
    assert_eq!(response.action, "register_extension_identity");
}

#[test]
fn rr_binary_native_host_path_returns_bridge_response_for_launch_intents() {
    let temp = tempdir().expect("tempdir");
    let runtime = CliRuntime {
        cwd: temp.path().to_path_buf(),
        store_root: temp.path().join(".roger-missing"),
        opencode_bin: "opencode".to_owned(),
    };

    let intent = BridgeLaunchIntent {
        action: "start_review".to_owned(),
        owner: "owner".to_owned(),
        repo: "repo".to_owned(),
        pr_number: 42,
        head_ref: None,
        instance: None,
        extension_id: None,
        browser: None,
    };
    let output = run_rr_process_with_stdin(&[], &runtime, &encode_native_intent(&intent));

    assert!(
        !output.status.success(),
        "preflight failure should fail closed with non-zero exit"
    );
    let response = decode_native_response(&output.stdout);
    assert!(!response.ok);
    assert_eq!(response.action, "start_review");
    assert!(
        response.message.contains("Roger is not ready"),
        "unexpected response message: {}",
        response.message
    );
}

#[test]
fn rr_binary_native_host_path_handles_all_primary_launch_actions_without_hanging() {
    let temp = tempdir().expect("tempdir");
    let runtime = CliRuntime {
        cwd: temp.path().to_path_buf(),
        store_root: temp.path().join(".roger-missing"),
        opencode_bin: "opencode".to_owned(),
    };

    for action in [
        "start_review",
        "resume_review",
        "show_findings",
        "refresh_review",
    ] {
        let intent = BridgeLaunchIntent {
            action: action.to_owned(),
            owner: "owner".to_owned(),
            repo: "repo".to_owned(),
            pr_number: 42,
            head_ref: None,
            instance: None,
            extension_id: None,
            browser: None,
        };
        let output = run_rr_process_with_stdin(&[], &runtime, &encode_native_intent(&intent));
        assert!(
            !output.stdout.is_empty(),
            "expected Native Messaging envelope output for action={action}"
        );
        let response = decode_native_response(&output.stdout);

        assert!(
            !output.status.success(),
            "preflight should fail closed for action={action}"
        );
        assert!(
            !response.ok,
            "response should fail closed for action={action}"
        );
        assert_eq!(response.action, action);
        assert!(
            response.message.contains("Roger is not ready"),
            "unexpected message for action={action}: {}",
            response.message
        );
        let guidance = response.guidance.as_deref().unwrap_or_default();
        assert!(
            guidance.contains("Run `rr init`") || guidance.contains("Run `rr extension setup`"),
            "expected setup guidance for action={action}: {:?}",
            response.guidance
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
        !second
            .stderr
            .contains("UNIQUE constraint failed: artifacts.digest"),
        "review path should avoid duplicate artifact digest failures: {}",
        second.stderr
    );
}

#[test]
fn separate_process_review_sequence_avoids_cross_process_artifact_id_collisions() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let review_42 = run_rr_process(
        &["review", "--repo", "owner/repo", "--pr", "42", "--robot"],
        &runtime,
    );
    assert_eq!(
        review_42.status.code(),
        Some(0),
        "review PR 42 failed: {}",
        String::from_utf8_lossy(&review_42.stderr)
    );
    let review_42_payload = parse_robot_payload(std::str::from_utf8(&review_42.stdout).unwrap());
    let bundle_42 = review_42_payload["data"]["resume_bundle_artifact_id"]
        .as_str()
        .expect("bundle id for PR 42")
        .to_owned();

    let review_43 = run_rr_process(
        &["review", "--repo", "owner/repo", "--pr", "43", "--robot"],
        &runtime,
    );
    assert_eq!(
        review_43.status.code(),
        Some(0),
        "review PR 43 failed: {}",
        String::from_utf8_lossy(&review_43.stderr)
    );
    let review_43_payload = parse_robot_payload(std::str::from_utf8(&review_43.stdout).unwrap());
    let bundle_43 = review_43_payload["data"]["resume_bundle_artifact_id"]
        .as_str()
        .expect("bundle id for PR 43")
        .to_owned();
    assert_ne!(
        bundle_42, bundle_43,
        "separate process invocations must not collide on bundle artifact id"
    );

    let review_codex = run_rr_process(
        &[
            "review",
            "--repo",
            "owner/repo",
            "--pr",
            "99",
            "--provider",
            "codex",
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(
        review_codex.status.code(),
        Some(5),
        "codex review should remain degraded tier-a, stderr: {}",
        String::from_utf8_lossy(&review_codex.stderr)
    );
    assert!(
        !String::from_utf8_lossy(&review_codex.stderr).contains("UNIQUE constraint failed"),
        "cross-process sequence must not fail with unique-constraint artifact collisions"
    );
    let codex_payload = parse_robot_payload(std::str::from_utf8(&review_codex.stdout).unwrap());
    assert_eq!(codex_payload["outcome"], "degraded");
    assert_eq!(codex_payload["data"]["provider"], "codex");
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
fn return_with_explicit_session_bypasses_repo_ambiguity() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let review_42 = run_rr(&["review", "--pr", "42", "--robot"], &runtime);
    assert_eq!(review_42.exit_code, 0, "{}", review_42.stderr);
    let review_42_payload = parse_robot_payload(&review_42.stdout);
    let session_42 = review_42_payload["data"]["session_id"]
        .as_str()
        .expect("session id for pr 42")
        .to_owned();

    let review_43 = run_rr(&["review", "--pr", "43", "--robot"], &runtime);
    assert_eq!(review_43.exit_code, 0, "{}", review_43.stderr);

    let explicit_return = run(
        &[
            "return".to_owned(),
            "--session".to_owned(),
            session_42.clone(),
            "--robot".to_owned(),
        ],
        &runtime,
    );
    assert_eq!(explicit_return.exit_code, 0, "{}", explicit_return.stderr);
    let explicit_payload = parse_robot_payload(&explicit_return.stdout);
    assert_eq!(explicit_payload["outcome"], "complete");
    assert_eq!(explicit_payload["data"]["session_id"], session_42);
    assert_eq!(
        explicit_payload["data"]["return_path"],
        "rebound_existing_session"
    );

    let ambiguous_return = run_rr(&["return", "--robot"], &runtime);
    assert_eq!(ambiguous_return.exit_code, 3, "{}", ambiguous_return.stderr);
    let ambiguous_payload = parse_robot_payload(&ambiguous_return.stdout);
    assert_eq!(ambiguous_payload["outcome"], "blocked");
    assert!(
        ambiguous_payload["data"]["reason"]
            .as_str()
            .expect("blocked reason")
            .contains("multiple repo-local sessions"),
    );
}

#[test]
fn resume_dry_run_with_explicit_pr_no_match_fails_closed_without_cross_pr_candidates() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    seed_session_with_provider(&runtime, "opencode", 123, "session-opencode-123");

    let resume = run_rr(&["resume", "--pr", "2", "--dry-run", "--robot"], &runtime);
    assert_eq!(resume.exit_code, 3, "{}", resume.stderr);

    let payload = parse_robot_payload(&resume.stdout);
    assert_eq!(payload["outcome"], "blocked");
    assert!(
        payload["data"]["reason"]
            .as_str()
            .expect("blocked reason")
            .contains("no matching repo-local session found for pull request 2")
    );
    assert_eq!(
        payload["data"]["candidates"]
            .as_array()
            .expect("candidate list")
            .len(),
        0,
        "explicit PR no-match should not include cross-PR picker candidates"
    );
    assert!(
        payload["warnings"]
            .as_array()
            .expect("warnings")
            .iter()
            .any(|warning| warning
                .as_str()
                .expect("warning text")
                .contains("no matching session found")),
        "no-match path should emit truthful no-match warning"
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
        &["review", "--pr", "42", "--provider", "pi-agent", "--robot"],
        &runtime,
    );
    assert_eq!(review.exit_code, 3, "{}", review.stderr);

    let payload = parse_robot_payload(&review.stdout);
    assert_eq!(payload["outcome"], "blocked");
    assert!(
        payload["data"]["supported_providers"]
            .as_array()
            .expect("supported list")
            .iter()
            .any(|p| p.as_str() == Some("opencode"))
    );
}

#[test]
fn review_succeeds_with_degraded_outcome_for_claude_and_gemini() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    for provider in ["claude", "gemini"] {
        let review = run_rr(
            &["review", "--pr", "42", "--provider", provider, "--robot"],
            &runtime,
        );
        // Exits 5 for Degraded because Tier A providers (Claude/Gemini) are always degraded
        assert_eq!(review.exit_code, 5, "provider {} failed: {}", provider, review.stderr);

        let payload = parse_robot_payload(&review.stdout);
        assert_eq!(payload["outcome"], "degraded");
        assert_eq!(payload["data"]["provider"], provider);
    }
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
    assert!(
        items
            .iter()
            .any(|item| item["schema_id"] == "rr.robot.update.v1")
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
    let command_items = compact_payload["data"]["items"]
        .as_array()
        .expect("command items");
    assert!(
        command_items
            .iter()
            .any(|item| item["command"] == "rr update")
    );

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
    let _lock = extension_pack_test_lock()
        .lock()
        .expect("acquire extension pack smoke lock");
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
    assert!(payload["data"]["version"].as_str().is_some());
    assert!(payload["data"]["version_name"].as_str().is_some());
    let package_dir = PathBuf::from(
        payload["data"]["package_dir"]
            .as_str()
            .expect("package_dir should be present"),
    );
    assert!(package_dir.join("SHA256SUMS").exists());
    assert!(package_dir.join("asset-manifest.json").exists());
    let manifest = fs::read_to_string(package_dir.join("manifest.json")).expect("read manifest");
    let manifest_json: serde_json::Value = serde_json::from_str(&manifest).expect("parse manifest");
    assert_eq!(manifest_json["version"], payload["data"]["version"]);
    assert_eq!(
        manifest_json["version_name"],
        payload["data"]["version_name"]
    );
}

#[test]
fn extension_setup_blocks_without_discovered_identity_in_smoke() {
    let _lock = extension_pack_test_lock()
        .lock()
        .expect("acquire extension pack smoke lock");
    let temp = tempdir().expect("tempdir");
    let runtime = CliRuntime {
        cwd: workspace_root(),
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };
    let install_root = temp.path().join("install-root");

    let setup = run(
        &[
            "extension".to_owned(),
            "setup".to_owned(),
            "--browser".to_owned(),
            "chrome".to_owned(),
            "--install-root".to_owned(),
            install_root.to_string_lossy().to_string(),
            "--robot".to_owned(),
        ],
        &runtime,
    );
    assert_eq!(setup.exit_code, 3, "{}", setup.stderr);
    let payload = parse_robot_payload(&setup.stdout);
    assert_eq!(payload["schema_id"], "rr.robot.extension.v1");
    assert_eq!(payload["outcome"], "blocked");
    assert_eq!(payload["data"]["subcommand"], "setup");
    assert_eq!(
        payload["data"]["reason_code"],
        "extension_registration_missing"
    );
    assert_eq!(payload["data"]["browser"], "chrome");
    assert!(
        payload["data"]["manual_browser_step"]
            .as_str()
            .expect("manual browser step")
            .contains("chrome://extensions")
    );
}

#[test]
fn extension_setup_and_doctor_emit_complete_envelopes_in_smoke() {
    let _lock = extension_pack_test_lock()
        .lock()
        .expect("acquire extension pack smoke lock");
    let temp = tempdir().expect("tempdir");
    let runtime = CliRuntime {
        cwd: workspace_root(),
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };
    let install_root = temp.path().join("install-root");
    let extension_id = "abcdefghijklmnopabcdefghijklmnop";
    write_guided_profile_discovery_state(&runtime, "edge", extension_id);

    let setup = run(
        &[
            "extension".to_owned(),
            "setup".to_owned(),
            "--browser".to_owned(),
            "edge".to_owned(),
            "--install-root".to_owned(),
            install_root.to_string_lossy().to_string(),
            "--robot".to_owned(),
        ],
        &runtime,
    );
    assert_eq!(setup.exit_code, 0, "{}", setup.stderr);
    let setup_payload = parse_robot_payload(&setup.stdout);
    assert_eq!(setup_payload["schema_id"], "rr.robot.extension.v1");
    assert_eq!(setup_payload["outcome"], "complete");
    assert_eq!(setup_payload["data"]["subcommand"], "setup");
    assert_eq!(setup_payload["data"]["browser"], "edge");
    assert_eq!(setup_payload["data"]["extension_id"], extension_id);
    assert_eq!(
        setup_payload["data"]["extension_id_source"],
        "browser_profile_preferences"
    );
    assert_eq!(
        setup_payload["data"]["doctor"]["subcommand"], "doctor",
        "setup should embed doctor result envelope"
    );
    assert!(
        setup_payload["data"]["doctor"]["checks"]
            .as_array()
            .expect("setup doctor checks")
            .iter()
            .all(|entry| entry["ok"] == true)
    );

    let doctor = run(
        &[
            "extension".to_owned(),
            "doctor".to_owned(),
            "--browser".to_owned(),
            "edge".to_owned(),
            "--install-root".to_owned(),
            install_root.to_string_lossy().to_string(),
            "--robot".to_owned(),
        ],
        &runtime,
    );
    assert_eq!(doctor.exit_code, 0, "{}", doctor.stderr);
    let doctor_payload = parse_robot_payload(&doctor.stdout);
    assert_eq!(doctor_payload["schema_id"], "rr.robot.extension.v1");
    assert_eq!(doctor_payload["outcome"], "complete");
    assert_eq!(doctor_payload["data"]["subcommand"], "doctor");
    assert_eq!(doctor_payload["data"]["browser"], "edge");
    assert!(
        doctor_payload["data"]["checks"]
            .as_array()
            .expect("doctor checks")
            .iter()
            .all(|entry| entry["ok"] == true)
    );
}

#[test]
fn extension_setup_and_doctor_succeed_after_bridge_registration_event_in_smoke() {
    let _lock = extension_pack_test_lock()
        .lock()
        .expect("acquire extension pack smoke lock");
    let temp = tempdir().expect("tempdir");
    let runtime = CliRuntime {
        cwd: workspace_root(),
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };
    let install_root = temp.path().join("install-root");
    let extension_id = "abcdefghijklmnopabcdefghijklmnop";

    let blocked = run(
        &[
            "extension".to_owned(),
            "setup".to_owned(),
            "--browser".to_owned(),
            "brave".to_owned(),
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
        "extension_registration_missing"
    );

    register_extension_identity_via_bridge(&runtime, "brave", extension_id);

    let setup = run(
        &[
            "extension".to_owned(),
            "setup".to_owned(),
            "--browser".to_owned(),
            "brave".to_owned(),
            "--install-root".to_owned(),
            install_root.to_string_lossy().to_string(),
            "--robot".to_owned(),
        ],
        &runtime,
    );
    assert_eq!(setup.exit_code, 0, "{}", setup.stderr);
    let setup_payload = parse_robot_payload(&setup.stdout);
    assert_eq!(setup_payload["outcome"], "complete");
    assert_eq!(setup_payload["data"]["browser"], "brave");
    assert_eq!(setup_payload["data"]["extension_id"], extension_id);
    assert_eq!(
        setup_payload["data"]["extension_id_source"],
        "store_registry"
    );

    let doctor = run(
        &[
            "extension".to_owned(),
            "doctor".to_owned(),
            "--browser".to_owned(),
            "brave".to_owned(),
            "--install-root".to_owned(),
            install_root.to_string_lossy().to_string(),
            "--robot".to_owned(),
        ],
        &runtime,
    );
    assert_eq!(doctor.exit_code, 0, "{}", doctor.stderr);
    let doctor_payload = parse_robot_payload(&doctor.stdout);
    assert_eq!(doctor_payload["outcome"], "complete");
    assert!(
        doctor_payload["data"]["checks"]
            .as_array()
            .expect("doctor checks")
            .iter()
            .all(|entry| entry["ok"] == true)
    );
}

#[test]
fn extension_setup_auto_completes_when_identity_is_observed_during_wait_in_smoke() {
    let _lock = extension_pack_test_lock()
        .lock()
        .expect("acquire extension pack smoke lock");
    let temp = tempdir().expect("tempdir");
    let runtime = CliRuntime {
        cwd: workspace_root(),
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };
    let install_root = temp.path().join("install-root");
    let extension_id = "abcdefghijklmnopabcdefghijklmnop";
    let runtime_for_observer = runtime.clone();
    let extension_id_for_observer = extension_id.to_owned();
    let observer = thread::spawn(move || {
        thread::sleep(Duration::from_millis(900));
        write_guided_profile_discovery_state(
            &runtime_for_observer,
            "chrome",
            &extension_id_for_observer,
        );
    });

    let setup = run(
        &[
            "extension".to_owned(),
            "setup".to_owned(),
            "--browser".to_owned(),
            "chrome".to_owned(),
            "--install-root".to_owned(),
            install_root.to_string_lossy().to_string(),
            "--robot".to_owned(),
        ],
        &runtime,
    );
    observer
        .join()
        .expect("join guided-profile registration observer");
    assert_eq!(setup.exit_code, 0, "{}", setup.stderr);
    let setup_payload = parse_robot_payload(&setup.stdout);
    assert_eq!(setup_payload["outcome"], "complete");
    assert_eq!(setup_payload["data"]["subcommand"], "setup");
    assert_eq!(setup_payload["data"]["browser"], "chrome");
    assert_eq!(setup_payload["data"]["extension_id"], extension_id);
    assert_eq!(
        setup_payload["data"]["extension_id_source"],
        "browser_profile_preferences"
    );
    assert_eq!(
        setup_payload["data"]["registration_observed_during_setup_wait"],
        true
    );
    assert!(
        setup_payload["data"]["doctor"]["checks"]
            .as_array()
            .expect("doctor checks")
            .iter()
            .all(|entry| entry["ok"] == true)
    );
}

#[test]
fn extension_setup_discovers_identity_from_guided_profile_preferences_in_smoke() {
    let _lock = extension_pack_test_lock()
        .lock()
        .expect("acquire extension pack smoke lock");
    let temp = tempdir().expect("tempdir");
    let runtime = CliRuntime {
        cwd: workspace_root(),
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };
    let install_root = temp.path().join("install-root");
    let extension_id = "abcdefghijklmnopabcdefghijklmnop";
    write_guided_profile_discovery_state(&runtime, "chrome", extension_id);

    let setup = run(
        &[
            "extension".to_owned(),
            "setup".to_owned(),
            "--browser".to_owned(),
            "chrome".to_owned(),
            "--install-root".to_owned(),
            install_root.to_string_lossy().to_string(),
            "--robot".to_owned(),
        ],
        &runtime,
    );
    assert_eq!(setup.exit_code, 0, "{}", setup.stderr);
    let setup_payload = parse_robot_payload(&setup.stdout);
    assert_eq!(setup_payload["outcome"], "complete");
    assert_eq!(
        setup_payload["data"]["extension_id_source"],
        "browser_profile_preferences"
    );
    assert_eq!(setup_payload["data"]["extension_id"], extension_id);
    let registry_path = runtime.store_root.join("bridge/extension-id");
    let persisted = fs::read_to_string(registry_path).expect("persisted extension id");
    assert_eq!(persisted.trim(), extension_id);
}

#[test]
fn bridge_install_uninstall_is_failure_closed_and_reports_asset_checksums_in_smoke() {
    let _lock = extension_pack_test_lock()
        .lock()
        .expect("acquire extension pack smoke lock");
    let temp = tempdir().expect("tempdir");
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
        "extension_id_discovery_failed"
    );

    let extension_registry = runtime.store_root.join("bridge/extension-id");
    fs::create_dir_all(
        extension_registry
            .parent()
            .expect("extension registry parent"),
    )
    .expect("create extension registry parent");
    fs::write(&extension_registry, "abcdefghijklmnopabcdefghijklmnop\n")
        .expect("write extension identity registry");

    let install = run(
        &[
            "bridge".to_owned(),
            "install".to_owned(),
            "--install-root".to_owned(),
            install_root.to_string_lossy().to_string(),
            "--robot".to_owned(),
        ],
        &runtime,
    );
    assert_eq!(install.exit_code, 0, "{}", install.stderr);
    let install_payload = parse_robot_payload(&install.stdout);
    assert_eq!(install_payload["outcome"], "complete");
    assert_eq!(
        install_payload["data"]["extension_id_source"],
        "store_registry"
    );
    assert_eq!(
        install_payload["data"]["bridge_binary_source"],
        "installed_rr_current_exe"
    );
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
