#![cfg(unix)]

use roger_app_core::{
    ReviewTarget, ReviewTask, ReviewTaskKind, WORKER_OPERATION_REQUEST_SCHEMA_V1,
    WORKER_STAGE_RESULT_SCHEMA_V1, WorkerTurnStrategy,
};
use roger_cli::{CliRuntime, run};
use roger_storage::{
    CreateMaterializedFinding, CreateReviewRun, CreateReviewSession, CreateSessionBaselineSnapshot,
    RogerStore, UpsertMemoryItem,
};
use serde_json::{Value, json};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
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

fn sample_worker_task() -> ReviewTask {
    ReviewTask {
        id: "task-search-parity-1".to_owned(),
        review_session_id: "session-search-parity-1".to_owned(),
        review_run_id: "run-search-parity-1".to_owned(),
        stage: "deep_review".to_owned(),
        task_kind: ReviewTaskKind::DeepReviewPass,
        task_nonce: "nonce-search-parity-1".to_owned(),
        objective: "Compare rr search and rr agent recall parity.".to_owned(),
        turn_strategy: WorkerTurnStrategy::SingleTurnReport,
        allowed_scopes: vec!["repo".to_owned()],
        allowed_operations: vec!["worker.search_memory".to_owned()],
        expected_result_schema: WORKER_STAGE_RESULT_SCHEMA_V1.to_owned(),
        prompt_preset_id: Some("preset-deep-review".to_owned()),
        created_at: 100,
    }
}

fn sample_worker_request(task: &ReviewTask, query_text: &str, query_mode: &str) -> Value {
    json!({
        "schema_id": WORKER_OPERATION_REQUEST_SCHEMA_V1,
        "review_session_id": task.review_session_id,
        "review_run_id": task.review_run_id,
        "review_task_id": task.id,
        "task_nonce": task.task_nonce,
        "operation": "worker.search_memory",
        "requested_scopes": ["repo"],
        "payload": {
            "query_text": query_text,
            "query_mode": query_mode,
        },
    })
}

