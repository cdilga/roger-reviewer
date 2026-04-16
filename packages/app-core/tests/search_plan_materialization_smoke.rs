use roger_app_core::{
    AGENT_TRANSPORT_REQUEST_SCHEMA_V1, AgentTransportErrorCode, AgentTransportRequestEnvelope,
    AgentTransportResponseStatus, RecallSourceRef, ReviewTarget, ReviewTask, ReviewTaskKind,
    SearchAnchorSet, SearchCandidateVisibility, SearchPlanError, SearchPlanInput,
    SearchRetrievalClass, SearchRetrievalLane, SearchScopeSet, SearchSemanticPosture,
    SearchSemanticRuntimePosture, SearchSessionBaseline, SearchTrustFloor,
    WORKER_OPERATION_REQUEST_SCHEMA_V1, WorkerCapabilityProfile, WorkerContextPacket,
    WorkerGatewaySnapshot, WorkerGitHubPosture, WorkerMutationPosture,
    WorkerOperationRequestEnvelope, WorkerRecallEnvelope, WorkerSearchMemoryRequest,
    WorkerSearchMemoryResponse, WorkerTransportKind, WorkerTurnStrategy,
    execute_agent_transport_request, materialize_search_plan,
};
use serde_json::{Value, json};

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
        objective: "review memory retrieval truth".to_owned(),
        turn_strategy: WorkerTurnStrategy::SingleTurnReport,
        allowed_scopes: vec!["repo".to_owned()],
        allowed_operations: vec!["worker.search_memory".to_owned()],
        expected_result_schema: "rr.worker.stage_result.v1".to_owned(),
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
        provider: "opencode".to_owned(),
        transport_kind: WorkerTransportKind::AgentCli,
        stage: task.stage.clone(),
        objective: task.objective.clone(),
        allowed_scopes: task.allowed_scopes.clone(),
        allowed_operations: task.allowed_operations.clone(),
        mutation_posture: WorkerMutationPosture::ReviewOnly,
        github_posture: WorkerGitHubPosture::Blocked,
        unresolved_findings: Vec::new(),
        continuity_summary: Some("provider session continuity is usable".to_owned()),
        memory_cards: Vec::new(),
        artifact_refs: Vec::new(),
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

fn operation_request(
    payload: WorkerSearchMemoryRequest,
) -> WorkerOperationRequestEnvelope {
    WorkerOperationRequestEnvelope {
        schema_id: WORKER_OPERATION_REQUEST_SCHEMA_V1.to_owned(),
        review_session_id: "session-1".to_owned(),
        review_run_id: "run-1".to_owned(),
        review_task_id: "task-1".to_owned(),
        task_nonce: "nonce-1".to_owned(),
        operation: "worker.search_memory".to_owned(),
        requested_scopes: vec!["repo".to_owned()],
        payload: Some(serde_json::to_value(payload).expect("serialize search payload")),
    }
}

fn transport_request(
    payload: WorkerSearchMemoryRequest,
    response: WorkerSearchMemoryResponse,
) -> AgentTransportRequestEnvelope {
    let task = sample_task();
    AgentTransportRequestEnvelope {
        schema_id: AGENT_TRANSPORT_REQUEST_SCHEMA_V1.to_owned(),
        review_task: task.clone(),
        worker_context: sample_context(&task),
        capability_profile: sample_capability_profile(),
        operation_request: operation_request(payload),
        gateway_snapshot: WorkerGatewaySnapshot {
            search_memory_response: Some(response),
            ..WorkerGatewaySnapshot::default()
        },
    }
}

