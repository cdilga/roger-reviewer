use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use roger_app_core::{
    ProviderContinuityCapability, ResumeAttemptOutcome, ResumeBundle, ResumeDecision,
    ResumeSessionState, ReviewTarget, SessionLocator, Surface, decide_resume_strategy,
};
use rusqlite::{Connection, OptionalExtension, params, params_from_iter, types::Value};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const CURRENT_SCHEMA_VERSION: i64 = 8;
const MIGRATION_0001: &str = include_str!("../migrations/0001_init.sql");
const MIGRATION_0002: &str = include_str!("../migrations/0002_session_ledger.sql");
const MIGRATION_0003: &str = include_str!("../migrations/0003_launch_binding_context.sql");
const MIGRATION_0004: &str = include_str!("../migrations/0004_launch_profile_routing.sql");
const MIGRATION_0005: &str = include_str!("../migrations/0005_prompt_invocation_outcomes.sql");
const MIGRATION_0006: &str = include_str!("../migrations/0006_finding_materialization.sql");
const MIGRATION_0007: &str =
    include_str!("../migrations/0007_prior_review_lookup_memory_hooks.sql");
const MIGRATION_0008: &str = include_str!("../migrations/0008_worktree_preflight_plans.sql");

#[derive(Debug)]
pub enum StorageError {
    Sqlite(rusqlite::Error),
    Io(std::io::Error),
    Serialization(serde_json::Error),
    NotFound { entity: &'static str, id: String },
    Conflict { entity: &'static str, id: String },
}

impl Display for StorageError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sqlite(err) => write!(f, "sqlite error: {err}"),
            Self::Io(err) => write!(f, "io error: {err}"),
            Self::Serialization(err) => write!(f, "serialization error: {err}"),
            Self::NotFound { entity, id } => write!(f, "{entity} not found: {id}"),
            Self::Conflict { entity, id } => write!(f, "stale write conflict for {entity}: {id}"),
        }
    }
}

impl std::error::Error for StorageError {}

impl From<rusqlite::Error> for StorageError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sqlite(value)
    }
}

impl From<std::io::Error> for StorageError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for StorageError {
    fn from(value: serde_json::Error) -> Self {
        Self::Serialization(value)
    }
}

pub type Result<T> = std::result::Result<T, StorageError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactStorageKind {
    Inline,
    ExternalContentAddressed,
    DerivedSidecar,
}

impl ArtifactStorageKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Inline => "inline",
            Self::ExternalContentAddressed => "external_content_addressed",
            Self::DerivedSidecar => "derived_sidecar",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactBudgetClass {
    InlineSummary,
    EvidenceExcerpt,
    ColdArtifact,
    DerivedIndexState,
}

