use std::time::{SystemTime, UNIX_EPOCH};

use roger_app_core::{SessionLocator, now_ts};
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
    pub review_session_id: &'a str,
    pub review_run_id: &'a str,
    pub stage: ReviewStage,
    pub session_locator: &'a SessionLocator,
    pub prompt: StagePrompt<'a>,
    pub repair_attempt: u8,
    pub repair_retry_budget: u8,
    pub actor_id: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageHarnessOutput {
    pub raw_output: String,
    pub structured_pack_json: Option<String>,
    pub degraded_reason: Option<String>,
}

pub trait StageHarness {
    fn execute_stage(
        &self,
        locator: &SessionLocator,
        stage: ReviewStage,
        prompt_text: &str,
    ) -> std::result::Result<StageHarnessOutput, String>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageOutcomeMetadata {
    pub stage: String,
    pub stage_state: StageState,
    pub repair_action: RepairAction,
    pub issue_count: usize,
    pub finding_count: usize,
    pub materialized_finding_ids: Vec<String>,
    pub structured_pack_present: bool,
    pub degraded: bool,
    pub degraded_reason: Option<String>,
    pub raw_output_artifact_id: String,
    pub raw_output_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageExecutionResult {
    pub stage: ReviewStage,
    pub prompt_invocation_id: String,
    pub prompt_text_artifact_id: String,
    pub raw_output_artifact_id: String,
    pub outcome_event_id: String,
    pub validation_outcome: ValidationOutcome,
    pub materialized_findings: Vec<MaterializedFindingRecord>,
    pub outcome_metadata: StageOutcomeMetadata,
}

#[derive(Debug, Error)]
pub enum StageExecutionError {
    #[error("harness stage execution failed: {0}")]
    Harness(String),
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("validation error: {0}")]
    Validation(#[from] ValidationError),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub fn execute_review_stage(
    store: &RogerStore,
    harness: &impl StageHarness,
    request: StageExecutionRequest<'_>,
) -> std::result::Result<StageExecutionResult, StageExecutionError> {
    let used_at = now_ts();
    let base_nonce = now_nonce();

    let prompt_digest = digest_hex(request.prompt.resolved_text.as_bytes());
    let prompt_text_artifact_id =
        next_id("prompt", request.review_run_id, request.stage, base_nonce);
    store.store_artifact(
        &prompt_text_artifact_id,
        ArtifactBudgetClass::InlineSummary,
        "text/plain; charset=utf-8",
        request.prompt.resolved_text.as_bytes(),
    )?;

    let harness_output = harness
        .execute_stage(
            request.session_locator,
            request.stage,
            request.prompt.resolved_text,
        )
        .map_err(StageExecutionError::Harness)?;

    let raw_output_artifact_id = next_id(
        "raw",
        request.review_run_id,
        request.stage,
        base_nonce.wrapping_add(1),
    );
    let raw_artifact = store.store_artifact(
        &raw_output_artifact_id,
        ArtifactBudgetClass::ColdArtifact,
        "text/plain; charset=utf-8",
        harness_output.raw_output.as_bytes(),
    )?;

    let invocation_id = next_id(
        "invocation",
        request.review_run_id,
        request.stage,
        base_nonce.wrapping_add(2),
    );
    store.record_prompt_invocation(CreatePromptInvocation {
        id: &invocation_id,
        review_session_id: request.review_session_id,
        review_run_id: request.review_run_id,
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
            review_session_id: request.review_session_id,
            review_run_id: request.review_run_id,
            repair_attempt: request.repair_attempt,
            repair_retry_budget: request.repair_retry_budget,
        },
        harness_output.structured_pack_json.as_deref(),
        &harness_output.raw_output,
    )?;

    let materialized_findings = materialize_findings(
        store,
        request.review_session_id,
        request.review_run_id,
        request.stage,
        &validation_outcome,
        base_nonce.wrapping_add(4),
    )?;
    let materialized_finding_ids = materialized_findings
        .iter()
        .map(|finding| finding.id.clone())
        .collect::<Vec<_>>();

    let outcome_metadata = StageOutcomeMetadata {
        stage: request.stage.as_str().to_owned(),
        stage_state: validation_outcome.stage_state,
        repair_action: validation_outcome.repair_action,
        issue_count: validation_outcome.issues.len(),
        finding_count: validation_outcome.findings.len(),
        materialized_finding_ids,
        structured_pack_present: harness_output
            .structured_pack_json
            .as_ref()
            .is_some_and(|json| !json.trim().is_empty()),
        degraded: harness_output.degraded_reason.is_some(),
        degraded_reason: harness_output.degraded_reason,
        raw_output_artifact_id: raw_output_artifact_id.clone(),
        raw_output_digest: raw_artifact.digest,
    };

    let payload_json = serde_json::to_string(&outcome_metadata)?;
    let outcome_event_id = next_id(
        "event",
        request.review_run_id,
        request.stage,
        base_nonce.wrapping_add(3),
    );
    store.record_outcome_event(CreateOutcomeEvent {
        id: &outcome_event_id,
        event_type: PROMPT_INVOKED_EVENT_TYPE,
        review_session_id: request.review_session_id,
        review_run_id: Some(request.review_run_id),
        prompt_invocation_id: Some(&invocation_id),
        actor_kind: "agent",
        actor_id: request.actor_id,
        source_surface: request.prompt.source_surface,
        payload_json: &payload_json,
        occurred_at: used_at,
    })?;

    Ok(StageExecutionResult {
        stage: request.stage,
        prompt_invocation_id: invocation_id,
        prompt_text_artifact_id,
        raw_output_artifact_id,
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
