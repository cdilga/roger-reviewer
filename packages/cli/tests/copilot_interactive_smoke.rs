#![cfg(unix)]

//! Copilot interactive drop-in + hook audit wiring
//! (bead rr-copilot-interactive-dropin-audit-jaj3).

use roger_cli::{CliRuntime, run};
use roger_session_copilot::{ENV_COPILOT_ADMISSION_GATE, ENV_COPILOT_BIN};
use roger_storage::RogerStore;
use serde_json::Value;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use tempfile::tempdir;

fn init_repo(path: &Path) {
    fs::create_dir_all(path).expect("create repo path");
    let init = Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("init")
        .output()
        .expect("git init");
    assert!(init.status.success(), "git init failed");
    let remote = Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("remote")
        .arg("add")
        .arg("origin")
        .arg("git@github.com:owner/repo.git")
        .output()
        .expect("git remote add");
    assert!(remote.status.success(), "git remote add failed");
}

fn write_executable(path: &Path, contents: &str) {
    fs::write(path, contents).expect("write executable");
    let mut permissions = fs::metadata(path).expect("stat executable").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).expect("chmod executable");
}

fn run_with_env_overrides<T>(
    overrides: &[(&str, Option<&str>)],
    run_block: impl FnOnce() -> T,
) -> T {
    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = ENV_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("env lock");

    let previous: Vec<(String, Option<OsString>)> = overrides
        .iter()
        .map(|(key, _)| ((*key).to_owned(), std::env::var_os(key)))
        .collect();

    for (key, value) in overrides {
        match value {
            // SAFETY: tests serialize env mutation via ENV_LOCK and restore before return.
            Some(value) => unsafe { std::env::set_var(key, value) },
            None => unsafe { std::env::remove_var(key) },
        }
    }

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(run_block));

    for (key, value) in previous {
        match value {
            // SAFETY: tests serialize env mutation via ENV_LOCK and restore before return.
            Some(value) => unsafe { std::env::set_var(&key, value) },
            None => unsafe { std::env::remove_var(&key) },
        }
    }

    match result {
        Ok(value) => value,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

// ---------------------------------------------------------------------------
// Parse-time gating
// ---------------------------------------------------------------------------

fn gating_runtime(temp: &Path) -> CliRuntime {
    let repo = temp.join("repo");
    init_repo(&repo);
    CliRuntime {
        cwd: repo,
        store_root: temp.join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    }
}

#[test]
fn interactive_rejects_robot_at_parse_time() {
    let temp = tempdir().expect("tempdir");
    let runtime = gating_runtime(temp.path());
    let result = run_with_env_overrides(&[(ENV_COPILOT_ADMISSION_GATE, Some("1"))], || {
        run(
            &[
                "review".to_owned(),
                "--pr".to_owned(),
                "42".to_owned(),
                "--provider".to_owned(),
                "copilot".to_owned(),
                "--interactive".to_owned(),
                "--robot".to_owned(),
            ],
            &runtime,
        )
    });
    assert_eq!(result.exit_code, 2, "{}", result.stdout);
    assert!(
        result
            .stderr
            .contains("--robot and --interactive cannot be combined"),
        "unexpected stderr: {}",
        result.stderr
    );
}

#[test]
fn interactive_rejects_non_copilot_provider_at_parse_time() {
    let temp = tempdir().expect("tempdir");
    let runtime = gating_runtime(temp.path());
    let result = run_with_env_overrides(&[(ENV_COPILOT_ADMISSION_GATE, Some("1"))], || {
        run(
            &[
                "review".to_owned(),
                "--pr".to_owned(),
                "42".to_owned(),
                "--provider".to_owned(),
                "opencode".to_owned(),
                "--interactive".to_owned(),
            ],
            &runtime,
        )
    });
    assert_eq!(result.exit_code, 2, "{}", result.stdout);
    assert!(
        result
            .stderr
            .contains("--interactive is only supported with --provider copilot"),
        "unexpected stderr: {}",
        result.stderr
    );
}

#[test]
fn interactive_rejects_when_gate_disabled_at_parse_time() {
    let temp = tempdir().expect("tempdir");
    let runtime = gating_runtime(temp.path());
    let result = run_with_env_overrides(&[(ENV_COPILOT_ADMISSION_GATE, None)], || {
        run(
            &[
                "review".to_owned(),
                "--pr".to_owned(),
                "42".to_owned(),
                "--provider".to_owned(),
                "copilot".to_owned(),
                "--interactive".to_owned(),
            ],
            &runtime,
        )
    });
    assert_eq!(result.exit_code, 2, "{}", result.stdout);
    assert!(
        result.stderr.contains("RR_ENABLE_COPILOT_PROVIDER=1"),
        "unexpected stderr: {}",
        result.stderr
    );
}

#[test]
fn interactive_rejected_for_non_launch_command() {
    let temp = tempdir().expect("tempdir");
    let runtime = gating_runtime(temp.path());
    let result = run_with_env_overrides(&[(ENV_COPILOT_ADMISSION_GATE, Some("1"))], || {
        run(&["status".to_owned(), "--interactive".to_owned()], &runtime)
    });
    assert_eq!(result.exit_code, 2, "{}", result.stdout);
    assert!(
        result
            .stderr
            .contains("--interactive is only supported by rr review, rr resume, and rr return"),
        "unexpected stderr: {}",
        result.stderr
    );
}

// ---------------------------------------------------------------------------
// Interactive launch: inherited stdio + audit wiring
// ---------------------------------------------------------------------------

#[test]
fn interactive_review_uses_inherited_stdio_verifies_and_ingests_hook_audit() {
    let temp = tempdir().expect("tempdir");
    let repo = temp.path().join("repo");
    init_repo(&repo);
    let store_root = temp.path().join("roger-store");
    let stdin_state = temp.path().join("stdin-state");
    let audit_env_capture = temp.path().join("audit-env");

    // Double records inherited-stdio state + the audit dir env, simulates two
    // hook denial events into the audit dir, then writes a verified session
    // start artifact. Its stdout marker must NOT be captured (interactive mode
    // inherits stdout), so we assert the persisted capture stdout is empty.
    let fake_copilot = temp.path().join("fake-copilot-interactive");
    let script = r#"#!/bin/sh
set -eu
if [ -t 0 ]; then echo tty > "$RR_TEST_STDIN_STATE"; else echo notty > "$RR_TEST_STDIN_STATE"; fi
printf '%s\n' "${RR_COPILOT_HOOK_AUDIT_DIR:-MISSING}" > "$RR_TEST_AUDIT_ENV"
mkdir -p "$RR_COPILOT_HOOK_AUDIT_DIR"
printf '{"event":"denial","tool":"shell"}\n{"event":"denial","tool":"write"}\n' > "$RR_COPILOT_HOOK_AUDIT_DIR/pre-tool-use.jsonl"
mkdir -p "$(dirname "$RR_COPILOT_SESSION_START_ARTIFACT")"
cat > "$RR_COPILOT_SESSION_START_ARTIFACT" <<EOF
{"hook":"session-start","payload":{"provider":"copilot","session_id":"cp-int-001","worktree_root":"$PWD","launch_profile_id":"profile-open-pr"}}
EOF
printf 'DOUBLE_STDOUT_MARKER\n'
"#;
    write_executable(&fake_copilot, script);

    let runtime = CliRuntime {
        cwd: repo.clone(),
        store_root: store_root.clone(),
        opencode_bin: "opencode".to_owned(),
    };

    let copilot_bin = fake_copilot.to_string_lossy().to_string();
    let stdin_state_str = stdin_state.to_string_lossy().to_string();
    let audit_env_str = audit_env_capture.to_string_lossy().to_string();
    let review = run_with_env_overrides(
        &[
            (ENV_COPILOT_ADMISSION_GATE, Some("1")),
            (ENV_COPILOT_BIN, Some(copilot_bin.as_str())),
            ("RR_TEST_STDIN_STATE", Some(stdin_state_str.as_str())),
            ("RR_TEST_AUDIT_ENV", Some(audit_env_str.as_str())),
        ],
        || {
            run(
                &[
                    "review".to_owned(),
                    "--pr".to_owned(),
                    "42".to_owned(),
                    "--provider".to_owned(),
                    "copilot".to_owned(),
                    "--interactive".to_owned(),
                ],
                &runtime,
            )
        },
    );

    // Interactive launch is not --robot, so it renders human output. A verified
    // start completes (exit 0) or degrades (exit 5); either way it must not
    // block/error.
    assert!(
        review.exit_code == 0 || review.exit_code == 5,
        "unexpected exit: {} stdout={} stderr={}",
        review.exit_code,
        review.stdout,
        review.stderr
    );
    assert!(
        review.stdout.contains("captured 2 hook audit events")
            || review.stderr.contains("captured 2 hook audit events"),
        "expected audit-count surfaced: stdout={} stderr={}",
        review.stdout,
        review.stderr
    );

    // The double actually received an inherited stdin fd it could stat.
    assert!(stdin_state.is_file(), "double did not record stdin state");
    // The CLI exported the audit dir env to the child.
    let audit_env = fs::read_to_string(&audit_env_capture)
        .expect("read audit env capture")
        .trim()
        .to_owned();
    assert!(
        audit_env.contains("hook-audit") && audit_env.contains("roger-store"),
        "unexpected audit dir env: {audit_env}"
    );

    // Inspect persisted evidence.
    let store = RogerStore::open(&store_root).expect("open store");
    let sessions = store
        .find_sessions_by_target("owner/repo", 42)
        .expect("find sessions by target");
    let session = sessions.first().expect("a review session");
    let locator = session.session_locator.as_ref().expect("session locator");
    assert_eq!(locator.session_id, "cp-int-001");
    let invocation: Value =
        serde_json::from_str(&locator.invocation_context_json).expect("parse invocation context");
    assert_eq!(invocation["interactive"], true);
    assert_eq!(invocation["execution_mode"], "interactive");
    assert_eq!(invocation["hook_audit_event_count"], 2);
    assert!(
        invocation["artifact_refs"]
            .as_array()
            .expect("artifact refs")
            .iter()
            .any(|value| value
                .as_str()
                .is_some_and(|s| s.ends_with("pre-tool-use.jsonl"))),
        "audit jsonl should be recorded as an artifact ref: {invocation}"
    );

    // Launch capture reflects interactive mode with empty captured stdout.
    let launch_capture_path = invocation["launch_capture_path"]
        .as_str()
        .expect("launch capture path");
    let launch_capture: Value =
        serde_json::from_slice(&fs::read(launch_capture_path).expect("read launch capture"))
            .expect("parse launch capture");
    assert_eq!(launch_capture["interactive"], true);
    assert_eq!(launch_capture["execution_mode"], "interactive");
    assert_eq!(launch_capture["hook_audit_event_count"], 2);
    assert_eq!(
        launch_capture["stdout"], "",
        "interactive stdout should not be captured (inherited to terminal)"
    );
    assert!(
        !launch_capture["stdout"]
            .as_str()
            .unwrap_or_default()
            .contains("DOUBLE_STDOUT_MARKER"),
        "interactive marker must not be captured"
    );
}
