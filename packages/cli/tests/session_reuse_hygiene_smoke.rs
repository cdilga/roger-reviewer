#![cfg(unix)]

//! Session lifecycle hygiene proof (bead rr-session-lifecycle-hygiene-xrjx):
//! two consecutive `rr review` runs on the same PR must yield ONE session
//! (reuse-or-new), while `--fresh` forces a second session.

use roger_cli::{CliRuntime, run};
use roger_storage::{RogerStore, SessionFinderQuery};
use serde_json::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use tempfile::{TempDir, tempdir};

fn run_rr(args: &[&str], runtime: &CliRuntime) -> roger_cli::CliRunResult {
    let argv = args.iter().map(|v| v.to_string()).collect::<Vec<_>>();
    run(&argv, runtime)
}

fn parse_json(stdout: &str) -> Value {
    serde_json::from_str(stdout).expect("json payload")
}

fn write_stub_binary() -> (TempDir, PathBuf) {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("opencode-stub");
    let script = "#!/bin/sh\nif [ \"$1\" = \"export\" ]; then echo '{}'; fi\nexit 0\n";
    fs::write(&path, script).expect("write stub");
    let mut perms = fs::metadata(&path).expect("metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).expect("chmod stub");
    (dir, path)
}

fn init_repo(temp: &TempDir) -> PathBuf {
    let repo = temp.path().join("repo");
    fs::create_dir_all(&repo).expect("create repo dir");
    Command::new("git")
        .arg("-C")
        .arg(&repo)
        .arg("init")
        .output()
        .expect("git init");
    Command::new("git")
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
    repo
}

fn session_count(runtime: &CliRuntime) -> usize {
    let store = RogerStore::open(&runtime.store_root).expect("open store");
    store
        .session_finder(SessionFinderQuery {
            repository: Some("owner/repo".to_owned()),
            pull_request_number: Some(42),
            attention_states: Vec::new(),
            limit: 250,
        })
        .expect("session finder")
        .len()
}

#[test]
fn two_reviews_on_same_pr_reuse_one_session() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary();

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let first = run_rr(&["review", "--pr", "42", "--robot"], &runtime);
    assert_eq!(first.exit_code, 0, "{}", first.stderr);
    let first_session = parse_json(&first.stdout)["data"]["session_id"]
        .as_str()
        .expect("first session id")
        .to_owned();
    assert_eq!(session_count(&runtime), 1);

    let second = run_rr(&["review", "--pr", "42", "--robot"], &runtime);
    assert_eq!(second.exit_code, 0, "{}", second.stderr);
    let second_payload = parse_json(&second.stdout);
    // Additive reuse marker; no schema id change.
    assert_eq!(second_payload["data"]["reused"], true, "{second_payload}");
    assert_eq!(second_payload["data"]["session_id"], first_session);
    // Still exactly one session for this PR — no zombie.
    assert_eq!(session_count(&runtime), 1);
}

#[test]
fn fresh_flag_forces_a_second_session() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary();

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let first = run_rr(&["review", "--pr", "42", "--robot"], &runtime);
    assert_eq!(first.exit_code, 0, "{}", first.stderr);
    assert_eq!(session_count(&runtime), 1);

    let second = run_rr(&["review", "--pr", "42", "--fresh", "--robot"], &runtime);
    assert_eq!(second.exit_code, 0, "{}", second.stderr);
    let second_payload = parse_json(&second.stdout);
    assert!(
        second_payload["data"]["reused"].is_null(),
        "fresh launch must not be marked reused: {second_payload}"
    );
    assert_eq!(session_count(&runtime), 2, "--fresh yields a new session");
}

#[test]
fn human_review_output_lists_session_and_next_commands() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary();

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let human = run_rr(&["review", "--pr", "42"], &runtime);
    assert_eq!(human.exit_code, 0, "{}", human.stderr);
    let stdout = human.stdout;
    assert!(stdout.contains("review session launched"), "{stdout}");
    assert!(stdout.contains("session:"), "{stdout}");
    assert!(stdout.contains("worker-task:"), "{stdout}");
    assert!(stdout.contains("rr open --session"), "{stdout}");
    assert!(stdout.contains("rr findings --session"), "{stdout}");
}
