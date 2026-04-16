#![cfg(unix)]

use roger_app_core::{ApprovalState, ReviewTarget};
use roger_cli::{CliRuntime, run};
use roger_storage::{CreateMaterializedFinding, CreateReviewRun, CreateReviewSession, RogerStore};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use tempfile::{TempDir, tempdir};

fn sample_target(pr_number: u64) -> ReviewTarget {
    ReviewTarget {
        repository: "owner/repo".to_owned(),
        pull_request_number: pr_number,
        base_ref: "main".to_owned(),
        head_ref: format!("feature-{pr_number}"),
        base_commit: "aaa".to_owned(),
        head_commit: "bbb".to_owned(),
    }
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

fn seed_session(
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
            continuity_state: "review_launched",
            attention_state,
            launch_profile_id: Some("profile-open-pr"),
        })
        .expect("create review session");
    store
        .create_review_run(CreateReviewRun {
            id: run_id,
            session_id,
            run_kind: "review",
            repo_snapshot: "{\"head\":\"bbb\"}",
            continuity_quality: "usable",
            session_locator_artifact_id: None,
        })
        .expect("create review run");
}

fn seed_finding(
    runtime: &CliRuntime,
    session_id: &str,
    run_id: &str,
    finding_id: &str,
    title: &str,
    triage_state: &str,
    outbound_state: &str,
) {
    let store = RogerStore::open(&runtime.store_root).expect("open store");
    store
        .upsert_materialized_finding(CreateMaterializedFinding {
            id: finding_id,
            session_id,
            review_run_id: run_id,
            stage: "deep_review",
            fingerprint: &format!("fp:{finding_id}"),
            title,
            normalized_summary: &format!("{title} summary"),
            severity: "medium",
            confidence: "medium",
            triage_state,
            outbound_state,
        })
        .expect("seed materialized finding");
}

#[test]
fn draft_robot_materializes_grouped_local_batch_and_queryable_outbound_state() {
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
    let review_run_id = review_payload["data"]["review_run_id"]
        .as_str()
        .expect("review run id")
        .to_owned();

    seed_finding(
        &runtime,
        &session_id,
        &review_run_id,
        "finding-draft-1",
        "First draftable finding",
        "accepted",
        "not_drafted",
    );
    seed_finding(
        &runtime,
        &session_id,
        &review_run_id,
        "finding-draft-2",
        "Second draftable finding",
        "accepted",
        "not_drafted",
    );

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
    assert_eq!(draft_payload["schema_id"], "rr.robot.draft.v1");
    assert_eq!(draft_payload["outcome"], "complete");
    assert_eq!(draft_payload["data"]["selection"]["mode"], "all_findings");
    assert_eq!(draft_payload["data"]["selection"]["grouped"], true);
    assert_eq!(draft_payload["data"]["selection"]["count"], 2);
    assert_eq!(draft_payload["data"]["target"]["provider"], "github");
    assert_eq!(draft_payload["data"]["target"]["repository"], "owner/repo");
    assert_eq!(draft_payload["data"]["target"]["pull_request"], 42);
    assert_eq!(
        draft_payload["data"]["draft_batch"]["approval_state"],
        "drafted"
    );
    assert_eq!(draft_payload["data"]["draft_batch"]["draft_count"], 2);
    assert_eq!(
        draft_payload["data"]["queryable_surfaces"]["outbound_state_counts"]["awaiting_approval"],
        2
    );
    assert_eq!(
        draft_payload["data"]["mutation_guard"]["approval_required"],
        true
    );
    assert_eq!(draft_payload["data"]["mutation_guard"]["posted"], false);
    assert_eq!(
        draft_payload["data"]["mutation_guard"]["github_posture"],
        "blocked"
    );
    assert_eq!(
        draft_payload["data"]["provider_capability"]["tier"],
        "tier_b"
    );
    assert_eq!(
        draft_payload["data"]["routine_surface"]["provider"]["status"],
        "first_class_live"
    );

    let target_tuple_json = draft_payload["data"]["draft_batch"]["target_tuple_json"]
        .as_str()
        .expect("target tuple json");
    let target_tuple: Value =
        serde_json::from_str(target_tuple_json).expect("parse target tuple json");
    assert_eq!(target_tuple["review_session_id"], session_id);
    assert_eq!(target_tuple["repo_id"], "owner/repo");
    assert_eq!(target_tuple["remote_review_target_id"], "pr-42");

    let batch_id = draft_payload["data"]["draft_batch"]["id"]
        .as_str()
        .expect("draft batch id");
    let store = RogerStore::open(&runtime.store_root).expect("open store");
    let stored_batch = store
        .outbound_draft_batch(batch_id)
        .expect("load stored draft batch")
        .expect("stored batch");
    assert_eq!(stored_batch.review_session_id, session_id);
    assert_eq!(stored_batch.review_run_id, review_run_id);
    assert_eq!(stored_batch.repo_id, "owner/repo");
    assert_eq!(stored_batch.remote_review_target_id, "pr-42");
    assert_eq!(stored_batch.approval_state, ApprovalState::Drafted);

    let stored_drafts = store
        .outbound_draft_items_for_batch(batch_id)
        .expect("load stored drafts");
    assert_eq!(stored_drafts.len(), 2);
    let stored_by_finding = stored_drafts
        .iter()
        .map(|draft| {
            (
                draft.finding_id.as_ref().expect("draft finding id").clone(),
                draft,
            )
        })
        .collect::<HashMap<_, _>>();

    for (finding_id, title) in [
        ("finding-draft-1", "First draftable finding"),
        ("finding-draft-2", "Second draftable finding"),
    ] {
        let stored = stored_by_finding
            .get(finding_id)
            .expect("stored draft by finding");
        assert_eq!(stored.approval_state, ApprovalState::Drafted);
        assert_eq!(stored.draft_batch_id, batch_id);
        assert_eq!(stored.repo_id, "owner/repo");
        assert_eq!(stored.remote_review_target_id, "pr-42");
        assert_eq!(
            stored.target_locator,
            format!("github:owner/repo#42:finding/{finding_id}")
        );
        assert!(
            stored.body.contains(title),
            "draft body should include title"
        );
        assert!(
            stored
                .body
                .contains(&format!("Fingerprint: fp:{finding_id}")),
            "draft body should include fingerprint"
        );
    }

    let status = run_rr(&["status", "--session", &session_id, "--robot"], &runtime);
    assert_eq!(status.exit_code, 0, "{}", status.stderr);
    let status_payload = parse_robot_payload(&status.stdout);
    assert_eq!(
        status_payload["data"]["outbound"]["state_counts"]["awaiting_approval"],
        2
    );
    assert_eq!(
        status_payload["data"]["outbound"]["state_counts"]["approved"],
        0
    );
    assert_eq!(
        status_payload["data"]["outbound"]["posting_gate"]["ready_count"],
        0
    );

    let findings = run_rr(&["findings", "--session", &session_id, "--robot"], &runtime);
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
    for finding_id in ["finding-draft-1", "finding-draft-2"] {
        let item = indexed.get(finding_id).expect("findings projection");
        assert_eq!(item["outbound_state"], "awaiting_approval");
        assert_eq!(item["outbound_detail"]["source"], "canonical_batch");
        assert_eq!(item["outbound_detail"]["draft_batch_id"], batch_id);
        assert_eq!(item["outbound_detail"]["mutation_elevated"], false);
    }
}

