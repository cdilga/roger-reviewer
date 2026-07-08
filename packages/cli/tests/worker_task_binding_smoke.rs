#![cfg(unix)]

//! Task-binding delivery proof: `rr review` must persist a canonical
//! `worker-task.json` whose `ReviewTask` round-trips through the `rr agent`
//! worker transport — i.e. `rr agent worker.get_review_context --task-file
//! <path>` succeeds against a real launch. This is the seam bead
//! rr-core-loop-worker-binding-py0f exists to close.

use roger_app_core::ReviewTask;
use roger_cli::{CliRuntime, run};
use serde_json::{Value, json};
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

#[test]
fn review_persists_worker_task_file_that_round_trips_through_rr_agent() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary();

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let review = run_rr(
        &["review", "--pr", "42", "--provider", "opencode", "--robot"],
        &runtime,
    );
    assert_eq!(review.exit_code, 0, "{}", review.stderr);
    let review_payload = parse_json(&review.stdout);

    // The launch surfaces the worker-task path additively on the robot envelope.
    let worker_task_path = review_payload["data"]["worker_task_path"]
        .as_str()
        .expect("worker_task_path in review data")
        .to_owned();
    let session_id = review_payload["data"]["session_id"]
        .as_str()
        .expect("session id")
        .to_owned();

    // The canonical binding file exists at <store>/sessions/<id>/worker-task.json.
    let expected_path = runtime
        .store_root
        .join("sessions")
        .join(&session_id)
        .join("worker-task.json");
    assert_eq!(PathBuf::from(&worker_task_path), expected_path);
    assert!(expected_path.is_file(), "worker-task.json must exist");

    // The file deserializes as a ReviewTask bound to the launched session/run
    // with the ten worker operations and a fresh nonce.
    let task: ReviewTask =
        serde_json::from_slice(&fs::read(&expected_path).expect("read worker-task"))
            .expect("worker-task.json is a valid ReviewTask");
    assert_eq!(task.review_session_id, session_id);
    assert!(!task.task_nonce.is_empty());
    assert!(
        task.allowed_operations
            .contains(&"worker.get_review_context".to_owned())
    );
    assert!(
        task.allowed_operations
            .contains(&"worker.submit_stage_result".to_owned())
    );

    // Round-trip: an in-session worker can bind to this file and read context.
    let request_path = temp.path().join("get_context_request.json");
    fs::write(
        &request_path,
        serde_json::to_vec_pretty(&json!({
            "schema_id": "worker_operation_request.v1",
            "review_session_id": task.review_session_id,
            "review_run_id": task.review_run_id,
            "review_task_id": task.id,
            "task_nonce": task.task_nonce,
            "operation": "worker.get_review_context",
            "requested_scopes": ["repo"]
        }))
        .expect("serialize request"),
    )
    .expect("write request");

    let agent = run_rr(
        &[
            "agent",
            "worker.get_review_context",
            "--task-file",
            &worker_task_path,
            "--request-file",
            request_path.to_str().unwrap(),
        ],
        &runtime,
    );
    assert_eq!(agent.exit_code, 0, "{}", agent.stderr);
    let agent_payload = parse_json(&agent.stdout);
    assert_eq!(agent_payload["status"], "succeeded", "{agent_payload}");
    assert_eq!(
        agent_payload["operation_response"]["operation"],
        "worker.get_review_context"
    );

    // The bead's canonical round-trip: the task file ALONE is sufficient for a
    // read operation — `rr agent worker.get_review_context --task-file <path>`
    // with no --request-file. This is the exact call an in-session worker makes
    // under a write-denied policy where it cannot stage a request file.
    let task_only = run_rr(
        &[
            "agent",
            "worker.get_review_context",
            "--task-file",
            &worker_task_path,
        ],
        &runtime,
    );
    assert_eq!(task_only.exit_code, 0, "{}", task_only.stderr);
    let task_only_payload = parse_json(&task_only.stdout);
    assert_eq!(
        task_only_payload["status"], "succeeded",
        "{task_only_payload}"
    );
}
