pub use crate::time::now_ts;
use serde::{Deserialize, Serialize};
use serde_json::from_str;
use std::collections::{HashMap, HashSet};

pub mod cli_config;
pub mod time;
pub mod tui_shell;

pub type Timestamp = i64;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("provider error: {0}")]
    ProviderError(String),
    #[error("harness error: {0}")]
    HarnessError(String),
    #[error("serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
    #[error("io error: {0}")]
    IoError(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, AppError>;

pub trait HarnessAdapter {
    fn start_session(&self, target: &ReviewTarget, intent: &LaunchIntent)
    -> Result<SessionLocator>;
    fn seed_from_resume_bundle(&self, bundle: &ResumeBundle) -> Result<SessionLocator>;
    fn capture_raw_output(&self, locator: &SessionLocator) -> Result<String>;
    fn report_continuity_quality(
        &self,
        locator: &SessionLocator,
        target: &ReviewTarget,
    ) -> Result<ContinuityQuality>;

    // Tier B
    fn reopen_by_locator(&self, locator: &SessionLocator) -> Result<()>;
    fn open_in_bare_harness_mode(
        &self,
        locator: &SessionLocator,
        bundle: &ResumeBundle,
    ) -> Result<()>;
    fn return_to_roger_session(&self, locator: &SessionLocator) -> Result<()>;
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Surface {
    Cli,
    Tui,
    Extension,
    Direct,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RogerCommandInvocationSurface {
    Cli,
    Tui,
    HarnessCommand,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RogerCommandId {
    RogerHelp,
    RogerStatus,
    RogerFindings,
    RogerReturn,
}

impl RogerCommandId {
    pub fn logical_id(self) -> &'static str {
        match self {
            Self::RogerHelp => "roger-help",
            Self::RogerStatus => "roger-status",
            Self::RogerFindings => "roger-findings",
            Self::RogerReturn => "roger-return",
        }
    }

    pub fn canonical_operation(self) -> &'static str {
        match self {
            Self::RogerHelp => "show_help",
            Self::RogerStatus => "show_status",
            Self::RogerFindings => "show_findings",
            Self::RogerReturn => "return_to_roger",
        }
    }

    pub fn fallback_cli_command(self) -> &'static str {
        match self {
            Self::RogerHelp => "rr help",
            Self::RogerStatus => "rr status",
            Self::RogerFindings => "rr findings",
            Self::RogerReturn => "rr return",
        }
    }
}

pub fn parse_harness_command_id(raw: &str) -> Option<RogerCommandId> {
    let normalized = raw
        .trim()
        .to_ascii_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    match normalized.as_str() {
        "roger-help" | "/roger-help" | "roger help" | "/roger help" | ":roger help" => {
            Some(RogerCommandId::RogerHelp)
        }
        "roger-status" | "/roger-status" | "roger status" | "/roger status" | ":roger status" => {
            Some(RogerCommandId::RogerStatus)
        }
        "roger-findings" | "/roger-findings" | "roger findings" | "/roger findings"
        | ":roger findings" => Some(RogerCommandId::RogerFindings),
        "roger-return" | "/roger-return" | "roger return" | "/roger return" | ":roger return" => {
            Some(RogerCommandId::RogerReturn)
        }
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RogerCommand {
    pub command_id: RogerCommandId,
    pub review_session_id: Option<String>,
    pub review_run_id: Option<String>,
    #[serde(default)]
    pub args: HashMap<String, String>,
    pub invocation_surface: RogerCommandInvocationSurface,
    pub provider: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessCommandBinding {
    pub provider: String,
    pub command_id: RogerCommandId,
    pub provider_command_syntax: String,
    #[serde(default)]
    pub capability_requirements: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RogerCommandRouteStatus {
    Routed,
    FallbackRequired,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RogerCommandNextAction {
    pub canonical_operation: String,
    pub fallback_cli_command: String,
    pub session_finder_hint: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RogerCommandResult {
    pub status: RogerCommandRouteStatus,
    pub user_message: String,
    pub next_action: RogerCommandNextAction,
    pub session_binding: Option<String>,
}

pub fn safe_harness_command_bindings(provider: &str) -> Vec<HarnessCommandBinding> {
    if !provider.eq_ignore_ascii_case("opencode") {
        return Vec::new();
    }

    vec![
        HarnessCommandBinding {
            provider: "opencode".to_owned(),
            command_id: RogerCommandId::RogerHelp,
            provider_command_syntax: "/roger-help".to_owned(),
            capability_requirements: vec!["supports_roger_commands".to_owned()],
        },
        HarnessCommandBinding {
            provider: "opencode".to_owned(),
            command_id: RogerCommandId::RogerStatus,
            provider_command_syntax: "/roger-status".to_owned(),
            capability_requirements: vec!["supports_roger_commands".to_owned()],
        },
        HarnessCommandBinding {
            provider: "opencode".to_owned(),
            command_id: RogerCommandId::RogerFindings,
            provider_command_syntax: "/roger-findings".to_owned(),
            capability_requirements: vec!["supports_roger_commands".to_owned()],
        },
        HarnessCommandBinding {
            provider: "opencode".to_owned(),
            command_id: RogerCommandId::RogerReturn,
            provider_command_syntax: "/roger-return".to_owned(),
            capability_requirements: vec!["supports_roger_commands".to_owned()],
        },
    ]
}

pub fn route_harness_command(
    command: &RogerCommand,
    bindings: &[HarnessCommandBinding],
) -> RogerCommandResult {
    let next_action = RogerCommandNextAction {
        canonical_operation: command.command_id.canonical_operation().to_owned(),
        fallback_cli_command: command.command_id.fallback_cli_command().to_owned(),
        session_finder_hint: session_finder_hint(command.command_id),
    };

    let provider_has_binding = bindings.iter().any(|binding| {
        binding.provider.eq_ignore_ascii_case(&command.provider)
            && binding.command_id == command.command_id
    });

    if provider_has_binding {
        return RogerCommandResult {
            status: RogerCommandRouteStatus::Routed,
            user_message: format!(
                "routed '{}' through Roger core operation '{}'",
                command.command_id.logical_id(),
                next_action.canonical_operation
            ),
            next_action,
            session_binding: command.review_session_id.clone(),
        };
    }

    RogerCommandResult {
        status: RogerCommandRouteStatus::FallbackRequired,
        user_message: format!(
            "provider '{}' does not expose '{}' in-harness; use '{}' instead",
            command.provider,
            command.command_id.logical_id(),
            next_action.fallback_cli_command
        ),
        next_action,
        session_binding: command.review_session_id.clone(),
    }
}

fn session_finder_hint(command_id: RogerCommandId) -> Option<String> {
    match command_id {
        RogerCommandId::RogerHelp => None,
        RogerCommandId::RogerStatus | RogerCommandId::RogerFindings => Some(
            "if session selection is ambiguous, re-run with --session <id> or --pr <number>"
                .to_owned(),
        ),
        RogerCommandId::RogerReturn => Some(
            "if return context is ambiguous, re-run rr return with --session <id> or --pr <number>"
                .to_owned(),
        ),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContinuityQuality {
    Usable,
    Degraded,
    Unusable,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LaunchAction {
    StartReview,
    ResumeReview,
    RefreshFindings,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FindingTriageState {
    New,
    Accepted,
    Ignored,
    NeedsFollowUp,
    Resolved,
    Stale,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FindingOutboundState {
    NotDrafted,
    Drafted,
    Approved,
    Posted,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApprovalState {
    NotDrafted,
    Drafted,
    Approved,
    Invalidated,
    Posted,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PostedActionStatus {
    Succeeded,
    Failed,
    Partial,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IndexLifecycleStatus {
    Pending,
    Ready,
    Stale,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvidenceRole {
    Primary,
    Supporting,
    Contradicting,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewTarget {
    pub repository: String,
    pub pull_request_number: u64,
    pub base_ref: String,
    pub head_ref: String,
    pub base_commit: String,
    pub head_commit: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchIntent {
    pub action: LaunchAction,
    pub source_surface: Surface,
    pub objective: Option<String>,
    pub launch_profile_id: Option<String>,
    pub cwd: Option<String>,
    pub worktree_root: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionLocator {
    pub provider: String,
    pub session_id: String,
    pub invocation_context_json: String,
    pub captured_at: Timestamp,
    pub last_tested_at: Option<Timestamp>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResumeBundleProfile {
    DropoutControl,
    ReseedResume,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumeBundle {
    pub schema_version: u32,
    pub profile: ResumeBundleProfile,
    pub review_target: ReviewTarget,
    pub launch_intent: LaunchIntent,
    pub provider: String,
    pub continuity_quality: ContinuityQuality,
    pub stage_summary: String,
    pub unresolved_finding_ids: Vec<String>,
    pub outbound_draft_ids: Vec<String>,
    pub attention_summary: String,
    pub artifact_refs: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionLaunchBinding {
    pub id: String,
    pub review_session_id: String,
    pub repo_locator: String,
    pub review_target: Option<ReviewTarget>,
    pub surface: Surface,
    pub launch_profile_id: Option<String>,
    pub ui_target: Option<String>,
    pub instance_preference: Option<String>,
    pub cwd: Option<String>,
    pub worktree_root: Option<String>,
    pub claimed_at: Timestamp,
    pub updated_at: Timestamp,
    pub row_version: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResumeAttemptOutcome {
    ReopenedUsable,
    ReopenedDegraded,
    ReopenUnavailable,
    MissingHarnessState,
    TargetMismatch,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResumeStrategy {
    ReopenExisting,
    ReseedFromBundle,
    FailClosed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderContinuityCapability {
    ReopenByLocator,
    ReseedOnly,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumeSessionState {
    pub locator_present: bool,
    pub resume_bundle_present: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResumeDecisionReason {
    LocatorReopenedUsable,
    ReopenedDegradedNeedsReseed,
    ReopenUnavailableNeedsReseed,
    MissingHarnessStateNeedsReseed,
    TargetMismatchNeedsReseed,
    ProviderLimitedNeedsReseed,
    ReopenedDegradedWithoutBundle,
    ReopenUnavailableWithoutBundle,
    MissingHarnessStateWithoutBundle,
    TargetMismatchWithoutBundle,
    ProviderLimitedWithoutBundle,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumeDecision {
    pub strategy: ResumeStrategy,
    pub continuity_quality: ContinuityQuality,
    pub reason_code: ResumeDecisionReason,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalLaunchProfile {
    pub id: String,
    pub name: String,
    pub source_surface: Surface,
    pub ui_target: UiTarget,
    pub terminal_environment: TerminalEnvironment,
    pub multiplexer_mode: MultiplexerMode,
    pub reuse_policy: ReusePolicy,
    pub repo_root: String,
    pub worktree_root: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub row_version: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiTarget {
    Cli,
    Tui,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalEnvironment {
    SystemDefault,
    VscodeIntegratedTerminal,
    WeztermWindow,
    WeztermSplit,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MultiplexerMode {
    None,
    Ntm,
    WeztermSplit,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReusePolicy {
    ReuseIfPossible,
    AlwaysNew,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchRoutingRequest {
    pub source_surface: Surface,
    pub profile: LocalLaunchProfile,
    pub available_terminal_environments: Vec<TerminalEnvironment>,
    pub available_multiplexer_modes: Vec<MultiplexerMode>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchRoutingDecision {
    pub source_surface: Surface,
    pub profile_id: String,
    pub ui_target: UiTarget,
    pub terminal_environment: TerminalEnvironment,
    pub multiplexer_mode: MultiplexerMode,
    pub reuse_policy: ReusePolicy,
    pub degraded: bool,
    pub reason: Option<String>,
}

pub fn resolve_launch_routing(input: LaunchRoutingRequest) -> LaunchRoutingDecision {
    let mut degraded_reasons = Vec::new();

    let requested_terminal = input.profile.terminal_environment.clone();
    let requested_muxer = input.profile.multiplexer_mode.clone();

    let terminal_environment = if input.available_terminal_environments.is_empty()
        || input
            .available_terminal_environments
            .contains(&requested_terminal)
    {
        requested_terminal
    } else {
        degraded_reasons.push(format!(
            "requested terminal environment {:?} unavailable; fell back to {:?}",
            requested_terminal, input.available_terminal_environments[0]
        ));
        input.available_terminal_environments[0].clone()
    };

    let multiplexer_mode = if input.available_multiplexer_modes.is_empty()
        || input.available_multiplexer_modes.contains(&requested_muxer)
    {
        requested_muxer
    } else if input
        .available_multiplexer_modes
        .contains(&MultiplexerMode::None)
    {
        degraded_reasons.push("requested multiplexer unavailable; fell back to none".to_owned());
        MultiplexerMode::None
    } else {
        degraded_reasons.push(format!(
            "requested multiplexer {:?} unavailable; fell back to {:?}",
            requested_muxer, input.available_multiplexer_modes[0]
        ));
        input.available_multiplexer_modes[0].clone()
    };

    let reason = if degraded_reasons.is_empty() {
        None
    } else {
        Some(degraded_reasons.join("; "))
    };

    LaunchRoutingDecision {
        source_surface: input.source_surface,
        profile_id: input.profile.id,
        ui_target: input.profile.ui_target,
        terminal_environment,
        multiplexer_mode,
        reuse_policy: input.profile.reuse_policy,
        degraded: reason.is_some(),
        reason,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewSession {
    pub id: String,
    pub review_target: ReviewTarget,
    pub provider: String,
    pub continuity_state: String,
    pub resume_bundle_artifact_id: Option<String>,
    pub launch_profile_id: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub row_version: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewRun {
    pub id: String,
    pub review_session_id: String,
    pub stage: String,
    pub continuity_quality: ContinuityQuality,
    pub session_locator_artifact_id: Option<String>,
    pub created_at: Timestamp,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub id: String,
    pub review_session_id: String,
    pub review_run_id: String,
    pub fingerprint: String,
    pub title: String,
    pub normalized_summary: String,
    pub severity: String,
    pub confidence: String,
    pub triage_state: FindingTriageState,
    pub outbound_state: FindingOutboundState,
    pub first_seen_at: Timestamp,
    pub last_seen_at: Timestamp,
    pub row_version: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboundDraftBatch {
    pub id: String,
    pub review_session_id: String,
    pub review_run_id: String,
    pub repo_id: String,
    pub remote_review_target_id: String,
    pub payload_digest: String,
    pub approval_state: ApprovalState,
    pub approved_at: Option<Timestamp>,
    pub invalidated_at: Option<Timestamp>,
    pub invalidation_reason_code: Option<String>,
    pub row_version: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboundDraft {
    pub id: String,
    pub review_session_id: String,
    pub review_run_id: String,
    pub finding_id: Option<String>,
    pub draft_batch_id: String,
    pub repo_id: String,
    pub remote_review_target_id: String,
    pub payload_digest: String,
    pub approval_state: ApprovalState,
    pub anchor_digest: String,
    pub row_version: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboundApprovalToken {
    pub id: String,
    pub draft_batch_id: String,
    pub payload_digest: String,
    pub target_tuple_json: String,
    pub approved_at: Timestamp,
    pub revoked_at: Option<Timestamp>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostedAction {
    pub id: String,
    pub draft_batch_id: String,
    pub provider: String,
    pub remote_identifier: String,
    pub status: PostedActionStatus,
    pub posted_payload_digest: String,
    pub posted_at: Timestamp,
    pub failure_code: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboundDraftBatchIssue {
    pub draft_id: Option<String>,
    pub reason_code: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboundDraftBatchValidation {
    pub valid: bool,
    pub issues: Vec<OutboundDraftBatchIssue>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutboundPostGateDecision {
    PostAllowed,
    Blocked { reason_code: String },
}

pub struct OutboundPostGateInput<'a> {
    pub batch: &'a OutboundDraftBatch,
    pub drafts: &'a [OutboundDraft],
    pub approval: &'a OutboundApprovalToken,
    pub refresh_signals: &'a [DraftRefreshSignal],
    pub reconfirmed_finding_ids: &'a HashSet<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PostingAdapterItemStatus {
    Posted,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostingAdapterItemResult {
    pub draft_id: String,
    pub status: PostingAdapterItemStatus,
    pub remote_identifier: Option<String>,
    pub failure_code: Option<String>,
}

pub trait OutboundPostingAdapter {
    fn post_approved_draft_batch(
        &self,
        batch: &OutboundDraftBatch,
        drafts: &[OutboundDraft],
    ) -> std::result::Result<Vec<PostingAdapterItemResult>, String>;
}

pub struct ExplicitPostingInput<'a> {
    pub action_id: &'a str,
    pub provider: &'a str,
    pub batch: &'a OutboundDraftBatch,
    pub drafts: &'a [OutboundDraft],
    pub approval: &'a OutboundApprovalToken,
    pub refresh_signals: &'a [DraftRefreshSignal],
    pub reconfirmed_finding_ids: &'a HashSet<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExplicitPostingOutcome {
    Posted,
    Partial,
    Failed,
    Blocked,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExplicitPostingResult {
    pub outcome: ExplicitPostingOutcome,
    pub reason_code: Option<String>,
    pub posted_action: Option<PostedAction>,
    pub item_results: Vec<PostingAdapterItemResult>,
    pub retry_draft_ids: Vec<String>,
}

pub fn outbound_target_tuple_json(batch: &OutboundDraftBatch) -> String {
    serde_json::json!({
        "review_session_id": batch.review_session_id,
        "repo_id": batch.repo_id,
        "remote_review_target_id": batch.remote_review_target_id,
    })
    .to_string()
}

pub fn validate_outbound_draft_batch_linkage(
    batch: &OutboundDraftBatch,
    drafts: &[OutboundDraft],
) -> OutboundDraftBatchValidation {
    let mut issues = Vec::new();
    if drafts.is_empty() {
        issues.push(OutboundDraftBatchIssue {
            draft_id: None,
            reason_code: "empty_batch".to_owned(),
        });
    }

    for draft in drafts {
        if draft.review_session_id != batch.review_session_id {
            issues.push(OutboundDraftBatchIssue {
                draft_id: Some(draft.id.clone()),
                reason_code: "session_mismatch".to_owned(),
            });
        }
        if draft.review_run_id != batch.review_run_id {
            issues.push(OutboundDraftBatchIssue {
                draft_id: Some(draft.id.clone()),
                reason_code: "run_mismatch".to_owned(),
            });
        }
        if draft.draft_batch_id != batch.id {
            issues.push(OutboundDraftBatchIssue {
                draft_id: Some(draft.id.clone()),
                reason_code: "batch_mismatch".to_owned(),
            });
        }
        if draft.repo_id != batch.repo_id
            || draft.remote_review_target_id != batch.remote_review_target_id
        {
            issues.push(OutboundDraftBatchIssue {
                draft_id: Some(draft.id.clone()),
                reason_code: "target_mismatch".to_owned(),
            });
        }
        if draft.payload_digest != batch.payload_digest {
            issues.push(OutboundDraftBatchIssue {
                draft_id: Some(draft.id.clone()),
                reason_code: "payload_digest_mismatch".to_owned(),
            });
        }
        if draft.finding_id.is_none() {
            issues.push(OutboundDraftBatchIssue {
                draft_id: Some(draft.id.clone()),
                reason_code: "missing_finding_link".to_owned(),
            });
        }
    }

    OutboundDraftBatchValidation {
        valid: issues.is_empty(),
        issues,
    }
}

pub fn evaluate_outbound_post_gate(input: OutboundPostGateInput<'_>) -> OutboundPostGateDecision {
    let validation = validate_outbound_draft_batch_linkage(input.batch, input.drafts);
    if !validation.valid {
        return OutboundPostGateDecision::Blocked {
            reason_code: "batch_linkage_invalid".to_owned(),
        };
    }

    if input.approval.draft_batch_id != input.batch.id {
        return OutboundPostGateDecision::Blocked {
            reason_code: "approval_batch_mismatch".to_owned(),
        };
    }

    if input.approval.revoked_at.is_some() {
        return OutboundPostGateDecision::Blocked {
            reason_code: "approval_revoked".to_owned(),
        };
    }

    if input.batch.invalidated_at.is_some() {
        return OutboundPostGateDecision::Blocked {
            reason_code: format!(
                "approval_invalidated:{}",
                input
                    .batch
                    .invalidation_reason_code
                    .as_deref()
                    .unwrap_or("unspecified")
            ),
        };
    }

    if input.approval.payload_digest != input.batch.payload_digest {
        return OutboundPostGateDecision::Blocked {
            reason_code: "approval_payload_digest_mismatch".to_owned(),
        };
    }

    let expected_target_tuple = outbound_target_tuple_json(input.batch);
    if input.approval.target_tuple_json != expected_target_tuple {
        return OutboundPostGateDecision::Blocked {
            reason_code: "approval_target_tuple_mismatch".to_owned(),
        };
    }

    let signals_by_finding: HashMap<&str, &DraftRefreshSignal> = input
        .refresh_signals
        .iter()
        .map(|signal| (signal.finding_id.as_str(), signal))
        .collect();

    for draft in input.drafts {
        let Some(finding_id) = draft.finding_id.as_deref() else {
            continue;
        };
        let Some(signal) = signals_by_finding.get(finding_id) else {
            continue;
        };

        match signal.kind {
            DraftRefreshSignalKind::Invalidate => {
                return OutboundPostGateDecision::Blocked {
                    reason_code: format!("refresh_invalidated:{}", signal.reason_code),
                };
            }
            DraftRefreshSignalKind::Reconfirm => {
                if !input.reconfirmed_finding_ids.contains(finding_id) {
                    return OutboundPostGateDecision::Blocked {
                        reason_code: format!("reconfirmation_required:{finding_id}"),
                    };
                }
            }
        }
    }

    OutboundPostGateDecision::PostAllowed
}

pub fn execute_explicit_posting_flow(
    input: ExplicitPostingInput<'_>,
    adapter: &dyn OutboundPostingAdapter,
) -> ExplicitPostingResult {
    let gate_decision = evaluate_outbound_post_gate(OutboundPostGateInput {
        batch: input.batch,
        drafts: input.drafts,
        approval: input.approval,
        refresh_signals: input.refresh_signals,
        reconfirmed_finding_ids: input.reconfirmed_finding_ids,
    });

    if let OutboundPostGateDecision::Blocked { reason_code } = gate_decision {
        return ExplicitPostingResult {
            outcome: ExplicitPostingOutcome::Blocked,
            reason_code: Some(reason_code),
            posted_action: None,
            item_results: Vec::new(),
            retry_draft_ids: Vec::new(),
        };
    }

    let retry_all_drafts = input
        .drafts
        .iter()
        .map(|draft| draft.id.clone())
        .collect::<Vec<_>>();

    let item_results = match adapter.post_approved_draft_batch(input.batch, input.drafts) {
        Ok(results) => results,
        Err(err) => {
            let reason_code = format!("adapter_error:{err}");
            return ExplicitPostingResult {
                outcome: ExplicitPostingOutcome::Failed,
                reason_code: Some(reason_code.clone()),
                posted_action: Some(PostedAction {
                    id: input.action_id.to_owned(),
                    draft_batch_id: input.batch.id.clone(),
                    provider: input.provider.to_owned(),
                    remote_identifier: "adapter_error".to_owned(),
                    status: PostedActionStatus::Failed,
                    posted_payload_digest: input.batch.payload_digest.clone(),
                    posted_at: now_ts(),
                    failure_code: Some(reason_code),
                }),
                item_results: Vec::new(),
                retry_draft_ids: retry_all_drafts,
            };
        }
    };

    if let Err(reason_code) = validate_posting_adapter_results(input.drafts, &item_results) {
        return ExplicitPostingResult {
            outcome: ExplicitPostingOutcome::Failed,
            reason_code: Some(reason_code.clone()),
            posted_action: Some(PostedAction {
                id: input.action_id.to_owned(),
                draft_batch_id: input.batch.id.clone(),
                provider: input.provider.to_owned(),
                remote_identifier: "adapter_result_invalid".to_owned(),
                status: PostedActionStatus::Failed,
                posted_payload_digest: input.batch.payload_digest.clone(),
                posted_at: now_ts(),
                failure_code: Some(reason_code),
            }),
            item_results,
            retry_draft_ids: retry_all_drafts,
        };
    }

    let succeeded = item_results
        .iter()
        .filter(|item| item.status == PostingAdapterItemStatus::Posted)
        .count();
    let total = input.drafts.len();
    let retry_draft_ids = item_results
        .iter()
        .filter(|item| item.status == PostingAdapterItemStatus::Failed)
        .map(|item| item.draft_id.clone())
        .collect::<Vec<_>>();

    let (outcome, status, failure_code) = if succeeded == total {
        (
            ExplicitPostingOutcome::Posted,
            PostedActionStatus::Succeeded,
            None,
        )
    } else if succeeded == 0 {
        (
            ExplicitPostingOutcome::Failed,
            PostedActionStatus::Failed,
            Some(
                item_results
                    .iter()
                    .find_map(|item| item.failure_code.clone())
                    .unwrap_or_else(|| "post_failed".to_owned()),
            ),
        )
    } else {
        (
            ExplicitPostingOutcome::Partial,
            PostedActionStatus::Partial,
            Some("partial_failure".to_owned()),
        )
    };

    let posted_action = PostedAction {
        id: input.action_id.to_owned(),
        draft_batch_id: input.batch.id.clone(),
        provider: input.provider.to_owned(),
        remote_identifier: aggregate_remote_identifiers(&item_results),
        status,
        posted_payload_digest: input.batch.payload_digest.clone(),
        posted_at: now_ts(),
        failure_code: failure_code.clone(),
    };

    ExplicitPostingResult {
        outcome,
        reason_code: failure_code,
        posted_action: Some(posted_action),
        item_results,
        retry_draft_ids,
    }
}

fn validate_posting_adapter_results(
    drafts: &[OutboundDraft],
    item_results: &[PostingAdapterItemResult],
) -> std::result::Result<(), String> {
    let expected = drafts
        .iter()
        .map(|draft| draft.id.as_str())
        .collect::<HashSet<_>>();
    if item_results.len() != expected.len() {
        return Err("adapter_result_invalid:missing_draft_results".to_owned());
    }

    let mut seen = HashSet::new();
    for item in item_results {
        if !expected.contains(item.draft_id.as_str()) {
            return Err(format!(
                "adapter_result_invalid:unexpected_draft:{}",
                item.draft_id
            ));
        }
        if !seen.insert(item.draft_id.as_str()) {
            return Err(format!(
                "adapter_result_invalid:duplicate_draft:{}",
                item.draft_id
            ));
        }
        if item.status == PostingAdapterItemStatus::Posted && item.remote_identifier.is_none() {
            return Err(format!(
                "adapter_result_invalid:missing_remote_identifier:{}",
                item.draft_id
            ));
        }
    }

    Ok(())
}

fn aggregate_remote_identifiers(item_results: &[PostingAdapterItemResult]) -> String {
    let mut remote_ids = item_results
        .iter()
        .filter(|item| item.status == PostingAdapterItemStatus::Posted)
        .filter_map(|item| item.remote_identifier.as_ref())
        .cloned()
        .collect::<Vec<_>>();
    remote_ids.sort();
    remote_ids.dedup();

    if remote_ids.is_empty() {
        "none".to_owned()
    } else if remote_ids.len() == 1 {
        remote_ids[0].clone()
    } else {
        remote_ids.join(",")
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexState {
    pub index_name: String,
    pub generation: i64,
    pub status: IndexLifecycleStatus,
    pub last_built_at: Option<Timestamp>,
    pub artifact_id: Option<String>,
    pub row_version: i64,
}

pub fn decide_resume_strategy(
    capability: ProviderContinuityCapability,
    session_state: &ResumeSessionState,
    outcome: ResumeAttemptOutcome,
) -> ResumeDecision {
    if matches!(capability, ProviderContinuityCapability::ReseedOnly) {
        return if session_state.resume_bundle_present {
            ResumeDecision {
                strategy: ResumeStrategy::ReseedFromBundle,
                continuity_quality: ContinuityQuality::Degraded,
                reason_code: ResumeDecisionReason::ProviderLimitedNeedsReseed,
                reason: "provider does not support locator reopen; reseeding from ResumeBundle"
                    .to_owned(),
            }
        } else {
            ResumeDecision {
                strategy: ResumeStrategy::FailClosed,
                continuity_quality: ContinuityQuality::Unusable,
                reason_code: ResumeDecisionReason::ProviderLimitedWithoutBundle,
                reason: "provider does not support locator reopen and no ResumeBundle is available"
                    .to_owned(),
            }
        };
    }

    match outcome {
        ResumeAttemptOutcome::ReopenedUsable => ResumeDecision {
            strategy: ResumeStrategy::ReopenExisting,
            continuity_quality: ContinuityQuality::Usable,
            reason_code: ResumeDecisionReason::LocatorReopenedUsable,
            reason: "session locator reopened successfully".to_owned(),
        },
        ResumeAttemptOutcome::ReopenedDegraded if session_state.resume_bundle_present => {
            ResumeDecision {
                strategy: ResumeStrategy::ReseedFromBundle,
                continuity_quality: ContinuityQuality::Degraded,
                reason_code: ResumeDecisionReason::ReopenedDegradedNeedsReseed,
                reason: "reopened session was degraded; reseeding from ResumeBundle".to_owned(),
            }
        }
        ResumeAttemptOutcome::ReopenUnavailable if session_state.resume_bundle_present => {
            ResumeDecision {
                strategy: ResumeStrategy::ReseedFromBundle,
                continuity_quality: ContinuityQuality::Degraded,
                reason_code: ResumeDecisionReason::ReopenUnavailableNeedsReseed,
                reason: "direct reopen unavailable; reseeding from ResumeBundle".to_owned(),
            }
        }
        ResumeAttemptOutcome::MissingHarnessState if session_state.resume_bundle_present => {
            ResumeDecision {
                strategy: ResumeStrategy::ReseedFromBundle,
                continuity_quality: ContinuityQuality::Degraded,
                reason_code: ResumeDecisionReason::MissingHarnessStateNeedsReseed,
                reason: "harness state is missing; reseeding from ResumeBundle".to_owned(),
            }
        }
        ResumeAttemptOutcome::TargetMismatch if session_state.resume_bundle_present => {
            ResumeDecision {
                strategy: ResumeStrategy::ReseedFromBundle,
                continuity_quality: ContinuityQuality::Degraded,
                reason_code: ResumeDecisionReason::TargetMismatchNeedsReseed,
                reason: "reopened session target does not match; reseeding from ResumeBundle"
                    .to_owned(),
            }
        }
        ResumeAttemptOutcome::ReopenedDegraded => ResumeDecision {
            strategy: ResumeStrategy::FailClosed,
            continuity_quality: ContinuityQuality::Unusable,
            reason_code: ResumeDecisionReason::ReopenedDegradedWithoutBundle,
            reason: "reopened session was degraded and no ResumeBundle is available".to_owned(),
        },
        ResumeAttemptOutcome::ReopenUnavailable => ResumeDecision {
            strategy: ResumeStrategy::FailClosed,
            continuity_quality: ContinuityQuality::Unusable,
            reason_code: ResumeDecisionReason::ReopenUnavailableWithoutBundle,
            reason: if session_state.locator_present {
                "session locator could not reopen and no ResumeBundle is available".to_owned()
            } else {
                "no session locator or ResumeBundle is available".to_owned()
            },
        },
        ResumeAttemptOutcome::MissingHarnessState => ResumeDecision {
            strategy: ResumeStrategy::FailClosed,
            continuity_quality: ContinuityQuality::Unusable,
            reason_code: ResumeDecisionReason::MissingHarnessStateWithoutBundle,
            reason: "harness state is missing and no ResumeBundle is available".to_owned(),
        },
        ResumeAttemptOutcome::TargetMismatch => ResumeDecision {
            strategy: ResumeStrategy::FailClosed,
            continuity_quality: ContinuityQuality::Unusable,
            reason_code: ResumeDecisionReason::TargetMismatchWithoutBundle,
            reason: "reopened session target does not match and no ResumeBundle is available"
                .to_owned(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_target() -> ReviewTarget {
        ReviewTarget {
            repository: "owner/repo".to_owned(),
            pull_request_number: 42,
            base_ref: "main".to_owned(),
            head_ref: "feature".to_owned(),
            base_commit: "abc".to_owned(),
            head_commit: "def".to_owned(),
        }
    }

    #[test]
    fn degraded_reopen_prefers_reseed_when_bundle_exists() {
        let decision = decide_resume_strategy(
            ProviderContinuityCapability::ReopenByLocator,
            &ResumeSessionState {
                locator_present: true,
                resume_bundle_present: true,
            },
            ResumeAttemptOutcome::ReopenedDegraded,
        );
        assert_eq!(decision.strategy, ResumeStrategy::ReseedFromBundle);
        assert_eq!(decision.continuity_quality, ContinuityQuality::Degraded);
        assert_eq!(
            decision.reason_code,
            ResumeDecisionReason::ReopenedDegradedNeedsReseed
        );
    }

    #[test]
    fn missing_state_fails_closed_without_bundle() {
        let decision = decide_resume_strategy(
            ProviderContinuityCapability::ReopenByLocator,
            &ResumeSessionState {
                locator_present: true,
                resume_bundle_present: false,
            },
            ResumeAttemptOutcome::MissingHarnessState,
        );
        assert_eq!(decision.strategy, ResumeStrategy::FailClosed);
        assert_eq!(decision.continuity_quality, ContinuityQuality::Unusable);
        assert_eq!(
            decision.reason_code,
            ResumeDecisionReason::MissingHarnessStateWithoutBundle
        );
    }

    #[test]
    fn provider_limited_resume_uses_reseed_when_bundle_exists() {
        let decision = decide_resume_strategy(
            ProviderContinuityCapability::ReseedOnly,
            &ResumeSessionState {
                locator_present: false,
                resume_bundle_present: true,
            },
            ResumeAttemptOutcome::ReopenUnavailable,
        );
        assert_eq!(decision.strategy, ResumeStrategy::ReseedFromBundle);
        assert_eq!(decision.continuity_quality, ContinuityQuality::Degraded);
        assert_eq!(
            decision.reason_code,
            ResumeDecisionReason::ProviderLimitedNeedsReseed
        );
    }

    #[test]
    fn target_mismatch_is_classified_and_fails_closed_without_bundle() {
        let decision = decide_resume_strategy(
            ProviderContinuityCapability::ReopenByLocator,
            &ResumeSessionState {
                locator_present: true,
                resume_bundle_present: false,
            },
            ResumeAttemptOutcome::TargetMismatch,
        );
        assert_eq!(decision.strategy, ResumeStrategy::FailClosed);
        assert_eq!(decision.continuity_quality, ContinuityQuality::Unusable);
        assert_eq!(
            decision.reason_code,
            ResumeDecisionReason::TargetMismatchWithoutBundle
        );
    }

    #[test]
    fn resume_bundle_round_trip_shape_is_serializable() {
        let bundle = ResumeBundle {
            schema_version: 1,
            profile: ResumeBundleProfile::ReseedResume,
            review_target: sample_target(),
            launch_intent: LaunchIntent {
                action: LaunchAction::ResumeReview,
                source_surface: Surface::Cli,
                objective: Some("resume the last review".to_owned()),
                launch_profile_id: Some("profile-1".to_owned()),
                cwd: Some("/tmp/repo".to_owned()),
                worktree_root: None,
            },
            provider: "opencode".to_owned(),
            continuity_quality: ContinuityQuality::Degraded,
            stage_summary: "awaiting follow-up".to_owned(),
            unresolved_finding_ids: vec!["finding-1".to_owned()],
            outbound_draft_ids: vec!["draft-1".to_owned()],
            attention_summary: "approval required".to_owned(),
            artifact_refs: vec!["artifact-1".to_owned()],
        };

        let encoded = serde_json::to_string(&bundle).expect("serialize bundle");
        let decoded: ResumeBundle = serde_json::from_str(&encoded).expect("deserialize bundle");
        assert_eq!(decoded, bundle);
    }

    #[test]
    fn parse_harness_command_id_accepts_common_spellings() {
        assert_eq!(
            parse_harness_command_id("/roger-status"),
            Some(RogerCommandId::RogerStatus)
        );
        assert_eq!(
            parse_harness_command_id(":roger findings"),
            Some(RogerCommandId::RogerFindings)
        );
        assert_eq!(
            parse_harness_command_id("roger-return"),
            Some(RogerCommandId::RogerReturn)
        );
        assert_eq!(parse_harness_command_id("/not-roger"), None);
    }

    #[test]
    fn route_harness_command_uses_binding_for_opencode_safe_subset() {
        let command = RogerCommand {
            command_id: RogerCommandId::RogerStatus,
            review_session_id: Some("session-1".to_owned()),
            review_run_id: None,
            args: std::collections::HashMap::new(),
            invocation_surface: RogerCommandInvocationSurface::HarnessCommand,
            provider: "opencode".to_owned(),
        };

        let routed = route_harness_command(&command, &safe_harness_command_bindings("opencode"));
        assert_eq!(routed.status, RogerCommandRouteStatus::Routed);
        assert_eq!(routed.next_action.canonical_operation, "show_status");
        assert_eq!(routed.next_action.fallback_cli_command, "rr status");
    }

    #[test]
    fn route_harness_command_returns_truthful_fallback_when_binding_is_missing() {
        let command = RogerCommand {
            command_id: RogerCommandId::RogerReturn,
            review_session_id: Some("session-1".to_owned()),
            review_run_id: None,
            args: std::collections::HashMap::new(),
            invocation_surface: RogerCommandInvocationSurface::HarnessCommand,
            provider: "gemini".to_owned(),
        };

        let routed = route_harness_command(&command, &safe_harness_command_bindings("gemini"));
        assert_eq!(routed.status, RogerCommandRouteStatus::FallbackRequired);
        assert!(routed.user_message.contains("provider 'gemini'"));
        assert_eq!(routed.next_action.fallback_cli_command, "rr return");
        assert!(
            routed
                .next_action
                .session_finder_hint
                .expect("hint")
                .contains("--session <id>")
        );
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructuredFindingsPackV1 {
    pub schema_version: String,
    pub stage: String,
    #[serde(default)]
    pub findings: Vec<StructuredFindingInput>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructuredFindingInput {
    pub fingerprint: String,
    pub title: String,
    pub normalized_summary: String,
    pub severity: String,
    pub confidence: String,
    #[serde(default)]
    pub code_evidence: Vec<StructuredCodeEvidenceInput>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructuredCodeEvidenceInput {
    pub evidence_role: EvidenceRole,
    pub repo_rel_path: String,
    pub start_line: u32,
    #[serde(default)]
    pub start_column: Option<u32>,
    #[serde(default)]
    pub end_line: Option<u32>,
    #[serde(default)]
    pub end_column: Option<u32>,
    #[serde(default)]
    pub excerpt_artifact_id: Option<String>,
    #[serde(default)]
    pub anchor_digest: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindingsBoundaryInput<'a> {
    pub raw_output_artifact_id: Option<&'a str>,
    pub pack_json: Option<&'a str>,
    pub repair_attempt: u8,
    pub retry_budget: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FindingsBoundaryState {
    Structured,
    Partial,
    RawOnly,
    RepairNeeded,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RepairIssueCode {
    MissingPack,
    MalformedSyntax,
    SchemaDrift,
    InvalidFieldValue,
    InvalidAnchor,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepairIssue {
    pub code: RepairIssueCode,
    pub path: String,
    pub message: String,
    pub repairable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedCodeEvidenceLocation {
    pub evidence_role: EvidenceRole,
    pub repo_rel_path: String,
    pub start_line: u32,
    pub start_column: Option<u32>,
    pub end_line: Option<u32>,
    pub end_column: Option<u32>,
    pub excerpt_artifact_id: Option<String>,
    pub anchor_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedFindingCandidate {
    pub fingerprint: String,
    pub title: String,
    pub normalized_summary: String,
    pub severity: String,
    pub confidence: String,
    pub code_evidence_locations: Vec<NormalizedCodeEvidenceLocation>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedFindingsPack {
    pub stage: String,
    pub findings: Vec<NormalizedFindingCandidate>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindingsBoundaryResult {
    pub state: FindingsBoundaryState,
    pub raw_output_artifact_id: Option<String>,
    pub original_pack: Option<StructuredFindingsPackV1>,
    pub validated_pack: Option<ValidatedFindingsPack>,
    pub issues: Vec<RepairIssue>,
    pub should_retry: bool,
    pub remaining_retry_budget: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefreshCandidate {
    pub fingerprint: String,
    pub normalized_summary: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DraftCandidate {
    pub fingerprint: String,
    pub title: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefreshLifecycleState {
    CarriedForward,
    Superseded,
    Resolved,
    Stale,
    InvalidAnchor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DraftRefreshSignalKind {
    Invalidate,
    Reconfirm,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftRefreshSignal {
    pub finding_id: String,
    pub kind: DraftRefreshSignalKind,
    pub reason_code: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExistingFindingSnapshot {
    pub finding_id: String,
    pub fingerprint: String,
    pub primary_anchor_digest: Option<String>,
    pub outbound_state: FindingOutboundState,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefreshedFindingSnapshot {
    pub fingerprint: String,
    pub primary_anchor_digest: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefreshLifecycleTransition {
    pub finding_id: String,
    pub previous_fingerprint: String,
    pub next_state: RefreshLifecycleState,
    pub replacement_fingerprint: Option<String>,
    pub draft_signal: Option<DraftRefreshSignal>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefreshLifecycleResult {
    pub transitions: Vec<RefreshLifecycleTransition>,
    pub unmatched_new_fingerprints: Vec<String>,
}

pub fn classify_refresh_lifecycle(
    existing: &[ExistingFindingSnapshot],
    refreshed: &[RefreshedFindingSnapshot],
) -> RefreshLifecycleResult {
    let refreshed_by_fingerprint: HashMap<&str, &RefreshedFindingSnapshot> = refreshed
        .iter()
        .map(|candidate| (candidate.fingerprint.as_str(), candidate))
        .collect();
    let refreshed_by_anchor: HashMap<&str, &RefreshedFindingSnapshot> = refreshed
        .iter()
        .filter_map(|candidate| {
            candidate
                .primary_anchor_digest
                .as_deref()
                .map(|anchor| (anchor, candidate))
        })
        .collect();

    let mut consumed_new = HashSet::new();
    let mut transitions = Vec::with_capacity(existing.len());

    for previous in existing {
        let (next_state, replacement_fingerprint) =
            if let Some(current) = refreshed_by_fingerprint.get(previous.fingerprint.as_str()) {
                consumed_new.insert(current.fingerprint.clone());
                if anchors_match(
                    previous.primary_anchor_digest.as_deref(),
                    current.primary_anchor_digest.as_deref(),
                ) {
                    (RefreshLifecycleState::CarriedForward, None)
                } else {
                    (
                        RefreshLifecycleState::InvalidAnchor,
                        Some(current.fingerprint.clone()),
                    )
                }
            } else if let Some(anchor_digest) = previous.primary_anchor_digest.as_deref() {
                if let Some(current) = refreshed_by_anchor.get(anchor_digest) {
                    consumed_new.insert(current.fingerprint.clone());
                    (
                        RefreshLifecycleState::Superseded,
                        Some(current.fingerprint.clone()),
                    )
                } else {
                    (RefreshLifecycleState::Resolved, None)
                }
            } else {
                (RefreshLifecycleState::Stale, None)
            };

        let draft_signal = draft_refresh_signal_for(
            &previous.finding_id,
            previous.outbound_state.clone(),
            next_state,
            replacement_fingerprint.as_deref(),
        );

        transitions.push(RefreshLifecycleTransition {
            finding_id: previous.finding_id.clone(),
            previous_fingerprint: previous.fingerprint.clone(),
            next_state,
            replacement_fingerprint,
            draft_signal,
        });
    }

    let mut unmatched_new_fingerprints = refreshed
        .iter()
        .filter(|candidate| !consumed_new.contains(&candidate.fingerprint))
        .map(|candidate| candidate.fingerprint.clone())
        .collect::<Vec<_>>();
    unmatched_new_fingerprints.sort();

    RefreshLifecycleResult {
        transitions,
        unmatched_new_fingerprints,
    }
}

fn anchors_match(previous: Option<&str>, current: Option<&str>) -> bool {
    match (previous, current) {
        (Some(left), Some(right)) => left == right,
        (None, None) => true,
        _ => false,
    }
}

fn draft_refresh_signal_for(
    finding_id: &str,
    outbound_state: FindingOutboundState,
    next_state: RefreshLifecycleState,
    replacement_fingerprint: Option<&str>,
) -> Option<DraftRefreshSignal> {
    let reason_code = match next_state {
        RefreshLifecycleState::CarriedForward => "carried_forward_reconfirm",
        RefreshLifecycleState::Superseded => "superseded_invalidate",
        RefreshLifecycleState::Resolved => "resolved_invalidate",
        RefreshLifecycleState::Stale => "stale_invalidate",
        RefreshLifecycleState::InvalidAnchor => "invalid_anchor_invalidate",
    };

    let kind = match next_state {
        RefreshLifecycleState::CarriedForward => match outbound_state {
            FindingOutboundState::Approved => Some(DraftRefreshSignalKind::Reconfirm),
            _ => None,
        },
        RefreshLifecycleState::Superseded
        | RefreshLifecycleState::Resolved
        | RefreshLifecycleState::Stale
        | RefreshLifecycleState::InvalidAnchor => match outbound_state {
            FindingOutboundState::Drafted
            | FindingOutboundState::Approved
            | FindingOutboundState::Posted => Some(DraftRefreshSignalKind::Invalidate),
            _ => None,
        },
    }?;

    let detail = replacement_fingerprint
        .map(|fingerprint| format!("{reason_code}:{fingerprint}"))
        .unwrap_or_else(|| reason_code.to_owned());

    Some(DraftRefreshSignal {
        finding_id: finding_id.to_owned(),
        kind,
        reason_code: detail,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FindingsConsumptionError {
    FindingsNotValidated(FindingsBoundaryState),
}

pub fn validate_structured_findings_boundary(
    input: FindingsBoundaryInput<'_>,
) -> FindingsBoundaryResult {
    let remaining_retry_budget = input.retry_budget.saturating_sub(input.repair_attempt);

    let Some(pack_json) = input.pack_json else {
        let state = if input.raw_output_artifact_id.is_some() {
            FindingsBoundaryState::RawOnly
        } else {
            FindingsBoundaryState::Failed
        };
        return FindingsBoundaryResult {
            state,
            raw_output_artifact_id: input.raw_output_artifact_id.map(str::to_owned),
            original_pack: None,
            validated_pack: None,
            issues: vec![RepairIssue {
                code: RepairIssueCode::MissingPack,
                path: "$".to_owned(),
                message: "structured findings pack missing".to_owned(),
                repairable: input.raw_output_artifact_id.is_some(),
            }],
            should_retry: input.raw_output_artifact_id.is_some() && remaining_retry_budget > 0,
            remaining_retry_budget,
        };
    };

    let pack = match from_str::<StructuredFindingsPackV1>(pack_json) {
        Ok(pack) => pack,
        Err(err) => {
            let repairable = remaining_retry_budget > 0;
            return FindingsBoundaryResult {
                state: if repairable {
                    FindingsBoundaryState::RepairNeeded
                } else {
                    FindingsBoundaryState::Failed
                },
                raw_output_artifact_id: input.raw_output_artifact_id.map(str::to_owned),
                original_pack: None,
                validated_pack: None,
                issues: vec![RepairIssue {
                    code: RepairIssueCode::MalformedSyntax,
                    path: "$".to_owned(),
                    message: err.to_string(),
                    repairable,
                }],
                should_retry: repairable,
                remaining_retry_budget,
            };
        }
    };

    if pack.schema_version != "structured_findings_pack/v1" {
        let repairable = remaining_retry_budget > 0;
        return FindingsBoundaryResult {
            state: if repairable {
                FindingsBoundaryState::RepairNeeded
            } else {
                FindingsBoundaryState::Failed
            },
            raw_output_artifact_id: input.raw_output_artifact_id.map(str::to_owned),
            original_pack: Some(pack),
            validated_pack: None,
            issues: vec![RepairIssue {
                code: RepairIssueCode::SchemaDrift,
                path: "$.schema_version".to_owned(),
                message: "unsupported StructuredFindingsPack schema version".to_owned(),
                repairable,
            }],
            should_retry: repairable,
            remaining_retry_budget,
        };
    }

    let mut issues = Vec::new();
    let mut findings = Vec::new();

    for (finding_index, finding) in pack.findings.iter().enumerate() {
        if finding.fingerprint.trim().is_empty() {
            issues.push(RepairIssue {
                code: RepairIssueCode::InvalidFieldValue,
                path: format!("$.findings[{finding_index}].fingerprint"),
                message: "fingerprint must not be empty".to_owned(),
                repairable: true,
            });
            continue;
        }

        if finding.title.trim().is_empty() || finding.normalized_summary.trim().is_empty() {
            issues.push(RepairIssue {
                code: RepairIssueCode::InvalidFieldValue,
                path: format!("$.findings[{finding_index}]"),
                message: "title and normalized_summary must not be empty".to_owned(),
                repairable: true,
            });
            continue;
        }

        let mut normalized_locations = Vec::new();
        for (location_index, location) in finding.code_evidence.iter().enumerate() {
            let anchor_digest = location.anchor_digest.clone().unwrap_or_default();
            let valid_anchor = !location.repo_rel_path.trim().is_empty()
                && location.start_line > 0
                && !anchor_digest.trim().is_empty()
                && location
                    .end_line
                    .map(|end_line| end_line >= location.start_line)
                    .unwrap_or(true);

            if !valid_anchor {
                issues.push(RepairIssue {
                    code: RepairIssueCode::InvalidAnchor,
                    path: format!("$.findings[{finding_index}].code_evidence[{location_index}]"),
                    message: "invalid code evidence anchor; location salvaged out of finding"
                        .to_owned(),
                    repairable: true,
                });
                continue;
            }

            normalized_locations.push(NormalizedCodeEvidenceLocation {
                evidence_role: location.evidence_role.clone(),
                repo_rel_path: location.repo_rel_path.clone(),
                start_line: location.start_line,
                start_column: location.start_column,
                end_line: location.end_line,
                end_column: location.end_column,
                excerpt_artifact_id: location.excerpt_artifact_id.clone(),
                anchor_digest,
            });
        }

        findings.push(NormalizedFindingCandidate {
            fingerprint: finding.fingerprint.clone(),
            title: finding.title.clone(),
            normalized_summary: finding.normalized_summary.clone(),
            severity: finding.severity.clone(),
            confidence: finding.confidence.clone(),
            code_evidence_locations: normalized_locations,
        });
    }

    let state = if findings.is_empty() {
        if remaining_retry_budget > 0 {
            FindingsBoundaryState::RepairNeeded
        } else {
            FindingsBoundaryState::Failed
        }
    } else if issues.is_empty() {
        FindingsBoundaryState::Structured
    } else {
        FindingsBoundaryState::Partial
    };

    FindingsBoundaryResult {
        state,
        raw_output_artifact_id: input.raw_output_artifact_id.map(str::to_owned),
        original_pack: Some(pack.clone()),
        validated_pack: if findings.is_empty() {
            None
        } else {
            Some(ValidatedFindingsPack {
                stage: pack.stage.clone(),
                findings,
            })
        },
        issues,
        should_retry: matches!(state, FindingsBoundaryState::RepairNeeded)
            && remaining_retry_budget > 0,
        remaining_retry_budget,
    }
}

impl FindingsBoundaryResult {
    pub fn refresh_candidates(
        &self,
    ) -> std::result::Result<Vec<RefreshCandidate>, FindingsConsumptionError> {
        let pack = self
            .validated_pack
            .as_ref()
            .ok_or_else(|| FindingsConsumptionError::FindingsNotValidated(self.state.clone()))?;

        Ok(pack
            .findings
            .iter()
            .map(|finding| RefreshCandidate {
                fingerprint: finding.fingerprint.clone(),
                normalized_summary: finding.normalized_summary.clone(),
            })
            .collect())
    }

    pub fn draft_candidates(
        &self,
    ) -> std::result::Result<Vec<DraftCandidate>, FindingsConsumptionError> {
        let pack = self
            .validated_pack
            .as_ref()
            .ok_or_else(|| FindingsConsumptionError::FindingsNotValidated(self.state.clone()))?;

        Ok(pack
            .findings
            .iter()
            .map(|finding| DraftCandidate {
                fingerprint: finding.fingerprint.clone(),
                title: finding.title.clone(),
            })
            .collect())
    }
}
