#![cfg(unix)]

//! Reverse-parity CLI smokes: the operator commands the TUI already had but the
//! CLI lacked — `rr memory review/accept/reject`, `rr timeline`, and
//! `rr clarify`. Each exercises the happy path plus the fail-closed guards, and
//! asserts the new stable robot schema ids.

use roger_app_core::{
    ApprovalState, OutboundDraftBatch, PostedAction, PostedActionStatus, ReviewTarget,
    WorkerStageOutcome, WorkerStageResult,
};
use roger_cli::{CliRuntime, run};
use roger_storage::{
    ClarificationRequestQuery, CreateMaterializedFinding, CreateMemoryReviewRequest,
    CreateReviewRun, CreateReviewSession, CreateWorkerStageResult, MemoryReviewRequestKind,
    MemoryReviewSource, RogerStore,
};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::{TempDir, tempdir};

const WORKER_STAGE_RESULT_SCHEMA_V1: &str = roger_app_core::WORKER_STAGE_RESULT_SCHEMA_V1;

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

fn runtime_for(temp: &TempDir) -> CliRuntime {
    CliRuntime {
        cwd: init_repo(temp),
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    }
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

fn seed_session(runtime: &CliRuntime, session_id: &str, run_id: &str, target: &ReviewTarget) {
    let store = RogerStore::open(&runtime.store_root).expect("open store");
    store
        .create_review_session(CreateReviewSession {
            id: session_id,
            review_target: target,
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
            id: run_id,
            session_id,
            run_kind: "deep_review",
            repo_snapshot: "{\"head\":\"bbb\"}",
            continuity_quality: "usable",
            session_locator_artifact_id: None,
        })
        .expect("create review run");
}

fn seed_memory_review_request(
    runtime: &CliRuntime,
    session_id: &str,
    run_id: &str,
    repository: &str,
    normalized_key: &str,
    external_ref: &str,
) -> String {
    let store = RogerStore::open(&runtime.store_root).expect("open store");
    let scope_key = format!("repo:{repository}");
    let record = store
        .create_memory_review_request(CreateMemoryReviewRequest {
            review_session_id: session_id,
            review_run_id: Some(run_id),
            source: MemoryReviewSource::Worker,
            request_kind: MemoryReviewRequestKind::Promote,
            statement: "Prefer the shared review-ops op over reimplementing checks.",
            normalized_key,
            scope_key: &scope_key,
            memory_class: "convention",
            rationale: Some("Surfaced repeatedly across reviews."),
            external_ref: Some(external_ref),
        })
        .expect("create memory review request");
    record.id
}

#[test]
fn memory_review_lists_pending_then_accept_materializes_and_reject_resolves() {
    let temp = tempdir().expect("tempdir");
    let runtime = runtime_for(&temp);
    let target = sample_target("owner/repo", 42);
    seed_session(&runtime, "session-mem", "run-mem", &target);
    let accept_id = seed_memory_review_request(
        &runtime,
        "session-mem",
        "run-mem",
        "owner/repo",
        "prefer-shared-ops",
        "req-accept",
    );
    let reject_id = seed_memory_review_request(
        &runtime,
        "session-mem",
        "run-mem",
        "owner/repo",
        "avoid-raw-gh",
        "req-reject",
    );

    // review lists both pending rows.
    let review = run_rr(&["memory", "review", "--robot"], &runtime);
    assert_eq!(review.exit_code, 0, "{}", review.stderr);
    let review_payload = parse_robot_payload(&review.stdout);
    assert_eq!(review_payload["schema_id"], "rr.robot.memory.v1");
    assert_eq!(review_payload["command"], "rr memory");
    assert_eq!(review_payload["outcome"], "complete");
    assert_eq!(review_payload["data"]["count"], Value::from(2));
    let ids = review_payload["data"]["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| item["id"].as_str().unwrap_or_default().to_owned())
        .collect::<Vec<_>>();
    assert!(ids.contains(&accept_id), "{}", review.stdout);
    assert!(ids.contains(&reject_id), "{}", review.stdout);
    // The listing exposes the fields the contract names.
    let first = &review_payload["data"]["items"][0];
    assert!(first["kind"].is_string());
    assert!(first["statement"].is_string());
    assert!(first["scope"].is_string());

    // accept materializes a memory_item and resolves the request.
    let accept = run_rr(
        &["memory", "accept", "--request", &accept_id, "--robot"],
        &runtime,
    );
    assert_eq!(accept.exit_code, 0, "{}", accept.stderr);
    let accept_payload = parse_robot_payload(&accept.stdout);
    assert_eq!(accept_payload["schema_id"], "rr.robot.memory.v1");
    assert_eq!(accept_payload["outcome"], "complete");
    assert_eq!(accept_payload["data"]["decision"], "accept");
    assert_eq!(
        accept_payload["data"]["materialized_new_item"],
        Value::Bool(true)
    );
    let memory_item_id = accept_payload["data"]["resulting_memory_item_id"]
        .as_str()
        .expect("materialized memory item id");
    assert!(!memory_item_id.is_empty());

    // reject just resolves.
    let reject = run_rr(
        &["memory", "reject", "--request", &reject_id, "--robot"],
        &runtime,
    );
    assert_eq!(reject.exit_code, 0, "{}", reject.stderr);
    let reject_payload = parse_robot_payload(&reject.stdout);
    assert_eq!(reject_payload["data"]["decision"], "reject");
    assert_eq!(
        reject_payload["data"]["resulting_memory_item_id"],
        Value::Null
    );

    // Both are resolved now: review lists nothing pending.
    let review_after = run_rr(&["memory", "review", "--robot"], &runtime);
    let review_after_payload = parse_robot_payload(&review_after.stdout);
    assert_eq!(review_after_payload["outcome"], "empty");
    assert_eq!(review_after_payload["data"]["count"], Value::from(0));

    // Storage-level truth: the accepted request now has status accepted with a
    // materialized item; the rejected one is rejected with none.
    let store = RogerStore::open(&runtime.store_root).expect("reopen store");
    let accepted = store
        .memory_review_request(&accept_id)
        .expect("lookup")
        .expect("row");
    assert_eq!(accepted.status, "accepted");
    assert_eq!(
        accepted.resulting_memory_item_id.as_deref(),
        Some(memory_item_id)
    );
    let rejected = store
        .memory_review_request(&reject_id)
        .expect("lookup")
        .expect("row");
    assert_eq!(rejected.status, "rejected");
    assert!(rejected.resulting_memory_item_id.is_none());
}

#[test]
fn memory_accept_unknown_id_fails_closed() {
    let temp = tempdir().expect("tempdir");
    let runtime = runtime_for(&temp);

    let accept = run_rr(
        &["memory", "accept", "--request", "does-not-exist", "--robot"],
        &runtime,
    );
    assert_eq!(accept.exit_code, 3, "{}", accept.stderr);
    let payload = parse_robot_payload(&accept.stdout);
    assert_eq!(payload["schema_id"], "rr.robot.memory.v1");
    assert_eq!(payload["outcome"], "blocked");
    assert_eq!(payload["data"]["reason_code"], "unknown_request_id");
    assert_eq!(payload["data"]["request_id"], "does-not-exist");
}

#[test]
fn memory_accept_already_resolved_is_blocked() {
    let temp = tempdir().expect("tempdir");
    let runtime = runtime_for(&temp);
    let target = sample_target("owner/repo", 42);
    seed_session(&runtime, "session-mem2", "run-mem2", &target);
    let request_id = seed_memory_review_request(
        &runtime,
        "session-mem2",
        "run-mem2",
        "owner/repo",
        "prefer-shared-ops",
        "req-once",
    );

    let first = run_rr(
        &["memory", "accept", "--request", &request_id, "--robot"],
        &runtime,
    );
    assert_eq!(first.exit_code, 0, "{}", first.stderr);

    // A second resolution of the same request must fail closed, not double-apply.
    let second = run_rr(
        &["memory", "reject", "--request", &request_id, "--robot"],
        &runtime,
    );
    assert_eq!(second.exit_code, 3, "{}", second.stderr);
    let payload = parse_robot_payload(&second.stdout);
    assert_eq!(payload["outcome"], "blocked");
    assert_eq!(payload["data"]["reason_code"], "already_resolved");
    assert_eq!(payload["data"]["status"], "accepted");
}

#[test]
fn memory_accept_missing_request_flag_fails_closed() {
    let temp = tempdir().expect("tempdir");
    let runtime = runtime_for(&temp);

    let accept = run_rr(&["memory", "accept", "--robot"], &runtime);
    assert_eq!(accept.exit_code, 3, "{}", accept.stderr);
    let payload = parse_robot_payload(&accept.stdout);
    assert_eq!(payload["outcome"], "blocked");
    assert_eq!(payload["data"]["reason_code"], "request_id_required");
}

fn seed_stage_result(runtime: &CliRuntime, session_id: &str, run_id: &str, task_id: &str) {
    let store = RogerStore::open(&runtime.store_root).expect("open store");
    let result = WorkerStageResult {
        schema_id: WORKER_STAGE_RESULT_SCHEMA_V1.to_owned(),
        review_session_id: session_id.to_owned(),
        review_run_id: run_id.to_owned(),
        review_task_id: task_id.to_owned(),
        worker_invocation_id: None,
        task_nonce: format!("nonce-{task_id}"),
        stage: "deep_review".to_owned(),
        task_kind: roger_app_core::ReviewTaskKind::DeepReviewPass,
        outcome: WorkerStageOutcome::Completed,
        summary: "Found one likely issue.".to_owned(),
        structured_findings_pack: None,
        clarification_requests: Vec::new(),
        memory_review_requests: Vec::new(),
        follow_up_proposals: Vec::new(),
        memory_citations: Vec::new(),
        artifact_refs: Vec::new(),
        provider_metadata: None,
        warnings: Vec::new(),
    };
    store
        .record_worker_stage_result(CreateWorkerStageResult {
            result: &result,
            submitted_result_artifact_id: None,
            structured_findings_pack_artifact_id: None,
        })
        .expect("record stage result");
}

#[test]
fn timeline_renders_runs_stages_and_posted_actions_in_order() {
    let temp = tempdir().expect("tempdir");
    let runtime = runtime_for(&temp);
    let target = sample_target("owner/repo", 42);
    seed_session(&runtime, "session-tl", "run-tl-1", &target);
    seed_stage_result(&runtime, "session-tl", "run-tl-1", "task-tl-1");

    // A second, later run so we can assert chronological (oldest-first) ordering.
    {
        let store = RogerStore::open(&runtime.store_root).expect("open store");
        store
            .create_review_run(CreateReviewRun {
                id: "run-tl-2",
                session_id: "session-tl",
                run_kind: "refresh",
                repo_snapshot: "{\"head\":\"ccc\"}",
                continuity_quality: "usable",
                session_locator_artifact_id: None,
            })
            .expect("create later run");
    }
    seed_stage_result(&runtime, "session-tl", "run-tl-2", "task-tl-2");

    // Seed a posted action so the timeline includes the posted lane. The posted
    // action's payload digest must match its batch (storage binding invariant).
    {
        let store = RogerStore::open(&runtime.store_root).expect("open store");
        store
            .store_outbound_draft_batch(&OutboundDraftBatch {
                id: "batch-tl-1".to_owned(),
                review_session_id: "session-tl".to_owned(),
                review_run_id: "run-tl-1".to_owned(),
                repo_id: "owner/repo".to_owned(),
                remote_review_target_id: "owner/repo#42".to_owned(),
                payload_digest: "digest-tl-1".to_owned(),
                approval_state: ApprovalState::Posted,
                approved_at: Some(400),
                invalidated_at: None,
                invalidation_reason_code: None,
                row_version: 0,
            })
            .expect("store draft batch");
        store
            .store_posted_batch_action(&PostedAction {
                id: "posted-tl-1".to_owned(),
                draft_batch_id: "batch-tl-1".to_owned(),
                provider: "github".to_owned(),
                remote_identifier: "https://github.com/owner/repo/pull/42#review-1".to_owned(),
                status: PostedActionStatus::Succeeded,
                posted_payload_digest: "digest-tl-1".to_owned(),
                posted_at: 500,
                failure_code: None,
            })
            .expect("store posted action");
    }

    let timeline = run_rr(
        &["timeline", "--session", "session-tl", "--robot"],
        &runtime,
    );
    assert_eq!(timeline.exit_code, 0, "{}", timeline.stderr);
    let payload = parse_robot_payload(&timeline.stdout);
    assert_eq!(payload["schema_id"], "rr.robot.timeline.v1");
    assert_eq!(payload["command"], "rr timeline");
    assert_eq!(payload["outcome"], "complete");
    assert_eq!(payload["data"]["run_count"], Value::from(2));
    assert_eq!(payload["data"]["posted_action_count"], Value::from(1));

    let runs = payload["data"]["runs"].as_array().expect("runs");
    // Oldest-first: run-tl-1 precedes run-tl-2.
    assert_eq!(runs[0]["run_id"], "run-tl-1");
    assert_eq!(runs[1]["run_id"], "run-tl-2");
    assert_eq!(runs[0]["stages"][0]["stage"], "deep_review");
    assert_eq!(runs[0]["stages"][0]["outcome"], "completed");

    let posted = payload["data"]["posted_actions"]
        .as_array()
        .expect("posted");
    assert_eq!(posted.len(), 1);
    assert_eq!(posted[0]["action_id"], "posted-tl-1");
    assert_eq!(posted[0]["status"], "succeeded");
}

#[test]
fn timeline_empty_when_no_session_for_target() {
    let temp = tempdir().expect("tempdir");
    let runtime = runtime_for(&temp);

    let timeline = run_rr(&["timeline", "--pr", "999", "--robot"], &runtime);
    assert_eq!(timeline.exit_code, 0, "{}", timeline.stderr);
    let payload = parse_robot_payload(&timeline.stdout);
    assert_eq!(payload["schema_id"], "rr.robot.timeline.v1");
    assert_eq!(payload["outcome"], "empty");
    assert_eq!(payload["data"]["run_count"], Value::from(0));
    assert_eq!(payload["data"]["posted_action_count"], Value::from(0));
}

fn seed_finding(runtime: &CliRuntime, session_id: &str, run_id: &str, finding_id: &str) {
    let store = RogerStore::open(&runtime.store_root).expect("open store");
    store
        .upsert_materialized_finding(CreateMaterializedFinding {
            id: finding_id,
            session_id,
            review_run_id: run_id,
            stage: "deep_review",
            fingerprint: "fp:clarify-one",
            title: "Clarifiable finding",
            normalized_summary: "clarifiable finding summary",
            severity: "high",
            confidence: "medium",
            triage_state: "new",
            outbound_state: "not_drafted",
        })
        .expect("seed finding");
}

#[test]
fn clarify_creates_durable_clarification_then_lists_it() {
    let temp = tempdir().expect("tempdir");
    let runtime = runtime_for(&temp);
    let target = sample_target("owner/repo", 42);
    seed_session(&runtime, "session-cl", "run-cl", &target);
    seed_finding(&runtime, "session-cl", "run-cl", "finding-cl-1");

    let create = run_rr(
        &[
            "clarify",
            "--session",
            "session-cl",
            "--finding",
            "finding-cl-1",
            "--body",
            "Is the null path intentional here?",
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(create.exit_code, 0, "{}", create.stderr);
    let create_payload = parse_robot_payload(&create.stdout);
    assert_eq!(create_payload["schema_id"], "rr.robot.clarify.v1");
    assert_eq!(create_payload["command"], "rr clarify");
    assert_eq!(create_payload["outcome"], "complete");
    let clarification = &create_payload["data"]["clarification"];
    assert_eq!(clarification["finding_id"], "finding-cl-1");
    assert_eq!(clarification["review_session_id"], "session-cl");
    assert_eq!(clarification["source"], "operator");
    assert_eq!(clarification["status"], "open");
    let clarification_id = clarification["id"].as_str().expect("clarification id");

    // list shows the open clarification.
    let list = run_rr(
        &["clarify", "--list", "--session", "session-cl", "--robot"],
        &runtime,
    );
    assert_eq!(list.exit_code, 0, "{}", list.stderr);
    let list_payload = parse_robot_payload(&list.stdout);
    assert_eq!(list_payload["schema_id"], "rr.robot.clarify.v1");
    assert_eq!(list_payload["outcome"], "complete");
    assert_eq!(list_payload["data"]["count"], Value::from(1));
    assert_eq!(
        list_payload["data"]["items"][0]["id"].as_str(),
        Some(clarification_id)
    );

    // Storage-level truth: the clarification row is durable and open.
    let store = RogerStore::open(&runtime.store_root).expect("reopen store");
    let open = store
        .list_clarification_requests(ClarificationRequestQuery {
            review_session_id: Some("session-cl"),
            status: Some("open"),
            limit: 10,
        })
        .expect("list clarifications");
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].id, clarification_id);
    assert_eq!(open[0].body, "Is the null path intentional here?");
}

#[test]
fn clarify_missing_finding_fails_closed() {
    let temp = tempdir().expect("tempdir");
    let runtime = runtime_for(&temp);
    let target = sample_target("owner/repo", 42);
    seed_session(&runtime, "session-cl2", "run-cl2", &target);

    // No --finding on the create path must fail closed.
    let create = run_rr(
        &[
            "clarify",
            "--session",
            "session-cl2",
            "--body",
            "why?",
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(create.exit_code, 3, "{}", create.stderr);
    let payload = parse_robot_payload(&create.stdout);
    assert_eq!(payload["schema_id"], "rr.robot.clarify.v1");
    assert_eq!(payload["outcome"], "blocked");
    assert_eq!(payload["data"]["reason_code"], "finding_required");

    // An unknown finding id must also fail closed.
    let unknown = run_rr(
        &[
            "clarify",
            "--session",
            "session-cl2",
            "--finding",
            "finding-does-not-exist",
            "--body",
            "why?",
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(unknown.exit_code, 3, "{}", unknown.stderr);
    let unknown_payload = parse_robot_payload(&unknown.stdout);
    assert_eq!(unknown_payload["outcome"], "blocked");
    assert_eq!(unknown_payload["data"]["reason_code"], "unknown_finding_id");
}

#[test]
fn clarify_missing_body_fails_closed() {
    let temp = tempdir().expect("tempdir");
    let runtime = runtime_for(&temp);
    let target = sample_target("owner/repo", 42);
    seed_session(&runtime, "session-cl3", "run-cl3", &target);
    seed_finding(&runtime, "session-cl3", "run-cl3", "finding-cl-3");

    let create = run_rr(
        &[
            "clarify",
            "--session",
            "session-cl3",
            "--finding",
            "finding-cl-3",
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(create.exit_code, 3, "{}", create.stderr);
    let payload = parse_robot_payload(&create.stdout);
    assert_eq!(payload["outcome"], "blocked");
    assert_eq!(
        payload["data"]["reason_code"],
        "clarification_body_required"
    );
}