fn write_json_fixture(path: &Path, value: &impl serde::Serialize) {
    fs::write(
        path,
        serde_json::to_vec_pretty(value).expect("serialize fixture json"),
    )
    .expect("write fixture json");
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

fn seed_search_session(runtime: &CliRuntime, task: &ReviewTask) {
    let store = RogerStore::open(&runtime.store_root).expect("open store");
    let target = sample_target(42);
    store
        .create_review_session(CreateReviewSession {
            id: &task.review_session_id,
            review_target: &target,
            provider: "opencode",
            session_locator: None,
            resume_bundle_artifact_id: None,
            continuity_state: "resume:usable",
            attention_state: "awaiting_user_input",
            launch_profile_id: Some("profile-open-pr"),
        })
        .expect("create review session");
    store
        .create_review_run(CreateReviewRun {
            id: &task.review_run_id,
            session_id: &task.review_session_id,
            run_kind: "deep_review",
            repo_snapshot: "{\"head\":\"bbb\"}",
            continuity_quality: "usable",
            session_locator_artifact_id: None,
        })
        .expect("create review run");

    let allowed_scopes = vec!["repo".to_owned()];
    let policy_epoch_refs = vec!["config:cfg-search-parity-1".to_owned()];
    store
        .create_session_baseline_snapshot(CreateSessionBaselineSnapshot {
            id: "baseline-search-parity-1",
            review_session_id: &task.review_session_id,
            review_run_id: Some(&task.review_run_id),
            review_target_snapshot: &target,
            allowed_scopes: &allowed_scopes,
            default_query_mode: "recall",
            candidate_visibility_policy: "review_only",
            prompt_strategy: "preset:preset-deep-review/single_turn_report",
            policy_epoch_refs: &policy_epoch_refs,
            degraded_flags: &[],
        })
        .expect("create baseline snapshot");
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

fn canonical_operator_item(item: &Value) -> Value {
    json!({
        "kind": item["kind"],
        "id": item["id"],
        "memory_lane": item["memory_lane"],
        "scope_bucket": item["scope_bucket"],
        "trust_state": item["trust_state"],
        "citation_posture": item["citation_posture"],
        "surface_posture": item["surface_posture"],
        "locator": item["locator"],
        "snippet": item["snippet"],
        "explain_summary": item["explain_summary"],
    })
}

fn canonical_agent_item(item: &Value) -> Value {
    json!({
        "kind": item["item_kind"],
        "id": item["item_id"],
        "memory_lane": item["memory_lane"],
        "scope_bucket": item["scope_bucket"],
        "trust_state": item["trust_state"],
        "citation_posture": item["citation_posture"],
        "surface_posture": item["surface_posture"],
        "locator": item["locator"],
        "snippet": item["snippet_or_summary"],
        "explain_summary": item["explain_summary"],
    })
}

fn sorted_projection(items: Vec<Value>) -> Vec<Value> {
    let mut items = items;
    items.sort_by(|left, right| {
        let left_kind = left["kind"].as_str().unwrap_or_default();
        let right_kind = right["kind"].as_str().unwrap_or_default();
        let left_id = left["id"].as_str().unwrap_or_default();
        let right_id = right["id"].as_str().unwrap_or_default();
        left_kind.cmp(right_kind).then(left_id.cmp(right_id))
    });
    items
}

fn flatten_agent_projection(payload: &Value) -> Vec<Value> {
    let mut projections = Vec::new();
    for lane in ["promoted_memory", "tentative_candidates", "evidence_hits"] {
        for item in payload[lane].as_array().expect("agent recall lane") {
            projections.push(canonical_agent_item(item));
        }
    }
    sorted_projection(projections)
}

fn flatten_operator_projection(payload: &Value) -> Vec<Value> {
    sorted_projection(
        payload["items"]
            .as_array()
            .expect("operator search items")
            .iter()
            .map(canonical_operator_item)
            .collect(),
    )
}

fn assert_shared_search_truth(search_data: &Value, agent_payload: &Value) {
    assert_eq!(
        search_data["requested_query_mode"],
        agent_payload["requested_query_mode"]
    );
    assert_eq!(
        search_data["resolved_query_mode"],
        agent_payload["resolved_query_mode"]
    );
    assert_eq!(
        search_data["retrieval_mode"],
        agent_payload["retrieval_mode"]
    );
    assert_eq!(
        search_data["search_plan"]["query_plan"],
        agent_payload["search_plan"]["query_plan"]
    );
    assert_eq!(
        search_data["search_plan"]["retrieval_classes"],
        agent_payload["search_plan"]["retrieval_classes"]
    );
    assert_eq!(
        search_data["search_plan"]["retrieval_strategy"],
        agent_payload["search_plan"]["retrieval_strategy"]
    );
    assert_eq!(
        search_data["search_plan"]["semantic_runtime_posture"],
        agent_payload["search_plan"]["semantic_runtime_posture"]
    );
    assert_eq!(
        search_data["degraded_reasons"],
        agent_payload["degraded_flags"]
    );
    assert_eq!(
        flatten_operator_projection(search_data),
        flatten_agent_projection(agent_payload)
    );
}

#[test]
fn rr_agent_candidate_audit_matches_rr_search_recall_projection() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary();

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let task = sample_worker_task();
    seed_search_session(&runtime, &task);
    let store = RogerStore::open(&runtime.store_root).expect("open store");
    seed_prior_review_lookup_records(
        &store,
        &task.review_session_id,
        &task.review_run_id,
        "owner/repo",
    );

    let task_path = temp.path().join("worker-task.json");
    let request_path = temp.path().join("worker-search-request.json");
    write_json_fixture(&task_path, &task);
    write_json_fixture(
        &request_path,
        &sample_worker_request(&task, "approval refresh", "candidate_audit"),
    );

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
    let search_payload = parse_robot_payload(&search.stdout);
    let search_data = &search_payload["data"];
    assert_eq!(search_data["candidate_included"], true);

    let agent = run_rr(
        &[
            "agent",
            "worker.search_memory",
            "--task-file",
            task_path.to_str().expect("task path"),
            "--request-file",
            request_path.to_str().expect("request path"),
        ],
        &runtime,
    );
    assert_eq!(agent.exit_code, 0, "{}", agent.stderr);
    let agent_payload = parse_robot_payload(&agent.stdout);
    let agent_operation = &agent_payload["operation_response"]["payload"];

    assert_shared_search_truth(search_data, agent_operation);
    assert_eq!(
        agent_operation["tentative_candidates"][0]["citation_posture"],
        "inspect_only"
    );
    assert_eq!(
        agent_operation["tentative_candidates"][0]["surface_posture"],
        "candidate_review"
    );
    assert_eq!(
        agent_operation["promoted_memory"][0]["citation_posture"],
        "cite_allowed"
    );
}

#[test]
fn rr_agent_auto_exact_lookup_matches_rr_search_hidden_candidate_projection() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary();

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let task = sample_worker_task();
    seed_search_session(&runtime, &task);
    let store = RogerStore::open(&runtime.store_root).expect("open store");
    seed_prior_review_lookup_records(
        &store,
        &task.review_session_id,
        &task.review_run_id,
        "owner/repo",
    );

    let task_path = temp.path().join("worker-task.json");
    let request_path = temp.path().join("worker-search-request.json");
    write_json_fixture(&task_path, &task);
    write_json_fixture(
        &request_path,
        &sample_worker_request(&task, "packages/cli/src/lib.rs", "auto"),
    );

    let search = run_rr(
        &["search", "--query", "packages/cli/src/lib.rs", "--robot"],
        &runtime,
    );
    assert_eq!(search.exit_code, 5, "{}", search.stderr);
    let search_payload = parse_robot_payload(&search.stdout);
    let search_data = &search_payload["data"];
    assert_eq!(search_data["resolved_query_mode"], "exact_lookup");

    let agent = run_rr(
        &[
            "agent",
            "worker.search_memory",
            "--task-file",
            task_path.to_str().expect("task path"),
            "--request-file",
            request_path.to_str().expect("request path"),
        ],
        &runtime,
    );
    assert_eq!(agent.exit_code, 0, "{}", agent.stderr);
    let agent_payload = parse_robot_payload(&agent.stdout);
    let agent_operation = &agent_payload["operation_response"]["payload"];

    assert_shared_search_truth(search_data, agent_operation);
    assert!(
        agent_operation["tentative_candidates"]
            .as_array()
            .expect("tentative candidates array")
            .is_empty()
    );
    assert_eq!(
        agent_operation["search_plan"]["query_plan"]["candidate_visibility"],
        "hidden"
    );
}
