use roger_app_core::{
    AGENT_TRANSPORT_REQUEST_SCHEMA_V1, AGENT_TRANSPORT_RESPONSE_SCHEMA_V1, AgentTransportErrorCode,
    AgentTransportRequestEnvelope, AgentTransportResponseStatus, RecallSourceRef, ReviewTarget,
    ReviewTask, ReviewTaskKind, SearchPlanInput, SessionBaselineSnapshot,
    WORKER_OPERATION_REQUEST_SCHEMA_V1, WORKER_STAGE_RESULT_SCHEMA_V1, WorkerArtifactExcerpt,
    WorkerCapabilityProfile, WorkerContextPacket, WorkerFindingDetail, WorkerFindingDetailRequest,
    WorkerFindingSummary, WorkerGatewaySnapshot, WorkerGitHubPosture, WorkerMutationPosture,
    WorkerOperationDenialCode, WorkerOperationResponseStatus, WorkerRecallEnvelope,
    WorkerSearchMemoryRequest, WorkerSearchMemoryResponse, WorkerStageOutcome, WorkerStageResult,
    WorkerTransportKind, WorkerTurnStrategy, execute_agent_transport_request,
    materialize_search_plan,
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

fn sample_context(task: &ReviewTask) -> WorkerContextPacket {
    WorkerContextPacket {
        review_target: sample_target(),
        review_session_id: task.review_session_id.clone(),
        review_run_id: task.review_run_id.clone(),
        review_task_id: task.id.clone(),
        task_nonce: task.task_nonce.clone(),
        baseline_snapshot_ref: Some("baseline-1".to_owned()),
        baseline_snapshot: Some(sample_baseline_snapshot(task)),
        provider: "opencode".to_owned(),
        transport_kind: WorkerTransportKind::AgentCli,
        stage: task.stage.clone(),
        objective: task.objective.clone(),
        allowed_scopes: task.allowed_scopes.clone(),
        allowed_operations: task.allowed_operations.clone(),
        mutation_posture: WorkerMutationPosture::ReviewOnly,
        github_posture: WorkerGitHubPosture::Blocked,
        unresolved_findings: vec![sample_finding_summary()],
        continuity_summary: Some("provider session continuity is usable".to_owned()),
        memory_cards: Vec::new(),
        artifact_refs: Vec::new(),
    }
}

fn sample_baseline_snapshot(task: &ReviewTask) -> SessionBaselineSnapshot {
    SessionBaselineSnapshot {
        id: "baseline-1".to_owned(),
        review_session_id: task.review_session_id.clone(),
        review_run_id: Some(task.review_run_id.clone()),
        baseline_generation: 1,
        review_target_snapshot: sample_target(),
        allowed_scopes: task.allowed_scopes.clone(),
        default_query_mode: "recall".to_owned(),
        candidate_visibility_policy: "review_only".to_owned(),
        prompt_strategy: "preset:preset-deep-review/single_turn_report".to_owned(),
        policy_epoch_refs: vec!["config:cfg-1".to_owned()],
        degraded_flags: Vec::new(),
        created_at: 100,
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

fn sample_finding_summary() -> WorkerFindingSummary {
    WorkerFindingSummary {
        finding_id: "finding-1".to_owned(),
        fingerprint: "fp-1".to_owned(),
        summary: "An approval token may survive head drift.".to_owned(),
        triage_state: "new".to_owned(),
        outbound_state: "not_drafted".to_owned(),
        primary_evidence_ref: Some("artifact-1".to_owned()),
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
        summary: "Found one likely correctness issue.".to_owned(),
        structured_findings_pack: Some(json!({
            "schema_version": "structured_findings_pack.v1",
            "findings": [
                {
                    "title": "Approval batch survives stale refresh",
                    "summary": "The invalidation path drops a stale batch signal and reports success.",
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
        warnings: vec!["provider required fallback context read".to_owned()],
    }
}

fn request(
    operation: &str,
    payload: Option<serde_json::Value>,
) -> roger_app_core::WorkerOperationRequestEnvelope {
    roger_app_core::WorkerOperationRequestEnvelope {
        schema_id: WORKER_OPERATION_REQUEST_SCHEMA_V1.to_owned(),
        review_session_id: "session-1".to_owned(),
        review_run_id: "run-1".to_owned(),
        review_task_id: "task-1".to_owned(),
        task_nonce: "nonce-1".to_owned(),
        operation: operation.to_owned(),
        requested_scopes: vec!["repo".to_owned()],
        payload,
    }
}

fn sample_search_plan(
    query_text: &str,
    query_mode: &str,
    requested_retrieval_classes: &[&str],
    anchor_hints: &[&str],
) -> roger_app_core::SearchPlan {
    let target = sample_target();
    let task = sample_task();
    let granted_scopes = vec!["repo".to_owned()];
    let requested_retrieval_classes = requested_retrieval_classes
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<Vec<_>>();
    let anchor_hints = anchor_hints
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<Vec<_>>();

    materialize_search_plan(SearchPlanInput {
        review_session_id: Some(&task.review_session_id),
        review_run_id: Some(&task.review_run_id),
        repository: &target.repository,
        granted_scopes: &granted_scopes,
        query_text,
        query_mode: Some(query_mode),
        requested_retrieval_classes: &requested_retrieval_classes,
        anchor_hints: &anchor_hints,
        supports_candidate_audit: true,
        supports_promotion_review: false,
        semantic_assets_verified: false,
    })
    .expect("sample search plan should materialize")
}

fn transport_request(
    operation_request: roger_app_core::WorkerOperationRequestEnvelope,
) -> AgentTransportRequestEnvelope {
    AgentTransportRequestEnvelope {
        schema_id: AGENT_TRANSPORT_REQUEST_SCHEMA_V1.to_owned(),
        review_task: sample_task(),
        worker_context: sample_context(&sample_task()),
        capability_profile: sample_capability_profile(),
        operation_request,
        gateway_snapshot: WorkerGatewaySnapshot::default(),
    }
}

#[test]
fn agent_transport_returns_context_in_dedicated_envelope() {
    let response = execute_agent_transport_request(&transport_request(request(
        "worker.get_review_context",
        None,
    )));

    assert_eq!(response.schema_id, AGENT_TRANSPORT_RESPONSE_SCHEMA_V1);
    assert_eq!(response.transport_kind, WorkerTransportKind::AgentCli);
    assert_eq!(response.status, AgentTransportResponseStatus::Succeeded);

    let operation_response = response.operation_response.expect("operation response");
    assert_eq!(
        operation_response.status,
        WorkerOperationResponseStatus::Succeeded
    );
    assert_eq!(operation_response.operation, "worker.get_review_context");
    assert_eq!(
        operation_response.payload.expect("context payload"),
        serde_json::to_value(sample_context(&sample_task())).expect("serialize context")
    );
}

#[test]
fn agent_transport_returns_search_memory_payload_for_planned_query() {
    let search_request = WorkerSearchMemoryRequest {
        query_text: "approval invalidation".to_owned(),
        query_mode: "auto".to_owned(),
        requested_retrieval_classes: vec!["promoted_memory".to_owned()],
        anchor_hints: vec!["finding-1".to_owned()],
    };
    let mut request = transport_request(request(
        "worker.search_memory",
        Some(serde_json::to_value(search_request).expect("serialize search request")),
    ));
    let search_plan = sample_search_plan(
        "approval invalidation",
        "auto",
        &["promoted_memory"],
        &["finding-1"],
    );
    request.gateway_snapshot.search_memory_response = Some(WorkerSearchMemoryResponse {
        requested_query_mode: "auto".to_owned(),
        resolved_query_mode: "related_context".to_owned(),
        search_plan,
        retrieval_mode: "lexical_only".to_owned(),
        degraded_flags: Vec::new(),
        promoted_memory: vec![WorkerRecallEnvelope {
            item_kind: "promoted_memory".to_owned(),
            item_id: "memory-1".to_owned(),
            requested_query_mode: "auto".to_owned(),
            resolved_query_mode: "related_context".to_owned(),
            retrieval_mode: "lexical_only".to_owned(),
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
            anchor_overlap_summary: "1 anchor hint(s) supplied; overlap scoring is unavailable for this record".to_owned(),
            degraded_flags: Vec::new(),
            explain_summary: "promoted_memory surfaced from promoted_memory in repository with requested query_mode auto, resolved query_mode related_context, retrieval_mode lexical_only, posture cite_allowed/ordinary; no degraded flags".to_owned(),
            citation_posture: "cite_allowed".to_owned(),
            surface_posture: "ordinary".to_owned(),
        }],
        tentative_candidates: Vec::new(),
        evidence_hits: Vec::new(),
    });

    let response = execute_agent_transport_request(&request);
    assert_eq!(response.status, AgentTransportResponseStatus::Succeeded);
    let payload = response
        .operation_response
        .and_then(|item| item.payload)
        .expect("search payload");
    assert_eq!(payload["resolved_query_mode"], "related_context");
    assert_eq!(
        payload["search_plan"]["query_plan"]["candidate_visibility"],
        "hidden"
    );
    assert_eq!(
        payload["search_plan"]["retrieval_classes"],
        json!(["promoted_memory"])
    );
    assert_eq!(
        payload["promoted_memory"][0]["citation_posture"],
        "cite_allowed"
    );
}

#[test]
fn agent_transport_preserves_degraded_recovery_search_truth() {
    let search_request = WorkerSearchMemoryRequest {
        query_text: "approval refresh".to_owned(),
        query_mode: "candidate_audit".to_owned(),
        requested_retrieval_classes: vec![
            "promoted_memory".to_owned(),
            "tentative_candidates".to_owned(),
        ],
        anchor_hints: vec!["finding-1".to_owned()],
    };
    let mut request = transport_request(request(
        "worker.search_memory",
        Some(serde_json::to_value(search_request).expect("serialize search request")),
    ));
    let degraded_reason =
        "lexical sidecar unavailable or stale; using canonical DB lexical scan".to_owned();
    let search_plan = sample_search_plan(
        "approval refresh",
        "candidate_audit",
        &["promoted_memory", "tentative_candidates"],
        &["finding-1"],
    );
    request.gateway_snapshot.search_memory_response = Some(WorkerSearchMemoryResponse {
        requested_query_mode: "candidate_audit".to_owned(),
        resolved_query_mode: "candidate_audit".to_owned(),
        search_plan,
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
            snippet_or_summary: "Prior approval invalidation regression".to_owned(),
            anchor_overlap_summary:
                "1 anchor hint(s) supplied; overlap scoring is unavailable for this record"
                    .to_owned(),
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
            snippet_or_summary: "Candidate needs operator review".to_owned(),
            anchor_overlap_summary:
                "1 anchor hint(s) supplied; overlap scoring is unavailable for this record"
                    .to_owned(),
            degraded_flags: vec![degraded_reason.clone()],
            explain_summary: "candidate_memory surfaced from tentative_candidates in repository with requested query_mode candidate_audit, resolved query_mode candidate_audit, retrieval_mode recovery_scan, posture inspect_only/candidate_review; degraded flags: lexical sidecar unavailable or stale; using canonical DB lexical scan".to_owned(),
            citation_posture: "inspect_only".to_owned(),
            surface_posture: "candidate_review".to_owned(),
        }],
        evidence_hits: Vec::new(),
    });

    let response = execute_agent_transport_request(&request);
    assert_eq!(response.status, AgentTransportResponseStatus::Succeeded);
    let payload = response
        .operation_response
        .and_then(|item| item.payload)
        .expect("search payload");
    assert_eq!(payload["requested_query_mode"], "candidate_audit");
    assert_eq!(payload["resolved_query_mode"], "candidate_audit");
    assert_eq!(payload["retrieval_mode"], "recovery_scan");
    assert_eq!(payload["degraded_flags"][0], degraded_reason);
    assert_eq!(
        payload["tentative_candidates"][0]["citation_posture"],
        "inspect_only"
    );
    assert_eq!(
        payload["tentative_candidates"][0]["surface_posture"],
        "candidate_review"
    );
    assert_eq!(
        payload["tentative_candidates"][0]["memory_lane"],
        "tentative_candidates"
    );
    assert_eq!(
        payload["search_plan"]["retrieval_strategy"]["semantic"],
        false
    );
    assert!(
        payload["tentative_candidates"][0]["explain_summary"]
            .as_str()
            .expect("candidate explain summary")
            .contains("retrieval_mode recovery_scan")
    );
}

#[test]
fn agent_transport_rejects_search_payload_that_widens_past_search_plan() {
    let search_request = WorkerSearchMemoryRequest {
        query_text: "approval invalidation".to_owned(),
        query_mode: "recall".to_owned(),
        requested_retrieval_classes: vec!["promoted_memory".to_owned()],
        anchor_hints: Vec::new(),
    };
    let mut request = transport_request(request(
        "worker.search_memory",
        Some(serde_json::to_value(search_request).expect("serialize search request")),
    ));
    request.gateway_snapshot.search_memory_response = Some(WorkerSearchMemoryResponse {
        requested_query_mode: "recall".to_owned(),
        resolved_query_mode: "recall".to_owned(),
        search_plan: sample_search_plan("approval invalidation", "recall", &["promoted_memory"], &[]),
        retrieval_mode: "lexical_only".to_owned(),
        degraded_flags: Vec::new(),
        promoted_memory: Vec::new(),
        tentative_candidates: vec![WorkerRecallEnvelope {
            item_kind: "candidate_memory".to_owned(),
            item_id: "memory-candidate-1".to_owned(),
            requested_query_mode: "recall".to_owned(),
            resolved_query_mode: "recall".to_owned(),
            retrieval_mode: "lexical_only".to_owned(),
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
            snippet_or_summary: "Candidate should not appear in ordinary recall".to_owned(),
            anchor_overlap_summary: "no anchor hints supplied".to_owned(),
            degraded_flags: Vec::new(),
            explain_summary: "candidate_memory surfaced from tentative_candidates in repository with requested query_mode recall, resolved query_mode recall, retrieval_mode lexical_only, posture inspect_only/candidate_review; no degraded flags".to_owned(),
            citation_posture: "inspect_only".to_owned(),
            surface_posture: "candidate_review".to_owned(),
        }],
        evidence_hits: Vec::new(),
    });

    let response = execute_agent_transport_request(&request);
    assert_eq!(response.status, AgentTransportResponseStatus::Error);
    let error = response.error.expect("transport error");
    assert_eq!(error.code, AgentTransportErrorCode::ValidationFailed);
    assert!(
        error
            .message
            .contains("surfaced tentative_candidates outside the planned retrieval classes"),
        "{}",
        error.message
    );
}

#[test]
fn agent_transport_returns_stage_result_acceptance_summary() {
    let task = sample_task();
    let response = execute_agent_transport_request(&transport_request(request(
        "worker.submit_stage_result",
        Some(serde_json::to_value(sample_stage_result(&task)).expect("serialize stage result")),
    )));

    assert_eq!(response.status, AgentTransportResponseStatus::Succeeded);
    let payload = response
        .operation_response
        .and_then(|item| item.payload)
        .expect("stage-result acceptance payload");
    assert_eq!(payload["result_schema_id"], WORKER_STAGE_RESULT_SCHEMA_V1);
    assert_eq!(payload["structured_findings_pack_present"], true);
}

#[test]
fn agent_transport_supports_finding_detail_and_artifact_excerpt_reads() {
    let mut finding_request = transport_request(request(
        "worker.get_finding_detail",
        Some(
            serde_json::to_value(WorkerFindingDetailRequest {
                finding_id: "finding-1".to_owned(),
            })
            .expect("serialize detail request"),
        ),
    ));
    finding_request.gateway_snapshot.finding_details = vec![WorkerFindingDetail {
        finding: sample_finding_summary(),
        evidence_locations: Vec::new(),
        clarification_ids: Vec::new(),
        outbound_draft_ids: Vec::new(),
    }];

    let detail_response = execute_agent_transport_request(&finding_request);
    assert_eq!(
        detail_response.status,
        AgentTransportResponseStatus::Succeeded
    );

    let mut artifact_request = transport_request(request(
        "worker.get_artifact_excerpt",
        Some(json!({"artifact_id": "artifact-1"})),
    ));
    artifact_request.gateway_snapshot.artifact_excerpts = vec![WorkerArtifactExcerpt {
        artifact_id: "artifact-1".to_owned(),
        excerpt: "fn reconcile() {}".to_owned(),
        digest: Some("abc123".to_owned()),
        truncated: false,
        byte_count: 18,
    }];

    let artifact_response = execute_agent_transport_request(&artifact_request);
    assert_eq!(
        artifact_response.status,
        AgentTransportResponseStatus::Succeeded
    );
}

#[test]
fn agent_transport_fails_closed_on_nonce_mismatch() {
    let mut bad_request = request("worker.get_review_context", None);
    bad_request.task_nonce = "stale-nonce".to_owned();
    let response = execute_agent_transport_request(&transport_request(bad_request));

    assert_eq!(response.status, AgentTransportResponseStatus::Error);
    assert_eq!(
        response.error.expect("validation error").code,
        AgentTransportErrorCode::ValidationFailed
    );
}

#[test]
fn agent_transport_rejects_invalid_request_schema_and_transport_kind_mismatch() {
    let mut bad_schema = transport_request(request("worker.get_review_context", None));
    bad_schema.schema_id = "rr.agent.request.v0".to_owned();
    let schema_response = execute_agent_transport_request(&bad_schema);
    assert_eq!(schema_response.status, AgentTransportResponseStatus::Error);
    assert_eq!(
        schema_response.error.expect("schema error").code,
        AgentTransportErrorCode::InvalidRequestSchema
    );

    let mut bad_transport = transport_request(request("worker.get_review_context", None));
    bad_transport.worker_context.transport_kind = WorkerTransportKind::Mcp;
    let transport_response = execute_agent_transport_request(&bad_transport);
    assert_eq!(
        transport_response.status,
        AgentTransportResponseStatus::Error
    );
    assert_eq!(
        transport_response.error.expect("transport error").code,
        AgentTransportErrorCode::TransportKindMismatch
    );
}

#[test]
fn agent_transport_rejects_cross_session_context_packets() {
    let mut cross_session = transport_request(request("worker.get_review_context", None));
    cross_session.worker_context.review_session_id = "session-elsewhere".to_owned();
    let response = execute_agent_transport_request(&cross_session);

    assert_eq!(response.status, AgentTransportResponseStatus::Error);
    let error = response.error.expect("cross-session validation error");
    assert_eq!(error.code, AgentTransportErrorCode::ValidationFailed);
    assert!(
        error
            .message
            .contains("worker context packet review_session_id"),
        "{}",
        error.message
    );
}

#[test]
fn agent_transport_denies_hidden_posting_or_approval_operations() {
    let response = execute_agent_transport_request(&transport_request(request(
        "worker.post_to_github",
        Some(json!({"draft_id": "draft-1"})),
    )));

    assert_eq!(response.status, AgentTransportResponseStatus::Denied);
    let operation = response
        .operation_response
        .expect("denied operation response");
    assert_eq!(operation.status, WorkerOperationResponseStatus::Denied);
    let denial = operation.denial.expect("denial payload");
    assert_eq!(denial.code, WorkerOperationDenialCode::UnsupportedOperation);
    assert!(
        denial
            .message
            .contains("not part of the canonical rr agent API"),
        "{}",
        denial.message
    );
}
