#![cfg(unix)]

//! Bridge launch-surface truth + poisoned-cwd repair.
//!
//! A bridge-origin launch inherits the browser's cwd, which is frequently a
//! poisoned path under NativeMessagingHosts. These tests prove that when rr is
//! launched with `--surface bridge`:
//!   1. the persisted launch attempt and binding carry surface=bridge;
//!   2. the binding records no repo-local cwd/worktree (neutral);
//!   3. later `rr status --session` / `rr resume --session` readback does NOT
//!      fail closed on the stale-binding invariant, even from a different root.
//!
//! The CLI/TUI stale-binding safety invariant itself stays covered by
//! `stale_launch_binding_smoke.rs`.

use roger_cli::{CliRuntime, run};
use rusqlite::{Connection, params};
use serde_json::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::{TempDir, tempdir};

fn run_rr(args: &[&str], runtime: &CliRuntime) -> roger_cli::CliRunResult {
    let argv = args
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    run(&argv, runtime)
}

fn parse_robot_payload(stdout: &str) -> Value {
    serde_json::from_str(stdout).expect("robot payload json")
}

fn init_repo_named(temp: &TempDir, name: &str) -> PathBuf {
    let repo = temp.path().join(name);
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

fn write_stub_binary() -> (TempDir, PathBuf) {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("opencode-stub");
    let script = r#"#!/bin/sh
if [ "$1" = "--session" ]; then
  exit 0
fi
if [ "$1" = "export" ]; then
  echo "{}"
  exit 0
fi
exit 0
"#;

    fs::write(&path, script).expect("write stub binary");
    let mut perms = fs::metadata(&path).expect("metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).expect("chmod stub binary");
    (dir, path)
}

/// A poisoned, non-repo working directory in the spirit of the browser's
/// NativeMessagingHosts cwd.
fn poisoned_cwd(temp: &TempDir) -> PathBuf {
    let poisoned = temp
        .path()
        .join("Library/Application Support/Microsoft Edge/NativeMessagingHosts");
    fs::create_dir_all(&poisoned).expect("create poisoned cwd");
    poisoned
}

fn launch_attempt_surface(store_root: &Path, session_id: &str, action: &str) -> String {
    // start_review attempts are created before the session id is known, so tie
    // by final_session_id (set on finalize) or requested_session_id (resume).
    let conn = Connection::open(store_root.join("roger.db")).expect("open roger.db");
    conn.query_row(
        "SELECT source_surface
         FROM launch_attempts
         WHERE action = ?2
           AND (final_session_id = ?1 OR requested_session_id = ?1)
         ORDER BY rowid DESC
         LIMIT 1",
        params![session_id, action],
        |row| row.get::<_, String>(0),
    )
    .expect("launch attempt surface")
}

fn binding_surface_and_cwd(store_root: &Path, session_id: &str) -> (String, Option<String>) {
    let conn = Connection::open(store_root.join("roger.db")).expect("open roger.db");
    conn.query_row(
        "SELECT surface, cwd
         FROM session_launch_bindings
         WHERE session_id = ?1
         ORDER BY updated_at DESC
         LIMIT 1",
        params![session_id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
    )
    .expect("binding surface + cwd")
}

fn start_bridge_review(temp: &TempDir, store_root: &Path, opencode_bin: &Path) -> String {
    let bridge_runtime = CliRuntime {
        cwd: poisoned_cwd(temp),
        store_root: store_root.to_path_buf(),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };
    // Mirror the bridge child argv: explicit --repo + --surface bridge.
    let review = run_rr(
        &[
            "review",
            "--repo",
            "owner/repo",
            "--pr",
            "42",
            "--surface",
            "bridge",
            "--robot",
        ],
        &bridge_runtime,
    );
    assert_eq!(review.exit_code, 0, "review stderr: {}", review.stderr);
    let payload = parse_robot_payload(&review.stdout);
    payload["data"]["session_id"]
        .as_str()
        .expect("session id")
        .to_owned()
}

#[test]
fn bridge_origin_launch_records_surface_and_neutral_cwd() {
    let temp = tempdir().expect("tempdir");
    let (_stub_dir, opencode_bin) = write_stub_binary();
    let store_root = temp.path().join("roger-store");

    let session_id = start_bridge_review(&temp, &store_root, &opencode_bin);

    // The launch attempt records the true origin surface, not a masqueraded CLI.
    assert_eq!(
        launch_attempt_surface(&store_root, &session_id, "start_review"),
        "bridge"
    );

    // The binding is surface-typed and carries NO repo-local cwd, so the
    // poisoned browser cwd never lands in durable binding context.
    let (surface, cwd) = binding_surface_and_cwd(&store_root, &session_id);
    assert_eq!(surface, "bridge");
    assert!(
        cwd.is_none(),
        "bridge binding must not bind a repo-local cwd, found {cwd:?}"
    );
}

#[test]
fn bridge_origin_launch_does_not_block_session_readback_from_a_clean_root() {
    let temp = tempdir().expect("tempdir");
    let (_stub_dir, opencode_bin) = write_stub_binary();
    let store_root = temp.path().join("roger-store");

    let session_id = start_bridge_review(&temp, &store_root, &opencode_bin);

    // Read back the session from a different, clean git worktree. Before the
    // fix the poisoned-cwd binding tripped the stale-binding invariant here.
    let clean_repo = init_repo_named(&temp, "clean-repo");
    let clean_runtime = CliRuntime {
        cwd: clean_repo,
        store_root: store_root.clone(),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let status = run_rr(
        &["status", "--session", &session_id, "--robot"],
        &clean_runtime,
    );
    assert_ne!(status.exit_code, 3, "status blocked: {}", status.stdout);
    let status_payload = parse_robot_payload(&status.stdout);
    assert_ne!(status_payload["outcome"], "blocked");
    assert!(
        status_payload["data"]["reason"]
            .as_str()
            .map_or(true, |reason| !reason.contains("stale")),
        "status must not report a stale binding: {}",
        status.stdout
    );

    let resume = run_rr(
        &["resume", "--session", &session_id, "--dry-run", "--robot"],
        &clean_runtime,
    );
    assert_ne!(resume.exit_code, 3, "resume blocked: {}", resume.stdout);
    let resume_payload = parse_robot_payload(&resume.stdout);
    assert_ne!(resume_payload["outcome"], "blocked");
    assert!(
        resume_payload["data"]["reason"]
            .as_str()
            .map_or(true, |reason| !reason.contains("stale")),
        "resume must not report a stale binding: {}",
        resume.stdout
    );
}

#[test]
fn bridge_surface_resume_readback_is_exempt_from_stale_binding_across_roots() {
    let temp = tempdir().expect("tempdir");
    let (_stub_dir, opencode_bin) = write_stub_binary();
    let store_root = temp.path().join("roger-store");

    let session_id = start_bridge_review(&temp, &store_root, &opencode_bin);

    // Even when the readback explicitly targets the bridge surface (so the
    // bridge binding is matched), the validator recognizes the typed
    // bridge-origin binding and exempts it from the local-root staleness check.
    let clean_repo = init_repo_named(&temp, "another-root");
    let clean_runtime = CliRuntime {
        cwd: clean_repo,
        store_root: store_root.clone(),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let resume = run_rr(
        &[
            "resume",
            "--session",
            &session_id,
            "--surface",
            "bridge",
            "--dry-run",
            "--robot",
        ],
        &clean_runtime,
    );
    assert_ne!(
        resume.exit_code, 3,
        "bridge-surface resume blocked: {}",
        resume.stdout
    );
    let payload = parse_robot_payload(&resume.stdout);
    assert_ne!(payload["outcome"], "blocked");
    assert!(
        payload["data"]["reason"]
            .as_str()
            .map_or(true, |reason| !reason.contains("stale")),
        "bridge-surface resume must not report a stale binding: {}",
        resume.stdout
    );
}
