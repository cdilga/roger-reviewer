pub use crate::time::now_ts;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, from_str};
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

impl FindingTriageState {
    pub fn as_str(&self) -> &'static str {
        match self {
            FindingTriageState::New => "New",
            FindingTriageState::Accepted => "Accepted",
            FindingTriageState::Ignored => "Ignored",
            FindingTriageState::NeedsFollowUp => "NeedsFollowUp",
            FindingTriageState::Resolved => "Resolved",
            FindingTriageState::Stale => "Stale",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FindingOutboundState {
    NotDrafted,
    Drafted,
    Approved,
    Posted,
    Failed,
}

impl FindingOutboundState {
    pub fn as_str(&self) -> &'static str {
        match self {
            FindingOutboundState::NotDrafted => "NotDrafted",
            FindingOutboundState::Drafted => "Drafted",
            FindingOutboundState::Approved => "Approved",
            FindingOutboundState::Posted => "Posted",
            FindingOutboundState::Failed => "Failed",
        }
    }
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

pub const WORKER_STAGE_RESULT_SCHEMA_V1: &str = "rr.worker.stage_result.v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewTaskKind {
    ExplorationPass,
    DeepReviewPass,
    FollowUpPass,
    RefreshCompare,
    ClarificationPass,
    RecheckFinding,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerTurnStrategy {
    SingleTurnReport,
    ConfiguredMultiTurnProgram,
    ManualFollowUp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerTransportKind {
    LegacyStageHarness,
    AgentCli,
    Mcp,
}

impl WorkerTransportKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LegacyStageHarness => "legacy_stage_harness",
            Self::AgentCli => "agent_cli",
            Self::Mcp => "mcp",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerMutationPosture {
    ReviewOnly,
    FixMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerGitHubPosture {
    Blocked,
    ApprovalRequired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerStageOutcome {
    Completed,
    CompletedPartial,
    NeedsClarification,
    NeedsContext,
    Abstained,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerInvocationOutcomeState {
    Running,
    Completed,
    CompletedPartial,
    NeedsClarification,
    NeedsContext,
    Abstained,
    Failed,
}

impl WorkerStageOutcome {
    pub fn invocation_state(self) -> WorkerInvocationOutcomeState {
        match self {
            Self::Completed => WorkerInvocationOutcomeState::Completed,
            Self::CompletedPartial => WorkerInvocationOutcomeState::CompletedPartial,
            Self::NeedsClarification => WorkerInvocationOutcomeState::NeedsClarification,
            Self::NeedsContext => WorkerInvocationOutcomeState::NeedsContext,
            Self::Abstained => WorkerInvocationOutcomeState::Abstained,
            Self::Failed => WorkerInvocationOutcomeState::Failed,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerToolCallOutcomeState {
    Succeeded,
    Denied,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewTask {
    pub id: String,
    pub review_session_id: String,
    pub review_run_id: String,
    pub stage: String,
    pub task_kind: ReviewTaskKind,
    pub task_nonce: String,
    pub objective: String,
    pub turn_strategy: WorkerTurnStrategy,
    #[serde(default)]
    pub allowed_scopes: Vec<String>,
    #[serde(default)]
    pub allowed_operations: Vec<String>,
    pub expected_result_schema: String,
    pub prompt_preset_id: Option<String>,
    pub created_at: Timestamp,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerFindingSummary {
    pub finding_id: String,
    pub fingerprint: String,
    pub summary: String,
    pub triage_state: String,
    pub outbound_state: String,
    pub primary_evidence_ref: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerMemoryCard {
    pub citation_id: String,
    pub scope: String,
    pub title: String,
    pub summary: String,
    pub provenance: String,
    pub trust_tier: String,
    pub tentative: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerArtifactRef {
    pub artifact_id: String,
    pub role: String,
    pub media_type: Option<String>,
    pub summary: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerContextPacket {
    pub review_target: ReviewTarget,
    pub review_session_id: String,
    pub review_run_id: String,
    pub review_task_id: String,
    pub task_nonce: String,
    pub baseline_snapshot_ref: Option<String>,
    pub provider: String,
    pub transport_kind: WorkerTransportKind,
    pub stage: String,
    pub objective: String,
    #[serde(default)]
    pub allowed_scopes: Vec<String>,
    #[serde(default)]
    pub allowed_operations: Vec<String>,
    pub mutation_posture: WorkerMutationPosture,
    pub github_posture: WorkerGitHubPosture,
    #[serde(default)]
    pub unresolved_findings: Vec<WorkerFindingSummary>,
    pub continuity_summary: Option<String>,
    #[serde(default)]
    pub memory_cards: Vec<WorkerMemoryCard>,
    #[serde(default)]
    pub artifact_refs: Vec<WorkerArtifactRef>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerCapabilityProfile {
    pub transport_kind: WorkerTransportKind,
    pub supports_context_reads: bool,
    pub supports_memory_search: bool,
    pub supports_finding_reads: bool,
    pub supports_artifact_reads: bool,
    pub supports_stage_result_submission: bool,
    pub supports_clarification_requests: bool,
    pub supports_follow_up_hints: bool,
    pub supports_fix_mode: bool,
}

pub const WORKER_OPERATION_REQUEST_SCHEMA_V1: &str = "worker_operation_request.v1";
pub const WORKER_OPERATION_RESPONSE_SCHEMA_V1: &str = "worker_operation_response.v1";
pub const AGENT_TRANSPORT_REQUEST_SCHEMA_V1: &str = "rr.agent.request.v1";
pub const AGENT_TRANSPORT_RESPONSE_SCHEMA_V1: &str = "rr.agent.response.v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerOperationLane {
    Read,
    Proposal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkerOperation {
    #[serde(rename = "worker.get_review_context")]
    GetReviewContext,
    #[serde(rename = "worker.search_memory")]
    SearchMemory,
    #[serde(rename = "worker.list_findings")]
    ListFindings,
    #[serde(rename = "worker.get_finding_detail")]
    GetFindingDetail,
    #[serde(rename = "worker.get_artifact_excerpt")]
    GetArtifactExcerpt,
    #[serde(rename = "worker.get_status")]
    GetStatus,
    #[serde(rename = "worker.submit_stage_result")]
    SubmitStageResult,
    #[serde(rename = "worker.request_clarification")]
    RequestClarification,
    #[serde(rename = "worker.request_memory_review")]
    RequestMemoryReview,
    #[serde(rename = "worker.propose_follow_up")]
    ProposeFollowUp,
}

impl WorkerOperation {
    pub fn parse(raw: &str) -> ReviewWorkerContractResult<Self> {
        match raw {
            "worker.get_review_context" => Ok(Self::GetReviewContext),
            "worker.search_memory" => Ok(Self::SearchMemory),
            "worker.list_findings" => Ok(Self::ListFindings),
            "worker.get_finding_detail" => Ok(Self::GetFindingDetail),
            "worker.get_artifact_excerpt" => Ok(Self::GetArtifactExcerpt),
            "worker.get_status" => Ok(Self::GetStatus),
            "worker.submit_stage_result" => Ok(Self::SubmitStageResult),
            "worker.request_clarification" => Ok(Self::RequestClarification),
            "worker.request_memory_review" => Ok(Self::RequestMemoryReview),
            "worker.propose_follow_up" => Ok(Self::ProposeFollowUp),
            other => Err(ReviewWorkerContractError::UnsupportedOperation {
                operation: other.to_owned(),
            }),
        }
    }

    pub fn logical_name(self) -> &'static str {
        match self {
            Self::GetReviewContext => "worker.get_review_context",
            Self::SearchMemory => "worker.search_memory",
            Self::ListFindings => "worker.list_findings",
            Self::GetFindingDetail => "worker.get_finding_detail",
            Self::GetArtifactExcerpt => "worker.get_artifact_excerpt",
            Self::GetStatus => "worker.get_status",
            Self::SubmitStageResult => "worker.submit_stage_result",
            Self::RequestClarification => "worker.request_clarification",
            Self::RequestMemoryReview => "worker.request_memory_review",
            Self::ProposeFollowUp => "worker.propose_follow_up",
        }
    }

    pub fn lane(self) -> WorkerOperationLane {
        match self {
            Self::GetReviewContext
            | Self::SearchMemory
            | Self::ListFindings
            | Self::GetFindingDetail
            | Self::GetArtifactExcerpt
            | Self::GetStatus => WorkerOperationLane::Read,
            Self::SubmitStageResult
            | Self::RequestClarification
            | Self::RequestMemoryReview
            | Self::ProposeFollowUp => WorkerOperationLane::Proposal,
        }
    }

    pub fn is_advisory(self) -> bool {
        matches!(self.lane(), WorkerOperationLane::Proposal)
    }

    pub fn required_capability(self) -> &'static str {
        match self {
            Self::GetReviewContext | Self::GetStatus => "supports_context_reads",
            Self::SearchMemory => "supports_memory_search",
            Self::ListFindings | Self::GetFindingDetail => "supports_finding_reads",
            Self::GetArtifactExcerpt => "supports_artifact_reads",
            Self::SubmitStageResult => "supports_stage_result_submission",
            Self::RequestClarification => "supports_clarification_requests",
            Self::RequestMemoryReview | Self::ProposeFollowUp => "supports_follow_up_hints",
        }
    }

    pub fn is_supported_by(self, profile: &WorkerCapabilityProfile) -> bool {
        match self {
            Self::GetReviewContext | Self::GetStatus => profile.supports_context_reads,
            Self::SearchMemory => profile.supports_memory_search,
            Self::ListFindings | Self::GetFindingDetail => profile.supports_finding_reads,
            Self::GetArtifactExcerpt => profile.supports_artifact_reads,
            Self::SubmitStageResult => profile.supports_stage_result_submission,
            Self::RequestClarification => profile.supports_clarification_requests,
            Self::RequestMemoryReview | Self::ProposeFollowUp => profile.supports_follow_up_hints,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkerOperationRequestEnvelope {
    pub schema_id: String,
    pub review_session_id: String,
    pub review_run_id: String,
    pub review_task_id: String,
    pub task_nonce: String,
    pub operation: String,
    #[serde(default)]
    pub requested_scopes: Vec<String>,
    #[serde(default)]
    pub payload: Option<Value>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerOperationAuthorization {
    pub operation: WorkerOperation,
    pub lane: WorkerOperationLane,
    #[serde(default)]
    pub granted_scopes: Vec<String>,
    pub advisory_only: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerOperationResponseStatus {
    Succeeded,
    Denied,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerOperationDenialCode {
    UnsupportedOperation,
    OperationNotAllowed,
    ScopeDenied,
    CapabilityDenied,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerOperationDenial {
    pub code: WorkerOperationDenialCode,
    pub message: String,
    #[serde(default)]
    pub denied_scopes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkerOperationResponseEnvelope {
    pub schema_id: String,
    pub review_session_id: String,
    pub review_run_id: String,
    pub review_task_id: String,
    pub task_nonce: String,
    pub operation: String,
    pub status: WorkerOperationResponseStatus,
    #[serde(default)]
    pub authorization: Option<WorkerOperationAuthorization>,
    #[serde(default)]
    pub denial: Option<WorkerOperationDenial>,
    #[serde(default)]
    pub payload: Option<Value>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

impl WorkerOperationResponseEnvelope {
    pub fn success(
        request: &WorkerOperationRequestEnvelope,
        authorization: WorkerOperationAuthorization,
        payload: Option<Value>,
    ) -> Self {
        Self {
            schema_id: WORKER_OPERATION_RESPONSE_SCHEMA_V1.to_owned(),
            review_session_id: request.review_session_id.clone(),
            review_run_id: request.review_run_id.clone(),
            review_task_id: request.review_task_id.clone(),
            task_nonce: request.task_nonce.clone(),
            operation: request.operation.clone(),
            status: WorkerOperationResponseStatus::Succeeded,
            authorization: Some(authorization),
            denial: None,
            payload,
            warnings: Vec::new(),
        }
    }

    pub fn denied(
        request: &WorkerOperationRequestEnvelope,
        denial: WorkerOperationDenial,
        warnings: Vec<String>,
    ) -> Self {
        Self {
            schema_id: WORKER_OPERATION_RESPONSE_SCHEMA_V1.to_owned(),
            review_session_id: request.review_session_id.clone(),
            review_run_id: request.review_run_id.clone(),
            review_task_id: request.review_task_id.clone(),
            task_nonce: request.task_nonce.clone(),
            operation: request.operation.clone(),
            status: WorkerOperationResponseStatus::Denied,
            authorization: None,
            denial: Some(denial),
            payload: None,
            warnings,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerGatewaySnapshot {
    #[serde(default)]
    pub status: Option<WorkerStatusSnapshot>,
    #[serde(default)]
    pub search_memory_response: Option<WorkerSearchMemoryResponse>,
    #[serde(default)]
    pub findings: Option<WorkerFindingListResponse>,
    #[serde(default)]
    pub finding_details: Vec<WorkerFindingDetail>,
    #[serde(default)]
    pub artifact_excerpts: Vec<WorkerArtifactExcerpt>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentTransportRequestEnvelope {
    pub schema_id: String,
    pub review_task: ReviewTask,
    pub worker_context: WorkerContextPacket,
    pub capability_profile: WorkerCapabilityProfile,
    pub operation_request: WorkerOperationRequestEnvelope,
    #[serde(default)]
    pub gateway_snapshot: WorkerGatewaySnapshot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentTransportResponseStatus {
    Succeeded,
    Denied,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentTransportErrorCode {
    InvalidRequestSchema,
    ValidationFailed,
    TransportKindMismatch,
    PayloadMissing,
    PayloadInvalid,
    GatewayDataMissing,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentTransportError {
    pub code: AgentTransportErrorCode,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentTransportResponseEnvelope {
    pub schema_id: String,
    pub transport_kind: WorkerTransportKind,
    pub status: AgentTransportResponseStatus,
    #[serde(default)]
    pub operation_response: Option<WorkerOperationResponseEnvelope>,
    #[serde(default)]
    pub error: Option<AgentTransportError>,
}

impl AgentTransportResponseEnvelope {
    pub fn success(operation_response: WorkerOperationResponseEnvelope) -> Self {
        Self {
            schema_id: AGENT_TRANSPORT_RESPONSE_SCHEMA_V1.to_owned(),
            transport_kind: WorkerTransportKind::AgentCli,
            status: AgentTransportResponseStatus::Succeeded,
            operation_response: Some(operation_response),
            error: None,
        }
    }

    pub fn denied(operation_response: WorkerOperationResponseEnvelope) -> Self {
        Self {
            schema_id: AGENT_TRANSPORT_RESPONSE_SCHEMA_V1.to_owned(),
            transport_kind: WorkerTransportKind::AgentCli,
            status: AgentTransportResponseStatus::Denied,
            operation_response: Some(operation_response),
            error: None,
        }
    }

    pub fn error(code: AgentTransportErrorCode, message: impl Into<String>) -> Self {
        Self {
            schema_id: AGENT_TRANSPORT_RESPONSE_SCHEMA_V1.to_owned(),
            transport_kind: WorkerTransportKind::AgentCli,
            status: AgentTransportResponseStatus::Error,
            operation_response: None,
            error: Some(AgentTransportError {
                code,
                message: message.into(),
            }),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerFindingDetailRequest {
    pub finding_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerArtifactExcerptRequest {
    pub artifact_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerStageResultAcceptance {
    pub review_session_id: String,
    pub review_run_id: String,
    pub review_task_id: String,
    pub task_nonce: String,
    pub result_schema_id: String,
    pub outcome: WorkerStageOutcome,
    pub structured_findings_pack_present: bool,
    pub clarification_request_count: usize,
    pub memory_review_request_count: usize,
    pub follow_up_proposal_count: usize,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerSearchMemoryRequest {
    pub query_text: String,
    #[serde(default = "default_search_query_mode_ingress")]
    pub query_mode: String,
    #[serde(default)]
    pub requested_retrieval_classes: Vec<String>,
    #[serde(default)]
    pub anchor_hints: Vec<String>,
}

fn default_search_query_mode_ingress() -> String {
    SearchQueryMode::Auto.as_str().to_owned()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchQueryMode {
    Auto,
    ExactLookup,
    Recall,
    RelatedContext,
    CandidateAudit,
    PromotionReview,
}

impl SearchQueryMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::ExactLookup => "exact_lookup",
            Self::Recall => "recall",
            Self::RelatedContext => "related_context",
            Self::CandidateAudit => "candidate_audit",
            Self::PromotionReview => "promotion_review",
        }
    }

    pub fn parse(raw: &str) -> SearchQueryPlanResult<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "" | "auto" => Ok(Self::Auto),
            "exact_lookup" => Ok(Self::ExactLookup),
            "recall" => Ok(Self::Recall),
            "related_context" => Ok(Self::RelatedContext),
            "candidate_audit" => Ok(Self::CandidateAudit),
            "promotion_review" => Ok(Self::PromotionReview),
            other => Err(SearchQueryPlanError::UnsupportedQueryMode {
                query_mode: other.to_owned(),
            }),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchQueryPlanningInput<'a> {
    pub query_text: &'a str,
    pub query_mode: Option<&'a str>,
    pub anchor_hints: &'a [String],
    pub supports_candidate_audit: bool,
    pub supports_promotion_review: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchScopeSet {
    CurrentRepository,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchSessionBaseline {
    AmbientSessionOptional,
    AnchorScopedContext,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchAnchorSet {
    None,
    ExplicitHints,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchTrustFloor {
    PromotedAndEvidenceOnly,
    CandidateInspectionAllowed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchCandidateVisibility {
    Hidden,
    CandidateAuditOnly,
    PromotionReviewOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchSemanticPosture {
    LexicalOnly,
    DegradedSemanticVisible,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchRetrievalLane {
    ExactLookup,
    LexicalRecall,
    RelatedContext,
    CandidateAudit,
    PromotionReview,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchStrategySelection {
    pub primary_lane: SearchRetrievalLane,
    pub lexical: bool,
    pub prior_review: bool,
    pub semantic: bool,
    pub candidate_audit: bool,
    pub query_expansion: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchRetrievalClass {
    PromotedMemory,
    TentativeCandidates,
    EvidenceHits,
}

impl SearchRetrievalClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PromotedMemory => "promoted_memory",
            Self::TentativeCandidates => "tentative_candidates",
            Self::EvidenceHits => "evidence_hits",
        }
    }

    pub fn parse(raw: &str) -> SearchPlanResult<Self> {
        match raw.trim() {
            "promoted_memory" => Ok(Self::PromotedMemory),
            "tentative_candidates" => Ok(Self::TentativeCandidates),
            "evidence_hits" => Ok(Self::EvidenceHits),
            other => Err(SearchPlanError::UnsupportedRetrievalClass {
                retrieval_class: other.to_owned(),
            }),
        }
    }

    fn sort_key(self) -> u8 {
        match self {
            Self::PromotedMemory => 0,
            Self::TentativeCandidates => 1,
            Self::EvidenceHits => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchSemanticRuntimePosture {
    DisabledByQueryMode,
    DisabledPendingVerification,
    EnabledVerifiedLocalAssets,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchQueryPlan {
    pub requested_query_mode: SearchQueryMode,
    pub resolved_query_mode: SearchQueryMode,
    pub scope_set: SearchScopeSet,
    pub session_baseline: SearchSessionBaseline,
    pub anchor_set: SearchAnchorSet,
    pub trust_floor: SearchTrustFloor,
    pub candidate_visibility: SearchCandidateVisibility,
    pub semantic_posture: SearchSemanticPosture,
    pub strategy: SearchStrategySelection,
}

impl SearchQueryPlan {
    pub fn includes_tentative_candidates(self) -> bool {
        self.candidate_visibility != SearchCandidateVisibility::Hidden
    }

    pub fn strategy_reason(self) -> &'static str {
        match self.strategy.primary_lane {
            SearchRetrievalLane::ExactLookup => {
                "exact lookup stays lexical-only so locator-style queries do not widen into recall lanes"
            }
            SearchRetrievalLane::LexicalRecall => {
                "free-text recall combines lexical and prior-review lanes while keeping candidate visibility hidden"
            }
            SearchRetrievalLane::RelatedContext => {
                "related-context planning keeps anchor-scoped retrieval explicit instead of widening to ordinary recall"
            }
            SearchRetrievalLane::CandidateAudit => {
                "candidate audit deliberately exposes tentative candidates without widening ordinary recall or promotion review"
            }
            SearchRetrievalLane::PromotionReview => {
                "promotion review stays explicit so promotion work does not masquerade as ordinary recall"
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchPlanInput<'a> {
    pub review_session_id: Option<&'a str>,
    pub review_run_id: Option<&'a str>,
    pub repository: &'a str,
    pub granted_scopes: &'a [String],
    pub query_text: &'a str,
    pub query_mode: Option<&'a str>,
    pub requested_retrieval_classes: &'a [String],
    pub anchor_hints: &'a [String],
    pub supports_candidate_audit: bool,
    pub supports_promotion_review: bool,
    pub semantic_assets_verified: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchPlan {
    pub query_plan: SearchQueryPlan,
    pub review_session_id: Option<String>,
    pub review_run_id: Option<String>,
    #[serde(default)]
    pub granted_scopes: Vec<String>,
    #[serde(default)]
    pub scope_keys: Vec<String>,
    #[serde(default)]
    pub retrieval_classes: Vec<SearchRetrievalClass>,
    pub semantic_runtime_posture: SearchSemanticRuntimePosture,
    pub retrieval_strategy: SearchStrategySelection,
    pub strategy_reason: String,
}

impl SearchPlan {
    pub fn includes_tentative_candidates(&self) -> bool {
        self.allows_retrieval_class(SearchRetrievalClass::TentativeCandidates)
    }

    pub fn allows_retrieval_class(&self, class: SearchRetrievalClass) -> bool {
        self.retrieval_classes.contains(&class)
    }
}

pub type SearchQueryPlanResult<T> = std::result::Result<T, SearchQueryPlanError>;
pub type SearchPlanResult<T> = std::result::Result<T, SearchPlanError>;

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SearchQueryPlanError {
    #[error(
        "search query input must include non-empty query text or anchor hints before Roger can plan intent"
    )]
    MissingSearchInputs,
    #[error(
        "search query mode '{query_mode}' is not part of Roger's canonical search intent contract"
    )]
    UnsupportedQueryMode { query_mode: String },
    #[error("related_context requires anchor hints or current-object context")]
    RelatedContextRequiresAnchors,
    #[error("candidate_audit is not supported on this search surface")]
    CandidateAuditUnsupported,
    #[error("promotion_review is not supported on this search surface")]
    PromotionReviewUnsupported,
}

impl SearchQueryPlanError {
    pub fn reason_code(&self) -> &'static str {
        match self {
            Self::MissingSearchInputs => "search_inputs_missing",
            Self::UnsupportedQueryMode { .. } => "query_mode_invalid",
            Self::RelatedContextRequiresAnchors => "query_mode_requires_anchor_hints",
            Self::CandidateAuditUnsupported => "query_mode_not_supported",
            Self::PromotionReviewUnsupported => "query_mode_not_supported",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SearchPlanError {
    #[error(transparent)]
    QueryPlanning(#[from] SearchQueryPlanError),
    #[error("search plan requires at least one granted scope before Roger can execute retrieval")]
    MissingGrantedScopes,
    #[error("search scope '{scope}' is not supported on this surface")]
    UnsupportedScope { scope: String },
    #[error(
        "retrieval class '{retrieval_class}' is not part of Roger's canonical retrieval contract"
    )]
    UnsupportedRetrievalClass { retrieval_class: String },
    #[error(
        "query_mode '{query_mode}' requires tentative_candidates retrieval so Roger does not silently hide candidate review"
    )]
    CandidateAwareQueryRequiresTentativeCandidates { query_mode: String },
    #[error(
        "tentative_candidates retrieval requires candidate-aware query_mode, but Roger resolved '{query_mode}'"
    )]
    TentativeCandidatesRequireCandidateAwareQuery { query_mode: String },
}

impl SearchPlanError {
    pub fn reason_code(&self) -> &'static str {
        match self {
            Self::QueryPlanning(err) => err.reason_code(),
            Self::MissingGrantedScopes => "search_scope_missing",
            Self::UnsupportedScope { .. } => "search_scope_unsupported",
            Self::UnsupportedRetrievalClass { .. } => "retrieval_class_invalid",
            Self::CandidateAwareQueryRequiresTentativeCandidates { .. } => {
                "candidate_visibility_hidden"
            }
            Self::TentativeCandidatesRequireCandidateAwareQuery { .. } => {
                "candidate_visibility_invalid"
            }
        }
    }
}

pub fn plan_search_query(
    input: SearchQueryPlanningInput<'_>,
) -> SearchQueryPlanResult<SearchQueryPlan> {
    let trimmed_query = input.query_text.trim();
    let requested_query_mode = input
        .query_mode
        .map(SearchQueryMode::parse)
        .transpose()?
        .unwrap_or(SearchQueryMode::Auto);

    if trimmed_query.is_empty() && input.anchor_hints.is_empty() {
        return Err(SearchQueryPlanError::MissingSearchInputs);
    }

    let resolved_query_mode = match requested_query_mode {
        SearchQueryMode::Auto => {
            if !input.anchor_hints.is_empty() {
                SearchQueryMode::RelatedContext
            } else if query_looks_like_exact_lookup(trimmed_query) {
                SearchQueryMode::ExactLookup
            } else {
                SearchQueryMode::Recall
            }
        }
        SearchQueryMode::ExactLookup | SearchQueryMode::Recall => requested_query_mode,
        SearchQueryMode::RelatedContext => {
            if input.anchor_hints.is_empty() {
                return Err(SearchQueryPlanError::RelatedContextRequiresAnchors);
            }
            SearchQueryMode::RelatedContext
        }
        SearchQueryMode::CandidateAudit => {
            if !input.supports_candidate_audit {
                return Err(SearchQueryPlanError::CandidateAuditUnsupported);
            }
            SearchQueryMode::CandidateAudit
        }
        SearchQueryMode::PromotionReview => {
            if !input.supports_promotion_review {
                return Err(SearchQueryPlanError::PromotionReviewUnsupported);
            }
            SearchQueryMode::PromotionReview
        }
    };

    Ok(SearchQueryPlan {
        requested_query_mode,
        resolved_query_mode,
        scope_set: SearchScopeSet::CurrentRepository,
        session_baseline: if input.anchor_hints.is_empty() {
            SearchSessionBaseline::AmbientSessionOptional
        } else {
            SearchSessionBaseline::AnchorScopedContext
        },
        anchor_set: if input.anchor_hints.is_empty() {
            SearchAnchorSet::None
        } else {
            SearchAnchorSet::ExplicitHints
        },
        trust_floor: match resolved_query_mode {
            SearchQueryMode::CandidateAudit | SearchQueryMode::PromotionReview => {
                SearchTrustFloor::CandidateInspectionAllowed
            }
            SearchQueryMode::Auto
            | SearchQueryMode::ExactLookup
            | SearchQueryMode::Recall
            | SearchQueryMode::RelatedContext => SearchTrustFloor::PromotedAndEvidenceOnly,
        },
        candidate_visibility: match resolved_query_mode {
            SearchQueryMode::CandidateAudit => SearchCandidateVisibility::CandidateAuditOnly,
            SearchQueryMode::PromotionReview => SearchCandidateVisibility::PromotionReviewOnly,
            SearchQueryMode::Auto
            | SearchQueryMode::ExactLookup
            | SearchQueryMode::Recall
            | SearchQueryMode::RelatedContext => SearchCandidateVisibility::Hidden,
        },
        semantic_posture: match resolved_query_mode {
            SearchQueryMode::Recall
            | SearchQueryMode::RelatedContext
            | SearchQueryMode::PromotionReview => SearchSemanticPosture::DegradedSemanticVisible,
            SearchQueryMode::Auto
            | SearchQueryMode::ExactLookup
            | SearchQueryMode::CandidateAudit => SearchSemanticPosture::LexicalOnly,
        },
        strategy: match resolved_query_mode {
            SearchQueryMode::Auto => unreachable!("auto resolves before strategy selection"),
            SearchQueryMode::ExactLookup => SearchStrategySelection {
                primary_lane: SearchRetrievalLane::ExactLookup,
                lexical: true,
                prior_review: false,
                semantic: false,
                candidate_audit: false,
                query_expansion: false,
            },
            SearchQueryMode::Recall => SearchStrategySelection {
                primary_lane: SearchRetrievalLane::LexicalRecall,
                lexical: true,
                prior_review: true,
                semantic: true,
                candidate_audit: false,
                query_expansion: false,
            },
            SearchQueryMode::RelatedContext => SearchStrategySelection {
                primary_lane: SearchRetrievalLane::RelatedContext,
                lexical: true,
                prior_review: true,
                semantic: true,
                candidate_audit: false,
                query_expansion: false,
            },
            SearchQueryMode::CandidateAudit => SearchStrategySelection {
                primary_lane: SearchRetrievalLane::CandidateAudit,
                lexical: true,
                prior_review: true,
                semantic: false,
                candidate_audit: true,
                query_expansion: false,
            },
            SearchQueryMode::PromotionReview => SearchStrategySelection {
                primary_lane: SearchRetrievalLane::PromotionReview,
                lexical: true,
                prior_review: true,
                semantic: true,
                candidate_audit: false,
                query_expansion: false,
            },
        },
    })
}

pub fn materialize_search_plan(input: SearchPlanInput<'_>) -> SearchPlanResult<SearchPlan> {
    let query_plan = plan_search_query(SearchQueryPlanningInput {
        query_text: input.query_text,
        query_mode: input.query_mode,
        anchor_hints: input.anchor_hints,
        supports_candidate_audit: input.supports_candidate_audit,
        supports_promotion_review: input.supports_promotion_review,
    })?;

    let granted_scopes = canonicalize_granted_scopes(input.granted_scopes);
    if granted_scopes.is_empty() {
        return Err(SearchPlanError::MissingGrantedScopes);
    }
    if let Some(scope) = granted_scopes.iter().find(|scope| scope.as_str() != "repo") {
        return Err(SearchPlanError::UnsupportedScope {
            scope: scope.clone(),
        });
    }

    let retrieval_classes =
        resolve_search_retrieval_classes(&query_plan, input.requested_retrieval_classes)?;
    let semantic_runtime_posture = if !query_plan.strategy.semantic {
        SearchSemanticRuntimePosture::DisabledByQueryMode
    } else if input.semantic_assets_verified {
        SearchSemanticRuntimePosture::EnabledVerifiedLocalAssets
    } else {
        SearchSemanticRuntimePosture::DisabledPendingVerification
    };
    let retrieval_strategy = SearchStrategySelection {
        primary_lane: query_plan.strategy.primary_lane,
        lexical: query_plan.strategy.lexical,
        prior_review: query_plan.strategy.prior_review,
        semantic: query_plan.strategy.semantic && input.semantic_assets_verified,
        candidate_audit: query_plan.strategy.candidate_audit
            && retrieval_classes.contains(&SearchRetrievalClass::TentativeCandidates),
        query_expansion: query_plan.strategy.query_expansion,
    };

    let scope_keys = granted_scopes
        .iter()
        .map(|scope| match scope.as_str() {
            "repo" => Ok(format!("repo:{}", input.repository)),
            other => Err(SearchPlanError::UnsupportedScope {
                scope: other.to_owned(),
            }),
        })
        .collect::<SearchPlanResult<Vec<_>>>()?;
    let scope_summary = scope_keys.join(", ");
    let retrieval_class_summary = retrieval_classes
        .iter()
        .map(|class| class.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let candidate_summary = if retrieval_strategy.candidate_audit {
        "tentative candidate review remains explicitly enabled"
    } else {
        "tentative candidate review remains hidden"
    };
    let semantic_summary = match semantic_runtime_posture {
        SearchSemanticRuntimePosture::DisabledByQueryMode => {
            "semantic retrieval is intentionally disabled for this query mode"
        }
        SearchSemanticRuntimePosture::DisabledPendingVerification => {
            "semantic retrieval is disabled until verified local semantic assets are available"
        }
        SearchSemanticRuntimePosture::EnabledVerifiedLocalAssets => {
            "semantic retrieval is enabled with verified local assets"
        }
    };
    let strategy_reason = format!(
        "{} Scope stays bound to {scope_summary}; retrieval classes: {retrieval_class_summary}; {candidate_summary}; {semantic_summary}.",
        query_plan.strategy_reason()
    );

    Ok(SearchPlan {
        query_plan,
        review_session_id: input.review_session_id.map(str::to_owned),
        review_run_id: input.review_run_id.map(str::to_owned),
        granted_scopes,
        scope_keys,
        retrieval_classes,
        semantic_runtime_posture,
        retrieval_strategy,
        strategy_reason,
    })
}

fn canonicalize_granted_scopes(scopes: &[String]) -> Vec<String> {
    let mut canonical = Vec::new();
    for scope in scopes {
        let trimmed = scope.trim();
        if trimmed.is_empty() || canonical.iter().any(|existing| existing == trimmed) {
            continue;
        }
        canonical.push(trimmed.to_owned());
    }
    canonical.sort_unstable();
    canonical
}

fn resolve_search_retrieval_classes(
    query_plan: &SearchQueryPlan,
    requested_retrieval_classes: &[String],
) -> SearchPlanResult<Vec<SearchRetrievalClass>> {
    let mut retrieval_classes = if requested_retrieval_classes.is_empty() {
        let mut defaults = vec![
            SearchRetrievalClass::PromotedMemory,
            SearchRetrievalClass::EvidenceHits,
        ];
        if query_plan.includes_tentative_candidates() {
            defaults.push(SearchRetrievalClass::TentativeCandidates);
        }
        defaults
    } else {
        let mut explicit = Vec::new();
        for class in requested_retrieval_classes {
            let class = SearchRetrievalClass::parse(class)?;
            if !explicit.contains(&class) {
                explicit.push(class);
            }
        }
        explicit
    };

    let tentative_candidates_requested =
        retrieval_classes.contains(&SearchRetrievalClass::TentativeCandidates);
    if query_plan.includes_tentative_candidates() && !tentative_candidates_requested {
        return Err(
            SearchPlanError::CandidateAwareQueryRequiresTentativeCandidates {
                query_mode: query_plan.resolved_query_mode.as_str().to_owned(),
            },
        );
    }
    if tentative_candidates_requested && !query_plan.includes_tentative_candidates() {
        return Err(
            SearchPlanError::TentativeCandidatesRequireCandidateAwareQuery {
                query_mode: query_plan.resolved_query_mode.as_str().to_owned(),
            },
        );
    }

    retrieval_classes.sort_by_key(|class| class.sort_key());
    Ok(retrieval_classes)
}

fn query_looks_like_exact_lookup(query: &str) -> bool {
    if query.is_empty() || query.split_whitespace().count() != 1 {
        return false;
    }

    let lowered = query.to_ascii_lowercase();
    let exact_prefix_match = [
        "fp:",
        "finding-",
        "mem-",
        "session-",
        "run-",
        "artifact-",
        "prompt-",
        "rr-",
    ]
    .iter()
    .any(|prefix| lowered.starts_with(prefix));

    exact_prefix_match
        || query.contains("::")
        || query.contains('/')
        || query.contains('#')
        || query_has_known_file_extension(&lowered)
}

fn query_has_known_file_extension(query: &str) -> bool {
    let Some((_, extension)) = query.rsplit_once('.') else {
        return false;
    };

    matches!(
        extension,
        "rs" | "md"
            | "txt"
            | "json"
            | "toml"
            | "yaml"
            | "yml"
            | "lock"
            | "sql"
            | "js"
            | "ts"
            | "tsx"
            | "jsx"
            | "go"
            | "py"
            | "java"
            | "kt"
            | "swift"
    )
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecallSourceRef {
    pub kind: String,
    pub id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecallEnvelope {
    pub item_kind: String,
    pub item_id: String,
    pub requested_query_mode: String,
    pub resolved_query_mode: String,
    pub retrieval_mode: String,
    pub scope_bucket: String,
    pub memory_lane: String,
    pub trust_state: Option<String>,
    pub source_refs: Vec<RecallSourceRef>,
    pub locator: Value,
    pub snippet_or_summary: String,
    pub anchor_overlap_summary: String,
    pub degraded_flags: Vec<String>,
    pub explain_summary: String,
    pub citation_posture: String,
    pub surface_posture: String,
}

pub type WorkerRecallEnvelope = RecallEnvelope;
pub type WorkerSearchPlan = SearchPlan;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerSearchMemoryResponse {
    pub requested_query_mode: String,
    pub resolved_query_mode: String,
    pub search_plan: WorkerSearchPlan,
    pub retrieval_mode: String,
    #[serde(default)]
    pub degraded_flags: Vec<String>,
    #[serde(default)]
    pub promoted_memory: Vec<WorkerRecallEnvelope>,
    #[serde(default)]
    pub tentative_candidates: Vec<WorkerRecallEnvelope>,
    #[serde(default)]
    pub evidence_hits: Vec<WorkerRecallEnvelope>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerFindingListResponse {
    #[serde(default)]
    pub items: Vec<WorkerFindingSummary>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerEvidenceLocation {
    pub artifact_id: String,
    pub repo_rel_path: Option<String>,
    pub start_line: Option<u32>,
    pub end_line: Option<u32>,
    pub evidence_role: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerFindingDetail {
    pub finding: WorkerFindingSummary,
    #[serde(default)]
    pub evidence_locations: Vec<WorkerEvidenceLocation>,
    #[serde(default)]
    pub clarification_ids: Vec<String>,
    #[serde(default)]
    pub outbound_draft_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerArtifactExcerpt {
    pub artifact_id: String,
    pub excerpt: String,
    pub digest: Option<String>,
    pub truncated: bool,
    pub byte_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerStatusSnapshot {
    pub review_session_id: String,
    pub review_run_id: String,
    pub attention_state: String,
    pub continuity_summary: Option<String>,
    #[serde(default)]
    pub degraded_flags: Vec<String>,
    pub unresolved_finding_count: usize,
    pub pending_clarification_count: usize,
    pub draft_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerInvocation {
    pub id: String,
    pub review_session_id: String,
    pub review_run_id: String,
    pub review_task_id: String,
    pub provider: String,
    pub provider_session_id: Option<String>,
    pub transport_kind: WorkerTransportKind,
    pub started_at: Timestamp,
    pub completed_at: Option<Timestamp>,
    pub outcome_state: WorkerInvocationOutcomeState,
    pub prompt_invocation_id: Option<String>,
    pub raw_output_artifact_id: Option<String>,
    pub result_artifact_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerToolCallEvent {
    pub id: String,
    pub review_task_id: String,
    pub worker_invocation_id: String,
    pub operation: String,
    pub request_digest: String,
    pub response_digest: Option<String>,
    pub outcome_state: WorkerToolCallOutcomeState,
    pub occurred_at: Timestamp,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerClarificationRequest {
    pub id: String,
    pub question: String,
    pub reason: Option<String>,
    pub blocking: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerMemoryReviewRequest {
    pub id: String,
    pub query: String,
    #[serde(default)]
    pub requested_scopes: Vec<String>,
    pub rationale: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerFollowUpProposal {
    pub id: String,
    pub title: String,
    pub objective: String,
    pub proposed_task_kind: ReviewTaskKind,
    #[serde(default)]
    pub suggested_scopes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerMemoryCitation {
    pub citation_id: String,
    pub source_kind: String,
    pub source_id: String,
    pub summary: String,
    pub scope: String,
    pub trust_tier: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkerStageResult {
    pub schema_id: String,
    pub review_session_id: String,
    pub review_run_id: String,
    pub review_task_id: String,
    pub worker_invocation_id: Option<String>,
    pub task_nonce: String,
    pub stage: String,
    pub task_kind: ReviewTaskKind,
    pub outcome: WorkerStageOutcome,
    pub summary: String,
    #[serde(default)]
    pub structured_findings_pack: Option<Value>,
    #[serde(default)]
    pub clarification_requests: Vec<WorkerClarificationRequest>,
    #[serde(default)]
    pub memory_review_requests: Vec<WorkerMemoryReviewRequest>,
    #[serde(default, alias = "follow_up_hints")]
    pub follow_up_proposals: Vec<WorkerFollowUpProposal>,
    #[serde(default)]
    pub memory_citations: Vec<WorkerMemoryCitation>,
    #[serde(default)]
    pub artifact_refs: Vec<WorkerArtifactRef>,
    #[serde(default)]
    pub provider_metadata: Option<Value>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

impl WorkerStageResult {
    pub fn structured_findings_pack_json(
        &self,
    ) -> std::result::Result<Option<String>, serde_json::Error> {
        self.structured_findings_pack
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
    }

    pub fn with_worker_invocation_id(mut self, worker_invocation_id: impl Into<String>) -> Self {
        self.worker_invocation_id = Some(worker_invocation_id.into());
        self
    }
}

pub type ReviewWorkerContractResult<T> = std::result::Result<T, ReviewWorkerContractError>;

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ReviewWorkerContractError {
    #[error(
        "worker context packet review_session_id '{found}' does not match review task '{expected}'"
    )]
    ContextSessionMismatch { expected: String, found: String },
    #[error(
        "worker context packet review_run_id '{found}' does not match review task '{expected}'"
    )]
    ContextRunMismatch { expected: String, found: String },
    #[error(
        "worker context packet review_task_id '{found}' does not match review task '{expected}'"
    )]
    ContextTaskMismatch { expected: String, found: String },
    #[error("worker context packet task_nonce '{found}' does not match review task '{expected}'")]
    ContextNonceMismatch { expected: String, found: String },
    #[error("worker context packet stage '{found}' does not match review task '{expected}'")]
    ContextStageMismatch { expected: String, found: String },
    #[error("worker context packet objective does not match review task objective")]
    ContextObjectiveMismatch,
    #[error("worker context packet allowed scopes do not match review task allowed scopes")]
    ContextAllowedScopesMismatch,
    #[error("worker context packet allowed operations do not match review task allowed operations")]
    ContextAllowedOperationsMismatch,
    #[error(
        "worker operation request schema_id '{found}' does not match review task expectation '{expected}'"
    )]
    RequestSchemaMismatch { expected: String, found: String },
    #[error(
        "worker operation request review_session_id '{found}' does not match review task '{expected}'"
    )]
    RequestSessionMismatch { expected: String, found: String },
    #[error(
        "worker operation request review_run_id '{found}' does not match review task '{expected}'"
    )]
    RequestRunMismatch { expected: String, found: String },
    #[error(
        "worker operation request review_task_id '{found}' does not match review task '{expected}'"
    )]
    RequestTaskMismatch { expected: String, found: String },
    #[error(
        "worker operation request task_nonce '{found}' does not match review task '{expected}'"
    )]
    RequestNonceMismatch { expected: String, found: String },
    #[error("worker operation '{operation}' is not part of the canonical Roger worker API")]
    UnsupportedOperation { operation: String },
    #[error("worker operation '{operation}' is not allowed for this review task")]
    OperationNotAllowed { operation: String },
    #[error(
        "worker operation requested scope '{requested}' outside allowed task scopes {allowed:?}"
    )]
    ScopeEscalationDenied {
        requested: String,
        allowed: Vec<String>,
    },
    #[error(
        "worker operation '{operation}' requires capability '{capability}' which the transport does not support"
    )]
    OperationCapabilityUnsupported {
        operation: String,
        capability: String,
    },
    #[error("worker capability profile does not support stage result submission")]
    StageResultSubmissionUnsupported,
    #[error(
        "worker stage result schema_id '{found}' does not match review task expectation '{expected}'"
    )]
    ResultSchemaMismatch { expected: String, found: String },
    #[error(
        "worker stage result review_session_id '{found}' does not match review task '{expected}'"
    )]
    ResultSessionMismatch { expected: String, found: String },
    #[error("worker stage result review_run_id '{found}' does not match review task '{expected}'")]
    ResultRunMismatch { expected: String, found: String },
    #[error("worker stage result review_task_id '{found}' does not match review task '{expected}'")]
    ResultTaskMismatch { expected: String, found: String },
    #[error("worker stage result task_nonce '{found}' does not match review task '{expected}'")]
    ResultNonceMismatch { expected: String, found: String },
    #[error("worker stage result stage '{found}' does not match review task '{expected}'")]
    ResultStageMismatch { expected: String, found: String },
    #[error("worker stage result task_kind '{found:?}' does not match review task '{expected:?}'")]
    ResultTaskKindMismatch {
        expected: ReviewTaskKind,
        found: ReviewTaskKind,
    },
    #[error("worker stage result summary must not be empty")]
    EmptyResultSummary,
    #[error(
        "worker stage result worker_invocation_id '{found}' does not match expected worker invocation '{expected}'"
    )]
    ResultWorkerInvocationMismatch { expected: String, found: String },
    #[error(
        "worker tool call event '{event_id}' review_task_id '{found}' does not match review task '{expected}'"
    )]
    ToolCallTaskMismatch {
        event_id: String,
        expected: String,
        found: String,
    },
    #[error(
        "worker tool call event '{event_id}' worker_invocation_id '{found}' does not match expected worker invocation '{expected}'"
    )]
    ToolCallInvocationMismatch {
        event_id: String,
        expected: String,
        found: String,
    },
    #[error("review task prompt_preset_id '{expected}' does not match resolved prompt '{found}'")]
    PromptPresetMismatch { expected: String, found: String },
}

impl ReviewTask {
    pub fn validate_context_packet(
        &self,
        packet: &WorkerContextPacket,
    ) -> ReviewWorkerContractResult<()> {
        if packet.review_session_id != self.review_session_id {
            return Err(ReviewWorkerContractError::ContextSessionMismatch {
                expected: self.review_session_id.clone(),
                found: packet.review_session_id.clone(),
            });
        }
        if packet.review_run_id != self.review_run_id {
            return Err(ReviewWorkerContractError::ContextRunMismatch {
                expected: self.review_run_id.clone(),
                found: packet.review_run_id.clone(),
            });
        }
        if packet.review_task_id != self.id {
            return Err(ReviewWorkerContractError::ContextTaskMismatch {
                expected: self.id.clone(),
                found: packet.review_task_id.clone(),
            });
        }
        if packet.task_nonce != self.task_nonce {
            return Err(ReviewWorkerContractError::ContextNonceMismatch {
                expected: self.task_nonce.clone(),
                found: packet.task_nonce.clone(),
            });
        }
        if packet.stage != self.stage {
            return Err(ReviewWorkerContractError::ContextStageMismatch {
                expected: self.stage.clone(),
                found: packet.stage.clone(),
            });
        }
        if packet.objective != self.objective {
            return Err(ReviewWorkerContractError::ContextObjectiveMismatch);
        }
        if packet.allowed_scopes != self.allowed_scopes {
            return Err(ReviewWorkerContractError::ContextAllowedScopesMismatch);
        }
        if packet.allowed_operations != self.allowed_operations {
            return Err(ReviewWorkerContractError::ContextAllowedOperationsMismatch);
        }
        Ok(())
    }

    pub fn validate_operation_request(
        &self,
        request: &WorkerOperationRequestEnvelope,
        profile: &WorkerCapabilityProfile,
    ) -> ReviewWorkerContractResult<WorkerOperationAuthorization> {
        if request.schema_id != WORKER_OPERATION_REQUEST_SCHEMA_V1 {
            return Err(ReviewWorkerContractError::RequestSchemaMismatch {
                expected: WORKER_OPERATION_REQUEST_SCHEMA_V1.to_owned(),
                found: request.schema_id.clone(),
            });
        }
        if request.review_session_id != self.review_session_id {
            return Err(ReviewWorkerContractError::RequestSessionMismatch {
                expected: self.review_session_id.clone(),
                found: request.review_session_id.clone(),
            });
        }
        if request.review_run_id != self.review_run_id {
            return Err(ReviewWorkerContractError::RequestRunMismatch {
                expected: self.review_run_id.clone(),
                found: request.review_run_id.clone(),
            });
        }
        if request.review_task_id != self.id {
            return Err(ReviewWorkerContractError::RequestTaskMismatch {
                expected: self.id.clone(),
                found: request.review_task_id.clone(),
            });
        }
        if request.task_nonce != self.task_nonce {
            return Err(ReviewWorkerContractError::RequestNonceMismatch {
                expected: self.task_nonce.clone(),
                found: request.task_nonce.clone(),
            });
        }

        let operation = WorkerOperation::parse(&request.operation)?;
        if !self
            .allowed_operations
            .iter()
            .any(|allowed| allowed == operation.logical_name())
        {
            return Err(ReviewWorkerContractError::OperationNotAllowed {
                operation: request.operation.clone(),
            });
        }
        for requested_scope in &request.requested_scopes {
            if !self
                .allowed_scopes
                .iter()
                .any(|allowed| allowed == requested_scope)
            {
                return Err(ReviewWorkerContractError::ScopeEscalationDenied {
                    requested: requested_scope.clone(),
                    allowed: self.allowed_scopes.clone(),
                });
            }
        }
        if !operation.is_supported_by(profile) {
            return Err(ReviewWorkerContractError::OperationCapabilityUnsupported {
                operation: operation.logical_name().to_owned(),
                capability: operation.required_capability().to_owned(),
            });
        }

        let granted_scopes = if request.requested_scopes.is_empty() {
            self.allowed_scopes.clone()
        } else {
            request.requested_scopes.clone()
        };

        Ok(WorkerOperationAuthorization {
            operation,
            lane: operation.lane(),
            granted_scopes,
            advisory_only: operation.is_advisory(),
        })
    }

    pub fn validate_capability_profile(
        &self,
        profile: &WorkerCapabilityProfile,
    ) -> ReviewWorkerContractResult<()> {
        if !profile.supports_stage_result_submission {
            return Err(ReviewWorkerContractError::StageResultSubmissionUnsupported);
        }
        Ok(())
    }

    pub fn validate_prompt_preset_id(
        &self,
        prompt_preset_id: &str,
    ) -> ReviewWorkerContractResult<()> {
        if let Some(expected) = self.prompt_preset_id.as_deref() {
            if expected != prompt_preset_id {
                return Err(ReviewWorkerContractError::PromptPresetMismatch {
                    expected: expected.to_owned(),
                    found: prompt_preset_id.to_owned(),
                });
            }
        }
        Ok(())
    }

    pub fn validate_stage_result(
        &self,
        result: &WorkerStageResult,
    ) -> ReviewWorkerContractResult<()> {
        if result.schema_id != self.expected_result_schema {
            return Err(ReviewWorkerContractError::ResultSchemaMismatch {
                expected: self.expected_result_schema.clone(),
                found: result.schema_id.clone(),
            });
        }
        if result.review_session_id != self.review_session_id {
            return Err(ReviewWorkerContractError::ResultSessionMismatch {
                expected: self.review_session_id.clone(),
                found: result.review_session_id.clone(),
            });
        }
        if result.review_run_id != self.review_run_id {
            return Err(ReviewWorkerContractError::ResultRunMismatch {
                expected: self.review_run_id.clone(),
                found: result.review_run_id.clone(),
            });
        }
        if result.review_task_id != self.id {
            return Err(ReviewWorkerContractError::ResultTaskMismatch {
                expected: self.id.clone(),
                found: result.review_task_id.clone(),
            });
        }
        if result.task_nonce != self.task_nonce {
            return Err(ReviewWorkerContractError::ResultNonceMismatch {
                expected: self.task_nonce.clone(),
                found: result.task_nonce.clone(),
            });
        }
        if result.stage != self.stage {
            return Err(ReviewWorkerContractError::ResultStageMismatch {
                expected: self.stage.clone(),
                found: result.stage.clone(),
            });
        }
        if result.task_kind != self.task_kind {
            return Err(ReviewWorkerContractError::ResultTaskKindMismatch {
                expected: self.task_kind,
                found: result.task_kind,
            });
        }
        if result.summary.trim().is_empty() {
            return Err(ReviewWorkerContractError::EmptyResultSummary);
        }
        Ok(())
    }

    pub fn validate_worker_invocation_binding(
        &self,
        result: &WorkerStageResult,
        expected_worker_invocation_id: &str,
    ) -> ReviewWorkerContractResult<()> {
        if let Some(found) = result.worker_invocation_id.as_deref() {
            if found != expected_worker_invocation_id {
                return Err(ReviewWorkerContractError::ResultWorkerInvocationMismatch {
                    expected: expected_worker_invocation_id.to_owned(),
                    found: found.to_owned(),
                });
            }
        }
        Ok(())
    }

    pub fn validate_tool_call_event(
        &self,
        event: &WorkerToolCallEvent,
        expected_worker_invocation_id: &str,
    ) -> ReviewWorkerContractResult<()> {
        if event.review_task_id != self.id {
            return Err(ReviewWorkerContractError::ToolCallTaskMismatch {
                event_id: event.id.clone(),
                expected: self.id.clone(),
                found: event.review_task_id.clone(),
            });
        }
        if event.worker_invocation_id != expected_worker_invocation_id {
            return Err(ReviewWorkerContractError::ToolCallInvocationMismatch {
                event_id: event.id.clone(),
                expected: expected_worker_invocation_id.to_owned(),
                found: event.worker_invocation_id.clone(),
            });
        }
        Ok(())
    }
}

pub fn execute_agent_transport_request(
    request: &AgentTransportRequestEnvelope,
) -> AgentTransportResponseEnvelope {
    if request.schema_id != AGENT_TRANSPORT_REQUEST_SCHEMA_V1 {
        return AgentTransportResponseEnvelope::error(
            AgentTransportErrorCode::InvalidRequestSchema,
            format!(
                "agent transport request schema_id '{}' does not match expected '{}'",
                request.schema_id, AGENT_TRANSPORT_REQUEST_SCHEMA_V1
            ),
        );
    }

    if request.worker_context.transport_kind != WorkerTransportKind::AgentCli {
        return AgentTransportResponseEnvelope::error(
            AgentTransportErrorCode::TransportKindMismatch,
            format!(
                "worker context transport_kind '{}' is invalid for rr agent",
                request.worker_context.transport_kind.as_str()
            ),
        );
    }

    if request.capability_profile.transport_kind != WorkerTransportKind::AgentCli {
        return AgentTransportResponseEnvelope::error(
            AgentTransportErrorCode::TransportKindMismatch,
            format!(
                "worker capability profile transport_kind '{}' is invalid for rr agent",
                request.capability_profile.transport_kind.as_str()
            ),
        );
    }

    if let Err(err) = request
        .review_task
        .validate_context_packet(&request.worker_context)
    {
        return AgentTransportResponseEnvelope::error(
            AgentTransportErrorCode::ValidationFailed,
            err.to_string(),
        );
    }

    let authorization = match request
        .review_task
        .validate_operation_request(&request.operation_request, &request.capability_profile)
    {
        Ok(authorization) => authorization,
        Err(err) => {
            if let Some(denial) = worker_operation_denial(&request.operation_request, err.clone()) {
                return AgentTransportResponseEnvelope::denied(
                    WorkerOperationResponseEnvelope::denied(
                        &request.operation_request,
                        denial,
                        Vec::new(),
                    ),
                );
            }
            return AgentTransportResponseEnvelope::error(
                AgentTransportErrorCode::ValidationFailed,
                err.to_string(),
            );
        }
    };

    let payload = match authorization.operation {
        WorkerOperation::GetReviewContext => serde_json::to_value(&request.worker_context)
            .map_err(|err| (AgentTransportErrorCode::PayloadInvalid, err.to_string())),
        WorkerOperation::GetStatus => snapshot_status_payload(request),
        WorkerOperation::SearchMemory => search_memory_payload(request),
        WorkerOperation::ListFindings => snapshot_findings_payload(request),
        WorkerOperation::GetFindingDetail => finding_detail_payload(request),
        WorkerOperation::GetArtifactExcerpt => artifact_excerpt_payload(request),
        WorkerOperation::SubmitStageResult => submit_stage_result_payload(request),
        WorkerOperation::RequestClarification => {
            echo_request_payload::<WorkerClarificationRequest>(
                request,
                "worker.request_clarification",
            )
        }
        WorkerOperation::RequestMemoryReview => echo_request_payload::<WorkerMemoryReviewRequest>(
            request,
            "worker.request_memory_review",
        ),
        WorkerOperation::ProposeFollowUp => {
            echo_request_payload::<WorkerFollowUpProposal>(request, "worker.propose_follow_up")
        }
    };

    match payload {
        Ok(payload) => {
            AgentTransportResponseEnvelope::success(WorkerOperationResponseEnvelope::success(
                &request.operation_request,
                authorization,
                Some(payload),
            ))
        }
        Err((code, message)) => AgentTransportResponseEnvelope::error(code, message),
    }
}

fn worker_operation_denial(
    request: &WorkerOperationRequestEnvelope,
    err: ReviewWorkerContractError,
) -> Option<WorkerOperationDenial> {
    match err {
        ReviewWorkerContractError::UnsupportedOperation { operation } => {
            Some(WorkerOperationDenial {
                code: WorkerOperationDenialCode::UnsupportedOperation,
                message: format!(
                    "worker operation '{operation}' is not part of the canonical rr agent API"
                ),
                denied_scopes: request.requested_scopes.clone(),
            })
        }
        ReviewWorkerContractError::OperationNotAllowed { operation } => {
            Some(WorkerOperationDenial {
                code: WorkerOperationDenialCode::OperationNotAllowed,
                message: format!(
                    "worker operation '{operation}' is outside the current ReviewTask allowance"
                ),
                denied_scopes: request.requested_scopes.clone(),
            })
        }
        ReviewWorkerContractError::ScopeEscalationDenied { requested, allowed } => {
            Some(WorkerOperationDenial {
                code: WorkerOperationDenialCode::ScopeDenied,
                message: format!(
                    "worker requested scope '{requested}' outside the allowed rr agent scopes {allowed:?}"
                ),
                denied_scopes: vec![requested],
            })
        }
        ReviewWorkerContractError::OperationCapabilityUnsupported {
            operation,
            capability,
        } => Some(WorkerOperationDenial {
            code: WorkerOperationDenialCode::CapabilityDenied,
            message: format!(
                "worker operation '{operation}' requires capability '{capability}' which this rr agent transport snapshot does not provide"
            ),
            denied_scopes: request.requested_scopes.clone(),
        }),
        _ => None,
    }
}

fn snapshot_status_payload(
    request: &AgentTransportRequestEnvelope,
) -> std::result::Result<Value, (AgentTransportErrorCode, String)> {
    let snapshot = request.gateway_snapshot.status.as_ref().ok_or_else(|| {
        (
            AgentTransportErrorCode::GatewayDataMissing,
            "rr agent get_status requires a WorkerStatusSnapshot in gateway_snapshot.status"
                .to_owned(),
        )
    })?;
    if snapshot.review_session_id != request.review_task.review_session_id
        || snapshot.review_run_id != request.review_task.review_run_id
    {
        return Err((
            AgentTransportErrorCode::ValidationFailed,
            "worker status snapshot does not match the bound ReviewTask session/run".to_owned(),
        ));
    }
    serde_json::to_value(snapshot)
        .map_err(|err| (AgentTransportErrorCode::PayloadInvalid, err.to_string()))
}

fn search_memory_payload(
    request: &AgentTransportRequestEnvelope,
) -> std::result::Result<Value, (AgentTransportErrorCode, String)> {
    let search_request =
        decode_operation_payload::<WorkerSearchMemoryRequest>(request, "worker.search_memory")?;
    let granted_scopes = if request.operation_request.requested_scopes.is_empty() {
        request.review_task.allowed_scopes.clone()
    } else {
        request.operation_request.requested_scopes.clone()
    };
    let plan = materialize_search_plan(SearchPlanInput {
        review_session_id: Some(&request.review_task.review_session_id),
        review_run_id: Some(&request.review_task.review_run_id),
        repository: &request.worker_context.review_target.repository,
        granted_scopes: &granted_scopes,
        query_text: &search_request.query_text,
        query_mode: Some(&search_request.query_mode),
        requested_retrieval_classes: &search_request.requested_retrieval_classes,
        anchor_hints: &search_request.anchor_hints,
        supports_candidate_audit: true,
        supports_promotion_review: false,
        semantic_assets_verified: false,
    })
    .map_err(|err| (AgentTransportErrorCode::ValidationFailed, err.to_string()))?;
    let response = request
        .gateway_snapshot
        .search_memory_response
        .as_ref()
        .ok_or_else(|| {
            (
                AgentTransportErrorCode::GatewayDataMissing,
                "rr agent search_memory requires a WorkerSearchMemoryResponse in gateway_snapshot.search_memory_response"
                    .to_owned(),
            )
        })?;
    if response.requested_query_mode != plan.query_plan.requested_query_mode.as_str()
        || response.resolved_query_mode != plan.query_plan.resolved_query_mode.as_str()
    {
        return Err((
            AgentTransportErrorCode::ValidationFailed,
            "worker search response query-mode resolution does not match Roger's planned search intent"
                .to_owned(),
        ));
    }
    validate_search_memory_response(response, &plan)?;
    serde_json::to_value(response)
        .map_err(|err| (AgentTransportErrorCode::PayloadInvalid, err.to_string()))
}

fn validate_search_memory_response(
    response: &WorkerSearchMemoryResponse,
    expected_plan: &SearchPlan,
) -> std::result::Result<(), (AgentTransportErrorCode, String)> {
    if response.search_plan != *expected_plan {
        return Err((
            AgentTransportErrorCode::ValidationFailed,
            "worker search response search_plan drifted from Roger's planned retrieval objective"
                .to_owned(),
        ));
    }
    if response.requested_query_mode
        != response
            .search_plan
            .query_plan
            .requested_query_mode
            .as_str()
        || response.resolved_query_mode
            != response.search_plan.query_plan.resolved_query_mode.as_str()
    {
        return Err((
            AgentTransportErrorCode::ValidationFailed,
            "worker search response top-level query-mode fields drifted from Roger's planned retrieval objective"
                .to_owned(),
        ));
    }
    if !expected_plan.allows_retrieval_class(SearchRetrievalClass::PromotedMemory)
        && !response.promoted_memory.is_empty()
    {
        return Err((
            AgentTransportErrorCode::ValidationFailed,
            "worker search response surfaced promoted_memory outside the planned retrieval classes"
                .to_owned(),
        ));
    }
    if !expected_plan.allows_retrieval_class(SearchRetrievalClass::TentativeCandidates)
        && !response.tentative_candidates.is_empty()
    {
        return Err((
            AgentTransportErrorCode::ValidationFailed,
            "worker search response surfaced tentative_candidates outside the planned retrieval classes"
                .to_owned(),
        ));
    }
    if !expected_plan.allows_retrieval_class(SearchRetrievalClass::EvidenceHits)
        && !response.evidence_hits.is_empty()
    {
        return Err((
            AgentTransportErrorCode::ValidationFailed,
            "worker search response surfaced evidence_hits outside the planned retrieval classes"
                .to_owned(),
        ));
    }
    if !expected_plan.retrieval_strategy.semantic && response.retrieval_mode == "hybrid" {
        return Err((
            AgentTransportErrorCode::ValidationFailed,
            "worker search response reported hybrid retrieval even though the search_plan disabled semantic retrieval"
                .to_owned(),
        ));
    }
    validate_recall_envelopes(
        &response.promoted_memory,
        "promoted_memory",
        &response.requested_query_mode,
        &response.resolved_query_mode,
        &response.retrieval_mode,
        &response.degraded_flags,
    )?;
    validate_recall_envelopes(
        &response.tentative_candidates,
        "tentative_candidates",
        &response.requested_query_mode,
        &response.resolved_query_mode,
        &response.retrieval_mode,
        &response.degraded_flags,
    )?;
    validate_recall_envelopes(
        &response.evidence_hits,
        "evidence_hits",
        &response.requested_query_mode,
        &response.resolved_query_mode,
        &response.retrieval_mode,
        &response.degraded_flags,
    )?;
    Ok(())
}

fn validate_recall_envelopes(
    envelopes: &[WorkerRecallEnvelope],
    expected_lane: &str,
    expected_requested_query_mode: &str,
    expected_resolved_query_mode: &str,
    expected_retrieval_mode: &str,
    expected_degraded_flags: &[String],
) -> std::result::Result<(), (AgentTransportErrorCode, String)> {
    for envelope in envelopes {
        if envelope.item_kind.trim().is_empty() {
            return Err((
                AgentTransportErrorCode::ValidationFailed,
                "worker search response recall envelope is missing item_kind".to_owned(),
            ));
        }
        if envelope.item_id.trim().is_empty() {
            return Err((
                AgentTransportErrorCode::ValidationFailed,
                "worker search response recall envelope is missing item_id".to_owned(),
            ));
        }
        if envelope.requested_query_mode != expected_requested_query_mode
            || envelope.resolved_query_mode != expected_resolved_query_mode
            || envelope.retrieval_mode != expected_retrieval_mode
        {
            return Err((
                AgentTransportErrorCode::ValidationFailed,
                "worker search response recall envelope query/retrieval truth drifted from the top-level search response".to_owned(),
            ));
        }
        if envelope.memory_lane != expected_lane {
            return Err((
                AgentTransportErrorCode::ValidationFailed,
                format!(
                    "worker search response recall envelope lane {} does not match expected lane {}",
                    envelope.memory_lane, expected_lane
                ),
            ));
        }
        if envelope.scope_bucket.trim().is_empty()
            || envelope.snippet_or_summary.trim().is_empty()
            || envelope.anchor_overlap_summary.trim().is_empty()
            || envelope.explain_summary.trim().is_empty()
        {
            return Err((
                AgentTransportErrorCode::ValidationFailed,
                "worker search response recall envelope omitted required truth fields".to_owned(),
            ));
        }
        if envelope.degraded_flags != expected_degraded_flags {
            return Err((
                AgentTransportErrorCode::ValidationFailed,
                "worker search response recall envelope degraded_flags drifted from the top-level search response".to_owned(),
            ));
        }
        if envelope.source_refs.is_empty()
            || envelope.source_refs.iter().any(|source_ref| {
                source_ref.kind.trim().is_empty() || source_ref.id.trim().is_empty()
            })
        {
            return Err((
                AgentTransportErrorCode::ValidationFailed,
                "worker search response recall envelope must include non-empty source_refs"
                    .to_owned(),
            ));
        }
        if !envelope.locator.is_object() {
            return Err((
                AgentTransportErrorCode::ValidationFailed,
                "worker search response recall envelope locator must be an object".to_owned(),
            ));
        }
        match envelope.citation_posture.as_str() {
            "cite_allowed" | "inspect_only" | "warning_only" => {}
            other => {
                return Err((
                    AgentTransportErrorCode::ValidationFailed,
                    format!(
                        "worker search response recall envelope has unsupported citation_posture {other}"
                    ),
                ));
            }
        }
        match envelope.surface_posture.as_str() {
            "ordinary" | "candidate_review" | "operator_review_only" => {}
            other => {
                return Err((
                    AgentTransportErrorCode::ValidationFailed,
                    format!(
                        "worker search response recall envelope has unsupported surface_posture {other}"
                    ),
                ));
            }
        }
        if expected_lane == "tentative_candidates" && envelope.citation_posture == "cite_allowed" {
            return Err((
                AgentTransportErrorCode::ValidationFailed,
                "tentative candidate memory cannot surface as cite_allowed".to_owned(),
            ));
        }
        if matches!(
            envelope.trust_state.as_deref(),
            Some("contradicted" | "anti_pattern")
        ) && envelope.citation_posture != "warning_only"
        {
            return Err((
                AgentTransportErrorCode::ValidationFailed,
                "contradicted or anti_pattern memory must surface as warning_only".to_owned(),
            ));
        }
    }

    Ok(())
}

fn snapshot_findings_payload(
    request: &AgentTransportRequestEnvelope,
) -> std::result::Result<Value, (AgentTransportErrorCode, String)> {
    let response = request
        .gateway_snapshot
        .findings
        .as_ref()
        .ok_or_else(|| {
            (
                AgentTransportErrorCode::GatewayDataMissing,
                "rr agent list_findings requires a WorkerFindingListResponse in gateway_snapshot.findings"
                    .to_owned(),
            )
        })?;
    serde_json::to_value(response)
        .map_err(|err| (AgentTransportErrorCode::PayloadInvalid, err.to_string()))
}

fn finding_detail_payload(
    request: &AgentTransportRequestEnvelope,
) -> std::result::Result<Value, (AgentTransportErrorCode, String)> {
    let detail_request = decode_operation_payload::<WorkerFindingDetailRequest>(
        request,
        "worker.get_finding_detail",
    )?;
    let detail = request
        .gateway_snapshot
        .finding_details
        .iter()
        .find(|detail| detail.finding.finding_id == detail_request.finding_id)
        .ok_or_else(|| {
            (
                AgentTransportErrorCode::GatewayDataMissing,
                format!(
                    "rr agent could not find finding detail '{}' in gateway_snapshot.finding_details",
                    detail_request.finding_id
                ),
            )
        })?;
    serde_json::to_value(detail)
        .map_err(|err| (AgentTransportErrorCode::PayloadInvalid, err.to_string()))
}

fn artifact_excerpt_payload(
    request: &AgentTransportRequestEnvelope,
) -> std::result::Result<Value, (AgentTransportErrorCode, String)> {
    let excerpt_request = decode_operation_payload::<WorkerArtifactExcerptRequest>(
        request,
        "worker.get_artifact_excerpt",
    )?;
    let excerpt = request
        .gateway_snapshot
        .artifact_excerpts
        .iter()
        .find(|excerpt| excerpt.artifact_id == excerpt_request.artifact_id)
        .ok_or_else(|| {
            (
                AgentTransportErrorCode::GatewayDataMissing,
                format!(
                    "rr agent could not find artifact excerpt '{}' in gateway_snapshot.artifact_excerpts",
                    excerpt_request.artifact_id
                ),
            )
        })?;
    serde_json::to_value(excerpt)
        .map_err(|err| (AgentTransportErrorCode::PayloadInvalid, err.to_string()))
}

fn submit_stage_result_payload(
    request: &AgentTransportRequestEnvelope,
) -> std::result::Result<Value, (AgentTransportErrorCode, String)> {
    request
        .review_task
        .validate_capability_profile(&request.capability_profile)
        .map_err(|err| (AgentTransportErrorCode::ValidationFailed, err.to_string()))?;
    let result =
        decode_operation_payload::<WorkerStageResult>(request, "worker.submit_stage_result")?;
    request
        .review_task
        .validate_stage_result(&result)
        .map_err(|err| (AgentTransportErrorCode::ValidationFailed, err.to_string()))?;
    let acceptance = WorkerStageResultAcceptance {
        review_session_id: result.review_session_id.clone(),
        review_run_id: result.review_run_id.clone(),
        review_task_id: result.review_task_id.clone(),
        task_nonce: result.task_nonce.clone(),
        result_schema_id: result.schema_id.clone(),
        outcome: result.outcome,
        structured_findings_pack_present: result.structured_findings_pack.is_some(),
        clarification_request_count: result.clarification_requests.len(),
        memory_review_request_count: result.memory_review_requests.len(),
        follow_up_proposal_count: result.follow_up_proposals.len(),
        warnings: result.warnings.clone(),
    };
    serde_json::to_value(acceptance)
        .map_err(|err| (AgentTransportErrorCode::PayloadInvalid, err.to_string()))
}

fn echo_request_payload<T: Serialize + DeserializeOwned>(
    request: &AgentTransportRequestEnvelope,
    operation_name: &str,
) -> std::result::Result<Value, (AgentTransportErrorCode, String)> {
    let payload = decode_operation_payload::<T>(request, operation_name)?;
    serde_json::to_value(payload)
        .map_err(|err| (AgentTransportErrorCode::PayloadInvalid, err.to_string()))
}

fn decode_operation_payload<T: DeserializeOwned>(
    request: &AgentTransportRequestEnvelope,
    operation_name: &str,
) -> std::result::Result<T, (AgentTransportErrorCode, String)> {
    let payload = request.operation_request.payload.clone().ok_or_else(|| {
        (
            AgentTransportErrorCode::PayloadMissing,
            format!("rr agent {operation_name} requires a JSON payload"),
        )
    })?;
    serde_json::from_value(payload).map_err(|err| {
        (
            AgentTransportErrorCode::PayloadInvalid,
            format!("rr agent {operation_name} payload is invalid: {err}"),
        )
    })
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
    pub target_locator: String,
    pub body: String,
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostedActionItem {
    pub id: String,
    pub posted_action_id: String,
    pub draft_id: String,
    pub status: PostingAdapterItemStatus,
    pub remote_identifier: Option<String>,
    pub failure_code: Option<String>,
}

pub trait OutboundPostingAdapter {
    fn post_approved_draft_batch(
        &self,
        target: &ReviewTarget,
        batch: &OutboundDraftBatch,
        drafts: &[OutboundDraft],
    ) -> std::result::Result<Vec<PostingAdapterItemResult>, String>;
}

pub struct ExplicitPostingInput<'a> {
    pub action_id: &'a str,
    pub provider: &'a str,
    pub target: &'a ReviewTarget,
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

pub fn posted_action_item_id(posted_action_id: &str, draft_id: &str) -> String {
    format!("{posted_action_id}:{draft_id}")
}

pub fn posted_action_items_from_item_results(
    posted_action_id: &str,
    item_results: &[PostingAdapterItemResult],
) -> Vec<PostedActionItem> {
    item_results
        .iter()
        .map(|item| PostedActionItem {
            id: posted_action_item_id(posted_action_id, &item.draft_id),
            posted_action_id: posted_action_id.to_owned(),
            draft_id: item.draft_id.clone(),
            status: item.status.clone(),
            remote_identifier: item.remote_identifier.clone(),
            failure_code: item.failure_code.clone(),
        })
        .collect()
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
        if draft.target_locator.trim().is_empty() {
            issues.push(OutboundDraftBatchIssue {
                draft_id: Some(draft.id.clone()),
                reason_code: "missing_target_locator".to_owned(),
            });
        }
        if draft.body.trim().is_empty() {
            issues.push(OutboundDraftBatchIssue {
                draft_id: Some(draft.id.clone()),
                reason_code: "missing_postable_body".to_owned(),
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

fn approval_invalidation_reason_for_linkage_issues(
    validation: &OutboundDraftBatchValidation,
) -> &'static str {
    if validation.issues.iter().any(|issue| {
        matches!(
            issue.reason_code.as_str(),
            "empty_batch" | "missing_finding_link"
        )
    }) {
        "missing_local_state"
    } else {
        "local_state_drift"
    }
}

pub fn evaluate_outbound_post_gate(input: OutboundPostGateInput<'_>) -> OutboundPostGateDecision {
    let validation = validate_outbound_draft_batch_linkage(input.batch, input.drafts);
    if !validation.valid {
        return OutboundPostGateDecision::Blocked {
            reason_code: format!(
                "approval_invalidated:{}",
                approval_invalidation_reason_for_linkage_issues(&validation)
            ),
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

    let item_results =
        match adapter.post_approved_draft_batch(input.target, input.batch, input.drafts) {
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
    fn candidate_audit_plan_explicitly_allows_candidates_without_widening_promotion_review() {
        let anchor_hints = vec!["finding-1".to_owned()];
        let plan = plan_search_query(SearchQueryPlanningInput {
            query_text: "stale draft invalidation",
            query_mode: Some("candidate_audit"),
            anchor_hints: &anchor_hints,
            supports_candidate_audit: true,
            supports_promotion_review: false,
        })
        .expect("candidate audit should plan");

        assert_eq!(
            plan.candidate_visibility,
            SearchCandidateVisibility::CandidateAuditOnly
        );
        assert_eq!(
            plan.trust_floor,
            SearchTrustFloor::CandidateInspectionAllowed
        );
        assert_eq!(
            plan.session_baseline,
            SearchSessionBaseline::AnchorScopedContext
        );
        assert_eq!(plan.semantic_posture, SearchSemanticPosture::LexicalOnly);
        assert_eq!(
            plan.strategy.primary_lane,
            SearchRetrievalLane::CandidateAudit
        );
        assert!(plan.strategy.candidate_audit);
        assert!(!plan.strategy.semantic);
        assert!(plan.includes_tentative_candidates());
        assert!(plan.strategy_reason().contains("candidate audit"));
    }

    #[test]
    fn recall_plan_keeps_candidates_hidden_while_planning_prior_review_and_semantic_lanes() {
        let plan = plan_search_query(SearchQueryPlanningInput {
            query_text: "stale draft invalidation",
            query_mode: Some("recall"),
            anchor_hints: &[],
            supports_candidate_audit: true,
            supports_promotion_review: false,
        })
        .expect("recall should plan");

        assert_eq!(plan.scope_set, SearchScopeSet::CurrentRepository);
        assert_eq!(
            plan.session_baseline,
            SearchSessionBaseline::AmbientSessionOptional
        );
        assert_eq!(plan.anchor_set, SearchAnchorSet::None);
        assert_eq!(plan.trust_floor, SearchTrustFloor::PromotedAndEvidenceOnly);
        assert_eq!(plan.candidate_visibility, SearchCandidateVisibility::Hidden);
        assert_eq!(
            plan.semantic_posture,
            SearchSemanticPosture::DegradedSemanticVisible
        );
        assert_eq!(
            plan.strategy.primary_lane,
            SearchRetrievalLane::LexicalRecall
        );
        assert!(plan.strategy.lexical);
        assert!(plan.strategy.prior_review);
        assert!(plan.strategy.semantic);
        assert!(!plan.strategy.candidate_audit);
        assert!(!plan.includes_tentative_candidates());
        assert!(plan.strategy_reason().contains("free-text recall"));
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
