#![cfg(unix)]

use roger_app_core::{ApprovalState, ReviewTarget};
use roger_cli::{CliRuntime, run};
use roger_storage::{CreateMaterializedFinding, CreateReviewRun, CreateReviewSession, RogerStore};
use serde_json::Value;
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
        .map(|value| value.to_string())
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
    store
        .upsert_materialized_finding(CreateMaterializedFinding {
            id: "finding-1",
            session_id,
            review_run_id: run_id,
            stage: "deep_review",
            fingerprint: "fp:draft-one",
            title: "First outbound finding",
            normalized_summary: "first draftable finding summary",
            severity: "high",
            confidence: "medium",
            triage_state: "accepted",
            outbound_state: "not_drafted",
        })
        .expect("seed finding one");
    store
        .upsert_materialized_finding(CreateMaterializedFinding {
            id: "finding-2",
            session_id,
            review_run_id: run_id,
            stage: "deep_review",
            fingerprint: "fp:draft-two",
            title: "Second outbound finding",
            normalized_summary: "second draftable finding summary",
            severity: "medium",
            confidence: "high",
            triage_state: "accepted",
            outbound_state: "not_drafted",
        })
        .expect("seed finding two");
}

#[test]
fn draft_robot_materializes_grouped_batch_and_status_findings_surface_it() {
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

    let draft = run_rr(
        &[
            "draft",
            "--session",
            "session-1",
            "--finding",
            "finding-1",
            "--finding",
            "finding-2",
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(draft.exit_code, 0, "{}", draft.stderr);
    let payload = parse_robot_payload(&draft.stdout);
    assert_eq!(payload["schema_id"], "rr.robot.draft.v1");
    assert_eq!(payload["outcome"], "complete");
    assert_eq!(payload["data"]["selection"]["grouped"], Value::Bool(true));
    assert_eq!(payload["data"]["selection"]["count"], Value::from(2));
    assert_eq!(
        payload["data"]["target"]["remote_review_target_id"],
        Value::String("pr-42".to_owned())
    );
    assert_eq!(
        payload["data"]["mutation_guard"]["github_posture"],
        Value::String("blocked".to_owned())
    );
    let target_tuple: Value = serde_json::from_str(
        payload["data"]["draft_batch"]["target_tuple_json"]
            .as_str()
            .expect("target tuple json"),
    )
    .expect("decode target tuple");
    assert_eq!(
        target_tuple["review_session_id"],
        Value::String("session-1".to_owned())
    );

    let batch_id = payload["data"]["draft_batch"]["id"]
        .as_str()
        .expect("batch id")
        .to_owned();

    let store = RogerStore::open(&runtime.store_root).expect("reopen store");
    let batch = store
        .outbound_draft_batch(&batch_id)
        .expect("batch lookup")
        .expect("batch should exist");
    assert_eq!(batch.approval_state, ApprovalState::Drafted);
    let drafts = store
        .outbound_draft_items_for_batch(&batch_id)
        .expect("draft items lookup");
    assert_eq!(drafts.len(), 2);
    assert!(
        drafts
            .iter()
            .all(|draft| draft.payload_digest == batch.payload_digest)
    );
    assert!(
        drafts
            .iter()
            .all(|draft| draft.target_locator.contains("github:owner/repo#42"))
    );
    assert!(
        drafts
            .iter()
            .any(|draft| draft.body.contains("first draftable finding summary"))
    );

    let status = run_rr(&["status", "--session", "session-1", "--robot"], &runtime);
    assert_eq!(status.exit_code, 0, "{}", status.stderr);
    let status_payload = parse_robot_payload(&status.stdout);
    assert_eq!(
        status_payload["data"]["outbound"]["state_counts"]["awaiting_approval"],
        Value::from(2)
    );
    assert_eq!(
        status_payload["data"]["outbound"]["posting_gate"]["ready_count"],
        Value::from(0)
    );

    let findings = run_rr(&["findings", "--session", "session-1", "--robot"], &runtime);
    assert_eq!(findings.exit_code, 0, "{}", findings.stderr);
    let findings_payload = parse_robot_payload(&findings.stdout);
    let items = findings_payload["data"]["items"]
        .as_array()
        .expect("findings items");
    assert_eq!(items.len(), 2);
    assert!(
        items
            .iter()
            .all(|item| item["outbound_state"] == "awaiting_approval")
    );
    assert!(items.iter().all(|item| {
        item["outbound_detail"]["draft_batch_id"] == Value::String(batch_id.clone())
    }));
}

#[test]
fn draft_blocks_when_review_target_is_missing() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };
    seed_session_with_findings(
        &runtime,
        "session-missing-target",
        "run-missing-target",
        &sample_target("", 0),
        "awaiting_user_input",
    );

    let draft = run_rr(
        &[
            "draft",
            "--session",
            "session-missing-target",
            "--finding",
            "finding-1",
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(draft.exit_code, 3, "{}", draft.stderr);
    let payload = parse_robot_payload(&draft.stdout);
    assert_eq!(payload["outcome"], "blocked");
    assert_eq!(payload["data"]["reason_code"], "missing_review_target");
}

#[test]
fn draft_blocks_when_attention_state_requires_reconciliation() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };
    seed_session_with_findings(
        &runtime,
        "session-stale",
        "run-stale",
        &sample_target("owner/repo", 42),
        "refresh_recommended",
    );

    let draft = run_rr(
        &[
            "draft",
            "--session",
            "session-stale",
            "--finding",
            "finding-1",
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(draft.exit_code, 3, "{}", draft.stderr);
    let payload = parse_robot_payload(&draft.stdout);
    assert_eq!(payload["outcome"], "blocked");
    assert_eq!(payload["data"]["reason_code"], "stale_local_state");
    assert_eq!(
        payload["data"]["reconciliation"]["stale_target_detected"],
        Value::Bool(true)
    );
}
