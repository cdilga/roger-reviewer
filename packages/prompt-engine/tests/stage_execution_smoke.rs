use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use roger_app_core::{
    LaunchAction, LaunchIntent, ReviewTarget, ReviewTask, ReviewTaskKind, SessionBaselineSnapshot,
    SessionLocator, Surface, WORKER_STAGE_RESULT_SCHEMA_V1, WorkerCapabilityProfile,
    WorkerContextPacket, WorkerGitHubPosture, WorkerMutationPosture, WorkerStageOutcome,
    WorkerStageResult, WorkerToolCallEvent, WorkerToolCallOutcomeState, WorkerTransportKind,
    WorkerTurnStrategy,
};
use roger_prompt_engine::stage_execution::{
    PROMPT_INVOKED_EVENT_TYPE, ReviewStage, StageExecutionRequest, StageHarness,
    StageHarnessOutput, StageOutcomeMetadata, StagePrompt, execute_review_stage,
};
use roger_prompt_engine::{RepairAction, StageState};
use roger_storage::{CreateReviewRun, CreateReviewSession, RogerStore};
use tempfile::TempDir;

struct StubStageHarness {
    responses: HashMap<ReviewStage, StageHarnessOutput>,
    calls: RefCell<Vec<ReviewStage>>,
}

impl StubStageHarness {
    fn new(responses: HashMap<ReviewStage, StageHarnessOutput>) -> Self {
        Self {
            responses,
            calls: RefCell::new(Vec::new()),
        }
    }

    fn calls(&self) -> Vec<ReviewStage> {
        self.calls.borrow().clone()
    }
}

impl StageHarness for StubStageHarness {
    fn execute_stage(
        &self,
        _locator: &SessionLocator,
        review_task: &ReviewTask,
        _worker_context: &WorkerContextPacket,
        _capability_profile: &WorkerCapabilityProfile,
        worker_invocation_id: &str,
        _prompt_text: &str,
    ) -> std::result::Result<StageHarnessOutput, String> {
        let stage = match review_task.stage.as_str() {
            "exploration" => ReviewStage::Exploration,
            "deep_review" => ReviewStage::DeepReview,
            "follow_up" => ReviewStage::FollowUp,
            other => return Err(format!("unsupported review task stage {other}")),
        };
        self.calls.borrow_mut().push(stage);
        let mut response = self
            .responses
            .get(&stage)
            .cloned()
            .ok_or_else(|| format!("missing stub output for stage {}", stage.as_str()))?;
        response.worker_stage_result.worker_invocation_id = Some(worker_invocation_id.to_owned());
        for event in &mut response.tool_call_events {
            event.worker_invocation_id = worker_invocation_id.to_owned();
        }
        Ok(response)
    }
}

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

fn sample_intent() -> LaunchIntent {
    LaunchIntent {
        action: LaunchAction::ResumeReview,
        source_surface: Surface::Cli,
        objective: Some("review this PR".to_owned()),
        launch_profile_id: Some("profile-open-pr".to_owned()),
        cwd: Some("/tmp/repo".to_owned()),
        worktree_root: None,
    }
}

fn sample_locator() -> SessionLocator {
    SessionLocator {
        provider: "opencode".to_owned(),
        session_id: "oc-existing".to_owned(),
        invocation_context_json:
            r#"{"mode":"start","review_target":{"repository":"owner/repo","pull_request_number":42}}"#
                .to_owned(),
        captured_at: 10,
        last_tested_at: Some(11),
    }
}

fn setup_store(session_id: &str, run_id: &str) -> (TempDir, RogerStore) {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let store = RogerStore::open(tempdir.path()).expect("open store");
    let target = sample_target();
    let locator = sample_locator();

    store
        .create_review_session(CreateReviewSession {
            id: session_id,
            review_target: &target,
            provider: "opencode",
            session_locator: Some(&locator),
            resume_bundle_artifact_id: None,
            continuity_state: "usable",
            attention_state: "awaiting_review",
            launch_profile_id: Some("profile-open-pr"),
        })
        .expect("create review session");

    store
        .create_review_run(CreateReviewRun {
            id: run_id,
            session_id,
            run_kind: "deep_review",
            repo_snapshot: "head=bbb",
            continuity_quality: "usable",
            session_locator_artifact_id: None,
        })
        .expect("create review run");

    (tempdir, store)
}

