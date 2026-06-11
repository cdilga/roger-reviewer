use roger_app_core::{
    AGENT_TRANSPORT_RESPONSE_SCHEMA_V1, ReviewTarget, ReviewTask, ReviewTaskKind,
    WORKER_STAGE_RESULT_SCHEMA_V1, WorkerStageOutcome, WorkerStageResult, WorkerTurnStrategy,
};
use roger_cli::{CliRuntime, run};
use roger_storage::{CreateReviewRun, CreateReviewSession, RogerStore};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;
use tempfile::tempdir;

fn sample_target() -> ReviewTarget {
    ReviewTarget {
        repository: "owner/repo".to_owned(),
        pull_request_number: 42,
        base_ref: "main".to_owned(),
        head_ref: "feature".to_owned(),
        base_commit: "aaa".to_owned(),
        head_commit: "bbb".to_owned(),
    }
}

fn sample_task() -> ReviewTask {
    ReviewTask {
        id: "task-rr-agent-1".to_owned(),
        review_session_id: "session-rr-agent-1".to_owned(),
        review_run_id: "run-rr-agent-1".to_owned(),
        stage: "deep_review".to_owned(),
        task_kind: ReviewTaskKind::DeepReviewPass,
        task_nonce: "nonce-rr-agent-1".to_owned(),
        objective: "Review approval and posting safety regressions.".to_owned(),
        turn_strategy: WorkerTurnStrategy::SingleTurnReport,
        allowed_scopes: vec!["repo".to_owned()],
        allowed_operations: vec![
            "worker.get_review_context".to_owned(),
            "worker.search_memory".to_owned(),
            "worker.list_findings".to_owned(),
            "worker.get_finding_detail".to_owned(),
            "worker.get_artifact_excerpt".to_owned(),
            "worker.get_status".to_owned(),
            "worker.submit_stage_result".to_owned(),
            "worker.request_clarification".to_owned(),
            "worker.request_memory_review".to_owned(),
            "worker.propose_follow_up".to_owned(),
        ],
        expected_result_schema: WORKER_STAGE_RESULT_SCHEMA_V1.to_owned(),
        prompt_preset_id: Some("preset-deep-review".to_owned()),
        created_at: 100,
    }
}

fn sample_stage_result(task: &ReviewTask) -> WorkerStageResult {
    WorkerStageResult {
        schema_id: WORKER_STAGE_RESULT_SCHEMA_V1.to_owned(),
        review_session_id: task.review_session_id.clone(),
        review_run_id: task.review_run_id.clone(),
        review_task_id: task.id.clone(),
        worker_invocation_id: Some("worker-invocation-1".to_owned()),
        task_nonce: task.task_nonce.clone(),
        stage: task.stage.clone(),
        task_kind: task.task_kind,
        outcome: WorkerStageOutcome::Completed,
        summary: "Found one likely invalidation issue.".to_owned(),
        structured_findings_pack: Some(json!({
            "schema_version": "structured_findings_pack.v1",
            "findings": [
                {
                    "title": "Approval token survives stale refresh",
                    "summary": "The refresh path reports success without invalidating an approval token.",
                    "severity": "high",
                    "confidence": "medium"
                }
            ]
        })),
        clarification_requests: Vec::new(),
        memory_review_requests: Vec::new(),
        follow_up_proposals: Vec::new(),
        memory_citations: Vec::new(),
        artifact_refs: Vec::new(),
        provider_metadata: Some(json!({"provider": "opencode"})),
        warnings: Vec::new(),
    }
}

fn write_json_fixture(path: &Path, value: &impl serde::Serialize) {
    fs::write(
        path,
        serde_json::to_vec_pretty(value).expect("serialize fixture json"),
    )
    .expect("write fixture json");
}