fn recall_envelope(
    item_kind: &str,
    item_id: &str,
    memory_lane: &str,
    requested_query_mode: &str,
    resolved_query_mode: &str,
    retrieval_mode: &str,
    citation_posture: &str,
    surface_posture: &str,
) -> WorkerRecallEnvelope {
    WorkerRecallEnvelope {
        item_kind: item_kind.to_owned(),
        item_id: item_id.to_owned(),
        requested_query_mode: requested_query_mode.to_owned(),
        resolved_query_mode: resolved_query_mode.to_owned(),
        retrieval_mode: retrieval_mode.to_owned(),
        scope_bucket: "repository".to_owned(),
        memory_lane: memory_lane.to_owned(),
        trust_state: Some("proven".to_owned()),
        source_refs: vec![
            RecallSourceRef {
                kind: "memory".to_owned(),
                id: item_id.to_owned(),
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
        snippet_or_summary: "approval refresh should reconfirm posting safety".to_owned(),
        anchor_overlap_summary: "no anchor hints supplied".to_owned(),
        degraded_flags: Vec::new(),
        explain_summary: format!(
            "{item_kind} surfaced from {memory_lane} in repository with requested query_mode {requested_query_mode}, resolved query_mode {resolved_query_mode}, retrieval_mode {retrieval_mode}, posture {citation_posture}/{surface_posture}; no degraded flags"
        ),
        citation_posture: citation_posture.to_owned(),
        surface_posture: surface_posture.to_owned(),
    }
}

fn sample_search_plan(
    query_text: &str,
    query_mode: &str,
    requested_retrieval_classes: &[&str],
    anchor_hints: &[&str],
) -> roger_app_core::SearchPlan {
    let retrieval_classes = requested_retrieval_classes
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<Vec<_>>();
    let anchors = anchor_hints
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<Vec<_>>();

    materialize_search_plan(SearchPlanInput {
        review_session_id: Some("session-1"),
        review_run_id: Some("run-1"),
        repository: "owner/repo",
        granted_scopes: &["repo".to_owned()],
        query_text,
        query_mode: Some(query_mode),
        requested_retrieval_classes: &retrieval_classes,
        anchor_hints: &anchors,
        supports_candidate_audit: true,
        supports_promotion_review: false,
        semantic_assets_verified: false,
    })
    .expect("sample search plan should materialize")
}

#[test]
fn search_plan_materialization_is_deterministic_and_inspectable() {
    let scopes = vec!["repo".to_owned(), "repo".to_owned(), "".to_owned()];
    let requested_classes: Vec<String> = Vec::new();
    let anchors: Vec<String> = Vec::new();

    let first = materialize_search_plan(SearchPlanInput {
        review_session_id: Some("session-1"),
        review_run_id: Some("run-1"),
        repository: "owner/repo",
        granted_scopes: &scopes,
        query_text: "approval invalidation refresh",
        query_mode: Some("recall"),
        requested_retrieval_classes: &requested_classes,
        anchor_hints: &anchors,
        supports_candidate_audit: true,
        supports_promotion_review: false,
        semantic_assets_verified: false,
    })
    .expect("materialize search plan");
    let second = materialize_search_plan(SearchPlanInput {
        review_session_id: Some("session-1"),
        review_run_id: Some("run-1"),
        repository: "owner/repo",
        granted_scopes: &scopes,
        query_text: "approval invalidation refresh",
        query_mode: Some("recall"),
        requested_retrieval_classes: &requested_classes,
        anchor_hints: &anchors,
        supports_candidate_audit: true,
        supports_promotion_review: false,
        semantic_assets_verified: false,
    })
    .expect("materialize same search plan twice");

    assert_eq!(first, second);
    assert_eq!(first.query_plan.scope_set, SearchScopeSet::CurrentRepository);
    assert_eq!(
        first.query_plan.session_baseline,
        SearchSessionBaseline::AmbientSessionOptional
    );
    assert_eq!(first.query_plan.anchor_set, SearchAnchorSet::None);
    assert_eq!(
        first.query_plan.trust_floor,
        SearchTrustFloor::PromotedAndEvidenceOnly
    );
    assert_eq!(
        first.query_plan.candidate_visibility,
        SearchCandidateVisibility::Hidden
    );
    assert_eq!(
        first.query_plan.semantic_posture,
        SearchSemanticPosture::DegradedSemanticVisible
    );
    assert_eq!(
        first.query_plan.strategy.primary_lane,
        SearchRetrievalLane::LexicalRecall
    );
    assert_eq!(first.granted_scopes, vec!["repo".to_owned()]);
    assert_eq!(first.scope_keys, vec!["repo:owner/repo".to_owned()]);
    assert_eq!(
        first.retrieval_classes,
        vec![
            SearchRetrievalClass::PromotedMemory,
            SearchRetrievalClass::EvidenceHits,
        ]
    );
    assert_eq!(
        first.semantic_runtime_posture,
        SearchSemanticRuntimePosture::DisabledPendingVerification
    );
    assert!(first.retrieval_strategy.lexical);
    assert!(first.retrieval_strategy.prior_review);
    assert!(!first.retrieval_strategy.semantic);
    assert!(!first.retrieval_strategy.candidate_audit);
    assert!(
        first
            .strategy_reason
            .contains("semantic retrieval is disabled until verified local semantic assets are available")
    );

    let encoded = serde_json::to_value(&first).expect("serialize search plan");
    assert_eq!(
        encoded,
        json!({
            "query_plan": {
                "requested_query_mode": "recall",
                "resolved_query_mode": "recall",
                "scope_set": "current_repository",
                "session_baseline": "ambient_session_optional",
                "anchor_set": "none",
                "trust_floor": "promoted_and_evidence_only",
                "candidate_visibility": "hidden",
                "semantic_posture": "degraded_semantic_visible",
                "strategy": {
                    "primary_lane": "lexical_recall",
                    "lexical": true,
                    "prior_review": true,
                    "semantic": true,
                    "candidate_audit": false,
                    "query_expansion": false
                }
            },
            "review_session_id": "session-1",
            "review_run_id": "run-1",
            "granted_scopes": ["repo"],
            "scope_keys": ["repo:owner/repo"],
            "retrieval_classes": ["promoted_memory", "evidence_hits"],
            "semantic_runtime_posture": "disabled_pending_verification",
            "retrieval_strategy": {
                "primary_lane": "lexical_recall",
                "lexical": true,
                "prior_review": true,
                "semantic": false,
                "candidate_audit": false,
                "query_expansion": false
            },
            "strategy_reason": Value::String(first.strategy_reason.clone())
        })
    );
}

#[test]
fn candidate_audit_requires_tentative_candidate_retrieval_class() {
    let scopes = vec!["repo".to_owned()];
    let requested_classes = vec!["promoted_memory".to_owned()];
    let anchors = vec!["finding-1".to_owned()];

    let err = materialize_search_plan(SearchPlanInput {
        review_session_id: Some("session-1"),
        review_run_id: Some("run-1"),
        repository: "owner/repo",
        granted_scopes: &scopes,
        query_text: "approval invalidation refresh",
        query_mode: Some("candidate_audit"),
        requested_retrieval_classes: &requested_classes,
        anchor_hints: &anchors,
        supports_candidate_audit: true,
        supports_promotion_review: false,
        semantic_assets_verified: false,
    })
    .expect_err("candidate audit must not silently hide tentative candidates");

    assert_eq!(
        err,
        SearchPlanError::CandidateAwareQueryRequiresTentativeCandidates {
            query_mode: "candidate_audit".to_owned(),
        }
    );
}

#[test]
fn transport_rejects_tentative_candidates_outside_planned_classes() {
    let request = WorkerSearchMemoryRequest {
        query_text: "approval invalidation refresh".to_owned(),
        query_mode: "recall".to_owned(),
        requested_retrieval_classes: vec!["promoted_memory".to_owned()],
        anchor_hints: Vec::new(),
    };
    let response = WorkerSearchMemoryResponse {
        requested_query_mode: "recall".to_owned(),
        resolved_query_mode: "recall".to_owned(),
        search_plan: sample_search_plan(
            "approval invalidation refresh",
            "recall",
            &["promoted_memory"],
            &[],
        ),
        retrieval_mode: "lexical_only".to_owned(),
        degraded_flags: Vec::new(),
        promoted_memory: Vec::new(),
        tentative_candidates: vec![recall_envelope(
            "candidate_memory",
            "memory-candidate-1",
            "tentative_candidates",
            "recall",
            "recall",
            "lexical_only",
            "inspect_only",
            "candidate_review",
        )],
        evidence_hits: Vec::new(),
    };

    let transport = execute_agent_transport_request(&transport_request(request, response));
    assert_eq!(transport.status, AgentTransportResponseStatus::Error);

    let error = transport.error.expect("validation error");
    assert_eq!(error.code, AgentTransportErrorCode::ValidationFailed);
    assert!(
        error
            .message
            .contains("surfaced tentative_candidates outside the planned retrieval classes")
    );
}

#[test]
fn transport_rejects_hybrid_retrieval_when_semantics_are_unverified() {
    let request = WorkerSearchMemoryRequest {
        query_text: "approval invalidation refresh".to_owned(),
        query_mode: "recall".to_owned(),
        requested_retrieval_classes: vec![
            "promoted_memory".to_owned(),
            "evidence_hits".to_owned(),
        ],
        anchor_hints: Vec::new(),
    };
    let response = WorkerSearchMemoryResponse {
        requested_query_mode: "recall".to_owned(),
        resolved_query_mode: "recall".to_owned(),
        search_plan: sample_search_plan(
            "approval invalidation refresh",
            "recall",
            &["promoted_memory", "evidence_hits"],
            &[],
        ),
        retrieval_mode: "hybrid".to_owned(),
        degraded_flags: Vec::new(),
        promoted_memory: vec![recall_envelope(
            "promoted_memory",
            "memory-1",
            "promoted_memory",
            "recall",
            "recall",
            "hybrid",
            "cite_allowed",
            "ordinary",
        )],
        tentative_candidates: Vec::new(),
        evidence_hits: vec![recall_envelope(
            "evidence_finding",
            "finding-1",
            "evidence_hits",
            "recall",
            "recall",
            "hybrid",
            "cite_allowed",
            "ordinary",
        )],
    };

    let transport = execute_agent_transport_request(&transport_request(request, response));
    assert_eq!(transport.status, AgentTransportResponseStatus::Error);

    let error = transport.error.expect("validation error");
    assert_eq!(error.code, AgentTransportErrorCode::ValidationFailed);
    assert!(
        error
            .message
            .contains("reported hybrid retrieval even though the search_plan disabled semantic retrieval")
    );
}
