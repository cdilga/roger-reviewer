#![cfg(unix)]

use roger_app_core::ApprovalState;
use roger_cli::{CliRuntime, run};
use roger_storage::{CreateMaterializedFinding, CreateReviewRun, RogerStore};
use roger_validation::{discover_suite_metadata, failure_artifact_paths};
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::Duration;
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
fn e2e_refresh_draft_reconciliation_handles_reconfirmation_after_drift() {
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
    let run_one_id = review_payload["data"]["review_run_id"]
        .as_str()
        .expect("review run id")
        .to_owned();

    let store = RogerStore::open(&runtime.store_root).expect("open store");
    store
        .upsert_materialized_finding(CreateMaterializedFinding {
            id: "finding-carried",
            session_id: &session_id,
            review_run_id: &run_one_id,
            stage: "deep_review",
            fingerprint: "fp-carried",
            title: "Carry this finding forward",
            normalized_summary: "carry this finding forward",
            severity: "medium",
            confidence: "high",
            triage_state: "accepted",
            outbound_state: "not_drafted",
        })
        .expect("seed carried finding in run one");
    store
        .upsert_materialized_finding(CreateMaterializedFinding {
            id: "finding-resolved",
            session_id: &session_id,
            review_run_id: &run_one_id,
            stage: "deep_review",
            fingerprint: "fp-resolved",
            title: "Resolve this finding in the next run",
            normalized_summary: "resolve this finding in the next run",
            severity: "low",
            confidence: "medium",
            triage_state: "accepted",
            outbound_state: "not_drafted",
        })
        .expect("seed resolved finding in run one");

    let draft_one = run_rr(
        &[
            "draft",
            "--session",
            &session_id,
            "--finding",
            "finding-resolved",
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(draft_one.exit_code, 0, "{}", draft_one.stderr);
    let draft_one_payload = parse_robot_payload(&draft_one.stdout);
    assert_eq!(draft_one_payload["outcome"], "complete");
    assert_eq!(
        draft_one_payload["data"]["selection"]["count"],
        Value::from(1)
    );
    let batch_one_id = draft_one_payload["data"]["draft_batch"]["id"]
        .as_str()
        .expect("first batch id")
        .to_owned();

    let approve_one = run_rr(
        &[
            "approve",
            "--session",
            &session_id,
            "--batch",
            &batch_one_id,
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(approve_one.exit_code, 0, "{}", approve_one.stderr);
    let approve_one_payload = parse_robot_payload(&approve_one.stdout);
    assert_eq!(approve_one_payload["outcome"], "complete");

    let session = store
        .review_session(&session_id)
        .expect("load review session")
        .expect("session should exist");
    let stale_session = store
        .update_review_session_attention(&session_id, session.row_version, "refresh_recommended")
        .expect("set stale attention state");

    let stale_draft = run_rr(
        &[
            "draft",
            "--session",
            &session_id,
            "--finding",
            "finding-carried",
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(stale_draft.exit_code, 3, "{}", stale_draft.stderr);
    let stale_draft_payload = parse_robot_payload(&stale_draft.stdout);
    assert_eq!(stale_draft_payload["outcome"], "blocked");
    assert_eq!(
        stale_draft_payload["data"]["reason_code"],
        "stale_local_state"
    );
    assert_eq!(
        stale_draft_payload["data"]["reconciliation"]["stale_target_detected"],
        Value::Bool(true)
    );

    let run_two_id = "run-e2e-04-refresh-2";
    store
        .create_review_run(CreateReviewRun {
            id: run_two_id,
            session_id: &session_id,
            run_kind: "review",
            repo_snapshot: "{\"head\":\"def789\"}",
            continuity_quality: "usable",
            session_locator_artifact_id: None,
        })
        .expect("create refreshed run");

    let reconciled_session = store
        .update_review_session_attention(
            &session_id,
            stale_session.row_version,
            "awaiting_user_input",
        )
        .expect("set reconciled attention state");
    assert_eq!(reconciled_session.attention_state, "awaiting_user_input");

    // Finding decision event ids are second-based; ensure refreshed upserts
    // generate distinct event ids instead of colliding in the same second.
    thread::sleep(Duration::from_secs(1));

    let carried_after_refresh = store
        .upsert_materialized_finding(CreateMaterializedFinding {
            id: "finding-carried",
            session_id: &session_id,
            review_run_id: run_two_id,
            stage: "deep_review",
            fingerprint: "fp-carried",
            title: "Carry this finding forward (refreshed)",
            normalized_summary: "carry this finding forward refreshed",
            severity: "medium",
            confidence: "high",
            triage_state: "accepted",
            outbound_state: "not_drafted",
        })
        .expect("carry finding into refreshed run");
    let new_after_refresh = store
        .upsert_materialized_finding(CreateMaterializedFinding {
            id: "finding-new",
            session_id: &session_id,
            review_run_id: run_two_id,
            stage: "deep_review",
            fingerprint: "fp-new",
            title: "New finding after target drift",
            normalized_summary: "new finding after target drift",
            severity: "high",
            confidence: "medium",
            triage_state: "accepted",
            outbound_state: "not_drafted",
        })
        .expect("insert new finding in refreshed run");

    assert_eq!(carried_after_refresh.first_run_id, run_one_id);
    assert_eq!(
        carried_after_refresh.last_seen_run_id.as_deref(),
        Some(run_two_id)
    );
    assert_eq!(new_after_refresh.first_run_id, run_two_id);
    assert_eq!(
        new_after_refresh.last_seen_run_id.as_deref(),
        Some(run_two_id)
    );

    let run_two_findings = store
        .materialized_findings_for_run(&session_id, run_two_id)
        .expect("findings for refreshed run");
    let run_two_ids = run_two_findings
        .iter()
        .map(|finding| finding.id.as_str())
        .collect::<HashSet<_>>();
    assert_eq!(run_two_ids.len(), 2);
    assert!(run_two_ids.contains("finding-carried"));
    assert!(run_two_ids.contains("finding-new"));

    let run_one_findings_after_refresh = store
        .materialized_findings_for_run(&session_id, &run_one_id)
        .expect("findings for original run after refresh");
    let run_one_ids_after_refresh = run_one_findings_after_refresh
        .iter()
        .map(|finding| finding.id.as_str())
        .collect::<HashSet<_>>();
    assert_eq!(run_one_ids_after_refresh.len(), 1);
    assert!(run_one_ids_after_refresh.contains("finding-resolved"));

    let post_old_batch = run_rr(
        &[
            "post",
            "--session",
            &session_id,
            "--batch",
            &batch_one_id,
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(post_old_batch.exit_code, 3, "{}", post_old_batch.stderr);
    let post_old_batch_payload = parse_robot_payload(&post_old_batch.stdout);
    assert_eq!(post_old_batch_payload["outcome"], "blocked");
    assert!(
        post_old_batch_payload["data"]["reason_code"]
            .as_str()
            .expect("post reason code")
            .starts_with("approval_invalidated:")
    );
    assert_eq!(
        post_old_batch_payload["data"]["latest_review_run_id"],
        Value::String(run_two_id.to_owned())
    );
    assert_eq!(
        post_old_batch_payload["data"]["batch_review_run_id"],
        Value::String(run_one_id.clone())
    );

    let post_block_session = store
        .review_session(&session_id)
        .expect("reload session after stale post block")
        .expect("session should still exist after stale post block");
    if post_block_session.attention_state == "refresh_recommended" {
        store
            .update_review_session_attention(
                &session_id,
                post_block_session.row_version,
                "awaiting_user_input",
            )
            .expect("clear stale attention after explicit stale-post reconciliation");
    }

    let draft_two = run_rr(
        &[
            "draft",
            "--session",
            &session_id,
            "--all-findings",
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(
        draft_two.exit_code, 0,
        "stderr: {}\nstdout: {}",
        draft_two.stderr, draft_two.stdout
    );
    let draft_two_payload = parse_robot_payload(&draft_two.stdout);
    assert_eq!(draft_two_payload["outcome"], "complete");
    assert_eq!(
        draft_two_payload["data"]["selection"]["count"],
        Value::from(2)
    );
    let batch_two_id = draft_two_payload["data"]["draft_batch"]["id"]
        .as_str()
        .expect("second batch id")
        .to_owned();

    let approve_two = run_rr(
        &[
            "approve",
            "--session",
            &session_id,
            "--batch",
            &batch_two_id,
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(approve_two.exit_code, 0, "{}", approve_two.stderr);
    let approve_two_payload = parse_robot_payload(&approve_two.stdout);
    assert_eq!(approve_two_payload["outcome"], "complete");

    let batch_one = store
        .outbound_draft_batch(&batch_one_id)
        .expect("load first batch")
        .expect("first batch exists");
    assert_eq!(batch_one.review_run_id, run_one_id);
    assert_eq!(batch_one.approval_state, ApprovalState::Approved);
    assert!(
        store
            .approval_token_for_batch(&batch_one_id)
            .expect("load first approval token")
            .is_some(),
        "first approval token should remain for audit history"
    );

    let batch_two = store
        .outbound_draft_batch(&batch_two_id)
        .expect("load second batch")
        .expect("second batch exists");
    assert_eq!(batch_two.review_run_id, run_two_id);
    assert_eq!(batch_two.approval_state, ApprovalState::Approved);
    assert!(
        store
            .approval_token_for_batch(&batch_two_id)
            .expect("load second approval token")
            .is_some(),
        "second approval token should exist after reconfirmation"
    );

    let findings = run_rr(&["findings", "--session", &session_id, "--robot"], &runtime);
    assert_eq!(findings.exit_code, 0, "{}", findings.stderr);
    let findings_payload = parse_robot_payload(&findings.stdout);
    let finding_ids = findings_payload["data"]["items"]
        .as_array()
        .expect("findings items")
        .iter()
        .map(|item| item["finding_id"].as_str().expect("finding id").to_owned())
        .collect::<HashSet<_>>();
    assert_eq!(finding_ids.len(), 2);
    assert!(finding_ids.contains("finding-carried"));
    assert!(finding_ids.contains("finding-new"));

    let metadata_dir = workspace_root().join("tests/suites");
    let suites = discover_suite_metadata(&metadata_dir).expect("discover suite metadata");
    let suite = suites
        .iter()
        .find(|item| item.id == "e2e_refresh_draft_reconciliation")
        .expect("E2E-04 suite metadata");
    assert_eq!(suite.budget_id.as_deref(), Some("E2E-04"));
    assert_eq!(suite.support_tier, "opencode_tier_b");
    assert_eq!(
        suite.fixture_families,
        vec![
            "fixture_refresh_rebase_target_drift".to_owned(),
            "fixture_github_draft_batch".to_owned(),
        ]
    );

    let failing_ids = vec!["e2e_refresh_draft_reconciliation".to_owned()];
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
            .contains("failures/e2e_refresh_draft_reconciliation/sample_failure"),
        "failure artifact namespace should preserve E2E-04 suite identity"
    );
}