#[test]
fn draft_blocks_when_review_target_is_missing() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary();

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let missing_target = ReviewTarget {
        repository: String::new(),
        pull_request_number: 0,
        base_ref: "main".to_owned(),
        head_ref: "feature-missing-target".to_owned(),
        base_commit: "aaa".to_owned(),
        head_commit: "bbb".to_owned(),
    };
    seed_session(
        &runtime,
        "session-missing-target",
        "run-missing-target",
        &missing_target,
        "awaiting_user_input",
    );
    seed_finding(
        &runtime,
        "session-missing-target",
        "run-missing-target",
        "finding-missing-target",
        "Missing target finding",
        "accepted",
        "not_drafted",
    );

    let draft = run_rr(
        &[
            "draft",
            "--session",
            "session-missing-target",
            "--finding",
            "finding-missing-target",
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(draft.exit_code, 3, "{}", draft.stderr);
    let payload = parse_robot_payload(&draft.stdout);
    assert_eq!(payload["schema_id"], "rr.robot.draft.v1");
    assert_eq!(payload["outcome"], "blocked");
    assert_eq!(payload["data"]["reason_code"], "missing_review_target");
    assert_eq!(payload["data"]["session_id"], "session-missing-target");
    assert_eq!(payload["data"]["review_target"]["repository"], "");
    assert_eq!(payload["data"]["review_target"]["pull_request_number"], 0);
}

#[test]
fn draft_blocks_when_selected_findings_are_no_longer_draftable() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary();

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let target = sample_target(77);
    seed_session(
        &runtime,
        "session-stale-draft",
        "run-stale-draft",
        &target,
        "awaiting_user_input",
    );
    seed_finding(
        &runtime,
        "session-stale-draft",
        "run-stale-draft",
        "finding-stale-draft",
        "Already drafted finding",
        "accepted",
        "drafted",
    );

    let draft = run_rr(
        &[
            "draft",
            "--session",
            "session-stale-draft",
            "--finding",
            "finding-stale-draft",
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(draft.exit_code, 3, "{}", draft.stderr);
    let payload = parse_robot_payload(&draft.stdout);
    assert_eq!(payload["schema_id"], "rr.robot.draft.v1");
    assert_eq!(payload["outcome"], "blocked");
    assert_eq!(payload["data"]["reason_code"], "stale_local_state");
    let issues = payload["data"]["selection_issues"]
        .as_array()
        .expect("selection issues");
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0]["finding_id"], "finding-stale-draft");
    assert_eq!(issues[0]["reason_code"], "existing_outbound_state");
    assert_eq!(issues[0]["current_outbound_state"], "awaiting_approval");
}
