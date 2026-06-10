#![cfg(unix)]

use roger_cli::{CliRuntime, run};
use serde_json::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
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

#[test]
fn non_robot_codex_review_and_resume_exit_zero_while_preserving_bounded_warning() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary();

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let review = run_rr(&["review", "--pr", "42", "--provider", "codex"], &runtime);
    assert_eq!(review.exit_code, 0, "{}", review.stderr);
    assert!(
        review.stdout.contains("review session launched"),
        "unexpected review stdout: {}",
        review.stdout
    );
    assert!(
        review.stderr.contains("bounded support")
            && review
                .stderr
                .contains("does not support locator reopen or rr return"),
        "unexpected review stderr: {}",
        review.stderr
    );

    let status = run_rr(
        &["status", "--repo", "owner/repo", "--pr", "42", "--robot"],
        &runtime,
    );
    assert_eq!(status.exit_code, 0, "{}", status.stderr);
    let status_payload = parse_robot_payload(&status.stdout);
    assert_eq!(status_payload["outcome"], "complete");
    assert_eq!(status_payload["data"]["session"]["provider"], "codex");

    let resume = run_rr(&["resume", "--pr", "42"], &runtime);
    assert_eq!(resume.exit_code, 0, "{}", resume.stderr);
    assert!(
        resume.stdout.contains("resume completed"),
        "unexpected resume stdout: {}",
        resume.stdout
    );
    assert!(
        resume.stderr.contains("bounded support")
            && resume
                .stderr
                .contains("does not support locator reopen or rr return"),
        "unexpected resume stderr: {}",
        resume.stderr
    );
}
