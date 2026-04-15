use std::cell::RefCell;
use std::collections::HashMap;

use roger_app_core::{
    LaunchAction, LaunchIntent, ReviewTarget, ReviewTask, ReviewTaskKind, SessionLocator, Surface,
    WORKER_STAGE_RESULT_SCHEMA_V1, WorkerCapabilityProfile, WorkerContextPacket,
    WorkerGitHubPosture, WorkerMutationPosture, WorkerStageOutcome, WorkerStageResult,
    WorkerToolCallEvent, WorkerTransportKind, WorkerTurnStrategy,
};
use roger_prompt_engine::stage_execution::{
    ReviewStage, StageExecutionRequest, StageHarness, StageHarnessOutput, StagePrompt,
    execute_review_stage,
};
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

    #[allow(dead_code)]
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

fn sample_prompt<'a>(preset_id: &'a str) -> StagePrompt<'a> {
    StagePrompt {
        prompt_preset_id: preset_id,
        resolved_text: "Review this PR for safety and continuity issues.",
        source_surface: "cli",
        explicit_objective: Some("focus on correctness regressions"),
        provider: Some("opencode"),
        model: Some("o4"),
        scope_context_json: Some(r#"{"repo":"owner/repo"}"#),
        config_layer_digest: Some("cfg-materialization"),
        launch_intake_id: Some("launch-materialization"),
    }
}

