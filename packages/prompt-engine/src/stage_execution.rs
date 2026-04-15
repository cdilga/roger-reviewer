use std::time::{SystemTime, UNIX_EPOCH};

use roger_app_core::{
    ReviewTask, ReviewWorkerContractError, SessionLocator, WorkerCapabilityProfile,
    WorkerContextPacket, WorkerInvocation, WorkerStageOutcome, WorkerStageResult,
    WorkerToolCallEvent, now_ts,
};
use roger_storage::{
    ArtifactBudgetClass, CreateCodeEvidenceLocation, CreateMaterializedFinding, CreateOutcomeEvent,
    CreatePromptInvocation, MaterializedFindingRecord, RogerStore, StorageError,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    RepairAction, StageState, ValidationContext, ValidationError, ValidationOutcome,
    validate_structured_findings_pack,
};

pub const PROMPT_INVOKED_EVENT_TYPE: &str = "prompt_invoked";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStage {
    Exploration,
    DeepReview,
    FollowUp,
}

impl ReviewStage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exploration => "exploration",
            Self::DeepReview => "deep_review",
            Self::FollowUp => "follow_up",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagePrompt<'a> {
    pub prompt_preset_id: &'a str,
    pub resolved_text: &'a str,
    pub source_surface: &'a str,
    pub explicit_objective: Option<&'a str>,
    pub provider: Option<&'a str>,
    pub model: Option<&'a str>,
    pub scope_context_json: Option<&'a str>,
    pub config_layer_digest: Option<&'a str>,
    pub launch_intake_id: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageExecutionRequest<'a> {
    pub review_task: &'a ReviewTask,
    pub worker_context: &'a WorkerContextPacket,
    pub capability_profile: &'a WorkerCapabilityProfile,
    pub stage: ReviewStage,
    pub session_locator: &'a SessionLocator,
    pub prompt: StagePrompt<'a>,
    pub repair_attempt: u8,
    pub repair_retry_budget: u8,
    pub actor_id: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReviewWorkerTransportOutput {
    pub raw_output: String,
    pub worker_stage_result: WorkerStageResult,
    pub tool_call_events: Vec<WorkerToolCallEvent>,
    pub degraded_reason: Option<String>,
}

pub use self::ReviewWorkerTransport as StageHarness;
pub use self::ReviewWorkerTransportOutput as StageHarnessOutput;

