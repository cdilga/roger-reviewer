#![cfg(unix)]

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread::sleep;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tempfile::tempdir;

fn run_git(repo: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .expect("run git command");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn init_repo(repo: &Path) {
    fs::create_dir_all(repo).expect("create repo");
    run_git(repo, &["init"]);
    run_git(
        repo,
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/owner/repo.git",
        ],
    );
}

fn parse_payload(stdout: &[u8]) -> Value {
    serde_json::from_slice(stdout).expect("parse robot payload")
}

fn run_rr(rr_bin: &Path, repo: &Path, store: &Path, args: &[&str]) -> (i32, Value, String) {
    let output = Command::new(rr_bin)
        .args(args)
        .current_dir(repo)
        .env("RR_STORE_ROOT", store)
        .env("RR_OPENCODE_BIN", "opencode")
        .output()
        .expect("run rr");

    let status = output.status.code().unwrap_or(1);
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let payload = parse_payload(&output.stdout);
    (status, payload, stderr)
}

fn rr_binary_path() -> PathBuf {
    if let Some(path) = option_env!("CARGO_BIN_EXE_rr") {
        return PathBuf::from(path);
    }

    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("packages parent")
        .parent()
        .expect("workspace root")
        .to_path_buf();
    let rr_path = workspace_root.join("target/debug/rr");
    if rr_path.exists() {
        return rr_path;
    }

    let status = Command::new("cargo")
        .args(["build", "-p", "roger-cli", "--bin", "rr"])
        .current_dir(&workspace_root)
        .status()
        .expect("build rr binary");
    assert!(status.success(), "cargo build -p roger-cli --bin rr failed");
    rr_path
}

fn wait_for_fresh_second_window(min_remaining_ms: u128) {
    let second_ms = 1_000u128;
    loop {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch");
        let millis_into_second = now.as_millis() % second_ms;
        let remaining = second_ms - millis_into_second;
        if remaining >= min_remaining_ms {
            return;
        }
        sleep(Duration::from_millis(5));
    }
}

#[test]
fn rapid_process_reviews_do_not_collide_on_generated_ids() {
    let rr_bin = rr_binary_path();
    let tmp = tempdir().expect("tempdir");
    let repo = tmp.path().join("repo");
    let store = tmp.path().join("store");
    init_repo(&repo);
    fs::create_dir_all(&store).expect("create store");

    // Ensure both invocations happen early in the same second.
    wait_for_fresh_second_window(850);

    let (first_status, first_payload, first_stderr) = run_rr(
        Path::new(&rr_bin),
        &repo,
        &store,
        &["review", "--pr", "2", "--provider", "opencode", "--robot"],
    );
    assert_eq!(first_status, 0, "{first_stderr}");
    assert_eq!(first_payload["exit_code"], 0, "{first_payload}");
    assert_eq!(first_payload["outcome"], "complete");

    let (second_status, second_payload, second_stderr) = run_rr(
        Path::new(&rr_bin),
        &repo,
        &store,
        &["review", "--pr", "3", "--provider", "opencode", "--robot"],
    );
    assert_eq!(second_status, 0, "{second_stderr}");
    assert_eq!(second_payload["exit_code"], 0, "{second_payload}");
    assert_eq!(second_payload["outcome"], "complete");

    let first_session_id = first_payload["data"]["session_id"]
        .as_str()
        .expect("first session id");
    let second_session_id = second_payload["data"]["session_id"]
        .as_str()
        .expect("second session id");
    assert_ne!(first_session_id, second_session_id);

    let first_bundle_id = first_payload["data"]["resume_bundle_artifact_id"]
        .as_str()
        .expect("first bundle id");
    let second_bundle_id = second_payload["data"]["resume_bundle_artifact_id"]
        .as_str()
        .expect("second bundle id");
    assert_ne!(first_bundle_id, second_bundle_id);
}