fn valid_structured_pack(title: &str, summary: &str) -> String {
    format!(
        r#"{{
  "schema_version": "structured_findings_pack.v1",
  "findings": [
    {{
      "title": "{title}",
      "summary": "{summary}",
      "severity": "high",
      "confidence": "medium",
      "evidence": [
        {{ "repo_rel_path": "src/lib.rs", "start_line": 10 }}
      ]
    }}
  ]
}}"#
    )
}

fn sample_review_task(stage: ReviewStage, prompt_preset_id: &str) -> ReviewTask {
    let (task_id, task_kind) = match stage {
        ReviewStage::Exploration => ("task-exploration", ReviewTaskKind::ExplorationPass),
        ReviewStage::DeepReview => ("task-deep-review", ReviewTaskKind::DeepReviewPass),
        ReviewStage::FollowUp => ("task-follow-up", ReviewTaskKind::FollowUpPass),
    };

    ReviewTask {
        id: task_id.to_owned(),
        review_session_id: "session-1".to_owned(),
        review_run_id: "run-1".to_owned(),
        stage: stage.as_str().to_owned(),
        task_kind,
        task_nonce: format!("nonce-{}", stage.as_str()),
        objective: format!("execute {} review work", stage.as_str()),
        turn_strategy: WorkerTurnStrategy::SingleTurnReport,
        allowed_scopes: vec!["repo".to_owned()],
        allowed_operations: vec![
            "worker.get_review_context".to_owned(),
            "worker.submit_stage_result".to_owned(),
        ],
        expected_result_schema: WORKER_STAGE_RESULT_SCHEMA_V1.to_owned(),
        prompt_preset_id: Some(prompt_preset_id.to_owned()),
        created_at: 50,
    }
}

fn sample_context_packet(task: &ReviewTask) -> WorkerContextPacket {
    WorkerContextPacket {
        review_target: sample_target(),
        review_session_id: task.review_session_id.clone(),
        review_run_id: task.review_run_id.clone(),
        review_task_id: task.id.clone(),
        task_nonce: task.task_nonce.clone(),
        baseline_snapshot_ref: Some("baseline-1".to_owned()),
        baseline_snapshot: Some(sample_baseline_snapshot(task)),
        provider: "opencode".to_owned(),
        transport_kind: WorkerTransportKind::LegacyStageHarness,
        stage: task.stage.clone(),
        objective: task.objective.clone(),
        allowed_scopes: task.allowed_scopes.clone(),
        allowed_operations: task.allowed_operations.clone(),
        mutation_posture: WorkerMutationPosture::ReviewOnly,
        github_posture: WorkerGitHubPosture::Blocked,
        unresolved_findings: Vec::new(),
        continuity_summary: Some("usable continuity".to_owned()),
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
        transport_kind: WorkerTransportKind::LegacyStageHarness,
        supports_context_reads: true,
        supports_memory_search: false,
        supports_finding_reads: true,
        supports_artifact_reads: false,
        supports_stage_result_submission: true,
        supports_clarification_requests: true,
        supports_follow_up_hints: true,
        supports_fix_mode: false,
    }
}

fn sample_stage_result(
    task: &ReviewTask,
    summary: &str,
    structured_pack_json: Option<String>,
    outcome: WorkerStageOutcome,
) -> WorkerStageResult {
    WorkerStageResult {
        schema_id: WORKER_STAGE_RESULT_SCHEMA_V1.to_owned(),
        review_session_id: task.review_session_id.clone(),
        review_run_id: task.review_run_id.clone(),
        review_task_id: task.id.clone(),
        worker_invocation_id: None,
        task_nonce: task.task_nonce.clone(),
        stage: task.stage.clone(),
        task_kind: task.task_kind,
        outcome,
        summary: summary.to_owned(),
        structured_findings_pack: structured_pack_json.map(|json| {
            serde_json::from_str(&json).expect("structured findings pack json should be valid")
        }),
        clarification_requests: Vec::new(),
        memory_review_requests: Vec::new(),
        follow_up_proposals: Vec::new(),
        memory_citations: Vec::new(),
        artifact_refs: Vec::new(),
        provider_metadata: None,
        warnings: Vec::new(),
    }
}

fn sample_tool_call_event(task: &ReviewTask, id: &str, operation: &str) -> WorkerToolCallEvent {
    WorkerToolCallEvent {
        id: id.to_owned(),
        review_task_id: task.id.clone(),
        worker_invocation_id: "pending-worker-id".to_owned(),
        operation: operation.to_owned(),
        request_digest: format!("sha256:{id}:request"),
        response_digest: Some(format!("sha256:{id}:response")),
        outcome_state: WorkerToolCallOutcomeState::Succeeded,
        occurred_at: 1_746_000_000,
    }
}

