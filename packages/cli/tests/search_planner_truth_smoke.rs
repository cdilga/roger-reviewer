#![cfg(unix)]

use roger_app_core::ReviewTarget;
use roger_cli::{CliRuntime, run};
use roger_storage::{
    CreateMaterializedFinding, CreateReviewRun, CreateReviewSession, RogerStore, UpsertMemoryItem,
};
use serde_json::Value;
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

fn seed_prior_review_lookup_records(
    store: &RogerStore,
    session_id: &str,
    review_run_id: &str,
    repository: &str,
) {
    let scope_key = format!("repo:{repository}");
    store
        .upsert_materialized_finding(CreateMaterializedFinding {
            id: "finding-search-1",
            session_id,
            review_run_id,
            stage: "deep_review",
            fingerprint: "fp:approval-refresh",
            title: "Approval token survives stale refresh",
            normalized_summary: "approval token stale refresh should gate posting",
            severity: "high",
            confidence: "high",
            triage_state: "accepted",
            outbound_state: "drafted",
        })
        .expect("seed materialized finding");
    store
        .upsert_memory_item(UpsertMemoryItem {
            id: "memory-promoted-1",
            scope_key: &scope_key,
            memory_class: "procedural",
            state: "proven",
            statement: "approval refresh should reconfirm posting safety",
            normalized_key: "approval refresh reconfirm posting safety",
            anchor_digest: Some("anchor:approval-refresh"),
            source_kind: "manual",
        })
        .expect("seed promoted memory");
    store
        .upsert_memory_item(UpsertMemoryItem {
            id: "memory-candidate-1",
            scope_key: &scope_key,
            memory_class: "semantic",
            state: "candidate",
            statement: "approval token stale refresh might need operator triage",
            normalized_key: "approval token stale refresh operator triage",
            anchor_digest: None,
            source_kind: "manual",
        })
        .expect("seed candidate memory");
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
fn search_robot_surfaces_explicit_recovery_scan_for_degraded_runs() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary();

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let search = run_rr(&["search", "--query", "stale draft", "--robot"], &runtime);
    assert_eq!(search.exit_code, 5, "{}", search.stderr);
    let payload = parse_robot_payload(&search.stdout);
    assert_eq!(payload["schema_id"], "rr.robot.search.v1");
    assert_eq!(payload["outcome"], "degraded");
    assert_eq!(payload["data"]["requested_query_mode"], "auto");
    assert_eq!(payload["data"]["resolved_query_mode"], "recall");
    assert_eq!(payload["data"]["retrieval_mode"], "recovery_scan");
    assert_eq!(
        payload["data"]["search_plan"]["query_plan"]["strategy"]["primary_lane"],
        "lexical_recall"
    );
    assert_eq!(
        payload["data"]["search_plan"]["query_plan"]["candidate_visibility"],
        "hidden"
    );
    assert_eq!(
        payload["data"]["search_plan"]["query_plan"]["trust_floor"],
        "promoted_and_evidence_only"
    );
    assert_eq!(
        payload["data"]["search_plan"]["scope_keys"],
        serde_json::json!(["repo:owner/repo"])
    );
    assert_eq!(
        payload["data"]["search_plan"]["retrieval_classes"],
        serde_json::json!(["promoted_memory", "evidence_hits"])
    );
    assert_eq!(
        payload["data"]["search_plan"]["semantic_runtime_posture"],
        "disabled_pending_verification"
    );
    assert_eq!(
        payload["data"]["search_plan"]["retrieval_strategy"]["lexical"],
        true
    );
    assert_eq!(
        payload["data"]["search_plan"]["retrieval_strategy"]["prior_review"],
        true
    );
    assert_eq!(
        payload["data"]["search_plan"]["retrieval_strategy"]["semantic"],
        false
    );
    assert!(
        payload["data"]["search_plan"]["strategy_reason"]
            .as_str()
            .expect("search plan strategy reason")
            .contains("free-text recall")
    );
    assert!(
        payload["data"]["degraded_reasons"]
            .as_array()
            .expect("degraded reasons")
            .iter()
            .any(|reason| reason
                .as_str()
                .expect("degraded reason")
                .contains("lexical"))
    );
}

