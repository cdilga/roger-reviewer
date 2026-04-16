#![cfg(unix)]

use roger_app_core::{ApprovalState, ReviewTarget};
use roger_cli::{CliRuntime, run};
use roger_storage::{
    CreateMaterializedFinding, CreateReviewRun, CreateReviewSession, RogerStore, StorageLayout,
};
use rusqlite::{Connection, params};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::{TempDir, tempdir};

fn sample_target(repository: &str, pr_number: u64) -> ReviewTarget {
    ReviewTarget {
        repository: repository.to_owned(),
        pull_request_number: pr_number,
        base_ref: "main".to_owned(),
        head_ref: format!("feature-{pr_number}"),
        base_commit: "aaa".to_owned(),
        head_commit: "bbb".to_owned(),
    }
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

fn run_rr(args: &[&str], runtime: &CliRuntime) -> roger_cli::CliRunResult {
    let argv = args
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<Vec<_>>();
    run(&argv, runtime)
}

fn parse_robot_payload(stdout: &str) -> Value {
    serde_json::from_str(stdout).expect("robot payload json")
}

fn seed_session_with_findings(
    runtime: &CliRuntime,
    session_id: &str,
    run_id: &str,
    target: &ReviewTarget,
    attention_state: &str,
) {
    let store = RogerStore::open(&runtime.store_root).expect("open store");
    store
        .create_review_session(CreateReviewSession {
            id: session_id,
            review_target: target,
            provider: "opencode",
            session_locator: None,
            resume_bundle_artifact_id: None,
            continuity_state: "resume:usable",
            attention_state,
            launch_profile_id: Some("profile-open-pr"),
        })
        .expect("create review session");
    store
        .create_review_run(CreateReviewRun {
            id: run_id,
            session_id,
            run_kind: "deep_review",
            repo_snapshot: "{\"head\":\"bbb\"}",
            continuity_quality: "usable",
            session_locator_artifact_id: None,
        })
        .expect("create review run");

    for (finding_id, title, summary, severity, confidence) in [
        (
            "finding-1",
            "First outbound finding",
            "first draftable finding summary",
            "high",
            "medium",
        ),
        (
            "finding-2",
            "Second outbound finding",
            "second draftable finding summary",
            "medium",
            "high",
        ),
    ] {
        store
            .upsert_materialized_finding(CreateMaterializedFinding {
                id: finding_id,
                session_id,
                review_run_id: run_id,
                stage: "deep_review",
                fingerprint: &format!("fp:{finding_id}"),
                title,
                normalized_summary: summary,
                severity,
                confidence,
                triage_state: "accepted",
                outbound_state: "not_drafted",
            })
            .expect("seed materialized finding");
    }
}

fn draft_batch(runtime: &CliRuntime, session_id: &str) -> Value {
    let draft = run_rr(
        &[
            "draft",
            "--session",
            session_id,
            "--finding",
            "finding-1",
            "--finding",
            "finding-2",
            "--robot",
        ],
        runtime,
    );
    assert_eq!(draft.exit_code, 0, "{}", draft.stderr);
    parse_robot_payload(&draft.stdout)
}

#[test]
fn approve_robot_binds_exact_batch_payload_and_target_tuple() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };
    seed_session_with_findings(
        &runtime,
        "session-1",
        "run-1",
        &sample_target("owner/repo", 42),
        "awaiting_user_input",
    );

    let draft_payload = draft_batch(&runtime, "session-1");
    let batch_id = draft_payload["data"]["draft_batch"]["id"]
        .as_str()
        .expect("draft batch id")
        .to_owned();
    let payload_digest = draft_payload["data"]["draft_batch"]["payload_digest"]
        .as_str()
        .expect("payload digest")
        .to_owned();
    let target_tuple_json = draft_payload["data"]["draft_batch"]["target_tuple_json"]
        .as_str()
        .expect("target tuple json")
        .to_owned();

    let approve = run_rr(
        &[
            "approve",
            "--session",
            "session-1",
            "--batch",
            &batch_id,
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(approve.exit_code, 0, "{}", approve.stderr);
    let payload = parse_robot_payload(&approve.stdout);
    assert_eq!(payload["schema_id"], "rr.robot.approve.v1");
    assert_eq!(payload["outcome"], "complete");
    assert_eq!(payload["data"]["draft_batch"]["id"], batch_id);
    assert_eq!(payload["data"]["draft_batch"]["approval_state"], "approved");
    assert_eq!(
        payload["data"]["draft_batch"]["payload_digest"],
        payload_digest
    );
    assert_eq!(
        payload["data"]["draft_batch"]["target_tuple_json"],
        target_tuple_json
    );
    assert_eq!(
        payload["data"]["approval"]["payload_digest"],
        payload_digest
    );
    assert_eq!(
        payload["data"]["approval"]["target_tuple_json"],
        target_tuple_json
    );
    assert_eq!(
        payload["data"]["approval"]["already_recorded"],
        Value::Bool(false)
    );
    assert_eq!(
        payload["data"]["mutation_guard"]["github_posture"],
        Value::String("blocked".to_owned())
    );
    assert_eq!(
        payload["data"]["queryable_surfaces"]["outbound_state_counts"]["approved"],
        Value::from(2)
    );

    let approval_id = payload["data"]["approval"]["id"]
        .as_str()
        .expect("approval id");
    let approved_at = payload["data"]["approval"]["approved_at"]
        .as_i64()
        .expect("approved at");

    let store = RogerStore::open(&runtime.store_root).expect("reopen store");
    let batch = store
        .outbound_draft_batch(&batch_id)
        .expect("batch lookup")
        .expect("batch should exist");
    assert!(matches!(&batch.approval_state, ApprovalState::Approved));
    assert_eq!(batch.approved_at, Some(approved_at));

    let drafts = store
        .outbound_draft_items_for_batch(&batch_id)
        .expect("draft items lookup");
    assert_eq!(drafts.len(), 2);
    assert!(
        drafts
            .iter()
            .all(|draft| matches!(&draft.approval_state, ApprovalState::Approved))
    );

    let stored_approval = store
        .approval_token_for_batch(&batch_id)
        .expect("approval token lookup")
        .expect("approval token should exist");
    assert_eq!(stored_approval.id, approval_id);
    assert_eq!(stored_approval.payload_digest, payload_digest);
    assert_eq!(stored_approval.target_tuple_json, target_tuple_json);
    assert_eq!(stored_approval.approved_at, approved_at);

    let status = run_rr(&["status", "--session", "session-1", "--robot"], &runtime);
    assert_eq!(status.exit_code, 0, "{}", status.stderr);
    let status_payload = parse_robot_payload(&status.stdout);
    assert_eq!(
        status_payload["data"]["outbound"]["state_counts"]["awaiting_approval"],
        Value::from(0)
    );
    assert_eq!(
        status_payload["data"]["outbound"]["state_counts"]["approved"],
        Value::from(2)
    );
    assert_eq!(
        status_payload["data"]["outbound"]["posting_gate"]["ready_count"],
        Value::from(2)
    );

    let findings = run_rr(&["findings", "--session", "session-1", "--robot"], &runtime);
    assert_eq!(findings.exit_code, 0, "{}", findings.stderr);
    let findings_payload = parse_robot_payload(&findings.stdout);
    let items = findings_payload["data"]["items"]
        .as_array()
        .expect("findings items");
    assert_eq!(items.len(), 2);
    let indexed = items
        .iter()
        .map(|item| {
            (
                item["finding_id"].as_str().expect("finding id").to_owned(),
                item,
            )
        })
        .collect::<HashMap<_, _>>();
    for finding_id in ["finding-1", "finding-2"] {
        let item = indexed.get(finding_id).expect("indexed finding");
        assert_eq!(item["outbound_state"], "approved");
        assert_eq!(
            item["outbound_detail"]["draft_batch_id"],
            Value::String(batch_id.clone())
        );
        assert_eq!(
            item["outbound_detail"]["approval_id"],
            Value::String(approval_id.to_owned())
        );
        assert_eq!(
            item["outbound_detail"]["mutation_elevated"],
            Value::Bool(true)
        );
    }
}

#[test]
fn approve_blocks_invalidated_batch_after_target_drift() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };
    seed_session_with_findings(
        &runtime,
        "session-invalidated",
        "run-invalidated",
        &sample_target("owner/repo", 42),
        "awaiting_user_input",
    );

    let draft_payload = draft_batch(&runtime, "session-invalidated");
    let batch_id = draft_payload["data"]["draft_batch"]["id"]
        .as_str()
        .expect("draft batch id")
        .to_owned();

    let store = RogerStore::open(&runtime.store_root).expect("open store");
    let mut batch = store
        .outbound_draft_batch(&batch_id)
        .expect("batch lookup")
        .expect("batch should exist");
    batch.approval_state = ApprovalState::Invalidated;
    batch.approved_at = Some(1_710_030_000);
    batch.invalidated_at = Some(1_710_030_010);
    batch.invalidation_reason_code = Some("target_rebased".to_owned());
    batch.row_version += 1;
    store
        .store_outbound_draft_batch(&batch)
        .expect("store invalidated batch");

    for mut draft in store
        .outbound_draft_items_for_batch(&batch_id)
        .expect("draft items lookup")
    {
        draft.approval_state = ApprovalState::Invalidated;
        draft.row_version += 1;
        store
            .store_outbound_draft_item(&draft)
            .expect("store invalidated draft");
    }

    let approve = run_rr(
        &[
            "approve",
            "--session",
            "session-invalidated",
            "--batch",
            &batch_id,
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(approve.exit_code, 3, "{}", approve.stderr);
    let payload = parse_robot_payload(&approve.stdout);
    assert_eq!(payload["schema_id"], "rr.robot.approve.v1");
    assert_eq!(payload["outcome"], "blocked");
    assert_eq!(
        payload["data"]["reason_code"],
        Value::String("approval_invalidated:target_rebased".to_owned())
    );
    assert_eq!(
        payload["data"]["invalidation_reason_code"],
        Value::String("target_rebased".to_owned())
    );
}

#[test]
fn approve_blocks_when_stored_payload_or_target_linkage_drifted() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };
    seed_session_with_findings(
        &runtime,
        "session-drifted",
        "run-drifted",
        &sample_target("owner/repo", 42),
        "awaiting_user_input",
    );

    let draft_payload = draft_batch(&runtime, "session-drifted");
    let batch_id = draft_payload["data"]["draft_batch"]["id"]
        .as_str()
        .expect("draft batch id")
        .to_owned();

    let layout = StorageLayout::under(&runtime.store_root);
    let conn = Connection::open(&layout.db_path).expect("open sqlite db");
    conn.execute(
        "UPDATE outbound_draft_items
         SET payload_digest = ?1, remote_review_target_id = ?2
         WHERE draft_batch_id = ?3",
        params!["sha256:tampered-payload", "pr-99", batch_id],
    )
    .expect("tamper canonical draft linkage");

    let approve = run_rr(
        &[
            "approve",
            "--session",
            "session-drifted",
            "--batch",
            &batch_id,
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(approve.exit_code, 3, "{}", approve.stderr);
    let payload = parse_robot_payload(&approve.stdout);
    assert_eq!(payload["schema_id"], "rr.robot.approve.v1");
    assert_eq!(payload["outcome"], "blocked");
    assert_eq!(
        payload["data"]["reason_code"],
        Value::String("approval_invalidated:target_or_payload_drift".to_owned())
    );

    let issues = payload["data"]["validation_issues"]
        .as_array()
        .expect("validation issues");
    assert!(
        issues
            .iter()
            .any(|issue| issue["reason_code"] == "target_mismatch")
    );
    assert!(
        issues
            .iter()
            .any(|issue| issue["reason_code"] == "payload_digest_mismatch")
    );
}

#[test]
fn robot_docs_advertise_rr_approve_surface() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };

    let commands = run_rr(&["robot-docs", "commands", "--robot"], &runtime);
    assert_eq!(commands.exit_code, 0, "{}", commands.stderr);
    let commands_payload = parse_robot_payload(&commands.stdout);
    let command_items = commands_payload["data"]["items"]
        .as_array()
        .expect("command items");
    assert!(
        command_items
            .iter()
            .any(|item| item["command"] == "rr approve"
                && item["required_formats"] == serde_json::json!(["json"]))
    );

    let schemas = run_rr(&["robot-docs", "schemas", "--robot"], &runtime);
    assert_eq!(schemas.exit_code, 0, "{}", schemas.stderr);
    let schema_items = parse_robot_payload(&schemas.stdout)["data"]["items"]
        .as_array()
        .expect("schema items")
        .clone();
    assert!(
        schema_items
            .iter()
            .any(|item| item["command"] == "rr approve"
                && item["schema_id"] == "rr.robot.approve.v1")
    );

    let workflows = run_rr(&["robot-docs", "workflows", "--robot"], &runtime);
    assert_eq!(workflows.exit_code, 0, "{}", workflows.stderr);
    let workflow_items = parse_robot_payload(&workflows.stdout)["data"]["items"]
        .as_array()
        .expect("workflow items")
        .clone();
    let workflow = workflow_items
        .iter()
        .find(|item| item["name"] == "local_outbound_approve")
        .expect("local outbound approve workflow");
    let steps = workflow["steps"].as_array().expect("workflow steps");
    assert!(
        steps
            .iter()
            .any(|step| step == "rr approve --session <id> --batch <draft-batch-id> --robot")
    );
}