fn sample_review_task(
    session_id: &str,
    run_id: &str,
    stage: ReviewStage,
    prompt_preset_id: &str,
) -> ReviewTask {
    let (task_id, task_kind) = match stage {
        ReviewStage::Exploration => ("task-exploration", ReviewTaskKind::ExplorationPass),
        ReviewStage::DeepReview => ("task-deep-review", ReviewTaskKind::DeepReviewPass),
        ReviewStage::FollowUp => ("task-follow-up", ReviewTaskKind::FollowUpPass),
    };

    ReviewTask {
        id: format!("{task_id}-{session_id}"),
        review_session_id: session_id.to_owned(),
        review_run_id: run_id.to_owned(),
        stage: stage.as_str().to_owned(),
        task_kind,
        task_nonce: format!("nonce-{session_id}-{}", stage.as_str()),
        objective: format!("execute {} review work", stage.as_str()),
        turn_strategy: WorkerTurnStrategy::SingleTurnReport,
        allowed_scopes: vec!["repo".to_owned()],
        allowed_operations: vec![
            "worker.get_review_context".to_owned(),
            "worker.submit_stage_result".to_owned(),
        ],
        expected_result_schema: WORKER_STAGE_RESULT_SCHEMA_V1.to_owned(),
        prompt_preset_id: Some(prompt_preset_id.to_owned()),
        created_at: 100,
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

fn valid_structured_pack() -> String {
    r#"{
  "schema_version": "structured_findings_pack.v1",
  "findings": [
    {
      "fingerprint": "fp-materialized-1",
      "title": "Critical guard is bypassed",
      "summary": "A safety guard does not execute before side effects.",
      "severity": "high",
      "confidence": "high",
      "evidence": [
        {
          "repo_rel_path": "src/lib.rs",
          "start_line": 12,
          "end_line": 16,
          "anchor_state": "exact",
          "evidence_role": "primary"
        }
      ]
    }
  ]
}"#
    .to_owned()
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

#[test]
fn structured_stage_materializes_findings_with_provenance_and_evidence() {
    let (_tempdir, store) = setup_store("session-materialized", "run-materialized");
    let locator = sample_locator();
    let review_task = sample_review_task(
        "session-materialized",
        "run-materialized",
        ReviewStage::Exploration,
        "preset-exploration",
    );
    let context_packet = sample_context_packet(&review_task);
    let capability_profile = sample_capability_profile();
    let harness = StubStageHarness::new(HashMap::from([(
        ReviewStage::Exploration,
        StageHarnessOutput {
            raw_output: "exploration raw transcript".to_owned(),
            worker_stage_result: sample_stage_result(
                &review_task,
                "Materialized one high-confidence finding from exploration.",
                Some(valid_structured_pack()),
                WorkerStageOutcome::Completed,
            ),
            tool_call_events: Vec::<WorkerToolCallEvent>::new(),
            degraded_reason: None,
        },
    )]));

    let result = execute_review_stage(
        &store,
        &harness,
        StageExecutionRequest {
            review_task: &review_task,
            worker_context: &context_packet,
            capability_profile: &capability_profile,
            stage: ReviewStage::Exploration,
            session_locator: &locator,
            prompt: sample_prompt("preset-exploration"),
            repair_attempt: 0,
            repair_retry_budget: 1,
            actor_id: Some("copper-brook"),
        },
    )
    .expect("stage execution succeeds");

    assert_eq!(result.materialized_findings.len(), 1);
    assert_eq!(result.outcome_metadata.materialized_finding_ids.len(), 1);
    let finding_id = &result.outcome_metadata.materialized_finding_ids[0];

    let stored = store
        .materialized_finding(finding_id)
        .expect("lookup materialized finding")
        .expect("materialized finding exists");
    assert_eq!(stored.session_id, "session-materialized");
    assert_eq!(stored.last_seen_run_id.as_deref(), Some("run-materialized"));
    assert_eq!(stored.first_seen_stage, "exploration");
    assert_eq!(stored.last_seen_stage.as_deref(), Some("exploration"));
    assert_eq!(stored.severity, "high");
    assert_eq!(stored.confidence, "high");

    let run_findings = store
        .materialized_findings_for_run("session-materialized", "run-materialized")
        .expect("list materialized findings for run");
    assert_eq!(run_findings.len(), 1);
    assert_eq!(run_findings[0].id, *finding_id);

    let evidence = store
        .code_evidence_locations_for_finding(finding_id)
        .expect("list evidence");
    assert_eq!(evidence.len(), 1);
    assert_eq!(evidence[0].repo_rel_path, "src/lib.rs");
    assert_eq!(evidence[0].start_line, 12);
    assert_eq!(evidence[0].end_line, Some(16));

    let _ = sample_intent();
}

#[test]
fn degraded_raw_only_stage_does_not_fabricate_materialized_findings() {
    let (_tempdir, store) = setup_store("session-degraded", "run-degraded");
    let locator = sample_locator();
    let review_task = sample_review_task(
        "session-degraded",
        "run-degraded",
        ReviewStage::FollowUp,
        "preset-follow-up",
    );
    let context_packet = sample_context_packet(&review_task);
    let capability_profile = sample_capability_profile();
    let harness = StubStageHarness::new(HashMap::from([(
        ReviewStage::FollowUp,
        StageHarnessOutput {
            raw_output: "follow-up raw transcript".to_owned(),
            worker_stage_result: sample_stage_result(
                &review_task,
                "Continuation degraded before follow-up findings could be materialized.",
                None,
                WorkerStageOutcome::NeedsContext,
            ),
            tool_call_events: Vec::<WorkerToolCallEvent>::new(),
            degraded_reason: Some("provider continuation degraded".to_owned()),
        },
    )]));

    let result = execute_review_stage(
        &store,
        &harness,
        StageExecutionRequest {
            review_task: &review_task,
            worker_context: &context_packet,
            capability_profile: &capability_profile,
            stage: ReviewStage::FollowUp,
            session_locator: &locator,
            prompt: sample_prompt("preset-follow-up"),
            repair_attempt: 0,
            repair_retry_budget: 1,
            actor_id: Some("copper-brook"),
        },
    )
    .expect("degraded stage execution succeeds");

    assert!(result.materialized_findings.is_empty());
    assert!(result.outcome_metadata.materialized_finding_ids.is_empty());
    assert!(result.outcome_metadata.degraded);
    assert_eq!(
        result.outcome_metadata.degraded_reason.as_deref(),
        Some("provider continuation degraded")
    );

    let run_findings = store
        .materialized_findings_for_run("session-degraded", "run-degraded")
        .expect("list materialized findings for degraded run");
    assert!(run_findings.is_empty());
}