pub trait ReviewWorkerTransport {
    fn execute_stage(
        &self,
        locator: &SessionLocator,
        review_task: &ReviewTask,
        worker_context: &WorkerContextPacket,
        capability_profile: &WorkerCapabilityProfile,
        worker_invocation_id: &str,
        prompt_text: &str,
    ) -> std::result::Result<ReviewWorkerTransportOutput, String>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageOutcomeMetadata {
    pub stage: String,
    pub review_task_id: String,
    pub task_nonce: String,
    pub task_kind: roger_app_core::ReviewTaskKind,
    pub turn_strategy: roger_app_core::WorkerTurnStrategy,
    pub worker_invocation_id: String,
    pub worker_transport_kind: String,
    pub worker_result_schema_id: String,
    pub worker_outcome: WorkerStageOutcome,
    pub summary: String,
    pub clarification_request_count: usize,
    pub memory_review_request_count: usize,
    pub follow_up_proposal_count: usize,
    pub tool_call_count: usize,
    pub warning_count: usize,
    pub stage_state: StageState,
    pub repair_action: RepairAction,
    pub issue_count: usize,
    pub finding_count: usize,
    pub materialized_finding_ids: Vec<String>,
    pub structured_pack_present: bool,
    pub degraded: bool,
    pub degraded_reason: Option<String>,
    pub result_artifact_id: String,
    pub raw_output_artifact_id: String,
    pub raw_output_digest: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StageExecutionResult {
    pub stage: ReviewStage,
    pub worker_invocation: WorkerInvocation,
    pub worker_stage_result: WorkerStageResult,
    pub worker_tool_call_events: Vec<WorkerToolCallEvent>,
    pub prompt_invocation_id: String,
    pub prompt_text_artifact_id: String,
    pub raw_output_artifact_id: String,
    pub result_artifact_id: String,
    pub outcome_event_id: String,
    pub validation_outcome: ValidationOutcome,
    pub materialized_findings: Vec<MaterializedFindingRecord>,
    pub outcome_metadata: StageOutcomeMetadata,
}

#[derive(Debug, Error)]
pub enum StageExecutionError {
    #[error("harness stage execution failed: {0}")]
    Harness(String),
    #[error("worker contract error: {0}")]
    WorkerContract(#[from] ReviewWorkerContractError),
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("validation error: {0}")]
    Validation(#[from] ValidationError),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub fn execute_review_stage(
    store: &RogerStore,
    harness: &impl ReviewWorkerTransport,
    request: StageExecutionRequest<'_>,
) -> std::result::Result<StageExecutionResult, StageExecutionError> {
    request
        .review_task
        .validate_context_packet(request.worker_context)?;
    request
        .review_task
        .validate_capability_profile(request.capability_profile)?;
    request
        .review_task
        .validate_prompt_preset_id(request.prompt.prompt_preset_id)?;

    let used_at = now_ts();
    let base_nonce = now_nonce();

    let prompt_digest = digest_hex(request.prompt.resolved_text.as_bytes());
    let prompt_text_artifact_id = next_id(
        "prompt",
        &request.review_task.review_run_id,
        request.stage,
        base_nonce,
    );
    store.store_artifact(
        &prompt_text_artifact_id,
        ArtifactBudgetClass::InlineSummary,
        "text/plain; charset=utf-8",
        request.prompt.resolved_text.as_bytes(),
    )?;

    let worker_invocation_id = next_id(
        "worker",
        &request.review_task.review_run_id,
        request.stage,
        base_nonce.wrapping_add(1),
    );
    let harness_output = harness
        .execute_stage(
            request.session_locator,
            request.review_task,
            request.worker_context,
            request.capability_profile,
            &worker_invocation_id,
            request.prompt.resolved_text,
        )
        .map_err(StageExecutionError::Harness)?;

    for event in &harness_output.tool_call_events {
        request
            .review_task
            .validate_tool_call_event(event, &worker_invocation_id)?;
    }

    let mut worker_stage_result = harness_output.worker_stage_result;
    request
        .review_task
        .validate_stage_result(&worker_stage_result)?;
    request
        .review_task
        .validate_worker_invocation_binding(&worker_stage_result, &worker_invocation_id)?;
    if worker_stage_result.worker_invocation_id.is_none() {
        worker_stage_result.worker_invocation_id = Some(worker_invocation_id.clone());
    }

    let raw_output_artifact_id = next_id(
        "raw",
        &request.review_task.review_run_id,
        request.stage,
        base_nonce.wrapping_add(2),
    );
    let raw_artifact = store.store_artifact(
        &raw_output_artifact_id,
        ArtifactBudgetClass::ColdArtifact,
        "text/plain; charset=utf-8",
        harness_output.raw_output.as_bytes(),
    )?;

    let structured_pack_json = worker_stage_result.structured_findings_pack_json()?;
    let result_artifact_id = next_id(
        "result",
        &request.review_task.review_run_id,
        request.stage,
        base_nonce.wrapping_add(3),
    );
    let result_artifact_bytes = serde_json::to_vec(&worker_stage_result)?;
    store.store_artifact(
        &result_artifact_id,
        ArtifactBudgetClass::ColdArtifact,
        "application/json",
        &result_artifact_bytes,
    )?;

    let prompt_invocation_id = next_id(
        "invocation",
        &request.review_task.review_run_id,
        request.stage,
        base_nonce.wrapping_add(4),
    );
    store.record_prompt_invocation(CreatePromptInvocation {
        id: &prompt_invocation_id,
        review_session_id: &request.review_task.review_session_id,
        review_run_id: &request.review_task.review_run_id,
        stage: request.stage.as_str(),
        prompt_preset_id: request.prompt.prompt_preset_id,
        source_surface: request.prompt.source_surface,
        resolved_text_digest: &prompt_digest,
        resolved_text_artifact_id: Some(&prompt_text_artifact_id),
        resolved_text_inline_preview: make_preview(request.prompt.resolved_text, 240).as_deref(),
        explicit_objective: request.prompt.explicit_objective,
        provider: request.prompt.provider,
        model: request.prompt.model,
        scope_context_json: request.prompt.scope_context_json,
        config_layer_digest: request.prompt.config_layer_digest,
        launch_intake_id: request.prompt.launch_intake_id,
        used_at,
    })?;

    let validation_outcome = validate_structured_findings_pack(
        &ValidationContext {
            review_session_id: &request.review_task.review_session_id,
            review_run_id: &request.review_task.review_run_id,
            repair_attempt: request.repair_attempt,
            repair_retry_budget: request.repair_retry_budget,
        },
        structured_pack_json.as_deref(),
        &harness_output.raw_output,
    )?;

    let materialized_findings = materialize_findings(
        store,
        &request.review_task.review_session_id,
        &request.review_task.review_run_id,
        request.stage,
        &validation_outcome,
        base_nonce.wrapping_add(6),
    )?;
    let materialized_finding_ids = materialized_findings
        .iter()
        .map(|finding| finding.id.clone())
        .collect::<Vec<_>>();

    let completed_at = now_ts();
    let worker_invocation = WorkerInvocation {
        id: worker_invocation_id.clone(),
        review_session_id: request.review_task.review_session_id.clone(),
        review_run_id: request.review_task.review_run_id.clone(),
        review_task_id: request.review_task.id.clone(),
        provider: request.worker_context.provider.clone(),
        provider_session_id: Some(request.session_locator.session_id.clone()),
        transport_kind: request.capability_profile.transport_kind,
        started_at: used_at,
        completed_at: Some(completed_at),
        outcome_state: worker_stage_result.outcome.invocation_state(),
        prompt_invocation_id: Some(prompt_invocation_id.clone()),
        raw_output_artifact_id: Some(raw_output_artifact_id.clone()),
        result_artifact_id: Some(result_artifact_id.clone()),
    };

    let outcome_metadata = StageOutcomeMetadata {
        stage: request.review_task.stage.clone(),
        review_task_id: request.review_task.id.clone(),
        task_nonce: request.review_task.task_nonce.clone(),
        task_kind: request.review_task.task_kind,
        turn_strategy: request.review_task.turn_strategy,
        worker_invocation_id: worker_invocation.id.clone(),
        worker_transport_kind: request
            .capability_profile
            .transport_kind
            .as_str()
            .to_owned(),
        worker_result_schema_id: worker_stage_result.schema_id.clone(),
        worker_outcome: worker_stage_result.outcome,
        summary: worker_stage_result.summary.clone(),
        clarification_request_count: worker_stage_result.clarification_requests.len(),
        memory_review_request_count: worker_stage_result.memory_review_requests.len(),
        follow_up_proposal_count: worker_stage_result.follow_up_proposals.len(),
        tool_call_count: harness_output.tool_call_events.len(),
        warning_count: worker_stage_result.warnings.len(),
        stage_state: validation_outcome.stage_state,
        repair_action: validation_outcome.repair_action,
        issue_count: validation_outcome.issues.len(),
        finding_count: validation_outcome.findings.len(),
        materialized_finding_ids,
        structured_pack_present: worker_stage_result.structured_findings_pack.is_some(),
        degraded: harness_output.degraded_reason.is_some(),
        degraded_reason: harness_output.degraded_reason,
        result_artifact_id: result_artifact_id.clone(),
        raw_output_artifact_id: raw_output_artifact_id.clone(),
        raw_output_digest: raw_artifact.digest,
    };

    let payload_json = serde_json::to_string(&outcome_metadata)?;
    let outcome_event_id = next_id(
        "event",
        &request.review_task.review_run_id,
        request.stage,
        base_nonce.wrapping_add(5),
    );
    store.record_outcome_event(CreateOutcomeEvent {
        id: &outcome_event_id,
        event_type: PROMPT_INVOKED_EVENT_TYPE,
        review_session_id: &request.review_task.review_session_id,
        review_run_id: Some(&request.review_task.review_run_id),
        prompt_invocation_id: Some(&prompt_invocation_id),
        actor_kind: "agent",
        actor_id: request.actor_id,
        source_surface: request.prompt.source_surface,
        payload_json: &payload_json,
        occurred_at: used_at,
    })?;

    Ok(StageExecutionResult {
        stage: request.stage,
        worker_invocation,
        worker_stage_result,
        worker_tool_call_events: harness_output.tool_call_events,
        prompt_invocation_id,
        prompt_text_artifact_id,
        raw_output_artifact_id,
        result_artifact_id,
        outcome_event_id,
        validation_outcome,
        materialized_findings,
        outcome_metadata,
    })
}

fn materialize_findings(
    store: &RogerStore,
    review_session_id: &str,
    review_run_id: &str,
    stage: ReviewStage,
    validation_outcome: &ValidationOutcome,
    base_nonce: u128,
) -> std::result::Result<Vec<MaterializedFindingRecord>, StageExecutionError> {
    let mut materialized = Vec::new();

    for (index, row) in validation_outcome.finding_rows.iter().enumerate() {
        let finding_id = finding_id_for(review_session_id, &row.fingerprint);
        let record = store.upsert_materialized_finding(CreateMaterializedFinding {
            id: &finding_id,
            session_id: review_session_id,
            review_run_id,
            stage: stage.as_str(),
            fingerprint: &row.fingerprint,
            title: &row.title,
            normalized_summary: &row.normalized_summary,
            severity: &row.severity,
            confidence: &row.confidence,
            triage_state: &row.triage_state,
            outbound_state: &row.outbound_state,
        })?;

        if let Some(finding) = validation_outcome.findings.get(index) {
            for (evidence_index, evidence) in finding.evidence.iter().enumerate() {
                let evidence_id = format!(
                    "evidence-{}-{}-{index}-{evidence_index}-{nonce:x}",
                    record.id,
                    review_run_id,
                    nonce = base_nonce.wrapping_add(index as u128 + evidence_index as u128)
                );
                store.add_code_evidence_location(CreateCodeEvidenceLocation {
                    id: &evidence_id,
                    finding_id: &record.id,
                    review_session_id,
                    review_run_id,
                    evidence_role: &evidence.evidence_role,
                    repo_rel_path: &evidence.repo_rel_path,
                    start_line: evidence.start_line as i64,
                    end_line: evidence.end_line.map(i64::from),
                    anchor_state: &evidence.anchor_state,
                    anchor_digest: None,
                    excerpt_artifact_id: None,
                })?;
            }
        }

        materialized.push(record);
    }

    Ok(materialized)
}

fn finding_id_for(review_session_id: &str, fingerprint: &str) -> String {
    format!(
        "finding-{}",
        digest_hex(format!("{review_session_id}:{fingerprint}").as_bytes())
    )
}

fn digest_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn make_preview(text: &str, max_chars: usize) -> Option<String> {
    let preview: String = text.chars().take(max_chars).collect();
    if preview.is_empty() {
        None
    } else {
        Some(preview)
    }
}

fn next_id(prefix: &str, run_id: &str, stage: ReviewStage, nonce: u128) -> String {
    format!("{prefix}-{run_id}-{}-{nonce:x}", stage.as_str())
}

fn now_nonce() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_nanos()
}
