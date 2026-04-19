#![cfg(unix)]

use roger_app_core::{ApprovalState, RogerCommandId};
use roger_cli::{run, run_harness_command, CliRuntime, HarnessCommandInvocation};
use roger_storage::{CreateMaterializedFinding, CreateSessionBaselineSnapshot, RogerStore};
use roger_validation::{discover_suite_metadata, failure_artifact_paths};
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use tempfile::{tempdir, TempDir};

fn run_rr(args: &[&str], runtime: &CliRuntime) -> roger_cli::CliRunResult {
    let argv = args
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    run(&argv, runtime)
}

fn run_harness(
    command_id: RogerCommandId,
    runtime: &CliRuntime,
    pr: Option<u64>,
) -> roger_cli::CliRunResult {
    run_harness_command(
        &HarnessCommandInvocation {
            provider: "opencode".to_owned(),
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
fn e2e_harness_dropout_return_preserves_baseline_and_posting_state() {
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
    let review_payload = parse_robot_payload(&review.stdout);
    let session_id = review_payload["data"]["session_id"]
        .as_str()
        .expect("session id")
        .to_owned();
    let run_id = review_payload["data"]["review_run_id"]
        .as_str()
        .expect("review run id")
        .to_owned();

    let store = RogerStore::open(&runtime.store_root).expect("open store");
    store
        .upsert_materialized_finding(CreateMaterializedFinding {
            id: "finding-e2e-06-1",
            session_id: &session_id,
            review_run_id: &run_id,
            stage: "deep_review",
            fingerprint: "fp-e2e-06-1",
            title: "Dropout continuity must preserve approved drafts",
            normalized_summary: "dropout continuity must preserve approved drafts",
            severity: "high",
            confidence: "medium",
            triage_state: "accepted",
            outbound_state: "not_drafted",
        })
        .expect("seed finding");

    let draft = run_rr(
        &[
            "draft",
            "--session",
            &session_id,
            "--all-findings",
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(draft.exit_code, 0, "{}", draft.stderr);
    let draft_payload = parse_robot_payload(&draft.stdout);
    let batch_id = draft_payload["data"]["draft_batch"]["id"]
        .as_str()
        .expect("draft batch id")
        .to_owned();

    let approve = run_rr(
        &[
            "approve",
            "--session",
            &session_id,
            "--batch",
            &batch_id,
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(approve.exit_code, 0, "{}", approve.stderr);
    let approve_payload = parse_robot_payload(&approve.stdout);
    assert_eq!(approve_payload["outcome"], "complete");

    let review_session = store
        .review_session(&session_id)
        .expect("load review session for baseline seeding")
        .expect("review session should exist");
    let allowed_scopes = vec!["repo".to_owned()];
    let policy_epoch_refs = vec!["config:cfg-e2e-06".to_owned()];
    store
        .create_session_baseline_snapshot(CreateSessionBaselineSnapshot {
            id: "baseline-e2e-06-1",
            review_session_id: &session_id,
            review_run_id: Some(&run_id),
            review_target_snapshot: &review_session.review_target,
            allowed_scopes: &allowed_scopes,
            default_query_mode: "recall",
            candidate_visibility_policy: "review_only",
            prompt_strategy: "preset:preset-deep-review/single_turn_report",
            policy_epoch_refs: &policy_epoch_refs,
            degraded_flags: &[],
        })
        .expect("seed baseline snapshot for dropout continuity proof");

    let status_before_dropout = run_rr(&["status", "--session", &session_id, "--robot"], &runtime);
    assert_eq!(
        status_before_dropout.exit_code, 0,
        "{}",
        status_before_dropout.stderr
    );
    let status_before_payload = parse_robot_payload(&status_before_dropout.stdout);
    let approved_before_dropout = status_before_payload["data"]["drafts"]["approved"]
        .as_i64()
        .expect("approved count before dropout");
    assert_eq!(approved_before_dropout, 1);
    assert_eq!(status_before_payload["data"]["drafts"]["posted"], json!(0));
    let baseline_before = store
        .latest_session_baseline_snapshot(&session_id)
        .expect("load baseline before dropout")
        .expect("review should persist baseline snapshot");

    // Simulate intentional dropout to the bare harness surface.
    let session = store
        .review_session(&session_id)
        .expect("load session")
        .expect("session exists");
    let session = store
        .update_review_session_continuity(&session_id, session.row_version, "awaiting_return")
        .expect("set dropout continuity state");
    let session = store
        .update_review_session_attention(&session_id, session.row_version, "awaiting_return")
        .expect("set dropout attention state");
    assert_eq!(session.continuity_state, "awaiting_return");
    assert_eq!(session.attention_state, "awaiting_return");

    let harness_status = run_harness(RogerCommandId::RogerStatus, &runtime, Some(42));
    assert_eq!(harness_status.exit_code, 0, "{}", harness_status.stderr);
    let harness_status_payload = parse_robot_payload(&harness_status.stdout);
    assert_eq!(harness_status_payload["schema_id"], "rr.robot.status.v1");
    assert_eq!(harness_status_payload["data"]["session"]["id"], session_id);
    assert_eq!(
        harness_status_payload["data"]["drafts"]["approved"],
        json!(approved_before_dropout)
    );
    assert_eq!(harness_status_payload["data"]["drafts"]["posted"], json!(0));

    let ret = run_rr(&["return", "--session", &session_id, "--robot"], &runtime);
    assert_eq!(ret.exit_code, 0, "{}", ret.stderr);
    let return_payload = parse_robot_payload(&ret.stdout);
    assert_eq!(return_payload["outcome"], "complete");
    assert_eq!(return_payload["data"]["session_id"], session_id);
    assert_eq!(
        return_payload["data"]["return_path"],
        "rebound_existing_session"
    );

    let status_after_return = run_rr(&["status", "--session", &session_id, "--robot"], &runtime);
    assert_eq!(
        status_after_return.exit_code, 0,
        "{}",
        status_after_return.stderr
    );
    let status_after_payload = parse_robot_payload(&status_after_return.stdout);
    assert_eq!(status_after_payload["data"]["session"]["id"], session_id);
    assert_eq!(status_after_payload["data"]["drafts"]["posted"], json!(0));
    let approved_after_return = status_after_payload["data"]["drafts"]["approved"]
        .as_i64()
        .expect("approved count after return");
    assert!(
        approved_after_return <= approved_before_dropout,
        "return path must not auto-promote posting state"
    );

    let baseline_after = store
        .latest_session_baseline_snapshot(&session_id)
        .expect("load baseline after return")
        .expect("baseline should remain available after return");
    assert_eq!(
        baseline_before.review_target_snapshot,
        baseline_after.review_target_snapshot
    );
    assert_eq!(
        baseline_before.allowed_scopes,
        baseline_after.allowed_scopes
    );
    assert_eq!(
        baseline_before.default_query_mode,
        baseline_after.default_query_mode
    );
    assert_eq!(
        baseline_before.candidate_visibility_policy,
        baseline_after.candidate_visibility_policy
    );
    assert!(
        baseline_after.baseline_generation >= baseline_before.baseline_generation,
        "baseline generation should not regress across dropout/return"
    );

    let batch = store
        .outbound_draft_batch(&batch_id)
        .expect("load draft batch")
        .expect("draft batch exists");
    assert_eq!(batch.approval_state, ApprovalState::Approved);

    let metadata_dir = workspace_root().join("tests/suites");
    let suites = discover_suite_metadata(&metadata_dir).expect("discover suite metadata");
    let suite = suites
        .iter()
        .find(|item| item.id == "e2e_harness_dropout_return")
        .expect("E2E-06 suite metadata");
    assert_eq!(suite.budget_id.as_deref(), Some("E2E-06"));
    assert_eq!(suite.support_tier, "opencode_tier_b");

    let failing_ids = vec!["e2e_harness_dropout_return".to_owned()];
    let failure_paths = failure_artifact_paths(
        &metadata_dir,
        temp.path().join("test-artifacts"),
        &failing_ids,
    )
    .expect("failure artifact paths");
    assert_eq!(failure_paths.len(), 1);
    assert!(
        failure_paths[0]
            .to_string_lossy()
            .contains("failures/e2e_harness_dropout_return/sample_failure"),
        "failure artifact namespace should preserve E2E-06 suite identity"
    );
}
