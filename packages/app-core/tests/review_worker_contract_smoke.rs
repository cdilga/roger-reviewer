use roger_app_core::{
    RecallSourceRef, ReviewTarget, ReviewTask, ReviewTaskKind, ReviewWorkerContractError,
    WORKER_OPERATION_REQUEST_SCHEMA_V1, WORKER_STAGE_RESULT_SCHEMA_V1, WorkerArtifactExcerpt,
    WorkerArtifactRef, WorkerCapabilityProfile, WorkerContextPacket, WorkerEvidenceLocation,
    WorkerFindingDetail, WorkerFindingListResponse, WorkerFindingSummary, WorkerGitHubPosture,
    WorkerInvocation, WorkerInvocationOutcomeState, WorkerMemoryCard, WorkerMutationPosture,
    WorkerOperation, WorkerOperationDenial, WorkerOperationDenialCode, WorkerOperationLane,
    WorkerOperationRequestEnvelope, WorkerOperationResponseEnvelope, WorkerOperationResponseStatus,
    WorkerRecallEnvelope, WorkerSearchMemoryRequest, WorkerSearchMemoryResponse,
    WorkerStageOutcome, WorkerStageResult, WorkerStatusSnapshot, WorkerToolCallEvent,
    WorkerToolCallOutcomeState, WorkerTransportKind, WorkerTurnStrategy,
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

fn sample_operation_request(operation: &str) -> WorkerOperationRequestEnvelope {
    WorkerOperationRequestEnvelope {
        schema_id: WORKER_OPERATION_REQUEST_SCHEMA_V1.to_owned(),
        review_session_id: "session-1".to_owned(),
        review_run_id: "run-1".to_owned(),
        review_task_id: "task-1".to_owned(),
        task_nonce: "nonce-1".to_owned(),
        operation: operation.to_owned(),
        requested_scopes: Vec::new(),
        payload: None,
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
fn worker_operation_contract_authorizes_read_and_proposal_lanes() {
    let task = sample_task();
    let capability = sample_capability_profile();

    let mut search_request = sample_operation_request("worker.search_memory");
    search_request.requested_scopes = vec!["repo".to_owned()];
    search_request.payload = Some(
        serde_json::to_value(WorkerSearchMemoryRequest {
            query_text: "approval invalidation refresh".to_owned(),
            query_mode: "auto".to_owned(),
            requested_retrieval_classes: vec![
                "promoted_memory".to_owned(),
                "evidence_hits".to_owned(),
            ],
            anchor_hints: vec!["finding-1".to_owned()],
        })
        .expect("serialize search payload"),
    );

    let search_auth = task
        .validate_operation_request(&search_request, &capability)
        .expect("search request should authorize");
    assert_eq!(search_auth.operation, WorkerOperation::SearchMemory);
    assert_eq!(search_auth.lane, WorkerOperationLane::Read);
    assert_eq!(search_auth.granted_scopes, vec!["repo".to_owned()]);
    assert!(!search_auth.advisory_only);

    let response = WorkerOperationResponseEnvelope::success(
        &search_request,
        search_auth.clone(),
        Some(
            serde_json::to_value(WorkerSearchMemoryResponse {
                requested_query_mode: "auto".to_owned(),
                resolved_query_mode: "recall".to_owned(),
                retrieval_mode: "hybrid".to_owned(),
                degraded_flags: Vec::new(),
                promoted_memory: vec![WorkerRecallEnvelope {
                    item_kind: "promoted_memory".to_owned(),
                    item_id: "memory-1".to_owned(),
                    requested_query_mode: "auto".to_owned(),
                    resolved_query_mode: "recall".to_owned(),
                    retrieval_mode: "hybrid".to_owned(),
                    scope_bucket: "repository".to_owned(),
                    memory_lane: "promoted_memory".to_owned(),
                    trust_state: Some("established".to_owned()),
                    source_refs: vec![
                        RecallSourceRef {
                            kind: "memory".to_owned(),
                            id: "memory-1".to_owned(),
                        },
                        RecallSourceRef {
                            kind: "scope".to_owned(),
                            id: "repo:owner/repo".to_owned(),
                        },
                    ],
                    locator: json!({
                        "scope_key": "repo:owner/repo",
                        "memory_class": "fact",
                        "state": "established"
                    }),
                    snippet_or_summary: "Prior approval invalidation regression".to_owned(),
                    anchor_overlap_summary: "no anchor hints supplied".to_owned(),
                    degraded_flags: Vec::new(),
                    explain_summary: "promoted_memory surfaced from promoted_memory in repository with requested query_mode auto, resolved query_mode recall, retrieval_mode hybrid, posture cite_allowed/ordinary; no degraded flags".to_owned(),
                    citation_posture: "cite_allowed".to_owned(),
                    surface_posture: "ordinary".to_owned(),
                }],
                tentative_candidates: Vec::new(),
                evidence_hits: vec![WorkerRecallEnvelope {
                    item_kind: "evidence_finding".to_owned(),
                    item_id: "finding-1".to_owned(),
                    requested_query_mode: "auto".to_owned(),
                    resolved_query_mode: "recall".to_owned(),
                    retrieval_mode: "hybrid".to_owned(),
                    scope_bucket: "repository".to_owned(),
                    memory_lane: "evidence_hits".to_owned(),
                    trust_state: None,
                    source_refs: vec![
                        RecallSourceRef {
                            kind: "finding".to_owned(),
                            id: "finding-1".to_owned(),
                        },
                        RecallSourceRef {
                            kind: "review_session".to_owned(),
                            id: "session-1".to_owned(),
                        },
                    ],
                    locator: json!({
                        "session_id": "session-1",
                        "review_run_id": "run-1",
                        "repository": "owner/repo",
                        "pull_request": 42
                    }),
                    snippet_or_summary: "Approval batch survives stale refresh".to_owned(),
                    anchor_overlap_summary: "no anchor hints supplied".to_owned(),
                    degraded_flags: Vec::new(),
                    explain_summary: "evidence_finding surfaced from evidence_hits in repository with requested query_mode auto, resolved query_mode recall, retrieval_mode hybrid, posture cite_allowed/ordinary; no degraded flags".to_owned(),
                    citation_posture: "cite_allowed".to_owned(),
                    surface_posture: "ordinary".to_owned(),
                }],
            })
            .expect("serialize search response"),
        ),
    );
    assert_eq!(response.status, WorkerOperationResponseStatus::Succeeded);
    assert_eq!(response.authorization, Some(search_auth));
    assert_eq!(response.denial, None);

    let search_payload: WorkerSearchMemoryResponse =
        serde_json::from_value(response.payload.clone().expect("response payload"))
            .expect("deserialize search response");
    assert_eq!(search_payload.requested_query_mode, "auto");
    assert_eq!(search_payload.resolved_query_mode, "recall");
    assert_eq!(search_payload.promoted_memory.len(), 1);
    assert_eq!(search_payload.evidence_hits.len(), 1);
    assert_eq!(
        search_payload.promoted_memory[0].memory_lane,
        "promoted_memory"
    );
    assert_eq!(
        search_payload.promoted_memory[0].citation_posture,
        "cite_allowed"
    );
    assert_eq!(
        search_payload.promoted_memory[0].surface_posture,
        "ordinary"
    );
    assert_eq!(
        search_payload.promoted_memory[0].requested_query_mode,
        search_payload.requested_query_mode
    );
    assert_eq!(search_payload.evidence_hits[0].memory_lane, "evidence_hits");
    assert!(search_payload.evidence_hits[0].locator.is_object());
    assert!(
        search_payload.evidence_hits[0]
            .explain_summary
            .contains("retrieval_mode hybrid")
    );

    let round_trip: WorkerOperationResponseEnvelope =
        serde_json::from_str(&serde_json::to_string(&response).expect("serialize response"))
            .expect("deserialize response");
    assert_eq!(round_trip, response);

    let clarification_request = sample_operation_request("worker.request_clarification");
    let clarification_auth = task
        .validate_operation_request(&clarification_request, &capability)
        .expect("clarification request should authorize");
    assert_eq!(
        clarification_auth.operation,
        WorkerOperation::RequestClarification
    );
    assert_eq!(clarification_auth.lane, WorkerOperationLane::Proposal);
    assert!(clarification_auth.advisory_only);
}

#[test]
fn worker_search_response_keeps_recovery_scan_and_candidate_posture_explicit() {
    let task = sample_task();
    let capability = sample_capability_profile();

    let mut search_request = sample_operation_request("worker.search_memory");
    search_request.requested_scopes = vec!["repo".to_owned()];
    search_request.payload = Some(
        serde_json::to_value(WorkerSearchMemoryRequest {
            query_text: "approval refresh".to_owned(),
            query_mode: "candidate_audit".to_owned(),
            requested_retrieval_classes: vec![
                "promoted_memory".to_owned(),
                "tentative_candidates".to_owned(),
            ],
            anchor_hints: vec!["finding-1".to_owned()],
        })
        .expect("serialize search payload"),
    );

    let search_auth = task
        .validate_operation_request(&search_request, &capability)
        .expect("search request should authorize");
    let degraded_reason =
        "lexical sidecar unavailable or stale; using canonical DB lexical scan".to_owned();
    let response = WorkerOperationResponseEnvelope::success(
        &search_request,
        search_auth,
        Some(
            serde_json::to_value(WorkerSearchMemoryResponse {
                requested_query_mode: "candidate_audit".to_owned(),
                resolved_query_mode: "candidate_audit".to_owned(),
                retrieval_mode: "recovery_scan".to_owned(),
                degraded_flags: vec![degraded_reason.clone()],
                promoted_memory: vec![WorkerRecallEnvelope {
                    item_kind: "promoted_memory".to_owned(),
                    item_id: "memory-1".to_owned(),
                    requested_query_mode: "candidate_audit".to_owned(),
                    resolved_query_mode: "candidate_audit".to_owned(),
                    retrieval_mode: "recovery_scan".to_owned(),
                    scope_bucket: "repository".to_owned(),
                    memory_lane: "promoted_memory".to_owned(),
                    trust_state: Some("proven".to_owned()),
                    source_refs: vec![
                        RecallSourceRef {
                            kind: "memory".to_owned(),
                            id: "memory-1".to_owned(),
                        },
                        RecallSourceRef {
                            kind: "scope".to_owned(),
                            id: "repo:owner/repo".to_owned(),
                        },
                    ],
                    locator: json!({
                        "scope_key": "repo:owner/repo",
                        "memory_class": "procedural",
                        "state": "proven"
                    }),
                    snippet_or_summary: "approval refresh should reconfirm posting safety"
                        .to_owned(),
                    anchor_overlap_summary: "1 anchor hint(s) supplied; overlap scoring is unavailable for this record".to_owned(),
                    degraded_flags: vec![degraded_reason.clone()],
                    explain_summary: "promoted_memory surfaced from promoted_memory in repository with requested query_mode candidate_audit, resolved query_mode candidate_audit, retrieval_mode recovery_scan, posture cite_allowed/ordinary; degraded flags: lexical sidecar unavailable or stale; using canonical DB lexical scan".to_owned(),
                    citation_posture: "cite_allowed".to_owned(),
                    surface_posture: "ordinary".to_owned(),
                }],
                tentative_candidates: vec![WorkerRecallEnvelope {
                    item_kind: "candidate_memory".to_owned(),
                    item_id: "memory-candidate-1".to_owned(),
                    requested_query_mode: "candidate_audit".to_owned(),
                    resolved_query_mode: "candidate_audit".to_owned(),
                    retrieval_mode: "recovery_scan".to_owned(),
                    scope_bucket: "repository".to_owned(),
                    memory_lane: "tentative_candidates".to_owned(),
                    trust_state: Some("candidate".to_owned()),
                    source_refs: vec![
                        RecallSourceRef {
                            kind: "memory".to_owned(),
                            id: "memory-candidate-1".to_owned(),
                        },
                        RecallSourceRef {
                            kind: "scope".to_owned(),
                            id: "repo:owner/repo".to_owned(),
                        },
                    ],
                    locator: json!({
                        "scope_key": "repo:owner/repo",
                        "memory_class": "semantic",
                        "state": "candidate"
                    }),
                    snippet_or_summary:
                        "approval token stale refresh might need operator triage".to_owned(),
                    anchor_overlap_summary:
                        "1 anchor hint(s) supplied; overlap scoring is unavailable for this record"
                            .to_owned(),
                    degraded_flags: vec![degraded_reason.clone()],
                    explain_summary: "candidate_memory surfaced from tentative_candidates in repository with requested query_mode candidate_audit, resolved query_mode candidate_audit, retrieval_mode recovery_scan, posture inspect_only/candidate_review; degraded flags: lexical sidecar unavailable or stale; using canonical DB lexical scan".to_owned(),
                    citation_posture: "inspect_only".to_owned(),
                    surface_posture: "candidate_review".to_owned(),
                }],
                evidence_hits: Vec::new(),
            })
            .expect("serialize search response"),
        ),
    );

    let search_payload: WorkerSearchMemoryResponse =
        serde_json::from_value(response.payload.clone().expect("response payload"))
            .expect("deserialize search response");
    assert_eq!(search_payload.requested_query_mode, "candidate_audit");
    assert_eq!(search_payload.resolved_query_mode, "candidate_audit");
    assert_eq!(search_payload.retrieval_mode, "recovery_scan");
    assert_eq!(search_payload.degraded_flags, vec![degraded_reason]);
    assert_eq!(search_payload.promoted_memory.len(), 1);
    assert_eq!(search_payload.tentative_candidates.len(), 1);
    assert_eq!(
        search_payload.promoted_memory[0].citation_posture,
        "cite_allowed"
    );
    assert_eq!(
        search_payload.tentative_candidates[0].citation_posture,
        "inspect_only"
    );
    assert_eq!(
        search_payload.tentative_candidates[0].surface_posture,
        "candidate_review"
    );
    assert_eq!(
        search_payload.tentative_candidates[0].memory_lane,
        "tentative_candidates"
    );
    assert!(
        search_payload.tentative_candidates[0]
            .explain_summary
            .contains("retrieval_mode recovery_scan")
    );
}

#[test]
fn worker_read_contract_payloads_round_trip_as_roger_owned_types() {
    let finding = WorkerFindingSummary {
        finding_id: "finding-1".to_owned(),
        fingerprint: "fp-1".to_owned(),
        summary: "Approval invalidation can be skipped after stale refresh.".to_owned(),
        triage_state: "new".to_owned(),
        outbound_state: "not_drafted".to_owned(),
        primary_evidence_ref: Some("artifact-1".to_owned()),
    };

    let list = WorkerFindingListResponse {
        items: vec![finding.clone()],
    };
    let list_round_trip: WorkerFindingListResponse =
        serde_json::from_str(&serde_json::to_string(&list).expect("serialize finding list"))
            .expect("deserialize finding list");
    assert_eq!(list_round_trip, list);

    let detail = WorkerFindingDetail {
        finding: finding.clone(),
        evidence_locations: vec![WorkerEvidenceLocation {
            artifact_id: "artifact-1".to_owned(),
            repo_rel_path: Some("packages/cli/src/lib.rs".to_owned()),
            start_line: Some(1200),
            end_line: Some(1218),
            evidence_role: Some("primary".to_owned()),
        }],
        clarification_ids: vec!["clarify-1".to_owned()],
        outbound_draft_ids: vec!["draft-1".to_owned()],
    };
    let detail_round_trip: WorkerFindingDetail =
        serde_json::from_str(&serde_json::to_string(&detail).expect("serialize finding detail"))
            .expect("deserialize finding detail");
    assert_eq!(detail_round_trip, detail);

    let excerpt = WorkerArtifactExcerpt {
        artifact_id: "artifact-1".to_owned(),
        excerpt: "if approval token is stale { ... }".to_owned(),
        digest: Some("sha256:abc".to_owned()),
        truncated: false,
        byte_count: 34,
    };
    let excerpt_round_trip: WorkerArtifactExcerpt =
        serde_json::from_str(&serde_json::to_string(&excerpt).expect("serialize excerpt"))
            .expect("deserialize excerpt");
    assert_eq!(excerpt_round_trip, excerpt);

    let status = WorkerStatusSnapshot {
        review_session_id: "session-1".to_owned(),
        review_run_id: "run-1".to_owned(),
        attention_state: "needs_review".to_owned(),
        continuity_summary: Some("resume bundle remains usable".to_owned()),
        degraded_flags: vec!["lexical_only_search".to_owned()],
        unresolved_finding_count: 1,
        pending_clarification_count: 1,
        draft_count: 0,
    };
    let status_round_trip: WorkerStatusSnapshot =
        serde_json::from_str(&serde_json::to_string(&status).expect("serialize status"))
            .expect("deserialize status");
    assert_eq!(status_round_trip, status);
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
fn worker_operation_contract_denies_scope_escalation_and_unknown_mutation_ops() {
    let task = sample_task();
    let capability = sample_capability_profile();

    let mut scope_request = sample_operation_request("worker.search_memory");
    scope_request.requested_scopes = vec!["org".to_owned()];
    assert_eq!(
        task.validate_operation_request(&scope_request, &capability),
        Err(ReviewWorkerContractError::ScopeEscalationDenied {
            requested: "org".to_owned(),
            allowed: vec!["repo".to_owned()],
        })
    );

    let unsupported_request = sample_operation_request("worker.promote_memory");
    assert_eq!(
        task.validate_operation_request(&unsupported_request, &capability),
        Err(ReviewWorkerContractError::UnsupportedOperation {
            operation: "worker.promote_memory".to_owned(),
        })
    );

    let denied = WorkerOperationResponseEnvelope::denied(
        &unsupported_request,
        WorkerOperationDenial {
            code: WorkerOperationDenialCode::UnsupportedOperation,
            message: "worker.promote_memory is not part of the review-mode worker API".to_owned(),
            denied_scopes: Vec::new(),
        },
        vec!["proposal-only memory review remains manager-owned".to_owned()],
    );
    assert_eq!(denied.status, WorkerOperationResponseStatus::Denied);
    assert_eq!(denied.authorization, None);
    assert_eq!(
        denied.denial,
        Some(WorkerOperationDenial {
            code: WorkerOperationDenialCode::UnsupportedOperation,
            message: "worker.promote_memory is not part of the review-mode worker API".to_owned(),
            denied_scopes: Vec::new(),
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

#[test]
fn worker_operation_contract_rejects_out_of_policy_and_stale_requests() {
    let mut task = sample_task();
    let capability = sample_capability_profile();

    let mut stale_request = sample_operation_request("worker.get_status");
    stale_request.task_nonce = "stale-nonce".to_owned();
    assert_eq!(
        task.validate_operation_request(&stale_request, &capability),
        Err(ReviewWorkerContractError::RequestNonceMismatch {
            expected: "nonce-1".to_owned(),
            found: "stale-nonce".to_owned(),
        })
    );

    task.allowed_operations = vec!["worker.get_review_context".to_owned()];
    let search_request = sample_operation_request("worker.search_memory");
    assert_eq!(
        task.validate_operation_request(&search_request, &capability),
        Err(ReviewWorkerContractError::OperationNotAllowed {
            operation: "worker.search_memory".to_owned(),
        })
    );

    let mut memory_capability = sample_capability_profile();
    memory_capability.supports_memory_search = false;
    let full_task = sample_task();
    assert_eq!(
        full_task.validate_operation_request(&search_request, &memory_capability),
        Err(ReviewWorkerContractError::OperationCapabilityUnsupported {
            operation: "worker.search_memory".to_owned(),
            capability: "supports_memory_search".to_owned(),
        })
    );
}
