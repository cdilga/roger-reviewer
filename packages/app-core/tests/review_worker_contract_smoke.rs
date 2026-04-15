use roger_app_core::{
    ReviewTarget, ReviewTask, ReviewTaskKind, ReviewWorkerContractError,
    WORKER_STAGE_RESULT_SCHEMA_V1, WorkerArtifactRef, WorkerCapabilityProfile, WorkerContextPacket,
    WorkerFindingSummary, WorkerGitHubPosture, WorkerInvocation, WorkerInvocationOutcomeState,
    WorkerMemoryCard, WorkerMutationPosture, WorkerStageOutcome, WorkerStageResult,
    WorkerToolCallEvent, WorkerToolCallOutcomeState, WorkerTransportKind, WorkerTurnStrategy,
};
use serde_json::json;

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
        id: "task-1".to_owned(),
        review_session_id: "session-1".to_owned(),
        review_run_id: "run-1".to_owned(),
        stage: "deep_review".to_owned(),
        task_kind: ReviewTaskKind::DeepReviewPass,
        task_nonce: "nonce-1".to_owned(),
        objective: "review the PR for approval and posting regressions".to_owned(),
        turn_strategy: WorkerTurnStrategy::SingleTurnReport,
        allowed_scopes: vec!["repo".to_owned()],
        allowed_operations: vec![
            "worker.get_review_context".to_owned(),
            "worker.list_findings".to_owned(),
            "worker.submit_stage_result".to_owned(),
        ],
        expected_result_schema: WORKER_STAGE_RESULT_SCHEMA_V1.to_owned(),
        prompt_preset_id: Some("preset-deep-review".to_owned()),
        created_at: 100,
    }
}

fn sample_context() -> WorkerContextPacket {
    WorkerContextPacket {
        review_target: sample_target(),
        review_session_id: "session-1".to_owned(),
        review_run_id: "run-1".to_owned(),
        review_task_id: "task-1".to_owned(),
        task_nonce: "nonce-1".to_owned(),
        baseline_snapshot_ref: Some("baseline-1".to_owned()),
        provider: "opencode".to_owned(),
        transport_kind: WorkerTransportKind::AgentCli,
        stage: "deep_review".to_owned(),
        objective: "review the PR for approval and posting regressions".to_owned(),
        allowed_scopes: vec!["repo".to_owned()],
        allowed_operations: vec![
            "worker.get_review_context".to_owned(),
            "worker.list_findings".to_owned(),
            "worker.submit_stage_result".to_owned(),
        ],
        mutation_posture: WorkerMutationPosture::ReviewOnly,
        github_posture: WorkerGitHubPosture::Blocked,
        unresolved_findings: vec![WorkerFindingSummary {
            finding_id: "finding-1".to_owned(),
            fingerprint: "fp-1".to_owned(),
            summary: "An approval token may survive head drift.".to_owned(),
            triage_state: "new".to_owned(),
            outbound_state: "not_drafted".to_owned(),
            primary_evidence_ref: Some("artifact-1".to_owned()),
        }],
        continuity_summary: Some("provider session continuity is usable".to_owned()),
        memory_cards: vec![WorkerMemoryCard {
            citation_id: "memory-1".to_owned(),
            scope: "repo".to_owned(),
            title: "Previous approval invalidation bug".to_owned(),
            summary: "Prior review found stale batch approval after refresh.".to_owned(),
            provenance: "promoted_memory".to_owned(),
            trust_tier: "validated".to_owned(),
            tentative: false,
        }],
        artifact_refs: vec![WorkerArtifactRef {
            artifact_id: "artifact-1".to_owned(),
            role: "diff_excerpt".to_owned(),
            media_type: Some("text/plain".to_owned()),
            summary: Some("approval path excerpt".to_owned()),
        }],
    }
}