impl ArtifactBudgetClass {
    fn as_str(self) -> &'static str {
        match self {
            Self::InlineSummary => "inline_summary",
            Self::EvidenceExcerpt => "evidence_excerpt",
            Self::ColdArtifact => "cold_artifact",
            Self::DerivedIndexState => "derived_index_state",
        }
    }

    pub fn policy(self) -> ArtifactBudgetPolicy {
        match self {
            Self::InlineSummary => ArtifactBudgetPolicy {
                class: self,
                max_inline_bytes: 4 * 1024,
                storage_preference: ArtifactStorageKind::Inline,
            },
            Self::EvidenceExcerpt => ArtifactBudgetPolicy {
                class: self,
                max_inline_bytes: 16 * 1024,
                storage_preference: ArtifactStorageKind::Inline,
            },
            Self::ColdArtifact => ArtifactBudgetPolicy {
                class: self,
                max_inline_bytes: 0,
                storage_preference: ArtifactStorageKind::ExternalContentAddressed,
            },
            Self::DerivedIndexState => ArtifactBudgetPolicy {
                class: self,
                max_inline_bytes: 0,
                storage_preference: ArtifactStorageKind::DerivedSidecar,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArtifactBudgetPolicy {
    pub class: ArtifactBudgetClass,
    pub max_inline_bytes: usize,
    pub storage_preference: ArtifactStorageKind,
}

impl ArtifactBudgetPolicy {
    pub fn select_storage(self, payload_len: usize) -> ArtifactStorageKind {
        match self.storage_preference {
            ArtifactStorageKind::Inline if payload_len <= self.max_inline_bytes => {
                ArtifactStorageKind::Inline
            }
            ArtifactStorageKind::Inline => ArtifactStorageKind::ExternalContentAddressed,
            other => other,
        }
    }
}

#[derive(Debug, Clone)]
pub struct StorageLayout {
    pub root: PathBuf,
    pub db_path: PathBuf,
    pub artifact_root: PathBuf,
    pub sidecar_root: PathBuf,
}

impl StorageLayout {
    pub fn under(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref().to_path_buf();
        Self {
            db_path: root.join("roger.db"),
            artifact_root: root.join("artifacts"),
            sidecar_root: root.join("sidecars"),
            root,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CreateReviewSession<'a> {
    pub id: &'a str,
    pub review_target: &'a ReviewTarget,
    pub provider: &'a str,
    pub session_locator: Option<&'a SessionLocator>,
    pub resume_bundle_artifact_id: Option<&'a str>,
    pub continuity_state: &'a str,
    pub attention_state: &'a str,
    pub launch_profile_id: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewSessionRecord {
    pub id: String,
    pub review_target: ReviewTarget,
    pub provider: String,
    pub session_locator: Option<SessionLocator>,
    pub resume_bundle_artifact_id: Option<String>,
    pub continuity_state: String,
    pub attention_state: String,
    pub launch_profile_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub row_version: i64,
}

#[derive(Debug, Clone)]
pub struct CreateReviewRun<'a> {
    pub id: &'a str,
    pub session_id: &'a str,
    pub run_kind: &'a str,
    pub repo_snapshot: &'a str,
    pub continuity_quality: &'a str,
    pub session_locator_artifact_id: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewRunRecord {
    pub id: String,
    pub session_id: String,
    pub run_kind: String,
    pub repo_snapshot: String,
    pub continuity_quality: String,
    pub session_locator_artifact_id: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct CreatePromptInvocation<'a> {
    pub id: &'a str,
    pub review_session_id: &'a str,
    pub review_run_id: &'a str,
    pub stage: &'a str,
    pub prompt_preset_id: &'a str,
    pub source_surface: &'a str,
    pub resolved_text_digest: &'a str,
    pub resolved_text_artifact_id: Option<&'a str>,
    pub resolved_text_inline_preview: Option<&'a str>,
    pub explicit_objective: Option<&'a str>,
    pub provider: Option<&'a str>,
    pub model: Option<&'a str>,
    pub scope_context_json: Option<&'a str>,
    pub config_layer_digest: Option<&'a str>,
    pub launch_intake_id: Option<&'a str>,
    pub used_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptInvocationRecord {
    pub id: String,
    pub review_session_id: String,
    pub review_run_id: String,
    pub stage: String,
    pub prompt_preset_id: String,
    pub source_surface: String,
    pub resolved_text_digest: String,
    pub resolved_text_artifact_id: Option<String>,
    pub resolved_text_inline_preview: Option<String>,
    pub explicit_objective: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub scope_context_json: Option<String>,
    pub config_layer_digest: Option<String>,
    pub launch_intake_id: Option<String>,
    pub used_at: i64,
    pub row_version: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct CreateOutcomeEvent<'a> {
    pub id: &'a str,
    pub event_type: &'a str,
    pub review_session_id: &'a str,
    pub review_run_id: Option<&'a str>,
    pub prompt_invocation_id: Option<&'a str>,
    pub actor_kind: &'a str,
    pub actor_id: Option<&'a str>,
    pub source_surface: &'a str,
    pub payload_json: &'a str,
    pub occurred_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutcomeEventRecord {
    pub id: String,
    pub event_type: String,
    pub occurred_at: i64,
    pub review_session_id: String,
    pub review_run_id: Option<String>,
    pub prompt_invocation_id: Option<String>,
    pub actor_kind: String,
    pub actor_id: Option<String>,
    pub source_surface: String,
    pub payload_json: String,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct CreateFinding<'a> {
    pub id: &'a str,
    pub session_id: &'a str,
    pub first_run_id: &'a str,
    pub fingerprint: &'a str,
    pub title: &'a str,
    pub triage_state: &'a str,
    pub outbound_state: &'a str,
}

#[derive(Debug, Clone)]
pub struct CreateMaterializedFinding<'a> {
    pub id: &'a str,
    pub session_id: &'a str,
    pub review_run_id: &'a str,
    pub stage: &'a str,
    pub fingerprint: &'a str,
    pub title: &'a str,
    pub normalized_summary: &'a str,
    pub severity: &'a str,
    pub confidence: &'a str,
    pub triage_state: &'a str,
    pub outbound_state: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializedFindingRecord {
    pub id: String,
    pub session_id: String,
    pub first_run_id: String,
    pub last_seen_run_id: Option<String>,
    pub fingerprint: String,
    pub title: String,
    pub normalized_summary: String,
    pub severity: String,
    pub confidence: String,
    pub first_seen_stage: String,
    pub last_seen_stage: Option<String>,
    pub triage_state: String,
    pub outbound_state: String,
    pub row_version: i64,
}

#[derive(Debug, Clone)]
pub struct CreateCodeEvidenceLocation<'a> {
    pub id: &'a str,
    pub finding_id: &'a str,
    pub review_session_id: &'a str,
    pub review_run_id: &'a str,
    pub evidence_role: &'a str,
    pub repo_rel_path: &'a str,
    pub start_line: i64,
    pub end_line: Option<i64>,
    pub anchor_state: &'a str,
    pub anchor_digest: Option<&'a str>,
    pub excerpt_artifact_id: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeEvidenceLocationRecord {
    pub id: String,
    pub finding_id: String,
    pub review_session_id: String,
    pub review_run_id: String,
    pub evidence_role: String,
    pub repo_rel_path: String,
    pub start_line: i64,
    pub end_line: Option<i64>,
    pub anchor_state: String,
    pub anchor_digest: Option<String>,
    pub excerpt_artifact_id: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct CreateOutboundDraft<'a> {
    pub id: &'a str,
    pub session_id: &'a str,
    pub finding_id: &'a str,
    pub target_locator: &'a str,
    pub payload_digest: &'a str,
    pub body: &'a str,
}

#[derive(Debug, Clone)]
pub struct CreateLaunchProfile<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub source_surface: LaunchSurface,
    pub ui_target: &'a str,
    pub terminal_environment: &'a str,
    pub multiplexer_mode: &'a str,
    pub reuse_policy: &'a str,
    pub repo_root: &'a str,
    pub worktree_strategy: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalLaunchProfileRecord {
    pub id: String,
    pub name: String,
    pub source_surface: String,
    pub ui_target: String,
    pub terminal_environment: String,
    pub multiplexer_mode: String,
    pub reuse_policy: String,
    pub repo_root: String,
    pub worktree_strategy: String,
    pub row_version: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaunchSurface {
    Cli,
    Tui,
    Extension,
    Bridge,
}

impl From<Surface> for LaunchSurface {
    fn from(value: Surface) -> Self {
        match value {
            Surface::Cli => Self::Cli,
            Surface::Tui => Self::Tui,
            Surface::Extension => Self::Extension,
            Surface::Direct => Self::Bridge,
        }
    }
}

impl LaunchSurface {
    fn as_str(self) -> &'static str {
        match self {
            Self::Cli => "cli",
            Self::Tui => "tui",
            Self::Extension => "extension",
            Self::Bridge => "bridge",
        }
    }
}

#[derive(Debug, Clone)]
pub struct CreateSessionLaunchBinding<'a> {
    pub id: &'a str,
    pub session_id: &'a str,
    pub repo_locator: &'a str,
    pub review_target: Option<&'a ReviewTarget>,
    pub surface: LaunchSurface,
    pub launch_profile_id: Option<&'a str>,
    pub ui_target: Option<&'a str>,
    pub instance_preference: Option<&'a str>,
    pub cwd: Option<&'a str>,
    pub worktree_root: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionLaunchBindingRecord {
    pub id: String,
    pub session_id: String,
    pub repo_locator: String,
    pub review_target: Option<ReviewTarget>,
    pub surface: String,
    pub launch_profile_id: Option<String>,
    pub ui_target: Option<String>,
    pub instance_preference: Option<String>,
    pub cwd: Option<String>,
    pub worktree_root: Option<String>,
    pub row_version: i64,
}

#[derive(Debug, Clone)]
pub struct ResolveSessionLaunchBinding<'a> {
    pub surface: LaunchSurface,
    pub repo_locator: &'a str,
    pub review_target: Option<&'a ReviewTarget>,
    pub ui_target: Option<&'a str>,
    pub instance_preference: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct ResolveLaunchProfileRoute {
    pub source_surface: LaunchSurface,
    pub requested_profile_id: Option<String>,
    pub fallback_profile_id: Option<String>,
    pub available_terminal_environments: Vec<String>,
    pub available_multiplexer_modes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchProfileRouteDecision {
    pub profile_id: String,
    pub source_surface: String,
    pub ui_target: String,
    pub terminal_environment: String,
    pub multiplexer_mode: String,
    pub reuse_policy: String,
    pub degraded: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchProfileRouteResolution {
    Resolved(LaunchProfileRouteDecision),
    NotFound { reason: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaunchPreflightResultClass {
    Ready,
    ReadyWithActions,
    ProfileRequired,
    UnsafeDefaultBlocked,
    VerificationFailed,
}

impl LaunchPreflightResultClass {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::ReadyWithActions => "ready_with_actions",
            Self::ProfileRequired => "profile_required",
            Self::UnsafeDefaultBlocked => "unsafe_default_blocked",
            Self::VerificationFailed => "verification_failed",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "ready" => Some(Self::Ready),
            "ready_with_actions" => Some(Self::ReadyWithActions),
            "profile_required" => Some(Self::ProfileRequired),
            "unsafe_default_blocked" => Some(Self::UnsafeDefaultBlocked),
            "verification_failed" => Some(Self::VerificationFailed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaunchPreflightMode {
    CurrentCheckout,
    NamedInstance,
    Worktree,
}

impl LaunchPreflightMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::CurrentCheckout => "current_checkout",
            Self::NamedInstance => "named_instance",
            Self::Worktree => "worktree",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "current_checkout" => Some(Self::CurrentCheckout),
            "named_instance" => Some(Self::NamedInstance),
            "worktree" => Some(Self::Worktree),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LaunchPreflightResourceDecisions {
    pub env_config: HashMap<String, String>,
    pub ports: Vec<String>,
    pub repo_local_db_paths: Vec<String>,
    pub container_naming: Vec<String>,
    pub caches: Vec<String>,
    pub artifacts: Vec<String>,
    pub logs: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct CreateLaunchPreflightPlan<'a> {
    pub id: &'a str,
    pub session_id: &'a str,
    pub launch_binding_id: Option<&'a str>,
    pub result_class: LaunchPreflightResultClass,
    pub selected_mode: LaunchPreflightMode,
    pub resource_decisions: &'a LaunchPreflightResourceDecisions,
    pub required_operator_actions: &'a [String],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchPreflightPlanRecord {
    pub id: String,
    pub session_id: String,
    pub launch_binding_id: Option<String>,
    pub result_class: LaunchPreflightResultClass,
    pub selected_mode: LaunchPreflightMode,
    pub resource_decisions: LaunchPreflightResourceDecisions,
    pub required_operator_actions: Vec<String>,
    pub row_version: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct ResolveLaunchPreflightPlan<'a> {
    pub session_id: &'a str,
    pub launch_binding_id: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct SessionFinderQuery {
    pub repository: Option<String>,
    pub pull_request_number: Option<u64>,
    pub attention_states: Vec<String>,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionFinderEntry {
    pub session_id: String,
    pub repository: String,
    pub pull_request_number: u64,
    pub attention_state: String,
    pub provider: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct ResolveSessionReentry {
    pub explicit_session_id: Option<String>,
    pub repository: Option<String>,
    pub pull_request_number: Option<u64>,
    pub source_surface: LaunchSurface,
    pub ui_target: Option<String>,
    pub instance_preference: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionReentryResolution {
    Resolved {
        session: ReviewSessionRecord,
        binding: Option<SessionLaunchBindingRecord>,
    },
    PickerRequired {
        reason: String,
        candidates: Vec<SessionFinderEntry>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionBindingResolution {
    Resolved(SessionLaunchBindingRecord),
    NotFound,
    Ambiguous { session_ids: Vec<String> },
    Stale { binding_id: String, reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumeLedgerRecord {
    pub binding: SessionLaunchBindingRecord,
    pub session: ReviewSessionRecord,
    pub resume_bundle: Option<ResumeBundle>,
    pub decision: ResumeDecision,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResumeLedgerResolution {
    Resolved(ResumeLedgerRecord),
    NotFound,
    Ambiguous { session_ids: Vec<String> },
    Stale { binding_id: String, reason: String },
    MissingSession { session_id: String },
}

#[derive(Debug, Clone)]
pub struct UpdateIndexState<'a> {
    pub scope_key: &'a str,
    pub generation: i64,
    pub status: &'a str,
    pub artifact_digest: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct UpsertMemoryItem<'a> {
    pub id: &'a str,
    pub scope_key: &'a str,
    pub memory_class: &'a str,
    pub state: &'a str,
    pub statement: &'a str,
    pub normalized_key: &'a str,
    pub anchor_digest: Option<&'a str>,
    pub source_kind: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryItemRecord {
    pub id: String,
    pub scope_key: String,
    pub memory_class: String,
    pub state: String,
    pub statement: String,
    pub normalized_key: String,
    pub anchor_digest: Option<String>,
    pub source_kind: String,
    pub row_version: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticLookupTargetKind {
    EvidenceFinding,
    MemoryItem,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticLookupCandidate {
    pub target_kind: SemanticLookupTargetKind,
    pub target_id: String,
    pub score: f32,
}

#[derive(Debug, Clone)]
pub struct PriorReviewLookupQuery<'a> {
    pub scope_key: &'a str,
    pub repository: &'a str,
    pub query_text: &'a str,
    pub limit: usize,
    pub include_tentative_candidates: bool,
    pub allow_project_scope: bool,
    pub allow_org_scope: bool,
    pub semantic_assets_verified: bool,
    pub semantic_candidates: Vec<SemanticLookupCandidate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PriorReviewRetrievalMode {
    Hybrid,
    LexicalOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriorReviewEvidenceHit {
    pub finding_id: String,
    pub session_id: String,
    pub review_run_id: Option<String>,
    pub repository: String,
    pub pull_request_number: u64,
    pub fingerprint: String,
    pub title: String,
    pub normalized_summary: String,
    pub severity: String,
    pub confidence: String,
    pub triage_state: String,
    pub outbound_state: String,
    pub lexical_score: i64,
    pub semantic_score_milli: i64,
    pub fused_score: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriorReviewMemoryHit {
    pub memory_id: String,
    pub scope_key: String,
    pub memory_class: String,
    pub state: String,
    pub statement: String,
    pub normalized_key: String,
    pub anchor_digest: Option<String>,
    pub source_kind: String,
    pub lexical_score: i64,
    pub semantic_score_milli: i64,
    pub fused_score: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriorReviewLookupResult {
    pub scope_bucket: String,
    pub mode: PriorReviewRetrievalMode,
    pub degraded_reasons: Vec<String>,
    pub evidence_hits: Vec<PriorReviewEvidenceHit>,
    pub promoted_memory: Vec<PriorReviewMemoryHit>,
    pub tentative_candidates: Vec<PriorReviewMemoryHit>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredArtifact {
    pub id: String,
    pub digest: String,
    pub storage_kind: ArtifactStorageKind,
    pub size_bytes: usize,
    pub inline_bytes: Option<Vec<u8>>,
    pub relative_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionOverview {
    pub attention_state: String,
    pub row_version: i64,
    pub run_count: i64,
    pub finding_count: i64,
    pub draft_count: i64,
    pub approval_count: i64,
    pub posted_action_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalRecord {
    pub id: String,
    pub draft_id: String,
    pub payload_digest: String,
    pub target_locator: String,
    pub row_version: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexStateRecord {
    pub scope_key: String,
    pub generation: i64,
    pub status: String,
    pub artifact_digest: Option<String>,
    pub row_version: i64,
}

pub struct RogerStore {
    conn: Connection,
    layout: StorageLayout,
}

impl RogerStore {
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let layout = StorageLayout::under(root);
        fs::create_dir_all(&layout.root)?;
        fs::create_dir_all(&layout.artifact_root)?;
        fs::create_dir_all(&layout.sidecar_root)?;

        let conn = Connection::open(&layout.db_path)?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;

        let store = Self { conn, layout };
        store.apply_migrations()?;
        Ok(store)
    }

    pub fn layout(&self) -> &StorageLayout {
        &self.layout
    }

    pub fn schema_version(&self) -> Result<i64> {
        Ok(self
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))?)
    }

    pub fn create_review_session(
        &self,
        input: CreateReviewSession<'_>,
    ) -> Result<ReviewSessionRecord> {
        let now = now_ts();
        self.conn.execute(
            "INSERT INTO review_sessions (
                id, review_target, provider, session_locator, resume_bundle_artifact_id,
                continuity_state, attention_state, launch_profile_id, row_version, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, ?9, ?9)",
            params![
                input.id,
                serde_json::to_string(input.review_target)?,
                input.provider,
                input.session_locator.map(serde_json::to_string).transpose()?,
                input.resume_bundle_artifact_id,
                input.continuity_state,
                input.attention_state,
                input.launch_profile_id,
                now
            ],
        )?;

        Ok(ReviewSessionRecord {
            id: input.id.to_owned(),
            review_target: input.review_target.clone(),
            provider: input.provider.to_owned(),
            session_locator: input.session_locator.cloned(),
            resume_bundle_artifact_id: input.resume_bundle_artifact_id.map(ToOwned::to_owned),
            continuity_state: input.continuity_state.to_owned(),
            attention_state: input.attention_state.to_owned(),
            launch_profile_id: input.launch_profile_id.map(ToOwned::to_owned),
            created_at: now,
            updated_at: now,
            row_version: 0,
        })
    }

    pub fn create_review_run(&self, input: CreateReviewRun<'_>) -> Result<()> {
        self.conn.execute(
            "INSERT INTO review_runs (
                id, session_id, run_kind, repo_snapshot,
                continuity_quality, session_locator_artifact_id, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                input.id,
                input.session_id,
                input.run_kind,
                input.repo_snapshot,
                input.continuity_quality,
                input.session_locator_artifact_id,
                now_ts()
            ],
        )?;
        Ok(())
    }

    pub fn record_prompt_invocation(
        &self,
        input: CreatePromptInvocation<'_>,
    ) -> Result<PromptInvocationRecord> {
        let now = now_ts();
        self.conn.execute(
            "INSERT INTO prompt_invocations (
                id, review_session_id, review_run_id, stage, prompt_preset_id,
                source_surface, resolved_text_digest, resolved_text_artifact_id,
                resolved_text_inline_preview, explicit_objective, provider, model,
                scope_context_json, config_layer_digest, launch_intake_id, used_at,
                row_version, created_at, updated_at
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5,
                ?6, ?7, ?8,
                ?9, ?10, ?11, ?12,
                ?13, ?14, ?15, ?16,
                0, ?17, ?17
            )",
            params![
                input.id,
                input.review_session_id,
                input.review_run_id,
                input.stage,
                input.prompt_preset_id,
                input.source_surface,
                input.resolved_text_digest,
                input.resolved_text_artifact_id,
                input.resolved_text_inline_preview,
                input.explicit_objective,
                input.provider,
                input.model,
                input.scope_context_json,
                input.config_layer_digest,
                input.launch_intake_id,
                input.used_at,
                now
            ],
        )?;

        Ok(PromptInvocationRecord {
            id: input.id.to_owned(),
            review_session_id: input.review_session_id.to_owned(),
            review_run_id: input.review_run_id.to_owned(),
            stage: input.stage.to_owned(),
            prompt_preset_id: input.prompt_preset_id.to_owned(),
            source_surface: input.source_surface.to_owned(),
            resolved_text_digest: input.resolved_text_digest.to_owned(),
            resolved_text_artifact_id: input.resolved_text_artifact_id.map(ToOwned::to_owned),
            resolved_text_inline_preview: input.resolved_text_inline_preview.map(ToOwned::to_owned),
            explicit_objective: input.explicit_objective.map(ToOwned::to_owned),
            provider: input.provider.map(ToOwned::to_owned),
            model: input.model.map(ToOwned::to_owned),
            scope_context_json: input.scope_context_json.map(ToOwned::to_owned),
            config_layer_digest: input.config_layer_digest.map(ToOwned::to_owned),
            launch_intake_id: input.launch_intake_id.map(ToOwned::to_owned),
            used_at: input.used_at,
            row_version: 0,
            created_at: now,
            updated_at: now,
        })
    }

    pub fn prompt_invocation(&self, invocation_id: &str) -> Result<Option<PromptInvocationRecord>> {
        self.conn
            .query_row(
                "SELECT id, review_session_id, review_run_id, stage, prompt_preset_id,
                    source_surface, resolved_text_digest, resolved_text_artifact_id,
                    resolved_text_inline_preview, explicit_objective, provider, model,
                    scope_context_json, config_layer_digest, launch_intake_id, used_at,
                    row_version, created_at, updated_at
                FROM prompt_invocations
                WHERE id = ?1",
                params![invocation_id],
                |row| {
                    Ok(PromptInvocationRecord {
                        id: row.get(0)?,
                        review_session_id: row.get(1)?,
                        review_run_id: row.get(2)?,
                        stage: row.get(3)?,
                        prompt_preset_id: row.get(4)?,
                        source_surface: row.get(5)?,
                        resolved_text_digest: row.get(6)?,
                        resolved_text_artifact_id: row.get(7)?,
                        resolved_text_inline_preview: row.get(8)?,
                        explicit_objective: row.get(9)?,
                        provider: row.get(10)?,
                        model: row.get(11)?,
                        scope_context_json: row.get(12)?,
                        config_layer_digest: row.get(13)?,
                        launch_intake_id: row.get(14)?,
                        used_at: row.get(15)?,
                        row_version: row.get(16)?,
                        created_at: row.get(17)?,
                        updated_at: row.get(18)?,
                    })
                },
            )
            .optional()
            .map_err(StorageError::from)
    }

    pub fn record_outcome_event(&self, input: CreateOutcomeEvent<'_>) -> Result<()> {
        self.conn.execute(
            "INSERT INTO outcome_events (
                id, event_type, occurred_at, review_session_id, review_run_id,
                prompt_invocation_id, actor_kind, actor_id, source_surface,
                payload_json, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                input.id,
                input.event_type,
                input.occurred_at,
                input.review_session_id,
                input.review_run_id,
                input.prompt_invocation_id,
                input.actor_kind,
                input.actor_id,
                input.source_surface,
                input.payload_json,
                now_ts()
            ],
        )?;
        Ok(())
    }

    pub fn outcome_events_for_run(
        &self,
        review_session_id: &str,
        review_run_id: &str,
    ) -> Result<Vec<OutcomeEventRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, event_type, occurred_at, review_session_id, review_run_id,
                prompt_invocation_id, actor_kind, actor_id, source_surface, payload_json, created_at
            FROM outcome_events
            WHERE review_session_id = ?1 AND review_run_id = ?2
            ORDER BY occurred_at ASC, rowid ASC",
        )?;
        let rows = stmt.query_map(params![review_session_id, review_run_id], |row| {
            Ok(OutcomeEventRecord {
                id: row.get(0)?,
                event_type: row.get(1)?,
                occurred_at: row.get(2)?,
                review_session_id: row.get(3)?,
                review_run_id: row.get(4)?,
                prompt_invocation_id: row.get(5)?,
                actor_kind: row.get(6)?,
                actor_id: row.get(7)?,
                source_surface: row.get(8)?,
                payload_json: row.get(9)?,
                created_at: row.get(10)?,
            })
        })?;

        let mut events = Vec::new();
        for row in rows {
            events.push(row?);
        }
        Ok(events)
    }

    pub fn create_finding(&self, input: CreateFinding<'_>) -> Result<()> {
        let now = now_ts();
        self.conn.execute(
            "INSERT INTO findings (
                id, session_id, first_run_id, fingerprint, title,
                triage_state, outbound_state, row_version, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8, ?8)",
            params![
                input.id,
                input.session_id,
                input.first_run_id,
                input.fingerprint,
                input.title,
                input.triage_state,
                input.outbound_state,
                now
            ],
        )?;
        self.conn.execute(
            "INSERT INTO finding_decision_events (
                id, finding_id, triage_state, outbound_state, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                format!("fde-{}", input.id),
                input.id,
                input.triage_state,
                input.outbound_state,
                now
            ],
        )?;
        Ok(())
    }

    pub fn upsert_materialized_finding(
        &self,
        input: CreateMaterializedFinding<'_>,
    ) -> Result<MaterializedFindingRecord> {
        let now = now_ts();
        let existing_id = self
            .conn
            .query_row(
                "SELECT id FROM findings
                WHERE session_id = ?1 AND fingerprint = ?2",
                params![input.session_id, input.fingerprint],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        if let Some(existing_id) = existing_id {
            self.conn.execute(
                "UPDATE findings
                SET title = ?1,
                    normalized_summary = ?2,
                    severity = ?3,
                    confidence = ?4,
                    last_seen_run_id = ?5,
                    last_seen_stage = ?6,
                    updated_at = ?7,
                    row_version = row_version + 1
                WHERE id = ?8",
                params![
                    input.title,
                    input.normalized_summary,
                    input.severity,
                    input.confidence,
                    input.review_run_id,
                    input.stage,
                    now,
                    existing_id
                ],
            )?;

            self.conn.execute(
                "INSERT INTO finding_decision_events (
                    id, finding_id, triage_state, outbound_state, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    format!("fde-{}-{now}", existing_id),
                    existing_id,
                    input.triage_state,
                    input.outbound_state,
                    now
                ],
            )?;

            return self.materialized_finding(&existing_id)?.ok_or_else(|| {
                StorageError::NotFound {
                    entity: "finding",
                    id: existing_id,
                }
            });
        }

        self.conn.execute(
            "INSERT INTO findings (
                id, session_id, first_run_id, fingerprint, title,
                normalized_summary, severity, confidence, first_seen_stage,
                last_seen_run_id, last_seen_stage,
                triage_state, outbound_state, row_version, created_at, updated_at
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5,
                ?6, ?7, ?8, ?9,
                ?10, ?11,
                ?12, ?13, 0, ?14, ?14
            )",
            params![
                input.id,
                input.session_id,
                input.review_run_id,
                input.fingerprint,
                input.title,
                input.normalized_summary,
                input.severity,
                input.confidence,
                input.stage,
                input.review_run_id,
                input.stage,
                input.triage_state,
                input.outbound_state,
                now
            ],
        )?;

        self.conn.execute(
            "INSERT INTO finding_decision_events (
                id, finding_id, triage_state, outbound_state, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                format!("fde-{}-{now}", input.id),
                input.id,
                input.triage_state,
                input.outbound_state,
                now
            ],
        )?;

        self.materialized_finding(input.id)?
            .ok_or_else(|| StorageError::NotFound {
                entity: "finding",
                id: input.id.to_owned(),
            })
    }

    pub fn materialized_finding(
        &self,
        finding_id: &str,
    ) -> Result<Option<MaterializedFindingRecord>> {
        self.conn
            .query_row(
                "SELECT id, session_id, first_run_id, last_seen_run_id, fingerprint, title,
                    normalized_summary, severity, confidence, first_seen_stage, last_seen_stage,
                    triage_state, outbound_state, row_version
                FROM findings
                WHERE id = ?1",
                params![finding_id],
                |row| {
                    Ok(MaterializedFindingRecord {
                        id: row.get(0)?,
                        session_id: row.get(1)?,
                        first_run_id: row.get(2)?,
                        last_seen_run_id: row.get(3)?,
                        fingerprint: row.get(4)?,
                        title: row.get(5)?,
                        normalized_summary: row.get(6)?,
                        severity: row.get(7)?,
                        confidence: row.get(8)?,
                        first_seen_stage: row.get(9)?,
                        last_seen_stage: row.get(10)?,
                        triage_state: row.get(11)?,
                        outbound_state: row.get(12)?,
                        row_version: row.get(13)?,
                    })
                },
            )
            .optional()
            .map_err(StorageError::from)
    }

    pub fn materialized_findings_for_run(
        &self,
        review_session_id: &str,
        review_run_id: &str,
    ) -> Result<Vec<MaterializedFindingRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, first_run_id, last_seen_run_id, fingerprint, title,
                normalized_summary, severity, confidence, first_seen_stage, last_seen_stage,
                triage_state, outbound_state, row_version
            FROM findings
            WHERE session_id = ?1
              AND COALESCE(last_seen_run_id, first_run_id) = ?2
            ORDER BY rowid ASC",
        )?;
        let rows = stmt.query_map(params![review_session_id, review_run_id], |row| {
            Ok(MaterializedFindingRecord {
                id: row.get(0)?,
                session_id: row.get(1)?,
                first_run_id: row.get(2)?,
                last_seen_run_id: row.get(3)?,
                fingerprint: row.get(4)?,
                title: row.get(5)?,
                normalized_summary: row.get(6)?,
                severity: row.get(7)?,
                confidence: row.get(8)?,
                first_seen_stage: row.get(9)?,
                last_seen_stage: row.get(10)?,
                triage_state: row.get(11)?,
                outbound_state: row.get(12)?,
                row_version: row.get(13)?,
            })
        })?;

        let mut findings = Vec::new();
        for row in rows {
            findings.push(row?);
        }
        Ok(findings)
    }

    pub fn add_code_evidence_location(&self, input: CreateCodeEvidenceLocation<'_>) -> Result<()> {
        self.conn.execute(
            "INSERT INTO code_evidence_locations (
                id, finding_id, review_session_id, review_run_id, evidence_role,
                repo_rel_path, start_line, end_line, anchor_state, anchor_digest,
                excerpt_artifact_id, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                input.id,
                input.finding_id,
                input.review_session_id,
                input.review_run_id,
                input.evidence_role,
                input.repo_rel_path,
                input.start_line,
                input.end_line,
                input.anchor_state,
                input.anchor_digest,
                input.excerpt_artifact_id,
                now_ts()
            ],
        )?;
        Ok(())
    }

    pub fn code_evidence_locations_for_finding(
        &self,
        finding_id: &str,
    ) -> Result<Vec<CodeEvidenceLocationRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, finding_id, review_session_id, review_run_id, evidence_role,
                repo_rel_path, start_line, end_line, anchor_state, anchor_digest,
                excerpt_artifact_id, created_at
            FROM code_evidence_locations
            WHERE finding_id = ?1
            ORDER BY rowid ASC",
        )?;
        let rows = stmt.query_map(params![finding_id], |row| {
            Ok(CodeEvidenceLocationRecord {
                id: row.get(0)?,
                finding_id: row.get(1)?,
                review_session_id: row.get(2)?,
                review_run_id: row.get(3)?,
                evidence_role: row.get(4)?,
                repo_rel_path: row.get(5)?,
                start_line: row.get(6)?,
                end_line: row.get(7)?,
                anchor_state: row.get(8)?,
                anchor_digest: row.get(9)?,
                excerpt_artifact_id: row.get(10)?,
                created_at: row.get(11)?,
            })
        })?;

        let mut evidence_rows = Vec::new();
        for row in rows {
            evidence_rows.push(row?);
        }
        Ok(evidence_rows)
    }

    pub fn create_outbound_draft(&self, input: CreateOutboundDraft<'_>) -> Result<()> {
        let now = now_ts();
        self.conn.execute(
            "INSERT INTO outbound_drafts (
                id, session_id, finding_id, target_locator,
                payload_digest, body, row_version, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7, ?7)",
            params![
                input.id,
                input.session_id,
                input.finding_id,
                input.target_locator,
                input.payload_digest,
                input.body,
                now
            ],
        )?;
        Ok(())
    }

    pub fn approve_outbound_draft(
        &self,
        approval_id: &str,
        draft_id: &str,
        payload_digest: &str,
        target_locator: &str,
    ) -> Result<()> {
        let now = now_ts();
        self.conn.execute(
            "INSERT INTO outbound_approval_tokens (
                id, draft_id, payload_digest, target_locator,
                approved_at, revoked_at, row_version, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, NULL, 0, ?5, ?5)",
            params![approval_id, draft_id, payload_digest, target_locator, now],
        )?;
        Ok(())
    }

    pub fn record_posted_action(
        &self,
        action_id: &str,
        draft_id: &str,
        remote_locator: &str,
        payload_digest: &str,
        status: &str,
    ) -> Result<()> {
        let now = now_ts();
        self.conn.execute(
            "INSERT INTO posted_actions (
                id, draft_id, remote_locator, payload_digest, status, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                action_id,
                draft_id,
                remote_locator,
                payload_digest,
                status,
                now
            ],
        )?;
        Ok(())
    }

    pub fn put_launch_profile(&self, profile: CreateLaunchProfile<'_>) -> Result<()> {
        let now = now_ts();
        self.conn.execute(
            "INSERT INTO local_launch_profiles (
                id, name, source_surface, ui_target, terminal_environment,
                multiplexer_mode, reuse_policy, repo_root, worktree_strategy,
                row_version, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, ?10, ?10)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                source_surface = excluded.source_surface,
                ui_target = excluded.ui_target,
                terminal_environment = excluded.terminal_environment,
                multiplexer_mode = excluded.multiplexer_mode,
                reuse_policy = excluded.reuse_policy,
                repo_root = excluded.repo_root,
                worktree_strategy = excluded.worktree_strategy,
                row_version = local_launch_profiles.row_version + 1,
                updated_at = excluded.updated_at",
            params![
                profile.id,
                profile.name,
                profile.source_surface.as_str(),
                profile.ui_target,
                profile.terminal_environment,
                profile.multiplexer_mode,
                profile.reuse_policy,
                profile.repo_root,
                profile.worktree_strategy,
                now
            ],
        )?;
        Ok(())
    }

    pub fn launch_profile(&self, profile_id: &str) -> Result<Option<LocalLaunchProfileRecord>> {
        self.conn
            .query_row(
                "SELECT id, name, source_surface, ui_target, terminal_environment,
                    multiplexer_mode, reuse_policy, repo_root, worktree_strategy, row_version
                FROM local_launch_profiles
                WHERE id = ?1",
                params![profile_id],
                |row| {
                    Ok(LocalLaunchProfileRecord {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        source_surface: row.get(2)?,
                        ui_target: row.get(3)?,
                        terminal_environment: row.get(4)?,
                        multiplexer_mode: row.get(5)?,
                        reuse_policy: row.get(6)?,
                        repo_root: row.get(7)?,
                        worktree_strategy: row.get(8)?,
                        row_version: row.get(9)?,
                    })
                },
            )
            .optional()
            .map_err(StorageError::from)
    }

    pub fn resolve_launch_profile_route(
        &self,
        query: ResolveLaunchProfileRoute,
    ) -> Result<LaunchProfileRouteResolution> {
        let selected = if let Some(requested_profile_id) = query.requested_profile_id.clone() {
            self.launch_profile(&requested_profile_id)?
        } else if let Some(fallback_profile_id) = query.fallback_profile_id.clone() {
            self.launch_profile(&fallback_profile_id)?
        } else {
            self.conn
                .query_row(
                    "SELECT id, name, source_surface, ui_target, terminal_environment,
                        multiplexer_mode, reuse_policy, repo_root, worktree_strategy, row_version
                    FROM local_launch_profiles
                    WHERE source_surface = ?1
                    ORDER BY updated_at DESC, rowid DESC
                    LIMIT 1",
                    params![query.source_surface.as_str()],
                    |row| {
                        Ok(LocalLaunchProfileRecord {
                            id: row.get(0)?,
                            name: row.get(1)?,
                            source_surface: row.get(2)?,
                            ui_target: row.get(3)?,
                            terminal_environment: row.get(4)?,
                            multiplexer_mode: row.get(5)?,
                            reuse_policy: row.get(6)?,
                            repo_root: row.get(7)?,
                            worktree_strategy: row.get(8)?,
                            row_version: row.get(9)?,
                        })
                    },
                )
                .optional()?
        };

        let Some(profile) = selected else {
            return Ok(LaunchProfileRouteResolution::NotFound {
                reason: "no matching launch profile found for routing request".to_owned(),
            });
        };

        let mut degraded_reasons = Vec::new();

        let terminal_environment = if query.available_terminal_environments.is_empty()
            || query
                .available_terminal_environments
                .contains(&profile.terminal_environment)
        {
            profile.terminal_environment.clone()
        } else {
            degraded_reasons.push(format!(
                "requested terminal environment {} unavailable",
                profile.terminal_environment
            ));
            query.available_terminal_environments[0].clone()
        };

        let multiplexer_mode = if query.available_multiplexer_modes.is_empty()
            || query
                .available_multiplexer_modes
                .contains(&profile.multiplexer_mode)
        {
            profile.multiplexer_mode.clone()
        } else if query
            .available_multiplexer_modes
            .iter()
            .any(|mode| mode == "none")
        {
            degraded_reasons.push(format!(
                "requested multiplexer mode {} unavailable",
                profile.multiplexer_mode
            ));
            "none".to_owned()
        } else {
            degraded_reasons.push(format!(
                "requested multiplexer mode {} unavailable",
                profile.multiplexer_mode
            ));
            query.available_multiplexer_modes[0].clone()
        };

        let reason = if degraded_reasons.is_empty() {
            None
        } else {
            Some(degraded_reasons.join("; "))
        };

        Ok(LaunchProfileRouteResolution::Resolved(
            LaunchProfileRouteDecision {
                profile_id: profile.id,
                source_surface: profile.source_surface,
                ui_target: profile.ui_target,
                terminal_environment,
                multiplexer_mode,
                reuse_policy: profile.reuse_policy,
                degraded: reason.is_some(),
                reason,
            },
        ))
    }

    pub fn put_session_launch_binding(
        &self,
        binding: CreateSessionLaunchBinding<'_>,
    ) -> Result<()> {
        let now = now_ts();
        self.conn.execute(
            "INSERT INTO session_launch_bindings (
                id, session_id, repo_locator, review_target, surface, launch_profile_id,
                ui_target, instance_preference, cwd, worktree_root,
                row_version, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0, ?11, ?11)
            ON CONFLICT(id) DO UPDATE SET
                repo_locator = excluded.repo_locator,
                review_target = excluded.review_target,
                surface = excluded.surface,
                launch_profile_id = excluded.launch_profile_id,
                ui_target = excluded.ui_target,
                instance_preference = excluded.instance_preference,
                cwd = excluded.cwd,
                worktree_root = excluded.worktree_root,
                row_version = session_launch_bindings.row_version + 1,
                updated_at = excluded.updated_at",
            params![
                binding.id,
                binding.session_id,
                binding.repo_locator,
                binding
                    .review_target
                    .map(serde_json::to_string)
                    .transpose()?,
                binding.surface.as_str(),
                binding.launch_profile_id,
                binding.ui_target,
                binding.instance_preference,
                binding.cwd,
                binding.worktree_root,
                now
            ],
        )?;
        Ok(())
    }

    pub fn put_launch_preflight_plan(&self, plan: CreateLaunchPreflightPlan<'_>) -> Result<()> {
        let now = now_ts();
        self.conn.execute(
            "INSERT INTO launch_preflight_plans (
                id, session_id, launch_binding_id, result_class, selected_mode,
                resource_decisions_json, required_operator_actions_json,
                row_version, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8, ?8)
            ON CONFLICT(id) DO UPDATE SET
                session_id = excluded.session_id,
                launch_binding_id = excluded.launch_binding_id,
                result_class = excluded.result_class,
                selected_mode = excluded.selected_mode,
                resource_decisions_json = excluded.resource_decisions_json,
                required_operator_actions_json = excluded.required_operator_actions_json,
                row_version = launch_preflight_plans.row_version + 1,
                updated_at = excluded.updated_at",
            params![
                plan.id,
                plan.session_id,
                plan.launch_binding_id,
                plan.result_class.as_str(),
                plan.selected_mode.as_str(),
                serde_json::to_string(plan.resource_decisions)?,
                serde_json::to_string(plan.required_operator_actions)?,
                now
            ],
        )?;
        Ok(())
    }

    pub fn latest_launch_preflight_plan(
        &self,
        query: ResolveLaunchPreflightPlan<'_>,
    ) -> Result<Option<LaunchPreflightPlanRecord>> {
        let parse_record =
            |row: &rusqlite::Row<'_>| -> rusqlite::Result<LaunchPreflightPlanRecord> {
                let result_class_raw: String = row.get(3)?;
                let result_class = LaunchPreflightResultClass::parse(&result_class_raw).ok_or_else(
                || {
                    rusqlite::Error::FromSqlConversionFailure(
                        3,
                        rusqlite::types::Type::Text,
                        Box::new(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!(
                                "invalid launch_preflight_plans.result_class value: {result_class_raw}"
                            ),
                        )),
                    )
                },
            )?;

                let selected_mode_raw: String = row.get(4)?;
                let selected_mode = LaunchPreflightMode::parse(&selected_mode_raw).ok_or_else(|| {
                rusqlite::Error::FromSqlConversionFailure(
                    4,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!(
                            "invalid launch_preflight_plans.selected_mode value: {selected_mode_raw}"
                        ),
                    )),
                )
            })?;

                let resource_decisions_json: String = row.get(5)?;
                let resource_decisions: LaunchPreflightResourceDecisions =
                    serde_json::from_str(&resource_decisions_json).map_err(|err| {
                        rusqlite::Error::FromSqlConversionFailure(
                            5,
                            rusqlite::types::Type::Text,
                            Box::new(err),
                        )
                    })?;

                let required_actions_json: String = row.get(6)?;
                let required_operator_actions: Vec<String> =
                    serde_json::from_str(&required_actions_json).map_err(|err| {
                        rusqlite::Error::FromSqlConversionFailure(
                            6,
                            rusqlite::types::Type::Text,
                            Box::new(err),
                        )
                    })?;

                Ok(LaunchPreflightPlanRecord {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    launch_binding_id: row.get(2)?,
                    result_class,
                    selected_mode,
                    resource_decisions,
                    required_operator_actions,
                    row_version: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                })
            };

        let selected = if let Some(launch_binding_id) = query.launch_binding_id {
            self.conn
                .query_row(
                    "SELECT id, session_id, launch_binding_id, result_class, selected_mode,
                        resource_decisions_json, required_operator_actions_json,
                        row_version, created_at, updated_at
                    FROM launch_preflight_plans
                    WHERE session_id = ?1 AND launch_binding_id = ?2
                    ORDER BY updated_at DESC, rowid DESC
                    LIMIT 1",
                    params![query.session_id, launch_binding_id],
                    parse_record,
                )
                .optional()?
        } else {
            self.conn
                .query_row(
                    "SELECT id, session_id, launch_binding_id, result_class, selected_mode,
                        resource_decisions_json, required_operator_actions_json,
                        row_version, created_at, updated_at
                    FROM launch_preflight_plans
                    WHERE session_id = ?1
                    ORDER BY updated_at DESC, rowid DESC
                    LIMIT 1",
                    params![query.session_id],
                    parse_record,
                )
                .optional()?
        };

        Ok(selected)
    }

    pub fn upsert_index_state(&self, update: UpdateIndexState<'_>) -> Result<()> {
        let now = now_ts();
        self.conn.execute(
            "INSERT INTO index_states (
                scope_key, generation, status, artifact_digest, row_version, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, 0, ?5, ?5)
            ON CONFLICT(scope_key) DO UPDATE SET
                generation = excluded.generation,
                status = excluded.status,
                artifact_digest = excluded.artifact_digest,
                row_version = index_states.row_version + 1,
                updated_at = excluded.updated_at",
            params![
                update.scope_key,
                update.generation,
                update.status,
                update.artifact_digest,
                now
            ],
        )?;
        Ok(())
    }

    pub fn upsert_memory_item(&self, item: UpsertMemoryItem<'_>) -> Result<MemoryItemRecord> {
        let now = now_ts();
        self.conn.execute(
            "INSERT INTO memory_items (
                id, scope_key, memory_class, state, statement, normalized_key,
                anchor_digest, source_kind, row_version, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, ?9, ?9)
            ON CONFLICT(id) DO UPDATE SET
                scope_key = excluded.scope_key,
                memory_class = excluded.memory_class,
                state = excluded.state,
                statement = excluded.statement,
                normalized_key = excluded.normalized_key,
                anchor_digest = excluded.anchor_digest,
                source_kind = excluded.source_kind,
                row_version = memory_items.row_version + 1,
                updated_at = excluded.updated_at",
            params![
                item.id,
                item.scope_key,
                item.memory_class,
                item.state,
                item.statement,
                item.normalized_key,
                item.anchor_digest,
                item.source_kind,
                now
            ],
        )?;

        self.memory_item(item.id)?
            .ok_or_else(|| StorageError::NotFound {
                entity: "memory_item",
                id: item.id.to_owned(),
            })
    }

    pub fn memory_item(&self, memory_id: &str) -> Result<Option<MemoryItemRecord>> {
        self.conn
            .query_row(
                "SELECT id, scope_key, memory_class, state, statement, normalized_key,
                    anchor_digest, source_kind, row_version, created_at, updated_at
                FROM memory_items
                WHERE id = ?1",
                params![memory_id],
                |row| {
                    Ok(MemoryItemRecord {
                        id: row.get(0)?,
                        scope_key: row.get(1)?,
                        memory_class: row.get(2)?,
                        state: row.get(3)?,
                        statement: row.get(4)?,
                        normalized_key: row.get(5)?,
                        anchor_digest: row.get(6)?,
                        source_kind: row.get(7)?,
                        row_version: row.get(8)?,
                        created_at: row.get(9)?,
                        updated_at: row.get(10)?,
                    })
                },
            )
            .optional()
            .map_err(StorageError::from)
    }

    pub fn prior_review_lookup(
        &self,
        query: PriorReviewLookupQuery<'_>,
    ) -> Result<PriorReviewLookupResult> {
        let scope_class = scope_class_for_key(query.scope_key);
        let scope_bucket = scope_bucket_for_class(scope_class).to_owned();
        let mut degraded_reasons = Vec::new();

        match scope_class {
            ScopeClass::Repo => {}
            ScopeClass::Project if !query.allow_project_scope => {
                degraded_reasons.push(
                    "project scope requested but overlays are disabled for this query".to_owned(),
                );
                return Ok(PriorReviewLookupResult {
                    scope_bucket,
                    mode: PriorReviewRetrievalMode::LexicalOnly,
                    degraded_reasons,
                    evidence_hits: Vec::new(),
                    promoted_memory: Vec::new(),
                    tentative_candidates: Vec::new(),
                });
            }
            ScopeClass::Org if !query.allow_org_scope => {
                degraded_reasons.push(
                    "org scope requested but overlays are disabled for this query".to_owned(),
                );
                return Ok(PriorReviewLookupResult {
                    scope_bucket,
                    mode: PriorReviewRetrievalMode::LexicalOnly,
                    degraded_reasons,
                    evidence_hits: Vec::new(),
                    promoted_memory: Vec::new(),
                    tentative_candidates: Vec::new(),
                });
            }
            ScopeClass::Unknown => degraded_reasons.push(
                "unknown scope key; defaulting to repo-first lexical retrieval only".to_owned(),
            ),
            _ => {}
        }

        let normalized_query = query.query_text.trim().to_ascii_lowercase();
        let lexical_query = normalized_query
            .split_whitespace()
            .next()
            .unwrap_or(normalized_query.as_str())
            .to_owned();
        let query_is_empty = lexical_query.is_empty();
        let limit = query.limit.clamp(1, 100);

        let lexical_ready = self
            .index_state(&format!("lexical:{}", query.scope_key))?
            .is_some_and(|state| state.status == "ready");
        if !lexical_ready {
            degraded_reasons.push(
                "lexical sidecar unavailable or stale; using canonical DB lexical scan".to_owned(),
            );
        }

        let semantic_index_ready = self
            .index_state(&format!("semantic:{}", query.scope_key))?
            .is_some_and(|state| state.status == "ready");

        if !query.semantic_assets_verified {
            degraded_reasons.push(
                "semantic assets are missing or unverified; using lexical-only retrieval"
                    .to_owned(),
            );
        }
        if !semantic_index_ready {
            degraded_reasons.push(
                "semantic sidecar generation unavailable; using lexical-only retrieval".to_owned(),
            );
        }
        if query.semantic_candidates.is_empty() {
            degraded_reasons.push(
                "semantic candidates were not provided by the semantic sidecar hook".to_owned(),
            );
        }

        let semantic_operational = query.semantic_assets_verified
            && semantic_index_ready
            && !query.semantic_candidates.is_empty();

        let mut evidence_hits =
            self.lookup_evidence_hits(query.repository, &lexical_query, query_is_empty, limit)?;
        let mut promoted_memory = self.lookup_memory_hits(
            query.scope_key,
            &lexical_query,
            query_is_empty,
            &["established", "proven"],
            limit,
        )?;
        let mut tentative_candidates = if query.include_tentative_candidates {
            self.lookup_memory_hits(
                query.scope_key,
                &lexical_query,
                query_is_empty,
                &["candidate"],
                limit,
            )?
        } else {
            Vec::new()
        };

        if semantic_operational {
            let evidence_semantic_scores = semantic_scores_for_target(
                &query.semantic_candidates,
                SemanticLookupTargetKind::EvidenceFinding,
            );
            let memory_semantic_scores = semantic_scores_for_target(
                &query.semantic_candidates,
                SemanticLookupTargetKind::MemoryItem,
            );

            for (finding_id, semantic_score_milli) in &evidence_semantic_scores {
                let exists = evidence_hits
                    .iter()
                    .any(|hit| hit.finding_id == *finding_id);
                if exists {
                    continue;
                }
                if let Some(mut hit) = self.evidence_hit_by_id(query.repository, finding_id)? {
                    hit.semantic_score_milli = *semantic_score_milli;
                    hit.fused_score = fused_score(hit.lexical_score, hit.semantic_score_milli);
                    evidence_hits.push(hit);
                }
            }

            for (memory_id, semantic_score_milli) in &memory_semantic_scores {
                let in_promoted = promoted_memory
                    .iter()
                    .any(|hit| hit.memory_id == *memory_id);
                let in_tentative = tentative_candidates
                    .iter()
                    .any(|hit| hit.memory_id == *memory_id);
                if in_promoted || in_tentative {
                    continue;
                }
                let Some(mut hit) = self.memory_hit_by_id(query.scope_key, memory_id)? else {
                    continue;
                };

                hit.semantic_score_milli = *semantic_score_milli;
                hit.fused_score = fused_score(hit.lexical_score, hit.semantic_score_milli);

                if hit.state == "candidate" && query.include_tentative_candidates {
                    tentative_candidates.push(hit);
                } else if hit.state == "established" || hit.state == "proven" {
                    promoted_memory.push(hit);
                }
            }

            for hit in &mut evidence_hits {
                hit.semantic_score_milli = *evidence_semantic_scores
                    .get(hit.finding_id.as_str())
                    .unwrap_or(&0);
                hit.fused_score = fused_score(hit.lexical_score, hit.semantic_score_milli);
            }
            for hit in &mut promoted_memory {
                hit.semantic_score_milli = *memory_semantic_scores
                    .get(hit.memory_id.as_str())
                    .unwrap_or(&0);
                hit.fused_score = fused_score(hit.lexical_score, hit.semantic_score_milli);
            }
            for hit in &mut tentative_candidates {
                hit.semantic_score_milli = *memory_semantic_scores
                    .get(hit.memory_id.as_str())
                    .unwrap_or(&0);
                hit.fused_score = fused_score(hit.lexical_score, hit.semantic_score_milli);
            }

            evidence_hits.sort_by(|left, right| {
                right
                    .fused_score
                    .cmp(&left.fused_score)
                    .then_with(|| right.lexical_score.cmp(&left.lexical_score))
                    .then_with(|| left.finding_id.cmp(&right.finding_id))
            });
            promoted_memory.sort_by(|left, right| {
                right
                    .fused_score
                    .cmp(&left.fused_score)
                    .then_with(|| right.lexical_score.cmp(&left.lexical_score))
                    .then_with(|| left.memory_id.cmp(&right.memory_id))
            });
            tentative_candidates.sort_by(|left, right| {
                right
                    .fused_score
                    .cmp(&left.fused_score)
                    .then_with(|| right.lexical_score.cmp(&left.lexical_score))
                    .then_with(|| left.memory_id.cmp(&right.memory_id))
            });
        } else {
            for hit in &mut evidence_hits {
                hit.semantic_score_milli = 0;
                hit.fused_score = fused_score(hit.lexical_score, 0);
            }
            for hit in &mut promoted_memory {
                hit.semantic_score_milli = 0;
                hit.fused_score = fused_score(hit.lexical_score, 0);
            }
            for hit in &mut tentative_candidates {
                hit.semantic_score_milli = 0;
                hit.fused_score = fused_score(hit.lexical_score, 0);
            }
        }

        if semantic_operational {
            degraded_reasons.retain(|reason| {
                !reason.contains("semantic assets")
                    && !reason.contains("semantic sidecar generation")
                    && !reason.contains("semantic candidates")
            });
        }

        Ok(PriorReviewLookupResult {
            scope_bucket,
            mode: if semantic_operational {
                PriorReviewRetrievalMode::Hybrid
            } else {
                PriorReviewRetrievalMode::LexicalOnly
            },
            degraded_reasons,
            evidence_hits,
            promoted_memory,
            tentative_candidates,
        })
    }

    pub fn update_review_session_attention(
        &self,
        session_id: &str,
        expected_row_version: i64,
        attention_state: &str,
    ) -> Result<ReviewSessionRecord> {
        let updated = self.conn.execute(
            "UPDATE review_sessions
            SET attention_state = ?1, row_version = row_version + 1, updated_at = ?2
            WHERE id = ?3 AND row_version = ?4",
            params![attention_state, now_ts(), session_id, expected_row_version],
        )?;

        if updated == 0 {
            let exists = self
                .conn
                .query_row(
                    "SELECT 1 FROM review_sessions WHERE id = ?1",
                    params![session_id],
                    |_| Ok(()),
                )
                .optional()?;

            return match exists {
                Some(_) => Err(StorageError::Conflict {
                    entity: "review_session",
                    id: session_id.to_owned(),
                }),
                None => Err(StorageError::NotFound {
                    entity: "review_session",
                    id: session_id.to_owned(),
                }),
            };
        }

        self.review_session(session_id)?
            .ok_or_else(|| StorageError::NotFound {
                entity: "review_session",
                id: session_id.to_owned(),
            })
    }

    pub fn find_sessions_by_target(
        &self,
        repository: &str,
        pull_request_number: u64,
    ) -> Result<Vec<ReviewSessionRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, review_target, attention_state, row_version
            , provider, session_locator, resume_bundle_artifact_id, continuity_state, launch_profile_id
            , created_at, updated_at
            FROM review_sessions
            WHERE json_extract(review_target, '$.repository') = ?1
              AND json_extract(review_target, '$.pull_request_number') = ?2
            ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map(params![repository, pull_request_number], |row| {
            Ok(ReviewSessionRecord {
                id: row.get(0)?,
                review_target: serde_json::from_str(&row.get::<_, String>(1)?).map_err(|err| {
                    rusqlite::Error::FromSqlConversionFailure(
                        1,
                        rusqlite::types::Type::Text,
                        Box::new(err),
                    )
                })?,
                attention_state: row.get(2)?,
                row_version: row.get(3)?,
                provider: row.get(4)?,
                session_locator: row
                    .get::<_, Option<String>>(5)?
                    .map(|json| {
                        serde_json::from_str(&json).map_err(|err| {
                            rusqlite::Error::FromSqlConversionFailure(
                                5,
                                rusqlite::types::Type::Text,
                                Box::new(err),
                            )
                        })
                    })
                    .transpose()?,
                resume_bundle_artifact_id: row.get(6)?,
                continuity_state: row.get(7)?,
                launch_profile_id: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        })?;

        let mut sessions = Vec::new();
        for row in rows {
            sessions.push(row?);
        }
        Ok(sessions)
    }

    pub fn update_review_session_continuity(
        &self,
        session_id: &str,
        expected_row_version: i64,
        continuity_state: &str,
    ) -> Result<ReviewSessionRecord> {
        let updated = self.conn.execute(
            "UPDATE review_sessions
            SET continuity_state = ?1, row_version = row_version + 1, updated_at = ?2
            WHERE id = ?3 AND row_version = ?4",
            params![continuity_state, now_ts(), session_id, expected_row_version],
        )?;

        if updated == 0 {
            let exists = self
                .conn
                .query_row(
                    "SELECT 1 FROM review_sessions WHERE id = ?1",
                    params![session_id],
                    |_| Ok(()),
                )
                .optional()?;

            return match exists {
                Some(_) => Err(StorageError::Conflict {
                    entity: "review_session",
                    id: session_id.to_owned(),
                }),
                None => Err(StorageError::NotFound {
                    entity: "review_session",
                    id: session_id.to_owned(),
                }),
            };
        }

        self.review_session(session_id)?
            .ok_or_else(|| StorageError::NotFound {
                entity: "review_session",
                id: session_id.to_owned(),
            })
    }

    pub fn review_session(&self, session_id: &str) -> Result<Option<ReviewSessionRecord>> {
        self.conn
            .query_row(
                "SELECT id, review_target, attention_state, row_version
                , provider, session_locator, resume_bundle_artifact_id, continuity_state, launch_profile_id
                , created_at, updated_at
                FROM review_sessions
                WHERE id = ?1",
                params![session_id],
                |row| {
                    Ok(ReviewSessionRecord {
                        id: row.get(0)?,
                        review_target: serde_json::from_str(&row.get::<_, String>(1)?).map_err(
                            |err| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    1,
                                    rusqlite::types::Type::Text,
                                    Box::new(err),
                                )
                            },
                        )?,
                        attention_state: row.get(2)?,
                        row_version: row.get(3)?,
                        provider: row.get(4)?,
                        session_locator: row
                            .get::<_, Option<String>>(5)?
                            .map(|json| {
                                serde_json::from_str(&json).map_err(|err| {
                                    rusqlite::Error::FromSqlConversionFailure(
                                        5,
                                        rusqlite::types::Type::Text,
                                        Box::new(err),
                                    )
                                })
                            })
                            .transpose()?,
                        resume_bundle_artifact_id: row.get(6)?,
                        continuity_state: row.get(7)?,
                        launch_profile_id: row.get(8)?,
                        created_at: row.get(9)?,
                        updated_at: row.get(10)?,
                    })
                },
            )
            .optional()
            .map_err(StorageError::from)
    }

    pub fn session_overview(&self, session_id: &str) -> Result<SessionOverview> {
        let session = self
            .review_session(session_id)?
            .ok_or_else(|| StorageError::NotFound {
                entity: "review_session",
                id: session_id.to_owned(),
            })?;

        let run_count = count_for_session(&self.conn, "review_runs", session_id)?;
        let finding_count = count_for_session(&self.conn, "findings", session_id)?;
        let draft_count = count_for_session(&self.conn, "outbound_drafts", session_id)?;
        let approval_count = self.conn.query_row(
            "SELECT COUNT(*) FROM outbound_approval_tokens oat
            JOIN outbound_drafts od ON od.id = oat.draft_id
            WHERE od.session_id = ?1",
            params![session_id],
            |row| row.get(0),
        )?;
        let posted_action_count = self.conn.query_row(
            "SELECT COUNT(*) FROM posted_actions pa
            JOIN outbound_drafts od ON od.id = pa.draft_id
            WHERE od.session_id = ?1",
            params![session_id],
            |row| row.get(0),
        )?;

        Ok(SessionOverview {
            attention_state: session.attention_state,
            row_version: session.row_version,
            run_count,
            finding_count,
            draft_count,
            approval_count,
            posted_action_count,
        })
    }

    pub fn latest_review_run(&self, session_id: &str) -> Result<Option<ReviewRunRecord>> {
        self.conn
            .query_row(
                "SELECT id, session_id, run_kind, repo_snapshot, continuity_quality,
                    session_locator_artifact_id, created_at
                FROM review_runs
                WHERE session_id = ?1
                ORDER BY created_at DESC, rowid DESC
                LIMIT 1",
                params![session_id],
                |row| {
                    Ok(ReviewRunRecord {
                        id: row.get(0)?,
                        session_id: row.get(1)?,
                        run_kind: row.get(2)?,
                        repo_snapshot: row.get(3)?,
                        continuity_quality: row.get(4)?,
                        session_locator_artifact_id: row.get(5)?,
                        created_at: row.get(6)?,
                    })
                },
            )
            .optional()
            .map_err(StorageError::from)
    }

    pub fn launch_bindings_for_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<SessionLaunchBindingRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, repo_locator, review_target, surface, launch_profile_id,
                ui_target, instance_preference, cwd, worktree_root, row_version
            FROM session_launch_bindings
            WHERE session_id = ?1
            ORDER BY updated_at ASC",
        )?;
        let rows = stmt.query_map(params![session_id], |row| {
            Ok(SessionLaunchBindingRecord {
                id: row.get(0)?,
                session_id: row.get(1)?,
                repo_locator: row.get(2)?,
                review_target: row
                    .get::<_, Option<String>>(3)?
                    .map(|json| {
                        serde_json::from_str(&json).map_err(|err| {
                            rusqlite::Error::FromSqlConversionFailure(
                                3,
                                rusqlite::types::Type::Text,
                                Box::new(err),
                            )
                        })
                    })
                    .transpose()?,
                surface: row.get(4)?,
                launch_profile_id: row.get(5)?,
                ui_target: row.get(6)?,
                instance_preference: row.get(7)?,
                cwd: row.get(8)?,
                worktree_root: row.get(9)?,
                row_version: row.get(10)?,
            })
        })?;

        let mut bindings = Vec::new();
        for row in rows {
            bindings.push(row?);
        }
        Ok(bindings)
    }

    pub fn resolve_session_launch_binding(
        &self,
        query: ResolveSessionLaunchBinding<'_>,
    ) -> Result<SessionBindingResolution> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, repo_locator, review_target, surface, launch_profile_id,
                ui_target, instance_preference, cwd, worktree_root, row_version
            FROM session_launch_bindings
            WHERE surface = ?1 AND repo_locator = ?2
            ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map(params![query.surface.as_str(), query.repo_locator], |row| {
            Ok(SessionLaunchBindingRecord {
                id: row.get(0)?,
                session_id: row.get(1)?,
                repo_locator: row.get(2)?,
                review_target: row
                    .get::<_, Option<String>>(3)?
                    .map(|json| {
                        serde_json::from_str(&json).map_err(|err| {
                            rusqlite::Error::FromSqlConversionFailure(
                                3,
                                rusqlite::types::Type::Text,
                                Box::new(err),
                            )
                        })
                    })
                    .transpose()?,
                surface: row.get(4)?,
                launch_profile_id: row.get(5)?,
                ui_target: row.get(6)?,
                instance_preference: row.get(7)?,
                cwd: row.get(8)?,
                worktree_root: row.get(9)?,
                row_version: row.get(10)?,
            })
        })?;

        let mut candidates = Vec::new();
        for row in rows {
            let record = row?;
            if let Some(ui_target) = query.ui_target {
                if record.ui_target.as_deref() != Some(ui_target) {
                    continue;
                }
            }
            if let Some(instance_preference) = query.instance_preference {
                if record.instance_preference.as_deref() != Some(instance_preference) {
                    continue;
                }
            }
            candidates.push(record);
        }

        if candidates.is_empty() {
            return Ok(SessionBindingResolution::NotFound);
        }

        if candidates.len() > 1 {
            let mut session_ids = candidates
                .into_iter()
                .map(|record| record.session_id)
                .collect::<Vec<_>>();
            session_ids.sort();
            session_ids.dedup();
            return Ok(SessionBindingResolution::Ambiguous { session_ids });
        }

        let binding = candidates.remove(0);
        if let Some(review_target) = query.review_target {
            match binding.review_target.as_ref() {
                Some(bound_target) if bound_target == review_target => {}
                Some(bound_target) => {
                    return Ok(SessionBindingResolution::Stale {
                        binding_id: binding.id,
                        reason: format!(
                            "binding target mismatch: expected {}#{}, found {}#{}",
                            review_target.repository,
                            review_target.pull_request_number,
                            bound_target.repository,
                            bound_target.pull_request_number
                        ),
                    });
                }
                None => {
                    return Ok(SessionBindingResolution::Stale {
                        binding_id: binding.id,
                        reason: "binding is missing durable review target state".to_owned(),
                    });
                }
            }
        }

        Ok(SessionBindingResolution::Resolved(binding))
    }

    pub fn resolve_resume_ledger(
        &self,
        query: ResolveSessionLaunchBinding<'_>,
        capability: ProviderContinuityCapability,
        outcome: ResumeAttemptOutcome,
    ) -> Result<ResumeLedgerResolution> {
        let binding = match self.resolve_session_launch_binding(query)? {
            SessionBindingResolution::Resolved(binding) => binding,
            SessionBindingResolution::NotFound => return Ok(ResumeLedgerResolution::NotFound),
            SessionBindingResolution::Ambiguous { session_ids } => {
                return Ok(ResumeLedgerResolution::Ambiguous { session_ids });
            }
            SessionBindingResolution::Stale { binding_id, reason } => {
                return Ok(ResumeLedgerResolution::Stale { binding_id, reason });
            }
        };

        let Some(session) = self.review_session(&binding.session_id)? else {
            return Ok(ResumeLedgerResolution::MissingSession {
                session_id: binding.session_id,
            });
        };

        let resume_bundle = match session.resume_bundle_artifact_id.as_deref() {
            Some(artifact_id) => Some(self.load_resume_bundle(artifact_id)?),
            None => None,
        };

        let decision = decide_resume_strategy(
            capability,
            &ResumeSessionState {
                locator_present: session.session_locator.is_some(),
                resume_bundle_present: resume_bundle.is_some(),
            },
            outcome,
        );

        Ok(ResumeLedgerResolution::Resolved(ResumeLedgerRecord {
            binding,
            session,
            resume_bundle,
            decision,
        }))
    }

    pub fn session_finder(&self, query: SessionFinderQuery) -> Result<Vec<SessionFinderEntry>> {
        let mut sql = String::from(
            "SELECT id, review_target, attention_state, provider, updated_at
            FROM review_sessions
            WHERE 1 = 1",
        );
        let mut values = Vec::<Value>::new();

        if let Some(repository) = query.repository {
            sql.push_str(" AND json_extract(review_target, '$.repository') = ?");
            sql.push_str(&(values.len() + 1).to_string());
            values.push(Value::Text(repository));
        }

        if let Some(pull_request_number) = query.pull_request_number {
            sql.push_str(" AND json_extract(review_target, '$.pull_request_number') = ?");
            sql.push_str(&(values.len() + 1).to_string());
            values.push(Value::Integer(pull_request_number as i64));
        }

        if !query.attention_states.is_empty() {
            sql.push_str(" AND attention_state IN (");
            let mut placeholders = Vec::with_capacity(query.attention_states.len());
            for attention_state in query.attention_states {
                placeholders.push(format!("?{}", values.len() + 1));
                values.push(Value::Text(attention_state));
            }
            sql.push_str(&placeholders.join(", "));
            sql.push(')');
        }

        sql.push_str(" ORDER BY updated_at DESC, rowid DESC");
        sql.push_str(" LIMIT ?");
        sql.push_str(&(values.len() + 1).to_string());
        let limit = if query.limit == 0 {
            25_i64
        } else {
            query.limit.min(250) as i64
        };
        values.push(Value::Integer(limit));

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(values), |row| {
            let review_target_json: String = row.get(1)?;
            let review_target: ReviewTarget =
                serde_json::from_str(&review_target_json).map_err(|err| {
                    rusqlite::Error::FromSqlConversionFailure(
                        1,
                        rusqlite::types::Type::Text,
                        Box::new(err),
                    )
                })?;

            Ok(SessionFinderEntry {
                session_id: row.get(0)?,
                repository: review_target.repository,
                pull_request_number: review_target.pull_request_number,
                attention_state: row.get(2)?,
                provider: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })?;

        let mut entries = Vec::new();
        for row in rows {
            entries.push(row?);
        }
        Ok(entries)
    }

    pub fn resolve_session_reentry(
        &self,
        query: ResolveSessionReentry,
    ) -> Result<SessionReentryResolution> {
        if let Some(explicit_session_id) = query.explicit_session_id.clone() {
            if let Some(session) = self.review_session(&explicit_session_id)? {
                return Ok(SessionReentryResolution::Resolved {
                    session,
                    binding: None,
                });
            }

            let candidates = self.session_finder(SessionFinderQuery {
                repository: query.repository.clone(),
                pull_request_number: query.pull_request_number,
                attention_states: Vec::new(),
                limit: 25,
            })?;
            return Ok(SessionReentryResolution::PickerRequired {
                reason: format!("explicit session id not found: {explicit_session_id}"),
                candidates,
            });
        }

        if let (Some(repository), Some(pull_request_number)) =
            (query.repository.as_deref(), query.pull_request_number)
        {
            let sessions = self.find_sessions_by_target(repository, pull_request_number)?;
            if sessions.len() == 1 {
                let session = sessions[0].clone();
                let binding_resolution =
                    self.resolve_session_launch_binding(ResolveSessionLaunchBinding {
                        surface: query.source_surface,
                        repo_locator: repository,
                        review_target: Some(&session.review_target),
                        ui_target: query.ui_target.as_deref(),
                        instance_preference: query.instance_preference.as_deref(),
                    })?;
                return match binding_resolution {
                    SessionBindingResolution::Resolved(binding) => {
                        Ok(SessionReentryResolution::Resolved {
                            session,
                            binding: Some(binding),
                        })
                    }
                    SessionBindingResolution::NotFound => Ok(SessionReentryResolution::Resolved {
                        session,
                        binding: None,
                    }),
                    SessionBindingResolution::Ambiguous { .. } => {
                        Ok(SessionReentryResolution::PickerRequired {
                            reason: "multiple launch bindings matched this target".to_owned(),
                            candidates: sessions.iter().map(session_entry_from_record).collect(),
                        })
                    }
                    SessionBindingResolution::Stale { reason, .. } => {
                        Ok(SessionReentryResolution::PickerRequired {
                            reason: format!("launch binding is stale: {reason}"),
                            candidates: sessions.iter().map(session_entry_from_record).collect(),
                        })
                    }
                };
            }

            let candidates = if sessions.is_empty() {
                self.session_finder(SessionFinderQuery {
                    repository: Some(repository.to_owned()),
                    pull_request_number: None,
                    attention_states: Vec::new(),
                    limit: 25,
                })?
            } else {
                sessions.iter().map(session_entry_from_record).collect()
            };

            let reason = if candidates.is_empty() {
                "no matching repo-local session found".to_owned()
            } else {
                "ambiguous repo-local session match; picker required".to_owned()
            };

            return Ok(SessionReentryResolution::PickerRequired { reason, candidates });
        }

        if let Some(repository) = query.repository.clone() {
            let candidates = self.session_finder(SessionFinderQuery {
                repository: Some(repository),
                pull_request_number: None,
                attention_states: Vec::new(),
                limit: 25,
            })?;

            if candidates.len() == 1 {
                let session_id = candidates[0].session_id.clone();
                let session =
                    self.review_session(&session_id)?
                        .ok_or_else(|| StorageError::NotFound {
                            entity: "review_session",
                            id: session_id,
                        })?;
                return Ok(SessionReentryResolution::Resolved {
                    session,
                    binding: None,
                });
            }

            let reason = if candidates.is_empty() {
                "no repo-local sessions found; open session picker".to_owned()
            } else {
                "multiple repo-local sessions found; open session picker".to_owned()
            };
            return Ok(SessionReentryResolution::PickerRequired { reason, candidates });
        }

        let candidates = self.session_finder(SessionFinderQuery {
            repository: None,
            pull_request_number: None,
            attention_states: Vec::new(),
            limit: 25,
        })?;
        Ok(SessionReentryResolution::PickerRequired {
            reason: "global session finder required for cross-repo re-entry".to_owned(),
            candidates,
        })
    }

    pub fn approval_for_draft(&self, draft_id: &str) -> Result<Option<ApprovalRecord>> {
        self.conn
            .query_row(
                "SELECT id, draft_id, payload_digest, target_locator, row_version
                FROM outbound_approval_tokens
                WHERE draft_id = ?1",
                params![draft_id],
                |row| {
                    Ok(ApprovalRecord {
                        id: row.get(0)?,
                        draft_id: row.get(1)?,
                        payload_digest: row.get(2)?,
                        target_locator: row.get(3)?,
                        row_version: row.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(StorageError::from)
    }

    pub fn index_state(&self, scope_key: &str) -> Result<Option<IndexStateRecord>> {
        self.conn
            .query_row(
                "SELECT scope_key, generation, status, artifact_digest, row_version
                FROM index_states
                WHERE scope_key = ?1",
                params![scope_key],
                |row| {
                    Ok(IndexStateRecord {
                        scope_key: row.get(0)?,
                        generation: row.get(1)?,
                        status: row.get(2)?,
                        artifact_digest: row.get(3)?,
                        row_version: row.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(StorageError::from)
    }

    fn lookup_evidence_hits(
        &self,
        repository: &str,
        normalized_query: &str,
        query_is_empty: bool,
        limit: usize,
    ) -> Result<Vec<PriorReviewEvidenceHit>> {
        let mut stmt = self.conn.prepare(
            "SELECT
                f.id,
                f.session_id,
                COALESCE(f.last_seen_run_id, f.first_run_id) AS review_run_id,
                json_extract(rs.review_target, '$.repository') AS repository,
                CAST(json_extract(rs.review_target, '$.pull_request_number') AS INTEGER) AS pull_request_number,
                f.fingerprint,
                f.title,
                f.normalized_summary,
                f.severity,
                f.confidence,
                f.triage_state,
                f.outbound_state,
                (
                    CASE WHEN lower(f.fingerprint) = ?2 THEN 120 ELSE 0 END +
                    CASE WHEN instr(lower(f.fingerprint), ?2) > 0 THEN 60 ELSE 0 END +
                    CASE WHEN instr(lower(f.title), ?2) > 0 THEN 45 ELSE 0 END +
                    CASE WHEN instr(lower(f.normalized_summary), ?2) > 0 THEN 35 ELSE 0 END
                ) AS lexical_score
            FROM findings f
            JOIN review_sessions rs ON rs.id = f.session_id
            WHERE json_extract(rs.review_target, '$.repository') = ?1
              AND (
                ?3 = 1
                OR lower(f.fingerprint) = ?2
                OR instr(lower(f.fingerprint), ?2) > 0
                OR instr(lower(f.title), ?2) > 0
                OR instr(lower(f.normalized_summary), ?2) > 0
              )
            ORDER BY lexical_score DESC, rs.updated_at DESC, f.rowid DESC
            LIMIT ?4",
        )?;

        let rows = stmt.query_map(
            params![
                repository,
                normalized_query,
                if query_is_empty { 1 } else { 0 },
                limit as i64
            ],
            |row| {
                let pull_request_number = row.get::<_, i64>(4).unwrap_or_default().max(0) as u64;
                let lexical_score = row.get::<_, i64>(12).unwrap_or_default();
                Ok(PriorReviewEvidenceHit {
                    finding_id: row.get(0)?,
                    session_id: row.get(1)?,
                    review_run_id: row.get(2)?,
                    repository: row.get(3)?,
                    pull_request_number,
                    fingerprint: row.get(5)?,
                    title: row.get(6)?,
                    normalized_summary: row.get(7)?,
                    severity: row.get(8)?,
                    confidence: row.get(9)?,
                    triage_state: row.get(10)?,
                    outbound_state: row.get(11)?,
                    lexical_score,
                    semantic_score_milli: 0,
                    fused_score: fused_score(lexical_score, 0),
                })
            },
        )?;

        let mut hits = Vec::new();
        for row in rows {
            hits.push(row?);
        }
        Ok(hits)
    }

    fn lookup_memory_hits(
        &self,
        scope_key: &str,
        normalized_query: &str,
        query_is_empty: bool,
        states: &[&str],
        limit: usize,
    ) -> Result<Vec<PriorReviewMemoryHit>> {
        if states.is_empty() {
            return Ok(Vec::new());
        }

        let mut values = Vec::<Value>::new();
        values.push(Value::Text(scope_key.to_owned()));

        let mut state_placeholders = Vec::new();
        for state in states {
            values.push(Value::Text((*state).to_owned()));
            state_placeholders.push(format!("?{}", values.len()));
        }

        let query_index = if query_is_empty {
            None
        } else {
            values.push(Value::Text(normalized_query.to_owned()));
            Some(values.len())
        };

        let lexical_score_sql = if let Some(query_index) = query_index {
            format!(
                "(CASE WHEN lower(normalized_key) = ?{query_index} THEN 120 ELSE 0 END +
                  CASE WHEN instr(lower(normalized_key), ?{query_index}) > 0 THEN 55 ELSE 0 END +
                  CASE WHEN instr(lower(statement), ?{query_index}) > 0 THEN 40 ELSE 0 END +
                  CASE WHEN instr(lower(COALESCE(anchor_digest, '')), ?{query_index}) > 0 THEN 15 ELSE 0 END +
                  CASE state
                    WHEN 'proven' THEN 20
                    WHEN 'established' THEN 10
                    WHEN 'candidate' THEN 5
                    ELSE 0
                  END)"
            )
        } else {
            "0".to_owned()
        };

        let mut sql = format!(
            "SELECT
                id, scope_key, memory_class, state, statement, normalized_key,
                anchor_digest, source_kind, {lexical_score_sql} AS lexical_score
            FROM memory_items
            WHERE scope_key = ?1
              AND state IN ({})",
            state_placeholders.join(", ")
        );

        if let Some(query_index) = query_index {
            sql.push_str(&format!(
                " AND (
                    lower(normalized_key) = ?{query_index}
                    OR instr(lower(normalized_key), ?{query_index}) > 0
                    OR instr(lower(statement), ?{query_index}) > 0
                    OR instr(lower(COALESCE(anchor_digest, '')), ?{query_index}) > 0
                )"
            ));
        }

        values.push(Value::Integer(limit as i64));
        let limit_index = values.len();
        sql.push_str(&format!(
            " ORDER BY lexical_score DESC, updated_at DESC, rowid DESC
              LIMIT ?{limit_index}"
        ));

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(values), |row| {
            let lexical_score = row.get::<_, i64>(8).unwrap_or_default();
            Ok(PriorReviewMemoryHit {
                memory_id: row.get(0)?,
                scope_key: row.get(1)?,
                memory_class: row.get(2)?,
                state: row.get(3)?,
                statement: row.get(4)?,
                normalized_key: row.get(5)?,
                anchor_digest: row.get(6)?,
                source_kind: row.get(7)?,
                lexical_score,
                semantic_score_milli: 0,
                fused_score: fused_score(lexical_score, 0),
            })
        })?;

        let mut hits = Vec::new();
        for row in rows {
            hits.push(row?);
        }
        Ok(hits)
    }

    fn evidence_hit_by_id(
        &self,
        repository: &str,
        finding_id: &str,
    ) -> Result<Option<PriorReviewEvidenceHit>> {
        self.conn
            .query_row(
                "SELECT
                    f.id,
                    f.session_id,
                    COALESCE(f.last_seen_run_id, f.first_run_id) AS review_run_id,
                    json_extract(rs.review_target, '$.repository') AS repository,
                    CAST(json_extract(rs.review_target, '$.pull_request_number') AS INTEGER) AS pull_request_number,
                    f.fingerprint,
                    f.title,
                    f.normalized_summary,
                    f.severity,
                    f.confidence,
                    f.triage_state,
                    f.outbound_state
                FROM findings f
                JOIN review_sessions rs ON rs.id = f.session_id
                WHERE f.id = ?1
                  AND json_extract(rs.review_target, '$.repository') = ?2",
                params![finding_id, repository],
                |row| {
                    Ok(PriorReviewEvidenceHit {
                        finding_id: row.get(0)?,
                        session_id: row.get(1)?,
                        review_run_id: row.get(2)?,
                        repository: row.get(3)?,
                        pull_request_number: row.get::<_, i64>(4).unwrap_or_default().max(0)
                            as u64,
                        fingerprint: row.get(5)?,
                        title: row.get(6)?,
                        normalized_summary: row.get(7)?,
                        severity: row.get(8)?,
                        confidence: row.get(9)?,
                        triage_state: row.get(10)?,
                        outbound_state: row.get(11)?,
                        lexical_score: 0,
                        semantic_score_milli: 0,
                        fused_score: 0,
                    })
                },
            )
            .optional()
            .map_err(StorageError::from)
    }

    fn memory_hit_by_id(
        &self,
        scope_key: &str,
        memory_id: &str,
    ) -> Result<Option<PriorReviewMemoryHit>> {
        self.conn
            .query_row(
                "SELECT id, scope_key, memory_class, state, statement, normalized_key,
                    anchor_digest, source_kind
                FROM memory_items
                WHERE id = ?1
                  AND scope_key = ?2",
                params![memory_id, scope_key],
                |row| {
                    Ok(PriorReviewMemoryHit {
                        memory_id: row.get(0)?,
                        scope_key: row.get(1)?,
                        memory_class: row.get(2)?,
                        state: row.get(3)?,
                        statement: row.get(4)?,
                        normalized_key: row.get(5)?,
                        anchor_digest: row.get(6)?,
                        source_kind: row.get(7)?,
                        lexical_score: 0,
                        semantic_score_milli: 0,
                        fused_score: 0,
                    })
                },
            )
            .optional()
            .map_err(StorageError::from)
    }

    pub fn store_artifact(
        &self,
        artifact_id: &str,
        budget_class: ArtifactBudgetClass,
        mime_type: &str,
        bytes: &[u8],
    ) -> Result<StoredArtifact> {
        let policy = budget_class.policy();
        let digest = format!("{:x}", Sha256::digest(bytes));
        let storage_kind = policy.select_storage(bytes.len());
        let now = now_ts();

        let (inline_bytes, relative_path) = match storage_kind {
            ArtifactStorageKind::Inline => (Some(bytes.to_vec()), None),
            ArtifactStorageKind::ExternalContentAddressed | ArtifactStorageKind::DerivedSidecar => {
                let relative = artifact_relative_path(&digest);
                let absolute = match storage_kind {
                    ArtifactStorageKind::ExternalContentAddressed => {
                        self.layout.artifact_root.join(&relative)
                    }
                    ArtifactStorageKind::DerivedSidecar => self.layout.sidecar_root.join(&relative),
                    ArtifactStorageKind::Inline => unreachable!(),
                };
                if let Some(parent) = absolute.parent() {
                    fs::create_dir_all(parent)?;
                }
                if !absolute.exists() {
                    fs::write(&absolute, bytes)?;
                }
                (None, Some(relative))
            }
        };

        self.conn.execute(
            "INSERT INTO artifacts (
                id, digest, budget_class, storage_kind, mime_type,
                size_bytes, inline_bytes, relative_path, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                artifact_id,
                digest,
                budget_class.as_str(),
                storage_kind.as_str(),
                mime_type,
                bytes.len() as i64,
                inline_bytes.as_deref(),
                relative_path
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned()),
                now
            ],
        )?;

        Ok(StoredArtifact {
            id: artifact_id.to_owned(),
            digest,
            storage_kind,
            size_bytes: bytes.len(),
            inline_bytes,
            relative_path,
        })
    }

    pub fn artifact_bytes(&self, artifact_id: &str) -> Result<Vec<u8>> {
        let artifact = self.conn.query_row(
            "SELECT storage_kind, inline_bytes, relative_path
            FROM artifacts WHERE id = ?1",
            params![artifact_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<Vec<u8>>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        )?;

        match artifact.0.as_str() {
            "inline" => Ok(artifact.1.unwrap_or_default()),
            "external_content_addressed" => {
                let relative = artifact.2.ok_or_else(|| StorageError::NotFound {
                    entity: "artifact_relative_path",
                    id: artifact_id.to_owned(),
                })?;
                Ok(fs::read(self.layout.artifact_root.join(relative))?)
            }
            "derived_sidecar" => {
                let relative = artifact.2.ok_or_else(|| StorageError::NotFound {
                    entity: "artifact_relative_path",
                    id: artifact_id.to_owned(),
                })?;
                Ok(fs::read(self.layout.sidecar_root.join(relative))?)
            }
            other => Err(StorageError::NotFound {
                entity: "artifact_storage_kind",
                id: other.to_owned(),
            }),
        }
    }

    pub fn store_resume_bundle(
        &self,
        artifact_id: &str,
        bundle: &ResumeBundle,
    ) -> Result<StoredArtifact> {
        let payload = serde_json::to_vec(bundle)?;
        self.store_artifact(
            artifact_id,
            ArtifactBudgetClass::EvidenceExcerpt,
            "application/json",
            &payload,
        )
    }

    pub fn load_resume_bundle(&self, artifact_id: &str) -> Result<ResumeBundle> {
        let bytes = self.artifact_bytes(artifact_id)?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    fn apply_migrations(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                applied_at INTEGER NOT NULL
            );",
        )?;

        let version = self.schema_version()?;
        if version >= CURRENT_SCHEMA_VERSION {
            return Ok(());
        }

        if version < 1 {
            self.conn.execute_batch(MIGRATION_0001)?;
            self.conn.execute(
                "INSERT INTO schema_migrations(version, name, applied_at)
                VALUES (1, 'initial_storage_foundation', ?1)",
                params![now_ts()],
            )?;
            self.conn.pragma_update(None, "user_version", 1)?;
        }

        if version < 2 {
            self.conn.execute_batch(MIGRATION_0002)?;
            self.conn.execute(
                "INSERT INTO schema_migrations(version, name, applied_at)
                VALUES (2, 'session_ledger_foundation', ?1)",
                params![now_ts()],
            )?;
            self.conn.pragma_update(None, "user_version", 2)?;
        }

        if version < 3 {
            self.conn.execute_batch(MIGRATION_0003)?;
            self.conn.execute(
                "INSERT INTO schema_migrations(version, name, applied_at)
                VALUES (3, 'launch_binding_context', ?1)",
                params![now_ts()],
            )?;
            self.conn.pragma_update(None, "user_version", 3)?;
        }

        if version < 4 {
            self.conn.execute_batch(MIGRATION_0004)?;
            self.conn.execute(
                "INSERT INTO schema_migrations(version, name, applied_at)
                VALUES (4, 'launch_profile_routing', ?1)",
                params![now_ts()],
            )?;
            self.conn.pragma_update(None, "user_version", 4)?;
        }

        if version < 5 {
            self.conn.execute_batch(MIGRATION_0005)?;
            self.conn.execute(
                "INSERT INTO schema_migrations(version, name, applied_at)
                VALUES (5, 'prompt_invocation_and_outcome_events', ?1)",
                params![now_ts()],
            )?;
            self.conn.pragma_update(None, "user_version", 5)?;
        }

        if version < 6 {
            self.conn.execute_batch(MIGRATION_0006)?;
            self.conn.execute(
                "INSERT INTO schema_migrations(version, name, applied_at)
                VALUES (6, 'finding_materialization_with_provenance', ?1)",
                params![now_ts()],
            )?;
            self.conn.pragma_update(None, "user_version", 6)?;
        }

        if version < 7 {
            self.conn.execute_batch(MIGRATION_0007)?;
            self.conn.execute(
                "INSERT INTO schema_migrations(version, name, applied_at)
                VALUES (7, 'prior_review_lookup_memory_hooks', ?1)",
                params![now_ts()],
            )?;
            self.conn.pragma_update(None, "user_version", 7)?;
        }

        if version < 8 {
            self.conn.execute_batch(MIGRATION_0008)?;
            self.conn.execute(
                "INSERT INTO schema_migrations(version, name, applied_at)
                VALUES (8, 'launch_preflight_plan_persistence', ?1)",
                params![now_ts()],
            )?;
            self.conn.pragma_update(None, "user_version", 8)?;
        }

        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScopeClass {
    Repo,
    Project,
    Org,
    Unknown,
}

fn scope_class_for_key(scope_key: &str) -> ScopeClass {
    if scope_key.starts_with("repo:") {
        ScopeClass::Repo
    } else if scope_key.starts_with("project:") {
        ScopeClass::Project
    } else if scope_key.starts_with("org:") {
        ScopeClass::Org
    } else {
        ScopeClass::Unknown
    }
}

fn scope_bucket_for_class(scope_class: ScopeClass) -> &'static str {
    match scope_class {
        ScopeClass::Repo => "repo_memory",
        ScopeClass::Project => "project_overlay",
        ScopeClass::Org => "org_policy",
        ScopeClass::Unknown => "repo_memory",
    }
}

fn semantic_scores_for_target(
    candidates: &[SemanticLookupCandidate],
    target_kind: SemanticLookupTargetKind,
) -> HashMap<&str, i64> {
    let mut scores = HashMap::new();
    for candidate in candidates {
        if candidate.target_kind != target_kind {
            continue;
        }
        let score = semantic_score_to_milli(candidate.score);
        let entry = scores.entry(candidate.target_id.as_str()).or_insert(score);
        *entry = (*entry).max(score);
    }
    scores
}

fn semantic_score_to_milli(score: f32) -> i64 {
    let clamped = if score.is_finite() {
        score.clamp(0.0, 1.0)
    } else {
        0.0
    };
    (clamped * 1000.0).round() as i64
}

fn fused_score(lexical_score: i64, semantic_score_milli: i64) -> i64 {
    lexical_score
        .saturating_mul(10)
        .saturating_add(semantic_score_milli)
}

fn session_entry_from_record(record: &ReviewSessionRecord) -> SessionFinderEntry {
    SessionFinderEntry {
        session_id: record.id.clone(),
        repository: record.review_target.repository.clone(),
        pull_request_number: record.review_target.pull_request_number,
        attention_state: record.attention_state.clone(),
        provider: record.provider.clone(),
        updated_at: record.updated_at,
    }
}

fn count_for_session(conn: &Connection, table: &str, session_id: &str) -> Result<i64> {
    let sql = format!("SELECT COUNT(*) FROM {table} WHERE session_id = ?1");
    Ok(conn.query_row(&sql, params![session_id], |row| row.get(0))?)
}

fn artifact_relative_path(digest: &str) -> PathBuf {
    let (prefix, rest) = digest.split_at(2);
    PathBuf::from(prefix).join(format!("{rest}.bin"))
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifact_budget_classes_route_large_payloads_out_of_hot_tables() {
        assert_eq!(
            ArtifactBudgetClass::InlineSummary
                .policy()
                .select_storage(512),
            ArtifactStorageKind::Inline
        );
        assert_eq!(
            ArtifactBudgetClass::EvidenceExcerpt
                .policy()
                .select_storage(32 * 1024),
            ArtifactStorageKind::ExternalContentAddressed
        );
        assert_eq!(
            ArtifactBudgetClass::ColdArtifact.policy().select_storage(1),
            ArtifactStorageKind::ExternalContentAddressed
        );
        assert_eq!(
            ArtifactBudgetClass::DerivedIndexState
                .policy()
                .select_storage(1),
            ArtifactStorageKind::DerivedSidecar
        );
    }
}