#[test]
fn search_robot_keeps_requested_vs_resolved_planner_truth() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary();

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let exact = run_rr(
        &["search", "--query", "packages/cli/src/lib.rs", "--robot"],
        &runtime,
    );
    assert_eq!(exact.exit_code, 5, "{}", exact.stderr);
    let exact_payload = parse_robot_payload(&exact.stdout);
    assert_eq!(exact_payload["data"]["requested_query_mode"], "auto");
    assert_eq!(exact_payload["data"]["resolved_query_mode"], "exact_lookup");
    assert_eq!(exact_payload["data"]["retrieval_mode"], "recovery_scan");
    assert_eq!(
        exact_payload["data"]["search_plan"]["query_plan"]["strategy"]["primary_lane"],
        "exact_lookup"
    );
    assert_eq!(
        exact_payload["data"]["search_plan"]["query_plan"]["candidate_visibility"],
        "hidden"
    );
    assert_eq!(
        exact_payload["data"]["search_plan"]["retrieval_classes"],
        serde_json::json!(["promoted_memory", "evidence_hits"])
    );
    assert_eq!(
        exact_payload["data"]["search_plan"]["semantic_runtime_posture"],
        "disabled_by_query_mode"
    );
    assert_eq!(
        exact_payload["data"]["search_plan"]["retrieval_strategy"]["lexical"],
        true
    );
    assert_eq!(
        exact_payload["data"]["search_plan"]["retrieval_strategy"]["prior_review"],
        false
    );
    assert_eq!(
        exact_payload["data"]["search_plan"]["retrieval_strategy"]["semantic"],
        false
    );
    assert!(
        exact_payload["data"]["search_plan"]["strategy_reason"]
            .as_str()
            .expect("exact lookup strategy reason")
            .contains("exact lookup")
    );

    let blocked = run_rr(
        &[
            "search",
            "--query",
            "stale draft",
            "--query-mode",
            "related_context",
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(blocked.exit_code, 3, "{}", blocked.stderr);
    let blocked_payload = parse_robot_payload(&blocked.stdout);
    assert_eq!(blocked_payload["outcome"], "blocked");
    assert_eq!(
        blocked_payload["data"]["requested_query_mode"],
        "related_context"
    );
    assert_eq!(
        blocked_payload["data"]["reason_code"],
        "query_mode_requires_anchor_hints"
    );
}

#[test]
fn search_robot_preserves_candidate_and_promoted_recall_truth() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary();

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let target = sample_target(42);
    let store = RogerStore::open(&runtime.store_root).expect("open store");
    store
        .create_review_session(CreateReviewSession {
            id: "session-search-1",
            review_target: &target,
            provider: "opencode",
            session_locator: None,
            resume_bundle_artifact_id: None,
            continuity_state: "awaiting_resume",
            attention_state: "awaiting_user_input",
            launch_profile_id: Some("profile-open-pr"),
        })
        .expect("create review session");
    store
        .create_review_run(CreateReviewRun {
            id: "run-search-1",
            session_id: "session-search-1",
            run_kind: "deep_review",
            repo_snapshot: "{\"head\":\"bbb\"}",
            continuity_quality: "usable",
            session_locator_artifact_id: None,
        })
        .expect("create review run");
    seed_prior_review_lookup_records(&store, "session-search-1", "run-search-1", "owner/repo");

    let search = run_rr(
        &[
            "search",
            "--query",
            "approval refresh",
            "--query-mode",
            "candidate_audit",
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(search.exit_code, 5, "{}", search.stderr);
    let payload = parse_robot_payload(&search.stdout);
    assert_eq!(payload["data"]["requested_query_mode"], "candidate_audit");
    assert_eq!(payload["data"]["resolved_query_mode"], "candidate_audit");
    assert_eq!(payload["data"]["retrieval_mode"], "recovery_scan");
    assert_eq!(payload["data"]["candidate_included"], true);
    assert_eq!(
        payload["data"]["search_plan"]["query_plan"]["strategy"]["primary_lane"],
        "candidate_audit"
    );
    assert_eq!(
        payload["data"]["search_plan"]["query_plan"]["candidate_visibility"],
        "candidate_audit_only"
    );
    assert_eq!(
        payload["data"]["search_plan"]["query_plan"]["trust_floor"],
        "candidate_inspection_allowed"
    );
    assert_eq!(
        payload["data"]["search_plan"]["scope_keys"],
        serde_json::json!(["repo:owner/repo"])
    );
    assert_eq!(
        payload["data"]["search_plan"]["retrieval_classes"],
        serde_json::json!(["promoted_memory", "tentative_candidates", "evidence_hits"])
    );
    assert_eq!(
        payload["data"]["search_plan"]["semantic_runtime_posture"],
        "disabled_by_query_mode"
    );
    assert_eq!(
        payload["data"]["search_plan"]["retrieval_strategy"]["candidate_audit"],
        true
    );
    assert_eq!(
        payload["data"]["search_plan"]["retrieval_strategy"]["semantic"],
        false
    );
    assert!(
        payload["data"]["search_plan"]["strategy_reason"]
            .as_str()
            .expect("candidate audit strategy reason")
            .contains("candidate audit")
    );

    let items = payload["data"]["items"].as_array().expect("search items");
    let promoted = items
        .iter()
        .find(|item| item["kind"] == "promoted_memory")
        .expect("promoted memory item");
    assert_eq!(promoted["memory_lane"], "promoted_memory");
    assert_eq!(promoted["citation_posture"], "cite_allowed");
    assert_eq!(promoted["surface_posture"], "ordinary");

    let candidate = items
        .iter()
        .find(|item| item["kind"] == "candidate_memory")
        .expect("candidate memory item");
    assert_eq!(candidate["memory_lane"], "tentative_candidates");
    assert_eq!(candidate["citation_posture"], "inspect_only");
    assert_eq!(candidate["surface_posture"], "candidate_review");
    assert!(
        candidate["explain_summary"]
            .as_str()
            .expect("candidate explain summary")
            .contains("retrieval_mode recovery_scan")
    );
}
