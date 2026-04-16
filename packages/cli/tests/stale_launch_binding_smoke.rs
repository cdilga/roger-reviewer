#![cfg(unix)]

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

fn count_launch_attempts(store_root: &Path, action: &str, requested_session_id: &str) -> i64 {
    let conn = Connection::open(store_root.join("roger.db")).expect("open roger.db");
    conn.query_row(
        "SELECT COUNT(*)
         FROM launch_attempts
         WHERE action = ?1 AND requested_session_id = ?2",
        params![action, requested_session_id],
        |row| row.get(0),
    )
    .expect("launch attempt count")
}

#[test]
fn resume_blocks_cross_root_binding_reuse_even_when_repo_and_pr_match() {
    let temp = tempdir().expect("tempdir");
    let repo_a = init_repo_named(&temp, "repo-a");
    let repo_b = init_repo_named(&temp, "repo-b");
    let (_stub_dir, opencode_bin) = write_stub_binary();

    let store_root = temp.path().join("roger-store");
    let review_runtime = CliRuntime {
        cwd: repo_a,
        store_root: store_root.clone(),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let review = run_rr(&["review", "--pr", "42", "--robot"], &review_runtime);
    assert_eq!(review.exit_code, 0, "{}", review.stderr);
    let review_payload = parse_robot_payload(&review.stdout);
    let session_id = review_payload["data"]["session_id"]
        .as_str()
        .expect("session id")
        .to_owned();

    let resume_runtime = CliRuntime {
        cwd: repo_b,
        store_root: store_root.clone(),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let resume = run_rr(&["resume", "--pr", "42", "--robot"], &resume_runtime);
    assert_eq!(resume.exit_code, 3, "{}", resume.stderr);
    let payload = parse_robot_payload(&resume.stdout);
    assert_eq!(payload["outcome"], "blocked");
    assert!(
        payload["data"]["reason"]
            .as_str()
            .is_some_and(|text| text.contains("launch binding is stale"))
    );
    assert!(
        payload["data"]["reason"]
            .as_str()
            .is_some_and(|text| text.contains("worktree root mismatch"))
    );
    assert_eq!(
        payload["data"]["candidates"][0]["session_id"],
        Value::String(session_id.clone())
    );
    assert_eq!(
        count_launch_attempts(&store_root, "resume_review", &session_id),
        0
    );
}