fn sample_capability_profile() -> WorkerCapabilityProfile {
    WorkerCapabilityProfile {
        transport_kind: WorkerTransportKind::AgentCli,
        supports_context_reads: true,
        supports_memory_search: true,
        supports_finding_reads: true,
        supports_artifact_reads: true,
        supports_stage_result_submission: true,
        supports_clarification_requests: true,
        supports_follow_up_hints: true,
        supports_fix_mode: false,
    }
}

fn sample_invocation() -> WorkerInvocation {
    WorkerInvocation {
        id: "worker-invocation-1".to_owned(),
        review_session_id: "session-1".to_owned(),
        review_run_id: "run-1".to_owned(),
        review_task_id: "task-1".to_owned(),
        provider: "opencode".to_owned(),
        provider_session_id: Some("oc-session-1".to_owned()),
        transport_kind: WorkerTransportKind::AgentCli,
        started_at: 200,
        completed_at: Some(210),
        outcome_state: WorkerInvocationOutcomeState::Completed,
        prompt_invocation_id: Some("prompt-invocation-1".to_owned()),
        raw_output_artifact_id: Some("raw-output-1".to_owned()),
        result_artifact_id: Some("worker-result-1".to_owned()),
    }
}

fn sample_tool_call_event() -> WorkerToolCallEvent {
    WorkerToolCallEvent {
        id: "tool-call-1".to_owned(),
        review_task_id: "task-1".to_owned(),
        worker_invocation_id: "worker-invocation-1".to_owned(),
        operation: "worker.get_review_context".to_owned(),
        request_digest: "request-digest-1".to_owned(),
        response_digest: Some("response-digest-1".to_owned()),
        outcome_state: WorkerToolCallOutcomeState::Succeeded,
        occurred_at: 205,
    }
}

fn sample_result() -> WorkerStageResult {
    WorkerStageResult {
        schema_id: WORKER_STAGE_RESULT_SCHEMA_V1.to_owned(),
        review_session_id: "session-1".to_owned(),
        review_run_id: "run-1".to_owned(),
        review_task_id: "task-1".to_owned(),
        worker_invocation_id: Some("worker-invocation-1".to_owned()),
        task_nonce: "nonce-1".to_owned(),
        stage: "deep_review".to_owned(),
        task_kind: ReviewTaskKind::DeepReviewPass,
        outcome: WorkerStageOutcome::Completed,
        summary: "Found one likely correctness issue in approval invalidation.".to_owned(),
        structured_findings_pack: Some(json!({
            "schema_version": "structured_findings_pack.v1",
            "findings": [
                {
                    "title": "Approval batch survives stale refresh",
                    "summary": "The invalidation path drops a stale batch signal and reports success.",
                    "severity": "high",
                    "confidence": "medium",
                    "evidence": [
                        {
                            "repo_rel_path": "packages/cli/src/lib.rs",
                            "start_line": 1200,
                            "end_line": 1218,
                            "evidence_role": "primary"
                        }
                    ]
                }
            ]
        })),
        clarification_requests: Vec::new(),
        memory_review_requests: vec![roger_app_core::WorkerMemoryReviewRequest {
            id: "memory-review-1".to_owned(),
            query: "approval invalidation refresh".to_owned(),
            requested_scopes: vec!["repo".to_owned()],
            rationale: Some("look for prior approval-token failures".to_owned()),
        }],
        follow_up_proposals: vec![roger_app_core::WorkerFollowUpProposal {
            id: "follow-up-1".to_owned(),
            title: "Recheck after invalidation refactor".to_owned(),
            objective: "rerun deep review after refresh-lifecycle fix lands".to_owned(),
            proposed_task_kind: ReviewTaskKind::RecheckFinding,
            suggested_scopes: vec!["repo".to_owned()],
        }],
        memory_citations: Vec::new(),
        artifact_refs: vec![WorkerArtifactRef {
            artifact_id: "artifact-1".to_owned(),
            role: "diff_excerpt".to_owned(),
            media_type: Some("text/plain".to_owned()),
            summary: Some("approval path excerpt".to_owned()),
        }],
        provider_metadata: Some(json!({"provider": "opencode", "model": "o4"})),
        warnings: vec!["provider required fallback context read".to_owned()],
    }
}