fn seed_rr_agent_session(runtime: &CliRuntime, task: &ReviewTask) {
    let store = RogerStore::open(&runtime.store_root).expect("open store");
    store
        .create_review_session(CreateReviewSession {
            id: &task.review_session_id,
            review_target: &sample_target(),
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
}

fn run_rr(args: &[&str], runtime: &CliRuntime) -> roger_cli::CliRunResult {
    let argv = args
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    run(&argv, runtime)
}

fn parse_agent(stdout: &str) -> Value {
    serde_json::from_str(stdout).expect("agent payload json")
}

fn parse_robot(stdout: &str) -> Value {
    serde_json::from_str(stdout).expect("robot payload json")
}

#[test]
fn rr_agent_status_emits_dedicated_transport_envelope() {
    let temp = tempdir().expect("tempdir");
    let runtime = CliRuntime {
        cwd: temp.path().to_path_buf(),
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };
    fs::create_dir_all(&runtime.store_root).expect("create store root");

    let task = sample_task();
    seed_rr_agent_session(&runtime, &task);

    let task_path = temp.path().join("task.json");
    let request_path = temp.path().join("request.json");
    write_json_fixture(&task_path, &task);
    write_json_fixture(
        &request_path,
        &json!({
            "schema_id": "worker_operation_request.v1",
            "review_session_id": task.review_session_id,
            "review_run_id": task.review_run_id,
            "review_task_id": task.id,
            "task_nonce": task.task_nonce,
            "operation": "worker.get_status",
            "requested_scopes": ["repo"]
        }),
    );

    let result = run_rr(
        &[
            "agent",
            "worker.get_status",
            "--task-file",
            task_path.to_str().unwrap(),
            "--request-file",
            request_path.to_str().unwrap(),
        ],
        &runtime,
    );

    assert_eq!(result.exit_code, 0, "{}", result.stderr);
    let payload = parse_agent(&result.stdout);
    assert_eq!(payload["schema_id"], AGENT_TRANSPORT_RESPONSE_SCHEMA_V1);
    assert_eq!(payload["transport_kind"], "agent_cli");
    assert_eq!(payload["status"], "succeeded");
    assert_eq!(
        payload["operation_response"]["operation"],
        "worker.get_status"
    );
    assert_eq!(
        payload["operation_response"]["payload"]["attention_state"],
        "awaiting_user_input"
    );
    assert!(payload.get("robot_format").is_none(), "{payload}");
}

#[test]
fn rr_agent_stage_result_returns_acceptance_payload() {
    let temp = tempdir().expect("tempdir");
    let runtime = CliRuntime {
        cwd: temp.path().to_path_buf(),
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };
    fs::create_dir_all(&runtime.store_root).expect("create store root");

    let task = sample_task();
    seed_rr_agent_session(&runtime, &task);

    let task_path = temp.path().join("task.json");
    let request_path = temp.path().join("request.json");
    write_json_fixture(&task_path, &task);
    write_json_fixture(
        &request_path,
        &json!({
            "schema_id": "worker_operation_request.v1",
            "review_session_id": task.review_session_id,
            "review_run_id": task.review_run_id,
            "review_task_id": task.id,
            "task_nonce": task.task_nonce,
            "operation": "worker.submit_stage_result",
            "requested_scopes": ["repo"],
            "payload": sample_stage_result(&task)
        }),
    );

    let result = run_rr(
        &[
            "agent",
            "worker.submit_stage_result",
            "--task-file",
            task_path.to_str().unwrap(),
            "--request-file",
            request_path.to_str().unwrap(),
        ],
        &runtime,
    );

    assert_eq!(result.exit_code, 0, "{}", result.stderr);
    let payload = parse_agent(&result.stdout);
    assert_eq!(payload["schema_id"], AGENT_TRANSPORT_RESPONSE_SCHEMA_V1);
    assert_eq!(payload["status"], "succeeded");
    assert_eq!(
        payload["operation_response"]["payload"]["result_schema_id"],
        WORKER_STAGE_RESULT_SCHEMA_V1
    );
    assert_eq!(
        payload["operation_response"]["payload"]["structured_findings_pack_present"],
        true
    );

    // Acceptance must be durable: the stage result is recorded for audit and
    // the validated findings pack is materialized into canonical Finding rows
    // that the operator readback surface can see.
    let store = RogerStore::open(&runtime.store_root).expect("reopen store");
    let stage_results = store
        .worker_stage_results_for_run(&task.review_session_id, &task.review_run_id)
        .expect("load stage results");
    assert_eq!(stage_results.len(), 1);
    assert_eq!(stage_results[0].review_task_id, task.id);

    let findings = store
        .materialized_findings_for_run(&task.review_session_id, &task.review_run_id)
        .expect("load materialized findings");
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].title, "Approval token survives stale refresh");
    assert_eq!(findings[0].triage_state, "new");
    assert_eq!(findings[0].outbound_state, "not_drafted");
    assert!(!findings[0].fingerprint.is_empty());

    // A completed pass with materialized findings must surface as the
    // canonical findings_ready attention state so every operator surface
    // (CLI status, TUI, extension mirror) sees a decision is waiting.
    let session = store
        .review_session(&task.review_session_id)
        .expect("load session")
        .expect("session exists");
    assert_eq!(session.attention_state, "findings_ready");

    let listed = run_rr(
        &[
            "findings",
            "--repo",
            "owner/repo",
            "--pr",
            "42",
            "--session",
            &task.review_session_id,
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(listed.exit_code, 0, "{}", listed.stderr);
    let listed_payload = parse_robot(&listed.stdout);
    let items = listed_payload["data"]["items"]
        .as_array()
        .expect("findings items");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["title"], "Approval token survives stale refresh");
}

#[test]
fn rr_agent_rejects_request_for_different_bound_session() {
    let temp = tempdir().expect("tempdir");
    let runtime = CliRuntime {
        cwd: temp.path().to_path_buf(),
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };
    fs::create_dir_all(&runtime.store_root).expect("create store root");

    let task = sample_task();
    seed_rr_agent_session(&runtime, &task);

    let task_path = temp.path().join("task.json");
    let request_path = temp.path().join("request.json");
    write_json_fixture(&task_path, &task);
    write_json_fixture(
        &request_path,
        &json!({
            "schema_id": "worker_operation_request.v1",
            "review_session_id": "session-elsewhere",
            "review_run_id": task.review_run_id,
            "review_task_id": task.id,
            "task_nonce": task.task_nonce,
            "operation": "worker.get_status",
            "requested_scopes": ["repo"]
        }),
    );

    let result = run_rr(
        &[
            "agent",
            "worker.get_status",
            "--task-file",
            task_path.to_str().unwrap(),
            "--request-file",
            request_path.to_str().unwrap(),
        ],
        &runtime,
    );

    assert_eq!(result.exit_code, 1, "{}", result.stderr);
    let payload = parse_agent(&result.stdout);
    assert_eq!(payload["schema_id"], AGENT_TRANSPORT_RESPONSE_SCHEMA_V1);
    assert_eq!(payload["status"], "error");
    assert_eq!(payload["error"]["code"], "validation_failed");
    assert!(
        payload["error"]["message"]
            .as_str()
            .expect("validation error message")
            .contains("review_session_id"),
        "{payload}"
    );
}

#[test]
fn rr_agent_denies_hidden_posting_operations() {
    let temp = tempdir().expect("tempdir");
    let runtime = CliRuntime {
        cwd: temp.path().to_path_buf(),
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };
    fs::create_dir_all(&runtime.store_root).expect("create store root");

    let task = sample_task();
    seed_rr_agent_session(&runtime, &task);

    let task_path = temp.path().join("task.json");
    let request_path = temp.path().join("request.json");
    write_json_fixture(&task_path, &task);
    write_json_fixture(
        &request_path,
        &json!({
            "schema_id": "worker_operation_request.v1",
            "review_session_id": task.review_session_id,
            "review_run_id": task.review_run_id,
            "review_task_id": task.id,
            "task_nonce": task.task_nonce,
            "operation": "worker.post_to_github",
            "requested_scopes": ["repo"],
            "payload": { "draft_id": "draft-1" }
        }),
    );

    let result = run_rr(
        &[
            "agent",
            "worker.post_to_github",
            "--task-file",
            task_path.to_str().unwrap(),
            "--request-file",
            request_path.to_str().unwrap(),
        ],
        &runtime,
    );

    assert_eq!(result.exit_code, 3, "{}", result.stderr);
    let payload = parse_agent(&result.stdout);
    assert_eq!(payload["schema_id"], AGENT_TRANSPORT_RESPONSE_SCHEMA_V1);
    assert_eq!(payload["status"], "denied");
    assert_eq!(
        payload["operation_response"]["denial"]["code"],
        "unsupported_operation"
    );
    assert!(
        payload["operation_response"]["denial"]["message"]
            .as_str()
            .expect("denial message")
            .contains("canonical rr agent API"),
        "{payload}"
    );
}

#[test]
fn rr_agent_rejects_robot_flag_at_parse_boundary() {
    let temp = tempdir().expect("tempdir");
    let runtime = CliRuntime {
        cwd: temp.path().to_path_buf(),
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };
    fs::create_dir_all(&runtime.store_root).expect("create store root");

    let task = sample_task();
    let task_path = temp.path().join("task.json");
    write_json_fixture(&task_path, &task);

    let result = run_rr(
        &[
            "agent",
            "worker.get_status",
            "--task-file",
            task_path.to_str().unwrap(),
            "--robot",
        ],
        &runtime,
    );

    assert_eq!(result.exit_code, 2);
    assert!(
        result
            .stderr
            .contains("rr agent is a separate transport from --robot"),
        "{}",
        result.stderr
    );
}

#[test]
fn robot_docs_exposes_rr_agent_as_separate_transport_contract() {
    let temp = tempdir().expect("tempdir");
    let runtime = CliRuntime {
        cwd: temp.path().to_path_buf(),
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };
    fs::create_dir_all(&runtime.store_root).expect("create store root");

    let commands = run_rr(&["robot-docs", "commands", "--robot"], &runtime);
    assert_eq!(commands.exit_code, 0, "{}", commands.stderr);
    let commands_payload = parse_robot(&commands.stdout);
    let agent_command = commands_payload["data"]["items"]
        .as_array()
        .expect("commands items")
        .iter()
        .find(|item| item["command"] == "rr agent <operation>")
        .expect("rr agent command entry");
    assert_eq!(agent_command["surface"], "dedicated_worker_transport");
    assert_eq!(agent_command["separate_from_robot"], true);
    assert!(
        agent_command["supported_operations"]
            .as_array()
            .expect("supported operations")
            .iter()
            .any(|entry| entry == "worker.search_memory"),
        "{agent_command}"
    );

    let schemas = run_rr(&["robot-docs", "schemas", "--robot"], &runtime);
    assert_eq!(schemas.exit_code, 0, "{}", schemas.stderr);
    let schemas_payload = parse_robot(&schemas.stdout);
    let agent_schema = schemas_payload["data"]["items"]
        .as_array()
        .expect("schema items")
        .iter()
        .find(|item| item["command"] == "rr agent")
        .expect("rr agent schema entry");
    assert_eq!(
        agent_schema["schema_id"],
        AGENT_TRANSPORT_RESPONSE_SCHEMA_V1
    );
}