#[test]
fn stage_passes_run_independently_and_capture_raw_output_even_when_degraded() {
    let (_tempdir, store) = setup_store("session-1", "run-1");
    let locator = sample_locator();
    let exploration_task = sample_review_task(ReviewStage::Exploration, "preset-exploration");
    let deep_review_task = sample_review_task(ReviewStage::DeepReview, "preset-deep-review");
    let follow_up_task = sample_review_task(ReviewStage::FollowUp, "preset-follow-up");
    let exploration_context = sample_context_packet(&exploration_task);
    let deep_review_context = sample_context_packet(&deep_review_task);
    let follow_up_context = sample_context_packet(&follow_up_task);
    let capability_profile = sample_capability_profile();
    let harness = StubStageHarness::new(HashMap::from([
        (
            ReviewStage::Exploration,
            StageHarnessOutput {
                raw_output: "exploration raw transcript".to_owned(),
                worker_stage_result: sample_stage_result(
                    &exploration_task,
                    "Found one exploratory issue worth deeper review.",
                    Some(valid_structured_pack(
                        "Exploration finding",
                        "Something needs attention.",
                    )),
                    WorkerStageOutcome::Completed,
                ),
                tool_call_events: vec![sample_tool_call_event(
                    &exploration_task,
                    "tool-exploration-1",
                    "worker.get_review_context",
                )],
                degraded_reason: None,
            },
        ),
        (
            ReviewStage::DeepReview,
            StageHarnessOutput {
                raw_output: "deep review raw transcript".to_owned(),
                worker_stage_result: sample_stage_result(
                    &deep_review_task,
                    "No structured findings pack was returned from the deep review pass.",
                    None,
                    WorkerStageOutcome::CompletedPartial,
                ),
                tool_call_events: Vec::<WorkerToolCallEvent>::new(),
                degraded_reason: None,
            },
        ),
        (
            ReviewStage::FollowUp,
            StageHarnessOutput {
                raw_output: "follow-up raw transcript".to_owned(),
                worker_stage_result: sample_stage_result(
                    &follow_up_task,
                    "Provider continuation degraded before a structured follow-up report.",
                    None,
                    WorkerStageOutcome::NeedsContext,
                ),
                tool_call_events: Vec::<WorkerToolCallEvent>::new(),
                degraded_reason: Some("provider flagged degraded continuation".to_owned()),
            },
        ),
    ]));

    let exploration = execute_review_stage(
        &store,
        &harness,
        StageExecutionRequest {
            review_task: &exploration_task,
            worker_context: &exploration_context,
            capability_profile: &capability_profile,
            stage: ReviewStage::Exploration,
            session_locator: &locator,
            prompt: StagePrompt {
                prompt_preset_id: "preset-exploration",
                resolved_text: "Explore key review risks.",
                source_surface: "cli",
                explicit_objective: Some("focus on regressions"),
                provider: Some("opencode"),
                model: Some("o4"),
                scope_context_json: Some(r#"{"repo":"owner/repo"}"#),
                config_layer_digest: Some("cfg-1"),
                launch_intake_id: Some("launch-1"),
            },
            repair_attempt: 1,
            repair_retry_budget: 1,
            actor_id: Some("azure-river"),
        },
    )
    .expect("exploration execution");

    let deep_review = execute_review_stage(
        &store,
        &harness,
        StageExecutionRequest {
            review_task: &deep_review_task,
            worker_context: &deep_review_context,
            capability_profile: &capability_profile,
            stage: ReviewStage::DeepReview,
            session_locator: &locator,
            prompt: StagePrompt {
                prompt_preset_id: "preset-deep-review",
                resolved_text: "Perform deep review for safety and approval gates.",
                source_surface: "cli",
                explicit_objective: None,
                provider: Some("opencode"),
                model: Some("o4"),
                scope_context_json: None,
                config_layer_digest: Some("cfg-1"),
                launch_intake_id: Some("launch-1"),
            },
            repair_attempt: 1,
            repair_retry_budget: 1,
            actor_id: Some("azure-river"),
        },
    )
    .expect("deep review execution");

    let follow_up = execute_review_stage(
        &store,
        &harness,
        StageExecutionRequest {
            review_task: &follow_up_task,
            worker_context: &follow_up_context,
            capability_profile: &capability_profile,
            stage: ReviewStage::FollowUp,
            session_locator: &locator,
            prompt: StagePrompt {
                prompt_preset_id: "preset-follow-up",
                resolved_text: "Follow up on unresolved high-risk findings.",
                source_surface: "cli",
                explicit_objective: None,
                provider: Some("opencode"),
                model: Some("o4"),
                scope_context_json: None,
                config_layer_digest: Some("cfg-1"),
                launch_intake_id: Some("launch-1"),
            },
            repair_attempt: 1,
            repair_retry_budget: 1,
            actor_id: Some("azure-river"),
        },
    )
    .expect("follow-up execution");

    assert_eq!(
        exploration.validation_outcome.stage_state,
        StageState::Structured
    );
    assert_eq!(
        deep_review.validation_outcome.stage_state,
        StageState::RawOnly
    );
    assert_eq!(
        follow_up.validation_outcome.stage_state,
        StageState::RawOnly
    );
    assert!(follow_up.outcome_metadata.degraded);
    assert_eq!(
        follow_up.outcome_metadata.degraded_reason.as_deref(),
        Some("provider flagged degraded continuation")
    );

    assert_eq!(
        store
            .artifact_bytes(&exploration.raw_output_artifact_id)
            .expect("exploration raw bytes"),
        b"exploration raw transcript"
    );
    assert_eq!(
        store
            .artifact_bytes(&deep_review.raw_output_artifact_id)
            .expect("deep raw bytes"),
        b"deep review raw transcript"
    );
    assert_eq!(
        store
            .artifact_bytes(&follow_up.raw_output_artifact_id)
            .expect("follow-up raw bytes"),
        b"follow-up raw transcript"
    );

    let exploration_invocation = store
        .prompt_invocation(&exploration.prompt_invocation_id)
        .expect("lookup invocation")
        .expect("invocation exists");
    assert_eq!(exploration_invocation.stage, "exploration");
    assert_eq!(
        exploration_invocation.review_task_id.as_deref(),
        Some("task-exploration")
    );
    assert_eq!(
        exploration_invocation.worker_invocation_id.as_deref(),
        Some(exploration.worker_invocation.id.as_str())
    );
    assert_eq!(exploration_invocation.turn_index, 0);
    assert_eq!(
        exploration_invocation.prompt_preset_id,
        "preset-exploration"
    );

    let worker_invocations = store
        .worker_invocations_for_run("session-1", "run-1")
        .expect("list worker invocations");
    assert_eq!(worker_invocations.len(), 3);
    let exploration_worker_invocation = worker_invocations
        .iter()
        .find(|invocation| invocation.id == exploration.worker_invocation.id)
        .expect("exploration worker invocation");
    assert_eq!(
        exploration_worker_invocation
            .prompt_invocation_id
            .as_deref(),
        Some(exploration.prompt_invocation_id.as_str())
    );
    assert_eq!(
        exploration_worker_invocation.result_artifact_id.as_deref(),
        Some(exploration.result_artifact_id.as_str())
    );

    let tool_call_events = store
        .worker_tool_call_events_for_invocation(&exploration.worker_invocation.id)
        .expect("list worker tool calls");
    assert_eq!(tool_call_events.len(), 1);
    assert_eq!(tool_call_events[0].operation, "worker.get_review_context");
    assert_eq!(
        tool_call_events[0].outcome_state,
        WorkerToolCallOutcomeState::Succeeded
    );

    let worker_stage_results = store
        .worker_stage_results_for_run("session-1", "run-1")
        .expect("list worker stage results");
    assert_eq!(worker_stage_results.len(), 3);
    let exploration_stage_result = worker_stage_results
        .iter()
        .find(|row| row.review_task_id == "task-exploration")
        .expect("exploration stage result");
    assert_eq!(
        exploration_stage_result.worker_invocation_id.as_deref(),
        Some(exploration.worker_invocation.id.as_str())
    );
    assert_eq!(
        exploration_stage_result.outcome,
        WorkerStageOutcome::Completed
    );
    assert_eq!(
        exploration_stage_result
            .submitted_result_artifact_id
            .as_deref(),
        Some(exploration.result_artifact_id.as_str())
    );
    let pack_artifact_id = exploration_stage_result
        .structured_findings_pack_artifact_id
        .as_deref()
        .expect("structured pack artifact id");
    let stored_pack: serde_json::Value =
        serde_json::from_slice(&store.artifact_bytes(pack_artifact_id).expect("pack bytes"))
            .expect("parse stored pack");
    let expected_pack: serde_json::Value = serde_json::from_str(&valid_structured_pack(
        "Exploration finding",
        "Something needs attention.",
    ))
    .expect("parse expected pack");
    assert_eq!(stored_pack, expected_pack);

    let events = store
        .outcome_events_for_run("session-1", "run-1")
        .expect("list events");
    assert_eq!(events.len(), 3);
    assert!(
        events
            .iter()
            .all(|event| event.event_type == PROMPT_INVOKED_EVENT_TYPE)
    );

    let stage_set: HashSet<String> = events
        .iter()
        .map(|event| {
            let payload: StageOutcomeMetadata =
                serde_json::from_str(&event.payload_json).expect("decode payload");
            payload.stage
        })
        .collect();
    assert_eq!(
        stage_set,
        HashSet::from([
            "exploration".to_owned(),
            "deep_review".to_owned(),
            "follow_up".to_owned()
        ])
    );

    assert_eq!(
        harness.calls(),
        vec![
            ReviewStage::Exploration,
            ReviewStage::DeepReview,
            ReviewStage::FollowUp
        ]
    );
    assert_eq!(
        exploration.worker_stage_result.summary,
        "Found one exploratory issue worth deeper review."
    );
    assert_eq!(
        exploration.outcome_metadata.worker_outcome,
        WorkerStageOutcome::Completed
    );
    assert_eq!(
        exploration.outcome_metadata.worker_result_schema_id,
        WORKER_STAGE_RESULT_SCHEMA_V1
    );
    assert_eq!(
        deep_review.outcome_metadata.worker_outcome,
        WorkerStageOutcome::CompletedPartial
    );
    assert_eq!(
        follow_up.outcome_metadata.worker_outcome,
        WorkerStageOutcome::NeedsContext
    );
    assert_eq!(
        store
            .artifact_bytes(&exploration.result_artifact_id)
            .expect("worker result bytes"),
        serde_json::to_vec(&exploration.worker_stage_result).expect("serialize stage result")
    );
}

#[test]
fn stage_outcome_metadata_retains_repair_state_for_audit_and_refresh() {
    let (_tempdir, store) = setup_store("session-2", "run-2");
    let locator = sample_locator();
    let deep_review_task = ReviewTask {
        review_session_id: "session-2".to_owned(),
        review_run_id: "run-2".to_owned(),
        ..sample_review_task(ReviewStage::DeepReview, "preset-deep-review")
    };
    let deep_review_context = WorkerContextPacket {
        review_session_id: "session-2".to_owned(),
        review_run_id: "run-2".to_owned(),
        review_task_id: deep_review_task.id.clone(),
        task_nonce: deep_review_task.task_nonce.clone(),
        stage: deep_review_task.stage.clone(),
        objective: deep_review_task.objective.clone(),
        allowed_scopes: deep_review_task.allowed_scopes.clone(),
        allowed_operations: deep_review_task.allowed_operations.clone(),
        ..sample_context_packet(&deep_review_task)
    };
    let capability_profile = sample_capability_profile();
    let harness = StubStageHarness::new(HashMap::from([(
        ReviewStage::DeepReview,
        StageHarnessOutput {
            raw_output: "raw deep review transcript".to_owned(),
            worker_stage_result: WorkerStageResult {
                structured_findings_pack: Some(serde_json::Value::String("{bad-json".to_owned())),
                ..sample_stage_result(
                    &deep_review_task,
                    "The provider returned malformed structured findings JSON.",
                    None,
                    WorkerStageOutcome::CompletedPartial,
                )
            },
            tool_call_events: Vec::<WorkerToolCallEvent>::new(),
            degraded_reason: None,
        },
    )]));

    let result = execute_review_stage(
        &store,
        &harness,
        StageExecutionRequest {
            review_task: &deep_review_task,
            worker_context: &deep_review_context,
            capability_profile: &capability_profile,
            stage: ReviewStage::DeepReview,
            session_locator: &locator,
            prompt: StagePrompt {
                prompt_preset_id: "preset-deep-review",
                resolved_text: "Deep review this patch for regressions.",
                source_surface: "cli",
                explicit_objective: Some("recover structured findings if malformed"),
                provider: Some("opencode"),
                model: Some("o4"),
                scope_context_json: None,
                config_layer_digest: Some("cfg-2"),
                launch_intake_id: Some("launch-2"),
            },
            repair_attempt: 0,
            repair_retry_budget: 1,
            actor_id: Some("azure-river"),
        },
    )
    .expect("execution should succeed with repair-needed state");

    assert_eq!(
        result.validation_outcome.stage_state,
        StageState::RepairNeeded
    );
    assert_eq!(
        result.validation_outcome.repair_action,
        RepairAction::RetryRepair
    );
    assert!(!result.validation_outcome.issues.is_empty());

    let events = store
        .outcome_events_for_run("session-2", "run-2")
        .expect("list outcome events");
    let event = events
        .iter()
        .find(|event| event.id == result.outcome_event_id)
        .expect("result event exists");

    let payload: StageOutcomeMetadata =
        serde_json::from_str(&event.payload_json).expect("decode payload");
    assert_eq!(payload.stage, "deep_review");
    assert_eq!(payload.review_task_id, deep_review_task.id);
    assert_eq!(payload.stage_state, StageState::RepairNeeded);
    assert_eq!(payload.repair_action, RepairAction::RetryRepair);
    assert!(payload.structured_pack_present);
    assert_eq!(payload.finding_count, 0);
    assert!(payload.issue_count > 0);
    assert!(!payload.degraded);
    assert_eq!(
        payload.worker_result_schema_id,
        WORKER_STAGE_RESULT_SCHEMA_V1
    );
    assert_eq!(payload.worker_outcome, WorkerStageOutcome::CompletedPartial);

    let invocation = store
        .prompt_invocation(&result.prompt_invocation_id)
        .expect("lookup invocation")
        .expect("invocation exists");
    assert_eq!(invocation.stage, "deep_review");
    assert_eq!(
        invocation.review_task_id.as_deref(),
        Some(deep_review_task.id.as_str())
    );
    assert_eq!(
        invocation.worker_invocation_id.as_deref(),
        Some(result.worker_invocation.id.as_str())
    );
    assert_eq!(invocation.turn_index, 0);
    assert_eq!(invocation.prompt_preset_id, "preset-deep-review");
    assert_eq!(invocation.source_surface, "cli");

    let _ = sample_intent();
}

#[test]
fn stage_execution_rejects_result_with_stale_task_nonce() {
    let (_tempdir, store) = setup_store("session-3", "run-3");
    let locator = sample_locator();
    let deep_review_task = ReviewTask {
        review_session_id: "session-3".to_owned(),
        review_run_id: "run-3".to_owned(),
        ..sample_review_task(ReviewStage::DeepReview, "preset-deep-review")
    };
    let deep_review_context = sample_context_packet(&deep_review_task);
    let capability_profile = sample_capability_profile();
    let harness = StubStageHarness::new(HashMap::from([(
        ReviewStage::DeepReview,
        StageHarnessOutput {
            raw_output: "deep review raw transcript".to_owned(),
            worker_stage_result: WorkerStageResult {
                task_nonce: "stale-nonce".to_owned(),
                ..sample_stage_result(
                    &deep_review_task,
                    "Returned a result bound to the wrong nonce.",
                    None,
                    WorkerStageOutcome::Failed,
                )
            },
            tool_call_events: Vec::<WorkerToolCallEvent>::new(),
            degraded_reason: None,
        },
    )]));

    let err = execute_review_stage(
        &store,
        &harness,
        StageExecutionRequest {
            review_task: &deep_review_task,
            worker_context: &deep_review_context,
            capability_profile: &capability_profile,
            stage: ReviewStage::DeepReview,
            session_locator: &locator,
            prompt: StagePrompt {
                prompt_preset_id: "preset-deep-review",
                resolved_text: "Deep review this patch for regressions.",
                source_surface: "cli",
                explicit_objective: Some("ensure wrong task binding fails closed"),
                provider: Some("opencode"),
                model: Some("o4"),
                scope_context_json: None,
                config_layer_digest: Some("cfg-3"),
                launch_intake_id: Some("launch-3"),
            },
            repair_attempt: 0,
            repair_retry_budget: 1,
            actor_id: Some("azure-river"),
        },
    )
    .expect_err("stale nonce should fail closed");

    assert!(matches!(
        err,
        roger_prompt_engine::stage_execution::StageExecutionError::WorkerContract(
            roger_app_core::ReviewWorkerContractError::ResultNonceMismatch { .. }
        )
    ));
}