#[test]
fn worker_contract_objects_round_trip_and_validate_binding() {
    let task = sample_task();
    let context = sample_context();
    let capability = sample_capability_profile();
    let invocation = sample_invocation();
    let tool_call = sample_tool_call_event();
    let result = sample_result();

    let task_round_trip: ReviewTask =
        serde_json::from_str(&serde_json::to_string(&task).expect("serialize task"))
            .expect("deserialize task");
    assert_eq!(task_round_trip, task);

    let context_round_trip: WorkerContextPacket =
        serde_json::from_str(&serde_json::to_string(&context).expect("serialize context"))
            .expect("deserialize context");
    assert_eq!(context_round_trip, context);

    let capability_round_trip: WorkerCapabilityProfile =
        serde_json::from_str(&serde_json::to_string(&capability).expect("serialize capability"))
            .expect("deserialize capability");
    assert_eq!(capability_round_trip, capability);

    let invocation_round_trip: WorkerInvocation =
        serde_json::from_str(&serde_json::to_string(&invocation).expect("serialize invocation"))
            .expect("deserialize invocation");
    assert_eq!(invocation_round_trip, invocation);

    let tool_call_round_trip: WorkerToolCallEvent =
        serde_json::from_str(&serde_json::to_string(&tool_call).expect("serialize tool call"))
            .expect("deserialize tool call");
    assert_eq!(tool_call_round_trip, tool_call);

    let result_round_trip: WorkerStageResult =
        serde_json::from_str(&serde_json::to_string(&result).expect("serialize result"))
            .expect("deserialize result");
    assert_eq!(result_round_trip, result);

    task.validate_context_packet(&context)
        .expect("context packet should bind to task");
    task.validate_capability_profile(&capability)
        .expect("capability should allow result submission");
    task.validate_stage_result(&result)
        .expect("result should bind to task");
    task.validate_worker_invocation_binding(&result, &invocation.id)
        .expect("result should bind to worker invocation");
    task.validate_tool_call_event(&tool_call, &invocation.id)
        .expect("tool call should bind to worker invocation");
    task.validate_prompt_preset_id("preset-deep-review")
        .expect("prompt preset should match");

    let pack_json = result
        .structured_findings_pack_json()
        .expect("serialize pack")
        .expect("pack present");
    let pack_value: serde_json::Value =
        serde_json::from_str(&pack_json).expect("pack json should remain valid");
    assert_eq!(
        pack_value["findings"][0]["title"],
        "Approval batch survives stale refresh"
    );
}

#[test]
fn worker_contract_rejects_stale_nonce_and_wrong_invocation_binding() {
    let task = sample_task();

    let mut stale_result = sample_result();
    stale_result.task_nonce = "stale-nonce".to_owned();
    assert_eq!(
        task.validate_stage_result(&stale_result),
        Err(ReviewWorkerContractError::ResultNonceMismatch {
            expected: "nonce-1".to_owned(),
            found: "stale-nonce".to_owned(),
        })
    );

    let mut wrong_invocation = sample_result();
    wrong_invocation.worker_invocation_id = Some("worker-invocation-2".to_owned());
    assert_eq!(
        task.validate_worker_invocation_binding(&wrong_invocation, "worker-invocation-1"),
        Err(ReviewWorkerContractError::ResultWorkerInvocationMismatch {
            expected: "worker-invocation-1".to_owned(),
            found: "worker-invocation-2".to_owned(),
        })
    );
}

#[test]
fn worker_contract_rejects_missing_stage_result_submission_capability() {
    let task = sample_task();
    let mut capability = sample_capability_profile();
    capability.supports_stage_result_submission = false;

    assert_eq!(
        task.validate_capability_profile(&capability),
        Err(ReviewWorkerContractError::StageResultSubmissionUnsupported)
    );
}
