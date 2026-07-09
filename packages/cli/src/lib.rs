#![recursion_limit = "256"]

use base64::{Engine, engine::general_purpose::STANDARD as BASE64_STANDARD};
use roger_app_core::cli_config;
use roger_app_core::time;
use roger_app_core::{
    AGENT_TRANSPORT_REQUEST_SCHEMA_V1, AGENT_TRANSPORT_RESPONSE_SCHEMA_V1, AgentTransportErrorCode,
    AgentTransportRequestEnvelope, AgentTransportResponseEnvelope, AgentTransportResponseStatus,
    AppError, ContinuityQuality, ExplicitPostingOutcome, FindingTriageState, HarnessAdapter,
    LaunchAction, LaunchIntent, RecallSourceRef, ResumeAttemptOutcome, ResumeBundle,
    ResumeBundleProfile, ReviewTarget, ReviewTask, ReviewTaskKind, RogerCommand, RogerCommandId,
    RogerCommandInvocationSurface, RogerCommandResult, RogerCommandRouteStatus, SearchPlanError,
    SearchPlanInput, SearchQueryPlanError, SearchRetrievalClass, SessionBaselineSnapshot,
    SessionLocator, Surface, WORKER_OPERATION_REQUEST_SCHEMA_V1, WORKER_STAGE_RESULT_SCHEMA_V1,
    WorkerArtifactExcerpt, WorkerArtifactExcerptRequest, WorkerCapabilityProfile,
    WorkerContextPacket, WorkerEvidenceLocation, WorkerFindingDetail, WorkerFindingDetailRequest,
    WorkerFindingListResponse, WorkerFindingSummary, WorkerGatewaySnapshot, WorkerGitHubPosture,
    WorkerMemoryReviewRequest, WorkerMutationPosture, WorkerOperation,
    WorkerOperationRequestEnvelope, WorkerRecallEnvelope, WorkerSearchMemoryRequest,
    WorkerSearchMemoryResponse, WorkerStageOutcome, WorkerStageResult, WorkerStatusSnapshot,
    WorkerTransportKind, WorkerTurnStrategy, execute_agent_transport_request,
    materialize_search_plan, outbound_target_tuple_json, route_harness_command,
    safe_harness_command_bindings,
};
use roger_bridge::{
    BridgePreflight, NativeHostManifest, SupportedBrowser, SupportedOs,
    native_host_install_path_for,
};
use roger_config::cli_defaults::{
    DEFAULT_OPENCODE_BIN, ENV_COPILOT_BIN, ENV_OPENCODE_BIN, ENV_STORE_ROOT,
};
use roger_config::{ResolvedProviderCapability, ResolvedRoutineSurfaceBaseline};
use roger_github_adapter::{GhCliAdapter, GitHubAdapterError, ReadSafeGitHubAdapter};
use roger_session_claude::{ClaudeAdapter, ClaudeSessionPath};
use roger_session_codex::{CodexAdapter, CodexSessionPath};
use roger_session_copilot as session_copilot;
use roger_session_gemini::{GeminiAdapter, GeminiSessionPath};
use roger_session_opencode::{
    OpenCodeAdapter, OpenCodeReturnPath, OpenCodeSessionPath, rr_return_to_roger_session,
};
use roger_storage::{
    ClarificationRequestQuery, ClarificationSource, CreateClarificationRequest,
    CreateLaunchAttempt, CreateMaterializedFinding, CreateMemoryReviewRequest, CreateReviewRun,
    CreateReviewSession, CreateSessionLaunchBinding, CreateWorkerStageResult,
    FinalizeExistingSessionLaunchAttempt, FinalizeReviewLaunchAttempt, LaunchAttemptAction,
    LaunchAttemptState, LaunchSurface, MemoryReviewDecision, MemoryReviewRequestKind,
    MemoryReviewRequestRecord, MemoryReviewSource, OutboundSurfaceProjection,
    PriorReviewLookupQuery, PriorReviewRetrievalMode, ResolveSessionLaunchBinding,
    ResolveSessionLocalRoot, ResolveSessionReentry, ReviewLaunchFinalizationError,
    ReviewSessionRecord, RogerStore, SemanticAssetManifest, SemanticComponentState,
    SemanticEmbedderAdapter, SessionBindingResolution, SessionFinderEntry, SessionFinderQuery,
    SessionLaunchBindingRecord, SessionReentryResolution, StorageError, StorageLayout,
    UpdateLaunchAttempt, derive_finding_fingerprint, normalize_memory_key,
    semantic_embedder_status,
};
use rusqlite::{Connection as SqliteConnection, OpenFlags};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};
use std::result::Result;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock, mpsc};
use std::thread;
use std::time::{Duration, Instant};
use toon_format::encode_default as encode_toon_default;

static ID_SEQ: AtomicU64 = AtomicU64::new(1);
const SUPPORTED_REVIEW_PROVIDERS: [&str; 4] = ["opencode", "codex", "gemini", "claude"];
const PLANNED_REVIEW_PROVIDERS: [&str; 1] = [session_copilot::PROVIDER_ID];
const NOT_LIVE_REVIEW_PROVIDERS: [&str; 1] = ["pi-agent"];
const BRIDGE_UNINSTALL_REPAIR_ALIAS_WARNING: &str =
    "rr bridge uninstall is a repair alias; prefer rr extension uninstall";

#[derive(Clone, Debug)]
pub struct CliRuntime {
    pub cwd: PathBuf,
    pub store_root: PathBuf,
    pub opencode_bin: String,
}

impl CliRuntime {
    pub fn from_env(cwd: PathBuf) -> Self {
        let store_root = std::env::var(ENV_STORE_ROOT)
            .ok()
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                // One canonical store per profile: every surface (CLI from any
                // directory, browser-launched native host, TUI) must read the
                // same truth, so the default is HOME-based, not cwd-based.
                let home = std::env::var("HOME").ok();
                roger_config::cli_defaults::default_store_root_from(home.as_deref(), &cwd)
            });
        let opencode_bin =
            std::env::var(ENV_OPENCODE_BIN).unwrap_or_else(|_| DEFAULT_OPENCODE_BIN.to_owned());
        Self {
            cwd,
            store_root,
            opencode_bin,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliRunResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Clone, Debug)]
pub struct HarnessCommandInvocation {
    pub provider: String,
    pub command_id: RogerCommandId,
    pub repo: Option<String>,
    pub pr: Option<u64>,
    pub session_id: Option<String>,
    pub robot: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommandKind {
    Agent,
    Init,
    Doctor,
    Review,
    Resume,
    Return,
    Sessions,
    Prs,
    Search,
    Triage,
    Draft,
    Edit,
    Approve,
    Post,
    Update,
    Bridge,
    Extension,
    RobotDocs,
    Findings,
    Status,
    Tui,
    Assets,
    Memory,
    Timeline,
    Clarify,
}

impl CommandKind {
    fn as_rr_command(self, dry_run: bool) -> &'static str {
        match (self, dry_run) {
            (Self::Agent, _) => "rr agent",
            (Self::Init, _) => "rr init",
            (Self::Doctor, _) => "rr doctor",
            (Self::Review, true) => "rr review --dry-run",
            (Self::Resume, true) => "rr resume --dry-run",
            (Self::Review, false) => "rr review",
            (Self::Resume, false) => "rr resume",
            (Self::Return, _) => "rr return",
            (Self::Sessions, _) => "rr sessions",
            (Self::Prs, _) => "rr prs",
            (Self::Search, _) => "rr search",
            (Self::Triage, _) => "rr triage",
            (Self::Draft, _) => "rr draft",
            (Self::Edit, _) => "rr send edit",
            (Self::Approve, _) => "rr approve",
            (Self::Post, _) => "rr post",
            (Self::Update, _) => "rr update",
            (Self::Bridge, _) => "rr bridge",
            (Self::Extension, _) => "rr extension",
            (Self::RobotDocs, _) => "rr robot-docs",
            (Self::Findings, _) => "rr findings",
            (Self::Status, _) => "rr status",
            (Self::Tui, _) => "rr tui",
            (Self::Assets, _) => "rr assets",
            (Self::Memory, _) => "rr memory",
            (Self::Timeline, _) => "rr timeline",
            (Self::Clarify, _) => "rr clarify",
        }
    }

    fn schema_id(self) -> &'static str {
        match self {
            Self::Agent => "rr.agent.transport.v1",
            Self::Init => "rr.robot.init.v1",
            Self::Doctor => "rr.robot.doctor.v1",
            Self::Review => "rr.robot.review.v1",
            Self::Resume => "rr.robot.resume.v1",
            Self::Return => "rr.robot.return.v1",
            Self::Sessions => "rr.robot.sessions.v1",
            Self::Prs => "rr.robot.prs.v1",
            Self::Search => "rr.robot.search.v1",
            Self::Triage => "rr.robot.triage.v1",
            Self::Draft => "rr.robot.draft.v1",
            // `rr send edit` is a local, human-only editing action that rejects
            // --robot at parse time, so this id is never emitted in an
            // envelope. It is present only to keep the match exhaustive and is
            // deliberately not a `rr.robot.*` schema id (no new robot schema in
            // this slice).
            Self::Edit => "rr.send.edit.v1",
            Self::Approve => "rr.robot.approve.v1",
            Self::Post => "rr.robot.post.v1",
            Self::Update => "rr.robot.update.v1",
            Self::Bridge => "rr.robot.bridge.v1",
            Self::Extension => "rr.robot.extension.v1",
            Self::RobotDocs => "rr.robot.robot_docs.v1",
            Self::Findings => "rr.robot.findings.v1",
            Self::Status => "rr.robot.status.v1",
            Self::Tui => "rr.robot.tui.v1",
            Self::Assets => "rr.robot.assets.v1",
            Self::Memory => "rr.robot.memory.v1",
            Self::Timeline => "rr.robot.timeline.v1",
            Self::Clarify => "rr.robot.clarify.v1",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BridgeCommandKind {
    ExportContracts,
    VerifyContracts,
    PackExtension,
    Install,
    Uninstall,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExtensionCommandKind {
    Setup,
    Doctor,
    Fetch,
    Uninstall,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AssetsCommandKind {
    Install,
    Status,
    Verify,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MemoryCommandKind {
    Review,
    Accept,
    Reject,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RobotFormat {
    Json,
    Compact,
    Toon,
}

impl RobotFormat {
    fn as_str(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Compact => "compact",
            Self::Toon => "toon",
        }
    }
}

#[derive(Clone, Debug)]
struct ParsedArgs {
    command: CommandKind,
    agent_operation: Option<String>,
    agent_task_file: Option<PathBuf>,
    agent_request_file: Option<PathBuf>,
    // Inline base64 request payload for `rr agent` — a write-free submission
    // path for in-session workers whose policy denies file creation and shell
    // metacharacters (base64 text contains none of the denied characters).
    agent_request_b64: Option<String>,
    agent_context_file: Option<PathBuf>,
    agent_capability_file: Option<PathBuf>,
    bridge_command: Option<BridgeCommandKind>,
    extension_command: Option<ExtensionCommandKind>,
    assets_command: Option<AssetsCommandKind>,
    memory_command: Option<MemoryCommandKind>,
    extension_browser: Option<SupportedBrowser>,
    extension_package_dir: Option<PathBuf>,
    bridge_extension_id: Option<String>,
    bridge_binary_path: Option<PathBuf>,
    bridge_install_root: Option<PathBuf>,
    bridge_output_dir: Option<PathBuf>,
    repo: Option<String>,
    pr: Option<u64>,
    session_id: Option<String>,
    draft_finding_ids: Vec<String>,
    draft_all_findings: bool,
    triage_state: Option<String>,
    batch_id: Option<String>,
    // `rr send edit` inputs: the outbound draft item id, and exactly one body
    // source (a file, or an interactive editor seeded with the current body).
    edit_draft_id: Option<String>,
    edit_body_file: Option<PathBuf>,
    edit_editor: bool,
    update_channel: String,
    update_version: Option<String>,
    update_api_root: Option<String>,
    update_download_root: Option<String>,
    update_target: Option<String>,
    update_yes: bool,
    attention_states: Vec<String>,
    limit: Option<usize>,
    query_text: Option<String>,
    query_mode: Option<String>,
    assets_package_id: Option<String>,
    robot_docs_topic: Option<String>,
    robot: bool,
    robot_format: RobotFormat,
    dry_run: bool,
    provider: String,
    // `rr review --resume` flips the command to Resume; `rr findings --query`
    // and `rr findings --sessions` flip to Search/Sessions. These flags are
    // captured here so the flip and its foreign-flag rejection stay in
    // parse_args, keeping every downstream handler and check byte-identical to
    // the old direct invocations.
    resume_requested: bool,
    findings_sessions: bool,
    interactive: bool,
    // Explicit launch surface for review/resume/return. `None` defaults to the
    // CLI surface; the bridge passes `--surface bridge` when it spawns rr so the
    // recorded launch attempt and binding carry the true origin surface.
    surface: Option<LaunchSurface>,
    // Opt-in live handshake for `rr extension doctor --live`.
    live: bool,
    // `rr review --fresh` forces a brand-new session even when a non-terminal
    // session already exists for the repo/PR (default is reuse-or-new).
    fresh: bool,
    // `rr sessions --all` lists every matching session instead of the grouped,
    // most-recent-per-PR default view.
    show_all: bool,
    // `rr memory accept|reject --request <id>` names the durable
    // MemoryReviewRequest row to resolve.
    request_id: Option<String>,
    // `rr clarify --body <text>` supplies an inline clarification body (the
    // file-based alternative reuses --body-file).
    clarify_body: Option<String>,
    // `rr clarify --list` flips rr clarify from create to a read-only listing of
    // open clarifications.
    clarify_list: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OutcomeKind {
    Complete,
    Empty,
    Degraded,
    Blocked,
    RepairNeeded,
    Error,
}

impl OutcomeKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Empty => "empty",
            Self::Degraded => "degraded",
            Self::Blocked => "blocked",
            Self::RepairNeeded => "repair_needed",
            Self::Error => "error",
        }
    }

    fn exit_code(self) -> i32 {
        match self {
            Self::Complete | Self::Empty => 0,
            Self::Degraded => 5,
            Self::Blocked => 3,
            Self::RepairNeeded => 4,
            Self::Error => 1,
        }
    }
}

#[derive(Debug)]
struct CommandResponse {
    outcome: OutcomeKind,
    data: Value,
    warnings: Vec<String>,
    repair_actions: Vec<String>,
    message: String,
}

#[derive(Serialize)]
struct RobotEnvelope {
    schema_id: String,
    command: String,
    robot_format: String,
    outcome: String,
    generated_at: String,
    exit_code: i32,
    warnings: Vec<String>,
    repair_actions: Vec<String>,
    data: Value,
}

pub fn run(argv: &[String], runtime: &CliRuntime) -> CliRunResult {
    let parsed = match parse_args(argv) {
        Ok(parsed) => parsed,
        Err(message) if message == "help requested" => {
            return CliRunResult {
                exit_code: 0,
                stdout: format!("{}\n", usage_text()),
                stderr: String::new(),
            };
        }
        Err(message) if message.starts_with("help:") => {
            let topic = &message["help:".len()..];
            return CliRunResult {
                exit_code: 0,
                stdout: format!("{}\n", command_usage(topic)),
                stderr: String::new(),
            };
        }
        Err(message) => {
            return CliRunResult {
                exit_code: 2,
                stdout: String::new(),
                stderr: format!("{message}\n{}", usage_text()),
            };
        }
    };

    let response = execute_command(&parsed, runtime);
    render_output(&parsed, response)
}

pub fn run_harness_command(
    invocation: &HarnessCommandInvocation,
    runtime: &CliRuntime,
) -> CliRunResult {
    let mut args = HashMap::new();
    if let Some(repo) = invocation.repo.as_ref() {
        args.insert("repo".to_owned(), repo.clone());
    }
    if let Some(pr) = invocation.pr {
        args.insert("pr".to_owned(), pr.to_string());
    }
    if let Some(session_id) = invocation.session_id.as_ref() {
        args.insert("session".to_owned(), session_id.clone());
    }

    let routed = route_harness_command(
        &RogerCommand {
            command_id: invocation.command_id,
            review_session_id: invocation.session_id.clone(),
            review_run_id: None,
            args,
            invocation_surface: RogerCommandInvocationSurface::HarnessCommand,
            provider: invocation.provider.clone(),
        },
        &safe_harness_command_bindings(&invocation.provider),
    );

    if routed.status == RogerCommandRouteStatus::FallbackRequired {
        return render_harness_route_result(invocation, &routed, OutcomeKind::Blocked);
    }

    if invocation.command_id == RogerCommandId::RogerHelp {
        return render_harness_help(invocation, &routed);
    }

    let Some(subcommand) = harness_command_to_cli_subcommand(invocation.command_id) else {
        return render_harness_route_result(
            invocation,
            &RogerCommandResult {
                status: RogerCommandRouteStatus::FallbackRequired,
                user_message: format!(
                    "command '{}' has no canonical CLI mapping in this slice",
                    invocation.command_id.logical_id()
                ),
                next_action: roger_app_core::RogerCommandNextAction {
                    canonical_operation: "show_help".to_owned(),
                    fallback_cli_command: "rr help".to_owned(),
                    session_finder_hint: None,
                },
                session_binding: invocation.session_id.clone(),
            },
            OutcomeKind::Blocked,
        );
    };

    let mut argv = vec![subcommand.to_owned()];
    if let Some(repo) = invocation.repo.as_ref() {
        argv.push("--repo".to_owned());
        argv.push(repo.clone());
    }
    if let Some(pr) = invocation.pr {
        argv.push("--pr".to_owned());
        argv.push(pr.to_string());
    }
    if let Some(session_id) = invocation.session_id.as_ref() {
        argv.push("--session".to_owned());
        argv.push(session_id.clone());
    }
    if invocation.robot {
        argv.push("--robot".to_owned());
    }

    run(&argv, runtime)
}

fn harness_command_to_cli_subcommand(command_id: RogerCommandId) -> Option<&'static str> {
    match command_id {
        RogerCommandId::RogerStatus => Some("status"),
        RogerCommandId::RogerFindings => Some("findings"),
        RogerCommandId::RogerReturn => Some("return"),
        RogerCommandId::RogerHelp => None,
    }
}

fn render_harness_help(
    invocation: &HarnessCommandInvocation,
    routed: &RogerCommandResult,
) -> CliRunResult {
    let supported = safe_harness_command_bindings(&invocation.provider);
    let supported_commands: Vec<Value> = supported
        .iter()
        .map(|binding| {
            json!({
                "logical_id": binding.command_id.logical_id(),
                "provider_command_syntax": binding.provider_command_syntax,
                "fallback_cli_command": binding.command_id.fallback_cli_command(),
            })
        })
        .collect();

    if invocation.robot {
        return render_harness_robot_envelope(
            invocation,
            OutcomeKind::Complete,
            Vec::new(),
            Vec::new(),
            json!({
                "provider": invocation.provider,
                "command_id": invocation.command_id.logical_id(),
                "canonical_operation": routed.next_action.canonical_operation,
                "supported_commands": supported_commands,
            }),
        );
    }

    let mut stdout = String::new();
    stdout.push_str("Roger harness commands (safe subset):\n");
    for command in supported_commands {
        let logical = command
            .get("logical_id")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let syntax = command
            .get("provider_command_syntax")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let fallback = command
            .get("fallback_cli_command")
            .and_then(Value::as_str)
            .unwrap_or("rr help");
        stdout.push_str(&format!("- {logical}: {syntax} (fallback: {fallback})\n"));
    }

    CliRunResult {
        exit_code: 0,
        stdout,
        stderr: String::new(),
    }
}

fn render_harness_route_result(
    invocation: &HarnessCommandInvocation,
    routed: &RogerCommandResult,
    outcome: OutcomeKind,
) -> CliRunResult {
    let mut repair_actions = vec![format!("run {}", routed.next_action.fallback_cli_command)];
    if let Some(hint) = routed.next_action.session_finder_hint.clone() {
        repair_actions.push(hint);
    }

    if invocation.robot {
        return render_harness_robot_envelope(
            invocation,
            outcome,
            vec![routed.user_message.clone()],
            repair_actions,
            json!({
                "provider": invocation.provider,
                "command_id": invocation.command_id.logical_id(),
                "canonical_operation": routed.next_action.canonical_operation,
                "fallback_cli_command": routed.next_action.fallback_cli_command,
                "session_binding": routed.session_binding,
            }),
        );
    }

    let mut stdout = String::new();
    stdout.push_str(&routed.user_message);
    stdout.push('\n');
    stdout.push_str("Suggested next steps:\n");
    for action in repair_actions {
        stdout.push_str("- ");
        stdout.push_str(&action);
        stdout.push('\n');
    }

    CliRunResult {
        exit_code: outcome.exit_code(),
        stdout,
        stderr: String::new(),
    }
}

fn render_harness_robot_envelope(
    invocation: &HarnessCommandInvocation,
    outcome: OutcomeKind,
    warnings: Vec<String>,
    repair_actions: Vec<String>,
    data: Value,
) -> CliRunResult {
    let exit_code = outcome.exit_code();
    let envelope = RobotEnvelope {
        schema_id: "rr.robot.harness_command.v1".to_owned(),
        command: invocation.command_id.logical_id().to_owned(),
        robot_format: RobotFormat::Json.as_str().to_owned(),
        outcome: outcome.as_str().to_owned(),
        generated_at: time::now_ts().to_string(),
        exit_code,
        warnings: warnings.clone(),
        repair_actions,
        data,
    };

    let stdout = match serde_json::to_string_pretty(&envelope) {
        Ok(text) => format!("{text}\n"),
        Err(err) => {
            return CliRunResult {
                exit_code: 1,
                stdout: String::new(),
                stderr: format!("failed to serialize harness-command output: {err}\n"),
            };
        }
    };

    let stderr = if warnings.is_empty() {
        String::new()
    } else {
        format!("{}\n", warnings.join("\n"))
    };

    CliRunResult {
        exit_code,
        stdout,
        stderr,
    }
}

fn parse_args(raw_argv: &[String]) -> Result<ParsedArgs, String> {
    if raw_argv.is_empty() {
        return Err("missing command".to_owned());
    }

    // Global help: a bare help token in the first position prints the full
    // usage. Per-command help: `--help`/`-h` anywhere after a recognized
    // command (or container) prints a focused usage block for that command.
    // This fixes the old bug where `rr review --help` errored "unknown flag".
    match raw_argv[0].as_str() {
        "-h" | "--help" | "help" => return Err("help requested".to_owned()),
        _ => {}
    }
    if raw_argv[1..]
        .iter()
        .any(|arg| arg == "--help" || arg == "-h")
    {
        if let Some(topic) = help_topic_for(raw_argv) {
            return Err(format!("help:{topic}"));
        }
    }

    // Normalize the container verbs (`send`, `setup`, `api`) into the
    // underlying command's argv before the main parse. This is the cleanest
    // way to guarantee identical ParsedArgs, identical schema ids, and
    // identical checks: `send post ...` becomes `post ...`, `setup update ...`
    // becomes `update ...`, `api docs schemas` becomes `robot-docs schemas`.
    let normalized = normalize_container_argv(raw_argv)?;
    let argv = normalized.as_slice();

    let command = match argv[0].as_str() {
        "agent" => CommandKind::Agent,
        "init" => CommandKind::Init,
        "doctor" => CommandKind::Doctor,
        "review" => CommandKind::Review,
        "resume" => CommandKind::Resume,
        "return" => CommandKind::Return,
        "sessions" => CommandKind::Sessions,
        "prs" | "queue" => CommandKind::Prs,
        "search" => CommandKind::Search,
        "triage" => CommandKind::Triage,
        "draft" => CommandKind::Draft,
        "edit" => CommandKind::Edit,
        "approve" => CommandKind::Approve,
        "post" => CommandKind::Post,
        "update" => CommandKind::Update,
        "bridge" => CommandKind::Bridge,
        "extension" => CommandKind::Extension,
        "robot-docs" => CommandKind::RobotDocs,
        "findings" => CommandKind::Findings,
        "status" => CommandKind::Status,
        "tui" | "open" => CommandKind::Tui,
        "assets" => CommandKind::Assets,
        "memory" => CommandKind::Memory,
        "timeline" => CommandKind::Timeline,
        "clarify" => CommandKind::Clarify,
        "-h" | "--help" | "help" => {
            return Err("help requested".to_owned());
        }
        other => return Err(format!("unknown command: {other}")),
    };

    let mut parsed = ParsedArgs {
        command,
        agent_operation: None,
        agent_task_file: None,
        agent_request_file: None,
        agent_request_b64: None,
        agent_context_file: None,
        agent_capability_file: None,
        bridge_command: None,
        extension_command: None,
        assets_command: None,
        memory_command: None,
        extension_browser: None,
        extension_package_dir: None,
        bridge_extension_id: None,
        bridge_binary_path: None,
        bridge_install_root: None,
        bridge_output_dir: None,
        repo: None,
        pr: None,
        session_id: None,
        draft_finding_ids: Vec::new(),
        draft_all_findings: false,
        triage_state: None,
        batch_id: None,
        edit_draft_id: None,
        edit_body_file: None,
        edit_editor: false,
        update_channel: "stable".to_owned(),
        update_version: None,
        update_api_root: None,
        update_download_root: None,
        update_target: None,
        update_yes: false,
        attention_states: Vec::new(),
        limit: None,
        query_text: None,
        query_mode: None,
        assets_package_id: None,
        robot_docs_topic: None,
        robot: false,
        robot_format: RobotFormat::Json,
        dry_run: false,
        provider: "opencode".to_owned(),
        resume_requested: false,
        findings_sessions: false,
        interactive: false,
        surface: None,
        live: false,
        fresh: false,
        show_all: false,
        request_id: None,
        clarify_body: None,
        clarify_list: false,
    };

    let mut i = 1;
    while i < argv.len() {
        match argv[i].as_str() {
            "--repo" => {
                let value = argv
                    .get(i + 1)
                    .ok_or_else(|| "--repo requires a value".to_owned())?;
                parsed.repo = Some(value.clone());
                i += 2;
            }
            "--task-file" => {
                let value = argv
                    .get(i + 1)
                    .ok_or_else(|| "--task-file requires a value".to_owned())?;
                parsed.agent_task_file = Some(PathBuf::from(value));
                i += 2;
            }
            "--request-file" => {
                let value = argv
                    .get(i + 1)
                    .ok_or_else(|| "--request-file requires a value".to_owned())?;
                parsed.agent_request_file = Some(PathBuf::from(value));
                i += 2;
            }
            "--request-b64" => {
                let value = argv
                    .get(i + 1)
                    .ok_or_else(|| "--request-b64 requires a value".to_owned())?;
                parsed.agent_request_b64 = Some(value.clone());
                i += 2;
            }
            "--context-file" => {
                let value = argv
                    .get(i + 1)
                    .ok_or_else(|| "--context-file requires a value".to_owned())?;
                parsed.agent_context_file = Some(PathBuf::from(value));
                i += 2;
            }
            "--capability-file" => {
                let value = argv
                    .get(i + 1)
                    .ok_or_else(|| "--capability-file requires a value".to_owned())?;
                parsed.agent_capability_file = Some(PathBuf::from(value));
                i += 2;
            }
            "--pr" => {
                let value = argv
                    .get(i + 1)
                    .ok_or_else(|| "--pr requires a numeric value".to_owned())?;
                parsed.pr = Some(
                    value
                        .parse::<u64>()
                        .map_err(|_| format!("invalid --pr value: {value}"))?,
                );
                i += 2;
            }
            "--session" => {
                let value = argv
                    .get(i + 1)
                    .ok_or_else(|| "--session requires a value".to_owned())?;
                parsed.session_id = Some(value.clone());
                i += 2;
            }
            "--finding" => {
                let value = argv
                    .get(i + 1)
                    .ok_or_else(|| "--finding requires a value".to_owned())?;
                parsed.draft_finding_ids.push(value.clone());
                i += 2;
            }
            "--all-findings" => {
                parsed.draft_all_findings = true;
                i += 1;
            }
            "--state" => {
                let value = argv
                    .get(i + 1)
                    .ok_or_else(|| "--state requires a value".to_owned())?;
                parsed.triage_state = Some(value.clone());
                i += 2;
            }
            "--batch" => {
                let value = argv
                    .get(i + 1)
                    .filter(|candidate| !candidate.starts_with("--"))
                    .ok_or_else(|| {
                        "--batch requires a draft-batch id value, not a flag (did you forget the id?)"
                            .to_owned()
                    })?;
                parsed.batch_id = Some(value.clone());
                i += 2;
            }
            "--draft" => {
                let value = argv
                    .get(i + 1)
                    .filter(|candidate| !candidate.starts_with("--"))
                    .ok_or_else(|| {
                        "--draft requires an outbound draft id value, not a flag (did you forget the id?)"
                            .to_owned()
                    })?;
                parsed.edit_draft_id = Some(value.clone());
                i += 2;
            }
            "--body-file" => {
                let value = argv
                    .get(i + 1)
                    .ok_or_else(|| "--body-file requires a path value".to_owned())?;
                parsed.edit_body_file = Some(PathBuf::from(value));
                i += 2;
            }
            "--editor" => {
                parsed.edit_editor = true;
                i += 1;
            }
            "--channel" => {
                let value = argv
                    .get(i + 1)
                    .ok_or_else(|| "--channel requires a value".to_owned())?;
                parsed.update_channel = value.clone();
                i += 2;
            }
            "--version" => {
                let value = argv
                    .get(i + 1)
                    .ok_or_else(|| "--version requires a value".to_owned())?;
                parsed.update_version = Some(value.clone());
                i += 2;
            }
            "--api-root" => {
                let value = argv
                    .get(i + 1)
                    .ok_or_else(|| "--api-root requires a value".to_owned())?;
                parsed.update_api_root = Some(value.clone());
                i += 2;
            }
            "--download-root" => {
                let value = argv
                    .get(i + 1)
                    .ok_or_else(|| "--download-root requires a value".to_owned())?;
                parsed.update_download_root = Some(value.clone());
                i += 2;
            }
            "--target" => {
                let value = argv
                    .get(i + 1)
                    .ok_or_else(|| "--target requires a value".to_owned())?;
                parsed.update_target = Some(value.clone());
                i += 2;
            }
            "--yes" | "-y" => {
                parsed.update_yes = true;
                i += 1;
            }
            "--attention" => {
                let value = argv
                    .get(i + 1)
                    .ok_or_else(|| "--attention requires a comma-separated value".to_owned())?;
                let mut states = value
                    .split(',')
                    .map(str::trim)
                    .filter(|entry| !entry.is_empty())
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>();
                if states.is_empty() {
                    return Err("--attention requires at least one non-empty state".to_owned());
                }
                parsed.attention_states.append(&mut states);
                i += 2;
            }
            "--limit" => {
                let value = argv
                    .get(i + 1)
                    .ok_or_else(|| "--limit requires a numeric value".to_owned())?;
                let parsed_limit = value
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --limit value: {value}"))?;
                if parsed_limit == 0 {
                    return Err("--limit must be greater than zero".to_owned());
                }
                parsed.limit = Some(parsed_limit);
                i += 2;
            }
            "--query" => {
                let value = argv
                    .get(i + 1)
                    .ok_or_else(|| "--query requires a value".to_owned())?;
                parsed.query_text = Some(value.clone());
                i += 2;
            }
            "--query-mode" => {
                let value = argv
                    .get(i + 1)
                    .ok_or_else(|| "--query-mode requires a value".to_owned())?;
                parsed.query_mode = Some(value.clone());
                i += 2;
            }
            "--asset" => {
                let value = argv
                    .get(i + 1)
                    .ok_or_else(|| "--asset requires a value".to_owned())?;
                parsed.assets_package_id = Some(value.clone());
                i += 2;
            }
            "--request" => {
                let value = argv
                    .get(i + 1)
                    .filter(|candidate| !candidate.starts_with("--"))
                    .ok_or_else(|| {
                        "--request requires a memory-review request id value, not a flag (did you forget the id?)"
                            .to_owned()
                    })?;
                parsed.request_id = Some(value.clone());
                i += 2;
            }
            "--body" => {
                let value = argv
                    .get(i + 1)
                    .ok_or_else(|| "--body requires a value".to_owned())?;
                parsed.clarify_body = Some(value.clone());
                i += 2;
            }
            "--list" => {
                parsed.clarify_list = true;
                i += 1;
            }
            "--topic" => {
                let value = argv
                    .get(i + 1)
                    .ok_or_else(|| "--topic requires a value".to_owned())?;
                parsed.robot_docs_topic = Some(value.clone());
                i += 2;
            }
            "--provider" => {
                let value = argv
                    .get(i + 1)
                    .ok_or_else(|| "--provider requires a value".to_owned())?;
                parsed.provider = canonicalize_provider_arg(value);
                i += 2;
            }
            "--extension-id" => {
                let value = argv
                    .get(i + 1)
                    .ok_or_else(|| "--extension-id requires a value".to_owned())?;
                parsed.bridge_extension_id = Some(value.clone());
                i += 2;
            }
            "--bridge-binary" => {
                let value = argv
                    .get(i + 1)
                    .ok_or_else(|| "--bridge-binary requires a value".to_owned())?;
                parsed.bridge_binary_path = Some(PathBuf::from(value));
                i += 2;
            }
            "--install-root" => {
                let value = argv
                    .get(i + 1)
                    .ok_or_else(|| "--install-root requires a value".to_owned())?;
                parsed.bridge_install_root = Some(PathBuf::from(value));
                i += 2;
            }
            "--output-dir" => {
                let value = argv
                    .get(i + 1)
                    .ok_or_else(|| "--output-dir requires a value".to_owned())?;
                parsed.bridge_output_dir = Some(PathBuf::from(value));
                i += 2;
            }
            "--browser" => {
                let value = argv
                    .get(i + 1)
                    .ok_or_else(|| "--browser requires edge, chrome, or brave".to_owned())?;
                parsed.extension_browser = Some(parse_supported_browser(value)?);
                i += 2;
            }
            "--package-dir" => {
                let value = argv
                    .get(i + 1)
                    .ok_or_else(|| "--package-dir requires a value".to_owned())?;
                parsed.extension_package_dir = Some(PathBuf::from(value));
                i += 2;
            }
            "--robot" => {
                parsed.robot = true;
                i += 1;
            }
            "--robot-format" => {
                let value = argv
                    .get(i + 1)
                    .ok_or_else(|| "--robot-format requires json, compact, or toon".to_owned())?;
                parsed.robot_format = match value.as_str() {
                    "json" => RobotFormat::Json,
                    "compact" => RobotFormat::Compact,
                    "toon" => RobotFormat::Toon,
                    other => return Err(format!("unsupported --robot-format: {other}")),
                };
                i += 2;
            }
            "--dry-run" => {
                parsed.dry_run = true;
                i += 1;
            }
            "--resume" => {
                parsed.resume_requested = true;
                i += 1;
            }
            "--sessions" => {
                parsed.findings_sessions = true;
                i += 1;
            }
            "--interactive" => {
                parsed.interactive = true;
                i += 1;
            }
            "--surface" => {
                let value = argv.get(i + 1).ok_or_else(|| {
                    "--surface requires cli, tui, extension, or bridge".to_owned()
                })?;
                parsed.surface = Some(LaunchSurface::parse(value).ok_or_else(|| {
                    format!(
                        "unsupported --surface: {value} (expected cli, tui, extension, or bridge)"
                    )
                })?);
                i += 2;
            }
            "--live" => {
                parsed.live = true;
                i += 1;
            }
            "--fresh" => {
                parsed.fresh = true;
                i += 1;
            }
            "--all" => {
                parsed.show_all = true;
                i += 1;
            }
            positional => {
                if positional.starts_with('-') {
                    return Err(format!("unknown flag: {positional}"));
                }
                match parsed.command {
                    CommandKind::Agent if parsed.agent_operation.is_none() => {
                        parsed.agent_operation = Some(positional.to_owned());
                        i += 1;
                    }
                    CommandKind::Bridge if parsed.bridge_command.is_none() => {
                        parsed.bridge_command = match positional {
                            "export-contracts" => Some(BridgeCommandKind::ExportContracts),
                            "verify-contracts" => Some(BridgeCommandKind::VerifyContracts),
                            "pack-extension" => Some(BridgeCommandKind::PackExtension),
                            "install" => Some(BridgeCommandKind::Install),
                            "uninstall" => Some(BridgeCommandKind::Uninstall),
                            other => {
                                return Err(format!("unknown bridge subcommand: {other}"));
                            }
                        };
                        i += 1;
                    }
                    CommandKind::Extension if parsed.extension_command.is_none() => {
                        parsed.extension_command = match positional {
                            "setup" => Some(ExtensionCommandKind::Setup),
                            "doctor" => Some(ExtensionCommandKind::Doctor),
                            "fetch" => Some(ExtensionCommandKind::Fetch),
                            "uninstall" => Some(ExtensionCommandKind::Uninstall),
                            other => {
                                return Err(format!("unknown extension subcommand: {other}"));
                            }
                        };
                        i += 1;
                    }
                    CommandKind::RobotDocs if parsed.robot_docs_topic.is_none() => {
                        parsed.robot_docs_topic = Some(positional.to_owned());
                        i += 1;
                    }
                    CommandKind::Search if parsed.query_text.is_none() => {
                        parsed.query_text = Some(positional.to_owned());
                        i += 1;
                    }
                    CommandKind::Assets if parsed.assets_command.is_none() => {
                        parsed.assets_command = match positional {
                            "install" => Some(AssetsCommandKind::Install),
                            "status" => Some(AssetsCommandKind::Status),
                            "verify" => Some(AssetsCommandKind::Verify),
                            other => {
                                return Err(format!("unknown assets subcommand: {other}"));
                            }
                        };
                        i += 1;
                    }
                    CommandKind::Memory if parsed.memory_command.is_none() => {
                        parsed.memory_command = match positional {
                            "review" => Some(MemoryCommandKind::Review),
                            "accept" => Some(MemoryCommandKind::Accept),
                            "reject" => Some(MemoryCommandKind::Reject),
                            other => {
                                return Err(format!("unknown memory subcommand: {other}"));
                            }
                        };
                        i += 1;
                    }
                    _ => {
                        return Err(format!("unexpected positional argument: {positional}"));
                    }
                }
            }
        }
    }

    // Alias routing flips. These run before every downstream check so the
    // resulting command routes to the same handler, whitelist, and schema id
    // as the old direct invocation. `original_command` distinguishes how we
    // arrived here so the routing flags can be rejected on unrelated commands.
    let original_command = parsed.command;
    if parsed.resume_requested {
        if original_command == CommandKind::Review {
            parsed.command = CommandKind::Resume;
        } else {
            return Err("--resume is only supported by rr review".to_owned());
        }
    }
    if original_command == CommandKind::Findings {
        if parsed.query_text.is_some() && parsed.findings_sessions {
            return Err("rr findings supports either --query or --sessions, not both".to_owned());
        }
        if parsed.query_text.is_some() {
            parsed.command = CommandKind::Search;
        } else if parsed.findings_sessions {
            parsed.command = CommandKind::Sessions;
        }
    }
    if parsed.findings_sessions && original_command != CommandKind::Findings {
        return Err("--sessions is only supported by rr findings".to_owned());
    }
    if parsed.fresh && parsed.command != CommandKind::Review {
        return Err("--fresh is only supported by rr review".to_owned());
    }
    if parsed.show_all && parsed.command != CommandKind::Sessions {
        return Err("--all is only supported by rr sessions".to_owned());
    }

    match parsed.robot_format {
        RobotFormat::Compact
            if !matches!(
                parsed.command,
                CommandKind::Status
                    | CommandKind::Findings
                    | CommandKind::Sessions
                    | CommandKind::Prs
                    | CommandKind::Search
                    | CommandKind::RobotDocs
            ) =>
        {
            return Err(
                "compact format is only supported for status/findings/sessions/prs/search/robot-docs in this slice".to_owned(),
            );
        }
        RobotFormat::Toon
            if !matches!(parsed.command, CommandKind::Status | CommandKind::Findings) =>
        {
            return Err(
                "toon format is only supported for status/findings in this slice".to_owned(),
            );
        }
        _ => {}
    }

    if parsed.interactive {
        // --interactive hands the terminal to Copilot for a live session. It is
        // only valid for the launch-shaped commands, only for the feature-gated
        // Copilot provider, and never alongside --robot (which promises a
        // machine-readable envelope, not a terminal handoff).
        if !matches!(
            parsed.command,
            CommandKind::Review | CommandKind::Resume | CommandKind::Return
        ) {
            return Err(
                "--interactive is only supported by rr review, rr resume, and rr return".to_owned(),
            );
        }
        if parsed.robot {
            return Err(
                "--robot and --interactive cannot be combined; --interactive hands the terminal to Copilot instead of emitting a robot envelope".to_owned(),
            );
        }
        if parsed.provider != session_copilot::PROVIDER_ID {
            return Err("--interactive is only supported with --provider copilot".to_owned());
        }
        if !copilot_admission_gate_enabled() {
            return Err(
                "--interactive requires the Copilot provider gate; set RR_ENABLE_COPILOT_PROVIDER=1".to_owned(),
            );
        }
    }

    if parsed.command == CommandKind::Bridge && parsed.bridge_command.is_none() {
        return Err(
            "rr bridge requires a subcommand: export-contracts, verify-contracts, pack-extension, install, or uninstall".to_owned(),
        );
    }

    if parsed.command == CommandKind::Agent && parsed.agent_operation.is_none() {
        return Err("rr agent requires an operation name".to_owned());
    }

    if parsed.command == CommandKind::Extension && parsed.extension_command.is_none() {
        return Err(
            "rr extension requires a subcommand: setup, doctor, fetch, or uninstall".to_owned(),
        );
    }

    if parsed.command == CommandKind::Assets && parsed.assets_command.is_none() {
        return Err("rr assets requires a subcommand: install, status, or verify".to_owned());
    }

    if parsed.command == CommandKind::Memory && parsed.memory_command.is_none() {
        return Err("rr memory requires a subcommand: review, accept, or reject".to_owned());
    }

    if parsed.command != CommandKind::Assets && parsed.assets_package_id.is_some() {
        return Err("--asset is only supported by rr assets".to_owned());
    }

    if parsed.command != CommandKind::Extension && parsed.extension_package_dir.is_some() {
        return Err("--package-dir is only supported by rr extension".to_owned());
    }

    if parsed.command != CommandKind::Search && parsed.query_mode.is_some() {
        return Err("--query-mode is only supported by rr search".to_owned());
    }

    // rr search is corpus-scoped, not session-scoped: it never binds a review
    // session or PR target. Accepting --session/--pr silently (inert) would
    // break the deliberate per-command flag-gating discipline and mislead the
    // operator, so reject them as command-irrelevant for rr search.
    if parsed.command == CommandKind::Search && (parsed.session_id.is_some() || parsed.pr.is_some())
    {
        return Err(
            "--session/--pr are not valid for rr search; rr search is corpus-scoped and does not bind a review session or PR target".to_owned(),
        );
    }

    if !matches!(parsed.command, CommandKind::Bridge)
        && (parsed.bridge_extension_id.is_some()
            || parsed.bridge_binary_path.is_some()
            || parsed.bridge_output_dir.is_some())
    {
        return Err("--extension-id/--bridge-binary/--output-dir are bridge-only flags".to_owned());
    }

    if !matches!(parsed.command, CommandKind::Bridge | CommandKind::Extension)
        && parsed.bridge_install_root.is_some()
    {
        return Err("--install-root is only supported by rr bridge and rr extension".to_owned());
    }

    if !matches!(
        parsed.command,
        CommandKind::Draft | CommandKind::Triage | CommandKind::Clarify
    ) && (parsed.draft_all_findings || !parsed.draft_finding_ids.is_empty())
    {
        return Err(
            "--finding is only supported by rr draft, rr triage, and rr clarify; --all-findings only by rr draft and rr triage".to_owned(),
        );
    }
    if parsed.command == CommandKind::Clarify && parsed.draft_all_findings {
        return Err(
            "rr clarify does not support --all-findings; pass a single --finding <id>".to_owned(),
        );
    }

    if parsed.command != CommandKind::Triage && parsed.triage_state.is_some() {
        return Err("--state is only supported by rr triage".to_owned());
    }

    if !matches!(parsed.command, CommandKind::Approve | CommandKind::Post)
        && parsed.batch_id.is_some()
    {
        return Err("--batch is only supported by rr approve and rr post".to_owned());
    }

    if parsed.command != CommandKind::Edit && (parsed.edit_draft_id.is_some() || parsed.edit_editor)
    {
        return Err("--draft/--editor are only supported by rr send edit".to_owned());
    }
    // --body-file is shared by rr send edit (replacement draft body) and rr
    // clarify (clarification body from a file); every other command rejects it.
    if !matches!(parsed.command, CommandKind::Edit | CommandKind::Clarify)
        && parsed.edit_body_file.is_some()
    {
        return Err("--body-file is only supported by rr send edit and rr clarify".to_owned());
    }
    if parsed.command != CommandKind::Clarify
        && (parsed.clarify_body.is_some() || parsed.clarify_list)
    {
        return Err("--body/--list are only supported by rr clarify".to_owned());
    }
    if parsed.command != CommandKind::Memory && parsed.request_id.is_some() {
        return Err(
            "--request is only supported by rr memory accept and rr memory reject".to_owned(),
        );
    }

    if parsed.command != CommandKind::Extension && parsed.extension_browser.is_some() {
        return Err("--browser is only supported by rr extension".to_owned());
    }

    // --surface names the true launch origin; only the launch-attempt commands
    // (review/resume/return) record a surface-typed launch attempt and binding,
    // so keep it out of the other commands' positive flag whitelists.
    if !matches!(
        parsed.command,
        CommandKind::Review | CommandKind::Resume | CommandKind::Return
    ) && parsed.surface.is_some()
    {
        return Err(
            "--surface is only supported by rr review, rr resume, and rr return".to_owned(),
        );
    }

    // --live only extends rr extension doctor with a real native-host handshake.
    if parsed.live
        && !(parsed.command == CommandKind::Extension
            && parsed.extension_command == Some(ExtensionCommandKind::Doctor))
    {
        return Err("--live is only supported by rr extension doctor".to_owned());
    }

    if parsed.command != CommandKind::Agent
        && (parsed.agent_task_file.is_some()
            || parsed.agent_request_file.is_some()
            || parsed.agent_request_b64.is_some()
            || parsed.agent_context_file.is_some()
            || parsed.agent_capability_file.is_some())
    {
        return Err(
            "--task-file/--request-file/--request-b64/--context-file/--capability-file are only supported by rr agent"
                .to_owned(),
        );
    }
    if parsed.agent_request_file.is_some() && parsed.agent_request_b64.is_some() {
        return Err("--request-file and --request-b64 are mutually exclusive".to_owned());
    }

    let extension_fetch_scope = parsed.command == CommandKind::Extension
        && parsed.extension_command == Some(ExtensionCommandKind::Fetch);
    if parsed.command != CommandKind::Update
        && (parsed.update_channel != "stable"
            || parsed.update_api_root.is_some()
            || parsed.update_target.is_some()
            || parsed.update_yes
            || (!extension_fetch_scope
                && (parsed.update_version.is_some() || parsed.update_download_root.is_some())))
    {
        return Err(
            "--channel/--version/--api-root/--download-root/--target/--yes are update-only flags (--version/--download-root are also accepted by rr extension fetch)"
                .to_owned(),
        );
    }

    if parsed.command == CommandKind::Update {
        if !matches!(parsed.update_channel.as_str(), "stable" | "rc") {
            return Err(format!(
                "unsupported --channel: {} (expected stable or rc)",
                parsed.update_channel
            ));
        }
        if parsed.pr.is_some()
            || parsed.session_id.is_some()
            || !parsed.attention_states.is_empty()
            || parsed.limit.is_some()
            || parsed.query_text.is_some()
            || parsed.query_mode.is_some()
            || parsed.robot_docs_topic.is_some()
            || parsed.provider != "opencode"
            || parsed.bridge_command.is_some()
            || parsed.extension_command.is_some()
            || parsed.extension_browser.is_some()
            || parsed.bridge_extension_id.is_some()
            || parsed.bridge_binary_path.is_some()
            || parsed.bridge_install_root.is_some()
            || parsed.bridge_output_dir.is_some()
        {
            return Err(
                "rr update only supports --repo, --channel, --version, --api-root, --download-root, --target, --yes/-y, --dry-run, and --robot".to_owned(),
            );
        }
    }

    if parsed.command == CommandKind::Triage {
        if parsed.dry_run {
            return Err("rr triage does not support --dry-run in this slice".to_owned());
        }
        if parsed.draft_all_findings {
            return Err(
                "rr triage requires explicit --finding ids; --all-findings is not supported"
                    .to_owned(),
            );
        }
        // The missing-finding, missing-state, and unsupported-state checks live
        // in handle_triage so that, like rr draft/approve/post, a bad or missing
        // required argument emits a `--robot` blocked envelope instead of plain
        // text + exit 2. Only structural flag-shape errors stay here.
        if !parsed.attention_states.is_empty()
            || parsed.limit.is_some()
            || parsed.query_text.is_some()
            || parsed.query_mode.is_some()
            || parsed.robot_docs_topic.is_some()
            || parsed.provider != "opencode"
            || parsed.bridge_command.is_some()
            || parsed.extension_command.is_some()
            || parsed.extension_browser.is_some()
            || parsed.bridge_extension_id.is_some()
            || parsed.bridge_binary_path.is_some()
            || parsed.bridge_install_root.is_some()
            || parsed.bridge_output_dir.is_some()
            || parsed.update_channel != "stable"
            || parsed.update_version.is_some()
            || parsed.update_api_root.is_some()
            || parsed.update_download_root.is_some()
            || parsed.update_target.is_some()
            || parsed.update_yes
        {
            return Err(
                "rr triage only supports --repo, --pr, --session, --finding, --state, and --robot"
                    .to_owned(),
            );
        }
    }

    if parsed.command == CommandKind::Draft {
        if parsed.dry_run {
            return Err("rr draft does not support --dry-run in this slice".to_owned());
        }
        if parsed.draft_all_findings && !parsed.draft_finding_ids.is_empty() {
            return Err("--all-findings cannot be combined with --finding".to_owned());
        }
        if !parsed.attention_states.is_empty()
            || parsed.limit.is_some()
            || parsed.query_text.is_some()
            || parsed.query_mode.is_some()
            || parsed.robot_docs_topic.is_some()
            || parsed.provider != "opencode"
            || parsed.bridge_command.is_some()
            || parsed.extension_command.is_some()
            || parsed.extension_browser.is_some()
            || parsed.bridge_extension_id.is_some()
            || parsed.bridge_binary_path.is_some()
            || parsed.bridge_install_root.is_some()
            || parsed.bridge_output_dir.is_some()
            || parsed.update_channel != "stable"
            || parsed.update_version.is_some()
            || parsed.update_api_root.is_some()
            || parsed.update_download_root.is_some()
            || parsed.update_target.is_some()
            || parsed.update_yes
        {
            return Err(
                "rr draft only supports --repo, --pr, --session, --finding, --all-findings, and --robot".to_owned(),
            );
        }
    }

    if parsed.command == CommandKind::Edit {
        // `rr send edit` is a local, human-only editing action. Like rr agent,
        // it is not a --robot transport: it rejects --robot at parse time
        // rather than emitting a machine envelope (no new robot schema id in
        // this slice).
        if parsed.robot {
            return Err(
                "rr send edit is a local editing action and does not support --robot; omit --robot"
                    .to_owned(),
            );
        }
        if parsed.dry_run {
            return Err("rr send edit does not support --dry-run".to_owned());
        }
        if parsed.edit_draft_id.is_none() {
            return Err("rr send edit requires --draft <id>".to_owned());
        }
        match (parsed.edit_body_file.is_some(), parsed.edit_editor) {
            (true, true) => {
                return Err(
                    "rr send edit accepts either --body-file or --editor, not both".to_owned(),
                );
            }
            (false, false) => {
                return Err("rr send edit requires --body-file <path> or --editor".to_owned());
            }
            _ => {}
        }
        if parsed.repo.is_some()
            || parsed.pr.is_some()
            || parsed.session_id.is_some()
            || !parsed.attention_states.is_empty()
            || parsed.limit.is_some()
            || parsed.query_text.is_some()
            || parsed.query_mode.is_some()
            || parsed.robot_docs_topic.is_some()
            || parsed.provider != "opencode"
            || parsed.bridge_command.is_some()
            || parsed.extension_command.is_some()
            || parsed.extension_browser.is_some()
            || parsed.bridge_extension_id.is_some()
            || parsed.bridge_binary_path.is_some()
            || parsed.bridge_install_root.is_some()
            || parsed.bridge_output_dir.is_some()
            || parsed.update_channel != "stable"
            || parsed.update_version.is_some()
            || parsed.update_api_root.is_some()
            || parsed.update_download_root.is_some()
            || parsed.update_target.is_some()
            || parsed.update_yes
            || parsed.draft_all_findings
            || !parsed.draft_finding_ids.is_empty()
            || parsed.batch_id.is_some()
            || parsed.agent_operation.is_some()
            || parsed.agent_task_file.is_some()
            || parsed.agent_request_file.is_some()
            || parsed.agent_context_file.is_some()
            || parsed.agent_capability_file.is_some()
        {
            return Err("rr send edit only supports --draft, --body-file, and --editor".to_owned());
        }
    }

    if parsed.command == CommandKind::Approve {
        if parsed.dry_run {
            return Err("rr approve does not support --dry-run in this slice".to_owned());
        }
        if !parsed.attention_states.is_empty()
            || parsed.limit.is_some()
            || parsed.query_text.is_some()
            || parsed.query_mode.is_some()
            || parsed.robot_docs_topic.is_some()
            || parsed.provider != "opencode"
            || parsed.bridge_command.is_some()
            || parsed.extension_command.is_some()
            || parsed.extension_browser.is_some()
            || parsed.bridge_extension_id.is_some()
            || parsed.bridge_binary_path.is_some()
            || parsed.bridge_install_root.is_some()
            || parsed.bridge_output_dir.is_some()
            || parsed.update_channel != "stable"
            || parsed.update_version.is_some()
            || parsed.update_api_root.is_some()
            || parsed.update_download_root.is_some()
            || parsed.update_target.is_some()
            || parsed.update_yes
            || parsed.draft_all_findings
            || !parsed.draft_finding_ids.is_empty()
        {
            return Err(
                "rr approve only supports --repo, --pr, --session, --batch, and --robot".to_owned(),
            );
        }
    }

    if parsed.command == CommandKind::Post {
        if parsed.dry_run {
            return Err("rr post does not support --dry-run in this slice".to_owned());
        }
        if !parsed.attention_states.is_empty()
            || parsed.limit.is_some()
            || parsed.query_text.is_some()
            || parsed.query_mode.is_some()
            || parsed.robot_docs_topic.is_some()
            || parsed.provider != "opencode"
            || parsed.bridge_command.is_some()
            || parsed.extension_command.is_some()
            || parsed.extension_browser.is_some()
            || parsed.bridge_extension_id.is_some()
            || parsed.bridge_binary_path.is_some()
            || parsed.bridge_install_root.is_some()
            || parsed.bridge_output_dir.is_some()
            || parsed.update_channel != "stable"
            || parsed.update_version.is_some()
            || parsed.update_api_root.is_some()
            || parsed.update_download_root.is_some()
            || parsed.update_target.is_some()
            || parsed.update_yes
            || parsed.draft_all_findings
            || !parsed.draft_finding_ids.is_empty()
        {
            return Err(
                "rr post only supports --repo, --pr, --session, --batch, and --robot".to_owned(),
            );
        }
    }

    if parsed.command == CommandKind::Agent {
        if parsed.robot {
            return Err("rr agent is a separate transport from --robot; omit --robot".to_owned());
        }
        if parsed.dry_run {
            return Err("rr agent does not support --dry-run".to_owned());
        }
        if parsed.agent_task_file.is_none() {
            return Err("rr agent requires --task-file <path>".to_owned());
        }
        if parsed.repo.is_some()
            || parsed.pr.is_some()
            || parsed.session_id.is_some()
            || !parsed.attention_states.is_empty()
            || parsed.limit.is_some()
            || parsed.query_text.is_some()
            || parsed.robot_docs_topic.is_some()
            || parsed.provider != "opencode"
            || parsed.bridge_command.is_some()
            || parsed.extension_command.is_some()
            || parsed.extension_browser.is_some()
            || parsed.bridge_extension_id.is_some()
            || parsed.bridge_binary_path.is_some()
            || parsed.bridge_install_root.is_some()
            || parsed.bridge_output_dir.is_some()
            || parsed.update_channel != "stable"
            || parsed.update_version.is_some()
            || parsed.update_api_root.is_some()
            || parsed.update_download_root.is_some()
            || parsed.update_target.is_some()
            || parsed.update_yes
        {
            return Err(
                "rr agent only supports <operation> plus --task-file, --request-file, --request-b64, --context-file, and --capability-file"
                    .to_owned(),
            );
        }
    }

    if parsed.command == CommandKind::Init {
        if parsed.dry_run {
            return Err("rr init does not support --dry-run in this slice".to_owned());
        }
        if parsed.repo.is_some()
            || parsed.pr.is_some()
            || parsed.session_id.is_some()
            || !parsed.attention_states.is_empty()
            || parsed.limit.is_some()
            || parsed.query_text.is_some()
            || parsed.query_mode.is_some()
            || parsed.robot_docs_topic.is_some()
            || parsed.provider != "opencode"
            || parsed.bridge_command.is_some()
            || parsed.extension_command.is_some()
            || parsed.extension_browser.is_some()
            || parsed.bridge_extension_id.is_some()
            || parsed.bridge_binary_path.is_some()
            || parsed.bridge_install_root.is_some()
            || parsed.bridge_output_dir.is_some()
            || parsed.update_channel != "stable"
            || parsed.update_version.is_some()
            || parsed.update_api_root.is_some()
            || parsed.update_download_root.is_some()
            || parsed.update_target.is_some()
            || parsed.update_yes
            || parsed.draft_all_findings
            || !parsed.draft_finding_ids.is_empty()
            || parsed.batch_id.is_some()
            || parsed.agent_operation.is_some()
            || parsed.agent_task_file.is_some()
            || parsed.agent_request_file.is_some()
            || parsed.agent_context_file.is_some()
            || parsed.agent_capability_file.is_some()
        {
            return Err("rr init only supports --robot".to_owned());
        }
    }

    if parsed.command == CommandKind::Doctor {
        if parsed.dry_run {
            return Err("rr doctor does not support --dry-run in this slice".to_owned());
        }
        if parsed.repo.is_some()
            || parsed.pr.is_some()
            || parsed.session_id.is_some()
            || !parsed.attention_states.is_empty()
            || parsed.limit.is_some()
            || parsed.query_text.is_some()
            || parsed.query_mode.is_some()
            || parsed.robot_docs_topic.is_some()
            || parsed.bridge_command.is_some()
            || parsed.extension_command.is_some()
            || parsed.extension_browser.is_some()
            || parsed.bridge_extension_id.is_some()
            || parsed.bridge_binary_path.is_some()
            || parsed.bridge_install_root.is_some()
            || parsed.bridge_output_dir.is_some()
            || parsed.update_channel != "stable"
            || parsed.update_version.is_some()
            || parsed.update_api_root.is_some()
            || parsed.update_download_root.is_some()
            || parsed.update_target.is_some()
            || parsed.update_yes
            || parsed.draft_all_findings
            || !parsed.draft_finding_ids.is_empty()
            || parsed.batch_id.is_some()
            || parsed.agent_operation.is_some()
            || parsed.agent_task_file.is_some()
            || parsed.agent_request_file.is_some()
            || parsed.agent_context_file.is_some()
            || parsed.agent_capability_file.is_some()
        {
            return Err("rr doctor only supports --provider and --robot".to_owned());
        }
    }

    if parsed.command == CommandKind::Prs {
        if parsed.dry_run {
            return Err("rr prs does not support --dry-run in this slice".to_owned());
        }
        if parsed.pr.is_some()
            || parsed.session_id.is_some()
            || !parsed.attention_states.is_empty()
            || parsed.query_text.is_some()
            || parsed.query_mode.is_some()
            || parsed.robot_docs_topic.is_some()
            || parsed.provider != "opencode"
            || parsed.bridge_command.is_some()
            || parsed.extension_command.is_some()
            || parsed.extension_browser.is_some()
            || parsed.bridge_extension_id.is_some()
            || parsed.bridge_binary_path.is_some()
            || parsed.bridge_install_root.is_some()
            || parsed.bridge_output_dir.is_some()
            || parsed.update_channel != "stable"
            || parsed.update_version.is_some()
            || parsed.update_api_root.is_some()
            || parsed.update_download_root.is_some()
            || parsed.update_target.is_some()
            || parsed.update_yes
            || parsed.draft_all_findings
            || !parsed.draft_finding_ids.is_empty()
            || parsed.batch_id.is_some()
            || parsed.agent_operation.is_some()
            || parsed.agent_task_file.is_some()
            || parsed.agent_request_file.is_some()
            || parsed.agent_context_file.is_some()
            || parsed.agent_capability_file.is_some()
        {
            return Err("rr prs only supports --repo, --limit, and --robot".to_owned());
        }
    }

    if parsed.command == CommandKind::Tui {
        if parsed.dry_run {
            return Err("rr tui does not support --dry-run".to_owned());
        }
        if !parsed.attention_states.is_empty()
            || parsed.limit.is_some()
            || parsed.query_text.is_some()
            || parsed.query_mode.is_some()
            || parsed.robot_docs_topic.is_some()
            || parsed.provider != "opencode"
            || parsed.bridge_command.is_some()
            || parsed.extension_command.is_some()
            || parsed.extension_browser.is_some()
            || parsed.bridge_extension_id.is_some()
            || parsed.bridge_binary_path.is_some()
            || parsed.bridge_install_root.is_some()
            || parsed.bridge_output_dir.is_some()
            || parsed.update_channel != "stable"
            || parsed.update_version.is_some()
            || parsed.update_api_root.is_some()
            || parsed.update_download_root.is_some()
            || parsed.update_target.is_some()
            || parsed.update_yes
            || parsed.draft_all_findings
            || !parsed.draft_finding_ids.is_empty()
            || parsed.batch_id.is_some()
            || parsed.agent_operation.is_some()
            || parsed.agent_task_file.is_some()
            || parsed.agent_request_file.is_some()
            || parsed.agent_context_file.is_some()
            || parsed.agent_capability_file.is_some()
        {
            return Err("rr tui only supports --repo, --pr, and --session".to_owned());
        }
    }

    // Positive flag whitelists for the remaining commands. Each rejects every
    // flag it does not implement (same style as rr prs/tui/doctor above) so a
    // misapplied flag fails loudly instead of being silently ignored.
    if parsed.command == CommandKind::Review {
        // review supports --dry-run (preflight); --resume flips to resume.
        if parsed.session_id.is_some()
            || !parsed.attention_states.is_empty()
            || parsed.limit.is_some()
            || parsed.query_text.is_some()
            || parsed.query_mode.is_some()
            || parsed.robot_docs_topic.is_some()
            || parsed.bridge_command.is_some()
            || parsed.extension_command.is_some()
            || parsed.assets_command.is_some()
            || parsed.extension_browser.is_some()
            || parsed.extension_package_dir.is_some()
            || parsed.assets_package_id.is_some()
            || parsed.bridge_extension_id.is_some()
            || parsed.bridge_binary_path.is_some()
            || parsed.bridge_install_root.is_some()
            || parsed.bridge_output_dir.is_some()
            || parsed.update_channel != "stable"
            || parsed.update_version.is_some()
            || parsed.update_api_root.is_some()
            || parsed.update_download_root.is_some()
            || parsed.update_target.is_some()
            || parsed.update_yes
            || parsed.draft_all_findings
            || !parsed.draft_finding_ids.is_empty()
            || parsed.batch_id.is_some()
            || parsed.agent_operation.is_some()
            || parsed.agent_task_file.is_some()
            || parsed.agent_request_file.is_some()
            || parsed.agent_context_file.is_some()
            || parsed.agent_capability_file.is_some()
        {
            return Err(
                "rr review only supports --repo, --pr, --provider, --resume, --fresh, --dry-run, and --robot"
                    .to_owned(),
            );
        }
    }

    if parsed.command == CommandKind::Resume {
        // resume supports --dry-run (preflight); reached via rr resume or rr review --resume.
        // --provider is otherwise session-derived; copilot is admitted only for
        // the explicit --interactive terminal handoff (gate checked above).
        let provider_allowed = parsed.provider == "opencode"
            || (parsed.provider == session_copilot::PROVIDER_ID && parsed.interactive);
        if !provider_allowed
            || !parsed.attention_states.is_empty()
            || parsed.limit.is_some()
            || parsed.query_text.is_some()
            || parsed.query_mode.is_some()
            || parsed.robot_docs_topic.is_some()
            || parsed.bridge_command.is_some()
            || parsed.extension_command.is_some()
            || parsed.assets_command.is_some()
            || parsed.extension_browser.is_some()
            || parsed.extension_package_dir.is_some()
            || parsed.assets_package_id.is_some()
            || parsed.bridge_extension_id.is_some()
            || parsed.bridge_binary_path.is_some()
            || parsed.bridge_install_root.is_some()
            || parsed.bridge_output_dir.is_some()
            || parsed.update_channel != "stable"
            || parsed.update_version.is_some()
            || parsed.update_api_root.is_some()
            || parsed.update_download_root.is_some()
            || parsed.update_target.is_some()
            || parsed.update_yes
            || parsed.draft_all_findings
            || !parsed.draft_finding_ids.is_empty()
            || parsed.batch_id.is_some()
            || parsed.agent_operation.is_some()
            || parsed.agent_task_file.is_some()
            || parsed.agent_request_file.is_some()
            || parsed.agent_context_file.is_some()
            || parsed.agent_capability_file.is_some()
        {
            return Err(
                "rr resume only supports --repo, --pr, --session, --dry-run, and --robot"
                    .to_owned(),
            );
        }
    }

    if parsed.command == CommandKind::Return {
        if parsed.dry_run {
            return Err("rr return does not support --dry-run".to_owned());
        }
        // --provider is otherwise session-derived; copilot is admitted only for
        // the explicit --interactive terminal handoff (gate checked above).
        let provider_allowed = parsed.provider == "opencode"
            || (parsed.provider == session_copilot::PROVIDER_ID && parsed.interactive);
        if !provider_allowed
            || !parsed.attention_states.is_empty()
            || parsed.limit.is_some()
            || parsed.query_text.is_some()
            || parsed.query_mode.is_some()
            || parsed.robot_docs_topic.is_some()
            || parsed.bridge_command.is_some()
            || parsed.extension_command.is_some()
            || parsed.assets_command.is_some()
            || parsed.extension_browser.is_some()
            || parsed.extension_package_dir.is_some()
            || parsed.assets_package_id.is_some()
            || parsed.bridge_extension_id.is_some()
            || parsed.bridge_binary_path.is_some()
            || parsed.bridge_install_root.is_some()
            || parsed.bridge_output_dir.is_some()
            || parsed.update_channel != "stable"
            || parsed.update_version.is_some()
            || parsed.update_api_root.is_some()
            || parsed.update_download_root.is_some()
            || parsed.update_target.is_some()
            || parsed.update_yes
            || parsed.draft_all_findings
            || !parsed.draft_finding_ids.is_empty()
            || parsed.batch_id.is_some()
            || parsed.agent_operation.is_some()
            || parsed.agent_task_file.is_some()
            || parsed.agent_request_file.is_some()
            || parsed.agent_context_file.is_some()
            || parsed.agent_capability_file.is_some()
        {
            return Err("rr return only supports --repo, --pr, --session, and --robot".to_owned());
        }
    }

    if parsed.command == CommandKind::Sessions {
        // reached via rr sessions or rr findings --sessions.
        if parsed.dry_run {
            return Err("rr sessions does not support --dry-run".to_owned());
        }
        if parsed.session_id.is_some()
            || parsed.provider != "opencode"
            || parsed.query_text.is_some()
            || parsed.query_mode.is_some()
            || parsed.robot_docs_topic.is_some()
            || parsed.bridge_command.is_some()
            || parsed.extension_command.is_some()
            || parsed.assets_command.is_some()
            || parsed.extension_browser.is_some()
            || parsed.extension_package_dir.is_some()
            || parsed.assets_package_id.is_some()
            || parsed.bridge_extension_id.is_some()
            || parsed.bridge_binary_path.is_some()
            || parsed.bridge_install_root.is_some()
            || parsed.bridge_output_dir.is_some()
            || parsed.update_channel != "stable"
            || parsed.update_version.is_some()
            || parsed.update_api_root.is_some()
            || parsed.update_download_root.is_some()
            || parsed.update_target.is_some()
            || parsed.update_yes
            || parsed.draft_all_findings
            || !parsed.draft_finding_ids.is_empty()
            || parsed.batch_id.is_some()
            || parsed.agent_operation.is_some()
            || parsed.agent_task_file.is_some()
            || parsed.agent_request_file.is_some()
            || parsed.agent_context_file.is_some()
            || parsed.agent_capability_file.is_some()
        {
            return Err(
                "rr sessions only supports --repo, --pr, --attention, --limit, --all, and --robot"
                    .to_owned(),
            );
        }
    }

    if parsed.command == CommandKind::Search {
        // reached via rr search or rr findings --query; --session/--pr already
        // rejected above as corpus-scope violations.
        if parsed.dry_run {
            return Err("rr search does not support --dry-run".to_owned());
        }
        if parsed.provider != "opencode"
            || !parsed.attention_states.is_empty()
            || parsed.robot_docs_topic.is_some()
            || parsed.bridge_command.is_some()
            || parsed.extension_command.is_some()
            || parsed.assets_command.is_some()
            || parsed.extension_browser.is_some()
            || parsed.extension_package_dir.is_some()
            || parsed.assets_package_id.is_some()
            || parsed.bridge_extension_id.is_some()
            || parsed.bridge_binary_path.is_some()
            || parsed.bridge_install_root.is_some()
            || parsed.bridge_output_dir.is_some()
            || parsed.update_channel != "stable"
            || parsed.update_version.is_some()
            || parsed.update_api_root.is_some()
            || parsed.update_download_root.is_some()
            || parsed.update_target.is_some()
            || parsed.update_yes
            || parsed.draft_all_findings
            || !parsed.draft_finding_ids.is_empty()
            || parsed.batch_id.is_some()
            || parsed.agent_operation.is_some()
            || parsed.agent_task_file.is_some()
            || parsed.agent_request_file.is_some()
            || parsed.agent_context_file.is_some()
            || parsed.agent_capability_file.is_some()
        {
            return Err(
                "rr search only supports --query, --query-mode, --repo, --limit, and --robot"
                    .to_owned(),
            );
        }
    }

    if parsed.command == CommandKind::Status {
        if parsed.dry_run {
            return Err("rr status does not support --dry-run".to_owned());
        }
        if parsed.provider != "opencode"
            || !parsed.attention_states.is_empty()
            || parsed.limit.is_some()
            || parsed.query_text.is_some()
            || parsed.query_mode.is_some()
            || parsed.robot_docs_topic.is_some()
            || parsed.bridge_command.is_some()
            || parsed.extension_command.is_some()
            || parsed.assets_command.is_some()
            || parsed.extension_browser.is_some()
            || parsed.extension_package_dir.is_some()
            || parsed.assets_package_id.is_some()
            || parsed.bridge_extension_id.is_some()
            || parsed.bridge_binary_path.is_some()
            || parsed.bridge_install_root.is_some()
            || parsed.bridge_output_dir.is_some()
            || parsed.update_channel != "stable"
            || parsed.update_version.is_some()
            || parsed.update_api_root.is_some()
            || parsed.update_download_root.is_some()
            || parsed.update_target.is_some()
            || parsed.update_yes
            || parsed.draft_all_findings
            || !parsed.draft_finding_ids.is_empty()
            || parsed.batch_id.is_some()
            || parsed.agent_operation.is_some()
            || parsed.agent_task_file.is_some()
            || parsed.agent_request_file.is_some()
            || parsed.agent_context_file.is_some()
            || parsed.agent_capability_file.is_some()
        {
            return Err("rr status only supports --repo, --pr, --session, and --robot".to_owned());
        }
    }

    if parsed.command == CommandKind::Findings {
        // bare rr findings; --query/--sessions already flipped to search/sessions.
        if parsed.dry_run {
            return Err("rr findings does not support --dry-run".to_owned());
        }
        if parsed.provider != "opencode"
            || !parsed.attention_states.is_empty()
            || parsed.limit.is_some()
            || parsed.query_text.is_some()
            || parsed.query_mode.is_some()
            || parsed.robot_docs_topic.is_some()
            || parsed.bridge_command.is_some()
            || parsed.extension_command.is_some()
            || parsed.assets_command.is_some()
            || parsed.extension_browser.is_some()
            || parsed.extension_package_dir.is_some()
            || parsed.assets_package_id.is_some()
            || parsed.bridge_extension_id.is_some()
            || parsed.bridge_binary_path.is_some()
            || parsed.bridge_install_root.is_some()
            || parsed.bridge_output_dir.is_some()
            || parsed.update_channel != "stable"
            || parsed.update_version.is_some()
            || parsed.update_api_root.is_some()
            || parsed.update_download_root.is_some()
            || parsed.update_target.is_some()
            || parsed.update_yes
            || parsed.draft_all_findings
            || !parsed.draft_finding_ids.is_empty()
            || parsed.batch_id.is_some()
            || parsed.agent_operation.is_some()
            || parsed.agent_task_file.is_some()
            || parsed.agent_request_file.is_some()
            || parsed.agent_context_file.is_some()
            || parsed.agent_capability_file.is_some()
        {
            return Err(
                "rr findings only supports --repo, --pr, --session, --query, --sessions, and --robot"
                    .to_owned(),
            );
        }
    }

    if parsed.command == CommandKind::Bridge {
        if parsed.dry_run {
            return Err("rr bridge does not support --dry-run".to_owned());
        }
        if parsed.repo.is_some()
            || parsed.pr.is_some()
            || parsed.session_id.is_some()
            || parsed.provider != "opencode"
            || !parsed.attention_states.is_empty()
            || parsed.limit.is_some()
            || parsed.query_text.is_some()
            || parsed.query_mode.is_some()
            || parsed.robot_docs_topic.is_some()
            || parsed.extension_command.is_some()
            || parsed.assets_command.is_some()
            || parsed.extension_browser.is_some()
            || parsed.extension_package_dir.is_some()
            || parsed.assets_package_id.is_some()
            || parsed.update_channel != "stable"
            || parsed.update_version.is_some()
            || parsed.update_api_root.is_some()
            || parsed.update_download_root.is_some()
            || parsed.update_target.is_some()
            || parsed.update_yes
            || parsed.draft_all_findings
            || !parsed.draft_finding_ids.is_empty()
            || parsed.batch_id.is_some()
            || parsed.agent_operation.is_some()
            || parsed.agent_task_file.is_some()
            || parsed.agent_request_file.is_some()
            || parsed.agent_context_file.is_some()
            || parsed.agent_capability_file.is_some()
        {
            return Err(
                "rr bridge only supports <subcommand> plus --extension-id, --bridge-binary, --install-root, --output-dir, and --robot"
                    .to_owned(),
            );
        }
    }

    if parsed.command == CommandKind::Extension {
        if parsed.dry_run {
            return Err("rr extension does not support --dry-run".to_owned());
        }
        // --version/--download-root are legitimate for `extension fetch` and are
        // scope-gated by the shared update-flag check above; the remaining
        // update flags and every unrelated flag are rejected here.
        if parsed.repo.is_some()
            || parsed.pr.is_some()
            || parsed.session_id.is_some()
            || parsed.provider != "opencode"
            || !parsed.attention_states.is_empty()
            || parsed.limit.is_some()
            || parsed.query_text.is_some()
            || parsed.query_mode.is_some()
            || parsed.robot_docs_topic.is_some()
            || parsed.bridge_command.is_some()
            || parsed.assets_command.is_some()
            || parsed.assets_package_id.is_some()
            || parsed.bridge_extension_id.is_some()
            || parsed.bridge_binary_path.is_some()
            || parsed.bridge_output_dir.is_some()
            || parsed.draft_all_findings
            || !parsed.draft_finding_ids.is_empty()
            || parsed.batch_id.is_some()
            || parsed.agent_operation.is_some()
            || parsed.agent_task_file.is_some()
            || parsed.agent_request_file.is_some()
            || parsed.agent_context_file.is_some()
            || parsed.agent_capability_file.is_some()
        {
            return Err(
                "rr extension only supports <subcommand> plus --browser, --package-dir, --install-root, --version (fetch), --download-root (fetch), and --robot"
                    .to_owned(),
            );
        }
    }

    if parsed.command == CommandKind::RobotDocs {
        if parsed.dry_run {
            return Err("rr robot-docs does not support --dry-run".to_owned());
        }
        if parsed.repo.is_some()
            || parsed.pr.is_some()
            || parsed.session_id.is_some()
            || parsed.provider != "opencode"
            || !parsed.attention_states.is_empty()
            || parsed.limit.is_some()
            || parsed.query_text.is_some()
            || parsed.query_mode.is_some()
            || parsed.bridge_command.is_some()
            || parsed.extension_command.is_some()
            || parsed.assets_command.is_some()
            || parsed.extension_browser.is_some()
            || parsed.extension_package_dir.is_some()
            || parsed.assets_package_id.is_some()
            || parsed.bridge_extension_id.is_some()
            || parsed.bridge_binary_path.is_some()
            || parsed.bridge_install_root.is_some()
            || parsed.bridge_output_dir.is_some()
            || parsed.update_channel != "stable"
            || parsed.update_version.is_some()
            || parsed.update_api_root.is_some()
            || parsed.update_download_root.is_some()
            || parsed.update_target.is_some()
            || parsed.update_yes
            || parsed.draft_all_findings
            || !parsed.draft_finding_ids.is_empty()
            || parsed.batch_id.is_some()
            || parsed.agent_operation.is_some()
            || parsed.agent_task_file.is_some()
            || parsed.agent_request_file.is_some()
            || parsed.agent_context_file.is_some()
            || parsed.agent_capability_file.is_some()
        {
            return Err(
                "rr robot-docs only supports [guide|commands|schemas|workflows] and --robot"
                    .to_owned(),
            );
        }
    }

    if parsed.command == CommandKind::Memory {
        // review lists pending rows read-only; accept/reject resolve one exact
        // request id. --request belongs to accept/reject; it is rejected on
        // review below. --limit caps the review listing.
        if parsed.dry_run {
            return Err("rr memory does not support --dry-run".to_owned());
        }
        if parsed.provider != "opencode"
            || !parsed.attention_states.is_empty()
            || parsed.query_text.is_some()
            || parsed.query_mode.is_some()
            || parsed.robot_docs_topic.is_some()
            || parsed.bridge_command.is_some()
            || parsed.extension_command.is_some()
            || parsed.assets_command.is_some()
            || parsed.extension_browser.is_some()
            || parsed.extension_package_dir.is_some()
            || parsed.assets_package_id.is_some()
            || parsed.bridge_extension_id.is_some()
            || parsed.bridge_binary_path.is_some()
            || parsed.bridge_install_root.is_some()
            || parsed.bridge_output_dir.is_some()
            || parsed.update_channel != "stable"
            || parsed.update_version.is_some()
            || parsed.update_api_root.is_some()
            || parsed.update_download_root.is_some()
            || parsed.update_target.is_some()
            || parsed.update_yes
            || parsed.draft_all_findings
            || !parsed.draft_finding_ids.is_empty()
            || parsed.batch_id.is_some()
            || parsed.triage_state.is_some()
            || parsed.edit_body_file.is_some()
            || parsed.clarify_body.is_some()
            || parsed.clarify_list
            || parsed.agent_operation.is_some()
            || parsed.agent_task_file.is_some()
            || parsed.agent_request_file.is_some()
            || parsed.agent_context_file.is_some()
            || parsed.agent_capability_file.is_some()
        {
            return Err(
                "rr memory only supports <review|accept|reject> plus --repo, --pr, --session, --request (accept/reject), --limit (review), and --robot".to_owned(),
            );
        }
        match parsed.memory_command {
            Some(MemoryCommandKind::Review) => {
                if parsed.request_id.is_some() {
                    return Err(
                        "rr memory review does not support --request; --request is for rr memory accept/reject".to_owned(),
                    );
                }
            }
            Some(MemoryCommandKind::Accept | MemoryCommandKind::Reject) => {
                if parsed.limit.is_some() {
                    return Err(
                        "rr memory accept/reject does not support --limit; --limit is for rr memory review".to_owned(),
                    );
                }
            }
            None => {}
        }
    }

    if parsed.command == CommandKind::Timeline {
        if parsed.dry_run {
            return Err("rr timeline does not support --dry-run".to_owned());
        }
        if parsed.provider != "opencode"
            || !parsed.attention_states.is_empty()
            || parsed.limit.is_some()
            || parsed.query_text.is_some()
            || parsed.query_mode.is_some()
            || parsed.robot_docs_topic.is_some()
            || parsed.bridge_command.is_some()
            || parsed.extension_command.is_some()
            || parsed.assets_command.is_some()
            || parsed.extension_browser.is_some()
            || parsed.extension_package_dir.is_some()
            || parsed.assets_package_id.is_some()
            || parsed.bridge_extension_id.is_some()
            || parsed.bridge_binary_path.is_some()
            || parsed.bridge_install_root.is_some()
            || parsed.bridge_output_dir.is_some()
            || parsed.update_channel != "stable"
            || parsed.update_version.is_some()
            || parsed.update_api_root.is_some()
            || parsed.update_download_root.is_some()
            || parsed.update_target.is_some()
            || parsed.update_yes
            || parsed.draft_all_findings
            || !parsed.draft_finding_ids.is_empty()
            || parsed.batch_id.is_some()
            || parsed.triage_state.is_some()
            || parsed.edit_body_file.is_some()
            || parsed.request_id.is_some()
            || parsed.clarify_body.is_some()
            || parsed.clarify_list
            || parsed.agent_operation.is_some()
            || parsed.agent_task_file.is_some()
            || parsed.agent_request_file.is_some()
            || parsed.agent_context_file.is_some()
            || parsed.agent_capability_file.is_some()
        {
            return Err(
                "rr timeline only supports --repo, --pr, --session, and --robot".to_owned(),
            );
        }
    }

    if parsed.command == CommandKind::Clarify {
        // create: --finding + (--body | --body-file). list: --list [--session].
        // The create-vs-list argument validation (fail-closed blocked envelope)
        // lives in handle_clarify so --robot callers get a JSON envelope.
        if parsed.dry_run {
            return Err("rr clarify does not support --dry-run".to_owned());
        }
        if parsed.provider != "opencode"
            || !parsed.attention_states.is_empty()
            || parsed.limit.is_some()
            || parsed.query_text.is_some()
            || parsed.query_mode.is_some()
            || parsed.robot_docs_topic.is_some()
            || parsed.bridge_command.is_some()
            || parsed.extension_command.is_some()
            || parsed.assets_command.is_some()
            || parsed.extension_browser.is_some()
            || parsed.extension_package_dir.is_some()
            || parsed.assets_package_id.is_some()
            || parsed.bridge_extension_id.is_some()
            || parsed.bridge_binary_path.is_some()
            || parsed.bridge_install_root.is_some()
            || parsed.bridge_output_dir.is_some()
            || parsed.update_channel != "stable"
            || parsed.update_version.is_some()
            || parsed.update_api_root.is_some()
            || parsed.update_download_root.is_some()
            || parsed.update_target.is_some()
            || parsed.update_yes
            || parsed.batch_id.is_some()
            || parsed.triage_state.is_some()
            || parsed.request_id.is_some()
            || parsed.agent_operation.is_some()
            || parsed.agent_task_file.is_some()
            || parsed.agent_request_file.is_some()
            || parsed.agent_context_file.is_some()
            || parsed.agent_capability_file.is_some()
        {
            return Err(
                "rr clarify only supports --repo, --pr, --session, --finding, --body, --body-file, --list, and --robot".to_owned(),
            );
        }
        if parsed.clarify_body.is_some() && parsed.edit_body_file.is_some() {
            return Err("rr clarify accepts either --body or --body-file, not both".to_owned());
        }
    }

    Ok(parsed)
}

fn canonicalize_provider_arg(value: &str) -> String {
    if let Some(provider) = session_copilot::parse_provider_identifier(value) {
        return provider.to_owned();
    }

    value.trim().to_ascii_lowercase()
}

fn execute_command(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    match parsed.command {
        CommandKind::Agent => handle_agent(parsed, runtime),
        CommandKind::Init => handle_init(runtime),
        CommandKind::Doctor => handle_doctor(parsed, runtime),
        CommandKind::Review => handle_review(parsed, runtime),
        CommandKind::Resume => handle_resume(parsed, runtime),
        CommandKind::Return => handle_return(parsed, runtime),
        CommandKind::Sessions => handle_sessions(parsed, runtime),
        CommandKind::Prs => handle_prs(parsed, runtime),
        CommandKind::Search => handle_search(parsed, runtime),
        CommandKind::Triage => handle_triage(parsed, runtime),
        CommandKind::Draft => handle_draft(parsed, runtime),
        CommandKind::Edit => handle_edit(parsed, runtime),
        CommandKind::Approve => handle_approve(parsed, runtime),
        CommandKind::Post => handle_post(parsed, runtime),
        CommandKind::Update => handle_update(parsed, runtime),
        CommandKind::Bridge => handle_bridge(parsed, runtime),
        CommandKind::Extension => handle_extension(parsed, runtime),
        CommandKind::RobotDocs => handle_robot_docs(parsed, runtime),
        CommandKind::Findings => handle_findings(parsed, runtime),
        CommandKind::Status => handle_status(parsed, runtime),
        CommandKind::Tui => handle_tui(parsed, runtime),
        CommandKind::Assets => handle_assets(parsed, runtime),
        CommandKind::Memory => handle_memory(parsed, runtime),
        CommandKind::Timeline => handle_timeline(parsed, runtime),
        CommandKind::Clarify => handle_clarify(parsed, runtime),
    }
}

fn built_in_agent_capability_profile() -> WorkerCapabilityProfile {
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

fn effective_agent_capability_profile(
    requested: Option<WorkerCapabilityProfile>,
) -> WorkerCapabilityProfile {
    let live = built_in_agent_capability_profile();
    let Some(requested) = requested else {
        return live;
    };

    WorkerCapabilityProfile {
        transport_kind: WorkerTransportKind::AgentCli,
        supports_context_reads: live.supports_context_reads && requested.supports_context_reads,
        supports_memory_search: live.supports_memory_search && requested.supports_memory_search,
        supports_finding_reads: live.supports_finding_reads && requested.supports_finding_reads,
        supports_artifact_reads: live.supports_artifact_reads && requested.supports_artifact_reads,
        supports_stage_result_submission: live.supports_stage_result_submission
            && requested.supports_stage_result_submission,
        supports_clarification_requests: live.supports_clarification_requests
            && requested.supports_clarification_requests,
        supports_follow_up_hints: live.supports_follow_up_hints
            && requested.supports_follow_up_hints,
        supports_fix_mode: live.supports_fix_mode && requested.supports_fix_mode,
    }
}

fn read_json_bytes_from_stdin_or_file(
    path: Option<&PathBuf>,
    stdin_label: &str,
) -> Result<Vec<u8>, String> {
    if let Some(path) = path {
        return fs::read(path).map_err(|err| format!("failed to read {}: {err}", path.display()));
    }

    if io::stdin().is_terminal() {
        return Err(format!(
            "{stdin_label} must be provided via --request-file or stdin"
        ));
    }

    let mut bytes = Vec::new();
    io::stdin()
        .read_to_end(&mut bytes)
        .map_err(|err| format!("failed to read {stdin_label} from stdin: {err}"))?;
    if bytes.is_empty() {
        return Err(format!("{stdin_label} from stdin was empty"));
    }
    Ok(bytes)
}

/// Read-only worker operations that need only the task binding — no request
/// payload. For these, `rr agent <op> --task-file <path>` is sufficient on its
/// own; Roger synthesizes a default bounded request from the ReviewTask. This is
/// what makes the seeded call `rr agent worker.get_review_context --task-file
/// <path>` succeed for an in-session worker under a write-denied policy (it
/// cannot stage a separate --request-file).
fn agent_operation_is_self_serviceable(operation: &str) -> bool {
    matches!(
        operation,
        "worker.get_review_context" | "worker.get_status" | "worker.list_findings"
    )
}

/// Build a default bounded request envelope from a ReviewTask for a
/// self-serviceable read operation. Binding fields come straight from the task
/// (so the nonce round-trips); requested scopes default to the task's allowed
/// scopes; no payload.
fn default_agent_request_from_task(
    task: &ReviewTask,
    operation: &str,
) -> WorkerOperationRequestEnvelope {
    WorkerOperationRequestEnvelope {
        schema_id: WORKER_OPERATION_REQUEST_SCHEMA_V1.to_owned(),
        review_session_id: task.review_session_id.clone(),
        review_run_id: task.review_run_id.clone(),
        review_task_id: task.id.clone(),
        task_nonce: task.task_nonce.clone(),
        operation: operation.to_owned(),
        requested_scopes: task.allowed_scopes.clone(),
        payload: None,
    }
}

fn read_json_file<T: DeserializeOwned>(path: &Path, label: &str) -> Result<T, String> {
    let bytes = fs::read(path).map_err(|err| format!("failed to read {label}: {err}"))?;
    serde_json::from_slice(&bytes).map_err(|err| format!("failed to parse {label} as JSON: {err}"))
}

fn finding_summary_from_record(
    record: &roger_storage::MaterializedFindingRecord,
) -> WorkerFindingSummary {
    WorkerFindingSummary {
        finding_id: record.id.clone(),
        fingerprint: record.fingerprint.clone(),
        summary: record.normalized_summary.clone(),
        triage_state: record.triage_state.clone(),
        outbound_state: record.outbound_state.clone(),
        primary_evidence_ref: None,
    }
}

fn load_agent_findings(
    store: &RogerStore,
    task: &ReviewTask,
) -> Result<Vec<WorkerFindingSummary>, String> {
    let findings = store
        .materialized_findings_for_run(&task.review_session_id, &task.review_run_id)
        .map_err(|err| format!("failed to load materialized findings for rr agent: {err}"))?;
    Ok(findings.iter().map(finding_summary_from_record).collect())
}

fn session_baseline_snapshot_projection(
    record: &roger_storage::SessionBaselineSnapshotRecord,
) -> SessionBaselineSnapshot {
    SessionBaselineSnapshot {
        id: record.id.clone(),
        review_session_id: record.review_session_id.clone(),
        review_run_id: record.review_run_id.clone(),
        baseline_generation: record.baseline_generation,
        review_target_snapshot: record.review_target_snapshot.clone(),
        allowed_scopes: record.allowed_scopes.clone(),
        default_query_mode: record.default_query_mode.clone(),
        candidate_visibility_policy: record.candidate_visibility_policy.clone(),
        prompt_strategy: record.prompt_strategy.clone(),
        policy_epoch_refs: record.policy_epoch_refs.clone(),
        degraded_flags: record.degraded_flags.clone(),
        created_at: record.created_at,
    }
}

fn synthesize_agent_context(
    store: &RogerStore,
    session: &roger_storage::ReviewSessionRecord,
    task: &ReviewTask,
    unresolved_findings: Vec<WorkerFindingSummary>,
) -> Result<WorkerContextPacket, String> {
    let baseline_snapshot = store
        .latest_session_baseline_snapshot(&task.review_session_id)
        .map_err(|err| format!("failed to load persisted baseline snapshot for rr agent: {err}"))?;
    let baseline_snapshot_ref = baseline_snapshot
        .as_ref()
        .map(|snapshot| snapshot.id.clone());
    let baseline_snapshot = baseline_snapshot
        .as_ref()
        .map(session_baseline_snapshot_projection);

    Ok(WorkerContextPacket {
        review_target: session.review_target.clone(),
        review_session_id: task.review_session_id.clone(),
        review_run_id: task.review_run_id.clone(),
        review_task_id: task.id.clone(),
        task_nonce: task.task_nonce.clone(),
        baseline_snapshot_ref,
        baseline_snapshot,
        provider: session.provider.clone(),
        transport_kind: WorkerTransportKind::AgentCli,
        stage: task.stage.clone(),
        objective: task.objective.clone(),
        allowed_scopes: task.allowed_scopes.clone(),
        allowed_operations: task.allowed_operations.clone(),
        mutation_posture: WorkerMutationPosture::ReviewOnly,
        github_posture: WorkerGitHubPosture::Blocked,
        unresolved_findings,
        continuity_summary: Some(session.continuity_state.clone()),
        memory_cards: Vec::new(),
        artifact_refs: Vec::new(),
    })
}

fn build_agent_status_snapshot(
    store: &RogerStore,
    session: &roger_storage::ReviewSessionRecord,
    task: &ReviewTask,
    unresolved_finding_count: usize,
) -> Result<WorkerStatusSnapshot, String> {
    let draft_count = store
        .session_overview(&task.review_session_id)
        .map_err(|err| format!("failed to build session overview for rr agent: {err}"))?
        .draft_count
        .max(0) as usize;
    Ok(WorkerStatusSnapshot {
        review_session_id: task.review_session_id.clone(),
        review_run_id: task.review_run_id.clone(),
        attention_state: session.attention_state.clone(),
        continuity_summary: Some(session.continuity_state.clone()),
        degraded_flags: Vec::new(),
        unresolved_finding_count,
        pending_clarification_count: 0,
        draft_count,
    })
}

fn retrieval_mode_label(mode: &PriorReviewRetrievalMode) -> &'static str {
    match mode {
        PriorReviewRetrievalMode::Hybrid => "hybrid",
        PriorReviewRetrievalMode::LexicalOnly => "lexical_only",
        PriorReviewRetrievalMode::RecoveryScan => "recovery_scan",
    }
}

fn recall_anchor_overlap_summary(anchor_hints: &[String], anchor_digest: Option<&str>) -> String {
    match (anchor_hints.is_empty(), anchor_digest) {
        (true, Some(digest)) => {
            format!("anchor digest {digest} recorded; no anchor hints supplied")
        }
        (true, None) => "no anchor hints supplied".to_owned(),
        (false, Some(digest)) => format!(
            "{} anchor hint(s) supplied; digest {digest} is recorded but overlap scoring is not implemented in this slice",
            anchor_hints.len()
        ),
        (false, None) => format!(
            "{} anchor hint(s) supplied; overlap scoring is unavailable for this record",
            anchor_hints.len()
        ),
    }
}

fn recall_explain_summary(
    item_kind: &str,
    memory_lane: &str,
    scope_bucket: &str,
    requested_query_mode: &str,
    resolved_query_mode: &str,
    retrieval_mode: &str,
    citation_posture: &str,
    surface_posture: &str,
    degraded_flags: &[String],
) -> String {
    let degraded_summary = if degraded_flags.is_empty() {
        "no degraded flags".to_owned()
    } else {
        format!("degraded flags: {}", degraded_flags.join(", "))
    };
    format!(
        "{item_kind} surfaced from {memory_lane} in {scope_bucket} with requested query_mode {requested_query_mode}, resolved query_mode {resolved_query_mode}, retrieval_mode {retrieval_mode}, posture {citation_posture}/{surface_posture}; {degraded_summary}"
    )
}

fn recall_source_ref(kind: &str, id: impl Into<String>) -> RecallSourceRef {
    RecallSourceRef {
        kind: kind.to_owned(),
        id: id.into(),
    }
}

fn recall_posture_for_memory_hit(memory_lane: &str, state: &str) -> (&'static str, &'static str) {
    match (memory_lane, state) {
        (_, "contradicted" | "anti_pattern") => ("warning_only", "operator_review_only"),
        ("tentative_candidates", _) | (_, "candidate") => ("inspect_only", "candidate_review"),
        _ => ("cite_allowed", "ordinary"),
    }
}

fn worker_recall_from_memory_hit(
    hit: &roger_storage::PriorReviewMemoryHit,
    requested_query_mode: &str,
    resolved_query_mode: &str,
    retrieval_mode: &str,
    scope_bucket: &str,
    degraded_flags: &[String],
    memory_lane: &str,
    anchor_hints: &[String],
) -> WorkerRecallEnvelope {
    let (citation_posture, surface_posture) =
        recall_posture_for_memory_hit(memory_lane, &hit.state);
    WorkerRecallEnvelope {
        item_kind: if memory_lane == "tentative_candidates" {
            "candidate_memory".to_owned()
        } else {
            "promoted_memory".to_owned()
        },
        item_id: hit.memory_id.clone(),
        requested_query_mode: requested_query_mode.to_owned(),
        resolved_query_mode: resolved_query_mode.to_owned(),
        retrieval_mode: retrieval_mode.to_owned(),
        scope_bucket: scope_bucket.to_owned(),
        memory_lane: memory_lane.to_owned(),
        trust_state: Some(hit.state.clone()),
        source_refs: vec![
            recall_source_ref("memory", hit.memory_id.clone()),
            recall_source_ref("scope", hit.scope_key.clone()),
        ],
        locator: json!({
            "scope_key": hit.scope_key,
            "memory_class": hit.memory_class,
            "state": hit.state,
        }),
        snippet_or_summary: hit.statement.clone(),
        anchor_overlap_summary: recall_anchor_overlap_summary(
            anchor_hints,
            hit.anchor_digest.as_deref(),
        ),
        degraded_flags: degraded_flags.to_vec(),
        explain_summary: recall_explain_summary(
            if memory_lane == "tentative_candidates" {
                "candidate_memory"
            } else {
                "promoted_memory"
            },
            memory_lane,
            scope_bucket,
            requested_query_mode,
            resolved_query_mode,
            retrieval_mode,
            citation_posture,
            surface_posture,
            degraded_flags,
        ),
        citation_posture: citation_posture.to_owned(),
        surface_posture: surface_posture.to_owned(),
    }
}

fn worker_recall_from_evidence_hit(
    hit: &roger_storage::PriorReviewEvidenceHit,
    requested_query_mode: &str,
    resolved_query_mode: &str,
    retrieval_mode: &str,
    scope_bucket: &str,
    degraded_flags: &[String],
    anchor_hints: &[String],
) -> WorkerRecallEnvelope {
    let mut source_refs = vec![
        recall_source_ref("finding", hit.finding_id.clone()),
        recall_source_ref("review_session", hit.session_id.clone()),
        recall_source_ref("repository", hit.repository.clone()),
    ];
    if let Some(review_run_id) = hit.review_run_id.as_ref() {
        source_refs.push(recall_source_ref("review_run", review_run_id.clone()));
    }

    WorkerRecallEnvelope {
        item_kind: "evidence_finding".to_owned(),
        item_id: hit.finding_id.clone(),
        requested_query_mode: requested_query_mode.to_owned(),
        resolved_query_mode: resolved_query_mode.to_owned(),
        retrieval_mode: retrieval_mode.to_owned(),
        scope_bucket: scope_bucket.to_owned(),
        memory_lane: "evidence_hits".to_owned(),
        trust_state: None,
        source_refs,
        locator: json!({
            "session_id": hit.session_id,
            "review_run_id": hit.review_run_id,
            "repository": hit.repository,
            "pull_request": hit.pull_request_number,
        }),
        snippet_or_summary: hit.normalized_summary.clone(),
        anchor_overlap_summary: recall_anchor_overlap_summary(anchor_hints, None),
        degraded_flags: degraded_flags.to_vec(),
        explain_summary: recall_explain_summary(
            "evidence_finding",
            "evidence_hits",
            scope_bucket,
            requested_query_mode,
            resolved_query_mode,
            retrieval_mode,
            "cite_allowed",
            "ordinary",
            degraded_flags,
        ),
        citation_posture: "cite_allowed".to_owned(),
        surface_posture: "ordinary".to_owned(),
    }
}

fn search_item_from_recall_envelope(
    envelope: &WorkerRecallEnvelope,
    title: &str,
    score: i64,
) -> Value {
    json!({
        "kind": envelope.item_kind,
        "id": envelope.item_id,
        "title": title,
        "score": score,
        "memory_lane": envelope.memory_lane,
        "scope_bucket": envelope.scope_bucket,
        "trust_state": envelope.trust_state,
        "citation_posture": envelope.citation_posture,
        "surface_posture": envelope.surface_posture,
        "locator": envelope.locator,
        "snippet": envelope.snippet_or_summary,
        "explain_summary": envelope.explain_summary,
    })
}

fn build_agent_search_response(
    store: &RogerStore,
    session: &roger_storage::ReviewSessionRecord,
    task: &ReviewTask,
    request: &WorkerOperationRequestEnvelope,
) -> Result<Option<WorkerSearchMemoryResponse>, String> {
    let Some(payload) = request.payload.clone() else {
        return Ok(None);
    };
    let Ok(search_request) = serde_json::from_value::<WorkerSearchMemoryRequest>(payload) else {
        return Ok(None);
    };

    let granted_scopes = if request.requested_scopes.is_empty() {
        task.allowed_scopes.clone()
    } else {
        request.requested_scopes.clone()
    };

    // Mirror rr search: real semantic verification gates hybrid eligibility so
    // the agent and operator surfaces stay in parity. False (lexical-only) until
    // verified assets + an operational embedder are installed.
    let component_state = store.semantic_component_state().ok();
    let embedder_operational = component_state
        .as_ref()
        .map(|state| state.embedder_available)
        .unwrap_or(false);
    let assets_verified = component_state
        .as_ref()
        .map(|state| state.assets_verified)
        .unwrap_or(false);
    let semantic_assets_verified = assets_verified && embedder_operational;

    let search_plan = materialize_search_plan(SearchPlanInput {
        review_session_id: Some(&task.review_session_id),
        review_run_id: Some(&task.review_run_id),
        repository: &session.review_target.repository,
        granted_scopes: &granted_scopes,
        query_text: &search_request.query_text,
        query_mode: Some(&search_request.query_mode),
        requested_retrieval_classes: &search_request.requested_retrieval_classes,
        anchor_hints: &search_request.anchor_hints,
        supports_candidate_audit: true,
        supports_promotion_review: false,
        semantic_assets_verified,
    })
    .map_err(|err| format!("failed to plan rr agent search intent: {err}"))?;

    let repository = &session.review_target.repository;
    let scope_key = format!("repo:{repository}");
    let semantic_candidates = if semantic_assets_verified && search_plan.retrieval_strategy.semantic
    {
        let mut embedder = build_search_semantic_embedder(store);
        store
            .generate_semantic_candidates(
                &scope_key,
                repository,
                &search_request.query_text,
                &mut embedder,
                25,
            )
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let lookup = store
        .prior_review_lookup(PriorReviewLookupQuery {
            scope_key: &scope_key,
            repository,
            query_text: &search_request.query_text,
            limit: 25,
            include_tentative_candidates: search_plan.includes_tentative_candidates(),
            allow_project_scope: false,
            allow_org_scope: false,
            semantic_assets_verified,
            semantic_candidates,
        })
        .map_err(|err| format!("failed to run rr agent prior-review lookup: {err}"))?;
    let retrieval_mode = retrieval_mode_label(&lookup.mode).to_owned();

    Ok(Some(WorkerSearchMemoryResponse {
        requested_query_mode: search_plan
            .query_plan
            .requested_query_mode
            .as_str()
            .to_owned(),
        resolved_query_mode: search_plan
            .query_plan
            .resolved_query_mode
            .as_str()
            .to_owned(),
        search_plan: search_plan.clone(),
        retrieval_mode: retrieval_mode.clone(),
        degraded_flags: lookup.degraded_reasons.clone(),
        promoted_memory: if search_plan.allows_retrieval_class(SearchRetrievalClass::PromotedMemory)
        {
            lookup
                .promoted_memory
                .iter()
                .map(|hit| {
                    worker_recall_from_memory_hit(
                        hit,
                        search_plan.query_plan.requested_query_mode.as_str(),
                        search_plan.query_plan.resolved_query_mode.as_str(),
                        &retrieval_mode,
                        &lookup.scope_bucket,
                        &lookup.degraded_reasons,
                        "promoted_memory",
                        &search_request.anchor_hints,
                    )
                })
                .collect()
        } else {
            Vec::new()
        },
        tentative_candidates: if search_plan
            .allows_retrieval_class(SearchRetrievalClass::TentativeCandidates)
        {
            lookup
                .tentative_candidates
                .iter()
                .map(|hit| {
                    worker_recall_from_memory_hit(
                        hit,
                        search_plan.query_plan.requested_query_mode.as_str(),
                        search_plan.query_plan.resolved_query_mode.as_str(),
                        &retrieval_mode,
                        &lookup.scope_bucket,
                        &lookup.degraded_reasons,
                        "tentative_candidates",
                        &search_request.anchor_hints,
                    )
                })
                .collect()
        } else {
            Vec::new()
        },
        evidence_hits: if search_plan.allows_retrieval_class(SearchRetrievalClass::EvidenceHits) {
            lookup
                .evidence_hits
                .iter()
                .map(|hit| {
                    worker_recall_from_evidence_hit(
                        hit,
                        search_plan.query_plan.requested_query_mode.as_str(),
                        search_plan.query_plan.resolved_query_mode.as_str(),
                        &retrieval_mode,
                        &lookup.scope_bucket,
                        &lookup.degraded_reasons,
                        &search_request.anchor_hints,
                    )
                })
                .collect()
        } else {
            Vec::new()
        },
    }))
}

fn finding_binds_to_task(
    finding: &roger_storage::MaterializedFindingRecord,
    task: &ReviewTask,
) -> bool {
    finding.session_id == task.review_session_id
        && finding
            .last_seen_run_id
            .as_deref()
            .unwrap_or(finding.first_run_id.as_str())
            == task.review_run_id
}

fn worker_evidence_location_from_record(
    record: &roger_storage::CodeEvidenceLocationRecord,
) -> Option<WorkerEvidenceLocation> {
    let artifact_id = record.excerpt_artifact_id.clone()?;
    Some(WorkerEvidenceLocation {
        artifact_id,
        repo_rel_path: Some(record.repo_rel_path.clone()),
        start_line: u32::try_from(record.start_line).ok(),
        end_line: record.end_line.and_then(|value| u32::try_from(value).ok()),
        evidence_role: Some(record.evidence_role.clone()),
    })
}

fn build_agent_finding_detail(
    store: &RogerStore,
    task: &ReviewTask,
    request: &WorkerOperationRequestEnvelope,
) -> Result<Option<WorkerFindingDetail>, String> {
    let Some(payload) = request.payload.clone() else {
        return Ok(None);
    };
    let Ok(detail_request) = serde_json::from_value::<WorkerFindingDetailRequest>(payload) else {
        return Ok(None);
    };

    let finding = store
        .materialized_finding(&detail_request.finding_id)
        .map_err(|err| format!("failed to load finding detail for rr agent: {err}"))?;
    let Some(finding) = finding else {
        return Err(format!(
            "finding '{}' was not found in the Roger store",
            detail_request.finding_id
        ));
    };
    if !finding_binds_to_task(&finding, task) {
        return Err(format!(
            "finding '{}' is outside the bound rr agent session/run",
            detail_request.finding_id
        ));
    }

    let evidence_locations = store
        .code_evidence_locations_for_finding(&detail_request.finding_id)
        .map_err(|err| format!("failed to load code evidence locations for rr agent: {err}"))?
        .iter()
        .filter_map(worker_evidence_location_from_record)
        .collect();

    Ok(Some(WorkerFindingDetail {
        finding: finding_summary_from_record(&finding),
        evidence_locations,
        clarification_ids: Vec::new(),
        outbound_draft_ids: Vec::new(),
    }))
}

fn build_agent_artifact_excerpt(
    store: &RogerStore,
    request: &WorkerOperationRequestEnvelope,
) -> Result<Option<WorkerArtifactExcerpt>, String> {
    const MAX_EXCERPT_BYTES: usize = 2048;

    let Some(payload) = request.payload.clone() else {
        return Ok(None);
    };
    let Ok(excerpt_request) = serde_json::from_value::<WorkerArtifactExcerptRequest>(payload)
    else {
        return Ok(None);
    };

    let bytes = store
        .artifact_bytes(&excerpt_request.artifact_id)
        .map_err(|err| format!("failed to load rr agent artifact excerpt: {err}"))?;
    let excerpt_bytes = if bytes.len() > MAX_EXCERPT_BYTES {
        &bytes[..MAX_EXCERPT_BYTES]
    } else {
        &bytes[..]
    };

    Ok(Some(WorkerArtifactExcerpt {
        artifact_id: excerpt_request.artifact_id,
        excerpt: String::from_utf8_lossy(excerpt_bytes).to_string(),
        digest: Some(sha256_hex(&bytes)),
        truncated: bytes.len() > excerpt_bytes.len(),
        byte_count: bytes.len(),
    }))
}

fn build_agent_gateway_snapshot(
    store: &RogerStore,
    session: &roger_storage::ReviewSessionRecord,
    task: &ReviewTask,
    request: &WorkerOperationRequestEnvelope,
    findings: &[WorkerFindingSummary],
) -> Result<WorkerGatewaySnapshot, String> {
    let mut snapshot = WorkerGatewaySnapshot::default();

    let Ok(operation) = WorkerOperation::parse(&request.operation) else {
        return Ok(snapshot);
    };

    match operation {
        WorkerOperation::GetStatus => {
            snapshot.status = Some(build_agent_status_snapshot(
                store,
                session,
                task,
                findings.len(),
            )?);
        }
        WorkerOperation::SearchMemory => {
            snapshot.search_memory_response =
                build_agent_search_response(store, session, task, request)?;
        }
        WorkerOperation::ListFindings => {
            snapshot.findings = Some(WorkerFindingListResponse {
                items: findings.to_vec(),
            });
        }
        WorkerOperation::GetFindingDetail => {
            if let Some(detail) = build_agent_finding_detail(store, task, request)? {
                snapshot.finding_details.push(detail);
            }
        }
        WorkerOperation::GetArtifactExcerpt => {
            if let Some(excerpt) = build_agent_artifact_excerpt(store, request)? {
                snapshot.artifact_excerpts.push(excerpt);
            }
        }
        WorkerOperation::GetReviewContext
        | WorkerOperation::SubmitStageResult
        | WorkerOperation::RequestClarification
        | WorkerOperation::RequestMemoryReview
        | WorkerOperation::ProposeFollowUp => {}
    }

    Ok(snapshot)
}

fn agent_command_response(envelope: AgentTransportResponseEnvelope) -> CommandResponse {
    let outcome = match envelope.status {
        AgentTransportResponseStatus::Succeeded => OutcomeKind::Complete,
        AgentTransportResponseStatus::Denied => OutcomeKind::Blocked,
        AgentTransportResponseStatus::Error => OutcomeKind::Error,
    };
    CommandResponse {
        outcome,
        data: serde_json::to_value(envelope).expect("serialize agent transport response"),
        warnings: Vec::new(),
        repair_actions: Vec::new(),
        message: "rr agent request completed".to_owned(),
    }
}

fn agent_error_response(
    code: AgentTransportErrorCode,
    message: impl Into<String>,
) -> CommandResponse {
    agent_command_response(AgentTransportResponseEnvelope::error(code, message))
}

fn handle_agent(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    let task_path = parsed
        .agent_task_file
        .as_deref()
        .expect("agent task file validated during parse");
    let task: ReviewTask = match read_json_file(task_path, "ReviewTask file") {
        Ok(task) => task,
        Err(message) => {
            return agent_error_response(AgentTransportErrorCode::PayloadInvalid, message);
        }
    };

    let capability_profile = match parsed.agent_capability_file.as_deref() {
        Some(path) => match read_json_file(path, "WorkerCapabilityProfile file") {
            Ok(profile) => effective_agent_capability_profile(Some(profile)),
            Err(message) => {
                return agent_error_response(AgentTransportErrorCode::PayloadInvalid, message);
            }
        },
        None => built_in_agent_capability_profile(),
    };

    let Some(expected_operation) = parsed.agent_operation.as_deref() else {
        return agent_error_response(
            AgentTransportErrorCode::ValidationFailed,
            "rr agent operation is missing",
        );
    };

    // Inline base64 payload takes precedence: the write-free submission path
    // for policy-sandboxed workers (base64 text carries none of the shell
    // metacharacters the review_readonly allowlist rejects), so
    // worker.submit_stage_result works without any file-creation capability.
    let request: WorkerOperationRequestEnvelope = if let Some(encoded) =
        parsed.agent_request_b64.as_deref()
    {
        let decoded = match BASE64_STANDARD.decode(encoded.trim()) {
            Ok(bytes) => bytes,
            Err(err) => {
                return agent_error_response(
                    AgentTransportErrorCode::PayloadInvalid,
                    format!("failed to decode --request-b64 payload: {err}"),
                );
            }
        };
        match serde_json::from_slice(&decoded) {
            Ok(request) => request,
            Err(err) => {
                return agent_error_response(
                    AgentTransportErrorCode::PayloadInvalid,
                    format!("failed to parse --request-b64 payload as JSON: {err}"),
                );
            }
        }
    } else {
        match read_json_bytes_from_stdin_or_file(
            parsed.agent_request_file.as_ref(),
            "rr agent request envelope",
        ) {
            Ok(bytes) => match serde_json::from_slice(&bytes) {
                Ok(request) => request,
                Err(err) => {
                    return agent_error_response(
                        AgentTransportErrorCode::PayloadInvalid,
                        format!("failed to parse rr agent request envelope as JSON: {err}"),
                    );
                }
            },
            Err(message) => {
                // No explicit --request-file and empty/terminal stdin: for a
                // self-serviceable read operation, bind directly from the task file
                // (the seeded `rr agent <op> --task-file <path>` form). Operations
                // that require a payload still demand an explicit request envelope.
                if parsed.agent_request_file.is_none()
                    && agent_operation_is_self_serviceable(expected_operation)
                {
                    default_agent_request_from_task(&task, expected_operation)
                } else {
                    return agent_error_response(AgentTransportErrorCode::PayloadMissing, message);
                }
            }
        }
    };

    if request.operation != expected_operation {
        return agent_error_response(
            AgentTransportErrorCode::ValidationFailed,
            format!(
                "request operation '{}' does not match rr agent operation '{}'",
                request.operation, expected_operation
            ),
        );
    }

    let store = match RogerStore::open(&runtime.store_root) {
        Ok(store) => store,
        Err(err) => {
            return agent_error_response(
                AgentTransportErrorCode::ValidationFailed,
                format!("failed to open Roger store for rr agent: {err}"),
            );
        }
    };
    let session = match store.review_session(&task.review_session_id) {
        Ok(Some(session)) => session,
        Ok(None) => {
            return agent_error_response(
                AgentTransportErrorCode::ValidationFailed,
                format!(
                    "review session '{}' is not present in the Roger store",
                    task.review_session_id
                ),
            );
        }
        Err(err) => {
            return agent_error_response(
                AgentTransportErrorCode::ValidationFailed,
                format!("failed to load rr agent review session: {err}"),
            );
        }
    };
    let findings = match load_agent_findings(&store, &task) {
        Ok(findings) => findings,
        Err(message) => {
            return agent_error_response(AgentTransportErrorCode::ValidationFailed, message);
        }
    };
    let worker_context = match parsed.agent_context_file.as_deref() {
        Some(path) => match read_json_file(path, "WorkerContextPacket file") {
            Ok(context) => context,
            Err(message) => {
                return agent_error_response(AgentTransportErrorCode::PayloadInvalid, message);
            }
        },
        None => match synthesize_agent_context(&store, &session, &task, findings.clone()) {
            Ok(context) => context,
            Err(message) => {
                return agent_error_response(AgentTransportErrorCode::ValidationFailed, message);
            }
        },
    };
    let gateway_snapshot =
        match build_agent_gateway_snapshot(&store, &session, &task, &request, &findings) {
            Ok(snapshot) => snapshot,
            Err(message) => {
                return agent_error_response(AgentTransportErrorCode::ValidationFailed, message);
            }
        };

    let envelope = execute_agent_transport_request(&AgentTransportRequestEnvelope {
        schema_id: AGENT_TRANSPORT_REQUEST_SCHEMA_V1.to_owned(),
        review_task: task.clone(),
        worker_context,
        capability_profile,
        operation_request: request.clone(),
        gateway_snapshot,
    });

    // An accepted stage result must be durable before Roger reports
    // acceptance: record the audit row and materialize canonical Finding rows
    // from the validated findings pack so readback and outbound drafting see
    // the worker's output.
    if envelope.status == AgentTransportResponseStatus::Succeeded
        && request.operation == "worker.submit_stage_result"
        && let Err(message) = persist_accepted_stage_result(&store, &session, &task, &request)
    {
        return agent_error_response(AgentTransportErrorCode::ValidationFailed, message);
    }

    // A validated memory-review proposal is now durable: persist a pending
    // review request row so the previously write-only worker proposal becomes
    // auditable and operator-resolvable. The transport echo shape stays stable;
    // the persisted id is surfaced additively on the command envelope.
    let mut persisted_review_request_id: Option<String> = None;
    if envelope.status == AgentTransportResponseStatus::Succeeded
        && request.operation == "worker.request_memory_review"
    {
        match request
            .payload
            .clone()
            .ok_or_else(|| "memory review request is missing its payload".to_owned())
            .and_then(|payload| {
                serde_json::from_value::<WorkerMemoryReviewRequest>(payload)
                    .map_err(|err| format!("failed to decode memory review request: {err}"))
            }) {
            Ok(review_request) => {
                let scope_key = format!("repo:{}", session.review_target.repository);
                let review_run_id = store
                    .latest_review_run(&session.id)
                    .ok()
                    .flatten()
                    .map(|run| run.id);
                let (persisted, _skipped) = persist_worker_memory_reviews(
                    &store,
                    &scope_key,
                    &session.id,
                    review_run_id.as_deref(),
                    std::slice::from_ref(&review_request),
                    MemoryReviewSource::Worker,
                );
                persisted_review_request_id = persisted.into_iter().next();
            }
            Err(message) => {
                return agent_error_response(AgentTransportErrorCode::ValidationFailed, message);
            }
        }
    }

    let mut response = agent_command_response(envelope);
    if let Some(id) = persisted_review_request_id
        && let Some(object) = response.data.as_object_mut()
    {
        object.insert("persisted_memory_review_request_id".to_owned(), json!(id));
    }
    response
}

fn memory_review_request_to_json(record: MemoryReviewRequestRecord) -> Value {
    json!({
        "id": record.id,
        "review_session_id": record.review_session_id,
        "review_run_id": record.review_run_id,
        "source": record.source,
        "request_kind": record.request_kind,
        "statement": record.statement,
        "normalized_key": record.normalized_key,
        "scope_key": record.scope_key,
        "memory_class": record.memory_class,
        "rationale": record.rationale,
        "status": record.status,
        "created_at": record.created_at,
        "resolved_at": record.resolved_at,
        "resolution_actor": record.resolution_actor,
        "resulting_memory_item_id": record.resulting_memory_item_id,
    })
}

/// Persist worker memory-review proposals as durable pending review requests.
/// Defensive: entries with an empty query are skipped (returned in the skipped
/// list) rather than aborting the whole ingest. All proposals map to the
/// `promote` request kind in this slice; the statement is the proposal query and
/// the dedup key is its normalized form.
fn persist_worker_memory_reviews(
    store: &RogerStore,
    scope_key: &str,
    session_id: &str,
    review_run_id: Option<&str>,
    requests: &[WorkerMemoryReviewRequest],
    source: MemoryReviewSource,
) -> (Vec<String>, Vec<String>) {
    let mut persisted = Vec::new();
    let mut skipped = Vec::new();
    for request in requests {
        let statement = request.query.trim();
        if statement.is_empty() {
            skipped.push(format!("{}: empty query", request.id));
            continue;
        }
        let normalized_key = normalize_memory_key(statement);
        if normalized_key.is_empty() {
            skipped.push(format!("{}: empty normalized key", request.id));
            continue;
        }
        match store.create_memory_review_request(CreateMemoryReviewRequest {
            review_session_id: session_id,
            review_run_id,
            source,
            request_kind: MemoryReviewRequestKind::Promote,
            statement,
            normalized_key: &normalized_key,
            scope_key,
            memory_class: "semantic",
            rationale: request.rationale.as_deref(),
            external_ref: Some(&request.id),
        }) {
            Ok(record) => persisted.push(record.id),
            Err(err) => skipped.push(format!("{}: {err}", request.id)),
        }
    }
    (persisted, skipped)
}

/// Answer a companion-tier bounded status probe from the browser extension.
///
/// Read-only by contract: resolve the newest session for the probe's
/// repo#pr from persisted state and report its attention state plus
/// freshness. States outside the extension's canonical decision-needing set
/// (or anything unresolvable) return an empty object so the extension
/// degrades honestly to launch-only mode instead of bluffing.
pub fn answer_bridge_status_probe(
    runtime: &CliRuntime,
    owner: &str,
    repo: &str,
    pr_number: u64,
) -> Value {
    let Ok(store) = RogerStore::open(&runtime.store_root) else {
        return json!({});
    };
    let repository = format!("{owner}/{repo}");
    let sessions = store
        .session_finder(SessionFinderQuery {
            repository: Some(repository),
            pull_request_number: Some(pr_number),
            attention_states: Vec::new(),
            limit: 6,
        })
        .unwrap_or_default();
    // Session existence is durable local truth, so the action model can rely
    // on it regardless of attention freshness: zero sessions means resume is
    // not a meaningful action for this PR.
    let session_summaries: Vec<Value> = sessions
        .iter()
        .take(5)
        .map(|session| {
            json!({
                "session_id": session.session_id,
                "provider": session.provider,
                "attention_state": session.attention_state,
                "updated_at": session.updated_at,
            })
        })
        .collect();
    let Some(session) = sessions.first() else {
        return json!({ "session_count": 0 });
    };
    let freshness_seconds = (time::now_ts() - session.updated_at).max(0);
    json!({
        "attention_state": session.attention_state,
        "freshness_seconds": freshness_seconds,
        "session_id": session.session_id,
        "provider": session.provider,
        "session_count": sessions.len(),
        "sessions": session_summaries,
    })
}

fn resolve_copilot_home() -> Option<PathBuf> {
    if let Ok(value) = std::env::var(session_copilot::COPILOT_HOME_ENV) {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }
    std::env::var("HOME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(|home| PathBuf::from(home).join(".copilot"))
}

fn persist_accepted_stage_result(
    store: &RogerStore,
    session: &ReviewSessionRecord,
    task: &ReviewTask,
    request: &WorkerOperationRequestEnvelope,
) -> Result<(), String> {
    let Some(payload) = request.payload.clone() else {
        return Err("accepted stage result is missing its payload".to_owned());
    };
    let mut result: WorkerStageResult = serde_json::from_value(payload)
        .map_err(|err| format!("failed to decode accepted stage result payload: {err}"))?;

    // The worker-supplied invocation id is unverified; the audit table only
    // links invocation rows Roger itself recorded, so drop unknown ids rather
    // than fail the whole acceptance on the foreign key.
    if let Some(invocation_id) = result.worker_invocation_id.clone() {
        let known = store
            .worker_invocations_for_run(&result.review_session_id, &result.review_run_id)
            .map_err(|err| format!("failed to load worker invocations: {err}"))?
            .iter()
            .any(|record| record.id == invocation_id);
        if !known {
            result.worker_invocation_id = None;
        }
    }

    store
        .record_worker_stage_result(CreateWorkerStageResult {
            result: &result,
            submitted_result_artifact_id: None,
            structured_findings_pack_artifact_id: None,
        })
        .map_err(|err| format!("failed to record accepted worker stage result: {err}"))?;

    let findings = result
        .structured_findings_pack
        .as_ref()
        .and_then(|pack| pack.get("findings"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut materialized_count = 0usize;
    for finding in &findings {
        let title = finding
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        if title.is_empty() {
            // Repair posture: an entry without a usable claim title cannot
            // become a canonical Finding; the raw pack stays in the audit row.
            continue;
        }
        let summary = finding
            .get("normalized_summary")
            .or_else(|| finding.get("summary"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        let severity = finding
            .get("severity")
            .and_then(Value::as_str)
            .unwrap_or("medium");
        let confidence = finding
            .get("confidence")
            .and_then(Value::as_str)
            .unwrap_or("medium");
        let primary_evidence_path = finding
            .get("code_evidence")
            .and_then(Value::as_array)
            .and_then(|entries| entries.first())
            .and_then(|entry| entry.get("repo_rel_path"))
            .and_then(Value::as_str);
        let fingerprint = finding
            .get("fingerprint")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| {
                derive_finding_fingerprint(
                    &session.review_target.repository,
                    session.review_target.pull_request_number,
                    title,
                    summary,
                    primary_evidence_path,
                )
            });
        let finding_id = next_id("finding");
        store
            .upsert_materialized_finding(CreateMaterializedFinding {
                id: &finding_id,
                session_id: &task.review_session_id,
                review_run_id: &task.review_run_id,
                stage: &result.stage,
                fingerprint: &fingerprint,
                title,
                normalized_summary: summary,
                severity,
                confidence,
                triage_state: "new",
                outbound_state: "not_drafted",
            })
            .map_err(|err| format!("failed to materialize finding from stage result: {err}"))?;
        materialized_count += 1;
    }

    // A stage result carrying memory-review proposals persists them as durable
    // pending review requests too (previously the accumulated JSON blob was
    // write-only). Defensive: malformed proposals are skipped, not fatal.
    if !result.memory_review_requests.is_empty() {
        let scope_key = format!("repo:{}", session.review_target.repository);
        persist_worker_memory_reviews(
            store,
            &scope_key,
            &session.id,
            Some(result.review_run_id.as_str()),
            &result.memory_review_requests,
            MemoryReviewSource::Worker,
        );
    }

    // A completed pass that materialized findings moves the session into the
    // canonical findings_ready attention state so operator surfaces (CLI
    // status, TUI, extension mirror) see that a decision is now waiting.
    if materialized_count > 0 && result.outcome == WorkerStageOutcome::Completed {
        let mut row_version = session.row_version;
        for attempt in 0..2 {
            match store.update_review_session_attention(
                &task.review_session_id,
                row_version,
                "findings_ready",
            ) {
                Ok(_) => break,
                Err(StorageError::Conflict { .. }) if attempt == 0 => {
                    row_version = store
                        .review_session(&task.review_session_id)
                        .map_err(|err| format!("failed to reload session for attention: {err}"))?
                        .ok_or_else(|| "session vanished during attention update".to_owned())?
                        .row_version;
                }
                Err(err) => {
                    return Err(format!(
                        "failed to mark session findings_ready after accepted stage result: {err}"
                    ));
                }
            }
        }
    }
    Ok(())
}

fn parse_supported_browser(value: &str) -> Result<SupportedBrowser, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "chrome" => Ok(SupportedBrowser::Chrome),
        "edge" => Ok(SupportedBrowser::Edge),
        "brave" => Ok(SupportedBrowser::Brave),
        other => Err(format!(
            "unsupported --browser value: {other} (expected edge, chrome, or brave)"
        )),
    }
}

fn supported_browser_label(browser: SupportedBrowser) -> &'static str {
    match browser {
        SupportedBrowser::Chrome => "chrome",
        SupportedBrowser::Edge => "edge",
        SupportedBrowser::Brave => "brave",
    }
}

fn extension_id_registry_path(store_root: &Path) -> PathBuf {
    store_root.join("bridge/extension-id")
}

fn init_metadata_marker_path(store_root: &Path) -> PathBuf {
    store_root.join("bootstrap/init-marker.v1.json")
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LegacyOpencodeConfigIssue {
    legacy_path: PathBuf,
    canonical_path: PathBuf,
}

fn runtime_targets_opencode_binary(binary_path: &str) -> bool {
    if binary_path.eq_ignore_ascii_case(DEFAULT_OPENCODE_BIN) {
        return true;
    }

    Path::new(binary_path)
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.eq_ignore_ascii_case(DEFAULT_OPENCODE_BIN))
        .unwrap_or(false)
}

fn detect_legacy_opencode_config_issue(binary_path: &str) -> Option<LegacyOpencodeConfigIssue> {
    if !runtime_targets_opencode_binary(binary_path) {
        return None;
    }

    let home_dir = std::env::var_os("HOME").map(PathBuf::from)?;
    let legacy_path = home_dir.join(".opencode/opencode.json");
    let canonical_path = home_dir.join(".config/opencode/config.json");
    let payload: Value = serde_json::from_str(&fs::read_to_string(&legacy_path).ok()?).ok()?;
    if !payload
        .as_object()
        .map(|object| object.contains_key("mcpServers"))
        .unwrap_or(false)
    {
        return None;
    }

    Some(LegacyOpencodeConfigIssue {
        legacy_path,
        canonical_path,
    })
}

fn opencode_legacy_config_warning(issue: &LegacyOpencodeConfigIssue) -> String {
    format!(
        "legacy OpenCode config detected at {}; opencode expects the current top-level 'mcp' schema and may otherwise emit 'Unrecognized key: mcpServers' during Roger-driven reopen/return flows",
        issue.legacy_path.display()
    )
}

fn opencode_legacy_config_repair_actions(issue: &LegacyOpencodeConfigIssue) -> Vec<String> {
    vec![
        format!(
            "move {} aside or migrate its top-level 'mcpServers' entries into {} under the current 'mcp' key",
            issue.legacy_path.display(),
            issue.canonical_path.display()
        ),
        "re-run rr doctor --provider opencode --robot after cleaning the legacy OpenCode config"
            .to_owned(),
    ]
}

fn opencode_legacy_config_guidance(binary_path: &str) -> Option<(String, Vec<String>)> {
    let issue = detect_legacy_opencode_config_issue(binary_path)?;
    Some((
        opencode_legacy_config_warning(&issue),
        opencode_legacy_config_repair_actions(&issue),
    ))
}

fn normalize_extension_id(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn discover_explicit_extension_id(parsed: &ParsedArgs) -> Option<(String, &'static str)> {
    parsed
        .bridge_extension_id
        .as_deref()
        .and_then(normalize_extension_id)
        .map(|value| (value, "explicit_flag"))
}

fn discover_stored_or_env_extension_id(runtime: &CliRuntime) -> Option<(String, &'static str)> {
    let registry_path = extension_id_registry_path(&runtime.store_root);
    if let Ok(contents) = fs::read_to_string(&registry_path) {
        if let Some(value) = normalize_extension_id(&contents) {
            return Some((value, "store_registry"));
        }
    }

    if let Ok(value) = std::env::var("RR_BRIDGE_EXTENSION_ID") {
        if let Some(value) = normalize_extension_id(&value) {
            return Some((value, "env_rr_bridge_extension_id"));
        }
    }

    None
}

fn discover_extension_id(
    parsed: &ParsedArgs,
    runtime: &CliRuntime,
) -> Option<(String, &'static str)> {
    discover_explicit_extension_id(parsed).or_else(|| discover_stored_or_env_extension_id(runtime))
}

fn extension_id_looks_valid(value: &str) -> bool {
    value.len() == 32 && value.chars().all(|ch| ch.is_ascii_lowercase())
}

fn verify_directory_write_access(path: &Path, label: &str) -> Result<Value, String> {
    let metadata = fs::metadata(path)
        .map_err(|err| format!("failed to stat {} at {}: {err}", label, path.display()))?;
    let read_only = metadata.permissions().readonly();
    if read_only {
        return Err(format!("{label} at {} is read-only", path.display()));
    }

    let probe_path = path.join(format!(
        ".rr-init-write-probe-{}-{}",
        std::process::id(),
        next_id("probe")
    ));
    fs::write(&probe_path, b"roger-init-write-probe").map_err(|err| {
        format!(
            "failed to write {} probe at {}: {err}",
            label,
            probe_path.display()
        )
    })?;
    fs::remove_file(&probe_path).map_err(|err| {
        format!(
            "failed to remove {} probe at {}: {err}",
            label,
            probe_path.display()
        )
    })?;

    Ok(json!({
        "label": label,
        "path": path.to_string_lossy(),
        "read_only": read_only,
        "write_probe": "ok",
    }))
}

fn handle_init(runtime: &CliRuntime) -> CommandResponse {
    let layout = StorageLayout::under(&runtime.store_root);
    let marker_path = init_metadata_marker_path(&runtime.store_root);
    let marker_parent = match marker_path.parent() {
        Some(parent) => parent.to_path_buf(),
        None => {
            return error_response(format!(
                "failed to resolve init marker parent path for {}",
                marker_path.display()
            ));
        }
    };

    let path_specs = [
        ("store_root", layout.root.clone()),
        ("db_path", layout.db_path.clone()),
        ("artifact_root", layout.artifact_root.clone()),
        ("sidecar_root", layout.sidecar_root.clone()),
        ("bootstrap_root", marker_parent.clone()),
        ("metadata_marker", marker_path.clone()),
    ];
    let existed_before: HashMap<String, bool> = path_specs
        .iter()
        .map(|(label, path)| (label.to_string(), path.exists()))
        .collect();

    let store = match open_store_or_response(runtime, "rr init") {
        Ok(store) => store,
        Err(response) => return response,
    };
    let schema_version = match store.schema_version() {
        Ok(version) => version,
        Err(err) => return error_response(format!("failed to read store schema version: {err}")),
    };

    if let Err(err) = fs::create_dir_all(&marker_parent) {
        return error_response(format!(
            "failed to create bootstrap marker directory at {}: {err}",
            marker_parent.display()
        ));
    }

    let first_initialized_at = fs::read_to_string(&marker_path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .and_then(|value| value.get("first_initialized_at").and_then(Value::as_i64))
        .unwrap_or_else(time::now_ts);
    let initialized_at = time::now_ts();

    let write_checks = match [
        ("store_root", layout.root.as_path()),
        ("artifact_root", layout.artifact_root.as_path()),
        ("sidecar_root", layout.sidecar_root.as_path()),
        ("bootstrap_root", marker_parent.as_path()),
    ]
    .iter()
    .map(|(label, path)| verify_directory_write_access(path, label))
    .collect::<Result<Vec<_>, _>>()
    {
        Ok(checks) => checks,
        Err(err) => {
            return blocked_response(
                format!("rr init blocked while verifying local write permissions: {err}"),
                vec![
                    "fix local filesystem permissions for the Roger store root, then rerun rr init"
                        .to_owned(),
                    "if filesystem drift persists, run rr doctor for bounded preflight guidance"
                        .to_owned(),
                ],
                json!({
                    "reason_code": "store_permissions_unverified",
                    "store_root": layout.root.to_string_lossy(),
                    "error": err,
                }),
            );
        }
    };

    let marker_payload = json!({
        "schema": "rr.init.marker.v1",
        "first_initialized_at": first_initialized_at,
        "last_initialized_at": initialized_at,
        "release_version": option_env!("ROGER_RELEASE_VERSION").unwrap_or("local-unpublished"),
        "package_version": env!("CARGO_PKG_VERSION"),
        "store_layout": {
            "root": layout.root.to_string_lossy(),
            "db_path": layout.db_path.to_string_lossy(),
            "artifact_root": layout.artifact_root.to_string_lossy(),
            "sidecar_root": layout.sidecar_root.to_string_lossy(),
        },
        "schema_version": schema_version,
    });
    let marker_bytes = match serde_json::to_vec_pretty(&marker_payload) {
        Ok(bytes) => bytes,
        Err(err) => return error_response(format!("failed to encode init metadata marker: {err}")),
    };
    if let Err(err) = fs::write(&marker_path, marker_bytes) {
        return error_response(format!(
            "failed to write init metadata marker at {}: {err}",
            marker_path.display()
        ));
    }

    let mut created_paths = Vec::new();
    let mut existing_paths = Vec::new();
    for (label, path) in path_specs {
        let existed = existed_before.get(label).copied().unwrap_or(false);
        let entry = json!({
            "label": label,
            "path": path.to_string_lossy(),
        });
        if existed {
            existing_paths.push(entry);
        } else {
            created_paths.push(entry);
        }
    }

    let already_initialized = existed_before.get("db_path").copied().unwrap_or(false);
    CommandResponse {
        outcome: OutcomeKind::Complete,
        data: json!({
            "store_root": layout.root.to_string_lossy(),
            "already_initialized": already_initialized,
            "schema_version": schema_version,
            "created_paths": created_paths,
            "existing_paths": existing_paths,
            "write_checks": write_checks,
            "metadata_marker": {
                "path": marker_path.to_string_lossy(),
                "schema": "rr.init.marker.v1",
                "first_initialized_at": first_initialized_at,
                "last_initialized_at": initialized_at,
                "release_version": option_env!("ROGER_RELEASE_VERSION").unwrap_or("local-unpublished"),
                "package_version": env!("CARGO_PKG_VERSION"),
            },
            "provider_follow_up": {
                "status": "not_checked_by_init",
                "doctor_surface_status": "available",
                "auth_or_install_verified": false,
                "live_review_providers": runtime_supported_review_providers(runtime),
                "planned_not_live_providers": runtime_planned_not_live_review_providers(runtime),
                "guidance": [
                    "rr init only bootstraps Roger-owned local state; provider auth/install is not verified here",
                    "run rr doctor --provider <name> for local and provider preflight checks",
                    "run rr review/rr resume to perform provider launch verification and follow surfaced repair guidance on failure",
                ],
            },
        }),
        warnings: Vec::new(),
        repair_actions: Vec::new(),
        message: if already_initialized {
            "rr init verified existing Roger local bootstrap state".to_owned()
        } else {
            "rr init created Roger local bootstrap state".to_owned()
        },
    }
}

fn resolve_provider_binary_path(configured_binary_path: &str) -> Option<PathBuf> {
    let configured = PathBuf::from(configured_binary_path);
    if configured.components().count() > 1 || configured_binary_path.contains('\\') {
        if configured.is_file() {
            return Some(configured);
        }
        if cfg!(windows) && configured.extension().is_none() {
            let with_exe = configured.with_extension("exe");
            if with_exe.is_file() {
                return Some(with_exe);
            }
        }
        return None;
    }

    let mut binary_names = vec![configured_binary_path.to_owned()];
    if cfg!(windows)
        && !configured_binary_path
            .to_ascii_lowercase()
            .ends_with(".exe")
    {
        binary_names.push(format!("{configured_binary_path}.exe"));
    }

    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for binary_name in &binary_names {
            let candidate = dir.join(binary_name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn handle_doctor(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    let resolved = cli_config::resolved_cli_config(&runtime.cwd);
    let baseline = match resolved.routine_surface_baseline(Some(parsed.provider.as_str())) {
        Ok(baseline) => baseline,
        Err(err) => {
            // Only recommend providers that rr doctor actually services. pi-agent
            // resolves but is not_supported (supports.doctor=false), so following
            // a recommendation to rerun doctor against it would immediately fail
            // closed; do not list it as a doctor-serviceable target.
            return blocked_response(
                format!(
                    "rr doctor cannot resolve provider '{}': {}",
                    parsed.provider, err.message
                ),
                vec![
                    "rerun rr doctor with one of: opencode, codex, gemini, claude, copilot"
                        .to_owned(),
                ],
                json!({
                    "reason_code": err.reason_code,
                    "provider": parsed.provider,
                    "supported_providers": ["opencode", "codex", "gemini", "claude", "copilot"],
                    "non_live_providers": ["pi-agent"],
                }),
            );
        }
    };

    let provider = baseline.provider.clone();
    let provider_capability = runtime_provider_capability(runtime, parsed.provider.as_str());
    let routine_surface =
        runtime_routine_surface_projection(runtime, parsed.provider.as_str(), None).unwrap_or_else(
            || {
                routine_surface_with_worktree_root(
                    routine_surface_baseline_projection(&baseline),
                    &runtime.cwd,
                    None,
                )
            },
        );
    let provider_status = provider_capability["status"]
        .as_str()
        .unwrap_or(provider.status.as_str());
    let provider_support_tier = provider_capability["support_tier"]
        .as_str()
        .unwrap_or(provider.support_tier.as_str());
    let provider_supports_doctor = provider_capability["supports"]["doctor"]
        .as_bool()
        .unwrap_or(provider.supports.doctor);
    let layout = StorageLayout::under(&runtime.store_root);
    let workspace_root = find_workspace_root(&runtime.cwd).unwrap_or_else(|| runtime.cwd.clone());
    let mut checks: Vec<Value> = Vec::new();
    let mut repair_actions: BTreeMap<String, ()> = BTreeMap::new();

    let mut push_check =
        |id: &str, label: &str, status: &str, reason_code: Option<&str>, details: Value| {
            checks.push(json!({
                "id": id,
                "label": label,
                "status": status,
                "verified": status == "verified",
                "deferred": status == "deferred",
                "reason_code": reason_code,
                "details": details,
            }));
        };

    if layout.root.is_dir() {
        push_check(
            "store_root_present",
            "Roger store root exists",
            "verified",
            None,
            json!({"path": layout.root.to_string_lossy()}),
        );
    } else {
        // The store auto-bootstraps on first use of any store-backed command;
        // a missing store on a fresh machine is expected, not an error.
        push_check(
            "store_root_present",
            "Roger store root exists",
            "deferred",
            Some("store_auto_bootstrap_pending"),
            json!({
                "path": layout.root.to_string_lossy(),
                "guidance": "Roger creates the store automatically on first use; run any review command (or rr init explicitly) to bootstrap now",
            }),
        );
    }

    let mut schema_version: Option<i64> = None;
    if layout.db_path.is_file() {
        match SqliteConnection::open_with_flags(&layout.db_path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
            Ok(conn) => {
                match conn.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0)) {
                    Ok(version) => {
                        schema_version = Some(version);
                        push_check(
                            "store_db_readable",
                            "Roger store database is readable",
                            "verified",
                            None,
                            json!({
                                "path": layout.db_path.to_string_lossy(),
                                "schema_version": version,
                            }),
                        );
                    }
                    Err(err) => {
                        push_check(
                            "store_db_readable",
                            "Roger store database is readable",
                            "blocked",
                            Some("store_db_schema_probe_failed"),
                            json!({
                                "path": layout.db_path.to_string_lossy(),
                                "error": err.to_string(),
                            }),
                        );
                        repair_actions.insert(
                            "run rr init to repair or regenerate local Roger storage".to_owned(),
                            (),
                        );
                    }
                }
            }
            Err(err) => {
                push_check(
                    "store_db_readable",
                    "Roger store database is readable",
                    "blocked",
                    Some("store_db_open_failed"),
                    json!({
                        "path": layout.db_path.to_string_lossy(),
                        "error": err.to_string(),
                    }),
                );
                repair_actions.insert(
                    "run rr init to repair or regenerate local Roger storage".to_owned(),
                    (),
                );
            }
        }
    } else {
        push_check(
            "store_db_readable",
            "Roger store database is readable",
            "deferred",
            Some("store_auto_bootstrap_pending"),
            json!({
                "path": layout.db_path.to_string_lossy(),
                "guidance": "Roger creates the store database automatically on first use; run any review command (or rr init explicitly) to bootstrap now",
            }),
        );
    }

    for (id, label, path) in [
        (
            "artifact_root_present",
            "Roger artifact directory exists",
            layout.artifact_root.as_path(),
        ),
        (
            "sidecar_root_present",
            "Roger sidecar directory exists",
            layout.sidecar_root.as_path(),
        ),
    ] {
        if path.is_dir() {
            push_check(
                id,
                label,
                "verified",
                None,
                json!({"path": path.to_string_lossy()}),
            );
        } else if !layout.root.is_dir() {
            // Fresh machine: the whole layout materializes on first use.
            push_check(
                id,
                label,
                "deferred",
                Some("store_auto_bootstrap_pending"),
                json!({"path": path.to_string_lossy()}),
            );
        } else {
            push_check(
                id,
                label,
                "blocked",
                Some("store_layout_incomplete"),
                json!({"path": path.to_string_lossy()}),
            );
            repair_actions.insert(
                "run rr init to repair local Roger directory layout".to_owned(),
                (),
            );
        }
    }

    if let Some(worktree_root) = infer_git_worktree_root(&runtime.cwd) {
        push_check(
            "git_worktree_context",
            "current cwd is inside a git worktree",
            "verified",
            None,
            json!({
                "cwd": runtime.cwd.to_string_lossy(),
                "worktree_root": worktree_root,
            }),
        );
    } else {
        push_check(
            "git_worktree_context",
            "current cwd is inside a git worktree",
            "blocked",
            Some("not_in_git_worktree"),
            json!({
                "cwd": runtime.cwd.to_string_lossy(),
            }),
        );
        repair_actions.insert(
            "run rr doctor from inside the target repository or worktree checkout".to_owned(),
            (),
        );
    }

    if !provider_supports_doctor {
        push_check(
            "provider_doctor_support",
            "provider has doctor support in this slice",
            "blocked",
            Some("provider_doctor_not_supported"),
            json!({
                "provider": provider.provider,
                "status": provider_status,
            }),
        );
        repair_actions.insert(
            "rerun rr doctor with one of: opencode, codex, gemini, claude, copilot".to_owned(),
            (),
        );
    } else {
        push_check(
            "provider_doctor_support",
            "provider has doctor support in this slice",
            "verified",
            None,
            json!({
                "provider": provider.provider,
                "status": provider_status,
                "support_tier": provider_support_tier,
            }),
        );
    }

    if provider_status == COPILOT_FEATURE_GATED_DISABLED_STATUS {
        // Copilot is a documented feature-gated tier-b provider that is
        // disabled-but-enableable. Do not steer the operator to a different
        // provider; name the documented enable step instead.
        push_check(
            "provider_admission_state",
            "provider is admitted as a live review lane",
            "blocked",
            Some("provider_feature_gate_disabled"),
            json!({
                "provider": provider.provider,
                "status": provider_status,
                "support_tier": provider_support_tier,
                "policy_profile_id": provider.policy_profile.id,
                "feature_gate_env": session_copilot::ENV_COPILOT_ADMISSION_GATE,
            }),
        );
        repair_actions.insert(
            format!(
                "enable the documented Copilot feature gate with {}=1 (feature-gated bounded tier-b support), then rerun rr doctor --provider copilot",
                session_copilot::ENV_COPILOT_ADMISSION_GATE
            ),
            (),
        );
    } else if provider_status == "planned_not_live" {
        push_check(
            "provider_admission_state",
            "provider is admitted as a live review lane",
            "blocked",
            Some("provider_not_live"),
            json!({
                "provider": provider.provider,
                "status": provider_status,
                "policy_profile_id": provider.policy_profile.id,
            }),
        );
        repair_actions.insert(
            "use a live lane for review work: rr review --provider opencode|codex|gemini|claude --pr <number>"
                .to_owned(),
            (),
        );
    } else if provider_status == "not_supported" {
        push_check(
            "provider_admission_state",
            "provider is admitted as a live review lane",
            "blocked",
            Some("provider_not_supported"),
            json!({
                "provider": provider.provider,
                "status": provider_status,
            }),
        );
        repair_actions.insert(
            "rerun rr doctor with one of: opencode, codex, gemini, claude, copilot".to_owned(),
            (),
        );
    } else {
        push_check(
            "provider_admission_state",
            "provider is admitted as a live review lane",
            "verified",
            None,
            json!({
                "provider": provider.provider,
                "status": provider_status,
                "support_tier": provider_support_tier,
            }),
        );
    }

    let binary_path_configured = provider.binary_path.value.clone();
    if let Some(resolved_binary) = resolve_provider_binary_path(&binary_path_configured) {
        push_check(
            "provider_binary_present",
            "provider binary is discoverable",
            "verified",
            None,
            json!({
                "provider": provider.provider,
                "configured_binary": binary_path_configured,
                "resolved_binary": resolved_binary.to_string_lossy(),
                "provenance": provider.binary_path.provenance,
            }),
        );
    } else {
        push_check(
            "provider_binary_present",
            "provider binary is discoverable",
            "blocked",
            Some("provider_binary_missing"),
            json!({
                "provider": provider.provider,
                "configured_binary": binary_path_configured,
                "provenance": provider.binary_path.provenance,
            }),
        );
        match provider.provider.as_str() {
            "opencode" => {
                repair_actions.insert(
                    "install opencode and ensure it is available on PATH, or set RR_OPENCODE_BIN=/absolute/path/to/opencode".to_owned(),
                    (),
                );
            }
            "copilot" => {
                repair_actions.insert(
                    format!(
                        "install the GitHub Copilot CLI binary and ensure it is discoverable, or set {}=/absolute/path/to/copilot",
                        ENV_COPILOT_BIN
                    ),
                    (),
                );
            }
            "codex" | "gemini" | "claude" => {
                repair_actions.insert(
                    format!(
                        "install the '{}' CLI binary and ensure it is discoverable on PATH",
                        provider.provider
                    ),
                    (),
                );
            }
            _ => {}
        }
    }

    if provider.provider == "copilot" {
        let copilot_instructions = workspace_root.join(".github/copilot-instructions.md");
        if copilot_instructions.is_file() {
            push_check(
                "copilot_instructions_present",
                "copilot instruction file is present",
                "verified",
                None,
                json!({"path": copilot_instructions.to_string_lossy()}),
            );
        } else {
            push_check(
                "copilot_instructions_present",
                "copilot instruction file is present",
                "blocked",
                Some("copilot_instruction_missing"),
                json!({"path": copilot_instructions.to_string_lossy()}),
            );
            repair_actions.insert(
                "add .github/copilot-instructions.md before relying on Copilot admission flows"
                    .to_owned(),
                (),
            );
        }

        let copilot_instructions_dir = workspace_root.join(".github/instructions");
        let has_instruction_assets = fs::read_dir(&copilot_instructions_dir)
            .ok()
            .map(|entries| entries.flatten().any(|entry| entry.path().is_file()))
            .unwrap_or(false);
        if has_instruction_assets {
            push_check(
                "copilot_instruction_assets_present",
                "copilot instruction asset directory is populated",
                "verified",
                None,
                json!({"path": copilot_instructions_dir.to_string_lossy()}),
            );
        } else {
            push_check(
                "copilot_instruction_assets_present",
                "copilot instruction asset directory is populated",
                "blocked",
                Some("copilot_instruction_assets_missing"),
                json!({"path": copilot_instructions_dir.to_string_lossy()}),
            );
            repair_actions.insert(
                "add Copilot instruction assets under .github/instructions/ before admission"
                    .to_owned(),
                (),
            );
        }

        let copilot_hooks_dir = workspace_root.join(".github/hooks");
        let has_hook_assets = fs::read_dir(&copilot_hooks_dir)
            .ok()
            .map(|entries| entries.flatten().any(|entry| entry.path().is_file()))
            .unwrap_or(false);
        if has_hook_assets {
            push_check(
                "copilot_hook_assets_present",
                "copilot hook assets are present",
                "verified",
                None,
                json!({"path": copilot_hooks_dir.to_string_lossy()}),
            );
        } else {
            push_check(
                "copilot_hook_assets_present",
                "copilot hook assets are present",
                "blocked",
                Some("copilot_hook_assets_missing"),
                json!({"path": copilot_hooks_dir.to_string_lossy()}),
            );
            repair_actions.insert(
                "add Roger Copilot hook assets under .github/hooks/ before admission".to_owned(),
                (),
            );
        }

        // Repo-level hooks only run once merged into the reviewed repo's
        // default branch, so verified start relies on the Roger-owned
        // user-level hooks instead. rr review installs/refreshes them before
        // every launch; doctor reports their current state truthfully.
        match resolve_copilot_home() {
            Some(copilot_home) => {
                let hook_status = session_copilot::verify_user_level_hooks(&copilot_home);
                let (status, reason_code) = match hook_status.state {
                    session_copilot::UserLevelHookState::Installed => ("verified", None),
                    session_copilot::UserLevelHookState::Missing => {
                        ("deferred", Some("user_level_hooks_not_installed"))
                    }
                    session_copilot::UserLevelHookState::Stale => {
                        ("deferred", Some("user_level_hooks_stale"))
                    }
                };
                if status != "verified" {
                    repair_actions.insert(
                        "run rr review --provider copilot to install/refresh the Roger user-level Copilot hooks"
                            .to_owned(),
                        (),
                    );
                }
                push_check(
                    "copilot_user_level_hooks_installed",
                    "Roger user-level Copilot hooks are installed and current",
                    status,
                    reason_code,
                    json!({
                        "config_path": hook_status.config_path,
                        "script_dir": hook_status.script_dir,
                        "stale_entries": hook_status.stale_entries,
                    }),
                );
            }
            None => {
                push_check(
                    "copilot_user_level_hooks_installed",
                    "Roger user-level Copilot hooks are installed and current",
                    "blocked",
                    Some("copilot_home_unresolvable"),
                    json!({}),
                );
                repair_actions.insert(
                    "set COPILOT_HOME or HOME so Roger can manage its user-level Copilot hooks"
                        .to_owned(),
                    (),
                );
            }
        }
    }

    // Auth preflight guidance must honor admission state. `rr review` only
    // accepts live review providers; for a provider Roger does not admit as a
    // live review lane (e.g. pi-agent / not_supported), telling the operator to
    // "run rr review --provider <p>" is dishonest boilerplate that fails closed.
    let provider_is_live_review_lane = provider_capability["supports"]["review_start"]
        .as_bool()
        .unwrap_or(false);
    if provider_is_live_review_lane {
        push_check(
            "provider_auth_preflight",
            "provider auth is preflight-verified",
            "deferred",
            Some("auth_not_preflight_verified"),
            json!({
                "provider": provider.provider,
                "deferred_to": "first_launch",
                "guidance": format!(
                    "run rr review --provider {} --pr <number> to verify auth/path fail-closed behavior on first launch",
                    provider.provider
                ),
            }),
        );
    } else {
        push_check(
            "provider_auth_preflight",
            "provider auth is preflight-verified",
            "deferred",
            Some("auth_preflight_not_a_live_review_provider"),
            json!({
                "provider": provider.provider,
                "status": provider_status,
                "deferred_to": "first_launch",
                "guidance": format!(
                    "{} is not a live rr review provider in this slice, so there is no rr review auth path to preflight; auth verification is not applicable until the provider is admitted as a live review lane",
                    provider.provider
                ),
            }),
        );
    }

    // Semantic/memory posture: a non-fatal informational check that surfaces the
    // semantic asset state and recommends the install repair action when hybrid
    // retrieval is unavailable (contract: rr doctor surfaces semantic posture).
    // Opening the store here is best-effort; a missing/unopenable store defers
    // to bootstrap rather than blocking the doctor run.
    if layout.db_path.is_file() {
        match RogerStore::open(&runtime.store_root)
            .and_then(|store| store.semantic_component_state())
        {
            Ok(state) => {
                if state.operational {
                    push_check(
                        "semantic_assets_operational",
                        "semantic retrieval assets are installed and verified",
                        "verified",
                        None,
                        json!({
                            "operational": true,
                            "assets_verified": state.assets_verified,
                            "embedder_available": state.embedder_available,
                            "embedder_backend": state.embedder_backend,
                            "retrieval_mode": "hybrid",
                        }),
                    );
                } else {
                    push_check(
                        "semantic_assets_operational",
                        "semantic retrieval assets are installed and verified",
                        "deferred",
                        Some("semantic_assets_unverified"),
                        json!({
                            "operational": false,
                            "assets_verified": state.assets_verified,
                            "embedder_available": state.embedder_available,
                            "embedder_backend": state.embedder_backend,
                            "retrieval_mode": "lexical_only",
                            "degraded_reasons": state.degraded_reasons,
                            "guidance": "hybrid semantic retrieval is unavailable; lexical-only search stays fully functional",
                        }),
                    );
                    repair_actions.insert(
                        "run rr assets install --asset semantic-default to enable hybrid semantic retrieval"
                            .to_owned(),
                        (),
                    );
                }
            }
            Err(_) => {
                push_check(
                    "semantic_assets_operational",
                    "semantic retrieval assets are installed and verified",
                    "deferred",
                    Some("semantic_posture_probe_deferred"),
                    json!({
                        "guidance": "semantic posture is probed on first store-backed use; lexical-only search stays fully functional",
                    }),
                );
            }
        }
    } else {
        push_check(
            "semantic_assets_operational",
            "semantic retrieval assets are installed and verified",
            "deferred",
            Some("store_auto_bootstrap_pending"),
            json!({
                "guidance": "semantic posture is probed once the store is bootstrapped; lexical-only search stays fully functional",
            }),
        );
    }

    let blocked_count = checks
        .iter()
        .filter(|check| check["status"] == "blocked")
        .count();
    let deferred_count = checks
        .iter()
        .filter(|check| check["status"] == "deferred")
        .count();
    let verified_count = checks
        .iter()
        .filter(|check| check["status"] == "verified")
        .count();
    let mut warnings = Vec::new();
    if deferred_count > 0 {
        warnings.push(
            "doctor defers auth verification to first launch and does not claim preflight auth truth"
                .to_owned(),
        );
    }
    if provider.provider == "opencode"
        && let Some((warning, actions)) = opencode_legacy_config_guidance(&runtime.opencode_bin)
    {
        warnings.push(warning);
        for action in actions {
            repair_actions.insert(action, ());
        }
    }
    let outcome = if blocked_count > 0 {
        OutcomeKind::Blocked
    } else {
        OutcomeKind::Complete
    };

    CommandResponse {
        outcome,
        data: json!({
            "subcommand": "doctor",
            "provider": provider.provider,
            "provider_capability": provider_capability,
            "routine_surface": routine_surface,
            "store_layout": {
                "root": layout.root.to_string_lossy(),
                "db_path": layout.db_path.to_string_lossy(),
                "artifact_root": layout.artifact_root.to_string_lossy(),
                "sidecar_root": layout.sidecar_root.to_string_lossy(),
                "schema_version": schema_version,
            },
            "checks": checks,
            "summary": {
                "status": if blocked_count > 0 { "blocked" } else { "complete" },
                "verified_count": verified_count,
                "deferred_count": deferred_count,
                "blocked_count": blocked_count,
                "auth_preflight": "deferred_to_first_launch",
            },
        }),
        warnings,
        repair_actions: repair_actions.into_keys().collect(),
        message: if blocked_count > 0 {
            format!(
                "rr doctor found {} blocked checks for provider {}",
                blocked_count, parsed.provider
            )
        } else {
            format!(
                "rr doctor completed with {} verified checks for provider {}",
                verified_count, parsed.provider
            )
        },
    }
}

fn extension_guided_profile_root(runtime: &CliRuntime, browser: &SupportedBrowser) -> PathBuf {
    runtime
        .store_root
        .join("bridge/browser-profiles")
        .join(supported_browser_label(browser.clone()))
}

const DEFAULT_EXTENSION_SETUP_REGISTRATION_WAIT_MS: u64 = 2000;
const EXTENSION_SETUP_REGISTRATION_POLL_MS: u64 = 100;

fn extension_setup_registration_wait_ms() -> u64 {
    std::env::var("RR_EXTENSION_SETUP_REGISTRATION_WAIT_MS")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(DEFAULT_EXTENSION_SETUP_REGISTRATION_WAIT_MS)
}

fn default_extension_install_root() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}

fn custom_extension_install_root_warning(
    parsed: &ParsedArgs,
    browser: &SupportedBrowser,
    install_root: &Path,
) -> Option<String> {
    let explicit_root = parsed.bridge_install_root.as_ref()?;
    let default_root = default_extension_install_root()?;
    let explicit_root = PathBuf::from(explicit_root);
    if normalize_path_for_compare(&explicit_root) == normalize_path_for_compare(&default_root) {
        return None;
    }

    Some(format!(
        "custom --install-root {} only writes the Native Messaging host manifest under that synthetic home root; live {} reads its real platform host path instead. Omit --install-root for real browser/operator-stability runs.",
        install_root.display(),
        supported_browser_label(browser.clone())
    ))
}

fn extension_default_profile_root(browser: &SupportedBrowser) -> Option<PathBuf> {
    let host_os = SupportedOs::current()?;
    let home = default_extension_install_root();
    let local_app_data = std::env::var("LOCALAPPDATA").ok().map(PathBuf::from);
    match (host_os, browser) {
        (SupportedOs::Macos, SupportedBrowser::Chrome) => {
            home.map(|path| path.join("Library/Application Support/Google/Chrome"))
        }
        (SupportedOs::Macos, SupportedBrowser::Edge) => {
            home.map(|path| path.join("Library/Application Support/Microsoft Edge"))
        }
        (SupportedOs::Macos, SupportedBrowser::Brave) => {
            home.map(|path| path.join("Library/Application Support/BraveSoftware/Brave-Browser"))
        }
        (SupportedOs::Windows, SupportedBrowser::Chrome) => {
            local_app_data.map(|path| path.join("Google/Chrome/User Data"))
        }
        (SupportedOs::Windows, SupportedBrowser::Edge) => {
            local_app_data.map(|path| path.join("Microsoft/Edge/User Data"))
        }
        (SupportedOs::Windows, SupportedBrowser::Brave) => {
            local_app_data.map(|path| path.join("BraveSoftware/Brave-Browser/User Data"))
        }
        (SupportedOs::Linux, SupportedBrowser::Chrome) => {
            home.map(|path| path.join(".config/google-chrome"))
        }
        (SupportedOs::Linux, SupportedBrowser::Edge) => {
            home.map(|path| path.join(".config/microsoft-edge"))
        }
        (SupportedOs::Linux, SupportedBrowser::Brave) => {
            home.map(|path| path.join(".config/BraveSoftware/Brave-Browser"))
        }
    }
}

fn extension_profile_roots_for_discovery(
    browser: &SupportedBrowser,
    runtime: &CliRuntime,
) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let mut explicit_profile_root = false;
    if let Ok(path) = std::env::var("RR_EXTENSION_PROFILE_ROOT") {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            roots.push(PathBuf::from(trimmed));
            explicit_profile_root = true;
        }
    }
    if !explicit_profile_root {
        roots.push(extension_guided_profile_root(runtime, browser));
    }
    // When an explicit profile root is supplied for setup/doctor, treat it as
    // authoritative and avoid probing the ambient browser profile tree.
    if explicit_profile_root {
        return roots;
    }
    if let Some(default_root) = extension_default_profile_root(browser) {
        if !roots.iter().any(|existing| existing == &default_root) {
            roots.push(default_root);
        }
    }
    roots
}

fn extension_profile_preference_files(profile_root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for name in ["Secure Preferences", "Preferences"] {
        let candidate = profile_root.join(name);
        if candidate.is_file() {
            files.push(candidate);
        }
    }
    if let Ok(entries) = fs::read_dir(profile_root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            for name in ["Secure Preferences", "Preferences"] {
                let candidate = path.join(name);
                if candidate.is_file() {
                    files.push(candidate);
                }
            }
        }
    }
    files.sort();
    files.dedup();
    files
}

fn normalize_path_for_compare(path: &Path) -> String {
    let normalized = path.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        normalized.to_ascii_lowercase()
    } else {
        normalized
    }
}

fn extension_path_matches_package_dir(
    value: &str,
    preference_file: &Path,
    package_dir: &Path,
) -> bool {
    let package_path = fs::canonicalize(package_dir).unwrap_or_else(|_| package_dir.to_path_buf());
    let candidate_path = PathBuf::from(value);
    let resolved_candidate = if candidate_path.is_absolute() {
        candidate_path
    } else {
        preference_file
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .join(candidate_path)
    };
    let resolved_candidate =
        fs::canonicalize(&resolved_candidate).unwrap_or_else(|_| resolved_candidate.to_path_buf());
    normalize_path_for_compare(&resolved_candidate) == normalize_path_for_compare(&package_path)
}

fn discover_extension_id_from_preferences_file(
    preference_file: &Path,
    package_dir: &Path,
) -> Option<String> {
    let contents = fs::read_to_string(preference_file).ok()?;
    let parsed: Value = serde_json::from_str(&contents).ok()?;
    let settings = parsed.get("extensions")?.get("settings")?.as_object()?;
    for (extension_id, entry) in settings {
        if !extension_id_looks_valid(extension_id) {
            continue;
        }
        let Some(path_value) = entry.get("path").and_then(Value::as_str) else {
            continue;
        };
        if extension_path_matches_package_dir(path_value, preference_file, package_dir) {
            return Some(extension_id.to_owned());
        }
    }
    None
}

fn discover_extension_id_from_browser_profiles(
    browser: &SupportedBrowser,
    runtime: &CliRuntime,
    package_dir: &Path,
) -> Option<String> {
    for profile_root in extension_profile_roots_for_discovery(browser, runtime) {
        for preference_file in extension_profile_preference_files(&profile_root) {
            if let Some(extension_id) =
                discover_extension_id_from_preferences_file(&preference_file, package_dir)
            {
                return Some(extension_id);
            }
        }
    }
    None
}

fn discover_extension_id_for_extension_setup(
    parsed: &ParsedArgs,
    runtime: &CliRuntime,
    browser: &SupportedBrowser,
    package_dir: &Path,
) -> Option<(String, &'static str)> {
    if let Some(discovered) = discover_explicit_extension_id(parsed) {
        return Some(discovered);
    }

    discover_extension_id_from_browser_profiles(browser, runtime, package_dir)
        .map(|value| (value, "browser_profile_preferences"))
}

fn discover_extension_id_for_extension_setup_with_wait(
    parsed: &ParsedArgs,
    runtime: &CliRuntime,
    browser: &SupportedBrowser,
    package_dir: &Path,
    wait_budget_ms: u64,
) -> Option<(String, &'static str, bool)> {
    if let Some((extension_id, source)) =
        discover_extension_id_for_extension_setup(parsed, runtime, browser, package_dir)
    {
        return Some((extension_id, source, false));
    }
    if wait_budget_ms == 0 {
        return None;
    }

    let deadline = Instant::now() + Duration::from_millis(wait_budget_ms);
    loop {
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        let remaining = deadline.saturating_duration_since(now);
        let sleep_for = remaining.min(Duration::from_millis(EXTENSION_SETUP_REGISTRATION_POLL_MS));
        thread::sleep(sleep_for);
        if let Some((extension_id, source)) =
            discover_extension_id_for_extension_setup(parsed, runtime, browser, package_dir)
        {
            return Some((extension_id, source, true));
        }
    }

    discover_extension_id_from_packaged_manifest(package_dir)
        .map(|(extension_id, source)| (extension_id, source, false))
}

fn extension_profile_launch_hint(
    browser: &SupportedBrowser,
    profile_root: &Path,
    package_dir: &str,
) -> String {
    let browser_label = supported_browser_label(browser.clone());
    format!(
        "launch {browser_label} once with --user-data-dir {} --load-extension {} --disable-extensions-except {}, then rerun rr extension setup",
        profile_root.display(),
        package_dir,
        package_dir
    )
}

fn shell_quote_arg(value: &str) -> String {
    if value.is_empty() {
        return "''".to_owned();
    }
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn extension_guided_browser_script_path(workspace_root: &Path) -> PathBuf {
    workspace_root.join("scripts/extension/launch_preloaded_browser.sh")
}

fn extension_guided_browser_command(
    workspace_root: &Path,
    browser: &SupportedBrowser,
    profile_root: &Path,
    package_dir: &str,
    start_url: &str,
) -> String {
    let script_path = extension_guided_browser_script_path(workspace_root);
    format!(
        "{} --browser {} --profile-root {} --package-dir {} --start-url {}",
        shell_quote_arg(&script_path.to_string_lossy()),
        shell_quote_arg(supported_browser_label(browser.clone())),
        shell_quote_arg(&profile_root.to_string_lossy()),
        shell_quote_arg(package_dir),
        shell_quote_arg(start_url),
    )
}

/// Installed-mode launch guidance when the dev guided-browser script is not
/// available. Live-tested truth (2026-06): branded Google Chrome >= 137
/// ignores --load-extension, so Chrome needs one manual "Load unpacked" pass
/// via chrome://extensions; Edge 150+ ignores it too (live-verified 2026-07-07),
/// so Edge also needs one manual Load-unpacked pass; Brave still honored the
/// flag-based launch at last verification.
fn extension_inline_browser_launch_guidance(
    browser: &SupportedBrowser,
    profile_root: &Path,
    package_dir: &str,
    start_url: &str,
) -> String {
    let browser_label = supported_browser_label(browser.clone());
    match browser {
        SupportedBrowser::Chrome => format!(
            "branded Google Chrome 137+ ignores --load-extension: open chrome://extensions, enable 'Developer mode', click 'Load unpacked', select {package_dir}, then open {start_url}"
        ),
        SupportedBrowser::Edge => format!(
            "Microsoft Edge 150+ ignores --load-extension: open edge://extensions, enable 'Developer mode', click 'Load unpacked', select {package_dir}, then open {start_url}"
        ),
        SupportedBrowser::Brave => format!(
            "launch {browser_label} once with --user-data-dir={} --load-extension={} --disable-extensions-except={} {} (Brave still honored flag-based extension load at last verification; branded Chrome 137+ and Edge 150+ do not)",
            profile_root.display(),
            package_dir,
            package_dir,
            start_url,
        ),
    }
}

/// Builds the guided browser launch surface for setup/doctor output:
/// dev workspaces use the repo's guided-browser script, installed binaries get
/// inline flag/manual-load guidance instead of a repo script path that does
/// not exist on the host.
fn extension_browser_launch_surface(
    workspace_root: Option<&Path>,
    browser: &SupportedBrowser,
    profile_root: &Path,
    package_dir: &str,
    start_url: &str,
) -> (String, Value, String) {
    match workspace_root {
        Some(root) => (
            extension_guided_browser_command(root, browser, profile_root, package_dir, start_url),
            Value::String(
                extension_guided_browser_script_path(root)
                    .to_string_lossy()
                    .to_string(),
            ),
            extension_profile_launch_hint(browser, profile_root, package_dir),
        ),
        None => {
            let inline = extension_inline_browser_launch_guidance(
                browser,
                profile_root,
                package_dir,
                start_url,
            );
            (inline.clone(), Value::Null, inline)
        }
    }
}

fn extension_browser_url(browser: SupportedBrowser) -> &'static str {
    match browser {
        SupportedBrowser::Chrome => "chrome://extensions",
        SupportedBrowser::Edge => "edge://extensions",
        SupportedBrowser::Brave => "brave://extensions",
    }
}

fn native_host_launcher_path(manifest_path: &Path, host_os: SupportedOs) -> PathBuf {
    let stem = manifest_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("com.roger_reviewer.bridge");
    let suffix = match host_os {
        SupportedOs::Windows => "cmd",
        SupportedOs::Macos | SupportedOs::Linux => "sh",
    };
    manifest_path.with_file_name(format!("{stem}.{suffix}"))
}

fn write_native_host_launcher(
    launcher_path: &Path,
    bridge_binary: &Path,
    host_os: SupportedOs,
) -> Result<(), String> {
    if let Some(parent) = launcher_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "failed to create native host launcher directory {}: {err}",
                parent.display()
            )
        })?;
    }

    let binary = bridge_binary.to_string_lossy();
    let contents = match host_os {
        SupportedOs::Windows => format!(
            "@echo off\r\nif \"%RR_STORE_ROOT%\"==\"\" set \"RR_STORE_ROOT=%USERPROFILE%\\.roger\"\r\n\"{}\" --native-host\r\n",
            binary.replace('\"', "\"\"")
        ),
        SupportedOs::Macos | SupportedOs::Linux => {
            let escaped = binary
                .replace('\\', "\\\\")
                .replace('\"', "\\\"")
                .replace('$', "\\$")
                .replace('`', "\\`");
            format!(
                "#!/bin/sh\nif [ -n \"${{PATH:-}}\" ]; then\n  export PATH=\"/opt/homebrew/bin:/usr/local/bin:${{PATH}}\"\nelse\n  export PATH=\"/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin\"\nfi\nif [ -z \"${{RR_STORE_ROOT:-}}\" ]; then\n  export RR_STORE_ROOT=\"${{HOME}}/.roger\"\nfi\nexec \"{escaped}\" --native-host\n"
            )
        }
    };

    fs::write(launcher_path, contents).map_err(|err| {
        format!(
            "failed to write native host launcher {}: {err}",
            launcher_path.display()
        )
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(launcher_path)
            .map_err(|err| {
                format!(
                    "failed to stat native host launcher {}: {err}",
                    launcher_path.display()
                )
            })?
            .permissions();
        permissions.set_mode(permissions.mode() | 0o755);
        fs::set_permissions(launcher_path, permissions).map_err(|err| {
            format!(
                "failed to make native host launcher executable {}: {err}",
                launcher_path.display()
            )
        })?;
    }

    Ok(())
}

fn persist_extension_id(runtime: &CliRuntime, extension_id: &str) -> Result<(), String> {
    let path = extension_id_registry_path(&runtime.store_root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "failed to create extension identity registry directory {}: {err}",
                parent.display()
            )
        })?;
    }
    fs::write(&path, format!("{extension_id}\n"))
        .map_err(|err| format!("failed to write extension identity registry: {err}"))
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ExtensionVersionProbe {
    exact_tag: Option<String>,
    rev_count: Option<String>,
    short_sha: Option<String>,
    dirty_fingerprint: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ExtensionBuildVersion {
    manifest_version: String,
    version_name: String,
}

fn read_extension_manifest_template(manifest_template_path: &Path) -> Result<Value, String> {
    let manifest_template = fs::read_to_string(manifest_template_path).map_err(|err| {
        format!(
            "failed to read extension manifest template {}: {err}",
            manifest_template_path.display()
        )
    })?;
    serde_json::from_str(&manifest_template)
        .map_err(|err| format!("failed to parse extension manifest template: {err}"))
}

fn extension_id_from_hash_prefix(hash_prefix: &[u8]) -> String {
    let mut extension_id = String::with_capacity(hash_prefix.len() * 2);
    for byte in hash_prefix {
        for nibble in [byte >> 4, byte & 0x0f] {
            extension_id.push((b'a' + nibble) as char);
        }
    }
    extension_id
}

fn derive_extension_id_from_manifest_key(manifest_key: &str) -> Option<String> {
    let decoded = BASE64_STANDARD.decode(manifest_key.trim()).ok()?;
    if decoded.is_empty() {
        return None;
    }
    let digest = Sha256::digest(&decoded);
    Some(extension_id_from_hash_prefix(&digest[..16]))
}

fn discover_extension_id_from_manifest_json(
    manifest_json: &Value,
) -> Option<(String, &'static str)> {
    let manifest_key = manifest_json.get("key")?.as_str()?;
    let extension_id = derive_extension_id_from_manifest_key(manifest_key)?;
    Some((extension_id, "packaged_manifest_key"))
}

fn discover_extension_id_from_packaged_manifest(
    package_dir: &Path,
) -> Option<(String, &'static str)> {
    let manifest_path = package_dir.join("manifest.json");
    let contents = fs::read_to_string(manifest_path).ok()?;
    let manifest_json: Value = serde_json::from_str(&contents).ok()?;
    discover_extension_id_from_manifest_json(&manifest_json)
}

fn collect_manifest_icon_paths(manifest_json: &Value) -> Vec<String> {
    let mut paths = Vec::new();

    if let Some(icons) = manifest_json.get("icons").and_then(Value::as_object) {
        for value in icons.values() {
            if let Some(path) = value.as_str() {
                if !path.trim().is_empty() {
                    paths.push(path.to_owned());
                }
            }
        }
    }

    if let Some(default_icon) = manifest_json
        .get("action")
        .and_then(|value| value.get("default_icon"))
    {
        match default_icon {
            Value::String(path) if !path.trim().is_empty() => {
                paths.push(path.to_owned());
            }
            Value::Object(icons) => {
                for value in icons.values() {
                    if let Some(path) = value.as_str() {
                        if !path.trim().is_empty() {
                            paths.push(path.to_owned());
                        }
                    }
                }
            }
            _ => {}
        }
    }

    paths.sort();
    paths.dedup();
    paths
}

fn validate_packaged_manifest_icon_paths(
    package_dir: &Path,
    manifest_json: &Value,
) -> Result<(), String> {
    // Repro note (2026-04-17): both Chrome and Edge reject unpacked loads when
    // manifest icon assets (for example assets/icon-16.png) are missing.
    let icon_paths = collect_manifest_icon_paths(manifest_json);
    let mut missing = Vec::new();
    for icon_path in icon_paths {
        if !package_dir.join(&icon_path).exists() {
            missing.push(icon_path);
        }
    }

    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "packaged extension is missing manifest-declared icon assets: {} (observed Chrome/Edge sideload failure: Could not load icon assets/icon-16.png)",
            missing.join(", ")
        ))
    }
}

fn normalize_extension_manifest_version(base_version: &str) -> String {
    let mut segments = base_version
        .split('.')
        .map(|segment| segment.parse::<u32>().ok())
        .collect::<Vec<_>>();
    while segments.len() < 4 {
        segments.push(Some(0));
    }
    segments
        .into_iter()
        .take(4)
        .map(|segment| segment.unwrap_or(0).to_string())
        .collect::<Vec<_>>()
        .join(".")
}

fn parse_release_calendar_tag(tag: &str) -> Option<(u32, u32, u32, Option<u32>, String)> {
    let raw = tag.strip_prefix('v')?;
    let (date_part, rc_part) = match raw.split_once("-rc.") {
        Some((date, rc)) => (date, Some(rc)),
        None => (raw, None),
    };
    let mut date_segments = date_part.split('.');
    let year = date_segments.next()?.parse::<u32>().ok()?;
    let month = date_segments.next()?.parse::<u32>().ok()?;
    let day = date_segments.next()?.parse::<u32>().ok()?;
    if date_segments.next().is_some() {
        return None;
    }
    let rc_number = match rc_part {
        Some(raw_rc) => Some(raw_rc.parse::<u32>().ok()?),
        None => None,
    };
    Some((year, month, day, rc_number, raw.to_owned()))
}

fn derive_extension_build_version_from_probe(
    template_version: &str,
    probe: &ExtensionVersionProbe,
) -> ExtensionBuildVersion {
    if let Some(tag) = probe.exact_tag.as_deref() {
        if let Some((year, month, day, rc_number, version_name)) = parse_release_calendar_tag(tag) {
            let build_number = rc_number.unwrap_or(1000);
            return ExtensionBuildVersion {
                manifest_version: format!("{year}.{month}.{day}.{build_number}"),
                version_name,
            };
        }
    }

    let manifest_version = normalize_extension_manifest_version(template_version);
    let rev_count = probe.rev_count.as_deref().unwrap_or("0");
    let short_sha = probe.short_sha.as_deref().unwrap_or("nogit");
    let mut version_name = format!("{template_version}-dev.{rev_count}+{short_sha}");
    if let Some(dirty_fingerprint) = probe.dirty_fingerprint.as_deref() {
        if !dirty_fingerprint.is_empty() {
            version_name.push_str(&format!(".dirty.{dirty_fingerprint}"));
        }
    }
    ExtensionBuildVersion {
        manifest_version,
        version_name,
    }
}

fn git_output_trimmed(workspace_root: &Path, args: &[&str]) -> Option<String> {
    let output = ProcessCommand::new("git")
        .arg("-C")
        .arg(workspace_root)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if text.is_empty() { None } else { Some(text) }
}

fn collect_extension_version_probe(workspace_root: &Path) -> ExtensionVersionProbe {
    let dirty_fingerprint = git_output_trimmed(workspace_root, &["status", "--porcelain"])
        .and_then(|status| {
            if status.is_empty() {
                None
            } else {
                Some(sha256_hex(status.as_bytes())[0..8].to_owned())
            }
        });

    ExtensionVersionProbe {
        exact_tag: git_output_trimmed(
            workspace_root,
            &[
                "describe",
                "--tags",
                "--exact-match",
                "--match",
                "v*",
                "HEAD",
            ],
        ),
        rev_count: git_output_trimmed(workspace_root, &["rev-list", "--count", "HEAD"]),
        short_sha: git_output_trimmed(workspace_root, &["rev-parse", "--short=12", "HEAD"]),
        dirty_fingerprint,
    }
}

fn derive_extension_build_version(
    workspace_root: &Path,
    manifest_json: &Value,
) -> ExtensionBuildVersion {
    let template_version = manifest_json
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or("0.0.0");
    derive_extension_build_version_from_probe(
        template_version,
        &collect_extension_version_probe(workspace_root),
    )
}

fn extension_package_dir_name(manifest_json: &Value) -> String {
    let _ = manifest_json;
    "roger-extension-unpacked".to_owned()
}

fn resolve_extension_package_dir(workspace_root: &Path) -> Result<PathBuf, String> {
    let manifest_template_path = workspace_root.join("apps/extension/manifest.template.json");
    let manifest_json = read_extension_manifest_template(&manifest_template_path)?;
    Ok(workspace_root
        .join("target/bridge/extension")
        .join(extension_package_dir_name(&manifest_json)))
}

fn installed_extension_package_root(store_root: &Path) -> PathBuf {
    store_root.join("bridge/extension-package")
}

fn installed_extension_package_dir(store_root: &Path, version: &str) -> PathBuf {
    installed_extension_package_root(store_root)
        .join(version)
        .join("roger-extension-unpacked")
}

fn installed_extension_package_is_usable(package_dir: &Path) -> bool {
    package_dir.join("manifest.json").is_file()
}

/// Resolves the installed-mode extension package directory under the store
/// root: prefer the exact embedded release version, then the newest fetched
/// version that still contains an unpacked manifest.
fn resolve_installed_extension_package_dir(store_root: &Path) -> Option<(PathBuf, &'static str)> {
    if let Some(version) = option_env!("ROGER_RELEASE_VERSION") {
        let exact = installed_extension_package_dir(store_root, version);
        if installed_extension_package_is_usable(&exact) {
            return Some((exact, "installed_layout_release_version"));
        }
    }

    let package_root = installed_extension_package_root(store_root);
    let mut candidates: Vec<(String, PathBuf)> = Vec::new();
    if let Ok(entries) = fs::read_dir(&package_root) {
        for entry in entries.flatten() {
            let candidate = entry.path().join("roger-extension-unpacked");
            if installed_extension_package_is_usable(&candidate) {
                candidates.push((entry.file_name().to_string_lossy().to_string(), candidate));
            }
        }
    }
    candidates.sort_by(|a, b| a.0.cmp(&b.0));
    candidates
        .pop()
        .map(|(_version, path)| (path, "installed_layout_newest_available"))
}

#[derive(Clone, Debug)]
struct ExtensionPackageResolution {
    package_dir: PathBuf,
    /// explicit_package_dir | dev_workspace | installed_layout_release_version |
    /// installed_layout_newest_available
    source: &'static str,
}

fn extension_package_missing_response(subcommand: &str, runtime: &CliRuntime) -> CommandResponse {
    blocked_response(
        "no Roger extension package is available for this rr install".to_owned(),
        vec![
            "run rr extension fetch to download and verify the published extension package for this release".to_owned(),
            "or run rr extension setup from a Roger dev workspace, which packs the extension from source".to_owned(),
            "or pass --package-dir <path> pointing at an unpacked Roger extension directory".to_owned(),
        ],
        json!({
            "subcommand": subcommand,
            "reason_code": "extension_package_missing",
            "installed_package_root": installed_extension_package_root(&runtime.store_root)
                .to_string_lossy()
                .to_string(),
        }),
    )
}

/// Resolution order for the unpacked extension package directory:
/// 1. explicit --package-dir
/// 2. dev workspace target/bridge/extension/... when workspace markers exist
/// 3. installed layout under <store_root>/bridge/extension-package/<version>/
fn resolve_extension_package_for_doctor(
    parsed: &ParsedArgs,
    runtime: &CliRuntime,
    workspace_root: Option<&Path>,
) -> Result<ExtensionPackageResolution, CommandResponse> {
    if let Some(explicit) = parsed.extension_package_dir.as_ref() {
        return Ok(ExtensionPackageResolution {
            package_dir: explicit.clone(),
            source: "explicit_package_dir",
        });
    }

    if let Some(workspace_root) = workspace_root {
        return match resolve_extension_package_dir(workspace_root) {
            Ok(path) => Ok(ExtensionPackageResolution {
                package_dir: path,
                source: "dev_workspace",
            }),
            Err(err) => Err(error_response(err)),
        };
    }

    match resolve_installed_extension_package_dir(&runtime.store_root) {
        Some((package_dir, source)) => Ok(ExtensionPackageResolution {
            package_dir,
            source,
        }),
        None => Err(extension_package_missing_response("doctor", runtime)),
    }
}

fn handle_extension(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    let Some(subcommand) = parsed.extension_command else {
        return error_response("rr extension missing subcommand".to_owned());
    };

    // Installed binaries run outside the Roger repo: workspace markers are a
    // dev-mode convenience, not a prerequisite for extension setup commands.
    let workspace_root = find_workspace_root(&runtime.cwd);

    match subcommand {
        ExtensionCommandKind::Setup => {
            handle_extension_setup(parsed, runtime, workspace_root.as_deref())
        }
        ExtensionCommandKind::Doctor => {
            handle_extension_doctor(parsed, runtime, workspace_root.as_deref())
        }
        ExtensionCommandKind::Fetch => handle_extension_fetch(parsed, runtime),
        ExtensionCommandKind::Uninstall => handle_extension_uninstall(parsed, runtime),
    }
}

fn handle_extension_uninstall(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    let mut bridge_parsed = parsed.clone();
    bridge_parsed.command = CommandKind::Bridge;
    bridge_parsed.bridge_command = Some(BridgeCommandKind::Uninstall);
    bridge_parsed.extension_command = None;
    bridge_parsed.extension_browser = None;

    let mut uninstall = handle_bridge(&bridge_parsed, runtime);
    if uninstall.outcome == OutcomeKind::Complete {
        uninstall
            .warnings
            .retain(|warning| warning != BRIDGE_UNINSTALL_REPAIR_ALIAS_WARNING);
        if let Some(data) = uninstall.data.as_object_mut() {
            data.insert("surface".to_owned(), json!("extension"));
            data.insert(
                "bridge_alias_command".to_owned(),
                json!("rr bridge uninstall"),
            );
        }
        uninstall.message = "extension uninstall completed".to_owned();
    }
    uninstall
}

/// Successful outcome of fetching + installing a published extension package.
#[derive(Clone, Debug)]
struct ExtensionFetchOutcome {
    version: String,
    tag: String,
    archive_name: String,
    archive_sha256: String,
    archive_url: String,
    install_metadata_url: String,
    checksums_url: String,
    checksums_legacy_fallback: bool,
    package_dir: PathBuf,
    fetch_manifest_path: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExtensionFetchFailureKind {
    /// Fail-closed contract violation (blocked envelope for the command surface).
    Blocked,
    /// Internal/filesystem error (error envelope for the command surface).
    Error,
}

/// Typed failure from the shared extension fetch core. The command surface maps
/// this into a `blocked`/`error` envelope; the update refresh phase degrades it
/// into a non-fatal `extension_refresh_failed: <reason>` warning.
#[derive(Clone, Debug)]
struct ExtensionFetchFailure {
    kind: ExtensionFetchFailureKind,
    reason_code: &'static str,
    message: String,
    repair_actions: Vec<String>,
    /// Reason-specific extra fields (never includes reason_code/subcommand).
    detail: Value,
}

/// Downloads the published extension.zip release asset for `version`, verifies
/// it against the release checksums manifest, and unpacks it into the installed
/// layout <store_root>/bridge/extension-package/<version>/roger-extension-unpacked.
///
/// This is the callable core shared by `rr extension fetch` and the in-place
/// update extension-refresh phase. It never shells out to `rr`.
fn fetch_and_install_extension_package_core(
    store_root: &Path,
    repo: &str,
    version: &str,
    download_root: &str,
) -> std::result::Result<ExtensionFetchOutcome, ExtensionFetchFailure> {
    let _ = repo;
    let tag = format!("v{version}");

    let install_metadata_name = format!("release-install-metadata-{version}.json");
    let install_metadata_url = format!("{download_root}/{tag}/{install_metadata_name}");
    let install_metadata_text =
        fetch_url_with_curl(&install_metadata_url).map_err(|err| ExtensionFetchFailure {
            kind: ExtensionFetchFailureKind::Blocked,
            reason_code: "install_metadata_missing",
            message: format!("failed to fetch install metadata bundle: {err}"),
            repair_actions: vec![
                "confirm the release tag is published".to_owned(),
                "or pass --version for a known published CalVer release".to_owned(),
            ],
            detail: json!({ "url": install_metadata_url }),
        })?;
    let install_metadata: Value =
        serde_json::from_str(&install_metadata_text).map_err(|err| ExtensionFetchFailure {
            kind: ExtensionFetchFailureKind::Blocked,
            reason_code: "install_metadata_invalid_json",
            message: format!("install metadata bundle is invalid JSON: {err}"),
            repair_actions: vec!["re-run release verification for this tag".to_owned()],
            detail: json!({}),
        })?;
    if install_metadata.get("schema").and_then(Value::as_str)
        != Some("roger.release.install-metadata.v1")
    {
        return Err(ExtensionFetchFailure {
            kind: ExtensionFetchFailureKind::Blocked,
            reason_code: "install_metadata_schema_mismatch",
            message: "install metadata schema mismatch; refusing extension fetch".to_owned(),
            repair_actions: vec!["rebuild release metadata bundle for this tag".to_owned()],
            detail: json!({}),
        });
    }
    let release = install_metadata.get("release").and_then(Value::as_object);
    if release
        .and_then(|value| value.get("version"))
        .and_then(Value::as_str)
        != Some(version)
    {
        return Err(ExtensionFetchFailure {
            kind: ExtensionFetchFailureKind::Blocked,
            reason_code: "install_metadata_version_mismatch",
            message: "install metadata release.version mismatch".to_owned(),
            repair_actions: vec!["verify release metadata and republish artifacts".to_owned()],
            detail: json!({}),
        });
    }

    let artifact_stem = release
        .and_then(|value| value.get("artifact_stem"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("roger-reviewer-{version}"));
    let checksums_name = install_metadata
        .get("checksums_name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty() && !name.contains('/') && !name.contains('\\'))
        .unwrap_or("SHA256SUMS")
        .to_owned();
    let archive_name = format!("{artifact_stem}-extension.zip");

    let checksums_fetch =
        fetch_checksums_manifest_with_fallback(download_root, &tag, &checksums_name).map_err(
            |err| ExtensionFetchFailure {
                kind: ExtensionFetchFailureKind::Blocked,
                reason_code: "checksums_missing",
                message: err.message,
                repair_actions: vec!["rebuild/upload checksums for this tag".to_owned()],
                detail: json!({ "attempted_urls": err.attempted_urls }),
            },
        )?;
    let expected_archive_sha = checksums_entry_for_archive(&checksums_fetch.text, &archive_name)
        .map_err(|err| ExtensionFetchFailure {
            kind: ExtensionFetchFailureKind::Blocked,
            reason_code: "extension_asset_missing",
            message: format!(
                "release {tag} does not publish a verifiable extension package asset: {err}"
            ),
            repair_actions: vec![
                "confirm this release shipped the extension lane (extension.zip asset)".to_owned(),
                "or run rr extension setup from a Roger dev workspace to pack from source"
                    .to_owned(),
            ],
            detail: json!({
                "archive_name": archive_name,
                "checksums_url": checksums_fetch.url,
            }),
        })?;

    let version_root = installed_extension_package_root(store_root).join(version);
    let staging_root = version_root.join(format!(".staging-{}", next_id("extension-fetch")));
    fs::create_dir_all(&staging_root).map_err(|err| ExtensionFetchFailure {
        kind: ExtensionFetchFailureKind::Error,
        reason_code: "extension_staging_unwritable",
        message: format!(
            "failed to create extension fetch staging directory {}: {err}",
            staging_root.display()
        ),
        repair_actions: Vec::new(),
        detail: json!({}),
    })?;
    let cleanup_staging = |staging_root: &Path| {
        let _ = fs::remove_dir_all(staging_root);
    };

    let archive_url = format!("{download_root}/{tag}/{archive_name}");
    let archive_path = staging_root.join(&archive_name);
    if let Err(err) = download_url_to_path(&archive_url, &archive_path) {
        cleanup_staging(&staging_root);
        return Err(ExtensionFetchFailure {
            kind: ExtensionFetchFailureKind::Blocked,
            reason_code: "extension_archive_download_failed",
            message: format!("failed to download extension package archive: {err}"),
            repair_actions: vec![
                "confirm the release published the extension.zip asset".to_owned(),
            ],
            detail: json!({ "url": archive_url }),
        });
    }
    let observed_archive_sha = match sha256_for_file(&archive_path) {
        Ok(value) => value,
        Err(err) => {
            cleanup_staging(&staging_root);
            return Err(ExtensionFetchFailure {
                kind: ExtensionFetchFailureKind::Error,
                reason_code: "extension_archive_hash_failed",
                message: err,
                repair_actions: Vec::new(),
                detail: json!({}),
            });
        }
    };
    if observed_archive_sha != expected_archive_sha.to_ascii_lowercase() {
        cleanup_staging(&staging_root);
        return Err(ExtensionFetchFailure {
            kind: ExtensionFetchFailureKind::Blocked,
            reason_code: "extension_archive_checksum_mismatch",
            message: format!(
                "extension archive checksum mismatch for {archive_name}: expected {}, got {observed_archive_sha}; refusing to install",
                expected_archive_sha.to_ascii_lowercase()
            ),
            repair_actions: vec![
                "re-run rr extension fetch; if the mismatch persists, the release assets need re-verification"
                    .to_owned(),
            ],
            detail: json!({
                "archive_name": archive_name,
                "expected_sha256": expected_archive_sha.to_ascii_lowercase(),
                "observed_sha256": observed_archive_sha,
            }),
        });
    }

    let staged_unpacked = staging_root.join("roger-extension-unpacked");
    if let Err(err) = fs::create_dir_all(&staged_unpacked) {
        cleanup_staging(&staging_root);
        return Err(ExtensionFetchFailure {
            kind: ExtensionFetchFailureKind::Error,
            reason_code: "extension_unpack_dir_unwritable",
            message: format!(
                "failed to create staged unpack directory {}: {err}",
                staged_unpacked.display()
            ),
            repair_actions: Vec::new(),
            detail: json!({}),
        });
    }
    if let Err(err) = extract_zip_archive(&archive_path, &staged_unpacked) {
        cleanup_staging(&staging_root);
        return Err(ExtensionFetchFailure {
            kind: ExtensionFetchFailureKind::Error,
            reason_code: "extension_archive_unpack_failed",
            message: format!("failed to unpack extension archive: {err}"),
            repair_actions: Vec::new(),
            detail: json!({}),
        });
    }
    if !installed_extension_package_is_usable(&staged_unpacked) {
        cleanup_staging(&staging_root);
        return Err(ExtensionFetchFailure {
            kind: ExtensionFetchFailureKind::Blocked,
            reason_code: "extension_archive_layout_invalid",
            message:
                "downloaded extension archive does not contain manifest.json at the package root"
                    .to_owned(),
            repair_actions: vec![
                "re-verify the published extension.zip asset for this tag".to_owned(),
            ],
            detail: json!({ "archive_name": archive_name }),
        });
    }

    let package_dir = installed_extension_package_dir(store_root, version);
    if package_dir.exists() {
        if let Err(err) = fs::remove_dir_all(&package_dir) {
            cleanup_staging(&staging_root);
            return Err(ExtensionFetchFailure {
                kind: ExtensionFetchFailureKind::Error,
                reason_code: "extension_package_replace_failed",
                message: format!(
                    "failed to replace existing extension package {}: {err}",
                    package_dir.display()
                ),
                repair_actions: Vec::new(),
                detail: json!({}),
            });
        }
    }
    if let Err(err) = fs::rename(&staged_unpacked, &package_dir) {
        cleanup_staging(&staging_root);
        return Err(ExtensionFetchFailure {
            kind: ExtensionFetchFailureKind::Error,
            reason_code: "extension_package_install_failed",
            message: format!(
                "failed to move verified extension package into place at {}: {err}",
                package_dir.display()
            ),
            repair_actions: Vec::new(),
            detail: json!({}),
        });
    }
    cleanup_staging(&staging_root);

    let fetch_manifest_path = version_root.join("fetch-manifest.json");
    let fetch_manifest = json!({
        "schema": "roger.extension.fetch-manifest.v1",
        "version": version,
        "tag": tag,
        "archive_name": archive_name,
        "archive_sha256": expected_archive_sha.to_ascii_lowercase(),
        "archive_url": archive_url,
        "checksums_url": checksums_fetch.url,
        "checksums_legacy_fallback": checksums_fetch.legacy_fallback_used,
        "package_dir": package_dir.to_string_lossy().to_string(),
        "fetched_at_epoch_seconds": time::now_ts(),
    });
    match serde_json::to_vec_pretty(&fetch_manifest) {
        Ok(mut bytes) => {
            bytes.push(b'\n');
            if let Err(err) = fs::write(&fetch_manifest_path, &bytes) {
                return Err(ExtensionFetchFailure {
                    kind: ExtensionFetchFailureKind::Error,
                    reason_code: "extension_fetch_manifest_unwritable",
                    message: format!(
                        "failed to record extension fetch manifest {}: {err}",
                        fetch_manifest_path.display()
                    ),
                    repair_actions: Vec::new(),
                    detail: json!({}),
                });
            }
        }
        Err(err) => {
            return Err(ExtensionFetchFailure {
                kind: ExtensionFetchFailureKind::Error,
                reason_code: "extension_fetch_manifest_encode_failed",
                message: format!("failed to serialize extension fetch manifest: {err}"),
                repair_actions: Vec::new(),
                detail: json!({}),
            });
        }
    }

    Ok(ExtensionFetchOutcome {
        version: version.to_owned(),
        tag,
        archive_name,
        archive_sha256: expected_archive_sha.to_ascii_lowercase(),
        archive_url,
        install_metadata_url,
        checksums_url: checksums_fetch.url,
        checksums_legacy_fallback: checksums_fetch.legacy_fallback_used,
        package_dir,
        fetch_manifest_path,
    })
}

/// Downloads the published extension.zip release asset for this binary's
/// release version (or an explicit --version), verifies it against the
/// release checksums manifest, and unpacks it into the installed layout
/// <store_root>/bridge/extension-package/<version>/roger-extension-unpacked.
fn handle_extension_fetch(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    let repo = parsed
        .repo
        .clone()
        .unwrap_or_else(|| "cdilga/roger-reviewer".to_owned());

    let version = if let Some(raw_version) = parsed.update_version.as_deref() {
        match normalize_calver_version(raw_version) {
            Ok(version) => version,
            Err(err) => {
                return blocked_response(
                    format!("invalid --version value: {err}"),
                    vec!["pass YYYY.MM.DD or YYYY.MM.DD-rc.N".to_owned()],
                    json!({"subcommand": "fetch", "reason_code": "invalid_version"}),
                );
            }
        }
    } else if let Some(version) = option_env!("ROGER_RELEASE_VERSION") {
        version.to_owned()
    } else {
        return blocked_response(
            "rr extension fetch is disabled for local/unpublished builds without embedded release metadata"
                .to_owned(),
            vec![
                "from a Roger dev workspace, run rr extension setup instead; it packs the extension from source via rr bridge pack-extension"
                    .to_owned(),
                "or pass --version <YYYY.MM.DD[-rc.N]> to fetch a specific published release's extension package"
                    .to_owned(),
            ],
            json!({"subcommand": "fetch", "reason_code": "local_or_unpublished_build"}),
        );
    };
    let download_root = parsed
        .update_download_root
        .clone()
        .unwrap_or_else(|| format!("https://github.com/{repo}/releases/download"));

    match fetch_and_install_extension_package_core(
        &runtime.store_root,
        &repo,
        &version,
        &download_root,
    ) {
        Ok(outcome) => CommandResponse {
            outcome: OutcomeKind::Complete,
            data: json!({
                "subcommand": "fetch",
                "version": outcome.version,
                "tag": outcome.tag,
                "archive": {
                    "name": outcome.archive_name,
                    "sha256": outcome.archive_sha256,
                    "url": outcome.archive_url,
                },
                "metadata_urls": {
                    "install_metadata": outcome.install_metadata_url,
                    "checksums": outcome.checksums_url,
                },
                "checksums_legacy_fallback": outcome.checksums_legacy_fallback,
                "package_dir": outcome.package_dir.to_string_lossy().to_string(),
                "fetch_manifest_path": outcome.fetch_manifest_path.to_string_lossy().to_string(),
            }),
            warnings: Vec::new(),
            repair_actions: vec![format!(
                "run rr extension setup --browser <edge|chrome|brave> to register the fetched package ({})",
                outcome.package_dir.to_string_lossy()
            )],
            message: format!(
                "extension package {} fetched, verified, and installed",
                outcome.version
            ),
        },
        Err(failure) => match failure.kind {
            ExtensionFetchFailureKind::Error => error_response(failure.message),
            ExtensionFetchFailureKind::Blocked => {
                let mut data = serde_json::Map::new();
                data.insert("subcommand".to_owned(), json!("fetch"));
                data.insert("reason_code".to_owned(), json!(failure.reason_code));
                if let Some(extra) = failure.detail.as_object() {
                    for (key, value) in extra {
                        data.insert(key.clone(), value.clone());
                    }
                }
                blocked_response(failure.message, failure.repair_actions, Value::Object(data))
            }
        },
    }
}

fn handle_extension_setup(
    parsed: &ParsedArgs,
    runtime: &CliRuntime,
    workspace_root: Option<&Path>,
) -> CommandResponse {
    let browser = parsed
        .extension_browser
        .clone()
        .unwrap_or(SupportedBrowser::Chrome);

    let (package_dir, package_source) = if let Some(explicit) =
        parsed.extension_package_dir.as_ref()
    {
        if !installed_extension_package_is_usable(explicit) {
            return blocked_response(
                format!(
                    "--package-dir {} does not contain an unpacked Roger extension (manifest.json missing)",
                    explicit.display()
                ),
                vec![
                    "pass --package-dir pointing at an unpacked Roger extension directory"
                        .to_owned(),
                    "or run rr extension fetch to install the published extension package"
                        .to_owned(),
                ],
                json!({
                    "subcommand": "setup",
                    "reason_code": "extension_package_dir_invalid",
                    "package_dir": explicit.to_string_lossy().to_string(),
                }),
            );
        }
        (
            explicit.to_string_lossy().to_string(),
            "explicit_package_dir",
        )
    } else if workspace_root.is_some() {
        let mut pack_parsed = parsed.clone();
        pack_parsed.command = CommandKind::Bridge;
        pack_parsed.bridge_command = Some(BridgeCommandKind::PackExtension);
        pack_parsed.bridge_extension_id = None;
        pack_parsed.bridge_binary_path = None;
        let pack = handle_bridge(&pack_parsed, runtime);
        if pack.outcome != OutcomeKind::Complete {
            return pack;
        }

        match pack
            .data
            .get("package_dir")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
        {
            Some(path) => (path, "dev_workspace"),
            None => {
                return error_response(
                    "extension setup failed to resolve package path from pack-extension output"
                        .to_owned(),
                );
            }
        }
    } else {
        match resolve_installed_extension_package_dir(&runtime.store_root) {
            Some((path, source)) => (path.to_string_lossy().to_string(), source),
            None => return extension_package_missing_response("setup", runtime),
        }
    };

    let load_step = format!(
        "open {} and load unpacked extension from {}",
        extension_browser_url(browser.clone()),
        package_dir
    );
    let guided_profile_root = extension_guided_profile_root(runtime, &browser);
    let (guided_browser_command, guided_browser_script_path_value, profile_hint_step) =
        extension_browser_launch_surface(
            workspace_root,
            &browser,
            &guided_profile_root,
            &package_dir,
            extension_browser_url(browser.clone()),
        );

    let registration_wait_budget_ms = extension_setup_registration_wait_ms();
    let Some((extension_id, extension_id_source, observed_during_setup_wait)) =
        discover_extension_id_for_extension_setup_with_wait(
            parsed,
            runtime,
            &browser,
            Path::new(&package_dir),
            registration_wait_budget_ms,
        )
    else {
        return CommandResponse {
            outcome: OutcomeKind::Blocked,
            data: json!({
                "subcommand": "setup",
                "reason_code": "extension_registration_missing",
                "browser": supported_browser_label(browser.clone()),
                "package_dir": package_dir,
                "package_source": package_source,
                "extension_id_registry_path": extension_id_registry_path(&runtime.store_root)
                    .to_string_lossy()
                    .to_string(),
                "guided_profile_root": guided_profile_root.to_string_lossy().to_string(),
                "guided_browser_script_path": guided_browser_script_path_value,
                "guided_browser_command": guided_browser_command,
                "registration_event": "browser_profile_identity_registered",
                "registration_wait_budget_ms": registration_wait_budget_ms,
                "manual_browser_step": load_step,
            }),
            warnings: vec![
                format!(
                    "extension identity registration has not been observed yet after waiting {registration_wait_budget_ms}ms"
                ),
                "guided setup needs one browser load/reload step before Roger can learn extension identity"
                    .to_owned(),
            ],
            repair_actions: vec![
                guided_browser_command,
                format!(
                    "rerun rr extension setup --browser {} after the guided browser launch",
                    supported_browser_label(browser.clone())
                ),
                profile_hint_step,
                load_step,
                "reload the browser extension while rr extension setup is running; if setup exits blocked, rerun rr extension setup"
                    .to_owned(),
                "if identity is still missing, this build still requires a repair/dev override via RR_BRIDGE_EXTENSION_ID or rr bridge install --extension-id <id>"
                    .to_owned(),
            ],
            message:
                "extension setup blocked because Roger has not observed extension identity registration yet"
                    .to_owned(),
        };
    };

    if let Err(err) = persist_extension_id(runtime, &extension_id) {
        return error_response(err);
    }

    let Some(host_os) = SupportedOs::current() else {
        return blocked_response(
            "rr extension setup supports macOS, Windows, and Linux only".to_owned(),
            vec!["run setup from a supported OS".to_owned()],
            json!({"reason_code": "unsupported_host_os"}),
        );
    };

    let install_root = parsed
        .bridge_install_root
        .clone()
        .or_else(default_extension_install_root);
    let Some(install_root) = install_root else {
        return blocked_response(
            "failed to determine install root; HOME is missing".to_owned(),
            vec!["pass --install-root <path> for recovery".to_owned()],
            json!({"reason_code": "install_root_missing"}),
        );
    };
    let custom_install_root_warning =
        custom_extension_install_root_warning(parsed, &browser, &install_root);

    let bridge_binary = match std::env::current_exe() {
        Ok(path) => path,
        Err(err) => {
            return blocked_response(
                format!("failed to resolve installed rr binary path: {err}"),
                vec!["rerun from an installed rr binary path".to_owned()],
                json!({"reason_code": "rr_binary_unresolved"}),
            );
        }
    };

    let manifest_path = native_host_install_path_for(&browser, host_os, &install_root);
    let launcher_path = native_host_launcher_path(&manifest_path, host_os);
    if let Err(err) = write_native_host_launcher(&launcher_path, &bridge_binary, host_os) {
        return error_response(err);
    }
    let manifest = NativeHostManifest::for_roger(&launcher_path, &extension_id);
    let manifest_bytes = match serde_json::to_vec_pretty(&manifest) {
        Ok(mut bytes) => {
            bytes.push(b'\n');
            bytes
        }
        Err(err) => {
            return error_response(format!(
                "failed to serialize native host manifest for {}: {err}",
                supported_browser_label(browser.clone())
            ));
        }
    };
    if let Some(parent) = manifest_path.parent() {
        if let Err(err) = fs::create_dir_all(parent) {
            return error_response(format!(
                "failed to create native host directory {}: {err}",
                parent.display()
            ));
        }
    }
    if let Err(err) = fs::write(&manifest_path, &manifest_bytes) {
        return error_response(format!(
            "failed to write native host manifest {}: {err}",
            manifest_path.display()
        ));
    }

    let mut doctor_args = parsed.clone();
    doctor_args.command = CommandKind::Extension;
    doctor_args.extension_command = Some(ExtensionCommandKind::Doctor);
    doctor_args.extension_browser = Some(browser.clone());
    doctor_args.bridge_install_root = Some(install_root.clone());
    let doctor = handle_extension_doctor(&doctor_args, runtime, workspace_root);
    if doctor.outcome != OutcomeKind::Complete {
        return CommandResponse {
            outcome: doctor.outcome,
            data: json!({
                "subcommand": "setup",
                "browser": supported_browser_label(browser.clone()),
                "package_dir": package_dir,
                "install_root": install_root.to_string_lossy().to_string(),
                "doctor": doctor.data,
            }),
            warnings: doctor.warnings,
            repair_actions: doctor.repair_actions,
            message: "extension setup completed with follow-up doctor failures".to_owned(),
        };
    }

    let mut warnings = Vec::new();
    if let Some(custom_install_root_warning) = custom_install_root_warning {
        warnings.push(custom_install_root_warning);
    }
    if extension_id_source == "packaged_manifest_key" {
        warnings.push(
            "Roger derived a deterministic extension id from the packaged manifest key; use the guided browser launch command (or manually load/reload the unpacked extension) before the first live PR-page launch."
                .to_owned(),
        );
    }

    let mut repair_actions = vec![format!(
        "rerun rr extension doctor --browser {} after browser or install changes",
        supported_browser_label(browser.clone())
    )];
    if extension_id_source == "packaged_manifest_key" {
        repair_actions.insert(0, guided_browser_command.clone());
        repair_actions.insert(1, load_step.clone());
    }

    CommandResponse {
        outcome: OutcomeKind::Complete,
        data: json!({
            "subcommand": "setup",
            "browser": supported_browser_label(browser.clone()),
            "package_dir": package_dir,
            "package_source": package_source,
            "extension_id": extension_id,
            "extension_id_source": extension_id_source,
            "registration_wait_budget_ms": registration_wait_budget_ms,
            "registration_observed_during_setup_wait": observed_during_setup_wait,
            "manual_browser_step": load_step,
            "guided_profile_root": guided_profile_root.to_string_lossy().to_string(),
            "guided_browser_script_path": guided_browser_script_path_value,
            "guided_browser_command": guided_browser_command,
            "install_root": install_root.to_string_lossy().to_string(),
            "host_binary": launcher_path.to_string_lossy().to_string(),
            "bridge_host_binary": bridge_binary.to_string_lossy().to_string(),
            "native_manifest_path": manifest_path.to_string_lossy().to_string(),
            "doctor": doctor.data,
        }),
        warnings,
        repair_actions,
        message: "extension setup completed".to_owned(),
    }
}

fn handle_extension_doctor(
    parsed: &ParsedArgs,
    runtime: &CliRuntime,
    workspace_root: Option<&Path>,
) -> CommandResponse {
    let browser = parsed
        .extension_browser
        .clone()
        .unwrap_or(SupportedBrowser::Chrome);
    let browser_label = supported_browser_label(browser.clone());

    let package_resolution =
        match resolve_extension_package_for_doctor(parsed, runtime, workspace_root) {
            Ok(resolution) => resolution,
            Err(response) => return response,
        };
    let package_dir = package_resolution.package_dir.clone();
    let package_source = package_resolution.source;
    let guided_profile_root = extension_guided_profile_root(runtime, &browser);
    let (guided_browser_command, guided_browser_script_path_value, _profile_hint_step) =
        extension_browser_launch_surface(
            workspace_root,
            &browser,
            &guided_profile_root,
            &package_dir.to_string_lossy(),
            extension_browser_url(browser.clone()),
        );
    let discovered_identity = discover_explicit_extension_id(parsed)
        .or_else(|| {
            discover_extension_id_from_browser_profiles(&browser, runtime, &package_dir)
                .map(|value| (value, "browser_profile_preferences"))
        })
        .or_else(|| discover_extension_id_from_packaged_manifest(&package_dir));
    let extension_id = discovered_identity
        .as_ref()
        .map(|(value, _source)| value.clone());
    let extension_id_source = discovered_identity.as_ref().map(|(_value, source)| *source);
    let Some(host_os) = SupportedOs::current() else {
        return blocked_response(
            "rr extension doctor supports macOS, Windows, and Linux only".to_owned(),
            vec!["run doctor from a supported OS".to_owned()],
            json!({"reason_code": "unsupported_host_os"}),
        );
    };
    let install_root = parsed
        .bridge_install_root
        .clone()
        .or_else(default_extension_install_root);
    let Some(install_root) = install_root else {
        return blocked_response(
            "failed to determine install root; HOME is missing".to_owned(),
            vec!["pass --install-root <path> for recovery".to_owned()],
            json!({"reason_code": "install_root_missing"}),
        );
    };
    let custom_install_root_warning =
        custom_extension_install_root_warning(parsed, &browser, &install_root);

    let manifest_path = native_host_install_path_for(&browser, host_os, &install_root);
    let mut checks: Vec<Value> = Vec::new();
    let package_exists = package_dir.exists();
    checks.push(json!({
        "name": "extension_package_present",
        "ok": package_exists,
        "detail": package_dir.to_string_lossy().to_string(),
    }));

    let extension_id_present = extension_id.as_deref().is_some_and(|id| !id.is_empty());
    checks.push(json!({
        "name": "extension_identity_discovered",
        "ok": extension_id_present,
        "detail": {
            "extension_id": extension_id.clone(),
            "source": extension_id_source,
        },
    }));

    let manifest_exists = manifest_path.exists();
    checks.push(json!({
        "name": "native_host_manifest_present",
        "ok": manifest_exists,
        "detail": manifest_path.to_string_lossy().to_string(),
    }));

    let mut manifest_allows_origin = false;
    let mut host_binary_exists = false;
    let mut host_binary_path: Option<String> = None;
    if manifest_exists {
        if let Ok(text) = fs::read_to_string(&manifest_path) {
            if let Ok(manifest) = serde_json::from_str::<NativeHostManifest>(&text) {
                host_binary_exists = Path::new(&manifest.path).exists();
                host_binary_path = Some(manifest.path.clone());
                if let Some(extension_id) = extension_id.as_ref() {
                    let expected_origin = format!("chrome-extension://{extension_id}/");
                    manifest_allows_origin = manifest
                        .allowed_origins
                        .iter()
                        .any(|origin| origin == &expected_origin);
                }
            }
        }
    }
    checks.push(json!({
        "name": "native_host_binary_present",
        "ok": host_binary_exists,
        "detail": host_binary_path
            .clone()
            .unwrap_or_else(|| manifest_path.to_string_lossy().to_string()),
    }));
    checks.push(json!({
        "name": "native_host_origin_matches_extension_id",
        "ok": manifest_allows_origin,
        "detail": browser_label,
    }));

    let all_ok = checks
        .iter()
        .all(|entry| entry.get("ok").and_then(Value::as_bool).unwrap_or(false));

    if !all_ok {
        let (reason_code, warning, repair_actions) = if !extension_id_present {
            (
                "extension_registration_missing",
                "extension doctor did not observe browser-side extension identity registration"
                    .to_owned(),
                vec![
                    guided_browser_command.clone(),
                    format!("rerun rr extension setup --browser {browser_label}"),
                    format!(
                        "open {} and reload the unpacked extension, then rerun setup",
                        extension_browser_url(browser.clone())
                    ),
                ],
            )
        } else if !manifest_exists {
            (
                "native_host_manifest_missing",
                "extension doctor did not find the Native Messaging host manifest".to_owned(),
                vec![
                    format!("rerun rr extension setup --browser {browser_label}"),
                    "verify rr is installed and writable under the selected install root"
                        .to_owned(),
                ],
            )
        } else if !host_binary_exists {
            (
                "native_host_binary_missing",
                "extension doctor found a host manifest but the referenced rr binary is missing"
                    .to_owned(),
                vec![
                    format!("rerun rr extension setup --browser {browser_label}"),
                    "if the install moved, rerun setup from the active rr install".to_owned(),
                ],
            )
        } else if !manifest_allows_origin {
            (
                "native_host_origin_mismatch",
                "extension doctor found a host manifest whose allowed origin does not match discovered extension identity".to_owned(),
                vec![
                    format!("rerun rr extension setup --browser {browser_label}"),
                    "reload the extension and rerun doctor to confirm matching identity".to_owned(),
                ],
            )
        } else {
            (
                "extension_setup_incomplete",
                "extension doctor detected missing or inconsistent setup prerequisites".to_owned(),
                vec![
                    format!("rerun rr extension setup --browser {browser_label}"),
                    "if setup remains blocked, complete the one browser load step and rerun setup"
                        .to_owned(),
                ],
            )
        };

        let mut warnings = vec![warning];
        if let Some(custom_warning) = custom_install_root_warning {
            warnings.push(custom_warning);
        }

        return CommandResponse {
            outcome: OutcomeKind::Blocked,
            data: json!({
                "subcommand": "doctor",
                "reason_code": reason_code,
                "browser": browser_label,
                "package_dir": package_dir.to_string_lossy().to_string(),
                "package_source": package_source,
                "guided_profile_root": guided_profile_root.to_string_lossy().to_string(),
                "guided_browser_script_path": guided_browser_script_path_value,
                "guided_browser_command": guided_browser_command,
                "install_root": install_root.to_string_lossy().to_string(),
                "checks": checks,
            }),
            warnings,
            repair_actions,
            message: "extension doctor failed closed".to_owned(),
        };
    }

    let mut data = json!({
        "subcommand": "doctor",
        "browser": browser_label,
        "package_dir": package_dir.to_string_lossy().to_string(),
        "package_source": package_source,
        "guided_profile_root": guided_profile_root.to_string_lossy().to_string(),
        "guided_browser_script_path": guided_browser_script_path_value,
        "guided_browser_command": guided_browser_command,
        "install_root": install_root.to_string_lossy().to_string(),
        "extension_id": extension_id,
        "extension_id_source": extension_id_source,
        "checks": checks,
    });
    let mut warnings: Vec<String> = custom_install_root_warning
        .into_iter()
        .chain((extension_id_source == Some("packaged_manifest_key")).then_some(
            "doctor is relying on the packaged manifest key for extension identity; if the unpacked extension is not yet active in the target browser profile, load or reload it once before the first live launch."
                .to_owned(),
        ))
        .collect();

    // The live section is additive: it only appears when --live is passed, so the
    // default doctor behavior and payload shape are unchanged.
    let mut outcome = OutcomeKind::Complete;
    let mut repair_actions: Vec<String> = Vec::new();
    let mut message = "extension doctor checks passed".to_owned();
    if parsed.live {
        let live =
            run_extension_doctor_live_handshake(host_binary_path.as_deref(), &runtime.store_root);
        data["live"] = live.section;
        if live.ok {
            message = "extension doctor checks passed with live native-host handshake".to_owned();
        } else {
            outcome = OutcomeKind::Blocked;
            repair_actions = live.repair_actions;
            message = "extension doctor live handshake failed".to_owned();
            if let Some(warning) = live.warning {
                warnings.push(warning);
            }
        }
    }

    CommandResponse {
        outcome,
        data,
        warnings,
        repair_actions,
        message,
    }
}

struct DoctorLiveHandshake {
    ok: bool,
    section: Value,
    repair_actions: Vec<String>,
    warning: Option<String>,
}

fn doctor_live_handshake_failed(
    preflight_section: &Value,
    launcher_path: Option<&str>,
    reason: &str,
    detail: String,
    repair_actions: Vec<String>,
) -> DoctorLiveHandshake {
    DoctorLiveHandshake {
        ok: false,
        section: json!({
            "preflight": preflight_section.clone(),
            "live_handshake": "failed",
            "reason": reason,
            "detail": detail,
            "launcher_path": launcher_path,
        }),
        repair_actions,
        warning: Some(format!("extension doctor live handshake failed ({reason})")),
    }
}

/// Drive a real, bounded native-host handshake: report bridge preflight, then
/// spawn the installed launcher, write a length-prefixed StatusProbe to its
/// stdin, and read back one length-prefixed reply with a 10s ceiling. All
/// failure modes are typed (launcher_missing, spawn_failed, timeout,
/// malformed_reply, preflight_failed) with concrete repair actions. gh auth is
/// a soft preflight signal and never fails the handshake on its own.
fn run_extension_doctor_live_handshake(
    launcher_path: Option<&str>,
    store_root: &Path,
) -> DoctorLiveHandshake {
    let rr_binary = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("rr"));
    let preflight = BridgePreflight::check(&rr_binary, store_root);
    let preflight_section = json!({
        "roger_binary_found": preflight.roger_binary_found,
        "roger_data_dir_exists": preflight.roger_data_dir_exists,
        "gh_available": preflight.gh_available,
        "ready": preflight.is_ready(),
    });

    // Hard prerequisites for any handshake: the rr binary and the local store
    // must both exist. gh is intentionally excluded here.
    if !preflight.roger_binary_found || !preflight.roger_data_dir_exists {
        let detail = preflight
            .guidance(&rr_binary)
            .unwrap_or_else(|| "bridge preflight not ready".to_owned());
        return doctor_live_handshake_failed(
            &preflight_section,
            launcher_path,
            "preflight_failed",
            detail,
            vec![
                "run rr init to bootstrap the local store".to_owned(),
                "reinstall the native host with rr extension setup, then rerun rr extension doctor --live".to_owned(),
            ],
        );
    }

    let Some(launcher) = launcher_path else {
        return doctor_live_handshake_failed(
            &preflight_section,
            launcher_path,
            "launcher_missing",
            "native host launcher path is unknown".to_owned(),
            vec!["rerun rr extension setup to install the native host launcher".to_owned()],
        );
    };
    if !Path::new(launcher).exists() {
        return doctor_live_handshake_failed(
            &preflight_section,
            launcher_path,
            "launcher_missing",
            format!("native host launcher not found at {launcher}"),
            vec!["rerun rr extension setup to reinstall the native host launcher".to_owned()],
        );
    }

    let probe = json!({
        "type": "roger_bridge_status",
        "owner": "roger-reviewer",
        "repo": "doctor-live-handshake",
        "pr_number": 0,
    });
    let probe_bytes = match serde_json::to_vec(&probe) {
        Ok(bytes) => bytes,
        Err(err) => {
            return doctor_live_handshake_failed(
                &preflight_section,
                launcher_path,
                "spawn_failed",
                format!("failed to encode status probe: {err}"),
                vec!["report this as an internal rr defect".to_owned()],
            );
        }
    };
    let mut wire = Vec::with_capacity(4 + probe_bytes.len());
    wire.extend_from_slice(&(probe_bytes.len() as u32).to_le_bytes());
    wire.extend_from_slice(&probe_bytes);

    let mut child = match ProcessCommand::new(launcher)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("RR_STORE_ROOT", store_root)
        .current_dir(store_root)
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            return doctor_live_handshake_failed(
                &preflight_section,
                launcher_path,
                "spawn_failed",
                format!("failed to spawn native host launcher {launcher}: {err}"),
                vec!["confirm the launcher is executable and rerun rr extension setup".to_owned()],
            );
        }
    };

    if let Some(mut stdin) = child.stdin.take() {
        // Best-effort write; a broken pipe surfaces as a malformed/absent reply.
        let _ = stdin.write_all(&wire);
        // Drop stdin here to signal EOF so the host stops reading and replies.
    }

    let Some(mut stdout) = child.stdout.take() else {
        let _ = child.kill();
        return doctor_live_handshake_failed(
            &preflight_section,
            launcher_path,
            "spawn_failed",
            "native host launcher exposed no stdout handle".to_owned(),
            vec!["rerun rr extension doctor --live after reinstalling the native host".to_owned()],
        );
    };

    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        let _ = tx.send(buf);
    });

    let buf = match rx.recv_timeout(Duration::from_secs(10)) {
        Ok(buf) => buf,
        Err(_) => {
            let _ = child.kill();
            return doctor_live_handshake_failed(
                &preflight_section,
                launcher_path,
                "timeout",
                "native host did not reply within 10s".to_owned(),
                vec![
                    "run rr doctor to inspect local setup".to_owned(),
                    "reinstall the native host with rr extension setup and rerun rr extension doctor --live".to_owned(),
                ],
            );
        }
    };
    let _ = child.wait();

    if buf.len() < 4 {
        return doctor_live_handshake_failed(
            &preflight_section,
            launcher_path,
            "malformed_reply",
            format!(
                "native host reply missing 4-byte length prefix ({} bytes)",
                buf.len()
            ),
            vec!["run rr extension doctor to inspect the native host manifest".to_owned()],
        );
    }
    let len = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
    if buf.len() < 4 + len {
        return doctor_live_handshake_failed(
            &preflight_section,
            launcher_path,
            "malformed_reply",
            "native host reply truncated relative to its length prefix".to_owned(),
            vec!["run rr extension doctor to inspect the native host manifest".to_owned()],
        );
    }
    let reply = match serde_json::from_slice::<Value>(&buf[4..4 + len]) {
        Ok(value) if value.is_object() => value,
        Ok(_) => {
            return doctor_live_handshake_failed(
                &preflight_section,
                launcher_path,
                "malformed_reply",
                "native host reply was valid JSON but not an object".to_owned(),
                vec!["run rr extension doctor to inspect the native host manifest".to_owned()],
            );
        }
        Err(err) => {
            return doctor_live_handshake_failed(
                &preflight_section,
                launcher_path,
                "malformed_reply",
                format!("native host reply was not valid JSON: {err}"),
                vec!["run rr extension doctor to inspect the native host manifest".to_owned()],
            );
        }
    };

    DoctorLiveHandshake {
        ok: true,
        section: json!({
            "preflight": preflight_section,
            "live_handshake": "ok",
            "reason": Value::Null,
            "launcher_path": launcher,
            "reply": reply,
        }),
        repair_actions: Vec::new(),
        warning: None,
    }
}

fn handle_bridge(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    let Some(subcommand) = parsed.bridge_command else {
        return error_response("rr bridge missing subcommand".to_owned());
    };

    // Contract/pack subcommands operate on the dev source tree and still
    // require workspace markers; host registration (install/uninstall) must
    // keep working from installed binaries outside the Roger repository.
    let workspace_root = match find_workspace_root(&runtime.cwd) {
        Some(root) => root,
        None if matches!(
            subcommand,
            BridgeCommandKind::Install | BridgeCommandKind::Uninstall
        ) =>
        {
            // install/uninstall never touch the generated contract path; a
            // cwd-rooted placeholder keeps the shared prelude unchanged.
            runtime.cwd.clone()
        }
        None => {
            return blocked_response(
                "failed to resolve Roger workspace root for bridge contract commands".to_owned(),
                vec![
                    "run rr bridge export-contracts/verify-contracts/pack-extension from the Roger repository root (or a child directory)"
                        .to_owned(),
                ],
                json!({"reason_code": "workspace_root_not_found"}),
            );
        }
    };

    let generated_path = workspace_root.join("apps/extension/src/generated/bridge.ts");
    let expected = bridge_contract_snapshot();

    match subcommand {
        BridgeCommandKind::ExportContracts => {
            let Some(parent) = generated_path.parent() else {
                return error_response(format!(
                    "invalid generated contract path: {}",
                    generated_path.display()
                ));
            };

            if let Err(err) = fs::create_dir_all(parent) {
                return error_response(format!(
                    "failed to create generated contract directory: {err}"
                ));
            }
            if let Err(err) = fs::write(&generated_path, expected) {
                return error_response(format!("failed to write generated bridge contract: {err}"));
            }

            CommandResponse {
                outcome: OutcomeKind::Complete,
                data: json!({
                    "subcommand": "export-contracts",
                    "output_path": generated_path.to_string_lossy().to_string(),
                    "bytes_written": expected.len(),
                }),
                warnings: Vec::new(),
                repair_actions: Vec::new(),
                message: "bridge contracts exported".to_owned(),
            }
        }
        BridgeCommandKind::VerifyContracts => {
            let existing = match fs::read_to_string(&generated_path) {
                Ok(text) => text,
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                    return CommandResponse {
                        outcome: OutcomeKind::RepairNeeded,
                        data: json!({
                            "subcommand": "verify-contracts",
                            "reason_code": "bridge_contract_missing",
                            "generated_path": generated_path.to_string_lossy().to_string(),
                        }),
                        warnings: vec![
                            "generated bridge contract is missing from the extension tree"
                                .to_owned(),
                        ],
                        repair_actions: vec!["rr bridge export-contracts".to_owned()],
                        message: "bridge contract verification failed".to_owned(),
                    };
                }
                Err(err) => {
                    return error_response(format!(
                        "failed to read generated bridge contract: {err}"
                    ));
                }
            };

            if existing != expected {
                return CommandResponse {
                    outcome: OutcomeKind::RepairNeeded,
                    data: json!({
                        "subcommand": "verify-contracts",
                        "reason_code": "bridge_contract_drift",
                        "generated_path": generated_path.to_string_lossy().to_string(),
                    }),
                    warnings: vec![
                        "generated bridge contract is stale relative to Rust-owned snapshot"
                            .to_owned(),
                    ],
                    repair_actions: vec!["rr bridge export-contracts".to_owned()],
                    message: "bridge contract verification failed".to_owned(),
                };
            }

            CommandResponse {
                outcome: OutcomeKind::Complete,
                data: json!({
                    "subcommand": "verify-contracts",
                    "generated_path": generated_path.to_string_lossy().to_string(),
                    "matches_expected": true,
                }),
                warnings: Vec::new(),
                repair_actions: Vec::new(),
                message: "bridge contract verification passed".to_owned(),
            }
        }
        BridgeCommandKind::PackExtension => {
            let extension_root = workspace_root.join("apps/extension");
            if !generated_path.exists() {
                return CommandResponse {
                    outcome: OutcomeKind::RepairNeeded,
                    data: json!({
                        "subcommand": "pack-extension",
                        "reason_code": "bridge_contract_missing",
                        "generated_path": generated_path.to_string_lossy().to_string(),
                    }),
                    warnings: vec![
                        "generated bridge contract is missing from extension tree".to_owned(),
                    ],
                    repair_actions: vec![
                        "rr bridge export-contracts".to_owned(),
                        "re-run rr bridge pack-extension".to_owned(),
                    ],
                    message: "extension packaging blocked by missing generated contract".to_owned(),
                };
            }

            let manifest_template_path = extension_root.join("manifest.template.json");
            let mut manifest_json = match read_extension_manifest_template(&manifest_template_path)
            {
                Ok(value) => value,
                Err(err) => return error_response(err),
            };
            let build_version = derive_extension_build_version(&workspace_root, &manifest_json);
            let package_dir_name = extension_package_dir_name(&manifest_json);
            manifest_json["version"] = Value::String(build_version.manifest_version.clone());
            manifest_json["version_name"] = Value::String(build_version.version_name.clone());
            let version = build_version.manifest_version.clone();

            let output_root = parsed
                .bridge_output_dir
                .clone()
                .unwrap_or_else(|| workspace_root.join("target/bridge/extension"));
            let package_dir = output_root.join(package_dir_name);
            if package_dir.exists() {
                let _ = fs::remove_dir_all(&package_dir);
            }
            if let Err(err) = fs::create_dir_all(&package_dir) {
                return error_response(format!(
                    "failed to create extension package directory: {err}"
                ));
            }

            let manifest_output_path = package_dir.join("manifest.json");
            let rendered_manifest = match serde_json::to_string_pretty(&manifest_json) {
                Ok(text) => format!("{text}\n"),
                Err(err) => {
                    return error_response(format!("failed to render manifest json: {err}"));
                }
            };
            if let Err(err) = fs::write(&manifest_output_path, rendered_manifest.as_bytes()) {
                return error_response(format!("failed to write packaged manifest.json: {err}"));
            }

            let src_root = extension_root.join("src");
            let static_root = extension_root.join("static");
            let assets_root = extension_root.join("assets");
            if let Err(err) = copy_dir_recursive(&src_root, &package_dir.join("src")) {
                return error_response(format!("failed to copy extension src tree: {err}"));
            }
            if static_root.exists() {
                if let Err(err) = copy_dir_recursive(&static_root, &package_dir.join("static")) {
                    return error_response(format!("failed to copy extension static tree: {err}"));
                }
            }
            if assets_root.exists() {
                if let Err(err) = copy_dir_recursive(&assets_root, &package_dir.join("assets")) {
                    return error_response(format!("failed to copy extension assets tree: {err}"));
                }
            }
            // Slim the packed payload to runtime-only files: drop test files
            // (*.test.js) and the non-runnable generated TypeScript contract
            // (src/generated/bridge.ts). Pruning here — before the checksum and
            // asset-manifest pass below — keeps SHA256SUMS, asset-manifest.json,
            // and the published zip describing exactly the same file set.
            if let Err(err) = prune_non_runtime_extension_files(&package_dir) {
                return error_response(format!(
                    "failed to slim packaged extension to runtime files: {err}"
                ));
            }
            if let Err(err) = validate_packaged_manifest_icon_paths(&package_dir, &manifest_json) {
                return error_response(err);
            }

            let mut files = match collect_relative_files(&package_dir) {
                Ok(items) => items,
                Err(err) => {
                    return error_response(format!("failed to collect packaged files: {err}"));
                }
            };
            files.sort();

            let mut checksums = Vec::with_capacity(files.len());
            let mut checksum_lines = Vec::with_capacity(files.len());
            for rel in files {
                let abs = package_dir.join(&rel);
                let bytes = match fs::read(&abs) {
                    Ok(bytes) => bytes,
                    Err(err) => {
                        return error_response(format!(
                            "failed to read packaged file {}: {err}",
                            abs.display()
                        ));
                    }
                };
                let digest = sha256_hex(&bytes);
                let rel_str = rel.to_string_lossy().to_string();
                checksum_lines.push(format!("{digest}  {rel_str}"));
                checksums.push(json!({
                    "path": rel_str,
                    "sha256": digest,
                    "bytes": bytes.len(),
                }));
            }
            checksum_lines.sort();
            let checksum_manifest = checksum_lines.join("\n") + "\n";
            let checksum_manifest_path = package_dir.join("SHA256SUMS");
            if let Err(err) = fs::write(&checksum_manifest_path, checksum_manifest.as_bytes()) {
                return error_response(format!("failed to write SHA256SUMS: {err}"));
            }

            let package_digest = sha256_hex(checksum_manifest.as_bytes());
            let asset_manifest_path = package_dir.join("asset-manifest.json");
            let asset_manifest = json!({
                "artifact_name": format!("roger-extension-{version}-unpacked"),
                "version": version,
                "version_name": build_version.version_name,
                "package_digest_sha256": package_digest,
                "checksums_path": checksum_manifest_path.to_string_lossy().to_string(),
                "files": checksums,
            });
            let asset_manifest_bytes = match serde_json::to_vec_pretty(&asset_manifest) {
                Ok(bytes) => bytes,
                Err(err) => {
                    return error_response(format!("failed to serialize asset manifest: {err}"));
                }
            };
            if let Err(err) = fs::write(&asset_manifest_path, &asset_manifest_bytes) {
                return error_response(format!("failed to write asset-manifest.json: {err}"));
            }

            CommandResponse {
                outcome: OutcomeKind::Complete,
                data: json!({
                    "subcommand": "pack-extension",
                    "package_dir": package_dir.to_string_lossy().to_string(),
                    "manifest_path": manifest_output_path.to_string_lossy().to_string(),
                    "asset_manifest_path": asset_manifest_path.to_string_lossy().to_string(),
                    "checksums_path": checksum_manifest_path.to_string_lossy().to_string(),
                    "package_digest_sha256": package_digest,
                    "version": build_version.manifest_version,
                    "version_name": build_version.version_name,
                    "install_mode": "unpacked_sideload",
                    "installs_browser_extension": false,
                }),
                warnings: Vec::new(),
                repair_actions: vec![
                    "load unpacked extension from package_dir in Chrome/Brave/Edge".to_owned(),
                ],
                message: "extension package assembled".to_owned(),
            }
        }
        BridgeCommandKind::Install => {
            let Some(host_os) = SupportedOs::current() else {
                return blocked_response(
                    "rr bridge install supports macOS, Windows, and Linux only".to_owned(),
                    vec![
                        "run install from a supported OS or use release-package-bridge artifacts"
                            .to_owned(),
                    ],
                    json!({"reason_code": "unsupported_host_os"}),
                );
            };

            let registry_path = extension_id_registry_path(&runtime.store_root);
            let Some((extension_id, extension_id_source)) = discover_extension_id(parsed, runtime)
            else {
                return blocked_response(
                    "rr bridge install could not discover extension identity for setup".to_owned(),
                    vec![
                        "run rr extension setup to prepare unpacked extension guidance and register identity".to_owned(),
                        format!(
                            "or write the discovered id to {}",
                            registry_path.to_string_lossy()
                        ),
                        "or pass --extension-id <chrome-extension-id> as a repair/dev override".to_owned(),
                        "or set RR_BRIDGE_EXTENSION_ID for non-interactive environments".to_owned(),
                    ],
                    json!({
                        "reason_code": "extension_id_discovery_failed",
                        "extension_id_registry_path": registry_path.to_string_lossy().to_string(),
                    }),
                );
            };

            let (bridge_binary, bridge_binary_source) = if let Some(path) =
                parsed.bridge_binary_path.clone()
            {
                (path, "explicit_flag")
            } else if let Some(path) = std::env::var("RR_BRIDGE_HOST_BINARY")
                .ok()
                .map(PathBuf::from)
            {
                (path, "env_rr_bridge_host_binary")
            } else {
                let installed_rr = match std::env::current_exe() {
                    Ok(path) => path,
                    Err(err) => {
                        return blocked_response(
                            format!("failed to resolve installed rr binary path: {err}"),
                            vec![
                                "rerun from an installed rr binary path".to_owned(),
                                "or pass --bridge-binary <path-to-rr-binary> as a repair/dev override"
                                    .to_owned(),
                            ],
                            json!({"reason_code": "rr_binary_unresolved"}),
                        );
                    }
                };
                (installed_rr, "installed_rr_current_exe")
            };
            if !bridge_binary.exists() {
                return blocked_response(
                    format!(
                        "bridge host binary was not found at {}",
                        bridge_binary.display()
                    ),
                    {
                        let mut actions = vec![
                            "omit --bridge-binary to use installed rr host mode".to_owned(),
                            "or pass --bridge-binary <path-to-rr-binary> as a repair/dev override"
                                .to_owned(),
                        ];
                        if bridge_binary_source == "installed_rr_current_exe" {
                            actions.insert(0, "rerun from an installed rr binary path".to_owned());
                        } else {
                            actions.insert(
                                0,
                                "verify RR_BRIDGE_HOST_BINARY/--bridge-binary points to an installed rr binary"
                                    .to_owned(),
                            );
                        }
                        actions
                    },
                    json!({
                        "reason_code": "bridge_binary_missing",
                        "bridge_binary": bridge_binary.to_string_lossy().to_string(),
                        "bridge_binary_source": bridge_binary_source,
                    }),
                );
            }

            let install_root = parsed
                .bridge_install_root
                .clone()
                .or_else(|| std::env::var("HOME").ok().map(PathBuf::from));
            let Some(install_root) = install_root else {
                return blocked_response(
                    "failed to determine install root; HOME is missing".to_owned(),
                    vec!["pass --install-root <path>".to_owned()],
                    json!({"reason_code": "install_root_missing"}),
                );
            };

            let mut installed_assets = Vec::new();
            for browser in [
                SupportedBrowser::Chrome,
                SupportedBrowser::Edge,
                SupportedBrowser::Brave,
            ] {
                let path = native_host_install_path_for(&browser, host_os, &install_root);
                let launcher_path = native_host_launcher_path(&path, host_os);
                if let Err(err) =
                    write_native_host_launcher(&launcher_path, &bridge_binary, host_os)
                {
                    return error_response(err);
                }
                let manifest = NativeHostManifest::for_roger(&launcher_path, &extension_id);
                let bytes = match serde_json::to_vec_pretty(&manifest) {
                    Ok(mut bytes) => {
                        bytes.push(b'\n');
                        bytes
                    }
                    Err(err) => {
                        return error_response(format!(
                            "failed to serialize native manifest for {browser:?}: {err}"
                        ));
                    }
                };
                if let Some(parent) = path.parent() {
                    if let Err(err) = fs::create_dir_all(parent) {
                        return error_response(format!(
                            "failed to create native host directory {}: {err}",
                            parent.display()
                        ));
                    }
                }
                if let Err(err) = fs::write(&path, &bytes) {
                    return error_response(format!(
                        "failed to install native host manifest {}: {err}",
                        path.display()
                    ));
                }
                installed_assets.push(json!({
                    "asset_kind": "native_host_manifest",
                    "browser": format!("{browser:?}").to_ascii_lowercase(),
                    "path": path.to_string_lossy().to_string(),
                    "sha256": sha256_hex(&bytes),
                    "bytes": bytes.len(),
                }));
            }

            let mut warnings = vec![
                "bridge install registers host assets only; browser extension install remains manual"
                    .to_owned(),
            ];
            if bridge_binary_source != "installed_rr_current_exe" {
                warnings.push(
                    "manual --bridge-binary/RR_BRIDGE_HOST_BINARY override is repair/dev-only; normal setup uses installed rr host mode".to_owned(),
                );
            }
            if extension_id_source == "explicit_flag" {
                warnings.push(
                    "manual --extension-id override is repair/dev-only; prefer discovered identity from rr extension setup".to_owned(),
                );
            }

            CommandResponse {
                outcome: OutcomeKind::Complete,
                data: json!({
                    "subcommand": "install",
                    "platform": host_os.as_str(),
                    "install_root": install_root.to_string_lossy().to_string(),
                    "extension_id_source": extension_id_source,
                    "bridge_binary_source": bridge_binary_source,
                    "bridge_host_binary": bridge_binary.to_string_lossy().to_string(),
                    "assets": installed_assets,
                    "installs_browser_extension": false,
                }),
                warnings,
                repair_actions: Vec::new(),
                message: "bridge registration assets installed".to_owned(),
            }
        }
        BridgeCommandKind::Uninstall => {
            let Some(host_os) = SupportedOs::current() else {
                return blocked_response(
                    "rr bridge uninstall supports macOS, Windows, and Linux only".to_owned(),
                    vec!["run uninstall from a supported OS".to_owned()],
                    json!({"reason_code": "unsupported_host_os"}),
                );
            };
            let install_root = parsed
                .bridge_install_root
                .clone()
                .or_else(|| std::env::var("HOME").ok().map(PathBuf::from));
            let Some(install_root) = install_root else {
                return blocked_response(
                    "failed to determine install root; HOME is missing".to_owned(),
                    vec!["pass --install-root <path>".to_owned()],
                    json!({"reason_code": "install_root_missing"}),
                );
            };

            let mut removed = Vec::new();
            let mut missing = Vec::new();
            for browser in [
                SupportedBrowser::Chrome,
                SupportedBrowser::Edge,
                SupportedBrowser::Brave,
            ] {
                let path = native_host_install_path_for(&browser, host_os, &install_root);
                if path.exists() {
                    match fs::remove_file(&path) {
                        Ok(()) => removed.push(path.to_string_lossy().to_string()),
                        Err(err) => {
                            return error_response(format!(
                                "failed to remove native manifest {}: {err}",
                                path.display()
                            ));
                        }
                    }
                } else {
                    missing.push(path.to_string_lossy().to_string());
                }
            }

            CommandResponse {
                outcome: OutcomeKind::Complete,
                data: json!({
                    "subcommand": "uninstall",
                    "surface": "bridge",
                    "preferred_surface": "rr extension uninstall",
                    "platform": host_os.as_str(),
                    "install_root": install_root.to_string_lossy().to_string(),
                    "removed": removed,
                    "missing": missing,
                    "installs_browser_extension": false,
                }),
                warnings: vec![BRIDGE_UNINSTALL_REPAIR_ALIAS_WARNING.to_owned()],
                repair_actions: vec![
                    "prefer rr extension uninstall for operator workflows".to_owned(),
                ],
                message: "bridge registration assets removed (repair alias path)".to_owned(),
            }
        }
    }
}

fn persist_launch_attempt_state(
    store: &RogerStore,
    attempt_id: &str,
    state: LaunchAttemptState,
    final_session_id: Option<&str>,
    launch_binding_id: Option<&str>,
    provider_session_id: Option<&str>,
    verified_locator: Option<&SessionLocator>,
    failure_reason: Option<&str>,
) -> std::result::Result<(), String> {
    store
        .update_launch_attempt(UpdateLaunchAttempt {
            id: attempt_id,
            state,
            final_session_id,
            launch_binding_id,
            provider_session_id,
            verified_locator,
            failure_reason,
        })
        .map(|_| ())
        .map_err(|err| format!("failed to persist launch attempt {attempt_id}: {err}"))
}

fn verified_provider_session_id<'a>(
    expected_provider: &str,
    locator: &'a SessionLocator,
) -> std::result::Result<&'a str, String> {
    if locator.provider != expected_provider {
        return Err(format!(
            "provider verification mismatch: expected '{expected_provider}', got '{}'",
            locator.provider
        ));
    }
    let session_id = locator.session_id.trim();
    if session_id.is_empty() {
        Err(format!(
            "provider '{}' returned an empty session identifier",
            locator.provider
        ))
    } else {
        Ok(session_id)
    }
}

const COPILOT_FEATURE_GATED_STATUS: &str = "feature_gated_bounded_live";
const COPILOT_FEATURE_GATED_TIER: &str = "tier_b_feature_gated";
const COPILOT_FEATURE_GATED_SURFACE_CLASS: &str = "review_bounded";
const COPILOT_FEATURE_GATED_STATUS_REASON: &str =
    "feature_gate_enabled_tier_b_reopen_return_with_reseed_fallback";
const COPILOT_FEATURE_GATED_NOTES: &str = "feature-gated bounded tier-b continuity path: verified start, locator/session-id reopen, rr return, and honest ResumeBundle reseed fallback; no default public live claim";

// Gate-OFF projection: copilot is a documented feature-gated tier-b provider
// that is disabled-but-enableable, NOT a genuinely-planned tier-a provider.
// Roger must not borrow the planned_not_live/tier_a_planned classification for
// it (that is reserved for providers with no live path at all); instead it is
// classified as feature-gated tier-b held behind the documented env gate.
const COPILOT_FEATURE_GATED_DISABLED_STATUS: &str = "feature_gated_disabled";
const COPILOT_FEATURE_GATED_DISABLED_STATUS_REASON: &str =
    "feature_gate_disabled_enable_rr_enable_copilot_provider";
const COPILOT_FEATURE_GATED_DISABLED_NOTES: &str = "feature-gated bounded tier-b continuity path, currently disabled; enable with RR_ENABLE_COPILOT_PROVIDER=1 for verified start, locator/session-id reopen, rr return, and honest ResumeBundle reseed fallback";

/// Capability projection for copilot when the feature gate is OFF.
///
/// Copilot is documented (rr --help, README.md, AGENTS.md) as feature-gated
/// bounded tier-b support enabled with `RR_ENABLE_COPILOT_PROVIDER=1`. With the
/// gate off it is disabled-but-enableable, so it must be classified as
/// feature-gated tier-b (not the `planned_not_live`/`tier_a_planned`/
/// `admission_pending` classification reserved for genuinely-planned providers).
/// It still does not claim a live surface: `supports.doctor` stays true (doctor
/// can inspect prerequisites) but the live-launch capabilities stay false until
/// the gate is enabled.
fn copilot_feature_gated_disabled_provider_capability(runtime: &CliRuntime) -> Value {
    let mut capability =
        match resolved_routine_surface_baseline(runtime, session_copilot::PROVIDER_ID) {
            Ok(baseline) => provider_capability_projection(
                &baseline.provider,
                Some(COPILOT_FEATURE_GATED_DISABLED_STATUS_REASON),
            ),
            Err(_) => provider_capability(session_copilot::PROVIDER_ID),
        };

    if let Some(provider_obj) = capability.as_object_mut() {
        provider_obj.insert(
            "status".to_owned(),
            Value::String(COPILOT_FEATURE_GATED_DISABLED_STATUS.to_owned()),
        );
        provider_obj.insert(
            "tier".to_owned(),
            Value::String(COPILOT_FEATURE_GATED_TIER.to_owned()),
        );
        provider_obj.insert(
            "support_tier".to_owned(),
            Value::String(COPILOT_FEATURE_GATED_TIER.to_owned()),
        );
        provider_obj.insert(
            "surface_class".to_owned(),
            Value::String(COPILOT_FEATURE_GATED_SURFACE_CLASS.to_owned()),
        );
        provider_obj.insert(
            "status_reason".to_owned(),
            Value::String(COPILOT_FEATURE_GATED_DISABLED_STATUS_REASON.to_owned()),
        );
        provider_obj.insert(
            "notes".to_owned(),
            Value::String(COPILOT_FEATURE_GATED_DISABLED_NOTES.to_owned()),
        );
        provider_obj.insert(
            "supports".to_owned(),
            json!({
                "review_start": false,
                "resume_reseed": false,
                "resume_reopen": false,
                "return": false,
                "status": true,
                "findings": true,
                "sessions": true,
                "doctor": true,
            }),
        );
    }

    capability
}

#[derive(Debug)]
struct CopilotLaunchError {
    state: LaunchAttemptState,
    reason_code: &'static str,
    detail: String,
    repair_actions: Vec<String>,
    extra_data: Value,
}

#[derive(Debug)]
struct CopilotReviewLaunchOutcome {
    locator: SessionLocator,
    session_path: String,
    continuity_quality: ContinuityQuality,
    artifact_refs: Vec<String>,
    hook_audit_event_count: usize,
    interactive: bool,
}

#[derive(Debug, Serialize)]
struct CopilotInvocationContext {
    mode: &'static str,
    /// "interactive" (terminal handoff) or "batch" (captured output).
    execution_mode: &'static str,
    interactive: bool,
    binary_path: String,
    launch_root: String,
    review_target: ReviewTarget,
    launch_profile_id: String,
    verification_source: &'static str,
    session_start_artifact_path: String,
    launch_capture_path: String,
    hook_audit_dir: String,
    hook_audit_event_count: usize,
    stdout_preview: Vec<String>,
    stderr_preview: Vec<String>,
    artifact_refs: Vec<String>,
    policy_profile_digest_sha256: String,
    hook_profile_digest_sha256: String,
    custom_instructions_digest_sha256: String,
}

#[derive(Clone, Debug, Serialize, serde::Deserialize)]
struct CopilotSessionStartPayload {
    provider: String,
    session_id: String,
    #[serde(default)]
    worktree_root: Option<String>,
    #[serde(default)]
    launch_profile_id: Option<String>,
    #[serde(default)]
    artifact_refs: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
enum CopilotSessionStartArtifact {
    Envelope {
        hook: String,
        payload: CopilotSessionStartPayload,
    },
    Payload(CopilotSessionStartPayload),
}

fn copilot_admission_gate_enabled() -> bool {
    session_copilot::copilot_admission_gate_enabled_from_value(
        std::env::var(session_copilot::ENV_COPILOT_ADMISSION_GATE)
            .ok()
            .as_deref(),
    )
}

fn copilot_feature_gated_launch_enabled(provider: &str) -> bool {
    provider == session_copilot::PROVIDER_ID && copilot_admission_gate_enabled()
}

fn copilot_session_start_artifact_path(store_root: &Path, attempt_id: &str) -> PathBuf {
    store_root
        .join("provider/copilot/launch-attempts")
        .join(attempt_id)
        .join("session-start.json")
}

fn copilot_launch_capture_path(store_root: &Path, attempt_id: &str) -> PathBuf {
    store_root
        .join("provider/copilot/launch-attempts")
        .join(attempt_id)
        .join("launch-capture.json")
}

/// Environment variable the Roger-owned Copilot hook scripts read to decide
/// where to append their audit jsonl. The CLI now always sets this for launches
/// so hook denials/transcript references are captured as Roger evidence.
const ENV_COPILOT_HOOK_AUDIT_DIR: &str = "RR_COPILOT_HOOK_AUDIT_DIR";
/// Exported to the provider child so the review_readonly pre-tool-use hook can
/// allow create/write ONLY under the Roger-owned worker inbox.
const ENV_WORKER_INBOX_DIR: &str = "RR_WORKER_INBOX_DIR";

/// Session/attempt-scoped directory the Roger-owned Copilot hooks write their
/// audit jsonl into (denials, transcript references, lifecycle events). Roger
/// exports this via `RR_COPILOT_HOOK_AUDIT_DIR` in the child environment.
fn copilot_hook_audit_dir(store_root: &Path, attempt_id: &str) -> PathBuf {
    store_root
        .join("provider/copilot/launch-attempts")
        .join(attempt_id)
        .join("hook-audit")
}

/// Result of scanning a Copilot hook audit directory after a session ends.
#[derive(Clone, Debug, Default)]
struct CopilotHookAuditScan {
    /// Absolute paths of the `*.jsonl` audit files that were written.
    artifact_refs: Vec<String>,
    /// Total number of recorded hook audit events (jsonl lines across files).
    event_count: usize,
}

/// Scan the per-attempt hook audit directory for jsonl artifacts the hooks
/// emitted and count the recorded events. Missing directory means no hooks
/// fired, which is a valid zero-result rather than an error.
fn scan_copilot_hook_audit_dir(audit_dir: &Path) -> CopilotHookAuditScan {
    let mut files: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = fs::read_dir(audit_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("jsonl") {
                files.push(path);
            }
        }
    }
    files.sort();

    let mut scan = CopilotHookAuditScan::default();
    for path in files {
        let event_count = fs::read_to_string(&path)
            .map(|text| text.lines().filter(|line| !line.trim().is_empty()).count())
            .unwrap_or(0);
        scan.event_count += event_count;
        scan.artifact_refs.push(path.to_string_lossy().into_owned());
    }
    scan
}

fn copilot_projected_provider_capability(runtime: &CliRuntime) -> Value {
    let mut capability =
        match resolved_routine_surface_baseline(runtime, session_copilot::PROVIDER_ID) {
            Ok(baseline) => provider_capability_projection(
                &baseline.provider,
                baseline.status_reason.as_deref(),
            ),
            Err(_) => provider_capability(session_copilot::PROVIDER_ID),
        };

    if let Some(provider_obj) = capability.as_object_mut() {
        provider_obj.insert(
            "status".to_owned(),
            Value::String(COPILOT_FEATURE_GATED_STATUS.to_owned()),
        );
        provider_obj.insert(
            "tier".to_owned(),
            Value::String(COPILOT_FEATURE_GATED_TIER.to_owned()),
        );
        provider_obj.insert(
            "support_tier".to_owned(),
            Value::String(COPILOT_FEATURE_GATED_TIER.to_owned()),
        );
        provider_obj.insert(
            "surface_class".to_owned(),
            Value::String(COPILOT_FEATURE_GATED_SURFACE_CLASS.to_owned()),
        );
        provider_obj.insert(
            "status_reason".to_owned(),
            Value::String(COPILOT_FEATURE_GATED_STATUS_REASON.to_owned()),
        );
        provider_obj.insert(
            "notes".to_owned(),
            Value::String(COPILOT_FEATURE_GATED_NOTES.to_owned()),
        );
        provider_obj.insert(
            "supports".to_owned(),
            json!({
                "review_start": true,
                "resume_reseed": true,
                "resume_reopen": true,
                "return": true,
                "status": true,
                "findings": true,
                "sessions": true,
                "doctor": true,
            }),
        );
    }

    capability
}

fn copilot_projected_routine_surface(
    runtime: &CliRuntime,
    worktree_root_override: Option<&str>,
) -> Option<Value> {
    resolved_routine_surface_baseline(runtime, session_copilot::PROVIDER_ID)
        .ok()
        .map(|baseline| {
            let mut projection = routine_surface_with_worktree_root(
                routine_surface_baseline_projection(&baseline),
                &runtime.cwd,
                worktree_root_override,
            );
            if let Some(surface_obj) = projection.as_object_mut() {
                surface_obj.insert(
                    "status_reason".to_owned(),
                    Value::String(COPILOT_FEATURE_GATED_STATUS_REASON.to_owned()),
                );
                surface_obj.insert(
                    "provider".to_owned(),
                    copilot_projected_provider_capability(runtime),
                );
            }
            projection
        })
}

fn copilot_launch_root(binding_context: &LaunchBindingContext, runtime: &CliRuntime) -> PathBuf {
    binding_context
        .worktree_root
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| runtime.cwd.clone())
}

fn read_copilot_session_start_payload(
    path: &Path,
) -> std::result::Result<CopilotSessionStartPayload, String> {
    let raw = fs::read(path).map_err(|err| {
        format!(
            "failed to read session-start artifact {}: {err}",
            path.display()
        )
    })?;
    let artifact: CopilotSessionStartArtifact = serde_json::from_slice(&raw).map_err(|err| {
        format!(
            "failed to parse session-start artifact {}: {err}",
            path.display()
        )
    })?;

    match artifact {
        CopilotSessionStartArtifact::Envelope { hook, payload } => {
            if hook != "session-start" {
                return Err(format!(
                    "unexpected Copilot hook artifact kind '{hook}' in {}",
                    path.display()
                ));
            }
            Ok(payload)
        }
        CopilotSessionStartArtifact::Payload(payload) => Ok(payload),
    }
}

fn preview_output_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(12)
        .map(ToOwned::to_owned)
        .collect()
}

#[derive(Debug, Serialize)]
struct CopilotLaunchCaptureRecord {
    schema_id: &'static str,
    mode: &'static str,
    execution_mode: &'static str,
    interactive: bool,
    binary_path: String,
    launch_root: String,
    exit_status: Option<i32>,
    stdout: String,
    stderr: String,
    session_start_artifact_path: String,
    hook_audit_dir: String,
    hook_audit_event_count: usize,
    artifact_refs: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_start_payload: Option<CopilotSessionStartPayload>,
}

fn persist_copilot_launch_capture(
    path: &Path,
    record: &CopilotLaunchCaptureRecord,
) -> std::result::Result<(), String> {
    let Some(parent) = path.parent() else {
        return Err(format!(
            "failed to resolve Copilot launch-capture directory for {}",
            path.display()
        ));
    };
    fs::create_dir_all(parent).map_err(|err| {
        format!(
            "failed to create Copilot launch-capture directory {}: {err}",
            parent.display()
        )
    })?;
    let payload = serde_json::to_vec_pretty(record)
        .map_err(|err| format!("failed to encode Copilot launch capture: {err}"))?;
    fs::write(path, payload).map_err(|err| {
        format!(
            "failed to write Copilot launch capture {}: {err}",
            path.display()
        )
    })
}

fn merge_copilot_artifact_refs(
    session_start_artifact: &Path,
    launch_capture_path: &Path,
    payload_artifact_refs: &[String],
) -> Vec<String> {
    let mut refs = vec![
        session_start_artifact.to_string_lossy().into_owned(),
        launch_capture_path.to_string_lossy().into_owned(),
    ];
    for artifact_ref in payload_artifact_refs {
        if !refs.iter().any(|existing| existing == artifact_ref) {
            refs.push(artifact_ref.clone());
        }
    }
    refs
}

fn normalized_path_string(path: &Path) -> String {
    fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

#[allow(clippy::too_many_arguments)]
fn launch_copilot_session(
    runtime: &CliRuntime,
    target: &ReviewTarget,
    attempt_id: &str,
    binding_context: &LaunchBindingContext,
    mode: &'static str,
    command: &[String],
    session_path: &str,
    continuity_quality: ContinuityQuality,
    expected_session_id: Option<&str>,
    interactive: bool,
    worker_inbox_dir: Option<&Path>,
) -> std::result::Result<CopilotReviewLaunchOutcome, CopilotLaunchError> {
    let execution_mode = if interactive { "interactive" } else { "batch" };
    let launch_root = copilot_launch_root(binding_context, runtime);
    let launch_root_string = normalized_path_string(&launch_root);
    let session_start_artifact =
        copilot_session_start_artifact_path(&runtime.store_root, attempt_id);
    let launch_capture_path = copilot_launch_capture_path(&runtime.store_root, attempt_id);
    let hook_audit_dir = copilot_hook_audit_dir(&runtime.store_root, attempt_id);
    let hook_audit_dir_string = hook_audit_dir.to_string_lossy().into_owned();
    let Some(artifact_parent) = session_start_artifact.parent() else {
        return Err(CopilotLaunchError {
            state: LaunchAttemptState::FailedSpawn,
            reason_code: "session_start_artifact_path_invalid",
            detail: format!(
                "failed to resolve session-start artifact directory for {}",
                session_start_artifact.display()
            ),
            repair_actions: vec![
                "re-run rr review after repairing the Roger store root".to_owned(),
            ],
            extra_data: json!({}),
        });
    };
    fs::create_dir_all(artifact_parent).map_err(|err| CopilotLaunchError {
        state: LaunchAttemptState::FailedSpawn,
        reason_code: "session_start_artifact_directory_unwritable",
        detail: format!(
            "failed to create session-start artifact directory {}: {err}",
            artifact_parent.display()
        ),
        repair_actions: vec![
            "re-run rr review after repairing filesystem permissions for the Roger store root"
                .to_owned(),
        ],
        extra_data: json!({
            "session_start_artifact_path": session_start_artifact.to_string_lossy(),
        }),
    })?;

    let provider_capability = copilot_projected_provider_capability(runtime);
    let policy_profile_digest_sha256 = provider_capability["policy_profile_digest_sha256"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let hook_profile_digest_sha256 = provider_capability["hook_profile_digest_sha256"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let custom_instructions_digest_sha256 =
        provider_capability["custom_instructions_digest_sha256"]
            .as_str()
            .unwrap_or_default()
            .to_owned();

    let copilot_binary_path =
        resolved_routine_surface_baseline(runtime, session_copilot::PROVIDER_ID)
            .map(|baseline| baseline.provider.binary_path.value)
            .unwrap_or_else(|_| {
                std::env::var(ENV_COPILOT_BIN)
                    .unwrap_or_else(|_| session_copilot::DEFAULT_COPILOT_BIN.to_owned())
            });

    // Copilot CLI only honors repo-level .github/hooks once they are merged
    // into the reviewed repo's default branch, which Roger cannot require of
    // arbitrary review targets. Verified start depends on the session-start
    // hook, so install/refresh the Roger-owned user-level hook assets before
    // every launch and fail closed if that is impossible.
    if let Some(copilot_home) = resolve_copilot_home() {
        session_copilot::install_user_level_hooks(&copilot_home).map_err(|err| {
            CopilotLaunchError {
                state: LaunchAttemptState::FailedSpawn,
                reason_code: "user_level_hook_install_failed",
                detail: format!(
                    "failed to install Roger user-level Copilot hooks under {}: {err}",
                    copilot_home.display()
                ),
                repair_actions: vec![format!(
                    "repair filesystem permissions for {} and re-run rr review",
                    copilot_home.display()
                )],
                extra_data: json!({
                    "copilot_home": copilot_home.to_string_lossy(),
                }),
            }
        })?;
    } else {
        return Err(CopilotLaunchError {
            state: LaunchAttemptState::FailedSpawn,
            reason_code: "copilot_home_unresolvable",
            detail: "cannot resolve the Copilot home directory for Roger hook installation"
                .to_owned(),
            repair_actions: vec![
                "set COPILOT_HOME or HOME so Roger can install its user-level Copilot hooks"
                    .to_owned(),
            ],
            extra_data: json!({}),
        });
    }

    // Ensure the session-scoped hook audit directory exists so both the child
    // hooks and Roger's post-exit scan share a stable path.
    if let Err(err) = fs::create_dir_all(&hook_audit_dir) {
        return Err(CopilotLaunchError {
            state: LaunchAttemptState::FailedSpawn,
            reason_code: "copilot_hook_audit_dir_unwritable",
            detail: format!(
                "failed to create Copilot hook audit directory {}: {err}",
                hook_audit_dir.display()
            ),
            repair_actions: vec![
                "repair Roger store write permissions and re-run the Copilot launch".to_owned(),
            ],
            extra_data: json!({ "hook_audit_dir": hook_audit_dir_string }),
        });
    }

    let mut process_command = ProcessCommand::new(&copilot_binary_path);
    process_command
        .args(command.iter().skip(1))
        .current_dir(&launch_root)
        .env(
            session_copilot::ENV_COPILOT_SESSION_START_ARTIFACT,
            &session_start_artifact,
        )
        .env(session_copilot::ENV_COPILOT_ATTEMPT_ID, attempt_id)
        .env(session_copilot::ENV_COPILOT_REPOSITORY, &target.repository)
        .env(
            session_copilot::ENV_COPILOT_PULL_REQUEST,
            target.pull_request_number.to_string(),
        )
        .env(
            session_copilot::ENV_COPILOT_WORKTREE_ROOT,
            &launch_root_string,
        )
        .env(
            session_copilot::ENV_COPILOT_POLICY_PROFILE_DIGEST,
            &policy_profile_digest_sha256,
        )
        .env(
            session_copilot::ENV_COPILOT_HOOK_PROFILE_DIGEST,
            &hook_profile_digest_sha256,
        )
        .env(
            session_copilot::ENV_COPILOT_CUSTOM_INSTRUCTIONS_DIGEST,
            &custom_instructions_digest_sha256,
        )
        // Audit wiring: the Roger-owned hooks append denial/transcript/lifecycle
        // jsonl here for both batch and interactive launches.
        .env(ENV_COPILOT_HOOK_AUDIT_DIR, &hook_audit_dir);
    if let Some(inbox) = worker_inbox_dir {
        // Scoped write exception: pre-tool-use allows create/write ONLY under
        // this Roger-owned inbox so the worker can stage its stage-result
        // request file without any repo-write capability.
        process_command.env(ENV_WORKER_INBOX_DIR, inbox);
    }

    let spawn_error = |err: std::io::Error| CopilotLaunchError {
        state: LaunchAttemptState::FailedSpawn,
        reason_code: "copilot_start_failed",
        detail: format!(
            "failed to invoke Copilot binary '{}': {err}",
            copilot_binary_path
        ),
        repair_actions: vec![format!(
            "install the GitHub Copilot CLI binary or set {}=/absolute/path/to/copilot",
            ENV_COPILOT_BIN
        )],
        extra_data: json!({
            "binary_path": copilot_binary_path,
            "launch_root": launch_root_string,
        }),
    };

    // Both modes share identical post-exit verification; only the process
    // execution differs. Interactive mode hands the real terminal to Copilot via
    // inherited stdio (like the OpenCode reopen path); batch mode captures
    // stdout/stderr for evidence.
    let (exit_status, stdout, stderr) = if interactive {
        let mut child = process_command
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(spawn_error)?;
        let status = child.wait().map_err(spawn_error)?;
        (status, String::new(), String::new())
    } else {
        let output = process_command.output().map_err(spawn_error)?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        (output.status, stdout, stderr)
    };

    // Session has ended: scan the hook audit directory for recorded evidence.
    let hook_audit_scan = scan_copilot_hook_audit_dir(&hook_audit_dir);

    let base_capture = CopilotLaunchCaptureRecord {
        schema_id: "rr.copilot.launch_capture.v1",
        mode,
        execution_mode,
        interactive,
        binary_path: copilot_binary_path.clone(),
        launch_root: launch_root_string.clone(),
        exit_status: exit_status.code(),
        stdout: stdout.clone(),
        stderr: stderr.clone(),
        session_start_artifact_path: session_start_artifact.to_string_lossy().into_owned(),
        hook_audit_dir: hook_audit_dir_string.clone(),
        hook_audit_event_count: hook_audit_scan.event_count,
        artifact_refs: Vec::new(),
        session_start_payload: None,
    };
    persist_copilot_launch_capture(&launch_capture_path, &base_capture).map_err(|detail| {
        CopilotLaunchError {
            state: LaunchAttemptState::FailedSessionBinding,
            reason_code: "copilot_launch_capture_unwritable",
            detail,
            repair_actions: vec![
                "re-run the Copilot launch after repairing Roger store write permissions"
                    .to_owned(),
            ],
            extra_data: json!({
                "launch_capture_path": launch_capture_path.to_string_lossy(),
                "session_start_artifact_path": session_start_artifact.to_string_lossy(),
            }),
        }
    })?;

    if !exit_status.success() {
        let detail = if stderr.trim().is_empty() {
            format!(
                "Copilot exited with status {} before Roger observed a verified session-start hook",
                exit_status
            )
        } else {
            format!(
                "Copilot exited with status {} before Roger observed a verified session-start hook: {}",
                exit_status,
                stderr.trim()
            )
        };
        return Err(CopilotLaunchError {
            state: LaunchAttemptState::FailedSpawn,
            reason_code: "provider_process_crash",
            detail,
            repair_actions: vec![
                "re-run rr review after verifying Copilot login and hook installation".to_owned(),
            ],
            extra_data: json!({
                "binary_path": copilot_binary_path,
                "launch_root": launch_root_string,
                "exit_status": exit_status.code(),
                "stdout_preview": preview_output_lines(&stdout),
                "stderr_preview": preview_output_lines(&stderr),
                "session_start_artifact_path": session_start_artifact.to_string_lossy(),
                "launch_capture_path": launch_capture_path.to_string_lossy(),
            }),
        });
    }

    if !session_start_artifact.is_file() {
        return Err(CopilotLaunchError {
            state: LaunchAttemptState::FailedProviderVerification,
            reason_code: "session_start_hook_missing",
            detail: format!(
                "Copilot launch completed without a Roger-readable session-start hook artifact at {}",
                session_start_artifact.display()
            ),
            repair_actions: vec![
                "install Roger Copilot hook assets and retry the launch".to_owned(),
            ],
            extra_data: json!({
                "binary_path": copilot_binary_path,
                "launch_root": launch_root_string,
                "stdout_preview": preview_output_lines(&stdout),
                "stderr_preview": preview_output_lines(&stderr),
                "session_start_artifact_path": session_start_artifact.to_string_lossy(),
                "launch_capture_path": launch_capture_path.to_string_lossy(),
            }),
        });
    }

    let payload =
        read_copilot_session_start_payload(&session_start_artifact).map_err(|detail| {
            CopilotLaunchError {
                state: LaunchAttemptState::FailedProviderVerification,
                reason_code: "hook_payload_schema_invalid",
                repair_actions: vec![
                    "repair the Roger Copilot session-start hook payload schema and retry"
                        .to_owned(),
                ],
                extra_data: json!({
                    "session_start_artifact_path": session_start_artifact.to_string_lossy(),
                    "launch_capture_path": launch_capture_path.to_string_lossy(),
                }),
                detail,
            }
        })?;

    if payload.provider != session_copilot::PROVIDER_ID {
        return Err(CopilotLaunchError {
            state: LaunchAttemptState::FailedProviderVerification,
            reason_code: "provider_verification_mismatch",
            detail: format!(
                "Copilot session-start hook reported provider '{}' instead of '{}'",
                payload.provider,
                session_copilot::PROVIDER_ID
            ),
            repair_actions: vec![
                "repair the Roger Copilot session-start hook provider metadata and retry"
                    .to_owned(),
            ],
            extra_data: json!({
                "session_start_artifact_path": session_start_artifact.to_string_lossy(),
            }),
        });
    }

    let verified_session_id = payload.session_id.trim();
    if verified_session_id.is_empty() {
        return Err(CopilotLaunchError {
            state: LaunchAttemptState::FailedProviderVerification,
            reason_code: "missing_verified_session_id",
            detail: format!(
                "Copilot session-start hook at {} omitted a non-empty session_id",
                session_start_artifact.display()
            ),
            repair_actions: vec![
                "repair the Roger Copilot session-start hook so it emits a real session_id"
                    .to_owned(),
            ],
            extra_data: json!({
                "session_start_artifact_path": session_start_artifact.to_string_lossy(),
                "stdout_preview": preview_output_lines(&stdout),
            }),
        });
    }

    if let Some(expected_session_id) = expected_session_id
        && verified_session_id != expected_session_id
    {
        return Err(CopilotLaunchError {
            state: LaunchAttemptState::FailedProviderVerification,
            reason_code: "reopened_session_id_mismatch",
            detail: format!(
                "Copilot reopen expected session_id '{}' but the Roger hook verified '{}'",
                expected_session_id, verified_session_id
            ),
            repair_actions: vec![
                "re-run rr resume to reseed from the stored ResumeBundle if the prior Copilot session was discarded"
                    .to_owned(),
            ],
            extra_data: json!({
                "expected_session_id": expected_session_id,
                "verified_session_id": verified_session_id,
                "session_start_artifact_path": session_start_artifact.to_string_lossy(),
                "launch_capture_path": launch_capture_path.to_string_lossy(),
            }),
        });
    }

    if let Some(payload_root) = payload.worktree_root.as_deref() {
        let normalized_payload_root = normalized_path_string(Path::new(payload_root));
        if normalized_payload_root != launch_root_string {
            return Err(CopilotLaunchError {
                state: LaunchAttemptState::FailedProviderVerification,
                reason_code: "worktree_root_mismatch",
                detail: format!(
                    "Copilot session-start hook reported worktree_root '{}' but Roger launched from '{}'",
                    normalized_payload_root, launch_root_string
                ),
                repair_actions: vec![
                    "re-run rr review from the intended repo/worktree root after repairing hook launch scope"
                        .to_owned(),
                ],
                extra_data: json!({
                    "launch_root": launch_root_string,
                    "reported_worktree_root": normalized_payload_root,
                    "session_start_artifact_path": session_start_artifact.to_string_lossy(),
                }),
            });
        }
    }

    if let Some(launch_profile_id) = payload.launch_profile_id.as_deref()
        && launch_profile_id != cli_config::PROFILE_ID
    {
        return Err(CopilotLaunchError {
            state: LaunchAttemptState::FailedProviderVerification,
            reason_code: "launch_profile_mismatch",
            detail: format!(
                "Copilot session-start hook reported launch_profile_id '{}' instead of '{}'",
                launch_profile_id,
                cli_config::PROFILE_ID
            ),
            repair_actions: vec![
                "repair the Roger Copilot hook profile metadata and retry the launch".to_owned(),
            ],
            extra_data: json!({
                "session_start_artifact_path": session_start_artifact.to_string_lossy(),
                "launch_capture_path": launch_capture_path.to_string_lossy(),
            }),
        });
    }

    let mut artifact_refs = merge_copilot_artifact_refs(
        &session_start_artifact,
        &launch_capture_path,
        &payload.artifact_refs,
    );
    // Fold the scanned hook audit jsonl files into the same evidence artifact
    // ref set the launch capture / resume bundle / invocation context all share.
    for audit_ref in &hook_audit_scan.artifact_refs {
        if !artifact_refs.iter().any(|existing| existing == audit_ref) {
            artifact_refs.push(audit_ref.clone());
        }
    }
    let updated_capture = CopilotLaunchCaptureRecord {
        artifact_refs: artifact_refs.clone(),
        session_start_payload: Some(payload.clone()),
        ..base_capture
    };
    persist_copilot_launch_capture(&launch_capture_path, &updated_capture).map_err(|detail| {
        CopilotLaunchError {
            state: LaunchAttemptState::FailedSessionBinding,
            reason_code: "copilot_launch_capture_unwritable",
            detail,
            repair_actions: vec![
                "re-run the Copilot launch after repairing Roger store write permissions"
                    .to_owned(),
            ],
            extra_data: json!({
                "launch_capture_path": launch_capture_path.to_string_lossy(),
                "session_start_artifact_path": session_start_artifact.to_string_lossy(),
            }),
        }
    })?;

    let invocation_context_json = serde_json::to_string(&CopilotInvocationContext {
        mode,
        execution_mode,
        interactive,
        binary_path: copilot_binary_path,
        launch_root: launch_root_string,
        review_target: target.clone(),
        launch_profile_id: cli_config::PROFILE_ID.to_owned(),
        verification_source: "session_start_hook_artifact",
        session_start_artifact_path: session_start_artifact.to_string_lossy().into_owned(),
        launch_capture_path: launch_capture_path.to_string_lossy().into_owned(),
        hook_audit_dir: hook_audit_dir_string,
        hook_audit_event_count: hook_audit_scan.event_count,
        stdout_preview: preview_output_lines(&stdout),
        stderr_preview: preview_output_lines(&stderr),
        artifact_refs: artifact_refs.clone(),
        policy_profile_digest_sha256,
        hook_profile_digest_sha256,
        custom_instructions_digest_sha256,
    })
    .map_err(|err| CopilotLaunchError {
        state: LaunchAttemptState::FailedSessionBinding,
        reason_code: "copilot_invocation_context_encode_failed",
        detail: format!("failed to encode Copilot invocation context: {err}"),
        repair_actions: vec![
            "re-run rr review after repairing the Copilot invocation context serializer".to_owned(),
        ],
        extra_data: json!({
            "session_start_artifact_path": session_start_artifact.to_string_lossy(),
            "launch_capture_path": launch_capture_path.to_string_lossy(),
        }),
    })?;

    Ok(CopilotReviewLaunchOutcome {
        locator: SessionLocator {
            provider: session_copilot::PROVIDER_ID.to_owned(),
            session_id: verified_session_id.to_owned(),
            invocation_context_json,
            captured_at: time::now_ts(),
            last_tested_at: Some(time::now_ts()),
        },
        session_path: session_path.to_owned(),
        continuity_quality,
        artifact_refs,
        hook_audit_event_count: hook_audit_scan.event_count,
        interactive,
    })
}

fn launch_copilot_review_session(
    runtime: &CliRuntime,
    target: &ReviewTarget,
    attempt_id: &str,
    binding_context: &LaunchBindingContext,
    worker_task_path: &str,
    interactive: bool,
) -> std::result::Result<CopilotReviewLaunchOutcome, CopilotLaunchError> {
    let seed_prompt = copilot_worker_seed_prompt(target, worker_task_path);
    let worker_inbox = Path::new(worker_task_path)
        .parent()
        .map(|p| p.join("inbox"));
    launch_copilot_session(
        runtime,
        target,
        attempt_id,
        binding_context,
        "start",
        &session_copilot::build_start_review_command(&seed_prompt),
        "hook_verified_start",
        ContinuityQuality::Degraded,
        None,
        interactive,
        worker_inbox.as_deref(),
    )
}

#[derive(Debug)]
struct CopilotContinuityLaunchOutcome {
    linkage: CopilotReviewLaunchOutcome,
    terminal_state: LaunchAttemptState,
    decision_reason: String,
}

fn copilot_resume_error_requires_reseed(err: &CopilotLaunchError) -> bool {
    if err.reason_code != "provider_process_crash" {
        return false;
    }

    let detail = err.detail.to_ascii_lowercase();
    [
        "not found",
        "no such session",
        "unknown session",
        "missing session",
        "missing state",
        "stale",
        "discarded",
        "expired",
    ]
    .iter()
    .any(|needle| detail.contains(needle))
}

fn launch_copilot_resume_or_return_session(
    runtime: &CliRuntime,
    target: &ReviewTarget,
    attempt_id: &str,
    binding_context: &LaunchBindingContext,
    session_locator: Option<&SessionLocator>,
    resume_bundle: Option<&ResumeBundle>,
    worker_task_path: &str,
    interactive: bool,
) -> std::result::Result<CopilotContinuityLaunchOutcome, CopilotLaunchError> {
    let worker_inbox = Path::new(worker_task_path)
        .parent()
        .map(|p| p.join("inbox"));
    if let Some(locator) = session_locator {
        match launch_copilot_session(
            runtime,
            target,
            attempt_id,
            binding_context,
            "resume",
            &session_copilot::build_resume_command(&locator.session_id),
            "reopened_by_locator",
            ContinuityQuality::Usable,
            Some(&locator.session_id),
            interactive,
            worker_inbox.as_deref(),
        ) {
            Ok(linkage) => {
                return Ok(CopilotContinuityLaunchOutcome {
                    linkage,
                    terminal_state: LaunchAttemptState::VerifiedReopened,
                    decision_reason: "copilot_reopened_by_locator".to_owned(),
                });
            }
            Err(err) if copilot_resume_error_requires_reseed(&err) => {}
            Err(err) => return Err(err),
        }
    }

    let Some(resume_bundle) = resume_bundle else {
        return Err(CopilotLaunchError {
            state: LaunchAttemptState::FailedSessionBinding,
            reason_code: "resume_bundle_missing_or_invalid",
            detail: "Copilot continuity fallback requires a stored ResumeBundle once locator reopen is unavailable".to_owned(),
            repair_actions: vec![
                "re-run rr review --provider copilot to regenerate a ResumeBundle".to_owned(),
            ],
            extra_data: json!({}),
        });
    };

    let reseed_prompt = format!(
        "Roger resume reseed for {} PR #{} in review-only posture. Use the stored ResumeBundle summary '{}' only as Roger continuity guidance. Keep all findings local to Roger; do not post to GitHub and do not modify repository files. {}",
        target.repository,
        target.pull_request_number,
        resume_bundle.stage_summary,
        worker_protocol_seed_summary(worker_task_path),
    );
    let linkage = launch_copilot_session(
        runtime,
        target,
        attempt_id,
        binding_context,
        "resume_reseed",
        &session_copilot::build_start_review_command(&reseed_prompt),
        "reseeded_from_bundle",
        ContinuityQuality::Degraded,
        None,
        interactive,
        worker_inbox.as_deref(),
    )?;

    Ok(CopilotContinuityLaunchOutcome {
        linkage,
        terminal_state: LaunchAttemptState::VerifiedReseeded,
        decision_reason: if session_locator.is_some() {
            "copilot_reseeded_after_stale_or_unusable_locator".to_owned()
        } else {
            "copilot_reseeded_from_bundle".to_owned()
        },
    })
}

/// The ten worker-transport operations a launch-bound `ReviewTask` grants. This
/// mirrors the `rr agent worker.*` surface documented in the review-worker
/// contract and the roger-worker-protocol skill.
const WORKER_TASK_ALLOWED_OPERATIONS: &[&str] = &[
    "worker.get_review_context",
    "worker.search_memory",
    "worker.list_findings",
    "worker.get_finding_detail",
    "worker.get_artifact_excerpt",
    "worker.get_status",
    "worker.submit_stage_result",
    "worker.request_clarification",
    "worker.request_memory_review",
    "worker.propose_follow_up",
];

/// Canonical on-disk location of the worker-task binding file for a session:
/// `<store_root>/sessions/<session-id>/worker-task.json`. This is the file the
/// in-session worker passes to every `rr agent worker.* --task-file <path>`
/// call; the seed prompt embeds this absolute path.
fn worker_task_file_path(store_root: &Path, session_id: &str) -> PathBuf {
    store_root
        .join("sessions")
        .join(session_id)
        .join("worker-task.json")
}

/// Build the canonical launch `ReviewTask` bound to a freshly minted (or reused)
/// session/run. The task carries a fresh nonce that must round-trip through
/// every worker result so Roger can reject stale or cross-session submissions.
fn build_launch_review_task(session_id: &str, run_id: &str, stage: &str) -> ReviewTask {
    ReviewTask {
        id: next_id("task"),
        review_session_id: session_id.to_owned(),
        review_run_id: run_id.to_owned(),
        stage: stage.to_owned(),
        task_kind: ReviewTaskKind::DeepReviewPass,
        task_nonce: next_id("nonce"),
        objective:
            "Perform a bounded review pass and return findings through the Roger worker transport."
                .to_owned(),
        turn_strategy: WorkerTurnStrategy::SingleTurnReport,
        allowed_scopes: vec!["repo".to_owned()],
        allowed_operations: WORKER_TASK_ALLOWED_OPERATIONS
            .iter()
            .map(|op| (*op).to_owned())
            .collect(),
        expected_result_schema: WORKER_STAGE_RESULT_SCHEMA_V1.to_owned(),
        prompt_preset_id: None,
        created_at: time::now_ts(),
    }
}

/// Atomically persist a worker-task binding file at its canonical path. Creates
/// the session artifact directory and overwrites any prior binding (a re-launch
/// rebinds the CURRENT run/task ids) via a temp-write-then-rename.
fn write_worker_task_file(store_root: &Path, task: &ReviewTask) -> Result<PathBuf, String> {
    let path = worker_task_file_path(store_root, &task.review_session_id);
    let parent = path
        .parent()
        .ok_or_else(|| format!("invalid worker-task path {}", path.display()))?;
    fs::create_dir_all(parent).map_err(|err| {
        format!(
            "failed to create worker-task directory {}: {err}",
            parent.display()
        )
    })?;
    let payload = serde_json::to_vec_pretty(task)
        .map_err(|err| format!("failed to serialize worker-task binding: {err}"))?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, &payload).map_err(|err| {
        format!(
            "failed to write worker-task binding {}: {err}",
            tmp.display()
        )
    })?;
    fs::rename(&tmp, &path).map_err(|err| {
        format!(
            "failed to finalize worker-task binding {}: {err}",
            path.display()
        )
    })?;
    // The worker inbox: the one directory the review_readonly policy lets the
    // in-session worker write into (staging its stage-result request JSON).
    let inbox = parent.join("inbox");
    fs::create_dir_all(&inbox)
        .map_err(|err| format!("failed to create worker inbox {}: {err}", inbox.display()))?;
    Ok(path)
}

/// The compact worker-protocol summary embedded in provider seed prompts. The
/// roger-worker-protocol skill carries the depth; this keeps the seed short but
/// unambiguous about the bind -> read -> submit loop and the boundary.
fn worker_protocol_seed_summary(worker_task_path: &str) -> String {
    format!(
        "Your Roger worker-task file is {worker_task_path}. Drive the review through the Roger worker transport: \
first `rr agent worker.get_review_context --task-file {worker_task_path}` to read bounded context, \
then do the review, then submit findings: write your stage-result request JSON with the create tool to a file under $RR_WORKER_INBOX_DIR (the one directory the policy lets you write) and run `rr agent worker.submit_stage_result --task-file {worker_task_path} --request-file <that file>`; if file writes are unavailable, base64-encode the request JSON yourself and pass it inline via `rr agent worker.submit_stage_result --task-file {worker_task_path} --request-b64 <base64>`. \
Use `rr agent worker.search_memory` before writing new analysis and `rr agent worker.request_clarification` when you need operator input; the task_nonce in the file must round-trip through every submission. \
The review_readonly policy allows only `rr agent <op>` and read-only `rr status|findings|sessions|search --robot`; every other shell command is denied, so issue one clean allowlisted command at a time."
    )
}

/// Build the Copilot start-review seed prompt with the worker-task path and the
/// bounded worker protocol embedded.
fn copilot_worker_seed_prompt(target: &ReviewTarget, worker_task_path: &str) -> String {
    format!(
        "Roger review start for {} PR #{} in review-only posture. Keep all findings local to Roger; do not post to GitHub and do not modify repository files. {}",
        target.repository,
        target.pull_request_number,
        worker_protocol_seed_summary(worker_task_path),
    )
}

/// Build the human next-steps block for a launched/reused review session: the
/// concrete follow-on commands an operator runs next.
fn review_next_commands(
    session_id: &str,
    provider: &str,
    repository: &str,
    pr: u64,
    interactive: bool,
) -> Vec<String> {
    let mut commands = vec![
        format!("rr open --session {session_id}"),
        format!("rr findings --session {session_id}"),
    ];
    if provider == session_copilot::PROVIDER_ID && !interactive {
        commands.push(format!(
            "rr review --pr {pr} --repo {repository} --provider copilot --interactive  # hand the terminal to Copilot"
        ));
    }
    commands
}

/// Attention states that mean a review session is done and must NOT be silently
/// reused by a later `rr review`. Everything else in the canonical vocabulary
/// (review_launched, review_resumed, awaiting_*, findings_ready,
/// outbound_approval_required, refresh_recommended, returned_to_roger) is a live
/// session an operator would want to re-enter rather than duplicate.
const TERMINAL_REVIEW_ATTENTION_STATES: &[&str] = &["review_failed"];

fn attention_state_is_reusable(state: &str) -> bool {
    !TERMINAL_REVIEW_ATTENTION_STATES.contains(&state)
}

/// Pick the newest non-terminal session to reuse from repo/PR candidates.
fn select_reusable_session(candidates: &[SessionFinderEntry]) -> Option<&SessionFinderEntry> {
    candidates
        .iter()
        .filter(|entry| attention_state_is_reusable(&entry.attention_state))
        .max_by_key(|entry| entry.updated_at)
}

/// Compact relative-age label ("2h ago") for human candidate lists and reuse
/// messages. `now`/`then` are unix-second timestamps.
fn format_relative_age(now: i64, then: i64) -> String {
    let secs = (now - then).max(0);
    if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3_600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86_400 {
        format!("{}h ago", secs / 3_600)
    } else {
        format!("{}d ago", secs / 86_400)
    }
}

/// Defensively pull a session-row projection out of either a picker `candidate`
/// object or a `rr sessions` `items` object (their key shapes differ slightly).
/// Returns (session_id, repository, pull_request, provider, attention, updated_at).
fn extract_session_row(entry: &Value) -> (String, String, u64, String, String, i64) {
    let session_id = entry["session_id"].as_str().unwrap_or("?").to_owned();
    let repository = entry
        .get("repository")
        .and_then(Value::as_str)
        .or_else(|| entry.get("repo").and_then(Value::as_str))
        .or_else(|| {
            entry
                .get("target")
                .and_then(|target| target.get("repository"))
                .and_then(Value::as_str)
        })
        .unwrap_or("?")
        .to_owned();
    let pull_request = entry
        .get("pull_request")
        .and_then(Value::as_u64)
        .or_else(|| {
            entry
                .get("target")
                .and_then(|target| target.get("pull_request"))
                .and_then(Value::as_u64)
        })
        .unwrap_or(0);
    let provider = entry["provider"].as_str().unwrap_or("?").to_owned();
    let attention = entry["attention_state"].as_str().unwrap_or("?").to_owned();
    let updated_at = entry.get("updated_at").and_then(Value::as_i64).unwrap_or(0);
    (
        session_id,
        repository,
        pull_request,
        provider,
        attention,
        updated_at,
    )
}

/// Render a human session list grouped by repo/PR: at most five most-recent per
/// group with relative ages, then an "and N older sessions" line pointing at
/// `rr sessions --all`. With `show_all`, every session in each group is listed.
fn render_grouped_session_lines(entries: &[Value], show_all: bool, now: i64) -> String {
    let mut groups: std::collections::BTreeMap<
        (String, u64),
        Vec<(String, String, u64, String, String, i64)>,
    > = std::collections::BTreeMap::new();
    for entry in entries {
        let row = extract_session_row(entry);
        groups.entry((row.1.clone(), row.2)).or_default().push(row);
    }
    let mut out = String::new();
    for ((repository, pull_request), mut group) in groups {
        group.sort_by(|a, b| b.5.cmp(&a.5));
        out.push_str(&format!("{repository}#{pull_request}:\n"));
        let shown = if show_all {
            group.len()
        } else {
            group.len().min(5)
        };
        for row in &group[..shown] {
            out.push_str(&format!(
                "  {}  {}  {}  {}\n",
                row.0,
                row.3,
                row.4,
                format_relative_age(now, row.5)
            ));
        }
        if !show_all && group.len() > shown {
            out.push_str(&format!(
                "  ... and {} older sessions (rr sessions --all to list)\n",
                group.len() - shown
            ));
        }
    }
    out
}

fn handle_review(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    let launch_surface = resolved_launch_surface(parsed);
    let supported_providers = runtime_supported_review_providers(runtime);
    let planned_not_live_providers = runtime_planned_not_live_review_providers(runtime);
    if !supported_providers.contains(&parsed.provider.as_str()) {
        let mut repair_actions = vec![
            "use --provider opencode for tier-b CLI continuity on the current live CLI surface"
                .to_owned(),
            "use --provider codex for bounded tier-a start/reseed support".to_owned(),
            "use --provider gemini for bounded tier-a start/reseed support".to_owned(),
            "use --provider claude for bounded tier-a start/reseed support".to_owned(),
        ];
        if parsed.provider == session_copilot::PROVIDER_ID
            && !copilot_feature_gated_launch_enabled(session_copilot::PROVIDER_ID)
        {
            // Copilot is feature-gated, not unsupported: tell the operator how to
            // turn it on rather than steering them to a different provider.
            repair_actions.push(
                "enable RR_ENABLE_COPILOT_PROVIDER=1 to use Copilot's feature-gated bounded tier-b review path"
                    .to_owned(),
            );
        } else if copilot_feature_gated_launch_enabled(session_copilot::PROVIDER_ID) {
            repair_actions.push(
                "use --provider copilot for feature-gated bounded tier-b continuity support"
                    .to_owned(),
            );
        }
        return blocked_response(
            format!(
                "provider '{}' is not supported for rr review in this slice",
                parsed.provider
            ),
            repair_actions,
            json!({
                "provider": parsed.provider,
                "supported_providers": supported_providers,
                "planned_not_live_providers": planned_not_live_providers,
                "feature_gated_disabled_providers":
                    runtime_feature_gated_disabled_review_providers(runtime),
                "not_supported_providers": NOT_LIVE_REVIEW_PROVIDERS,
                "live_review_provider_support": runtime_review_provider_support_matrix(runtime),
            }),
        );
    }

    let Some(repository) = resolve_repository(parsed.repo.clone(), &runtime.cwd) else {
        return blocked_response(
            "repo context inference failed; review target is ambiguous".to_owned(),
            vec!["pass --repo owner/repo or configure git remote.origin.url".to_owned()],
            json!({"reason_code": "repo_context_missing"}),
        );
    };

    let Some(pr) = parsed.pr else {
        return blocked_response(
            "rr review requires --pr because no safe single PR inference is available".to_owned(),
            vec!["pass --pr <number>".to_owned()],
            json!({"reason_code": "pr_required"}),
        );
    };

    let target = build_review_target(&repository, pr);

    if parsed.dry_run {
        return CommandResponse {
            outcome: OutcomeKind::Complete,
            data: json!({
                "mode": "dry_run",
                "provider": parsed.provider,
                "repository": repository,
                "pull_request": pr,
                "launch_profile_id": cli_config::PROFILE_ID,
                "provider_capability": runtime_provider_capability(runtime, &parsed.provider),
                "routine_surface": runtime_routine_surface_projection(
                    runtime,
                    &parsed.provider,
                    None,
                ),
            }),
            warnings: provider_support_warning(&parsed.provider, "rr review")
                .into_iter()
                .collect(),
            repair_actions: Vec::new(),
            message: "review launch plan generated (dry-run)".to_owned(),
        };
    }

    // Launch preflight: a review launch must never claim success when the
    // provider binary cannot run, and a definitively missing PR must block
    // loudly instead of registering a session that can never be reviewed.
    if let Some(binary) = provider_launch_binary_for(runtime, &parsed.provider)
        && !binary_resolves_locally(&binary)
    {
        return provider_binary_missing_response("rr review", &parsed.provider, &binary);
    }

    let target_verification_warning = match github_review_target_preflight(&repository, pr) {
        ReviewTargetPreflight::Verified => None,
        ReviewTargetPreflight::Unverified { warning } => Some(warning),
        ReviewTargetPreflight::Blocked(response) => return *response,
    };

    let store = match open_store_or_response(runtime, "rr review") {
        Ok(store) => store,
        Err(response) => return response,
    };

    // Session reuse-or-new. Unless --fresh is passed, a non-terminal session
    // already covering this repo/PR is reused with resume semantics instead of
    // minting yet another zombie session. This is where the 25-zombies-per-2-PRs
    // pollution is stopped.
    if !parsed.fresh {
        match store.session_finder(SessionFinderQuery {
            repository: Some(target.repository.clone()),
            pull_request_number: Some(pr),
            attention_states: Vec::new(),
            limit: 250,
        }) {
            Ok(candidates) => {
                if let Some(reused) = select_reusable_session(&candidates) {
                    let reused_id = reused.session_id.clone();
                    let age = format_relative_age(time::now_ts(), reused.updated_at);
                    let mut reuse_parsed = parsed.clone();
                    reuse_parsed.session_id = Some(reused_id.clone());
                    let mut response = handle_resume(&reuse_parsed, runtime);
                    let failed = matches!(
                        response.outcome,
                        OutcomeKind::Blocked | OutcomeKind::Error | OutcomeKind::RepairNeeded
                    );
                    if let Value::Object(map) = &mut response.data {
                        map.insert("reused".to_owned(), Value::Bool(true));
                        map.insert(
                            "reused_session_id".to_owned(),
                            Value::String(reused_id.clone()),
                        );
                        map.insert("reused_session_age".to_owned(), Value::String(age.clone()));
                    }
                    if !failed {
                        response.message = format!(
                            "reusing existing session {reused_id} (started {age}); pass --fresh for a new session"
                        );
                    }
                    return response;
                }
            }
            Err(err) => {
                return error_response(format!("failed to check for reusable sessions: {err}"));
            }
        }
    }

    let attempt_id = next_id("attempt");
    if let Err(err) = store.create_launch_attempt(CreateLaunchAttempt {
        id: &attempt_id,
        action: LaunchAttemptAction::StartReview,
        provider: &parsed.provider,
        source_surface: launch_surface,
        review_target: &target,
        requested_session_id: None,
        state: LaunchAttemptState::Pending,
    }) {
        return error_response(format!("failed to create launch attempt: {err}"));
    }

    if let Err(err) = persist_launch_attempt_state(
        &store,
        &attempt_id,
        LaunchAttemptState::Dispatching,
        None,
        None,
        None,
        None,
        None,
    ) {
        return error_response(err);
    }

    let binding_context = LaunchBindingContext::for_cwd(&runtime.cwd);
    let intent = launch_intent(LaunchAction::StartReview, runtime);

    // Pre-mint the session and run ids so the worker-task binding path is known
    // BEFORE the provider launches: the Copilot seed prompt embeds the absolute
    // worker-task path, and the same ids are finalized into the store below.
    let session_id = next_id("session");
    let run_id = next_id("run");
    let worker_task = build_launch_review_task(&session_id, &run_id, "deep_review");
    let worker_task_path = worker_task_file_path(&runtime.store_root, &session_id);
    let worker_task_path_string = normalized_path_string(&worker_task_path);

    // Pre-stage the worker-task binding file NOW, before the provider process
    // runs, so an in-session agent that calls `rr agent worker.* --task-file
    // <path>` during the launch finds a valid ReviewTask at the seeded path
    // (the store session/run are committed by finalize below). If the launch
    // later fails, the file is an orphan under an uncommitted session id.
    let mut worker_task_binding_warning: Option<String> = None;
    if let Err(err) = write_worker_task_file(&runtime.store_root, &worker_task) {
        worker_task_binding_warning = Some(format!("failed to persist worker-task binding: {err}"));
    }

    let record_failure = |state: LaunchAttemptState, reason: &str| {
        persist_launch_attempt_state(
            &store,
            &attempt_id,
            state,
            None,
            None,
            None,
            None,
            Some(reason),
        )
    };
    let mut copilot_interactive = false;
    let mut copilot_hook_audit_event_count = 0usize;
    let mut provisional_session_committed = false;
    let (session_locator, session_path, continuity_quality, mut warnings, bundle_artifact_refs) =
        match parsed.provider.as_str() {
            "opencode" => {
                let adapter = OpenCodeAdapter::with_binary(runtime.opencode_bin.clone());
                let linkage = match adapter.link_session(&target, &intent, None, None) {
                    Ok(linkage) => linkage,
                    Err(err) => {
                        let detail = err.to_string();
                        if let Err(update_err) =
                            record_failure(LaunchAttemptState::FailedSpawn, &detail)
                        {
                            return error_response(update_err);
                        }
                        return blocked_response(
                            format!("failed to start OpenCode session: {detail}"),
                            vec!["verify OpenCode is installed and reachable".to_owned()],
                            json!({
                                "reason_code": "opencode_start_failed",
                                "launch_attempt_id": attempt_id,
                            }),
                        );
                    }
                };
                (
                    linkage.locator,
                    session_path_label(&linkage.path).to_owned(),
                    linkage.continuity_quality,
                    Vec::new(),
                    Vec::new(),
                )
            }
            "codex" => {
                let adapter = CodexAdapter::new();
                let linkage = match adapter.link_session(&target, &intent, None, None) {
                    Ok(linkage) => linkage,
                    Err(err) => {
                        let detail = err.to_string();
                        if let Err(update_err) =
                            record_failure(LaunchAttemptState::FailedSpawn, &detail)
                        {
                            return error_response(update_err);
                        }
                        return blocked_response(
                            format!("failed to start Codex session: {detail}"),
                            vec!["verify Codex CLI is installed and reachable".to_owned()],
                            json!({
                                "reason_code": "codex_start_failed",
                                "launch_attempt_id": attempt_id,
                            }),
                        );
                    }
                };
                (
                    linkage.locator,
                    codex_session_path_label(&linkage.path).to_owned(),
                    linkage.continuity_quality,
                    provider_support_warning("codex", "rr review")
                        .into_iter()
                        .collect(),
                    Vec::new(),
                )
            }
            "claude" => {
                let adapter = ClaudeAdapter::new();
                let linkage = match adapter.link_session(&target, &intent, None, None) {
                    Ok(linkage) => linkage,
                    Err(err) => {
                        let detail = err.to_string();
                        if let Err(update_err) =
                            record_failure(LaunchAttemptState::FailedSpawn, &detail)
                        {
                            return error_response(update_err);
                        }
                        return blocked_response(
                            format!("failed to start Claude session: {detail}"),
                            vec!["verify Claude CLI is installed and reachable".to_owned()],
                            json!({
                                "reason_code": "claude_start_failed",
                                "launch_attempt_id": attempt_id,
                            }),
                        );
                    }
                };
                (
                    linkage.locator,
                    claude_session_path_label(&linkage.path).to_owned(),
                    linkage.continuity_quality,
                    provider_support_warning("claude", "rr review")
                        .into_iter()
                        .collect(),
                    Vec::new(),
                )
            }
            "gemini" => {
                let adapter = GeminiAdapter::new();
                let linkage = match adapter.link_session(&target, &intent, None, None) {
                    Ok(linkage) => linkage,
                    Err(err) => {
                        let detail = err.to_string();
                        if let Err(update_err) =
                            record_failure(LaunchAttemptState::FailedSpawn, &detail)
                        {
                            return error_response(update_err);
                        }
                        return blocked_response(
                            format!("failed to start Gemini session: {detail}"),
                            vec!["verify Gemini CLI is installed and reachable".to_owned()],
                            json!({
                                "reason_code": "gemini_start_failed",
                                "launch_attempt_id": attempt_id,
                            }),
                        );
                    }
                };
                (
                    linkage.locator,
                    gemini_session_path_label(&linkage.path).to_owned(),
                    linkage.continuity_quality,
                    provider_support_warning("gemini", "rr review")
                        .into_iter()
                        .collect(),
                    Vec::new(),
                )
            }
            "copilot" => {
                // Copilot runs its in-session worker DURING the launch
                // subprocess, so the session/run rows must already be visible
                // for the worker's `rr agent` calls (get_review_context etc.).
                // Provision them now; finalize upgrades the same rows (upsert)
                // with the verified binding, and the failure arms below demote
                // the provisional session to review_failed.
                if let Err(err) = store.create_review_session(CreateReviewSession {
                    id: &session_id,
                    review_target: &target,
                    provider: &parsed.provider,
                    session_locator: None,
                    resume_bundle_artifact_id: None,
                    continuity_state: continuity_state_label(&ContinuityQuality::Degraded),
                    attention_state: "review_launched",
                    launch_profile_id: Some(cli_config::PROFILE_ID),
                }) {
                    return error_response(format!(
                        "failed to provision review session before Copilot launch: {err}"
                    ));
                }
                if let Err(err) = store.create_review_run(CreateReviewRun {
                    id: &run_id,
                    session_id: &session_id,
                    run_kind: "review",
                    repo_snapshot: &format!("{}#{}", target.repository, target.pull_request_number),
                    continuity_quality: continuity_state_label(&ContinuityQuality::Degraded),
                    session_locator_artifact_id: None,
                }) {
                    return error_response(format!(
                        "failed to provision review run before Copilot launch: {err}"
                    ));
                }
                provisional_session_committed = true;
                let linkage = match launch_copilot_review_session(
                    runtime,
                    &target,
                    &attempt_id,
                    &binding_context,
                    &worker_task_path_string,
                    parsed.interactive,
                ) {
                    Ok(linkage) => linkage,
                    Err(err) => {
                        demote_provisional_session(&store, &session_id);
                        if let Err(update_err) = persist_launch_attempt_state(
                            &store,
                            &attempt_id,
                            err.state,
                            None,
                            None,
                            None,
                            None,
                            Some(&err.detail),
                        ) {
                            return error_response(update_err);
                        }
                        return blocked_response(
                            format!("failed to start Copilot session: {}", err.detail),
                            err.repair_actions,
                            {
                                let mut data = serde_json::Map::new();
                                data.insert(
                                    "reason_code".to_owned(),
                                    Value::String(err.reason_code.to_owned()),
                                );
                                data.insert(
                                    "launch_attempt_id".to_owned(),
                                    Value::String(attempt_id.clone()),
                                );
                                data.insert(
                                    "provider".to_owned(),
                                    Value::String(session_copilot::PROVIDER_ID.to_owned()),
                                );
                                if let Some(extra) = err.extra_data.as_object() {
                                    for (key, value) in extra {
                                        data.insert(key.clone(), value.clone());
                                    }
                                }
                                Value::Object(data)
                            },
                        );
                    }
                };
                copilot_interactive = linkage.interactive;
                copilot_hook_audit_event_count = linkage.hook_audit_event_count;
                let mut copilot_warnings: Vec<String> =
                    provider_support_warning("copilot", "rr review")
                        .into_iter()
                        .collect();
                if linkage.hook_audit_event_count > 0 {
                    copilot_warnings.push(format!(
                        "captured {} hook audit events",
                        linkage.hook_audit_event_count
                    ));
                }
                (
                    linkage.locator,
                    linkage.session_path,
                    linkage.continuity_quality,
                    copilot_warnings,
                    linkage.artifact_refs,
                )
            }
            _ => unreachable!("provider validated above"),
        };

    if let Err(err) = persist_launch_attempt_state(
        &store,
        &attempt_id,
        LaunchAttemptState::AwaitingProviderVerification,
        None,
        None,
        None,
        Some(&session_locator),
        None,
    ) {
        return error_response(err);
    }

    let provider_session_id = match verified_provider_session_id(&parsed.provider, &session_locator)
    {
        Ok(session_id) => session_id.to_owned(),
        Err(detail) => {
            if provisional_session_committed {
                demote_provisional_session(&store, &session_id);
            }
            if let Err(update_err) = persist_launch_attempt_state(
                &store,
                &attempt_id,
                LaunchAttemptState::FailedProviderVerification,
                None,
                None,
                None,
                Some(&session_locator),
                Some(&detail),
            ) {
                return error_response(update_err);
            }
            return blocked_response(
                format!("failed to verify provider session: {detail}"),
                vec!["re-run rr review after verifying provider launch output".to_owned()],
                json!({
                    "reason_code": "provider_session_unverified",
                    "launch_attempt_id": attempt_id,
                    "provider": parsed.provider,
                }),
            );
        }
    };

    let bundle_id = next_id("bundle");
    let binding_id = next_id("binding");

    let bundle = build_resume_bundle(
        ResumeBundleProfile::ReseedResume,
        target.clone(),
        intent,
        parsed.provider.clone(),
        continuity_quality.clone(),
        "review launched via rr review",
        bundle_artifact_refs,
    );

    let bundle_payload = match serde_json::to_vec(&bundle) {
        Ok(payload) => payload,
        Err(err) => {
            let detail = format!("failed to serialize ResumeBundle: {err}");
            if provisional_session_committed {
                demote_provisional_session(&store, &session_id);
            }
            if let Err(update_err) = persist_launch_attempt_state(
                &store,
                &attempt_id,
                LaunchAttemptState::FailedSessionBinding,
                None,
                None,
                Some(&provider_session_id),
                Some(&session_locator),
                Some(&detail),
            ) {
                return error_response(update_err);
            }
            return error_response(detail);
        }
    };
    let bundle_digest = sha256_hex(&bundle_payload);
    let bundle_artifact_id = match store.artifact_id_by_digest(&bundle_digest) {
        Ok(Some(existing_id)) => existing_id,
        Ok(None) => match store.store_resume_bundle(&bundle_id, &bundle) {
            Ok(stored) => stored.id,
            Err(err)
                if err
                    .to_string()
                    .contains("UNIQUE constraint failed: artifacts.digest") =>
            {
                match store.artifact_id_by_digest(&bundle_digest) {
                    Ok(Some(existing_id)) => existing_id,
                    Ok(None) => {
                        let detail =
                            "failed to persist ResumeBundle: duplicate digest detected but no stored artifact could be resolved".to_owned();
                        if provisional_session_committed {
                            demote_provisional_session(&store, &session_id);
                        }
                        if let Err(update_err) = persist_launch_attempt_state(
                            &store,
                            &attempt_id,
                            LaunchAttemptState::FailedSessionBinding,
                            None,
                            None,
                            Some(&provider_session_id),
                            Some(&session_locator),
                            Some(&detail),
                        ) {
                            return error_response(update_err);
                        }
                        return error_response(detail);
                    }
                    Err(lookup_err) => {
                        let detail = format!(
                            "failed to persist ResumeBundle: duplicate digest lookup failed: {lookup_err}"
                        );
                        if provisional_session_committed {
                            demote_provisional_session(&store, &session_id);
                        }
                        if let Err(update_err) = persist_launch_attempt_state(
                            &store,
                            &attempt_id,
                            LaunchAttemptState::FailedSessionBinding,
                            None,
                            None,
                            Some(&provider_session_id),
                            Some(&session_locator),
                            Some(&detail),
                        ) {
                            return error_response(update_err);
                        }
                        return error_response(detail);
                    }
                }
            }
            Err(err) => {
                let detail = format!("failed to persist ResumeBundle: {err}");
                if provisional_session_committed {
                    demote_provisional_session(&store, &session_id);
                }
                if let Err(update_err) = persist_launch_attempt_state(
                    &store,
                    &attempt_id,
                    LaunchAttemptState::FailedSessionBinding,
                    None,
                    None,
                    Some(&provider_session_id),
                    Some(&session_locator),
                    Some(&detail),
                ) {
                    return error_response(update_err);
                }
                return error_response(detail);
            }
        },
        Err(err) => {
            let detail =
                format!("failed to resolve existing ResumeBundle artifact by digest: {err}");
            if provisional_session_committed {
                demote_provisional_session(&store, &session_id);
            }
            if let Err(update_err) = persist_launch_attempt_state(
                &store,
                &attempt_id,
                LaunchAttemptState::FailedSessionBinding,
                None,
                None,
                Some(&provider_session_id),
                Some(&session_locator),
                Some(&detail),
            ) {
                return error_response(update_err);
            }
            return error_response(detail);
        }
    };

    if let Err(err) = store.finalize_review_launch_attempt(FinalizeReviewLaunchAttempt {
        attempt_id: &attempt_id,
        terminal_state: LaunchAttemptState::VerifiedStarted,
        provider_session_id: &provider_session_id,
        verified_locator: &session_locator,
        review_session: CreateReviewSession {
            id: &session_id,
            review_target: &target,
            provider: &parsed.provider,
            session_locator: Some(&session_locator),
            resume_bundle_artifact_id: Some(&bundle_artifact_id),
            continuity_state: continuity_state_label(&continuity_quality),
            attention_state: "review_launched",
            launch_profile_id: Some(cli_config::PROFILE_ID),
        },
        review_run: CreateReviewRun {
            id: &run_id,
            session_id: &session_id,
            run_kind: "review",
            repo_snapshot: &format!("{}#{}", target.repository, target.pull_request_number),
            continuity_quality: continuity_state_label(&continuity_quality),
            session_locator_artifact_id: None,
        },
        launch_binding: CreateSessionLaunchBinding {
            id: &binding_id,
            session_id: &session_id,
            repo_locator: &target.repository,
            review_target: Some(&target),
            surface: launch_surface,
            launch_profile_id: Some(cli_config::PROFILE_ID),
            ui_target: Some(cli_config::UI_TARGET),
            instance_preference: Some(cli_config::INSTANCE_PREFERENCE),
            cwd: binding_local_root_for_surface(launch_surface, &binding_context).0,
            worktree_root: binding_local_root_for_surface(launch_surface, &binding_context).1,
        },
    }) {
        let detail = format!("failed to finalize review launch: {err}");
        if provisional_session_committed {
            demote_provisional_session(&store, &session_id);
        }
        let failure_state = match err {
            ReviewLaunchFinalizationError::SessionBinding(_) => {
                LaunchAttemptState::FailedSessionBinding
            }
            ReviewLaunchFinalizationError::Commit(_) => LaunchAttemptState::FailedCommit,
        };
        if let Err(update_err) = persist_launch_attempt_state(
            &store,
            &attempt_id,
            failure_state,
            None,
            None,
            Some(&provider_session_id),
            Some(&session_locator),
            Some(&detail),
        ) {
            return error_response(update_err);
        }
        return error_response(detail);
    }

    let outcome = if matches!(continuity_quality, ContinuityQuality::Usable) {
        OutcomeKind::Complete
    } else {
        OutcomeKind::Degraded
    };

    let target_verification = if target_verification_warning.is_some() {
        "unverified"
    } else {
        "verified"
    };
    if let Some(warning) = target_verification_warning {
        warnings.push(warning);
    }
    // The worker-task binding was pre-staged before launch; surface any write
    // failure now that we are assembling the response.
    if let Some(warning) = worker_task_binding_warning {
        warnings.push(warning);
    }

    let next_commands = review_next_commands(
        &session_id,
        &parsed.provider,
        &target.repository,
        target.pull_request_number,
        copilot_interactive,
    );
    let human_details = format!(
        "\n  session: {session_id}\n  provider: {}\n  worker-task: {worker_task_path_string}\nNext:\n{}",
        parsed.provider,
        next_commands
            .iter()
            .map(|command| format!("  {command}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );

    CommandResponse {
        outcome,
        data: json!({
            "launch_attempt_id": attempt_id,
            "session_id": session_id,
            "review_run_id": run_id,
            "review_task_id": worker_task.id,
            "task_nonce": worker_task.task_nonce,
            "worker_task_path": worker_task_path_string,
            "resume_bundle_artifact_id": bundle_artifact_id,
            "repository": target.repository,
            "pull_request": target.pull_request_number,
            "provider": parsed.provider,
            "session_path": session_path,
            "target_verification": target_verification,
            "continuity_quality": continuity_state_label(&continuity_quality),
            "launch_execution_mode": if copilot_interactive { "interactive" } else { "batch" },
            "interactive": copilot_interactive,
            "hook_audit_event_count": copilot_hook_audit_event_count,
            "next_commands": next_commands,
            "provider_capability": runtime_provider_capability(runtime, &parsed.provider),
            "routine_surface": runtime_routine_surface_projection(
                runtime,
                &parsed.provider,
                binding_context.worktree_root.as_deref(),
            ),
        }),
        warnings,
        repair_actions: Vec::new(),
        message: format!("review session launched{human_details}"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ReentryInferenceScore {
    pr_match_rank: u8,
    binding_quality_rank: u8,
    continuity_quality_rank: u8,
    updated_at: i64,
}

fn continuity_inference_rank(continuity_state: &str) -> u8 {
    let normalized = continuity_state.to_ascii_lowercase();
    if normalized.contains("unusable")
        || normalized.contains("stale")
        || normalized.contains("missing")
        || normalized.contains("invalid")
    {
        0
    } else if normalized.contains("degraded") || normalized.contains("reseed") {
        1
    } else {
        2
    }
}

fn select_unique_strongest_score_index(scores: &[ReentryInferenceScore]) -> Option<usize> {
    if scores.is_empty() {
        return None;
    }

    let mut best_index = 0usize;
    let mut best_score = scores[0];
    let mut best_is_tied = false;

    for (index, score) in scores.iter().enumerate().skip(1) {
        let ordering = (
            score.pr_match_rank,
            score.binding_quality_rank,
            score.continuity_quality_rank,
            score.updated_at,
        )
            .cmp(&(
                best_score.pr_match_rank,
                best_score.binding_quality_rank,
                best_score.continuity_quality_rank,
                best_score.updated_at,
            ));
        if ordering.is_gt() {
            best_index = index;
            best_score = *score;
            best_is_tied = false;
        } else if ordering.is_eq() {
            best_is_tied = true;
        }
    }

    if best_is_tied { None } else { Some(best_index) }
}

fn picker_reason_supports_auto_selection(reason: &str, candidates: &[SessionFinderEntry]) -> bool {
    if candidates.len() < 2 {
        return false;
    }
    reason.contains("ambiguous repo-local session match")
        || reason.contains("multiple repo-local sessions")
}

fn infer_strongest_reentry_selection(
    store: &RogerStore,
    candidates: &[SessionFinderEntry],
    requested_pull_request: Option<u64>,
    source_surface: LaunchSurface,
    local_root: ResolveSessionLocalRoot<'_>,
    ui_target: Option<&str>,
    instance_preference: Option<&str>,
) -> std::result::Result<
    Option<(
        String,
        Option<SessionLaunchBindingRecord>,
        ReentryInferenceScore,
    )>,
    String,
> {
    let mut ranked = Vec::new();
    for candidate in candidates {
        let Some(session) = store.review_session(&candidate.session_id).map_err(|err| {
            format!(
                "failed to load candidate session {}: {err}",
                candidate.session_id
            )
        })?
        else {
            continue;
        };

        let binding_resolution = store
            .resolve_session_launch_binding_with_context(
                ResolveSessionLaunchBinding {
                    explicit_session_id: Some(&session.id),
                    surface: source_surface,
                    repo_locator: &session.review_target.repository,
                    review_target: Some(&session.review_target),
                    ui_target,
                    instance_preference,
                },
                local_root,
            )
            .map_err(|err| format!("failed to resolve launch binding for {}: {err}", session.id))?;

        let (binding_quality_rank, binding) = match binding_resolution {
            SessionBindingResolution::Resolved(binding) => (2, Some(binding)),
            SessionBindingResolution::NotFound => (1, None),
            SessionBindingResolution::Ambiguous { .. } | SessionBindingResolution::Stale { .. } => {
                (0, None)
            }
        };

        let score = ReentryInferenceScore {
            pr_match_rank: u8::from(
                requested_pull_request
                    .map(|value| value == session.review_target.pull_request_number)
                    .unwrap_or(false),
            ),
            binding_quality_rank,
            continuity_quality_rank: continuity_inference_rank(&session.continuity_state),
            updated_at: candidate.updated_at,
        };
        ranked.push((session.id, binding, score));
    }

    if ranked.is_empty() {
        return Ok(None);
    }

    let scores: Vec<ReentryInferenceScore> = ranked.iter().map(|(_, _, score)| *score).collect();
    let Some(best_index) = select_unique_strongest_score_index(&scores) else {
        return Ok(None);
    };
    let (session_id, binding, score) = ranked
        .into_iter()
        .nth(best_index)
        .expect("best index should exist");
    if score.binding_quality_rank == 0 {
        return Ok(None);
    }

    Ok(Some((session_id, binding, score)))
}

fn handle_resume(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    let launch_surface = resolved_launch_surface(parsed);
    let store = match open_store_or_response(runtime, "rr resume") {
        Ok(store) => store,
        Err(response) => return response,
    };

    let binding_context = LaunchBindingContext::for_cwd(&runtime.cwd);
    let repository = resolve_repository(parsed.repo.clone(), &runtime.cwd);
    let resolution = match store.resolve_session_reentry_with_context(
        ResolveSessionReentry {
            explicit_session_id: parsed.session_id.clone(),
            repository,
            pull_request_number: parsed.pr,
            source_surface: launch_surface,
            ui_target: Some(cli_config::UI_TARGET.to_owned()),
            instance_preference: Some(cli_config::INSTANCE_PREFERENCE.to_owned()),
        },
        binding_context.storage_local_root(),
    ) {
        Ok(resolution) => resolution,
        Err(err) => return error_response(format!("failed to resolve session re-entry: {err}")),
    };

    let mut inferred_selection_warning: Option<String> = None;
    let (session, binding) = match resolution {
        SessionReentryResolution::Resolved { session, binding } => (session, binding),
        SessionReentryResolution::PickerRequired { reason, candidates } => {
            if !picker_reason_supports_auto_selection(&reason, &candidates) {
                return blocked_picker_response(reason, candidates);
            }

            match infer_strongest_reentry_selection(
                &store,
                &candidates,
                parsed.pr,
                launch_surface,
                binding_context.storage_local_root(),
                Some(cli_config::UI_TARGET),
                Some(cli_config::INSTANCE_PREFERENCE),
            ) {
                Ok(Some((session_id, binding, score))) => {
                    let Some(session) = (match store.review_session(&session_id) {
                        Ok(value) => value,
                        Err(err) => {
                            return error_response(format!(
                                "failed to load inferred session {session_id}: {err}"
                            ));
                        }
                    }) else {
                        return blocked_picker_response(reason, candidates);
                    };
                    inferred_selection_warning = Some(format!(
                        "auto-selected session {} from {} candidates (pr_rank={}, binding_rank={}, continuity_rank={}, updated_at={})",
                        session.id,
                        candidates.len(),
                        score.pr_match_rank,
                        score.binding_quality_rank,
                        score.continuity_quality_rank,
                        score.updated_at
                    ));
                    (session, binding)
                }
                Ok(None) => return blocked_picker_response(reason, candidates),
                Err(err) => return error_response(err),
            }
        }
    };

    let supported_providers = runtime_supported_review_providers(runtime);
    let planned_not_live_providers = runtime_planned_not_live_review_providers(runtime);
    if !supported_providers.contains(&session.provider.as_str()) {
        let mut repair_actions = vec![
            "resume is currently available for opencode, codex, gemini, and claude sessions"
                .to_owned(),
        ];
        if session.provider == session_copilot::PROVIDER_ID
            && !copilot_feature_gated_launch_enabled(session_copilot::PROVIDER_ID)
        {
            repair_actions.push(
                "enable RR_ENABLE_COPILOT_PROVIDER=1 to resume Copilot sessions through the feature-gated tier-b continuity path"
                    .to_owned(),
            );
        }
        return blocked_response(
            format!(
                "session {} uses provider '{}' which cannot be resumed by this CLI slice",
                session.id, session.provider
            ),
            repair_actions,
            json!({
                "session_id": session.id,
                "provider": session.provider,
                "supported_providers": supported_providers,
                "planned_not_live_providers": planned_not_live_providers,
                "feature_gated_disabled_providers":
                    runtime_feature_gated_disabled_review_providers(runtime),
                "not_supported_providers": NOT_LIVE_REVIEW_PROVIDERS,
                "live_review_provider_support": runtime_review_provider_support_matrix(runtime),
            }),
        );
    }

    let command_name = "rr resume";
    let provider_capability = runtime_provider_capability(runtime, &session.provider);
    let routine_surface = runtime_routine_surface_projection(
        runtime,
        &session.provider,
        binding
            .as_ref()
            .and_then(|entry| entry.worktree_root.as_deref()),
    );

    if parsed.robot {
        let continuity_state = session.continuity_state.to_ascii_lowercase();
        let provider_support_tier = provider_capability["support_tier"]
            .as_str()
            .unwrap_or_default();
        let provider_is_tier_b = matches!(provider_support_tier, "tier_b" | "tier_b_feature_gated");
        let degraded = !provider_is_tier_b
            || continuity_state.contains("degraded")
            || continuity_state.contains("reseed")
            || continuity_state.contains("unusable");
        let has_locator = session.session_locator.is_some();
        let has_resume_bundle = session.resume_bundle_artifact_id.is_some();
        let provider_supports_reopen = provider_capability["supports"]["resume_reopen"]
            .as_bool()
            .unwrap_or(false);
        let provider_supports_reseed = provider_capability["supports"]["resume_reseed"]
            .as_bool()
            .unwrap_or(false);
        let continuity_quality = if continuity_state.contains("unusable") {
            "unusable"
        } else if degraded {
            "degraded"
        } else {
            "usable"
        };
        let inferred_resume_path = if continuity_state.contains("reseed") {
            "reseeded_from_bundle"
        } else if has_locator && provider_supports_reopen && !continuity_state.contains("unusable")
        {
            "reopened_by_locator"
        } else if has_resume_bundle || (provider_supports_reseed && !provider_supports_reopen) {
            "reseeded_from_bundle"
        } else if continuity_state.contains("reopen") {
            "reopened_by_locator"
        } else {
            "launch_suppressed_non_interactive"
        };
        return CommandResponse {
            outcome: if degraded {
                OutcomeKind::Degraded
            } else {
                OutcomeKind::Complete
            },
            data: json!({
                "mode": "robot_non_interactive",
                "launch_suppressed": true,
                "reason_code": "interactive_launch_suppressed_for_robot_mode",
                "session_id": session.id,
                "repository": session.review_target.repository,
                "pull_request": session.review_target.pull_request_number,
                "provider": session.provider,
                "command": "resume",
                "resume_path": inferred_resume_path,
                "continuity_quality": continuity_quality,
                "continuity_state_snapshot": session.continuity_state,
                "worker_task_path": normalized_path_string(&worker_task_file_path(
                    &runtime.store_root,
                    &session.id,
                )),
                "provider_capability": provider_capability.clone(),
                "routine_surface": routine_surface.clone(),
            }),
            warnings: {
                let mut warnings: Vec<String> =
                    inferred_selection_warning.iter().cloned().collect();
                warnings.extend(provider_support_warning(&session.provider, command_name));
                warnings
            },
            repair_actions: Vec::new(),
            message: format!("{command_name} completed in robot non-interactive mode"),
        };
    }

    if parsed.dry_run {
        return CommandResponse {
            outcome: OutcomeKind::Complete,
            data: json!({
                "mode": "dry_run",
                "session_id": session.id,
                "repository": session.review_target.repository,
                "pull_request": session.review_target.pull_request_number,
                "command": "resume",
                "provider": session.provider,
                "provider_capability": provider_capability.clone(),
                "routine_surface": routine_surface.clone(),
            }),
            warnings: {
                let mut warnings: Vec<String> =
                    inferred_selection_warning.iter().cloned().collect();
                warnings.extend(provider_support_warning(&session.provider, command_name));
                warnings
            },
            repair_actions: Vec::new(),
            message: "resume plan generated (dry-run)".to_owned(),
        };
    }

    // Interactive resume actually reopens the provider session; fail closed
    // and loudly when the provider binary cannot run instead of silently
    // degrading into a virtual reseed.
    if let Some(binary) = provider_launch_binary_for(runtime, &session.provider)
        && !binary_resolves_locally(&binary)
    {
        return provider_binary_missing_response("rr resume", &session.provider, &binary);
    }

    let attempt_id = next_id("attempt");
    if let Err(err) = store.create_launch_attempt(CreateLaunchAttempt {
        id: &attempt_id,
        action: LaunchAttemptAction::ResumeReview,
        provider: &session.provider,
        source_surface: launch_surface,
        review_target: &session.review_target,
        requested_session_id: Some(&session.id),
        state: LaunchAttemptState::Pending,
    }) {
        return error_response(format!("failed to create launch attempt: {err}"));
    }

    if let Err(err) = persist_launch_attempt_state(
        &store,
        &attempt_id,
        LaunchAttemptState::Dispatching,
        None,
        None,
        None,
        None,
        None,
    ) {
        return error_response(err);
    }

    let intent = launch_intent(LaunchAction::ResumeReview, runtime);

    let resume_bundle = match session.resume_bundle_artifact_id.as_deref() {
        Some(id) => match store.load_resume_bundle(id) {
            Ok(bundle) => Some(bundle),
            Err(err) => {
                let detail = format!("resume bundle could not be loaded: {err}");
                if let Err(update_err) = persist_launch_attempt_state(
                    &store,
                    &attempt_id,
                    LaunchAttemptState::FailedSessionBinding,
                    None,
                    None,
                    None,
                    None,
                    Some(&detail),
                ) {
                    return error_response(update_err);
                }
                return blocked_response(
                    detail,
                    vec!["re-run rr review to regenerate ResumeBundle".to_owned()],
                    json!({
                        "reason_code": "resume_bundle_missing_or_invalid",
                        "session_id": session.id,
                        "launch_attempt_id": attempt_id,
                    }),
                );
            }
        },
        None => None,
    };

    let mut copilot_interactive = false;
    let mut copilot_hook_audit_event_count = 0usize;
    let (
        session_locator,
        resume_path,
        terminal_state,
        continuity_quality,
        decision_reason,
        mut warnings,
    ) = match session.provider.as_str() {
        "opencode" => {
            let adapter = OpenCodeAdapter::with_binary(runtime.opencode_bin.clone());
            let linkage = match adapter.link_session(
                &session.review_target,
                &intent,
                session.session_locator.as_ref(),
                resume_bundle.as_ref(),
            ) {
                Ok(linkage) => linkage,
                Err(err) => {
                    let detail = format!("resume failed: {err}");
                    if let Err(update_err) = persist_launch_attempt_state(
                        &store,
                        &attempt_id,
                        LaunchAttemptState::FailedSpawn,
                        None,
                        None,
                        None,
                        None,
                        Some(&detail),
                    ) {
                        return error_response(update_err);
                    }
                    return blocked_response(
                        detail,
                        vec![
                            "ensure a valid ResumeBundle exists or launch a new review with rr review"
                                .to_owned(),
                        ],
                        json!({
                            "reason_code": "resume_failed_closed",
                            "session_id": session.id,
                            "launch_attempt_id": attempt_id,
                            "error": err.to_string(),
                        }),
                    );
                }
            };
            (
                linkage.locator,
                session_path_label(&linkage.path).to_owned(),
                match linkage.path {
                    OpenCodeSessionPath::ReopenedByLocator => LaunchAttemptState::VerifiedReopened,
                    OpenCodeSessionPath::ReseededFromBundle => LaunchAttemptState::VerifiedReseeded,
                    OpenCodeSessionPath::StartedFresh => LaunchAttemptState::VerifiedStarted,
                },
                linkage.continuity_quality,
                linkage
                    .decision
                    .as_ref()
                    .map(|decision| format!("{:?}", decision.reason_code))
                    .unwrap_or_else(|| "none".to_owned()),
                Vec::new(),
            )
        }
        "codex" => {
            let adapter = CodexAdapter::new();
            let linkage = match adapter.link_session(
                &session.review_target,
                &intent,
                session.session_locator.as_ref(),
                resume_bundle.as_ref(),
            ) {
                Ok(linkage) => linkage,
                Err(err) => {
                    let detail = format!("resume failed: {err}");
                    if let Err(update_err) = persist_launch_attempt_state(
                        &store,
                        &attempt_id,
                        LaunchAttemptState::FailedSpawn,
                        None,
                        None,
                        None,
                        None,
                        Some(&detail),
                    ) {
                        return error_response(update_err);
                    }
                    return blocked_response(
                        detail,
                        vec![
                            "ensure a valid ResumeBundle exists or launch a new review with rr review --provider codex"
                                .to_owned(),
                        ],
                        json!({
                            "reason_code": "resume_failed_closed",
                            "session_id": session.id,
                            "launch_attempt_id": attempt_id,
                            "error": err.to_string(),
                        }),
                    );
                }
            };
            (
                linkage.locator,
                codex_session_path_label(&linkage.path).to_owned(),
                match linkage.path {
                    CodexSessionPath::ReseededFromBundle => LaunchAttemptState::VerifiedReseeded,
                    CodexSessionPath::StartedFresh => LaunchAttemptState::VerifiedStarted,
                },
                linkage.continuity_quality,
                linkage
                    .decision
                    .as_ref()
                    .map(|decision| format!("{:?}", decision.reason_code))
                    .unwrap_or_else(|| "none".to_owned()),
                provider_support_warning(&session.provider, "rr resume")
                    .into_iter()
                    .collect(),
            )
        }
        "claude" => {
            let adapter = ClaudeAdapter::new();
            let linkage = match adapter.link_session(
                &session.review_target,
                &intent,
                session.session_locator.as_ref(),
                resume_bundle.as_ref(),
            ) {
                Ok(linkage) => linkage,
                Err(err) => {
                    let detail = format!("resume failed: {err}");
                    if let Err(update_err) = persist_launch_attempt_state(
                        &store,
                        &attempt_id,
                        LaunchAttemptState::FailedSpawn,
                        None,
                        None,
                        None,
                        None,
                        Some(&detail),
                    ) {
                        return error_response(update_err);
                    }
                    return blocked_response(
                        detail,
                        vec![
                            "ensure a valid ResumeBundle exists or launch a new review with rr review --provider claude"
                                .to_owned(),
                        ],
                        json!({
                            "reason_code": "resume_failed_closed",
                            "session_id": session.id,
                            "launch_attempt_id": attempt_id,
                            "error": err.to_string(),
                        }),
                    );
                }
            };
            (
                linkage.locator,
                claude_session_path_label(&linkage.path).to_owned(),
                match linkage.path {
                    ClaudeSessionPath::ReseededFromBundle => LaunchAttemptState::VerifiedReseeded,
                    ClaudeSessionPath::StartedFresh => LaunchAttemptState::VerifiedStarted,
                },
                linkage.continuity_quality,
                linkage
                    .decision
                    .as_ref()
                    .map(|decision| format!("{:?}", decision.reason_code))
                    .unwrap_or_else(|| "none".to_owned()),
                provider_support_warning(&session.provider, "rr resume")
                    .into_iter()
                    .collect(),
            )
        }
        "gemini" => {
            let adapter = GeminiAdapter::new();
            let linkage = match adapter.link_session(
                &session.review_target,
                &intent,
                session.session_locator.as_ref(),
                resume_bundle.as_ref(),
            ) {
                Ok(linkage) => linkage,
                Err(err) => {
                    let detail = format!("resume failed: {err}");
                    if let Err(update_err) = persist_launch_attempt_state(
                        &store,
                        &attempt_id,
                        LaunchAttemptState::FailedSpawn,
                        None,
                        None,
                        None,
                        None,
                        Some(&detail),
                    ) {
                        return error_response(update_err);
                    }
                    return blocked_response(
                        detail,
                        vec![
                            "ensure a valid ResumeBundle exists or launch a new review with rr review --provider gemini"
                                .to_owned(),
                        ],
                        json!({
                            "reason_code": "resume_failed_closed",
                            "session_id": session.id,
                            "launch_attempt_id": attempt_id,
                            "error": err.to_string(),
                        }),
                    );
                }
            };
            (
                linkage.locator,
                gemini_session_path_label(&linkage.path).to_owned(),
                match linkage.path {
                    GeminiSessionPath::ReseededFromBundle => LaunchAttemptState::VerifiedReseeded,
                    GeminiSessionPath::StartedFresh => LaunchAttemptState::VerifiedStarted,
                },
                linkage.continuity_quality,
                linkage
                    .decision
                    .as_ref()
                    .map(|decision| format!("{:?}", decision.reason_code))
                    .unwrap_or_else(|| "none".to_owned()),
                provider_support_warning(&session.provider, "rr resume")
                    .into_iter()
                    .collect(),
            )
        }
        "copilot" => {
            let worker_task_path =
                normalized_path_string(&worker_task_file_path(&runtime.store_root, &session.id));
            let continuity = match launch_copilot_resume_or_return_session(
                runtime,
                &session.review_target,
                &attempt_id,
                &binding_context,
                session.session_locator.as_ref(),
                resume_bundle.as_ref(),
                &worker_task_path,
                parsed.interactive,
            ) {
                Ok(linkage) => linkage,
                Err(err) => {
                    if let Err(update_err) = persist_launch_attempt_state(
                        &store,
                        &attempt_id,
                        err.state,
                        None,
                        None,
                        None,
                        None,
                        Some(&err.detail),
                    ) {
                        return error_response(update_err);
                    }
                    return blocked_response(
                        format!("resume failed: {}", err.detail),
                        err.repair_actions,
                        {
                            let mut data = serde_json::Map::new();
                            data.insert(
                                "reason_code".to_owned(),
                                Value::String(err.reason_code.to_owned()),
                            );
                            data.insert(
                                "launch_attempt_id".to_owned(),
                                Value::String(attempt_id.clone()),
                            );
                            data.insert(
                                "provider".to_owned(),
                                Value::String(session_copilot::PROVIDER_ID.to_owned()),
                            );
                            data.insert("session_id".to_owned(), Value::String(session.id.clone()));
                            if let Some(extra) = err.extra_data.as_object() {
                                for (key, value) in extra {
                                    data.insert(key.clone(), value.clone());
                                }
                            }
                            Value::Object(data)
                        },
                    );
                }
            };
            copilot_interactive = continuity.linkage.interactive;
            copilot_hook_audit_event_count = continuity.linkage.hook_audit_event_count;
            let mut copilot_warnings: Vec<String> =
                provider_support_warning(&session.provider, "rr resume")
                    .into_iter()
                    .collect();
            if continuity.linkage.hook_audit_event_count > 0 {
                copilot_warnings.push(format!(
                    "captured {} hook audit events",
                    continuity.linkage.hook_audit_event_count
                ));
            }
            (
                continuity.linkage.locator,
                continuity.linkage.session_path,
                continuity.terminal_state,
                continuity.linkage.continuity_quality,
                continuity.decision_reason,
                copilot_warnings,
            )
        }
        _ => unreachable!("provider validated above"),
    };
    if let Some(warning) = inferred_selection_warning {
        warnings.insert(0, warning);
    }
    let mut repair_actions = Vec::new();
    if session.provider == "opencode"
        && let Some((warning, actions)) = opencode_legacy_config_guidance(&runtime.opencode_bin)
    {
        warnings.push(warning);
        repair_actions.extend(actions);
    }

    let provider_session_id =
        match verified_provider_session_id(&session.provider, &session_locator) {
            Ok(session_id) => session_id.to_owned(),
            Err(detail) => {
                if let Err(update_err) = persist_launch_attempt_state(
                    &store,
                    &attempt_id,
                    LaunchAttemptState::FailedProviderVerification,
                    None,
                    None,
                    None,
                    Some(&session_locator),
                    Some(&detail),
                ) {
                    return error_response(update_err);
                }
                return blocked_response(
                    format!("failed to verify provider session: {detail}"),
                    vec!["re-run rr resume after verifying provider launch output".to_owned()],
                    json!({
                        "reason_code": "provider_session_unverified",
                        "launch_attempt_id": attempt_id,
                        "provider": session.provider,
                        "session_id": session.id,
                    }),
                );
            }
        };

    if let Err(err) = persist_launch_attempt_state(
        &store,
        &attempt_id,
        LaunchAttemptState::AwaitingProviderVerification,
        None,
        None,
        Some(&provider_session_id),
        Some(&session_locator),
        None,
    ) {
        return error_response(err);
    }

    let run_kind = "resume";
    let run_id = next_id("run");
    let binding_id = binding
        .as_ref()
        .map(|record| record.id.clone())
        .unwrap_or_else(|| next_id("binding"));
    let continuity_state = format!("{run_kind}:{}", continuity_state_label(&continuity_quality));
    if let Err(err) =
        store.finalize_existing_session_launch_attempt(FinalizeExistingSessionLaunchAttempt {
            attempt_id: &attempt_id,
            terminal_state,
            provider_session_id: &provider_session_id,
            verified_locator: &session_locator,
            review_session_id: &session.id,
            expected_session_row_version: session.row_version,
            continuity_state: &continuity_state,
            attention_state: "review_resumed",
            review_run: CreateReviewRun {
                id: &run_id,
                session_id: &session.id,
                run_kind,
                repo_snapshot: &format!(
                    "{}#{}",
                    session.review_target.repository, session.review_target.pull_request_number
                ),
                continuity_quality: continuity_state_label(&continuity_quality),
                session_locator_artifact_id: None,
            },
            launch_binding: CreateSessionLaunchBinding {
                id: &binding_id,
                session_id: &session.id,
                repo_locator: &session.review_target.repository,
                review_target: Some(&session.review_target),
                surface: launch_surface,
                launch_profile_id: Some(cli_config::PROFILE_ID),
                ui_target: Some(cli_config::UI_TARGET),
                instance_preference: Some(cli_config::INSTANCE_PREFERENCE),
                cwd: binding_local_root_for_surface(launch_surface, &binding_context).0,
                worktree_root: binding_local_root_for_surface(launch_surface, &binding_context).1,
            },
        })
    {
        let detail = format!("failed to finalize resume launch: {err}");
        let failure_state = match err {
            roger_storage::ExistingSessionLaunchFinalizationError::SessionBinding(_) => {
                LaunchAttemptState::FailedSessionBinding
            }
            roger_storage::ExistingSessionLaunchFinalizationError::Commit(_) => {
                LaunchAttemptState::FailedCommit
            }
        };
        if let Err(update_err) = persist_launch_attempt_state(
            &store,
            &attempt_id,
            failure_state,
            None,
            None,
            Some(&provider_session_id),
            Some(&session_locator),
            Some(&detail),
        ) {
            return error_response(update_err);
        }
        return error_response(detail);
    }

    let degraded = !matches!(continuity_quality, ContinuityQuality::Usable)
        || resume_path == "reseeded_from_bundle";

    // Rebind the worker-task file to the CURRENT resume run so the in-session
    // worker's `rr agent worker.* --task-file <path>` calls target live ids.
    let worker_task = build_launch_review_task(&session.id, &run_id, "deep_review");
    let worker_task_path_string =
        normalized_path_string(&worker_task_file_path(&runtime.store_root, &session.id));
    if let Err(err) = write_worker_task_file(&runtime.store_root, &worker_task) {
        warnings.push(format!("failed to persist worker-task binding: {err}"));
    }
    let next_commands = review_next_commands(
        &session.id,
        &session.provider,
        &session.review_target.repository,
        session.review_target.pull_request_number,
        copilot_interactive,
    );

    CommandResponse {
        outcome: if degraded {
            OutcomeKind::Degraded
        } else {
            OutcomeKind::Complete
        },
        data: json!({
            "launch_attempt_id": attempt_id,
            "session_id": session.id,
            "review_run_id": run_id,
            "review_task_id": worker_task.id,
            "task_nonce": worker_task.task_nonce,
            "worker_task_path": worker_task_path_string,
            "repository": session.review_target.repository,
            "pull_request": session.review_target.pull_request_number,
            "provider": session.provider,
            "resume_path": resume_path,
            "continuity_quality": continuity_state_label(&continuity_quality),
            "decision_reason": decision_reason,
            "launch_execution_mode": if copilot_interactive { "interactive" } else { "batch" },
            "interactive": copilot_interactive,
            "hook_audit_event_count": copilot_hook_audit_event_count,
            "next_commands": next_commands,
            "provider_capability": provider_capability.clone(),
            "routine_surface": routine_surface.clone(),
        }),
        warnings,
        repair_actions,
        message: format!("{run_kind} completed"),
    }
}

fn handle_return(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    let launch_surface = resolved_launch_surface(parsed);
    let store = match open_store_or_response(runtime, "rr return") {
        Ok(store) => store,
        Err(response) => return response,
    };

    let binding_context = LaunchBindingContext::for_cwd(&runtime.cwd);
    let repository = resolve_repository(parsed.repo.clone(), &runtime.cwd);
    let resolution = match store.resolve_session_reentry_with_context(
        ResolveSessionReentry {
            explicit_session_id: parsed.session_id.clone(),
            repository,
            pull_request_number: parsed.pr,
            source_surface: launch_surface,
            ui_target: Some(cli_config::UI_TARGET.to_owned()),
            instance_preference: Some(cli_config::INSTANCE_PREFERENCE.to_owned()),
        },
        binding_context.storage_local_root(),
    ) {
        Ok(resolution) => resolution,
        Err(err) => {
            return error_response(format!("failed to resolve session for rr return: {err}"));
        }
    };

    let (session, binding) = match resolution {
        SessionReentryResolution::Resolved { session, binding } => (session, binding),
        SessionReentryResolution::PickerRequired { reason, candidates } => {
            return blocked_picker_response(reason, candidates);
        }
    };

    let provider_capability = runtime_provider_capability(runtime, &session.provider);
    if session.provider != "opencode"
        && !(session.provider == session_copilot::PROVIDER_ID
            && copilot_feature_gated_launch_enabled(&session.provider))
    {
        let mut capability = provider_capability.clone();
        capability["required_tier_for_return"] = json!("tier_b");
        capability["supports_rr_return"] = json!(false);
        return blocked_response(
            format!(
                "rr return is unsupported for provider '{}' on the current live CLI surface",
                session.provider
            ),
            vec![
                "rr return is only blessed on OpenCode and feature-gated Copilot tier-b sessions"
                    .to_owned(),
            ],
            json!({
                "session_id": session.id,
                "provider": session.provider,
                "provider_capability": capability,
            }),
        );
    }

    // rr return drives a real provider reopen; a missing binary must block
    // loudly instead of silently reseeding or claiming a return happened.
    if let Some(binary) = provider_launch_binary_for(runtime, &session.provider)
        && !binary_resolves_locally(&binary)
    {
        return provider_binary_missing_response("rr return", &session.provider, &binary);
    }

    let attempt_id = next_id("attempt");
    if let Err(err) = store.create_launch_attempt(CreateLaunchAttempt {
        id: &attempt_id,
        action: LaunchAttemptAction::ReturnToRoger,
        provider: &session.provider,
        source_surface: launch_surface,
        review_target: &session.review_target,
        requested_session_id: Some(&session.id),
        state: LaunchAttemptState::Pending,
    }) {
        return error_response(format!("failed to create launch attempt: {err}"));
    }

    if let Err(err) = persist_launch_attempt_state(
        &store,
        &attempt_id,
        LaunchAttemptState::Dispatching,
        None,
        None,
        None,
        None,
        None,
    ) {
        return error_response(err);
    }

    let resume_bundle = if session.provider == session_copilot::PROVIDER_ID {
        match session.resume_bundle_artifact_id.as_deref() {
            Some(id) => match store.load_resume_bundle(id) {
                Ok(bundle) => Some(bundle),
                Err(err) => {
                    let detail = format!("resume bundle could not be loaded: {err}");
                    if let Err(update_err) = persist_launch_attempt_state(
                        &store,
                        &attempt_id,
                        LaunchAttemptState::FailedSessionBinding,
                        None,
                        None,
                        None,
                        None,
                        Some(&detail),
                    ) {
                        return error_response(update_err);
                    }
                    return blocked_response(
                        detail,
                        vec!["re-run rr review to regenerate ResumeBundle".to_owned()],
                        json!({
                            "reason_code": "resume_bundle_missing_or_invalid",
                            "session_id": session.id,
                            "launch_attempt_id": attempt_id,
                        }),
                    );
                }
            },
            None => None,
        }
    } else {
        None
    };

    let mut copilot_interactive = false;
    let mut copilot_hook_audit_event_count = 0usize;
    let (
        provider_session_locator,
        return_path,
        terminal_state,
        continuity_quality,
        decision_reason,
    ) = match session.provider.as_str() {
        "opencode" => {
            let adapter = OpenCodeAdapter::with_binary(runtime.opencode_bin.clone());
            let reopen_outcome = classify_reopen_outcome_for_return(
                &adapter,
                &session.review_target,
                session.session_locator.as_ref(),
            );

            let outcome = match rr_return_to_roger_session(
                &adapter,
                &store,
                ResolveSessionLaunchBinding {
                    explicit_session_id: Some(&session.id),
                    surface: launch_surface,
                    repo_locator: &session.review_target.repository,
                    review_target: Some(&session.review_target),
                    ui_target: Some(cli_config::UI_TARGET),
                    instance_preference: Some(cli_config::INSTANCE_PREFERENCE),
                },
                reopen_outcome,
            ) {
                Ok(outcome) => outcome,
                Err(err) => {
                    let detail = format!("rr return failed: {err}");
                    if let Err(update_err) = persist_launch_attempt_state(
                        &store,
                        &attempt_id,
                        LaunchAttemptState::FailedSpawn,
                        None,
                        None,
                        None,
                        None,
                        Some(&detail),
                    ) {
                        return error_response(update_err);
                    }
                    return blocked_response(
                        detail,
                        vec![
                            "ensure a valid binding and ResumeBundle exist for this repo"
                                .to_owned(),
                        ],
                        json!({
                            "reason_code": "rr_return_failed",
                            "session_id": session.id,
                            "launch_attempt_id": attempt_id,
                        }),
                    );
                }
            };

            let return_path = return_path_label(outcome.path).to_owned();
            let terminal_state = if return_path == "reseeded_session" {
                LaunchAttemptState::VerifiedReseeded
            } else {
                LaunchAttemptState::VerifiedReopened
            };
            (
                outcome.locator,
                return_path,
                terminal_state,
                outcome.continuity_quality,
                format!("{:?}", outcome.decision.reason_code),
            )
        }
        "copilot" => {
            let worker_task_path =
                normalized_path_string(&worker_task_file_path(&runtime.store_root, &session.id));
            let continuity = match launch_copilot_resume_or_return_session(
                runtime,
                &session.review_target,
                &attempt_id,
                &binding_context,
                session.session_locator.as_ref(),
                resume_bundle.as_ref(),
                &worker_task_path,
                parsed.interactive,
            ) {
                Ok(linkage) => linkage,
                Err(err) => {
                    if let Err(update_err) = persist_launch_attempt_state(
                        &store,
                        &attempt_id,
                        err.state,
                        None,
                        None,
                        None,
                        None,
                        Some(&err.detail),
                    ) {
                        return error_response(update_err);
                    }
                    return blocked_response(
                        format!("rr return failed: {}", err.detail),
                        err.repair_actions,
                        {
                            let mut data = serde_json::Map::new();
                            data.insert(
                                "reason_code".to_owned(),
                                Value::String(err.reason_code.to_owned()),
                            );
                            data.insert(
                                "launch_attempt_id".to_owned(),
                                Value::String(attempt_id.clone()),
                            );
                            data.insert(
                                "provider".to_owned(),
                                Value::String(session_copilot::PROVIDER_ID.to_owned()),
                            );
                            data.insert("session_id".to_owned(), Value::String(session.id.clone()));
                            if let Some(extra) = err.extra_data.as_object() {
                                for (key, value) in extra {
                                    data.insert(key.clone(), value.clone());
                                }
                            }
                            Value::Object(data)
                        },
                    );
                }
            };

            copilot_interactive = continuity.linkage.interactive;
            copilot_hook_audit_event_count = continuity.linkage.hook_audit_event_count;
            (
                continuity.linkage.locator,
                continuity.linkage.session_path,
                continuity.terminal_state,
                continuity.linkage.continuity_quality,
                continuity.decision_reason,
            )
        }
        _ => unreachable!("provider return support validated above"),
    };

    let provider_session_id =
        match verified_provider_session_id(&session.provider, &provider_session_locator) {
            Ok(session_id) => session_id.to_owned(),
            Err(detail) => {
                if let Err(update_err) = persist_launch_attempt_state(
                    &store,
                    &attempt_id,
                    LaunchAttemptState::FailedProviderVerification,
                    None,
                    None,
                    None,
                    Some(&provider_session_locator),
                    Some(&detail),
                ) {
                    return error_response(update_err);
                }
                return blocked_response(
                    format!("failed to verify provider session: {detail}"),
                    vec!["re-run rr return after verifying provider launch output".to_owned()],
                    json!({
                        "reason_code": "provider_session_unverified",
                        "launch_attempt_id": attempt_id,
                        "provider": session.provider,
                        "session_id": session.id,
                    }),
                );
            }
        };

    if let Err(err) = persist_launch_attempt_state(
        &store,
        &attempt_id,
        LaunchAttemptState::AwaitingProviderVerification,
        None,
        None,
        Some(&provider_session_id),
        Some(&provider_session_locator),
        None,
    ) {
        return error_response(err);
    }

    let binding_id = binding
        .as_ref()
        .map(|record| record.id.clone())
        .unwrap_or_else(|| next_id("binding"));
    let run_id = next_id("run");
    let continuity_state = format!("return:{}", continuity_state_label(&continuity_quality));
    if let Err(err) =
        store.finalize_existing_session_launch_attempt(FinalizeExistingSessionLaunchAttempt {
            attempt_id: &attempt_id,
            terminal_state,
            provider_session_id: &provider_session_id,
            verified_locator: &provider_session_locator,
            review_session_id: &session.id,
            expected_session_row_version: session.row_version,
            continuity_state: &continuity_state,
            attention_state: "returned_to_roger",
            review_run: CreateReviewRun {
                id: &run_id,
                session_id: &session.id,
                run_kind: "return",
                repo_snapshot: &format!(
                    "{}#{}",
                    session.review_target.repository, session.review_target.pull_request_number
                ),
                continuity_quality: continuity_state_label(&continuity_quality),
                session_locator_artifact_id: None,
            },
            launch_binding: CreateSessionLaunchBinding {
                id: &binding_id,
                session_id: &session.id,
                repo_locator: &session.review_target.repository,
                review_target: Some(&session.review_target),
                surface: launch_surface,
                launch_profile_id: Some(cli_config::PROFILE_ID),
                ui_target: Some(cli_config::UI_TARGET),
                instance_preference: Some(cli_config::INSTANCE_PREFERENCE),
                cwd: binding_local_root_for_surface(launch_surface, &binding_context).0,
                worktree_root: binding_local_root_for_surface(launch_surface, &binding_context).1,
            },
        })
    {
        let detail = format!("failed to finalize return launch: {err}");
        let failure_state = match err {
            roger_storage::ExistingSessionLaunchFinalizationError::SessionBinding(_) => {
                LaunchAttemptState::FailedSessionBinding
            }
            roger_storage::ExistingSessionLaunchFinalizationError::Commit(_) => {
                LaunchAttemptState::FailedCommit
            }
        };
        if let Err(update_err) = persist_launch_attempt_state(
            &store,
            &attempt_id,
            failure_state,
            None,
            None,
            Some(&provider_session_id),
            Some(&provider_session_locator),
            Some(&detail),
        ) {
            return error_response(update_err);
        }
        return error_response(detail);
    }

    let degraded = !matches!(continuity_quality, ContinuityQuality::Usable)
        || return_path == "reseeded_from_bundle"
        || return_path == "reseeded_session";
    let mut warnings = Vec::new();
    let mut repair_actions = Vec::new();
    if session.provider == "opencode"
        && let Some((warning, actions)) = opencode_legacy_config_guidance(&runtime.opencode_bin)
    {
        warnings.push(warning);
        repair_actions.extend(actions);
    }
    if copilot_hook_audit_event_count > 0 {
        warnings.push(format!(
            "captured {} hook audit events",
            copilot_hook_audit_event_count
        ));
    }

    CommandResponse {
        outcome: if degraded {
            OutcomeKind::Degraded
        } else {
            OutcomeKind::Complete
        },
        data: {
            let mut capability = provider_capability.clone();
            capability["supports_rr_return"] = json!(true);
            capability["required_tier_for_return"] = json!("tier_b");
            json!({
                "launch_attempt_id": attempt_id,
                "session_id": session.id,
                "review_run_id": run_id,
                "provider_capability": capability,
                "return_path": return_path,
                "continuity_quality": continuity_state_label(&continuity_quality),
                "decision_reason": decision_reason,
                "launch_execution_mode": if copilot_interactive { "interactive" } else { "batch" },
                "interactive": copilot_interactive,
                "hook_audit_event_count": copilot_hook_audit_event_count,
            })
        },
        warnings,
        repair_actions,
        message: "rr return completed".to_owned(),
    }
}

/// Canonical attention-state vocabulary `rr sessions --attention` may filter on.
///
/// The set is the union of the states the system writes onto a session
/// (`review_launched`, `review_resumed`, `returned_to_roger`, `findings_ready`,
/// `awaiting_user_input`, and the persisted `refresh_recommended` flag) and the
/// forward-compat states the prs-queue projection (`derive_prs_queue_state`)
/// explicitly recognizes (`awaiting_return`, `review_failed`,
/// `outbound_approval_required`). Filtering on any of these can return real
/// rows, so the validator must never reject them; only an unknown typo is
/// blocked, matching how `--query-mode` fails closed on an unsupported value.
const CANONICAL_ATTENTION_STATES: &[&str] = &[
    "awaiting_return",
    "awaiting_user_input",
    "findings_ready",
    "outbound_approval_required",
    "refresh_recommended",
    "returned_to_roger",
    "review_failed",
    "review_launched",
    "review_resumed",
];

fn handle_sessions(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    let store = match open_store_or_response(runtime, "rr sessions") {
        Ok(store) => store,
        Err(response) => return response,
    };

    let unknown_attention_states = parsed
        .attention_states
        .iter()
        .filter(|state| !CANONICAL_ATTENTION_STATES.contains(&state.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if !unknown_attention_states.is_empty() {
        return blocked_response(
            format!(
                "rr sessions cannot filter on unknown attention state(s): {}",
                unknown_attention_states.join(", ")
            ),
            vec![format!(
                "pass --attention with one or more of: {}",
                CANONICAL_ATTENTION_STATES.join(", ")
            )],
            json!({
                "reason_code": "unknown_attention_state",
                "unknown_attention_states": unknown_attention_states,
                "supported_attention_states": CANONICAL_ATTENTION_STATES,
            }),
        );
    }

    // --all widens the underlying fetch so the grouped human view can list every
    // session per PR; the default view keeps the bounded window.
    let default_limit = if parsed.show_all { 250 } else { 25 };
    let limit = parsed.limit.unwrap_or(default_limit).min(250);
    let fetch_limit = limit.saturating_add(1).min(250);
    let sessions = match store.session_finder(SessionFinderQuery {
        repository: parsed.repo.clone(),
        pull_request_number: parsed.pr,
        attention_states: parsed.attention_states.clone(),
        limit: fetch_limit,
    }) {
        Ok(items) => items,
        Err(err) => return error_response(format!("failed to list sessions: {err}")),
    };

    let truncated = sessions.len() > limit;
    let visible = if truncated {
        sessions.into_iter().take(limit).collect::<Vec<_>>()
    } else {
        sessions
    };

    let count = visible.len();
    let items = visible
        .into_iter()
        .map(|entry| {
            json!({
                "session_id": entry.session_id,
                "repo": entry.repository,
                "target": {
                    "repository": entry.repository,
                    "pull_request": entry.pull_request_number,
                },
                "attention_state": entry.attention_state,
                "provider": entry.provider,
                "provider_capability": provider_capability(&entry.provider),
                "updated_at": entry.updated_at,
                "follow_on": {
                    "requires_explicit_session": true,
                    "resume_command": format!("rr resume --session {}", entry.session_id),
                    "reconciliation_mode": if entry.attention_state == "refresh_recommended" {
                        "reentry_required"
                    } else {
                        "automatic_background"
                    },
                    "manual_refresh_supported": false,
                    "stale_target_detected": entry.attention_state == "refresh_recommended",
                }
            })
        })
        .collect::<Vec<_>>();

    let outcome = if count == 0 {
        OutcomeKind::Empty
    } else {
        OutcomeKind::Complete
    };
    let message = if count == 0 {
        "no sessions matched filters".to_owned()
    } else {
        format!("loaded {count} sessions")
    };

    CommandResponse {
        outcome,
        data: json!({
            "items": items,
            "count": count,
            "truncated": truncated,
            "show_all": parsed.show_all,
            "filters_applied": {
                "repository": parsed.repo,
                "pull_request": parsed.pr,
                "attention_states": parsed.attention_states,
                "limit": limit,
            }
        }),
        warnings: Vec::new(),
        repair_actions: Vec::new(),
        message,
    }
}

const PRS_QUEUE_DEFAULT_PROVIDER: &str = "opencode";

/// Derive the truthful review-queue state for one open PR from the joined
/// local session state. Outbound evidence (posted/drafted) wins over the
/// persisted attention state; unrecognized attention states are surfaced
/// as-is instead of being guessed into a bucket.
fn derive_prs_queue_state(
    attention_state: &str,
    draft_count: i64,
    posted_action_count: i64,
) -> String {
    if posted_action_count > 0 {
        return "posted".to_owned();
    }
    if draft_count > 0 {
        return "drafted".to_owned();
    }
    match attention_state {
        "awaiting_user_input"
        | "refresh_recommended"
        | "review_failed"
        | "outbound_approval_required" => "needs_attention".to_owned(),
        "review_launched" | "review_resumed" | "awaiting_return" | "returned_to_roger" => {
            "in_review".to_owned()
        }
        other => other.to_owned(),
    }
}

fn prs_queue_next_command(roger_state: &str, pr_number: u64) -> String {
    if roger_state == "not_started" {
        format!("rr review --pr {pr_number} --provider {PRS_QUEUE_DEFAULT_PROVIDER}")
    } else {
        format!("rr resume --pr {pr_number}")
    }
}

fn handle_prs(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    let Some(repository) = resolve_repository(parsed.repo.clone(), &runtime.cwd) else {
        return blocked_response(
            "repo context inference failed; rr prs needs a concrete repository".to_owned(),
            vec![
                "pass --repo owner/repo".to_owned(),
                "or run rr prs inside a git repo with a GitHub remote.origin.url".to_owned(),
            ],
            json!({"reason_code": "repo_context_missing"}),
        );
    };

    let Some((owner, repo_name)) = repository.split_once('/') else {
        return blocked_response(
            format!("repository slug is not in owner/repo form: {repository}"),
            vec!["pass --repo owner/repo".to_owned()],
            json!({"reason_code": "repo_slug_invalid", "repository": repository}),
        );
    };

    let store = match open_store_or_response(runtime, "rr prs") {
        Ok(store) => store,
        Err(response) => return response,
    };

    let limit = parsed.limit.unwrap_or(25).min(100);
    let adapter = GhCliAdapter::new();
    let open_prs = match adapter.list_open_pull_requests(owner, repo_name, limit) {
        Ok(mut open_prs) => {
            open_prs.truncate(limit);
            open_prs
        }
        Err(GitHubAdapterError::GhNotFound) => {
            return blocked_response(
                "rr prs requires the GitHub CLI (gh), which was not found or not executable"
                    .to_owned(),
                vec![
                    "install the GitHub CLI (gh)".to_owned(),
                    "run gh auth login to authenticate".to_owned(),
                ],
                json!({
                    "reason_code": "gh_unavailable",
                    "adapter_error": GitHubAdapterError::GhNotFound.to_string(),
                    "repository": repository,
                }),
            );
        }
        Err(err @ GitHubAdapterError::GhCommandFailed { .. }) => {
            return blocked_response(
                format!("rr prs could not list open pull requests for {repository}: {err}"),
                vec![
                    "run gh auth status to verify GitHub CLI authentication".to_owned(),
                    "run gh auth login if gh is not authenticated".to_owned(),
                    format!("verify the repository slug {repository} is correct and reachable"),
                ],
                json!({
                    "reason_code": "gh_command_failed",
                    "adapter_error": err.to_string(),
                    "repository": repository,
                }),
            );
        }
        Err(err) => {
            return error_response(format!(
                "failed to list open pull requests for {repository}: {err}"
            ));
        }
    };

    let mut items = Vec::with_capacity(open_prs.len());
    for pr in &open_prs {
        let sessions = match store.session_finder(SessionFinderQuery {
            repository: Some(repository.clone()),
            pull_request_number: Some(pr.number),
            attention_states: Vec::new(),
            limit: 1,
        }) {
            Ok(sessions) => sessions,
            Err(err) => {
                return error_response(format!(
                    "failed to resolve local session state for PR #{}: {err}",
                    pr.number
                ));
            }
        };

        let (roger_state, session_id) = match sessions.first() {
            Some(entry) => {
                let overview = match store.session_overview(&entry.session_id) {
                    Ok(overview) => overview,
                    Err(err) => {
                        return error_response(format!(
                            "failed to load session overview for {}: {err}",
                            entry.session_id
                        ));
                    }
                };
                (
                    derive_prs_queue_state(
                        &entry.attention_state,
                        overview.draft_count,
                        overview.posted_action_count,
                    ),
                    Some(entry.session_id.clone()),
                )
            }
            None => ("not_started".to_owned(), None),
        };

        let next_command = prs_queue_next_command(&roger_state, pr.number);
        items.push(json!({
            "pr_number": pr.number,
            "title": pr.title,
            "author": pr.author,
            "is_draft": pr.is_draft,
            "head_ref": pr.head_ref,
            "updated_at": pr.updated_at,
            "url": pr.url,
            "roger_state": roger_state,
            "session_id": session_id,
            "next_command": next_command,
        }));
    }

    let count = items.len();
    let outcome = if count == 0 {
        OutcomeKind::Empty
    } else {
        OutcomeKind::Complete
    };
    let message = if count == 0 {
        format!("no open pull requests found for {repository}")
    } else {
        format!("loaded {count} open pull requests for {repository}")
    };

    CommandResponse {
        outcome,
        data: json!({
            "repository": repository,
            "items": items,
            "count": count,
            "filters_applied": {
                "repository": repository,
                "limit": limit,
            }
        }),
        warnings: Vec::new(),
        repair_actions: Vec::new(),
        message,
    }
}

fn render_prs_table(data: &Value) -> String {
    let Some(items) = data.get("items").and_then(Value::as_array) else {
        return String::new();
    };
    if items.is_empty() {
        return String::new();
    }

    let header = ["PR", "STATE", "DRAFT", "AUTHOR", "UPDATED", "TITLE", "NEXT"];
    let mut rows = Vec::with_capacity(items.len());
    for item in items {
        let text = |key: &str| {
            item.get(key)
                .and_then(Value::as_str)
                .unwrap_or("-")
                .to_owned()
        };
        rows.push([
            item.get("pr_number")
                .and_then(Value::as_u64)
                .map(|number| format!("#{number}"))
                .unwrap_or_else(|| "-".to_owned()),
            text("roger_state"),
            if item.get("is_draft").and_then(Value::as_bool) == Some(true) {
                "yes".to_owned()
            } else {
                "no".to_owned()
            },
            text("author"),
            text("updated_at"),
            text("title"),
            text("next_command"),
        ]);
    }

    let mut widths = header.map(str::len);
    for row in &rows {
        for (index, cell) in row.iter().enumerate() {
            widths[index] = widths[index].max(cell.len());
        }
    }

    let render_row = |cells: &[String]| {
        let mut line = String::new();
        for (index, cell) in cells.iter().enumerate() {
            if index > 0 {
                line.push_str("  ");
            }
            if index + 1 == cells.len() {
                line.push_str(cell);
            } else {
                line.push_str(&format!("{cell:<width$}", width = widths[index]));
            }
        }
        line.push('\n');
        line
    };

    let mut table = render_row(&header.map(str::to_owned));
    for row in &rows {
        table.push_str(&render_row(row.as_slice()));
    }
    table
}

/// `rr assets` manages the local semantic-asset surface that flips search into
/// hybrid retrieval. The semantic lane is strictly additive: lexical
/// canonical-DB recall is always on, so every assets subcommand reports posture
/// honestly and never downgrades a healthy install to a degraded outcome.
fn handle_assets(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    let Some(subcommand) = parsed.assets_command else {
        return error_response("rr assets missing subcommand".to_owned());
    };

    match subcommand {
        AssetsCommandKind::Status => handle_assets_status(runtime),
        AssetsCommandKind::Verify => handle_assets_verify(runtime),
        AssetsCommandKind::Install => handle_assets_install(parsed, runtime),
    }
}

/// Reports the full semantic posture: manifest verification, embedder backend,
/// operational gate, and whether the semantic index sidecar is ready. Always
/// resolves to a healthy outcome (Complete/Empty) because the lexical fallback
/// keeps search exit-0 regardless of semantic availability.
fn handle_assets_status(runtime: &CliRuntime) -> CommandResponse {
    let store = match open_store_or_response(runtime, "rr assets status") {
        Ok(store) => store,
        Err(response) => return response,
    };

    let component_state = match store.semantic_component_state() {
        Ok(state) => state,
        Err(err) => {
            return error_response(format!("failed to read semantic component state: {err}"));
        }
    };
    let verification = match store.verify_semantic_asset_manifest() {
        Ok(verification) => verification,
        Err(err) => {
            return error_response(format!("failed to verify semantic asset manifest: {err}"));
        }
    };
    let embedder_status = semantic_embedder_status();
    let index_ready = matches!(
        store.index_state("semantic:assets"),
        Ok(Some(state)) if state.status == "ready"
    );

    let manifest_value = verification
        .manifest
        .as_ref()
        .map(semantic_manifest_to_json)
        .unwrap_or(Value::Null);

    let data = json!({
        "subcommand": "status",
        "manifest_present": verification.manifest.is_some(),
        "manifest_verified": verification.verified,
        "manifest": manifest_value,
        "embedder": {
            "available": embedder_status.available,
            "compiled": roger_storage::semantic_embedder_compiled(),
            "backend": embedder_status.backend,
            "reason": embedder_status.reason,
        },
        "index_ready": index_ready,
        "operational": component_state.operational,
        "posture": semantic_posture_label(&component_state),
        "degraded_reasons": component_state.degraded_reasons,
    });

    let outcome = if verification.manifest.is_some() {
        OutcomeKind::Complete
    } else {
        OutcomeKind::Empty
    };

    CommandResponse {
        outcome,
        data,
        warnings: Vec::new(),
        repair_actions: if component_state.operational {
            Vec::new()
        } else {
            vec![
                "run rr assets install --asset semantic-default to install the verified model package"
                    .to_owned(),
            ]
        },
        message: format!(
            "semantic assets {} (embedder {})",
            semantic_posture_label(&component_state),
            embedder_status
                .backend
                .clone()
                .unwrap_or_else(|| "disabled".to_owned())
        ),
    }
}

/// Verifies the installed semantic asset manifest against its on-disk artifact
/// digest. Complete when verified, Empty when no manifest is installed, and
/// RepairNeeded (exit 4) on a digest mismatch or missing artifact.
fn handle_assets_verify(runtime: &CliRuntime) -> CommandResponse {
    let store = match open_store_or_response(runtime, "rr assets verify") {
        Ok(store) => store,
        Err(response) => return response,
    };

    let verification = match store.verify_semantic_asset_manifest() {
        Ok(verification) => verification,
        Err(err) => {
            return error_response(format!("failed to verify semantic asset manifest: {err}"));
        }
    };

    let manifest_value = verification
        .manifest
        .as_ref()
        .map(semantic_manifest_to_json)
        .unwrap_or(Value::Null);

    if verification.verified {
        return CommandResponse {
            outcome: OutcomeKind::Complete,
            data: json!({
                "subcommand": "verify",
                "verified": true,
                "manifest": manifest_value,
            }),
            warnings: Vec::new(),
            repair_actions: Vec::new(),
            message: "semantic assets verified against installed manifest digest".to_owned(),
        };
    }

    if verification.manifest.is_none() {
        return CommandResponse {
            outcome: OutcomeKind::Empty,
            data: json!({
                "subcommand": "verify",
                "verified": false,
                "manifest": Value::Null,
                "reason": verification.reason,
            }),
            warnings: Vec::new(),
            repair_actions: vec![
                "run rr assets install --asset semantic-default to install the verified model package"
                    .to_owned(),
            ],
            message: "no semantic asset manifest is installed".to_owned(),
        };
    }

    CommandResponse {
        outcome: OutcomeKind::RepairNeeded,
        data: json!({
            "subcommand": "verify",
            "verified": false,
            "manifest": manifest_value,
            "reason": verification.reason,
            "reason_code": "semantic_asset_digest_mismatch",
        }),
        warnings: Vec::new(),
        repair_actions: vec![
            "re-run rr assets install --asset semantic-default to repair the corrupted/missing artifact"
                .to_owned(),
        ],
        message: verification
            .reason
            .unwrap_or_else(|| "semantic asset verification failed".to_owned()),
    }
}

/// Installs the real semantic model the FastEmbed embedder loads at search
/// time.
///
/// When the `semantic-fastembed` feature is compiled in, this constructs the
/// real embedder against the store's `semantic_asset_root()` — triggering
/// fastembed's HuggingFace download of the model into that directory — embeds a
/// probe string to confirm the model loads and runs inference, then records an
/// `active_manifest.json` whose digest covers the exact on-disk model tree the
/// embedder just loaded. `rr assets verify` re-digests that same tree, so the
/// install gate and the embedder always look at the same bytes.
///
/// When the feature is NOT compiled, this fails closed honestly: there is no
/// embedder to load the model, so installing one would be dishonest. The build
/// must be recompiled with `--features semantic-fastembed` first.
fn handle_assets_install(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    let asset_id = parsed
        .assets_package_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("semantic-default");
    if asset_id != "semantic-default" {
        return blocked_response(
            format!("unknown semantic asset package: {asset_id}"),
            vec!["pass --asset semantic-default (the only supported baseline)".to_owned()],
            json!({"subcommand": "install", "reason_code": "unknown_asset_package"}),
        );
    }

    // Fail closed honestly when the embedder is not compiled in: there is no
    // model loader to install for, so we never write an unbacked manifest.
    if !roger_storage::semantic_embedder_compiled() {
        return blocked_response(
            "rr assets install requires the FastEmbed embedder, which is not compiled into this build"
                .to_owned(),
            vec![
                "compile rr with --features semantic-fastembed to install the semantic model"
                    .to_owned(),
            ],
            json!({
                "subcommand": "install",
                "reason_code": "semantic_feature_not_compiled",
            }),
        );
    }

    let store = match open_store_or_response(runtime, "rr assets install") {
        Ok(store) => store,
        Err(response) => return response,
    };
    let asset_root = store.layout().semantic_asset_root();
    if let Err(err) = fs::create_dir_all(&asset_root) {
        return error_response(format!(
            "failed to create semantic asset directory {}: {err}",
            asset_root.display()
        ));
    }

    // Construct the real embedder against the asset root. With the `hf-hub`
    // feature, fastembed downloads the model into `asset_root` on first use and
    // the probe proves it loads + runs inference. Any network/load failure is a
    // fail-closed blocked outcome — we never record an unverifiable model.
    let model = roger_storage::FastEmbedModel::default();
    let probe =
        match roger_storage::probe_semantic_embedder(roger_storage::FastEmbedAdapterConfig {
            model,
            cache_dir: asset_root.clone(),
            show_download_progress: false,
        }) {
            Ok(probe) => probe,
            Err(err) => {
                return blocked_response(
                    format!("failed to download or load the semantic model: {err}"),
                    vec![
                        "confirm network access to huggingface.co and retry rr assets install"
                            .to_owned(),
                    ],
                    json!({
                        "subcommand": "install",
                        "reason_code": "semantic_model_download_or_load_failed",
                        "model_id": model.model_id(),
                    }),
                );
            }
        };

    // Digest the exact on-disk model tree the embedder loaded. This is the
    // stable, re-verifiable proof recorded in the manifest; `rr assets verify`
    // re-computes it over the same files.
    let artifact_digest = match roger_storage::semantic_model_tree_digest(&asset_root) {
        Ok(digest) => digest,
        Err(err) => {
            return error_response(format!(
                "failed to digest downloaded semantic model tree at {}: {err}",
                asset_root.display()
            ));
        }
    };

    let manifest = SemanticAssetManifest {
        schema_version: 1,
        package_id: asset_id.to_owned(),
        // The revision identifies the model descriptor the embedder loaded; the
        // tree digest below is the authoritative re-verifiable artifact proof.
        revision: probe.model_id.clone(),
        // The model lives directly under the asset root (the fastembed cache),
        // so the artifact path is the asset root itself ("."): verification
        // digests the whole model tree.
        artifact_rel_path: ".".to_owned(),
        artifact_digest,
        installed_at: time::now_ts(),
    };
    if let Err(err) = store.install_semantic_asset_manifest(&manifest) {
        return error_response(format!("failed to write semantic asset manifest: {err}"));
    }

    // Re-verify fail-closed: the manifest we just wrote must verify clean
    // against the same model tree the embedder loaded.
    match store.verify_semantic_asset_manifest() {
        Ok(verification) if verification.verified => {}
        Ok(verification) => {
            return CommandResponse {
                outcome: OutcomeKind::RepairNeeded,
                data: json!({
                    "subcommand": "install",
                    "reason_code": "post_install_verification_failed",
                    "reason": verification.reason,
                }),
                warnings: Vec::new(),
                repair_actions: vec!["re-run rr assets install to repair the manifest".to_owned()],
                message: "semantic asset install did not verify post-write".to_owned(),
            };
        }
        Err(err) => {
            return error_response(format!(
                "failed to verify semantic asset manifest after install: {err}"
            ));
        }
    }

    // Mark the semantic index ready for every repo scope at install time: the
    // in-process embedder *is* the semantic index for this design (candidates
    // are embedded live from the canonical corpus), so a verified+probed model
    // means the semantic lane is generation-ready.
    if let Err(err) = store.upsert_index_state(roger_storage::UpdateIndexState {
        scope_key: "semantic:assets",
        generation: manifest.installed_at,
        status: "ready",
        artifact_digest: Some(manifest.artifact_digest.as_str()),
    }) {
        return error_response(format!("failed to record semantic index state: {err}"));
    }

    CommandResponse {
        outcome: OutcomeKind::Complete,
        data: json!({
            "subcommand": "install",
            "asset": asset_id,
            "model_id": probe.model_id,
            "embedding_dimension": probe.dimension,
            "backend": probe.backend,
            "manifest": semantic_manifest_to_json(&manifest),
        }),
        warnings: Vec::new(),
        repair_actions: Vec::new(),
        message: format!(
            "semantic model {} installed, probed, verified, and active",
            probe.model_id
        ),
    }
}

/// Build the semantic embedder used for live search candidate generation.
///
/// When the `semantic-fastembed` feature is compiled in, this loads the real
/// FastEmbed adapter against the installed model cache. When the feature is off
/// (the default 0.2 build), it returns the Unavailable adapter, whose
/// `state().is_ready()` is false, so `generate_semantic_candidates` returns an
/// empty vector and search stays on the always-on lexical fallback.
fn build_search_semantic_embedder(store: &RogerStore) -> SemanticEmbedderAdapter {
    let status = semantic_embedder_status();
    if !status.available {
        return SemanticEmbedderAdapter::default_for_runtime();
    }

    let cache_dir = store.layout().semantic_asset_root();
    let config = roger_storage::FastEmbedAdapterConfig {
        model: roger_storage::FastEmbedModel::default(),
        cache_dir,
        show_download_progress: false,
    };
    match SemanticEmbedderAdapter::try_fastembed(config) {
        Ok(adapter) => adapter,
        Err(err) => SemanticEmbedderAdapter::unavailable(format!(
            "FastEmbed adapter could not load installed semantic model: {err}"
        )),
    }
}

fn semantic_manifest_to_json(manifest: &SemanticAssetManifest) -> Value {
    json!({
        "schema_version": manifest.schema_version,
        "package_id": manifest.package_id,
        "revision": manifest.revision,
        "artifact_rel_path": manifest.artifact_rel_path,
        "artifact_digest": manifest.artifact_digest,
        "installed_at": manifest.installed_at,
    })
}

fn semantic_posture_label(state: &SemanticComponentState) -> &'static str {
    if state.operational {
        // Compiled embedder + verified+probed model: hybrid search is live.
        "operational"
    } else if state.embedder_available {
        // Embedder compiled in but the model is not installed/verified yet.
        "compiled_pending_install"
    } else {
        // Feature not compiled: semantic lane is honestly disabled.
        "disabled_pending_install"
    }
}

fn handle_search(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    let Some(query_text) = parsed
        .query_text
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
    else {
        return blocked_response(
            "rr search requires --query <text>".to_owned(),
            vec!["pass --query \"<search text>\"".to_owned()],
            json!({"reason_code": "query_required"}),
        );
    };

    let Some(repository) = resolve_repository(parsed.repo.clone(), &runtime.cwd) else {
        return blocked_response(
            "repo context inference failed; search scope is ambiguous".to_owned(),
            vec!["pass --repo owner/repo or configure git remote.origin.url".to_owned()],
            json!({"reason_code": "repo_context_missing"}),
        );
    };

    let store = match open_store_or_response(runtime, "rr search") {
        Ok(store) => store,
        Err(response) => return response,
    };

    let limit = parsed.limit.unwrap_or(10).min(100);
    let granted_scopes = vec!["repo".to_owned()];

    // Real semantic verification: assets must verify clean AND the embedder must
    // be operational before we let search claim semantic readiness. This gates
    // both the plan's semantic posture and the lookup's hybrid eligibility, so a
    // stub embedder or unverified asset can never fake a hybrid run.
    let component_state = store.semantic_component_state().ok();
    let embedder_operational = component_state
        .as_ref()
        .map(|state| state.embedder_available)
        .unwrap_or(false);
    let assets_verified = component_state
        .as_ref()
        .map(|state| state.assets_verified)
        .unwrap_or(false);
    let semantic_assets_verified = assets_verified && embedder_operational;

    // When the semantic lane is fully operational (verified+probed model and a
    // compiled embedder), mark this repo scope's semantic index ready so the
    // in-process lookup's `semantic_index_ready` gate flips to hybrid. The
    // embedder is the index for this design — candidates are embedded live from
    // the canonical corpus — so an operational lane is generation-ready.
    let scope_index_key = format!("semantic:repo:{repository}");
    if semantic_assets_verified {
        let digest = component_state
            .as_ref()
            .and_then(|_| store.semantic_asset_manifest().ok().flatten())
            .map(|manifest| manifest.artifact_digest);
        let _ = store.upsert_index_state(roger_storage::UpdateIndexState {
            scope_key: &scope_index_key,
            generation: time::now_ts(),
            status: "ready",
            artifact_digest: digest.as_deref(),
        });
    }

    let search_plan = match materialize_search_plan(SearchPlanInput {
        review_session_id: None,
        review_run_id: None,
        repository: &repository,
        granted_scopes: &granted_scopes,
        query_text,
        query_mode: parsed.query_mode.as_deref(),
        requested_retrieval_classes: &[],
        anchor_hints: &[],
        supports_candidate_audit: true,
        // `rr search` (operator + robot) supports promotion_review as a
        // read-only listing surface for pending MemoryReviewRequest rows. The
        // worker `search_memory` path keeps promotion_review unsupported so
        // workers cannot drive promotion review.
        supports_promotion_review: true,
        semantic_assets_verified,
    }) {
        Ok(plan) => plan,
        Err(err) => {
            let repair_actions = match &err {
                SearchPlanError::QueryPlanning(SearchQueryPlanError::MissingSearchInputs) => {
                    vec!["pass --query \"<search text>\"".to_owned()]
                }
                SearchPlanError::QueryPlanning(SearchQueryPlanError::UnsupportedQueryMode { .. }) => vec![
                    "pass --query-mode auto, exact_lookup, recall, related_context, or candidate_audit".to_owned(),
                ],
                SearchPlanError::QueryPlanning(
                    SearchQueryPlanError::RelatedContextRequiresAnchors,
                ) => vec![
                    "omit --query-mode to let Roger resolve auto for this entrypoint".to_owned(),
                    "or use --query-mode recall, exact_lookup, or candidate_audit on rr search"
                        .to_owned(),
                ],
                SearchPlanError::QueryPlanning(SearchQueryPlanError::CandidateAuditUnsupported) => {
                    vec!["retry on a surface that supports candidate inspection".to_owned()]
                }
                SearchPlanError::QueryPlanning(
                    SearchQueryPlanError::PromotionReviewUnsupported,
                ) => vec![
                    "rr search does not support promotion_review in this slice; use candidate_audit or recall instead".to_owned(),
                ],
                SearchPlanError::MissingGrantedScopes | SearchPlanError::UnsupportedScope { .. } => vec![
                    "rr search currently executes with repo-only scope; retry with --repo owner/repo".to_owned(),
                ],
                SearchPlanError::UnsupportedRetrievalClass { .. }
                | SearchPlanError::CandidateAwareQueryRequiresTentativeCandidates { .. }
                | SearchPlanError::TentativeCandidatesRequireCandidateAwareQuery { .. } => vec![
                    "this surface resolves retrieval lanes automatically; retry without overriding the worker retrieval contract".to_owned(),
                ],
            };
            return blocked_response(
                err.to_string(),
                repair_actions,
                json!({
                    "reason_code": err.reason_code(),
                    "requested_query_mode": parsed.query_mode.as_deref().unwrap_or("auto"),
                }),
            );
        }
    };
    let scope_key = format!("repo:{repository}");

    // Only attempt real semantic candidate generation when verification says the
    // semantic lane is fully operational (verified assets + operational
    // embedder). The generator itself re-checks embedder readiness and returns
    // an empty vector otherwise, so this can never fabricate a hybrid run.
    let semantic_candidates = if semantic_assets_verified && search_plan.retrieval_strategy.semantic
    {
        let mut embedder = build_search_semantic_embedder(&store);
        store
            .generate_semantic_candidates(
                &scope_key,
                &repository,
                query_text,
                &mut embedder,
                limit.saturating_add(1),
            )
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let lookup = match store.prior_review_lookup(PriorReviewLookupQuery {
        scope_key: &scope_key,
        repository: &repository,
        query_text,
        limit: limit.saturating_add(1),
        include_tentative_candidates: search_plan.includes_tentative_candidates(),
        allow_project_scope: false,
        allow_org_scope: false,
        semantic_assets_verified,
        semantic_candidates,
    }) {
        Ok(result) => result,
        Err(err) => return error_response(format!("failed to run prior-review lookup: {err}")),
    };

    let lane_counts = json!({
        "evidence_hits": if search_plan.allows_retrieval_class(SearchRetrievalClass::EvidenceHits) {
            lookup.evidence_hits.len()
        } else {
            0
        },
        "promoted_memory": if search_plan.allows_retrieval_class(SearchRetrievalClass::PromotedMemory) {
            lookup.promoted_memory.len()
        } else {
            0
        },
        "tentative_candidates": if search_plan.includes_tentative_candidates() {
            lookup.tentative_candidates.len()
        } else {
            0
        },
    });
    let scope_bucket = lookup.scope_bucket.clone();
    let degraded_reasons = lookup.degraded_reasons.clone();
    let mut items = Vec::new();
    let retrieval_mode = retrieval_mode_label(&lookup.mode).to_owned();
    if search_plan.allows_retrieval_class(SearchRetrievalClass::EvidenceHits) {
        for hit in lookup.evidence_hits {
            let recall = worker_recall_from_evidence_hit(
                &hit,
                search_plan.query_plan.requested_query_mode.as_str(),
                search_plan.query_plan.resolved_query_mode.as_str(),
                &retrieval_mode,
                &scope_bucket,
                &degraded_reasons,
                &[],
            );
            items.push(search_item_from_recall_envelope(
                &recall,
                &hit.title,
                hit.fused_score,
            ));
        }
    }
    if search_plan.allows_retrieval_class(SearchRetrievalClass::PromotedMemory) {
        for hit in lookup.promoted_memory {
            let recall = worker_recall_from_memory_hit(
                &hit,
                search_plan.query_plan.requested_query_mode.as_str(),
                search_plan.query_plan.resolved_query_mode.as_str(),
                &retrieval_mode,
                &scope_bucket,
                &degraded_reasons,
                "promoted_memory",
                &[],
            );
            items.push(search_item_from_recall_envelope(
                &recall,
                &hit.statement,
                hit.fused_score,
            ));
        }
    }
    if search_plan.includes_tentative_candidates() {
        for hit in lookup.tentative_candidates {
            let recall = worker_recall_from_memory_hit(
                &hit,
                search_plan.query_plan.requested_query_mode.as_str(),
                search_plan.query_plan.resolved_query_mode.as_str(),
                &retrieval_mode,
                &scope_bucket,
                &degraded_reasons,
                "tentative_candidates",
                &[],
            );
            items.push(search_item_from_recall_envelope(
                &recall,
                &hit.statement,
                hit.fused_score,
            ));
        }
    }

    items.sort_by(|left, right| {
        let left_score = left
            .get("score")
            .and_then(Value::as_i64)
            .unwrap_or_default();
        let right_score = right
            .get("score")
            .and_then(Value::as_i64)
            .unwrap_or_default();
        right_score.cmp(&left_score)
    });

    let truncated = items.len() > limit;
    if truncated {
        items.truncate(limit);
    }

    let mode = retrieval_mode;
    let count = items.len();

    // promotion_review is a read-only listing surface for pending
    // MemoryReviewRequest rows. The count is always surfaced (posture); the full
    // list is projected only when the operator asked for promotion_review.
    let pending_review_count = store
        .count_pending_memory_review_requests(Some(&scope_key))
        .unwrap_or(0);
    let is_promotion_review = search_plan.query_plan.resolved_query_mode.as_str()
        == roger_app_core::SearchQueryMode::PromotionReview.as_str();
    let pending_review_requests: Vec<Value> = if is_promotion_review {
        store
            .pending_memory_review_requests(Some(&scope_key), limit)
            .unwrap_or_default()
            .into_iter()
            .map(memory_review_request_to_json)
            .collect()
    } else {
        Vec::new()
    };

    // Hybrid, lexical_only, and recovery_scan are all HEALTHY retrieval modes:
    // the lexical canonical-DB scan is the always-on exit-0 default, and a stale
    // sidecar recovery scan still returns real results. A true lexical-scan
    // error is surfaced earlier via `prior_review_lookup` returning `Err`
    // (mapped to an Error outcome), so we never reach here in that case.
    // Therefore: results -> Complete (exit 0), zero results -> Empty (exit 0).
    // Degraded is no longer emitted for default installs.
    let outcome = if count == 0 && pending_review_requests.is_empty() {
        OutcomeKind::Empty
    } else {
        OutcomeKind::Complete
    };

    // Surface semantic/sidecar shortfalls as a structured fallback block plus
    // warnings, not as a degraded verdict. `semantic_available` reflects the
    // real operational gate (verified assets + operational embedder + hybrid
    // mode actually engaged).
    let semantic_available = semantic_assets_verified && mode == "hybrid";
    let (fallback_lane, reason_code, advice) = if semantic_available {
        ("hybrid", "semantic_operational", Value::Null)
    } else if !assets_verified {
        (
            mode.as_str(),
            "semantic_assets_unverified",
            json!(
                "run rr assets install --asset semantic-default to enable hybrid semantic retrieval"
            ),
        )
    } else if !embedder_operational {
        (
            mode.as_str(),
            "semantic_embedder_unavailable",
            json!("rebuild with the semantic-fastembed feature to enable the local embedder"),
        )
    } else {
        (
            mode.as_str(),
            "semantic_sidecar_or_candidates_unavailable",
            json!(
                "semantic assets verified but the semantic index/candidates are not yet ready; lexical fallback is serving results"
            ),
        )
    };
    let fallback = json!({
        "lane": fallback_lane,
        "semantic_available": semantic_available,
        "reason_code": reason_code,
        "advice": advice,
    });

    let mut warnings = Vec::new();
    if !semantic_available {
        warnings.push(format!(
            "semantic retrieval inactive ({reason_code}); served via {mode} lexical fallback"
        ));
    }

    CommandResponse {
        outcome,
        data: json!({
            "query": query_text,
            "requested_query_mode": search_plan.query_plan.requested_query_mode.as_str(),
            "resolved_query_mode": search_plan.query_plan.resolved_query_mode.as_str(),
            "search_plan": search_plan.clone(),
            "retrieval_mode": mode,
            "mode": mode,
            "scope_key": scope_key,
            "candidate_included": search_plan.includes_tentative_candidates(),
            "allow_project_scope": false,
            "allow_org_scope": false,
            "items": items,
            "count": count,
            "truncated": truncated,
            "degraded_reasons": degraded_reasons,
            "fallback": fallback,
            "scope_bucket": scope_bucket,
            "lane_counts": lane_counts,
            "pending_review_count": pending_review_count,
            "pending_review_requests": pending_review_requests,
        }),
        warnings,
        repair_actions: Vec::new(),
        message: format!(
            "search completed with query_mode {} and retrieval_mode {mode}",
            search_plan.query_plan.resolved_query_mode.as_str()
        ),
    }
}

fn normalize_calver_version(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim().trim_start_matches('v');
    if trimmed.is_empty() {
        return Err("version value is empty".to_owned());
    }

    let (date_part, rc_part) = if let Some((lhs, rhs)) = trimmed.split_once("-rc.") {
        (lhs, Some(rhs))
    } else {
        (trimmed, None)
    };

    let mut date_parts = date_part.split('.');
    let Some(year) = date_parts.next() else {
        return Err("version must match YYYY.MM.DD or YYYY.MM.DD-rc.N".to_owned());
    };
    let Some(month) = date_parts.next() else {
        return Err("version must match YYYY.MM.DD or YYYY.MM.DD-rc.N".to_owned());
    };
    let Some(day) = date_parts.next() else {
        return Err("version must match YYYY.MM.DD or YYYY.MM.DD-rc.N".to_owned());
    };
    if date_parts.next().is_some() {
        return Err("version must match YYYY.MM.DD or YYYY.MM.DD-rc.N".to_owned());
    }
    if year.len() != 4
        || month.len() != 2
        || day.len() != 2
        || !year.chars().all(|ch| ch.is_ascii_digit())
        || !month.chars().all(|ch| ch.is_ascii_digit())
        || !day.chars().all(|ch| ch.is_ascii_digit())
    {
        return Err("version must match YYYY.MM.DD or YYYY.MM.DD-rc.N".to_owned());
    }
    if let Some(rc) = rc_part {
        if rc.is_empty()
            || !rc.chars().all(|ch| ch.is_ascii_digit())
            || rc.parse::<u32>().ok().unwrap_or(0) == 0
        {
            return Err("rc version must use -rc.N with N >= 1".to_owned());
        }
    }

    Ok(trimmed.to_owned())
}

fn fetch_url_with_curl(url: &str) -> Result<String, String> {
    let output = ProcessCommand::new("curl")
        .args(["-fsSL", url])
        .output()
        .map_err(|err| format!("failed to execute curl for {url}: {err}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(format!(
            "curl failed for {url} (status {}): {}",
            output.status,
            if stderr.is_empty() {
                "no stderr output".to_owned()
            } else {
                stderr
            }
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ChecksumsManifestFetch {
    text: String,
    url: String,
    legacy_fallback_used: bool,
    attempted_urls: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ChecksumsManifestFetchError {
    attempted_urls: Vec<String>,
    message: String,
}

fn fetch_checksums_manifest_with_fallback(
    download_root: &str,
    target_tag: &str,
    declared_checksums_name: &str,
) -> Result<ChecksumsManifestFetch, ChecksumsManifestFetchError> {
    let declared_url = format!("{download_root}/{target_tag}/{declared_checksums_name}");
    let mut attempted_urls = vec![declared_url.clone()];
    match fetch_url_with_curl(&declared_url) {
        Ok(text) => {
            return Ok(ChecksumsManifestFetch {
                text,
                url: declared_url,
                legacy_fallback_used: false,
                attempted_urls,
            });
        }
        Err(primary_err) => {
            if declared_checksums_name == "SHA256SUMS" {
                return Err(ChecksumsManifestFetchError {
                    attempted_urls,
                    message: format!("failed to fetch checksums: {primary_err}"),
                });
            }

            let fallback_url = format!("{download_root}/{target_tag}/SHA256SUMS");
            attempted_urls.push(fallback_url.clone());
            match fetch_url_with_curl(&fallback_url) {
                Ok(text) => Ok(ChecksumsManifestFetch {
                    text,
                    url: fallback_url,
                    legacy_fallback_used: true,
                    attempted_urls,
                }),
                Err(fallback_err) => Err(ChecksumsManifestFetchError {
                    attempted_urls,
                    message: format!(
                        "failed to fetch checksums: {primary_err}; fallback SHA256SUMS also failed: {fallback_err}"
                    ),
                }),
            }
        }
    }
}

fn release_hosted_reinstall_command(
    repo: &str,
    target_version: Option<&str>,
    target_tag: Option<&str>,
    channel: Option<&str>,
) -> String {
    if cfg!(target_os = "windows") {
        let base = if let Some(tag) = target_tag {
            format!("https://github.com/{repo}/releases/download/{tag}/rr-install.ps1")
        } else {
            format!("https://github.com/{repo}/releases/latest/download/rr-install.ps1")
        };
        let mut args = Vec::new();
        if let Some(channel) = channel.filter(|value| !value.is_empty() && *value != "stable") {
            args.push(format!("-Channel {channel}"));
        }
        if let Some(version) = target_version {
            args.push(format!("-Version {version}"));
        }
        args.push(format!("-Repo {repo}"));
        let arg_string = args.join(" ");
        return format!(
            "powershell -ExecutionPolicy Bypass -Command \"& {{ $tmp = Join-Path $env:TEMP 'rr-install.ps1'; Invoke-WebRequest '{base}' -OutFile $tmp; & $tmp {arg_string} }}\""
        );
    }

    let base = if let Some(tag) = target_tag {
        format!("https://github.com/{repo}/releases/download/{tag}/rr-install.sh")
    } else {
        format!("https://github.com/{repo}/releases/latest/download/rr-install.sh")
    };
    let mut args = Vec::new();
    if let Some(channel) = channel.filter(|value| !value.is_empty() && *value != "stable") {
        args.push(format!("--channel {channel}"));
    }
    if let Some(version) = target_version {
        args.push(format!("--version {version}"));
    }
    args.push(format!("--repo {repo}"));
    format!("curl -fsSL {base} | bash -s -- {}", args.join(" "))
}

/// The canonical release-hosted installer one-liner. Blocked update envelopes
/// must carry this as a repair action so a user whose in-place update is fenced
/// off by migration posture always has a copy-pasteable recovery path that works
/// from an installed-binary context (no repo checkout required). Repo-aware, but
/// resolves to the documented `cdilga/roger-reviewer` command for the default.
fn release_latest_installer_one_liner(repo: &str) -> String {
    format!("curl -fsSL https://github.com/{repo}/releases/latest/download/rr-install.sh | bash")
}

fn resolve_latest_release_tag(api_root: &str, channel: &str) -> Result<String, String> {
    if channel == "stable" {
        let payload = fetch_url_with_curl(&format!("{api_root}/releases/latest"))?;
        let json: Value = serde_json::from_str(&payload)
            .map_err(|err| format!("invalid latest release payload: {err}"))?;
        let Some(tag) = json.get("tag_name").and_then(Value::as_str) else {
            return Err("latest release payload missing tag_name".to_owned());
        };
        return Ok(tag.to_owned());
    }

    let payload = fetch_url_with_curl(&format!("{api_root}/releases?per_page=30"))?;
    let json: Value =
        serde_json::from_str(&payload).map_err(|err| format!("invalid releases payload: {err}"))?;
    let Some(entries) = json.as_array() else {
        return Err("releases payload must be an array".to_owned());
    };
    for entry in entries {
        let prerelease = entry
            .get("prerelease")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let tag = entry
            .get("tag_name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if prerelease && tag.contains("-rc.") {
            return Ok(tag.to_owned());
        }
    }
    Err("no rc prerelease found in release feed".to_owned())
}

fn detect_update_target(target_override: Option<&String>) -> Result<String, String> {
    if let Some(target) = target_override {
        if target.trim().is_empty() {
            return Err("--target cannot be empty".to_owned());
        }
        return Ok(target.clone());
    }

    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Ok("aarch64-apple-darwin".to_owned()),
        ("macos", "x86_64") => Ok("x86_64-apple-darwin".to_owned()),
        ("linux", "x86_64") => Ok("x86_64-unknown-linux-gnu".to_owned()),
        ("linux", "aarch64") => Ok("aarch64-unknown-linux-gnu".to_owned()),
        ("windows", "x86_64") => Ok("x86_64-pc-windows-msvc".to_owned()),
        ("windows", "aarch64") => Ok("aarch64-pc-windows-msvc".to_owned()),
        (os, arch) => Err(format!(
            "unsupported host platform for rr update: {os}/{arch}; pass --target explicitly"
        )),
    }
}

fn checksums_entry_for_archive(checksums_text: &str, archive_name: &str) -> Result<String, String> {
    let mut matches = Vec::new();
    for line in checksums_text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }
        let candidate_name = parts[parts.len() - 1].trim_start_matches('*');
        let candidate_basename = candidate_name
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(candidate_name);
        if candidate_name == archive_name || candidate_basename == archive_name {
            matches.push(parts[0].to_ascii_lowercase());
        }
    }
    if matches.is_empty() {
        return Err(format!(
            "checksums file missing entry for archive {archive_name}"
        ));
    }
    if matches.len() > 1 {
        return Err(format!(
            "checksums file has ambiguous entries for archive {archive_name}"
        ));
    }
    Ok(matches.remove(0))
}

fn download_url_to_path(url: &str, destination: &Path) -> Result<(), String> {
    let output = ProcessCommand::new("curl")
        .arg("-fsSL")
        .arg(url)
        .arg("-o")
        .arg(destination)
        .output()
        .map_err(|err| format!("failed to execute curl for {url}: {err}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(format!(
            "curl failed for {url} (status {}): {}",
            output.status,
            if stderr.is_empty() {
                "no stderr output".to_owned()
            } else {
                stderr
            }
        ));
    }
    Ok(())
}

fn sha256_for_file(path: &Path) -> Result<String, String> {
    let bytes =
        fs::read(path).map_err(|err| format!("failed to read file {}: {err}", path.display()))?;
    Ok(sha256_hex(&bytes))
}

fn extract_targz_archive(archive_path: &Path, destination: &Path) -> Result<(), String> {
    let output = ProcessCommand::new("tar")
        .arg("-xzf")
        .arg(archive_path)
        .arg("-C")
        .arg(destination)
        .output()
        .map_err(|err| {
            format!(
                "failed to execute tar extraction for {}: {err}",
                archive_path.display()
            )
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(format!(
            "tar extraction failed for {} (status {}): {}",
            archive_path.display(),
            output.status,
            if stderr.is_empty() {
                "no stderr output".to_owned()
            } else {
                stderr
            }
        ));
    }
    Ok(())
}

fn extract_zip_archive(archive_path: &Path, destination: &Path) -> Result<(), String> {
    // Prefer unzip; fall back to bsdtar-compatible `tar -xf` (macOS/Windows
    // tar handle zip archives) so installed hosts without unzip still work.
    let unzip_result = ProcessCommand::new("unzip")
        .arg("-q")
        .arg(archive_path)
        .arg("-d")
        .arg(destination)
        .output();
    let unzip_failure = match unzip_result {
        Ok(output) if output.status.success() => return Ok(()),
        Ok(output) => format!(
            "unzip failed for {} (status {}): {}",
            archive_path.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ),
        Err(err) => format!(
            "failed to execute unzip for {}: {err}",
            archive_path.display()
        ),
    };

    let tar_failure = {
        let output = ProcessCommand::new("tar")
            .arg("-xf")
            .arg(archive_path)
            .arg("-C")
            .arg(destination)
            .output();
        match output {
            Ok(output) if output.status.success() => return Ok(()),
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
                format!(
                    "fallback tar extraction failed (status {}): {}",
                    output.status,
                    if stderr.is_empty() {
                        "no stderr output".to_owned()
                    } else {
                        stderr
                    }
                )
            }
            Err(err) => format!("fallback tar extraction failed to execute: {err}"),
        }
    };

    // Minimal Linux hosts (e.g. containers) often ship python3 but neither
    // unzip nor a zip-capable tar; python's stdlib zipfile is the last
    // dependency-free fallback before failing closed with install guidance.
    let python_result = ProcessCommand::new("python3")
        .arg("-c")
        .arg("import sys, zipfile; zipfile.ZipFile(sys.argv[1]).extractall(sys.argv[2])")
        .arg(archive_path)
        .arg(destination)
        .output();
    match python_result {
        Ok(output) if output.status.success() => return Ok(()),
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            Err(format!(
                "{unzip_failure}; {tar_failure}; python3 zipfile fallback failed (status {}): {}; install unzip (or a zip-capable tar) and retry",
                output.status, stderr
            ))
        }
        Err(err) => Err(format!(
            "{unzip_failure}; {tar_failure}; python3 zipfile fallback failed to execute: {err}; install unzip (or a zip-capable tar) and retry",
        )),
    }
}

fn resolve_update_install_path(
    current_exe: &Path,
    expected_binary_name: &str,
) -> Result<PathBuf, String> {
    let metadata = fs::symlink_metadata(current_exe).map_err(|err| {
        format!(
            "failed to inspect current executable {}: {err}",
            current_exe.display()
        )
    })?;
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "unsupported install layout: {} is a symlink; rerun install on a direct binary path",
            current_exe.display()
        ));
    }
    if !metadata.is_file() {
        return Err(format!(
            "unsupported install layout: current executable is not a regular file ({})",
            current_exe.display()
        ));
    }

    let Some(file_name) = current_exe.file_name().and_then(|value| value.to_str()) else {
        return Err(format!(
            "unsupported install layout: executable file name is not UTF-8 ({})",
            current_exe.display()
        ));
    };
    if file_name != expected_binary_name {
        return Err(format!(
            "unsupported install layout: running binary name {file_name} does not match expected release binary {expected_binary_name}"
        ));
    }
    if current_exe.parent().is_none() {
        return Err(format!(
            "unsupported install layout: executable has no parent directory ({})",
            current_exe.display()
        ));
    }
    Ok(current_exe.to_path_buf())
}

fn stage_candidate_binary(
    extract_root: &Path,
    payload_dir: &str,
    binary_name: &str,
    staged_binary_path: &Path,
) -> Result<(), String> {
    let candidate_binary = extract_root.join(payload_dir).join(binary_name);
    if !candidate_binary.is_file() {
        return Err(format!(
            "archive missing expected binary path {}/{}",
            payload_dir, binary_name
        ));
    }

    fs::copy(&candidate_binary, staged_binary_path).map_err(|err| {
        format!(
            "failed to stage candidate binary from {} to {}: {err}",
            candidate_binary.display(),
            staged_binary_path.display()
        )
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(staged_binary_path)
            .map_err(|err| {
                format!(
                    "failed to stat staged candidate binary {}: {err}",
                    staged_binary_path.display()
                )
            })?
            .permissions();
        permissions.set_mode(permissions.mode() | 0o755);
        fs::set_permissions(staged_binary_path, permissions).map_err(|err| {
            format!(
                "failed to make staged candidate executable {}: {err}",
                staged_binary_path.display()
            )
        })?;
    }

    let staged_metadata = fs::metadata(staged_binary_path).map_err(|err| {
        format!(
            "failed to stat staged candidate binary {}: {err}",
            staged_binary_path.display()
        )
    })?;
    if staged_metadata.len() == 0 {
        return Err(format!(
            "staged candidate binary is empty: {}",
            staged_binary_path.display()
        ));
    }

    Ok(())
}

fn apply_binary_replacement_with_rollback(
    install_path: &Path,
    staged_binary_path: &Path,
    backup_suffix: &str,
) -> Result<PathBuf, String> {
    let install_dir = install_path.parent().ok_or_else(|| {
        format!(
            "unsupported install layout: executable has no parent directory ({})",
            install_path.display()
        )
    })?;
    let install_file_name = install_path
        .file_name()
        .ok_or_else(|| format!("invalid install path: {}", install_path.display()))?
        .to_string_lossy()
        .to_string();
    let backup_path = install_dir.join(format!("{install_file_name}.backup-{backup_suffix}"));
    if backup_path.exists() {
        return Err(format!(
            "refusing to apply update because backup path already exists: {}",
            backup_path.display()
        ));
    }

    fs::rename(install_path, &backup_path).map_err(|err| {
        format!(
            "failed to move current binary to backup {}: {err}",
            backup_path.display()
        )
    })?;

    match fs::rename(staged_binary_path, install_path) {
        Ok(_) => {
            let _ = fs::remove_file(&backup_path);
            Ok(backup_path)
        }
        Err(apply_err) => {
            let rollback = fs::rename(&backup_path, install_path);
            if let Err(rollback_err) = rollback {
                return Err(format!(
                    "failed to replace binary ({apply_err}); rollback failed ({rollback_err}); backup left at {}",
                    backup_path.display()
                ));
            }
            Err(format!(
                "failed to replace binary ({apply_err}); rollback restored previous binary"
            ))
        }
    }
}

#[derive(Debug)]
struct UpdateApplyOutcome {
    install_path: PathBuf,
    backup_path: PathBuf,
}

fn apply_update_archive_in_place(
    archive_url: &str,
    archive_name: &str,
    expected_archive_sha256: &str,
    payload_dir: &str,
    binary_name: &str,
    install_path: &Path,
    target_version: &str,
) -> Result<UpdateApplyOutcome, String> {
    let update_tmp_root = std::env::temp_dir().join(format!("rr-update-{}", next_id("apply")));
    let outcome = (|| {
        fs::create_dir_all(&update_tmp_root).map_err(|err| {
            format!(
                "failed to create update staging directory {}: {err}",
                update_tmp_root.display()
            )
        })?;

        let archive_path = update_tmp_root.join(archive_name);
        download_url_to_path(archive_url, &archive_path)?;
        let archive_sha = sha256_for_file(&archive_path)?;
        if archive_sha != expected_archive_sha256.to_ascii_lowercase() {
            return Err(format!(
                "archive checksum mismatch for {archive_name}: expected {}, got {}",
                expected_archive_sha256.to_ascii_lowercase(),
                archive_sha
            ));
        }

        let extract_root = update_tmp_root.join("extract");
        fs::create_dir_all(&extract_root).map_err(|err| {
            format!(
                "failed to create extract directory {}: {err}",
                extract_root.display()
            )
        })?;
        extract_targz_archive(&archive_path, &extract_root)?;

        let staged_binary_path = update_tmp_root.join(format!("{binary_name}.staged"));
        stage_candidate_binary(&extract_root, payload_dir, binary_name, &staged_binary_path)?;

        let backup_suffix = target_version
            .chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
            .collect::<String>();
        let backup_path = apply_binary_replacement_with_rollback(
            install_path,
            &staged_binary_path,
            &backup_suffix,
        )?;

        Ok(UpdateApplyOutcome {
            install_path: install_path.to_path_buf(),
            backup_path,
        })
    })();

    let _ = fs::remove_dir_all(&update_tmp_root);
    outcome
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UpdateConfirmationRequirement {
    NotRequired(&'static str),
    BypassedByYes,
    NeedsPrompt,
    BlockedRobotMode,
    BlockedNonInteractive,
}

fn confirmation_response_is_affirmative(raw: &str) -> bool {
    matches!(raw.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

fn evaluate_update_confirmation_requirement(
    parsed: &ParsedArgs,
    interactive_tty: bool,
) -> UpdateConfirmationRequirement {
    if parsed.dry_run {
        return UpdateConfirmationRequirement::NotRequired("dry_run");
    }
    if parsed.update_yes {
        return UpdateConfirmationRequirement::BypassedByYes;
    }
    if parsed.robot {
        return UpdateConfirmationRequirement::BlockedRobotMode;
    }
    if !interactive_tty {
        return UpdateConfirmationRequirement::BlockedNonInteractive;
    }
    UpdateConfirmationRequirement::NeedsPrompt
}

fn prompt_for_update_confirmation(target_version: &str, target_tag: &str) -> Result<bool, String> {
    eprint!(
        "rr update will replace the installed rr binary with {target_tag} ({target_version}). Continue? [y/N]: "
    );
    io::stderr()
        .flush()
        .map_err(|err| format!("failed to flush confirmation prompt: {err}"))?;

    let mut response = String::new();
    io::stdin()
        .read_line(&mut response)
        .map_err(|err| format!("failed to read confirmation response: {err}"))?;
    Ok(confirmation_response_is_affirmative(&response))
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StoreCompatibilityEnvelope {
    envelope_version: i64,
    store_schema_version: i64,
    min_supported_store_schema: i64,
    auto_migrate_from: i64,
    migration_policy: String,
    migration_class_max_auto: String,
    sidecar_generation: String,
    backup_required: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MigrationPreflight {
    status: &'static str,
    classification: &'static str,
    apply_allowed: bool,
    blocked_reason: Option<String>,
}

fn embedded_store_compatibility_envelope() -> StoreCompatibilityEnvelope {
    StoreCompatibilityEnvelope {
        envelope_version: 1,
        // Derived from the storage crate so the embedded envelope can never
        // drift from the schema this binary actually produces (a stale
        // hardcoded value here silently bricks rr update apply).
        store_schema_version: roger_storage::CURRENT_SCHEMA_VERSION,
        min_supported_store_schema: 0,
        // Auto-migrate floor. The store-open runner classifies a jump by delta
        // magnitude (see roger_storage::store_migration_class_label): only a
        // single-version additive bump is class_a. With migration_class_max_auto
        // pinned to class_a below, the only delta we truthfully auto-apply is
        // exactly one schema version, so the oldest schema with a class_a-only
        // path to the current schema is CURRENT_SCHEMA_VERSION - 1. This is the
        // conservative, honest floor: anything older is a >=2-version jump that
        // classifies class_b+ and is deliberately not claimed as auto here.
        auto_migrate_from: roger_storage::CURRENT_SCHEMA_VERSION.saturating_sub(1),
        // The new binary auto-migrates additive (class_a) schema deltas on first
        // store open (proven live v17->v18). Publishing auto_safe here — instead
        // of the old binary_only, which hard-blocked every schema bump — lets an
        // installed binary read a newer release's envelope and update in place
        // across an additive bump. Policy that governs a given update still comes
        // from the *target* release's published envelope, not this embedded one.
        migration_policy: "auto_safe".to_owned(),
        migration_class_max_auto: "class_a".to_owned(),
        sidecar_generation: "v1".to_owned(),
        backup_required: true,
    }
}

fn parse_store_compatibility_envelope(
    install_metadata: &Value,
) -> Result<StoreCompatibilityEnvelope, String> {
    let Some(compat) = install_metadata
        .get("store_compatibility")
        .and_then(Value::as_object)
    else {
        return Err("install metadata missing store_compatibility envelope".to_owned());
    };

    let parse_i64 = |field: &str| -> Result<i64, String> {
        compat.get(field).and_then(Value::as_i64).ok_or_else(|| {
            format!("install metadata store_compatibility.{field} must be an integer")
        })
    };
    let parse_string = |field: &str| -> Result<String, String> {
        compat
            .get(field)
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_owned)
            .ok_or_else(|| {
                format!("install metadata store_compatibility.{field} must be a non-empty string")
            })
    };

    let envelope = StoreCompatibilityEnvelope {
        envelope_version: parse_i64("envelope_version")?,
        store_schema_version: parse_i64("store_schema_version")?,
        min_supported_store_schema: parse_i64("min_supported_store_schema")?,
        auto_migrate_from: parse_i64("auto_migrate_from")?,
        migration_policy: parse_string("migration_policy")?,
        migration_class_max_auto: parse_string("migration_class_max_auto")?,
        sidecar_generation: parse_string("sidecar_generation")?,
        backup_required: compat
            .get("backup_required")
            .and_then(Value::as_bool)
            .ok_or_else(|| {
                "install metadata store_compatibility.backup_required must be a boolean".to_owned()
            })?,
    };

    if envelope.envelope_version < 1 {
        return Err(
            "install metadata store_compatibility.envelope_version must be >= 1".to_owned(),
        );
    }
    if envelope.min_supported_store_schema > envelope.store_schema_version {
        return Err(
            "install metadata store_compatibility.min_supported_store_schema cannot exceed store_schema_version"
                .to_owned(),
        );
    }
    if envelope.auto_migrate_from > envelope.store_schema_version {
        return Err(
            "install metadata store_compatibility.auto_migrate_from cannot exceed store_schema_version"
                .to_owned(),
        );
    }
    if !matches!(
        envelope.migration_policy.as_str(),
        "binary_only" | "auto_safe" | "explicit_operator_gate" | "unsupported"
    ) {
        return Err(
            "install metadata store_compatibility.migration_policy must be one of binary_only, auto_safe, explicit_operator_gate, unsupported"
                .to_owned(),
        );
    }
    if !matches!(
        envelope.migration_class_max_auto.as_str(),
        "class_a" | "class_b" | "none"
    ) {
        return Err(
            "install metadata store_compatibility.migration_class_max_auto must be one of class_a, class_b, none"
                .to_owned(),
        );
    }

    Ok(envelope)
}

fn read_local_store_schema_for_update(
    runtime: &CliRuntime,
    target_store_schema: i64,
) -> Result<i64, String> {
    let layout = StorageLayout::under(&runtime.store_root);
    if !layout.db_path.exists() {
        return Ok(target_store_schema);
    }
    let conn = SqliteConnection::open(&layout.db_path).map_err(|err| {
        format!(
            "failed to open local store for migration preflight ({}): {err}",
            layout.db_path.display()
        )
    })?;
    conn.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
        .map_err(|err| {
            format!(
                "failed to read local store schema version from {}: {err}",
                layout.db_path.display()
            )
        })
}

/// Rank of a migration class so preflight can compare an *actual* delta class
/// against the *ceiling* the target release publishes. Higher is more invasive;
/// `none` is a no-op and `class_d`/unknown sits above every auto-safe ceiling.
fn migration_class_rank(class: &str) -> u8 {
    match class {
        "none" => 0,
        "class_a" => 1,
        "class_b" => 2,
        "class_c" => 3,
        _ => 4,
    }
}

/// Semantic envelope-format compatibility. The embedded and published envelopes
/// are compared on *format* fields only — the envelope format version and the
/// sidecar generation marker. A mere store_schema_version difference (or a
/// migration_policy/class/window difference) between an installed binary and a
/// newer target release is NOT an incompatibility: assessing that schema delta
/// is the job of assess_migration_preflight, not this equality gate. Only a
/// genuinely different envelope format (envelope_version) or a different
/// derived-asset generation (sidecar_generation) is a structural mismatch that
/// must fail closed, because then the two sides do not agree on how to read the
/// envelope or the sidecars at all.
fn envelope_formats_compatible(
    embedded: &StoreCompatibilityEnvelope,
    published: &StoreCompatibilityEnvelope,
) -> bool {
    embedded.envelope_version == published.envelope_version
        && embedded.sidecar_generation == published.sidecar_generation
}

fn assess_migration_preflight(
    current_store_schema: i64,
    published: &StoreCompatibilityEnvelope,
    envelope_formats_compatible: bool,
) -> MigrationPreflight {
    if !envelope_formats_compatible {
        return MigrationPreflight {
            status: "migration_unsupported",
            classification: "class_d",
            apply_allowed: false,
            blocked_reason: Some("embedded_and_published_envelope_mismatch".to_owned()),
        };
    }

    if current_store_schema == published.store_schema_version {
        return MigrationPreflight {
            status: "no_migration_needed",
            classification: "none",
            apply_allowed: true,
            blocked_reason: None,
        };
    }

    if current_store_schema < published.min_supported_store_schema {
        return MigrationPreflight {
            status: "migration_unsupported",
            classification: "class_d",
            apply_allowed: false,
            blocked_reason: Some("local_store_schema_below_min_supported".to_owned()),
        };
    }

    if current_store_schema > published.store_schema_version {
        return MigrationPreflight {
            status: "migration_unsupported",
            classification: "class_d",
            apply_allowed: false,
            blocked_reason: Some("local_store_schema_newer_than_target_release".to_owned()),
        };
    }

    match published.migration_policy.as_str() {
        "auto_safe" => {
            if current_store_schema < published.auto_migrate_from {
                return MigrationPreflight {
                    status: "migration_requires_explicit_operator_gate",
                    classification: "class_c",
                    apply_allowed: false,
                    blocked_reason: Some(
                        "local_store_schema_outside_auto_migrate_window".to_owned(),
                    ),
                };
            }

            // The target release publishes an auto ceiling (class_a | class_b).
            // A "none" ceiling under an auto_safe policy is a malformed envelope.
            let ceiling = published.migration_class_max_auto.as_str();
            if !matches!(ceiling, "class_a" | "class_b") {
                return MigrationPreflight {
                    status: "migration_requires_explicit_operator_gate",
                    classification: "class_c",
                    apply_allowed: false,
                    blocked_reason: Some(
                        "auto_safe_policy_missing_auto_migration_class".to_owned(),
                    ),
                };
            }

            // Honest classification: use the storage runner's own delta-based
            // classifier for the ACTUAL current->target jump instead of blindly
            // echoing the published ceiling. A single-version bump is class_a; a
            // two-version bump is class_b; anything wider is class_d. This is the
            // same class first-open would apply, so we can never claim class_a on
            // a jump the runner would actually treat as class_b or refuse.
            let actual_class = roger_storage::store_migration_class_label(
                current_store_schema,
                published.store_schema_version,
            );

            if actual_class == "class_d" {
                return MigrationPreflight {
                    status: "migration_unsupported",
                    classification: "class_d",
                    apply_allowed: false,
                    blocked_reason: Some("auto_migration_class_unsupported_for_delta".to_owned()),
                };
            }

            if migration_class_rank(actual_class) > migration_class_rank(ceiling) {
                // e.g. a class_b jump under a class_a ceiling: the store could be
                // migrated, but not automatically under this release's policy.
                return MigrationPreflight {
                    status: "migration_requires_explicit_operator_gate",
                    classification: actual_class,
                    apply_allowed: false,
                    blocked_reason: Some(
                        "auto_migration_class_exceeds_published_ceiling".to_owned(),
                    ),
                };
            }

            MigrationPreflight {
                status: "auto_safe_migration_after_update",
                classification: actual_class,
                apply_allowed: true,
                blocked_reason: None,
            }
        }
        "explicit_operator_gate" => MigrationPreflight {
            status: "migration_requires_explicit_operator_gate",
            classification: "class_c",
            apply_allowed: false,
            blocked_reason: Some("target_release_requires_explicit_operator_gate".to_owned()),
        },
        "unsupported" => MigrationPreflight {
            status: "migration_unsupported",
            classification: "class_d",
            apply_allowed: false,
            blocked_reason: Some("target_release_declares_unsupported_migration_policy".to_owned()),
        },
        "binary_only" => MigrationPreflight {
            status: "migration_unsupported",
            classification: "class_d",
            apply_allowed: false,
            blocked_reason: Some("binary_only_policy_blocks_schema_migration".to_owned()),
        },
        _ => MigrationPreflight {
            status: "migration_unsupported",
            classification: "class_d",
            apply_allowed: false,
            blocked_reason: Some("unknown_migration_policy".to_owned()),
        },
    }
}

fn migration_preflight_payload(
    runtime: &CliRuntime,
    published: &StoreCompatibilityEnvelope,
    embedded: &StoreCompatibilityEnvelope,
) -> Result<Value, String> {
    let current_store_schema =
        read_local_store_schema_for_update(runtime, published.store_schema_version)?;
    let formats_compatible = envelope_formats_compatible(embedded, published);
    let preflight = assess_migration_preflight(current_store_schema, published, formats_compatible);

    let mut payload = json!({
        "status": preflight.status,
        "current_store_schema": current_store_schema,
        "target_store_schema": published.store_schema_version,
        "min_supported_store_schema": published.min_supported_store_schema,
        "auto_migrate_from": published.auto_migrate_from,
        "policy": published.migration_policy,
        "classification": preflight.classification,
        "backup_required": published.backup_required,
        "apply_allowed": preflight.apply_allowed,
        "migration_class_max_auto": published.migration_class_max_auto,
        "sidecar_generation": published.sidecar_generation,
        "envelope_version": published.envelope_version,
        "embedded_envelope_format_compatible": formats_compatible,
    });
    if let Some(reason) = preflight.blocked_reason {
        payload["blocked_reason"] = Value::String(reason);
    }
    Ok(payload)
}

fn migration_policy_payload() -> Value {
    json!({
        "policy": "binary_only",
        "schema_migrations_supported": false,
        "status": "deferred_for_now",
        "guidance": "if a future release requires local-state/schema migration, fail closed and use explicit backup/export + reinstall guidance",
    })
}

/// True when Roger extension integration was ever set up on this machine: the
/// persisted extension-id registry exists, or at least one per-version package
/// directory was fetched/packed under the store bridge layout.
fn extension_integration_ever_configured(store_root: &Path) -> bool {
    if extension_id_registry_path(store_root).is_file() {
        return true;
    }
    fs::read_dir(installed_extension_package_root(store_root))
        .map(|entries| entries.flatten().any(|entry| entry.path().is_dir()))
        .unwrap_or(false)
}

/// Recovers the extension id already registered in a Native Messaging host
/// manifest by parsing its `chrome-extension://<id>/` allowed origin.
fn extension_id_from_native_manifest(manifest_path: &Path) -> Option<String> {
    let text = fs::read_to_string(manifest_path).ok()?;
    let manifest: NativeHostManifest = serde_json::from_str(&text).ok()?;
    manifest.allowed_origins.iter().find_map(|origin| {
        origin
            .strip_prefix("chrome-extension://")
            .and_then(|rest| rest.strip_suffix('/'))
            .filter(|id| !id.is_empty())
            .map(ToOwned::to_owned)
    })
}

/// Non-fatal report from the post-update extension refresh phase.
struct ExtensionRefreshReport {
    data: Value,
    warnings: Vec<String>,
    repair_actions: Vec<String>,
}

/// After a successful in-place binary replacement, refresh the fetched
/// extension package and rewrite native-messaging host launcher + manifests so
/// the browser integration keeps matching the newly installed binary.
///
/// This phase is best-effort: any failure degrades into a typed
/// `extension_refresh_failed: <reason>` warning plus repair guidance and never
/// rolls back or fails the binary update. Returns `None` (skip silently) when
/// extension integration was never configured on this machine.
fn refresh_extension_after_update(
    runtime: &CliRuntime,
    repo: &str,
    version: &str,
    download_root: &str,
    bridge_binary: &Path,
) -> Option<ExtensionRefreshReport> {
    if !extension_integration_ever_configured(&runtime.store_root) {
        return None;
    }

    let mut warnings = Vec::new();
    let mut repair_actions = Vec::new();

    // 1. Fetch + install the new version's extension package (in-process; no
    //    shelling out to rr).
    let fetch =
        fetch_and_install_extension_package_core(&runtime.store_root, repo, version, download_root);
    let package_refreshed = fetch.is_ok();
    let package_dir = match &fetch {
        Ok(outcome) => Some(outcome.package_dir.to_string_lossy().to_string()),
        Err(failure) => {
            warnings.push(format!(
                "extension_refresh_failed: {} ({})",
                failure.reason_code, failure.message
            ));
            repair_actions.push(format!(
                "run rr extension fetch --version {version} to refresh the extension package"
            ));
            None
        }
    };

    // 2. Rewrite the native-messaging host launcher + manifest for every browser
    //    that already has a Roger host manifest present, keeping the existing
    //    extension id.
    let mut rewritten_browsers: Vec<String> = Vec::new();
    match (SupportedOs::current(), default_extension_install_root()) {
        (Some(host_os), Some(install_root)) => {
            let browsers = [
                SupportedBrowser::Chrome,
                SupportedBrowser::Edge,
                SupportedBrowser::Brave,
            ];
            let extension_id = discover_stored_or_env_extension_id(runtime)
                .map(|(id, _source)| id)
                .or_else(|| {
                    browsers.iter().find_map(|browser| {
                        let manifest_path =
                            native_host_install_path_for(browser, host_os, &install_root);
                        manifest_path
                            .exists()
                            .then(|| extension_id_from_native_manifest(&manifest_path))
                            .flatten()
                    })
                });

            match extension_id {
                Some(extension_id) => {
                    for browser in browsers {
                        let manifest_path =
                            native_host_install_path_for(&browser, host_os, &install_root);
                        if !manifest_path.exists() {
                            continue;
                        }
                        let launcher_path = native_host_launcher_path(&manifest_path, host_os);
                        if let Err(err) =
                            write_native_host_launcher(&launcher_path, bridge_binary, host_os)
                        {
                            warnings.push(format!(
                                "extension_refresh_failed: native_host_launcher_rewrite ({err})"
                            ));
                            continue;
                        }
                        let manifest = NativeHostManifest::for_roger(&launcher_path, &extension_id);
                        let manifest_bytes = match serde_json::to_vec_pretty(&manifest) {
                            Ok(mut bytes) => {
                                bytes.push(b'\n');
                                bytes
                            }
                            Err(err) => {
                                warnings.push(format!(
                                    "extension_refresh_failed: native_host_manifest_encode ({err})"
                                ));
                                continue;
                            }
                        };
                        if let Err(err) = fs::write(&manifest_path, &manifest_bytes) {
                            warnings.push(format!(
                                "extension_refresh_failed: native_host_manifest_rewrite ({err})"
                            ));
                            continue;
                        }
                        rewritten_browsers.push(supported_browser_label(browser).to_owned());
                    }
                }
                None => {
                    warnings.push(
                        "extension_refresh_failed: extension_id_unknown (native host manifests left unchanged; existing extension id could not be resolved)"
                            .to_owned(),
                    );
                    repair_actions.push(
                        "run rr extension setup --browser <edge|chrome|brave> to re-register the native host"
                            .to_owned(),
                    );
                }
            }
        }
        (None, _) => warnings.push(
            "extension_refresh_failed: unsupported_host_os (native host manifests left unchanged)"
                .to_owned(),
        ),
        (_, None) => warnings.push(
            "extension_refresh_failed: install_root_unresolved (HOME is missing; native host manifests left unchanged)"
                .to_owned(),
        ),
    }

    if package_refreshed {
        warnings.push(format!(
            "extension package refreshed to {version}; reload the unpacked extension in your browser"
        ));
    }
    if package_refreshed || !rewritten_browsers.is_empty() {
        repair_actions.push(
            "reload the unpacked Roger extension in your browser to pick up the refreshed package"
                .to_owned(),
        );
    }
    // If the refresh degraded, always leave a coherent re-setup path.
    if warnings
        .iter()
        .any(|warning| warning.starts_with("extension_refresh_failed:"))
    {
        repair_actions.push(
            "run rr extension setup --browser <edge|chrome|brave> after rr extension fetch to fully repair extension integration"
                .to_owned(),
        );
    }

    let data = json!({
        "attempted": true,
        "package_refreshed": package_refreshed,
        "version": version,
        "package_dir": package_dir,
        "native_hosts_rewritten": rewritten_browsers,
        "warnings": warnings.clone(),
    });

    Some(ExtensionRefreshReport {
        data,
        warnings,
        repair_actions,
    })
}

fn handle_update(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    let repo = parsed
        .repo
        .clone()
        .unwrap_or_else(|| "cdilga/roger-reviewer".to_owned());

    let Some(current_version) = option_env!("ROGER_RELEASE_VERSION").map(str::to_owned) else {
        let recommended_reinstall_command =
            release_hosted_reinstall_command(&repo, None, None, Some("stable"));
        return blocked_response(
            "rr update is disabled for local/unpublished builds without embedded release metadata"
                .to_owned(),
            vec![
                "install a published Roger release artifact before running rr update".to_owned(),
                format!("or run {recommended_reinstall_command}"),
            ],
            json!({
                "reason_code": "local_or_unpublished_build",
                "migration": migration_policy_payload(),
                "recommended_reinstall_command": recommended_reinstall_command,
            }),
        );
    };
    let current_channel = option_env!("ROGER_RELEASE_CHANNEL")
        .map(str::to_owned)
        .unwrap_or_else(|| "unknown".to_owned());
    let current_tag = option_env!("ROGER_RELEASE_TAG")
        .map(str::to_owned)
        .unwrap_or_else(|| format!("v{current_version}"));
    let channel = parsed.update_channel.clone();
    let api_root = parsed
        .update_api_root
        .clone()
        .unwrap_or_else(|| format!("https://api.github.com/repos/{repo}"));
    let download_root = parsed
        .update_download_root
        .clone()
        .unwrap_or_else(|| format!("https://github.com/{repo}/releases/download"));

    let target = match detect_update_target(parsed.update_target.as_ref()) {
        Ok(value) => value,
        Err(err) => {
            return blocked_response(
                err,
                vec!["pass --target <triple> explicitly".to_owned()],
                json!({"reason_code": "target_resolution_failed"}),
            );
        }
    };

    let target_version = if let Some(raw_version) = parsed.update_version.as_deref() {
        match normalize_calver_version(raw_version) {
            Ok(version) => version,
            Err(err) => {
                return blocked_response(
                    format!("invalid --version value: {err}"),
                    vec!["pass YYYY.MM.DD or YYYY.MM.DD-rc.N".to_owned()],
                    json!({"reason_code": "invalid_version"}),
                );
            }
        }
    } else {
        let tag = match resolve_latest_release_tag(&api_root, &channel) {
            Ok(tag) => tag,
            Err(err) => {
                return blocked_response(
                    format!("failed to resolve latest release tag: {err}"),
                    vec!["pass --version <YYYY.MM.DD[-rc.N]> explicitly".to_owned()],
                    json!({"reason_code": "latest_tag_resolution_failed"}),
                );
            }
        };
        match normalize_calver_version(&tag) {
            Ok(version) => version,
            Err(err) => {
                return blocked_response(
                    format!("resolved tag is not a valid CalVer release: {err}"),
                    vec!["pass --version <YYYY.MM.DD[-rc.N]> explicitly".to_owned()],
                    json!({"reason_code": "latest_tag_invalid"}),
                );
            }
        }
    };
    let target_tag = format!("v{target_version}");

    let install_metadata_name = format!("release-install-metadata-{target_version}.json");
    let install_metadata_url = format!("{download_root}/{target_tag}/{install_metadata_name}");
    let install_metadata_text = match fetch_url_with_curl(&install_metadata_url) {
        Ok(text) => text,
        Err(err) => {
            return blocked_response(
                format!("failed to fetch install metadata bundle: {err}"),
                vec![
                    "confirm the release tag is published".to_owned(),
                    "or pass --version for a known published CalVer release".to_owned(),
                ],
                json!({"reason_code": "install_metadata_missing", "url": install_metadata_url}),
            );
        }
    };
    let install_metadata: Value = match serde_json::from_str(&install_metadata_text) {
        Ok(value) => value,
        Err(err) => {
            return blocked_response(
                format!("install metadata bundle is invalid JSON: {err}"),
                vec!["re-run release verification for this tag".to_owned()],
                json!({"reason_code": "install_metadata_invalid_json"}),
            );
        }
    };
    if install_metadata.get("schema").and_then(Value::as_str)
        != Some("roger.release.install-metadata.v1")
    {
        return blocked_response(
            "install metadata schema mismatch; refusing update".to_owned(),
            vec!["rebuild release metadata bundle for this tag".to_owned()],
            json!({"reason_code": "install_metadata_schema_mismatch"}),
        );
    }

    let release = install_metadata.get("release").and_then(Value::as_object);
    let Some(release) = release else {
        return blocked_response(
            "install metadata missing release object".to_owned(),
            vec!["rebuild release metadata bundle for this tag".to_owned()],
            json!({"reason_code": "install_metadata_release_missing"}),
        );
    };
    if release.get("version").and_then(Value::as_str) != Some(target_version.as_str()) {
        return blocked_response(
            "install metadata release.version mismatch".to_owned(),
            vec!["verify release metadata and republish artifacts".to_owned()],
            json!({"reason_code": "install_metadata_version_mismatch"}),
        );
    }
    if release.get("tag").and_then(Value::as_str) != Some(target_tag.as_str()) {
        return blocked_response(
            "install metadata release.tag mismatch".to_owned(),
            vec!["verify release metadata and republish artifacts".to_owned()],
            json!({"reason_code": "install_metadata_tag_mismatch"}),
        );
    }

    let checksums_name = install_metadata
        .get("checksums_name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let core_manifest_name = install_metadata
        .get("core_manifest_name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    if checksums_name.is_empty()
        || core_manifest_name.is_empty()
        || checksums_name.contains('/')
        || checksums_name.contains('\\')
        || core_manifest_name.contains('/')
        || core_manifest_name.contains('\\')
    {
        return blocked_response(
            "install metadata checksums/core manifest names are invalid".to_owned(),
            vec!["rebuild release metadata bundle for this tag".to_owned()],
            json!({"reason_code": "install_metadata_name_invalid"}),
        );
    }

    let Some(target_entries) = install_metadata.get("targets").and_then(Value::as_array) else {
        return blocked_response(
            "install metadata targets must be an array".to_owned(),
            vec!["rebuild release metadata bundle for this tag".to_owned()],
            json!({"reason_code": "install_metadata_targets_invalid"}),
        );
    };
    let mut matching_targets = target_entries
        .iter()
        .filter(|entry| entry.get("target").and_then(Value::as_str) == Some(target.as_str()));
    let Some(target_entry) = matching_targets.next() else {
        return blocked_response(
            format!("install metadata has no entry for target {target}"),
            vec!["pass --target with an available triple".to_owned()],
            json!({"reason_code": "install_metadata_target_missing"}),
        );
    };
    if matching_targets.next().is_some() {
        return blocked_response(
            format!("install metadata has ambiguous entries for target {target}"),
            vec!["rebuild release metadata bundle to remove duplicate targets".to_owned()],
            json!({"reason_code": "install_metadata_target_ambiguous"}),
        );
    }

    let archive_name = target_entry
        .get("archive_name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let archive_sha256 = target_entry
        .get("archive_sha256")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let payload_dir = target_entry
        .get("payload_dir")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let binary_name = target_entry
        .get("binary_name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    if archive_name.is_empty()
        || archive_sha256.is_empty()
        || payload_dir.is_empty()
        || binary_name.is_empty()
    {
        return blocked_response(
            "install metadata target entry missing required fields".to_owned(),
            vec!["rebuild release metadata bundle for this tag".to_owned()],
            json!({"reason_code": "install_metadata_target_invalid"}),
        );
    }

    let core_manifest_url = format!("{download_root}/{target_tag}/{core_manifest_name}");
    let core_manifest_text = match fetch_url_with_curl(&core_manifest_url) {
        Ok(text) => text,
        Err(err) => {
            return blocked_response(
                format!("failed to fetch core manifest: {err}"),
                vec!["rebuild/upload core manifest for this tag".to_owned()],
                json!({"reason_code": "core_manifest_missing", "url": core_manifest_url}),
            );
        }
    };
    let core_manifest: Value = match serde_json::from_str(&core_manifest_text) {
        Ok(value) => value,
        Err(err) => {
            return blocked_response(
                format!("core manifest is invalid JSON: {err}"),
                vec!["rebuild core manifest for this tag".to_owned()],
                json!({"reason_code": "core_manifest_invalid_json"}),
            );
        }
    };
    if core_manifest.get("version").and_then(Value::as_str) != Some(target_version.as_str()) {
        return blocked_response(
            "core manifest version mismatch".to_owned(),
            vec!["rebuild core manifest and install metadata bundle".to_owned()],
            json!({"reason_code": "core_manifest_version_mismatch"}),
        );
    }
    let Some(core_targets) = core_manifest.get("targets").and_then(Value::as_array) else {
        return blocked_response(
            "core manifest targets must be an array".to_owned(),
            vec!["rebuild core manifest for this tag".to_owned()],
            json!({"reason_code": "core_manifest_targets_invalid"}),
        );
    };
    let mut matching_core = core_targets
        .iter()
        .filter(|entry| entry.get("target").and_then(Value::as_str) == Some(target.as_str()));
    let Some(core_target) = matching_core.next() else {
        return blocked_response(
            format!("core manifest has no entry for target {target}"),
            vec!["rebuild core manifest for this tag".to_owned()],
            json!({"reason_code": "core_manifest_target_missing"}),
        );
    };
    if matching_core.next().is_some() {
        return blocked_response(
            format!("core manifest has ambiguous entries for target {target}"),
            vec!["rebuild core manifest for this tag".to_owned()],
            json!({"reason_code": "core_manifest_target_ambiguous"}),
        );
    }
    for (field, expected) in [
        ("archive_name", archive_name.as_str()),
        ("archive_sha256", archive_sha256.as_str()),
        ("payload_dir", payload_dir.as_str()),
        ("binary_name", binary_name.as_str()),
    ] {
        let observed = core_target
            .get(field)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_ascii_lowercase();
        if observed != expected.to_ascii_lowercase() {
            return blocked_response(
                format!("core manifest target mismatch for {field}"),
                vec!["rebuild core manifest + install metadata bundle for this tag".to_owned()],
                json!({"reason_code": "core_manifest_target_mismatch", "field": field}),
            );
        }
    }

    let checksums_fetch = match fetch_checksums_manifest_with_fallback(
        &download_root,
        &target_tag,
        &checksums_name,
    ) {
        Ok(fetch) => fetch,
        Err(err) => {
            return blocked_response(
                err.message,
                vec!["rebuild/upload checksums for this tag".to_owned()],
                json!({
                    "reason_code": "checksums_missing",
                    "url": format!("{download_root}/{target_tag}/{checksums_name}"),
                    "attempted_urls": err.attempted_urls,
                    "legacy_fallback_attempted": checksums_name != "SHA256SUMS",
                }),
            );
        }
    };
    let checksums_url = checksums_fetch.url.clone();
    let checksums_legacy_fallback = checksums_fetch.legacy_fallback_used;
    let checksums_text = checksums_fetch.text;
    let checksums_sha = match checksums_entry_for_archive(&checksums_text, &archive_name) {
        Ok(value) => value,
        Err(err) => {
            return blocked_response(
                err,
                vec!["rebuild checksums for this tag".to_owned()],
                json!({"reason_code": "checksums_entry_invalid"}),
            );
        }
    };
    if checksums_sha != archive_sha256 {
        return blocked_response(
            "install metadata/checksums mismatch for release archive".to_owned(),
            vec!["re-run verify-assets and publish gates for this tag".to_owned()],
            json!({"reason_code": "checksums_mismatch"}),
        );
    }

    let published_envelope = match parse_store_compatibility_envelope(&install_metadata) {
        Ok(value) => value,
        Err(err) => {
            return blocked_response(
                format!("install metadata store compatibility envelope is invalid: {err}"),
                vec!["rebuild release install metadata for this tag".to_owned()],
                json!({"reason_code": "install_metadata_store_compatibility_invalid"}),
            );
        }
    };
    let embedded_envelope = embedded_store_compatibility_envelope();
    let migration_policy =
        match migration_preflight_payload(runtime, &published_envelope, &embedded_envelope) {
            Ok(value) => value,
            Err(err) => {
                return blocked_response(
                    format!("failed to inspect local store migration posture: {err}"),
                    vec![
                        "repair or remove the local Roger store, then re-run rr update --dry-run"
                            .to_owned(),
                        "or run scripts/release/rr-install.sh directly after backing up local state"
                            .to_owned(),
                    ],
                    json!({"reason_code": "store_schema_probe_failed"}),
                );
            }
        };
    let migration_apply_allowed = migration_policy
        .get("apply_allowed")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    if current_version == target_version {
        return CommandResponse {
            outcome: OutcomeKind::Empty,
            data: json!({
                "current_version": current_version,
                "current_channel": current_channel,
                "current_tag": current_tag,
                "current_release": {
                    "version": current_version,
                    "channel": current_channel,
                    "tag": current_tag,
                },
                "target_version": Value::Null,
                "target_tag": Value::Null,
                "target_release": {
                    "version": Value::Null,
                    "channel": channel,
                    "tag": Value::Null,
                },
                "target": target,
                "up_to_date": true,
                "checksums_legacy_fallback": checksums_legacy_fallback,
                "metadata_urls": {
                    "install_metadata": install_metadata_url,
                    "core_manifest": core_manifest_url,
                    "checksums": checksums_url,
                },
                "migration": migration_policy.clone(),
                "confirmation": {
                    "required": false,
                    "confirmed": false,
                    "mode": "not_required_up_to_date",
                },
            }),
            warnings: Vec::new(),
            repair_actions: Vec::new(),
            message: "rr is already on the requested release".to_owned(),
        };
    }

    let archive_url = format!("{download_root}/{target_tag}/{archive_name}");
    let recommended_command = release_hosted_reinstall_command(
        &repo,
        Some(&target_version),
        Some(&target_tag),
        Some(&channel),
    );

    if parsed.dry_run {
        return CommandResponse {
            outcome: OutcomeKind::Complete,
            data: json!({
                "current_release": {
                    "version": current_version,
                    "channel": current_channel,
                    "tag": current_tag,
                },
                "target_release": {
                    "version": target_version,
                    "channel": channel,
                    "tag": target_tag,
                },
                "target": target,
                "metadata_urls": {
                    "install_metadata": install_metadata_url,
                    "core_manifest": core_manifest_url,
                    "checksums": checksums_url,
                },
                "checksums_legacy_fallback": checksums_legacy_fallback,
                "archive": {
                    "name": archive_name,
                    "sha256": archive_sha256,
                    "payload_dir": payload_dir,
                    "binary_name": binary_name,
                    "url": archive_url,
                },
                "migration": migration_policy.clone(),
                "confirmation": {
                    "required": false,
                    "confirmed": false,
                    "mode": "dry_run",
                },
                "mode": "dry_run",
                "recommended_install_command": recommended_command,
            }),
            warnings: Vec::new(),
            repair_actions: vec!["run the recommended_install_command to apply update".to_owned()],
            message: "rr update dry-run metadata validation complete".to_owned(),
        };
    }

    if !migration_apply_allowed {
        let blocked_reason = migration_policy
            .get("blocked_reason")
            .and_then(Value::as_str)
            .unwrap_or("migration_preflight_blocked");
        let installer_one_liner = release_latest_installer_one_liner(&repo);
        return blocked_response(
            format!("rr update apply blocked by migration posture: {blocked_reason}"),
            vec![
                installer_one_liner,
                "run rr update --dry-run --robot to inspect migration posture details".to_owned(),
                "apply is allowed only when migration.apply_allowed=true".to_owned(),
            ],
            json!({
                "reason_code": "migration_preflight_blocked",
                "target_version": target_version,
                "target_tag": target_tag,
                "target": target,
                "migration": migration_policy.clone(),
                "recommended_install_command": recommended_command,
            }),
        );
    }

    let interactive_tty = io::stdin().is_terminal() && io::stdout().is_terminal();
    let confirmation = match evaluate_update_confirmation_requirement(parsed, interactive_tty) {
        UpdateConfirmationRequirement::NotRequired(mode) => json!({
            "required": false,
            "confirmed": false,
            "mode": mode,
        }),
        UpdateConfirmationRequirement::BypassedByYes => json!({
            "required": true,
            "confirmed": true,
            "mode": "yes_flag",
        }),
        UpdateConfirmationRequirement::BlockedRobotMode => {
            return blocked_response(
                "rr update in --robot mode requires --yes/-y to confirm non-interactive apply"
                    .to_owned(),
                vec![
                    "re-run rr update --robot --yes once preflight checks are acceptable"
                        .to_owned(),
                    "or run rr update interactively to confirm at the prompt".to_owned(),
                ],
                json!({
                    "reason_code": "update_confirmation_required_robot",
                    "target_version": target_version,
                    "target_tag": target_tag,
                    "target": target,
                    "confirmation": {
                        "required": true,
                        "confirmed": false,
                        "mode": "robot_blocked",
                    },
                }),
            );
        }
        UpdateConfirmationRequirement::BlockedNonInteractive => {
            return blocked_response(
                "rr update requires explicit confirmation on a TTY or --yes/-y".to_owned(),
                vec![
                    "re-run rr update in an interactive terminal and confirm".to_owned(),
                    "or pass --yes / -y for non-interactive confirmation".to_owned(),
                ],
                json!({
                    "reason_code": "update_confirmation_required_non_tty",
                    "target_version": target_version,
                    "target_tag": target_tag,
                    "target": target,
                    "confirmation": {
                        "required": true,
                        "confirmed": false,
                        "mode": "non_interactive_blocked",
                    },
                }),
            );
        }
        UpdateConfirmationRequirement::NeedsPrompt => {
            match prompt_for_update_confirmation(&target_version, &target_tag) {
                Ok(true) => json!({
                    "required": true,
                    "confirmed": true,
                    "mode": "interactive_prompt",
                }),
                Ok(false) => {
                    return blocked_response(
                        "rr update cancelled before apply".to_owned(),
                        vec![
                            "re-run rr update and confirm when ready".to_owned(),
                            "or pass --yes / -y for non-interactive confirmation".to_owned(),
                        ],
                        json!({
                            "reason_code": "update_cancelled",
                            "target_version": target_version,
                            "target_tag": target_tag,
                            "target": target,
                            "confirmation": {
                                "required": true,
                                "confirmed": false,
                                "mode": "interactive_prompt_declined",
                            },
                        }),
                    );
                }
                Err(err) => {
                    return error_response(format!("failed to read update confirmation: {err}"));
                }
            }
        }
    };

    let current_exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(err) => {
            let recommended_reinstall_command = release_hosted_reinstall_command(
                &repo,
                Some(&target_version),
                Some(&target_tag),
                Some(&channel),
            );
            return blocked_response(
                format!("failed to resolve current executable path: {err}"),
                vec![format!("run {recommended_reinstall_command}")],
                json!({
                    "reason_code": "current_exe_resolution_failed",
                    "recommended_reinstall_command": recommended_reinstall_command,
                }),
            );
        }
    };
    let install_path = match resolve_update_install_path(&current_exe, &binary_name) {
        Ok(path) => path,
        Err(err) => {
            let recommended_reinstall_command = release_hosted_reinstall_command(
                &repo,
                Some(&target_version),
                Some(&target_tag),
                Some(&channel),
            );
            return blocked_response(
                err,
                vec![
                    "install Roger to a direct rr binary path before running rr update".to_owned(),
                    format!("or run {recommended_reinstall_command}"),
                ],
                json!({
                    "reason_code": "unsupported_install_layout",
                    "recommended_reinstall_command": recommended_reinstall_command,
                }),
            );
        }
    };

    let apply_outcome = match apply_update_archive_in_place(
        &archive_url,
        &archive_name,
        &archive_sha256,
        &payload_dir,
        &binary_name,
        &install_path,
        &target_version,
    ) {
        Ok(outcome) => outcome,
        Err(err) => {
            let recommended_reinstall_command = release_hosted_reinstall_command(
                &repo,
                Some(&target_version),
                Some(&target_tag),
                Some(&channel),
            );
            return blocked_response(
                format!("failed to apply in-place update: {err}"),
                vec![
                    "re-run rr update after resolving install path and permissions".to_owned(),
                    format!("or run {recommended_reinstall_command}"),
                ],
                json!({
                    "reason_code": "in_place_apply_failed",
                    "install_path": install_path.to_string_lossy(),
                    "recommended_reinstall_command": recommended_reinstall_command,
                }),
            );
        }
    };

    // Post-binary refresh: keep browser integration matching the new binary.
    // Best-effort — failures degrade to typed warnings and never roll back the
    // binary update that already succeeded above.
    let extension_refresh = refresh_extension_after_update(
        runtime,
        &repo,
        &target_version,
        &download_root,
        &apply_outcome.install_path,
    );
    let (extension_refresh_value, refresh_warnings, refresh_repair_actions) =
        match extension_refresh {
            Some(report) => (report.data, report.warnings, report.repair_actions),
            None => (json!({ "attempted": false }), Vec::new(), Vec::new()),
        };

    CommandResponse {
        outcome: OutcomeKind::Complete,
        data: json!({
            "current_release": {
                "version": current_version,
                "channel": current_channel,
                "tag": current_tag,
            },
            "target_release": {
                "version": target_version,
                "channel": channel,
                "tag": target_tag,
            },
            "target": target,
            "metadata_urls": {
                "install_metadata": install_metadata_url,
                "core_manifest": core_manifest_url,
                "checksums": checksums_url,
            },
            "checksums_legacy_fallback": checksums_legacy_fallback,
            "archive": {
                "name": archive_name,
                "sha256": archive_sha256,
                "payload_dir": payload_dir,
                "binary_name": binary_name,
                "url": archive_url,
            },
            "migration": migration_policy,
            "confirmation": confirmation,
            "mode": "in_place_apply",
            "apply": {
                "install_path": apply_outcome.install_path.to_string_lossy(),
                "backup_path": apply_outcome.backup_path.to_string_lossy(),
                "rollback_strategy": "rename_with_immediate_restore_on_failure",
            },
            "extension_refresh": extension_refresh_value,
            "recommended_install_command": recommended_command,
        }),
        warnings: refresh_warnings,
        repair_actions: refresh_repair_actions,
        message: format!("rr updated from {} to {}", current_version, target_version),
    }
}

fn handle_robot_docs(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    let topic = parsed.robot_docs_topic.as_deref().unwrap_or("guide");

    let (items, version) = match topic {
        "guide" => (
            vec![
                json!({
                    "kind": "preferred_surface",
                    "summary": "The operator surface is seven verbs: doctor, queue, review, open, findings, send, setup. Older command names remain routable compatibility aliases and emit the underlying command's schema id unchanged.",
                    "verbs": {
                        "doctor": "check whether Roger can run",
                        "queue": "choose review work (alias: prs)",
                        "review": "start or re-enter review work (rr review --resume aliases rr resume)",
                        "open": "use the local cockpit (alias: tui)",
                        "findings": "inspect and search output (--query aliases search, --sessions aliases sessions)",
                        "send": "triage/draft/approve/post outbound comms (rr send <sub>)",
                        "setup": "install/update/repair integrations (rr setup extension|doctor|fetch|uninstall|update|assets)"
                    },
                    "machine_surfaces": {
                        "rr api docs": "machine-contract docs (alias: rr robot-docs)",
                        "rr agent": "in-session worker transport, separate from --robot"
                    },
                    "alias_schema_mapping": [
                        {"alias": "rr send post", "schema_id": "rr.robot.post.v1"},
                        {"alias": "rr send approve", "schema_id": "rr.robot.approve.v1"},
                        {"alias": "rr send draft", "schema_id": "rr.robot.draft.v1"},
                        {"alias": "rr send triage", "schema_id": "rr.robot.triage.v1"},
                        {"alias": "rr review --resume", "schema_id": "rr.robot.resume.v1"},
                        {"alias": "rr findings --query", "schema_id": "rr.robot.search.v1"},
                        {"alias": "rr findings --sessions", "schema_id": "rr.robot.sessions.v1"},
                        {"alias": "rr setup update", "schema_id": "rr.robot.update.v1"},
                        {"alias": "rr api docs", "schema_id": "rr.robot.robot_docs.v1"}
                    ]
                }),
                json!({"command": "rr status --robot", "purpose": "session attention snapshot"}),
                json!({"command": "rr init --robot", "purpose": "bootstrap Roger-owned local store and marker state; provider auth/install preflight remains a separate follow-up surface"}),
                json!({"command": "rr doctor --provider <name> --robot", "purpose": "provider-aware preflight for local bootstrap, binary presence, policy/profile resolution, and deferred first-launch checks"}),
                json!({"command": "rr sessions --robot", "purpose": "global session finder"}),
                json!({"command": "rr tui", "purpose": "interactive-only operator cockpit; rr tui --robot fails closed — use rr status/findings/sessions --robot for machine-readable state", "interactive_only": true}),
                json!({"command": "rr prs --robot", "purpose": "read-only review queue of open pull requests joined with local Roger session state"}),
                json!({"command": "rr findings --robot", "purpose": "structured findings list"}),
                json!({"command": "rr search --query <text> --query-mode recall --robot", "purpose": "prior-review lookup"}),
                json!({"command": "rr draft --session <id> --finding <finding-id> --robot", "purpose": "materialize local outbound drafts bound to the current review target without posting to GitHub"}),
                json!({"command": "rr approve --session <id> --batch <draft-batch-id> --robot", "purpose": "record an explicit local approval token bound to one exact batch payload and target without posting to GitHub"}),
                json!({"command": "rr post --session <id> --batch <draft-batch-id> --robot", "purpose": "execute only one exact Roger-approved stored batch through the GitHub adapter and return a truthful posting result envelope"}),
                json!({
                    "kind": "provider_support",
                    "command": "rr review --provider <name>",
                    "summary": runtime_review_provider_support_summary(runtime),
                    "live_review_providers": runtime_review_provider_support_matrix(runtime),
                    "planned_not_live_providers": runtime_planned_not_live_review_providers(runtime),
                    "feature_gated_disabled_providers": runtime_feature_gated_disabled_review_providers(runtime),
                    "not_supported_providers": NOT_LIVE_REVIEW_PROVIDERS,
                }),
                json!({"command": "rr update --channel stable --dry-run --robot", "purpose": "update metadata preflight (non-mutating)"}),
                json!({"command": "rr update --channel stable --yes --robot", "purpose": "non-interactive in-place apply after explicit confirmation bypass"}),
                json!({"command": "rr bridge verify-contracts --robot", "purpose": "bridge contract drift check"}),
                json!({"command": "rr bridge pack-extension --robot", "purpose": "assemble unpacked browser sideload artifact"}),
                json!({"command": "rr extension setup --browser <edge|chrome|brave> --robot", "purpose": "guided package/setup flow with fail-closed identity + host checks"}),
                json!({"command": "rr extension doctor --browser <edge|chrome|brave> --robot", "purpose": "verify package, identity, native host registration, and bridge reachability"}),
                json!({"command": "rr extension fetch [--version <YYYY.MM.DD[-rc.N]>] --robot", "purpose": "download, checksum-verify, and install the published extension package into the installed layout for hosts outside the Roger dev workspace"}),
                json!({"command": "rr extension uninstall --robot", "purpose": "guided operator uninstall path for bridge host-registration assets"}),
                json!({"command": "rr bridge install [--extension-id <id>] --robot", "purpose": "repair/dev host registration override when guided setup cannot discover identity"}),
                json!({"command": "rr bridge uninstall --robot", "purpose": "repair alias for host-registration asset removal when extension uninstall is unavailable"}),
                json!({"command": "rr robot-docs schemas --robot", "purpose": "schema inventory"}),
                json!({
                    "kind": "reconciliation_contract",
                    "mode": "persisted_readback",
                    "manual_refresh_supported": false,
                    "summary": "There is no standalone refresh command. Roger surfaces the last persisted review state and requires explicit re-entry or a fresh pass when target drift is detected."
                }),
                json!({
                    "kind": "inside_roger_skill",
                    "context": "inside_roger",
                    "audience": "agent",
                    "skill_path": ".claude/skills/roger-inside-roger-agent/SKILL.md",
                    "purpose": "safe in-harness review loop when already inside a Roger-managed provider session",
                    "example": {
                        "commands": ["roger-help", "roger-status", "roger-findings", "roger-return"],
                        "notes": [
                            "use only inside an active Roger-managed provider session or bare-harness continuation",
                            "if unsupported, fail closed to the equivalent rr command outside the harness",
                            "does not authorize approval, posting, raw gh writes, or bypassing Roger review policy"
                        ]
                    },
                    "finding_return_contract": {
                        "canonical_transport": "rr agent worker.submit_stage_result",
                        "availability": "canonical worker contract; separate from the --robot command shortlist and not implied to be shipped by this discovery item alone",
                        "binding_fields": [
                            "review_session_id",
                            "review_run_id",
                            "review_task_id",
                            "task_nonce"
                        ],
                        "result_fields": [
                            "schema_id",
                            "stage",
                            "task_kind",
                            "outcome",
                            "summary",
                            "structured_findings_pack"
                        ],
                        "finding_pack": {
                            "schema_version": "structured_findings_pack/v1",
                            "finding_fields": [
                                "fingerprint",
                                "title",
                                "normalized_summary",
                                "severity",
                                "confidence",
                                "code_evidence"
                            ]
                        },
                        "result_envelope_example": {
                            "operation": "worker.submit_stage_result",
                            "payload": {
                                "schema_id": "<worker-stage-result-schema>",
                                "review_session_id": "<review-session-id>",
                                "review_run_id": "<review-run-id>",
                                "review_task_id": "<review-task-id>",
                                "task_nonce": "<task-nonce>",
                                "stage": "deep_review",
                                "task_kind": "review",
                                "outcome": "completed",
                                "summary": "Found 1 likely correctness issue.",
                                "structured_findings_pack": {
                                    "schema_version": "structured_findings_pack/v1",
                                    "stage": "deep_review",
                                    "findings": [
                                        {
                                            "fingerprint": "<finding-fingerprint>",
                                            "title": "Null result ignored in refresh path",
                                            "normalized_summary": "The refresh path drops a null adapter result and reports success instead of surfacing a failure.",
                                            "severity": "high",
                                            "confidence": "medium",
                                            "code_evidence": [
                                                {
                                                    "evidence_role": "primary",
                                                    "repo_rel_path": "packages/cli/src/lib.rs",
                                                    "start_line": 1200,
                                                    "end_line": 1218,
                                                    "anchor_digest": "<anchor-digest>"
                                                }
                                            ]
                                        }
                                    ]
                                }
                            }
                        },
                        "notes": [
                            "Roger validates the session/run/task/nonce binding before accepting the result",
                            "Roger validates and repairs the nested findings pack before materializing canonical Finding rows",
                            "roger-return is a control handoff back to Roger, not the findings submission transport"
                        ]
                    }
                }),
            ],
            "0.1.0",
        ),
        "commands" => (
            vec![
                json!({"command": "rr send triage", "preferred_alias_of": "rr triage", "required_formats": ["json"], "optional_formats": []}),
                json!({"command": "rr send draft", "preferred_alias_of": "rr draft", "required_formats": ["json"], "optional_formats": []}),
                json!({"command": "rr send approve", "preferred_alias_of": "rr approve", "required_formats": ["json"], "optional_formats": []}),
                json!({"command": "rr send post", "preferred_alias_of": "rr post", "required_formats": ["json"], "optional_formats": []}),
                json!({"command": "rr setup update", "preferred_alias_of": "rr update", "required_formats": ["json"], "optional_formats": []}),
                json!({"command": "rr api docs", "preferred_alias_of": "rr robot-docs", "required_formats": ["json"], "optional_formats": ["compact"]}),
                json!({"command": "rr status", "required_formats": ["json"], "optional_formats": ["compact"]}),
                json!({"command": "rr init", "required_formats": ["json"], "optional_formats": []}),
                json!({"command": "rr doctor", "required_formats": ["json"], "optional_formats": []}),
                json!({"command": "rr sessions", "required_formats": ["json"], "optional_formats": ["compact"]}),
                json!({"command": "rr prs", "required_formats": ["json"], "optional_formats": ["compact"]}),
                json!({"command": "rr findings", "required_formats": ["json"], "optional_formats": ["compact"]}),
                json!({"command": "rr search", "required_formats": ["json"], "optional_formats": ["compact"]}),
                json!({"command": "rr timeline", "required_formats": ["json"], "optional_formats": []}),
                json!({"command": "rr memory review", "required_formats": ["json"], "optional_formats": []}),
                json!({"command": "rr memory accept", "required_formats": ["json"], "optional_formats": []}),
                json!({"command": "rr memory reject", "required_formats": ["json"], "optional_formats": []}),
                json!({"command": "rr clarify", "required_formats": ["json"], "optional_formats": []}),
                json!({"command": "rr triage", "required_formats": ["json"], "optional_formats": []}),
                json!({"command": "rr draft", "required_formats": ["json"], "optional_formats": []}),
                json!({"command": "rr approve", "required_formats": ["json"], "optional_formats": []}),
                json!({"command": "rr post", "required_formats": ["json"], "optional_formats": []}),
                json!({"command": "rr update", "required_formats": ["json"], "optional_formats": []}),
                json!({
                    "command": "rr review --dry-run",
                    "required_formats": ["json"],
                    "optional_formats": [],
                    "supported_providers": runtime_supported_review_providers(runtime),
                    "planned_not_live_providers": runtime_planned_not_live_review_providers(runtime),
                    "feature_gated_disabled_providers": runtime_feature_gated_disabled_review_providers(runtime),
                    "not_supported_providers": NOT_LIVE_REVIEW_PROVIDERS,
                }),
                json!({"command": "rr resume --dry-run", "required_formats": ["json"], "optional_formats": []}),
                json!({"command": "rr return", "required_formats": ["json"], "optional_formats": []}),
                json!({"command": "rr bridge export-contracts", "required_formats": ["json"], "optional_formats": []}),
                json!({"command": "rr bridge verify-contracts", "required_formats": ["json"], "optional_formats": []}),
                json!({"command": "rr bridge pack-extension", "required_formats": ["json"], "optional_formats": []}),
                json!({"command": "rr extension setup", "required_formats": ["json"], "optional_formats": []}),
                json!({"command": "rr extension doctor", "required_formats": ["json"], "optional_formats": []}),
                json!({"command": "rr extension fetch", "required_formats": ["json"], "optional_formats": []}),
                json!({"command": "rr extension uninstall", "required_formats": ["json"], "optional_formats": []}),
                json!({"command": "rr bridge install", "required_formats": ["json"], "optional_formats": []}),
                json!({"command": "rr bridge uninstall", "required_formats": ["json"], "optional_formats": []}),
                json!({"command": "rr robot-docs guide", "required_formats": ["json"], "optional_formats": ["compact"]}),
                json!({"command": "rr robot-docs commands", "required_formats": ["json"], "optional_formats": ["compact"]}),
                json!({"command": "rr robot-docs schemas", "required_formats": ["json"], "optional_formats": ["compact"]}),
                json!({"command": "rr robot-docs workflows", "required_formats": ["json"], "optional_formats": ["compact"]}),
                json!({
                    "command": "rr agent <operation>",
                    "surface": "dedicated_worker_transport",
                    "separate_from_robot": true,
                    "required_formats": ["json"],
                    "optional_formats": [],
                    "request_schema_id": AGENT_TRANSPORT_REQUEST_SCHEMA_V1,
                    "response_schema_id": AGENT_TRANSPORT_RESPONSE_SCHEMA_V1,
                    "supported_operations": [
                        "worker.get_review_context",
                        "worker.search_memory",
                        "worker.list_findings",
                        "worker.get_finding_detail",
                        "worker.get_artifact_excerpt",
                        "worker.get_status",
                        "worker.submit_stage_result",
                        "worker.request_clarification",
                        "worker.request_memory_review",
                        "worker.propose_follow_up"
                    ],
                    "notes": "rr agent is the dedicated in-session worker transport; it rejects --robot and is not part of the operator robot shortlist"
                }),
            ],
            "0.1.0",
        ),
        "schemas" => (
            vec![
                json!({"command": "rr review", "schema_id": "rr.robot.review.v1"}),
                json!({"command": "rr init", "schema_id": "rr.robot.init.v1"}),
                json!({"command": "rr doctor", "schema_id": "rr.robot.doctor.v1"}),
                json!({"command": "rr resume", "schema_id": "rr.robot.resume.v1"}),
                json!({"command": "rr return", "schema_id": "rr.robot.return.v1"}),
                json!({"command": "rr sessions", "schema_id": "rr.robot.sessions.v1"}),
                json!({"command": "rr prs", "schema_id": "rr.robot.prs.v1"}),
                json!({"command": "rr search", "schema_id": "rr.robot.search.v1"}),
                json!({"command": "rr triage", "schema_id": "rr.robot.triage.v1"}),
                json!({"command": "rr draft", "schema_id": "rr.robot.draft.v1"}),
                json!({"command": "rr approve", "schema_id": "rr.robot.approve.v1"}),
                json!({"command": "rr post", "schema_id": "rr.robot.post.v1"}),
                json!({"command": "rr update", "schema_id": "rr.robot.update.v1"}),
                json!({"command": "rr bridge", "schema_id": "rr.robot.bridge.v1"}),
                json!({"command": "rr extension", "schema_id": "rr.robot.extension.v1"}),
                json!({"command": "rr findings", "schema_id": "rr.robot.findings.v1"}),
                json!({"command": "rr status", "schema_id": "rr.robot.status.v1"}),
                json!({"command": "rr memory", "schema_id": "rr.robot.memory.v1"}),
                json!({"command": "rr timeline", "schema_id": "rr.robot.timeline.v1"}),
                json!({"command": "rr clarify", "schema_id": "rr.robot.clarify.v1"}),
                json!({"command": "rr robot-docs", "schema_id": "rr.robot.robot_docs.v1"}),
                json!({"command": "rr tui", "schema_id": "rr.robot.tui.v1"}),
                json!({"command": "rr <command> (harness)", "schema_id": "rr.robot.harness_command.v1", "surface": "inside_roger_harness"}),
                json!({"command": "rr agent", "schema_id": AGENT_TRANSPORT_RESPONSE_SCHEMA_V1, "surface": "dedicated_worker_transport"}),
                // Preferred container/alias names emit the underlying command's
                // schema id unchanged. No new schema ids are introduced by the
                // simplified surface; the mapping is documented here truthfully.
                json!({"command": "rr send triage", "alias_of": "rr triage", "schema_id": "rr.robot.triage.v1"}),
                json!({"command": "rr send draft", "alias_of": "rr draft", "schema_id": "rr.robot.draft.v1"}),
                json!({"command": "rr send approve", "alias_of": "rr approve", "schema_id": "rr.robot.approve.v1"}),
                json!({"command": "rr send post", "alias_of": "rr post", "schema_id": "rr.robot.post.v1"}),
                json!({"command": "rr review --resume", "alias_of": "rr resume", "schema_id": "rr.robot.resume.v1"}),
                json!({"command": "rr findings --query", "alias_of": "rr search", "schema_id": "rr.robot.search.v1"}),
                json!({"command": "rr findings --sessions", "alias_of": "rr sessions", "schema_id": "rr.robot.sessions.v1"}),
                json!({"command": "rr setup extension", "alias_of": "rr extension", "schema_id": "rr.robot.extension.v1"}),
                json!({"command": "rr setup update", "alias_of": "rr update", "schema_id": "rr.robot.update.v1"}),
                json!({"command": "rr setup assets", "alias_of": "rr assets", "schema_id": "rr.robot.assets.v1"}),
                json!({"command": "rr api docs", "alias_of": "rr robot-docs", "schema_id": "rr.robot.robot_docs.v1"}),
            ],
            "0.1.0",
        ),
        "workflows" => (
            vec![
                json!({"name": "resume_loop", "steps": ["rr sessions --robot", "rr resume --session <id> --robot", "rr findings --session <id> --robot"], "notes": "There is no standalone refresh action. Readback surfaces expose persisted attention state and repair guidance; re-entry surfaces remain the place where Roger can safely reconcile stale review context."}),
                json!({"name": "review_queue", "steps": ["rr prs --robot", "rr review --pr <n> --provider <p> --robot"], "notes": "rr prs is read-only: it lists open pull requests via the GitHub adapter and joins each one with persisted local Roger session state. It never posts to GitHub and never mutates sessions or the store."}),
                json!({"name": "search_followup", "steps": ["rr search --query <text> --query-mode recall --robot", "rr status --session <id> --robot"]}),
                json!({"name": "local_outbound_draft", "steps": ["rr findings --session <id> --robot", "rr triage --session <id> --finding <finding-id> --state accepted --robot", "rr draft --session <id> --finding <finding-id> [--finding <finding-id>] --robot", "rr status --session <id> --robot"], "notes": "rr draft materializes local Roger-owned draft batches only. It requires findings triaged to accepted (record the decision with rr triage first), does not approve or post anything to GitHub, and fails closed if the session target or persisted review state is stale."}),
                json!({"name": "local_outbound_approve", "steps": ["rr findings --session <id> --robot", "rr triage --session <id> --finding <finding-id> --state accepted --robot", "rr draft --session <id> --finding <finding-id> [--finding <finding-id>] --robot", "rr approve --session <id> --batch <draft-batch-id> --robot", "rr status --session <id> --robot"], "notes": "rr approve records a local approval token for one exact stored batch payload and target tuple. It remains local-only and blocks when drift or invalidation revoked approval eligibility."}),
                json!({"name": "local_outbound_post", "steps": ["rr findings --session <id> --robot", "rr triage --session <id> --finding <finding-id> --state accepted --robot", "rr draft --session <id> --finding <finding-id> [--finding <finding-id>] --robot", "rr approve --session <id> --batch <draft-batch-id> --robot", "rr post --session <id> --batch <draft-batch-id> --robot", "rr status --session <id> --robot"], "notes": "rr post executes only one exact approved stored batch on the bound target. It re-verifies approval and payload binding before posting, records immutable posting lineage, and surfaces partial failures explicitly."}),
                json!({
                    "name": "inside_roger_safe_subset",
                    "context": "inside_roger",
                    "skill_path": ".claude/skills/roger-inside-roger-agent/SKILL.md",
                    "steps": ["roger-help", "roger-status", "roger-findings", "roger-return"],
                    "notes": "Use only inside a Roger-managed provider session. These are optional harness-native convenience commands; if unsupported, return to the equivalent rr command outside the harness."
                }),
            ],
            "0.1.0",
        ),
        _ => {
            return blocked_response(
                format!("unknown robot-docs topic: {topic}"),
                vec![
                    "use one of: guide, commands, schemas, workflows".to_owned(),
                    "or pass --topic <name>".to_owned(),
                ],
                json!({"reason_code": "unknown_robot_docs_topic", "topic": topic}),
            );
        }
    };

    CommandResponse {
        outcome: OutcomeKind::Complete,
        data: json!({
            "topic": topic,
            "version": version,
            "items": items,
        }),
        warnings: Vec::new(),
        repair_actions: Vec::new(),
        message: format!("robot docs loaded for topic {topic}"),
    }
}

// Single source of truth lives in the shared review-ops layer so every surface
// validates operator-settable triage states identically.
const TRIAGE_OPERATOR_STATES: &[&str] = roger_review_ops::TRIAGE_OPERATOR_STATES;

fn handle_triage(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    let store = match open_store_or_response(runtime, "rr triage") {
        Ok(store) => store,
        Err(response) => return response,
    };

    // Required-argument validation lives here (not in parse_args) so that under
    // --robot a bad or missing argument emits a blocked JSON envelope, matching
    // rr draft/approve/post conformance instead of plain text + exit 2.
    if parsed.draft_finding_ids.is_empty() {
        return blocked_response(
            "rr triage requires at least one --finding <id>".to_owned(),
            vec!["pass --finding <id> one or more times".to_owned()],
            json!({
                "reason_code": "finding_selection_required",
            }),
        );
    }
    let Some(triage_state_arg) = parsed.triage_state.as_deref() else {
        return blocked_response(
            "rr triage requires --state <accepted|ignored|needs_follow_up|resolved>".to_owned(),
            vec![format!(
                "pass --state with one of: {}",
                TRIAGE_OPERATOR_STATES.join(", ")
            )],
            json!({
                "reason_code": "triage_state_required",
                "supported_states": TRIAGE_OPERATOR_STATES,
            }),
        );
    };
    if !TRIAGE_OPERATOR_STATES.contains(&triage_state_arg) {
        return blocked_response(
            format!(
                "unsupported --state: {triage_state_arg} (expected accepted, ignored, needs_follow_up, or resolved; new and stale are Roger-derived triage states and cannot be set by the operator)"
            ),
            vec![format!(
                "pass --state with one of: {}",
                TRIAGE_OPERATOR_STATES.join(", ")
            )],
            json!({
                "reason_code": "unsupported_triage_state",
                "requested_state": triage_state_arg,
                "supported_states": TRIAGE_OPERATOR_STATES,
            }),
        );
    }

    let binding_context = LaunchBindingContext::for_cwd(&runtime.cwd);
    let repository = resolve_repository(parsed.repo.clone(), &runtime.cwd);
    let resolution = match store.resolve_session_reentry_with_context(
        ResolveSessionReentry {
            explicit_session_id: parsed.session_id.clone(),
            repository,
            pull_request_number: parsed.pr,
            source_surface: LaunchSurface::Cli,
            ui_target: Some(cli_config::UI_TARGET.to_owned()),
            instance_preference: Some(cli_config::INSTANCE_PREFERENCE.to_owned()),
        },
        binding_context.storage_local_root(),
    ) {
        Ok(resolution) => resolution,
        Err(err) => return error_response(format!("failed to resolve triage context: {err}")),
    };

    let (session, _binding) = match resolution {
        SessionReentryResolution::Resolved { session, binding } => (session, binding),
        SessionReentryResolution::PickerRequired { reason, candidates } => {
            return blocked_picker_response(reason, candidates);
        }
    };

    // Validated against TRIAGE_OPERATOR_STATES at the top of this handler;
    // new/stale are Roger-derived states and are rejected before this point.
    let triage_state = match parsed.triage_state.as_deref() {
        Some(state) => state,
        None => {
            return error_response(
                "rr triage reached its handler without a validated --state value".to_owned(),
            );
        }
    };

    // Domain triage application (finding existence + session binding + update
    // + decision event) lives in the shared review-ops layer so the TUI and
    // bridge enforce the same fail-closed rules.
    let updated_findings = match roger_review_ops::set_finding_triage(
        &store,
        &session,
        &parsed.draft_finding_ids,
        triage_state,
    ) {
        Ok(outcome) => outcome.updated_findings,
        Err(roger_review_ops::SetTriageRejection::UnknownFindingIds {
            unknown_finding_ids,
        }) => {
            return blocked_response(
                "rr triage could not bind every requested finding to the resolved session"
                    .to_owned(),
                vec![format!(
                    "inspect rr findings --session {} --robot for the current finding ids",
                    session.id
                )],
                json!({
                    "reason_code": "unknown_finding_ids",
                    "session_id": session.id,
                    "unknown_finding_ids": unknown_finding_ids,
                }),
            );
        }
        Err(roger_review_ops::SetTriageRejection::UnsupportedState { requested_state }) => {
            // Pre-validated at the top of this handler; kept fail-closed for parity.
            return blocked_response(
                format!(
                    "unsupported --state: {requested_state} (expected accepted, ignored, needs_follow_up, or resolved; new and stale are Roger-derived triage states and cannot be set by the operator)"
                ),
                vec![format!(
                    "pass --state with one of: {}",
                    TRIAGE_OPERATOR_STATES.join(", ")
                )],
                json!({
                    "reason_code": "unsupported_triage_state",
                    "requested_state": requested_state,
                    "supported_states": TRIAGE_OPERATOR_STATES,
                }),
            );
        }
        Err(roger_review_ops::SetTriageRejection::Failed(message)) => {
            return error_response(message);
        }
    };

    let warnings = provider_support_warning(&session.provider, "rr triage")
        .into_iter()
        .collect::<Vec<_>>();

    let lines = updated_findings
        .iter()
        .map(|finding| {
            format!(
                "{} triage_state={} outbound_state={} row_version={} {}",
                finding.id,
                finding.triage_state,
                finding.outbound_state,
                finding.row_version,
                finding.title
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    CommandResponse {
        outcome: OutcomeKind::Complete,
        data: json!({
            "session_id": session.id.clone(),
            "triage_state": triage_state,
            "count": updated_findings.len(),
            "items": updated_findings
                .iter()
                .map(|finding| {
                    json!({
                        "id": finding.id.clone(),
                        "title": finding.title.clone(),
                        "triage_state": finding.triage_state.clone(),
                        "outbound_state": finding.outbound_state.clone(),
                        "row_version": finding.row_version,
                    })
                })
                .collect::<Vec<_>>(),
            "mutation_guard": {
                "github_posture": "blocked",
                "local_only": true,
            },
            "queryable_surfaces": {
                "findings_command": format!("rr findings --session {} --robot", session.id),
                "status_command": format!("rr status --session {}", session.id),
            },
        }),
        warnings,
        repair_actions: Vec::new(),
        message: lines,
    }
}

/// Render a shared session-precondition block (attention/target/no-run) into
/// the exact command-specific blocked envelope. The domain decision (which
/// precondition failed) lives in the shared review-ops layer; only the surface
/// wording differs across `rr draft`/`rr approve`/`rr post`.
fn session_precondition_response(
    command: &str,
    stale_reason_tail: &str,
    target_reason_tail: &str,
    session: &ReviewSessionRecord,
    block: roger_review_ops::SessionPreconditionBlock,
) -> CommandResponse {
    use roger_review_ops::SessionPreconditionBlock;
    match block {
        SessionPreconditionBlock::StaleLocalState => {
            let reconciliation = json!({
                "mode": "persisted_readback",
                "manual_refresh_supported": false,
                "stale_target_detected": true,
                "repair_required": true,
                "freshness_basis": "persisted_attention_state",
                "attention_updated_at": session.updated_at,
                "recommended_reentry_command": format!("rr resume --session {}", session.id),
                "recommended_fresh_pass_command": format!(
                    "rr review --repo {} --pr {}",
                    session.review_target.repository, session.review_target.pull_request_number
                ),
            });
            blocked_response(
                format!(
                    "rr {command} is blocked because the persisted review state requires explicit reconciliation before {stale_reason_tail}"
                ),
                vec![
                    format!(
                        "run rr resume --session {} to reopen the Roger session locally",
                        session.id
                    ),
                    format!(
                        "run rr review --repo {} --pr {} to start a fresh pass if target drift invalidated the older review",
                        session.review_target.repository, session.review_target.pull_request_number
                    ),
                ],
                json!({
                    "reason_code": "stale_local_state",
                    "session_id": session.id,
                    "attention_state": session.attention_state,
                    "reconciliation": reconciliation,
                }),
            )
        }
        SessionPreconditionBlock::MissingReviewTarget => blocked_response(
            format!(
                "rr {command} requires a concrete review target before Roger can {target_reason_tail}"
            ),
            vec![
                "re-run rr review --repo <owner/repo> --pr <number> to capture a real target"
                    .to_owned(),
                format!("or inspect rr status --session {} --robot", session.id),
            ],
            json!({
                "reason_code": "missing_review_target",
                "session_id": session.id,
                "review_target": session.review_target,
            }),
        ),
        SessionPreconditionBlock::MissingLocalStateNoRun => blocked_response(
            format!("rr {command} requires persisted local review state for the selected target"),
            vec![format!(
                "run rr review --repo {} --pr {} to materialize a local review first",
                session.review_target.repository, session.review_target.pull_request_number
            )],
            json!({
                "reason_code": "missing_local_state",
                "session_id": session.id,
            }),
        ),
    }
}

/// Serialize a shared draft-selection issue into the exact `rr draft` blocked
/// envelope shape.
fn draft_selection_issue_json(issue: &roger_review_ops::DraftSelectionIssue) -> Value {
    use roger_review_ops::DraftSelectionIssue;
    match issue {
        DraftSelectionIssue::TriageStateNotAccepted {
            finding_id,
            triage_state,
        } => json!({
            "finding_id": finding_id.clone(),
            "reason_code": "triage_state_not_accepted",
            "triage_state": triage_state.clone(),
        }),
        DraftSelectionIssue::ExistingOutboundState {
            finding_id,
            current_outbound_state,
            draft_id,
            draft_batch_id,
            approval_id,
            posted_action_id,
        } => json!({
            "finding_id": finding_id.clone(),
            "reason_code": "existing_outbound_state",
            "current_outbound_state": current_outbound_state.clone(),
            "draft_id": draft_id.clone(),
            "draft_batch_id": draft_batch_id.clone(),
            "approval_id": approval_id.clone(),
            "posted_action_id": posted_action_id.clone(),
        }),
    }
}

fn handle_draft(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    let store = match open_store_or_response(runtime, "rr draft") {
        Ok(store) => store,
        Err(response) => return response,
    };

    let binding_context = LaunchBindingContext::for_cwd(&runtime.cwd);
    let repository = resolve_repository(parsed.repo.clone(), &runtime.cwd);
    let resolution = match store.resolve_session_reentry_with_context(
        ResolveSessionReentry {
            explicit_session_id: parsed.session_id.clone(),
            repository,
            pull_request_number: parsed.pr,
            source_surface: LaunchSurface::Cli,
            ui_target: Some(cli_config::UI_TARGET.to_owned()),
            instance_preference: Some(cli_config::INSTANCE_PREFERENCE.to_owned()),
        },
        binding_context.storage_local_root(),
    ) {
        Ok(resolution) => resolution,
        Err(err) => return error_response(format!("failed to resolve draft context: {err}")),
    };

    let (session, binding) = match resolution {
        SessionReentryResolution::Resolved { session, binding } => (session, binding),
        SessionReentryResolution::PickerRequired { reason, candidates } => {
            return blocked_picker_response(reason, candidates);
        }
    };

    // All draft-materialization domain rules (session preconditions, accepted-only
    // enforcement, not-drafted gating, target binding, batch/item digests, and the
    // stored batch + item rows) live in the shared review-ops layer so the TUI and
    // bridge materialize drafts through the same fail-closed path.
    let selection = if parsed.draft_all_findings {
        roger_review_ops::DraftSelection::AllFindings
    } else {
        roger_review_ops::DraftSelection::Explicit(parsed.draft_finding_ids.clone())
    };
    let draft_outcome = match roger_review_ops::materialize_draft_batch(
        &store, &session, &selection,
    ) {
        Ok(outcome) => outcome,
        Err(roger_review_ops::MaterializeDraftRejection::Precondition(block)) => {
            return session_precondition_response(
                "draft",
                "outbound material can be derived",
                "bind local outbound state",
                &session,
                block,
            );
        }
        Err(roger_review_ops::MaterializeDraftRejection::MissingFindings { review_run_id }) => {
            return blocked_response(
                "rr draft requires persisted findings from the latest local review run".to_owned(),
                vec![format!(
                    "run rr review --repo {} --pr {} to materialize findings before drafting",
                    session.review_target.repository, session.review_target.pull_request_number
                )],
                json!({
                    "reason_code": "missing_local_state",
                    "session_id": session.id,
                    "review_run_id": review_run_id,
                }),
            );
        }
        Err(roger_review_ops::MaterializeDraftRejection::FindingSelectionRequired {
            review_run_id,
            available_finding_ids,
        }) => {
            return blocked_response(
                "rr draft requires explicit finding selection in this slice".to_owned(),
                vec![
                    "pass --finding <id> one or more times".to_owned(),
                    "or pass --all-findings to group every finding in the latest run".to_owned(),
                ],
                json!({
                    "reason_code": "finding_selection_required",
                    "session_id": session.id,
                    "review_run_id": review_run_id,
                    "available_finding_ids": available_finding_ids,
                }),
            );
        }
        Err(roger_review_ops::MaterializeDraftRejection::MissingFindingSelection {
            review_run_id,
            missing_finding_ids,
        }) => {
            return blocked_response(
                "rr draft could not find every requested finding in the latest local run"
                    .to_owned(),
                vec![format!(
                    "inspect rr findings --session {} --robot for the current finding ids",
                    session.id
                )],
                json!({
                    "reason_code": "missing_local_state",
                    "session_id": session.id,
                    "review_run_id": review_run_id,
                    "missing_finding_ids": missing_finding_ids,
                }),
            );
        }
        Err(roger_review_ops::MaterializeDraftRejection::SelectionNotDraftable {
            review_run_id,
            issues,
        }) => {
            let selection_issues = issues
                .iter()
                .map(draft_selection_issue_json)
                .collect::<Vec<_>>();
            return blocked_response(
                "selected findings cannot be drafted from the current local state".to_owned(),
                vec![
                    format!(
                        "inspect rr findings --session {} --robot to review triage and outbound state",
                        session.id
                    ),
                    "choose only Accepted findings whose outbound state is still not_drafted"
                        .to_owned(),
                ],
                json!({
                    "reason_code": "stale_local_state",
                    "session_id": session.id,
                    "review_run_id": review_run_id,
                    "selection_issues": selection_issues,
                }),
            );
        }
        Err(roger_review_ops::MaterializeDraftRejection::Failed(message)) => {
            return error_response(message);
        }
    };

    let review_run_id = draft_outcome.review_run_id.clone();
    let selection_mode = draft_outcome.selection_mode;
    let selected_finding_ids = draft_outcome.selected_finding_ids.clone();
    let batch = draft_outcome.batch;
    let stored_drafts = draft_outcome.drafts;
    let draft_previews = draft_outcome.previews;

    let state_counts = match store.outbound_state_counts_for_run(&session.id, &review_run_id) {
        Ok(counts) => counts,
        Err(err) => {
            return error_response(format!(
                "failed to project outbound approval state after drafting: {err}"
            ));
        }
    };

    let routine_surface = runtime_routine_surface_projection(
        runtime,
        &session.provider,
        binding
            .as_ref()
            .and_then(|entry| entry.worktree_root.as_deref()),
    );
    let provider_capability = runtime_provider_capability(runtime, &session.provider);
    let warnings = match session.provider.as_str() {
        "opencode" => Vec::new(),
        "codex" | "gemini" | "claude" => vec![format!(
            "provider '{}' has bounded support (tier-a start/reseed/raw-capture only); 'rr draft' does not support locator reopen or rr return",
            session.provider
        )],
        _ => vec![format!(
            "provider '{}' has bounded support (tier-a); 'rr draft' may offer reduced continuity behavior",
            session.provider
        )],
    };

    CommandResponse {
        outcome: OutcomeKind::Complete,
        data: json!({
            "session_id": session.id.clone(),
            "review_run_id": review_run_id.clone(),
            "selection": {
                "mode": selection_mode,
                "grouped": stored_drafts.len() > 1,
                "finding_ids": selected_finding_ids.clone(),
                "count": stored_drafts.len(),
            },
            "target": {
                "provider": "github",
                "repository": session.review_target.repository.clone(),
                "pull_request": session.review_target.pull_request_number,
                "repo_id": batch.repo_id.clone(),
                "remote_review_target_id": batch.remote_review_target_id.clone(),
            },
            "draft_batch": {
                "id": batch.id.clone(),
                "approval_state": "drafted",
                "payload_digest": batch.payload_digest.clone(),
                "target_tuple_json": outbound_target_tuple_json(&batch),
                "draft_count": stored_drafts.len(),
            },
            "drafts": stored_drafts
                .iter()
                .zip(draft_previews.iter())
                .map(|(draft, preview)| {
                    json!({
                        "id": draft.id.clone(),
                        "finding_id": draft.finding_id.clone(),
                        "fingerprint": preview.fingerprint.clone(),
                        "title": preview.title.clone(),
                        "summary": preview.normalized_summary.clone(),
                        "target_locator": draft.target_locator.clone(),
                        "anchor_digest": draft.anchor_digest.clone(),
                        "payload_digest": draft.payload_digest.clone(),
                        "body": draft.body.clone(),
                        "approval_state": "drafted",
                    })
                })
                .collect::<Vec<_>>(),
            "mutation_guard": {
                "github_posture": "blocked",
                "approval_required": true,
                "posted": false,
            },
            "queryable_surfaces": {
                "status_command": format!("rr status --session {}", session.id),
                "findings_command": format!("rr findings --session {} --robot", session.id),
                "approve_command": format!("rr approve --session {} --batch {}", session.id, batch.id),
                "outbound_state_counts": {
                    "not_drafted": state_counts.not_drafted,
                    "awaiting_approval": state_counts.awaiting_approval,
                    "approved": state_counts.approved,
                    "invalidated": state_counts.invalidated,
                    "posted": state_counts.posted,
                    "failed": state_counts.failed,
                },
            },
            "provider_capability": provider_capability,
            "routine_surface": routine_surface,
        }),
        warnings,
        repair_actions: Vec::new(),
        message: format!(
            "materialized {} local outbound draft{}",
            stored_drafts.len(),
            if stored_drafts.len() == 1 { "" } else { "s" }
        ),
    }
}

/// Open `$VISUAL`/`$EDITOR`/`vi` on a temp file seeded with the current draft
/// body and return the edited text. Interactive-only: a non-terminal caller is
/// told to use `--body-file` instead (the editor path cannot be driven headless).
fn edit_body_via_editor(seed: &str) -> std::result::Result<String, String> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(
            "rr send edit --editor requires an interactive terminal; use --body-file <path> in non-interactive contexts"
                .to_owned(),
        );
    }
    let editor = std::env::var("VISUAL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            std::env::var("EDITOR")
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or_else(|| "vi".to_owned());

    let mut path = std::env::temp_dir();
    path.push(format!(
        "rr-send-edit-{}-{}.md",
        std::process::id(),
        time::now_ts()
    ));
    fs::write(&path, seed).map_err(|err| format!("failed to seed editor temp file: {err}"))?;

    let status = ProcessCommand::new(&editor)
        .arg(&path)
        .status()
        .map_err(|err| format!("failed to launch editor '{editor}': {err}"));
    let status = match status {
        Ok(status) => status,
        Err(err) => {
            let _ = fs::remove_file(&path);
            return Err(err);
        }
    };
    if !status.success() {
        let _ = fs::remove_file(&path);
        return Err(format!(
            "editor '{editor}' exited without success; the draft was not revised"
        ));
    }
    let edited =
        fs::read_to_string(&path).map_err(|err| format!("failed to read edited draft body: {err}"));
    let _ = fs::remove_file(&path);
    edited
}

/// `rr send edit --draft <id> (--body-file <path> | --editor)`: revise a local
/// outbound draft body. Fail-closed like the other outbound verbs: a missing
/// draft, an empty body, or a posted batch is rejected; editing an approved
/// batch revokes its approval (via the storage layer) and forces re-approval.
fn handle_edit(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    let store = match open_store_or_response(runtime, "rr send edit") {
        Ok(store) => store,
        Err(response) => return response,
    };

    // parse_args guarantees --draft is present; stay fail-closed regardless.
    let Some(draft_id) = parsed.edit_draft_id.as_deref() else {
        return error_response("rr send edit requires --draft <id>".to_owned());
    };

    // Existence + batch binding. The canonical draft row body mirrors the
    // newest revision, so this also gives us the current body for the compare.
    let draft_item = match store.outbound_draft_item(draft_id) {
        Ok(item) => item,
        Err(err) => return error_response(format!("failed to load outbound draft item: {err}")),
    };
    let Some(draft_item) = draft_item else {
        return blocked_response(
            "rr send edit could not find the requested outbound draft".to_owned(),
            vec![
                "inspect rr findings --session <id> --robot to find outbound draft ids".to_owned(),
            ],
            json!({
                "reason_code": "missing_local_state",
                "draft_id": draft_id,
            }),
        );
    };
    let batch_id = draft_item.draft_batch_id.clone();

    let current_body = match store.current_outbound_draft_body(draft_id) {
        Ok(Some(body)) => body,
        Ok(None) => draft_item.body.clone(),
        Err(err) => return error_response(format!("failed to load current draft body: {err}")),
    };

    // Resolve the replacement body from --body-file or --editor (parse enforces
    // exactly one).
    let new_body = if let Some(path) = parsed.edit_body_file.as_ref() {
        match fs::read_to_string(path) {
            Ok(body) => body,
            Err(err) => {
                return error_response(format!(
                    "failed to read --body-file {}: {err}",
                    path.display()
                ));
            }
        }
    } else {
        match edit_body_via_editor(&current_body) {
            Ok(body) => body,
            Err(err) => return error_response(err),
        }
    };

    if new_body.trim().is_empty() {
        return blocked_response(
            "rr send edit rejected an empty draft body; provide non-empty replacement text"
                .to_owned(),
            vec![format!(
                "re-run rr send edit --draft {draft_id} --body-file <path> with non-empty content"
            )],
            json!({
                "reason_code": "empty_draft_body",
                "draft_id": draft_id,
                "draft_batch_id": batch_id,
            }),
        );
    }

    if new_body == current_body {
        return CommandResponse {
            outcome: OutcomeKind::Complete,
            data: json!({
                "draft_id": draft_id,
                "draft_batch_id": batch_id,
                "revised": false,
            }),
            warnings: Vec::new(),
            repair_actions: Vec::new(),
            message:
                "rr send edit made no change: the new body is identical to the current draft body (no revision written)"
                    .to_owned(),
        };
    }

    let revision = match store.revise_outbound_draft_body(
        draft_id,
        &new_body,
        roger_storage::RevisionAuthorKind::Operator,
    ) {
        Ok(revision) => revision,
        Err(StorageError::Conflict { entity, id })
            if entity == "outbound_draft_revision"
                && id.ends_with(":posted_batch_edit_rejected") =>
        {
            return blocked_response(
                "rr send edit refused to edit a draft whose batch was already posted to GitHub"
                    .to_owned(),
                vec![
                    "posted batches are immutable; start a fresh review pass if new outbound comms are needed"
                        .to_owned(),
                    format!("inspect rr status --batch {batch_id} --robot for the posting state"),
                ],
                json!({
                    "reason_code": "posted_batch_edit_rejected",
                    "draft_id": draft_id,
                    "draft_batch_id": batch_id,
                }),
            );
        }
        Err(err) => return error_response(format!("failed to revise outbound draft body: {err}")),
    };

    let revocation_reason = match store.outbound_approval_revocation_reason(&batch_id) {
        Ok(reason) => reason,
        Err(err) => {
            return error_response(format!(
                "failed to inspect approval revocation reason: {err}"
            ));
        }
    };
    let approval_revoked = revocation_reason.as_deref()
        == Some(roger_storage::OUTBOUND_APPROVAL_REVOKED_REASON_DRAFT_REVISED);

    let approve_command = format!("rr send approve --batch {batch_id}");
    let repair_actions = if approval_revoked {
        vec![format!(
            "editing revoked the batch approval; re-run {approve_command} to re-approve before posting"
        )]
    } else {
        vec![format!(
            "run {approve_command} when ready to approve the revised batch"
        )]
    };

    let message = if approval_revoked {
        format!(
            "rr send edit recorded revision {} and revoked the batch approval (reason {}); re-approval is required before posting",
            revision.revision_index,
            roger_storage::OUTBOUND_APPROVAL_REVOKED_REASON_DRAFT_REVISED
        )
    } else {
        format!(
            "rr send edit recorded revision {} for the outbound draft",
            revision.revision_index
        )
    };

    CommandResponse {
        outcome: OutcomeKind::Complete,
        data: json!({
            "draft_id": draft_id,
            "draft_batch_id": batch_id,
            "revised": true,
            "revision_index": revision.revision_index,
            "revision_id": revision.id,
            "author_kind": revision.author_kind.as_str(),
            "approval_revoked": approval_revoked,
            "approval_revocation_reason": revocation_reason,
            "queryable_surfaces": {
                "approve_command": approve_command,
                "findings_command": "rr findings --robot",
            },
        }),
        warnings: Vec::new(),
        repair_actions,
        message,
    }
}

fn linkage_issues_json(issues: &[(Option<String>, String)]) -> Vec<Value> {
    issues
        .iter()
        .map(|(draft_id, reason_code)| {
            json!({
                "draft_id": draft_id.clone(),
                "reason_code": reason_code.clone(),
            })
        })
        .collect()
}

fn draft_state_issues_json(issues: &[roger_review_ops::DraftStateIssue]) -> Vec<Value> {
    issues
        .iter()
        .map(|issue| {
            json!({
                "draft_id": issue.draft_id.clone(),
                "finding_id": issue.finding_id.clone(),
                "reason_code": issue.reason_code.clone(),
            })
        })
        .collect()
}

/// Render an `approve_batch` rejection into the exact `rr approve` blocked/error
/// envelope. The fail-closed decision lives in the shared review-ops layer; this
/// only renders it for the CLI surface.
fn approve_rejection_response(
    session: &ReviewSessionRecord,
    rejection: roger_review_ops::ApproveRejection,
) -> CommandResponse {
    use roger_review_ops::ApproveRejection;
    match rejection {
        ApproveRejection::Precondition(block) => session_precondition_response(
            "approve",
            "approval can be granted",
            "bind local approval state",
            session,
            block,
        ),
        ApproveRejection::BatchSelectionRequired {
            review_run_id,
            available_batch_ids,
        } => blocked_response(
            "rr approve requires an explicit draft batch id in this slice".to_owned(),
            vec![
                format!(
                    "inspect rr findings --session {} --robot to find awaiting_approval draft batches",
                    session.id
                ),
                "re-run rr approve --batch <draft-batch-id> once you select the exact stored batch"
                    .to_owned(),
            ],
            json!({
                "reason_code": "draft_batch_selection_required",
                "session_id": session.id,
                "review_run_id": review_run_id,
                "available_batch_ids": available_batch_ids,
            }),
        ),
        ApproveRejection::BatchNotFound {
            review_run_id,
            draft_batch_id,
            available_batch_ids,
        } => blocked_response(
            "rr approve could not find the requested local draft batch".to_owned(),
            vec![
                format!(
                    "inspect rr findings --session {} --robot to find the current awaiting_approval batch ids",
                    session.id
                ),
                format!(
                    "re-run rr draft --session {} --finding <finding-id> if the older batch was superseded",
                    session.id
                ),
            ],
            json!({
                "reason_code": "missing_local_state",
                "session_id": session.id,
                "review_run_id": review_run_id,
                "draft_batch_id": draft_batch_id,
                "available_batch_ids": available_batch_ids,
            }),
        ),
        ApproveRejection::SessionMismatch {
            review_run_id,
            draft_batch_id,
            batch_review_session_id,
        } => blocked_response(
            "rr approve refused to bind approval because the requested batch belongs to a different Roger session".to_owned(),
            vec![
                format!("inspect rr status --session {} --robot", session.id),
                "use the batch id returned by rr draft for this exact session".to_owned(),
            ],
            json!({
                "reason_code": "approval_invalidated:local_state_drift",
                "session_id": session.id,
                "review_run_id": review_run_id,
                "draft_batch_id": draft_batch_id,
                "batch_review_session_id": batch_review_session_id,
            }),
        ),
        ApproveRejection::RunMismatch {
            latest_review_run_id,
            draft_batch_id,
            batch_review_run_id,
        } => blocked_response(
            "rr approve is blocked because the requested batch does not belong to the latest persisted review run".to_owned(),
            vec![
                format!("inspect rr findings --session {} --robot for the current run state", session.id),
                format!(
                    "re-run rr draft --session {} --finding <finding-id> after reconciling the newer local run",
                    session.id
                ),
            ],
            json!({
                "reason_code": "approval_invalidated:local_state_drift",
                "session_id": session.id,
                "latest_review_run_id": latest_review_run_id,
                "draft_batch_id": draft_batch_id,
                "batch_review_run_id": batch_review_run_id,
            }),
        ),
        ApproveRejection::TargetDrift {
            draft_batch_id,
            expected_repo_id,
            expected_remote_review_target_id,
            stored_repo_id,
            stored_remote_review_target_id,
        } => blocked_response(
            "rr approve is blocked because the stored batch target no longer matches the active Roger review target".to_owned(),
            vec![
                format!("inspect rr status --session {} --robot", session.id),
                format!(
                    "re-run rr draft --session {} --finding <finding-id> after reconciling target drift",
                    session.id
                ),
            ],
            json!({
                "reason_code": "approval_invalidated:target_drift",
                "session_id": session.id,
                "draft_batch_id": draft_batch_id,
                "expected_repo_id": expected_repo_id,
                "expected_remote_review_target_id": expected_remote_review_target_id,
                "stored_repo_id": stored_repo_id,
                "stored_remote_review_target_id": stored_remote_review_target_id,
            }),
        ),
        ApproveRejection::ExistingPostedAction {
            draft_batch_id,
            posted_action_id,
            posted_action_status,
            failure_code,
        } => blocked_response(
            "rr approve is no longer available because Roger already recorded a post attempt for this batch".to_owned(),
            vec![format!(
                "inspect rr status --session {} --robot for the current outbound posting state",
                session.id
            )],
            json!({
                "reason_code": "existing_posted_action",
                "session_id": session.id,
                "draft_batch_id": draft_batch_id,
                "posted_action_id": posted_action_id,
                "posted_action_status": posted_action_status,
                "failure_code": failure_code,
            }),
        ),
        ApproveRejection::MissingDraftItems { draft_batch_id } => blocked_response(
            "rr approve requires persisted local draft items for the selected batch".to_owned(),
            vec![format!(
                "re-run rr draft --session {} --finding <finding-id> to materialize the batch again",
                session.id
            )],
            json!({
                "reason_code": "missing_local_state",
                "session_id": session.id,
                "draft_batch_id": draft_batch_id,
            }),
        ),
        ApproveRejection::LinkageInvalid {
            draft_batch_id,
            reason_suffix,
            issues,
        } => blocked_response(
            "rr approve refused to bind approval because the stored draft batch no longer matches its payload or target evidence".to_owned(),
            vec![
                format!(
                    "inspect rr findings --session {} --robot to review the current outbound state",
                    session.id
                ),
                format!(
                    "re-run rr draft --session {} --finding <finding-id> to materialize a fresh batch after drift",
                    session.id
                ),
            ],
            json!({
                "reason_code": format!("approval_invalidated:{reason_suffix}"),
                "session_id": session.id,
                "draft_batch_id": draft_batch_id,
                "validation_issues": linkage_issues_json(&issues),
            }),
        ),
        ApproveRejection::BatchInvalidated {
            draft_batch_id,
            invalidation_reason_code,
            invalidated_at,
        } => blocked_response(
            "rr approve is blocked because the stored batch was already invalidated by target or local-state drift".to_owned(),
            vec![
                format!(
                    "inspect rr findings --session {} --robot to review the invalidation state",
                    session.id
                ),
                format!(
                    "re-run rr draft --session {} --finding <finding-id> after reconciling the newer local state",
                    session.id
                ),
            ],
            json!({
                "reason_code": format!(
                    "approval_invalidated:{}",
                    invalidation_reason_code.clone().unwrap_or_else(|| "unspecified".to_owned())
                ),
                "session_id": session.id,
                "draft_batch_id": draft_batch_id,
                "approval_state": "invalidated",
                "invalidation_reason_code": invalidation_reason_code,
                "invalidated_at": invalidated_at,
            }),
        ),
        ApproveRejection::DraftStateNotApprovable {
            draft_batch_id,
            issues,
        } => blocked_response(
            "rr approve is blocked because the stored draft items are no longer all in an approvable state".to_owned(),
            vec![format!(
                "inspect rr findings --session {} --robot to review the current outbound state",
                session.id
            )],
            json!({
                "reason_code": "stale_local_state",
                "session_id": session.id,
                "draft_batch_id": draft_batch_id,
                "draft_state_issues": draft_state_issues_json(&issues),
            }),
        ),
        ApproveRejection::ApprovalRevoked {
            draft_batch_id,
            approval_id,
            revoked_at,
        } => blocked_response(
            "rr approve is blocked because the stored approval token was already revoked".to_owned(),
            vec![format!(
                "re-run rr draft --session {} --finding <finding-id> after reviewing the revoked batch state",
                session.id
            )],
            json!({
                "reason_code": "approval_revoked",
                "session_id": session.id,
                "draft_batch_id": draft_batch_id,
                "approval_id": approval_id,
                "revoked_at": revoked_at,
            }),
        ),
        ApproveRejection::ApprovalPayloadDigestMismatch {
            draft_batch_id,
            approval_id,
            expected_payload_digest,
            stored_payload_digest,
        } => blocked_response(
            "rr approve refused to reuse the stored approval token because its payload digest no longer matches the batch".to_owned(),
            vec![format!(
                "re-run rr draft --session {} --finding <finding-id> to materialize a fresh batch",
                session.id
            )],
            json!({
                "reason_code": "approval_payload_digest_mismatch",
                "session_id": session.id,
                "draft_batch_id": draft_batch_id,
                "approval_id": approval_id,
                "expected_payload_digest": expected_payload_digest,
                "stored_payload_digest": stored_payload_digest,
            }),
        ),
        ApproveRejection::ApprovalTargetTupleMismatch {
            draft_batch_id,
            approval_id,
            expected_target_tuple_json,
            stored_target_tuple_json,
        } => blocked_response(
            "rr approve refused to reuse the stored approval token because its target tuple no longer matches the batch".to_owned(),
            vec![format!(
                "re-run rr draft --session {} --finding <finding-id> after reconciling target drift",
                session.id
            )],
            json!({
                "reason_code": "approval_target_tuple_mismatch",
                "session_id": session.id,
                "draft_batch_id": draft_batch_id,
                "approval_id": approval_id,
                "expected_target_tuple_json": expected_target_tuple_json,
                "stored_target_tuple_json": stored_target_tuple_json,
            }),
        ),
        ApproveRejection::BatchNotApprovable {
            draft_batch_id,
            approval_state,
        } => blocked_response(
            "rr approve is blocked because the stored batch is no longer in an approvable state"
                .to_owned(),
            vec![format!(
                "inspect rr status --session {} --robot for the current outbound state",
                session.id
            )],
            json!({
                "reason_code": "stale_local_state",
                "session_id": session.id,
                "draft_batch_id": draft_batch_id,
                "approval_state": approval_state,
            }),
        ),
        ApproveRejection::Failed(message) => error_response(message),
    }
}

fn handle_approve(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    let store = match open_store_or_response(runtime, "rr approve") {
        Ok(store) => store,
        Err(response) => return response,
    };

    let binding_context = LaunchBindingContext::for_cwd(&runtime.cwd);
    let repository = resolve_repository(parsed.repo.clone(), &runtime.cwd);
    let resolution = match store.resolve_session_reentry_with_context(
        ResolveSessionReentry {
            explicit_session_id: parsed.session_id.clone(),
            repository,
            pull_request_number: parsed.pr,
            source_surface: LaunchSurface::Cli,
            ui_target: Some(cli_config::UI_TARGET.to_owned()),
            instance_preference: Some(cli_config::INSTANCE_PREFERENCE.to_owned()),
        },
        binding_context.storage_local_root(),
    ) {
        Ok(resolution) => resolution,
        Err(err) => return error_response(format!("failed to resolve approval context: {err}")),
    };

    let (session, binding) = match resolution {
        SessionReentryResolution::Resolved { session, binding } => (session, binding),
        SessionReentryResolution::PickerRequired { reason, candidates } => {
            return blocked_picker_response(reason, candidates);
        }
    };

    // The full fail-closed approval domain rule (session preconditions, batch
    // selection/binding, target binding, prior-post guard, linkage validation,
    // invalidation guard, draft-state guard, revoked/digest/target token checks,
    // and the approval-token + batch writes) lives in the shared review-ops
    // layer so every surface approves through the same path.
    let approve_outcome =
        match roger_review_ops::approve_batch(&store, &session, parsed.batch_id.as_deref()) {
            Ok(outcome) => outcome,
            Err(rejection) => return approve_rejection_response(&session, rejection),
        };
    let review_run_id = approve_outcome.review_run_id.clone();
    let batch = approve_outcome.batch;
    let drafts = approve_outcome.drafts;
    let approval = approve_outcome.approval;
    let expected_target_tuple_json = approve_outcome.expected_target_tuple_json;
    let batch_already_approved = approve_outcome.batch_already_approved;
    let approval_created = approve_outcome.approval_created;

    let state_counts = match store.outbound_state_counts_for_run(&session.id, &review_run_id) {
        Ok(counts) => counts,
        Err(err) => {
            return error_response(format!(
                "failed to project outbound approval state after approving: {err}"
            ));
        }
    };

    let routine_surface = runtime_routine_surface_projection(
        runtime,
        &session.provider,
        binding
            .as_ref()
            .and_then(|entry| entry.worktree_root.as_deref()),
    );
    let provider_capability = runtime_provider_capability(runtime, &session.provider);
    let warnings: Vec<String> = provider_support_warning(&session.provider, "rr approve")
        .into_iter()
        .collect();

    CommandResponse {
        outcome: OutcomeKind::Complete,
        data: json!({
            "session_id": session.id.clone(),
            "review_run_id": review_run_id.clone(),
            "target": {
                "provider": "github",
                "repository": session.review_target.repository.clone(),
                "pull_request": session.review_target.pull_request_number,
                "repo_id": batch.repo_id.clone(),
                "remote_review_target_id": batch.remote_review_target_id.clone(),
            },
            "draft_batch": {
                "id": batch.id.clone(),
                "approval_state": "approved",
                "payload_digest": batch.payload_digest.clone(),
                "target_tuple_json": expected_target_tuple_json.clone(),
                "draft_count": drafts.len(),
                "approved_at": approval.approved_at,
            },
            "approval": {
                "id": approval.id.clone(),
                "payload_digest": approval.payload_digest.clone(),
                "target_tuple_json": approval.target_tuple_json.clone(),
                "approved_at": approval.approved_at,
                "already_recorded": batch_already_approved,
            },
            "drafts": drafts
                .iter()
                .map(|draft| {
                    json!({
                        "id": draft.id.clone(),
                        "finding_id": draft.finding_id.clone(),
                        "target_locator": draft.target_locator.clone(),
                        "payload_digest": draft.payload_digest.clone(),
                        "approval_state": "approved",
                    })
                })
                .collect::<Vec<_>>(),
            "mutation_guard": {
                "github_posture": "blocked",
                "approval_required": false,
                "posted": false,
            },
            "queryable_surfaces": {
                "status_command": format!("rr status --session {}", session.id),
                "findings_command": format!("rr findings --session {} --robot", session.id),
                "post_command": format!("rr post --session {} --batch {}", session.id, batch.id),
                "outbound_state_counts": {
                    "not_drafted": state_counts.not_drafted,
                    "awaiting_approval": state_counts.awaiting_approval,
                    "approved": state_counts.approved,
                    "invalidated": state_counts.invalidated,
                    "posted": state_counts.posted,
                    "failed": state_counts.failed,
                },
            },
            "provider_capability": provider_capability,
            "routine_surface": routine_surface,
        }),
        warnings,
        repair_actions: Vec::new(),
        message: if approval_created {
            "recorded local approval for the outbound draft batch".to_owned()
        } else {
            "approval already recorded for the outbound draft batch".to_owned()
        },
    }
}

/// Render a `post_batch` rejection into the exact `rr post` blocked/error
/// envelope. The fail-closed decision lives in the shared review-ops layer.
fn post_rejection_response(
    session: &ReviewSessionRecord,
    rejection: roger_review_ops::PostRejection,
) -> CommandResponse {
    use roger_review_ops::PostRejection;
    match rejection {
        PostRejection::Precondition(block) => session_precondition_response(
            "post",
            "GitHub mutation can run",
            "execute outbound mutation",
            session,
            block,
        ),
        PostRejection::BatchSelectionRequired {
            review_run_id,
            available_batch_ids,
        } => blocked_response(
            "rr post requires an explicit approved draft batch id in this slice".to_owned(),
            vec![
                format!(
                    "inspect rr findings --session {} --robot to find approved draft batches",
                    session.id
                ),
                "re-run rr post --batch <draft-batch-id> once you select the exact approved batch"
                    .to_owned(),
            ],
            json!({
                "reason_code": "approved_batch_selection_required",
                "session_id": session.id,
                "review_run_id": review_run_id,
                "available_batch_ids": available_batch_ids,
            }),
        ),
        PostRejection::BatchNotFound {
            review_run_id,
            draft_batch_id,
            available_batch_ids,
        } => blocked_response(
            "rr post could not find the requested approved draft batch".to_owned(),
            vec![
                format!(
                    "inspect rr findings --session {} --robot to find the current approved batch ids",
                    session.id
                ),
                format!(
                    "re-run rr draft --session {} --finding <finding-id> and rr approve if the older batch was superseded",
                    session.id
                ),
            ],
            json!({
                "reason_code": "missing_local_state",
                "session_id": session.id,
                "review_run_id": review_run_id,
                "draft_batch_id": draft_batch_id,
                "available_batch_ids": available_batch_ids,
            }),
        ),
        PostRejection::SessionMismatch {
            review_run_id,
            draft_batch_id,
            batch_review_session_id,
        } => blocked_response(
            "rr post refused to execute because the requested batch belongs to a different Roger session".to_owned(),
            vec![
                format!("inspect rr status --session {} --robot", session.id),
                "use the batch id returned by rr approve for this exact session".to_owned(),
            ],
            json!({
                "reason_code": "approval_invalidated:local_state_drift",
                "session_id": session.id,
                "review_run_id": review_run_id,
                "draft_batch_id": draft_batch_id,
                "batch_review_session_id": batch_review_session_id,
            }),
        ),
        PostRejection::RunMismatch {
            latest_review_run_id,
            draft_batch_id,
            batch_review_run_id,
        } => blocked_response(
            "rr post is blocked because the requested batch does not belong to the latest persisted review run".to_owned(),
            vec![
                format!(
                    "inspect rr findings --session {} --robot for the current run state",
                    session.id
                ),
                format!(
                    "re-run rr draft --session {} --finding <finding-id> and rr approve after reconciling the newer local run",
                    session.id
                ),
            ],
            json!({
                "reason_code": "approval_invalidated:local_state_drift",
                "session_id": session.id,
                "latest_review_run_id": latest_review_run_id,
                "draft_batch_id": draft_batch_id,
                "batch_review_run_id": batch_review_run_id,
            }),
        ),
        PostRejection::TargetDrift {
            draft_batch_id,
            expected_repo_id,
            expected_remote_review_target_id,
            stored_repo_id,
            stored_remote_review_target_id,
        } => blocked_response(
            "rr post is blocked because the stored batch target no longer matches the active Roger review target".to_owned(),
            vec![
                format!("inspect rr status --session {} --robot", session.id),
                format!(
                    "re-run rr draft --session {} --finding <finding-id> and rr approve after reconciling target drift",
                    session.id
                ),
            ],
            json!({
                "reason_code": "approval_invalidated:target_drift",
                "session_id": session.id,
                "draft_batch_id": draft_batch_id,
                "expected_repo_id": expected_repo_id,
                "expected_remote_review_target_id": expected_remote_review_target_id,
                "stored_repo_id": stored_repo_id,
                "stored_remote_review_target_id": stored_remote_review_target_id,
            }),
        ),
        PostRejection::ExistingPostedAction {
            draft_batch_id,
            posted_action_id,
            posted_action_status,
            failure_code,
        } => blocked_response(
            "rr post is blocked because Roger already recorded a post attempt for this batch"
                .to_owned(),
            vec![
                format!(
                    "inspect rr status --session {} --robot for the current outbound posting state",
                    session.id
                ),
                format!(
                    "inspect rr findings --session {} --robot to review the recorded posted action lineage",
                    session.id
                ),
            ],
            json!({
                "reason_code": "existing_posted_action",
                "session_id": session.id,
                "draft_batch_id": draft_batch_id,
                "posted_action_id": posted_action_id,
                "posted_action_status": posted_action_status,
                "failure_code": failure_code,
            }),
        ),
        PostRejection::MissingDraftItems { draft_batch_id } => blocked_response(
            "rr post requires persisted approved draft items for the selected batch".to_owned(),
            vec![format!(
                "re-run rr draft --session {} --finding <finding-id> and rr approve to materialize the batch again",
                session.id
            )],
            json!({
                "reason_code": "missing_local_state",
                "session_id": session.id,
                "draft_batch_id": draft_batch_id,
            }),
        ),
        PostRejection::LinkageInvalid {
            draft_batch_id,
            reason_suffix,
            issues,
        } => blocked_response(
            "rr post refused to execute because the stored draft batch no longer matches its payload or target evidence".to_owned(),
            vec![
                format!(
                    "inspect rr findings --session {} --robot to review the current outbound state",
                    session.id
                ),
                format!(
                    "re-run rr draft --session {} --finding <finding-id> and rr approve to materialize a fresh batch after drift",
                    session.id
                ),
            ],
            json!({
                "reason_code": format!("approval_invalidated:{reason_suffix}"),
                "session_id": session.id,
                "draft_batch_id": draft_batch_id,
                "validation_issues": linkage_issues_json(&issues),
            }),
        ),
        PostRejection::BatchInvalidated {
            draft_batch_id,
            invalidation_reason_code,
            invalidated_at,
        } => blocked_response(
            "rr post is blocked because the stored batch was already invalidated by target or local-state drift".to_owned(),
            vec![
                format!(
                    "inspect rr findings --session {} --robot to review the invalidation state",
                    session.id
                ),
                format!(
                    "re-run rr draft --session {} --finding <finding-id> and rr approve after reconciling the newer local state",
                    session.id
                ),
            ],
            json!({
                "reason_code": format!(
                    "approval_invalidated:{}",
                    invalidation_reason_code.clone().unwrap_or_else(|| "unspecified".to_owned())
                ),
                "session_id": session.id,
                "draft_batch_id": draft_batch_id,
                "approval_state": "invalidated",
                "invalidation_reason_code": invalidation_reason_code,
                "invalidated_at": invalidated_at,
            }),
        ),
        PostRejection::DraftStateNotPostable {
            draft_batch_id,
            issues,
        } => blocked_response(
            "rr post is blocked because the stored draft items are no longer all in an approved postable state".to_owned(),
            vec![format!(
                "inspect rr findings --session {} --robot to review the current outbound state",
                session.id
            )],
            json!({
                "reason_code": "stale_local_state",
                "session_id": session.id,
                "draft_batch_id": draft_batch_id,
                "draft_state_issues": draft_state_issues_json(&issues),
            }),
        ),
        PostRejection::ApprovalRequiredBatchState {
            draft_batch_id,
            approval_state,
        } => blocked_response(
            "rr post requires an approved batch before GitHub mutation can run".to_owned(),
            vec![
                format!(
                    "run rr approve --session {} --batch {} before posting",
                    session.id, draft_batch_id
                ),
                format!("inspect rr findings --session {} --robot", session.id),
            ],
            json!({
                "reason_code": "approval_required",
                "session_id": session.id,
                "draft_batch_id": draft_batch_id,
                "approval_state": approval_state,
            }),
        ),
        PostRejection::ApprovalRequiredNoToken { draft_batch_id } => blocked_response(
            "rr post requires an explicit local approval token for the selected batch".to_owned(),
            vec![
                format!(
                    "run rr approve --session {} --batch {} before posting",
                    session.id, draft_batch_id
                ),
                format!("inspect rr findings --session {} --robot", session.id),
            ],
            json!({
                "reason_code": "approval_required",
                "session_id": session.id,
                "draft_batch_id": draft_batch_id,
            }),
        ),
        PostRejection::ApprovalRevoked {
            draft_batch_id,
            approval_id,
            revoked_at,
        } => blocked_response(
            "rr post is blocked because the stored approval token was already revoked".to_owned(),
            vec![format!(
                "re-run rr draft --session {} --finding <finding-id> and rr approve after reviewing the revoked batch state",
                session.id
            )],
            json!({
                "reason_code": "approval_revoked",
                "session_id": session.id,
                "draft_batch_id": draft_batch_id,
                "approval_id": approval_id,
                "revoked_at": revoked_at,
            }),
        ),
        PostRejection::PostingBlocked {
            review_run_id,
            draft_batch_id,
            approval_id,
            reason_code,
            item_results,
            retry_draft_ids,
        } => blocked_response(
            "rr post refused to execute because the stored approval token or batch binding no longer passes exact-payload verification".to_owned(),
            vec![
                format!(
                    "inspect rr findings --session {} --robot to review the current outbound state",
                    session.id
                ),
                format!(
                    "re-run rr draft --session {} --finding <finding-id> and rr approve to materialize a fresh approved batch",
                    session.id
                ),
            ],
            json!({
                "reason_code": reason_code,
                "session_id": session.id,
                "review_run_id": review_run_id,
                "draft_batch_id": draft_batch_id,
                "approval_id": approval_id,
                "item_results": item_results,
                "retry_draft_ids": retry_draft_ids,
            }),
        ),
        PostRejection::Failed(message) => error_response(message),
    }
}

fn handle_post(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    let store = match open_store_or_response(runtime, "rr post") {
        Ok(store) => store,
        Err(response) => return response,
    };

    let binding_context = LaunchBindingContext::for_cwd(&runtime.cwd);
    let repository = resolve_repository(parsed.repo.clone(), &runtime.cwd);
    let resolution = match store.resolve_session_reentry_with_context(
        ResolveSessionReentry {
            explicit_session_id: parsed.session_id.clone(),
            repository,
            pull_request_number: parsed.pr,
            source_surface: LaunchSurface::Cli,
            ui_target: Some(cli_config::UI_TARGET.to_owned()),
            instance_preference: Some(cli_config::INSTANCE_PREFERENCE.to_owned()),
        },
        binding_context.storage_local_root(),
    ) {
        Ok(resolution) => resolution,
        Err(err) => return error_response(format!("failed to resolve posting context: {err}")),
    };

    let (session, binding) = match resolution {
        SessionReentryResolution::Resolved { session, binding } => (session, binding),
        SessionReentryResolution::PickerRequired { reason, candidates } => {
            return blocked_picker_response(reason, candidates);
        }
    };

    // The full fail-closed posting domain rule (session preconditions, approved-
    // batch selection/binding, target binding, prior-post guard, linkage
    // validation, invalidation guard, all-approved draft-state guard, batch-
    // approved guard, live approval-token guard, and the gated GitHub adapter
    // execution + posted-action persistence) lives in the shared review-ops
    // layer. The real GitHub CLI adapter is injected behind the same gate.
    let adapter = GhCliAdapter::new();
    let post_outcome = match roger_review_ops::post_batch(
        &store,
        &session,
        parsed.batch_id.as_deref(),
        &adapter,
    ) {
        Ok(outcome) => outcome,
        Err(rejection) => return post_rejection_response(&session, rejection),
    };
    let review_run_id = post_outcome.review_run_id.clone();
    let batch = post_outcome.batch;
    let drafts = post_outcome.drafts;
    let approval = post_outcome.approval;
    let posting_result = post_outcome.posting_result;

    let state_counts = match store.outbound_state_counts_for_run(&session.id, &review_run_id) {
        Ok(counts) => counts,
        Err(err) => {
            return error_response(format!(
                "failed to project outbound posting state after posting: {err}"
            ));
        }
    };

    let routine_surface = runtime_routine_surface_projection(
        runtime,
        &session.provider,
        binding
            .as_ref()
            .and_then(|entry| entry.worktree_root.as_deref()),
    );
    let provider_capability = runtime_provider_capability(runtime, &session.provider);
    let warnings: Vec<String> = provider_support_warning(&session.provider, "rr post")
        .into_iter()
        .collect();
    let posted_anything = matches!(
        &posting_result.outcome,
        ExplicitPostingOutcome::Posted | ExplicitPostingOutcome::Partial
    );
    let outcome = match &posting_result.outcome {
        ExplicitPostingOutcome::Posted => OutcomeKind::Complete,
        ExplicitPostingOutcome::Partial | ExplicitPostingOutcome::Failed => OutcomeKind::Degraded,
        ExplicitPostingOutcome::Blocked => OutcomeKind::Blocked,
    };
    let github_posture = match &posting_result.outcome {
        ExplicitPostingOutcome::Posted => "posted",
        ExplicitPostingOutcome::Partial => "partial_failure",
        ExplicitPostingOutcome::Failed => "failed",
        ExplicitPostingOutcome::Blocked => "blocked",
    };
    let repair_actions = match &posting_result.outcome {
        ExplicitPostingOutcome::Posted => Vec::new(),
        ExplicitPostingOutcome::Partial | ExplicitPostingOutcome::Failed => vec![
            format!(
                "inspect rr status --session {} --robot for the recorded outbound posting state",
                session.id
            ),
            format!(
                "inspect rr findings --session {} --robot to review which findings now project as failed",
                session.id
            ),
        ],
        ExplicitPostingOutcome::Blocked => Vec::new(),
    };

    CommandResponse {
        outcome,
        data: json!({
            "session_id": session.id.clone(),
            "review_run_id": review_run_id.clone(),
            "target": {
                "provider": "github",
                "repository": session.review_target.repository.clone(),
                "pull_request": session.review_target.pull_request_number,
                "repo_id": batch.repo_id.clone(),
                "remote_review_target_id": batch.remote_review_target_id.clone(),
            },
            "draft_batch": {
                "id": batch.id.clone(),
                "approval_state": "approved",
                "payload_digest": batch.payload_digest.clone(),
                "target_tuple_json": outbound_target_tuple_json(&batch),
                "draft_count": drafts.len(),
            },
            "approval": {
                "id": approval.id.clone(),
                "payload_digest": approval.payload_digest.clone(),
                "target_tuple_json": approval.target_tuple_json.clone(),
                "approved_at": approval.approved_at,
                "revoked_at": approval.revoked_at,
            },
            "posting_result": {
                "outcome": posting_result.outcome.clone(),
                "reason_code": posting_result.reason_code.clone(),
            },
            "posted_action": posting_result.posted_action,
            "item_results": posting_result.item_results,
            "retry_draft_ids": posting_result.retry_draft_ids,
            "mutation_guard": {
                "github_posture": github_posture,
                "approval_required": false,
                "posted": posted_anything,
            },
            "queryable_surfaces": {
                "status_command": format!("rr status --session {}", session.id),
                "findings_command": format!("rr findings --session {} --robot", session.id),
                "outbound_state_counts": {
                    "not_drafted": state_counts.not_drafted,
                    "awaiting_approval": state_counts.awaiting_approval,
                    "approved": state_counts.approved,
                    "invalidated": state_counts.invalidated,
                    "posted": state_counts.posted,
                    "failed": state_counts.failed,
                },
            },
            "provider_capability": provider_capability,
            "routine_surface": routine_surface,
        }),
        warnings,
        repair_actions,
        message: match outcome {
            OutcomeKind::Complete => "posted the approved outbound draft batch".to_owned(),
            OutcomeKind::Degraded => {
                "post attempt finished with partial or failed outbound results".to_owned()
            }
            OutcomeKind::Blocked => "post attempt was blocked".to_owned(),
            _ => "post attempt finished".to_owned(),
        },
    }
}

fn outbound_retry_needed(projection: &OutboundSurfaceProjection) -> bool {
    projection
        .failure_code
        .as_deref()
        .is_some_and(|code| code.starts_with("retryable:"))
}

fn outbound_recovery_state(projection: &OutboundSurfaceProjection) -> Option<&'static str> {
    if projection
        .invalidation_reason_code
        .as_deref()
        .is_some_and(|reason| reason.starts_with("superseded"))
    {
        Some("superseded")
    } else if outbound_retry_needed(projection) {
        Some("retry_needed")
    } else if projection.state == "invalidated" {
        Some("invalidated")
    } else {
        None
    }
}

fn outbound_recovery_summary(
    projections: impl IntoIterator<Item = OutboundSurfaceProjection>,
) -> serde_json::Value {
    let mut retry_needed_count = 0_i64;
    let mut superseded_count = 0_i64;
    let mut invalidation_reason_counts = BTreeMap::<String, i64>::new();

    for projection in projections {
        if outbound_retry_needed(&projection) {
            retry_needed_count += 1;
        }

        if matches!(outbound_recovery_state(&projection), Some("superseded")) {
            superseded_count += 1;
        }

        if let Some(reason) = projection.invalidation_reason_code.as_ref() {
            *invalidation_reason_counts
                .entry(reason.clone())
                .or_insert(0) += 1;
        }
    }

    json!({
        "retry_needed_count": retry_needed_count,
        "superseded_count": superseded_count,
        "invalidation_reason_counts": invalidation_reason_counts,
    })
}

fn handle_status(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    let store = match open_store_or_response(runtime, "rr status") {
        Ok(store) => store,
        Err(response) => return response,
    };

    let binding_context = LaunchBindingContext::for_cwd(&runtime.cwd);
    let repository = resolve_repository(parsed.repo.clone(), &runtime.cwd);
    let resolution = match store.resolve_session_reentry_with_context(
        ResolveSessionReentry {
            explicit_session_id: parsed.session_id.clone(),
            repository,
            pull_request_number: parsed.pr,
            source_surface: LaunchSurface::Cli,
            ui_target: Some(cli_config::UI_TARGET.to_owned()),
            instance_preference: Some(cli_config::INSTANCE_PREFERENCE.to_owned()),
        },
        binding_context.storage_local_root(),
    ) {
        Ok(resolution) => resolution,
        Err(err) => return error_response(format!("failed to resolve status context: {err}")),
    };

    let (session, binding) = match resolution {
        SessionReentryResolution::Resolved { session, binding } => (session, binding),
        SessionReentryResolution::PickerRequired { reason, candidates } => {
            if candidates.is_empty() {
                return CommandResponse {
                    outcome: OutcomeKind::Empty,
                    data: json!({"reason": reason, "candidates": []}),
                    warnings: Vec::new(),
                    repair_actions: vec![
                        "run rr review --pr <number> to create a new session".to_owned(),
                    ],
                    message: "no matching session found".to_owned(),
                };
            }
            return blocked_picker_response(reason, candidates);
        }
    };

    let latest_run = match store.latest_review_run(&session.id) {
        Ok(run) => run,
        Err(err) => return error_response(format!("failed to load latest run: {err}")),
    };

    let findings_count = match latest_run.as_ref() {
        Some(run) => match store.materialized_findings_for_run(&session.id, &run.id) {
            Ok(findings) => findings.len(),
            Err(err) => return error_response(format!("failed to load findings: {err}")),
        },
        None => 0,
    };

    let needs_follow_up_count = if let Some(run) = latest_run.as_ref() {
        match store.count_findings_by_triage_state(
            &session.id,
            &run.id,
            FindingTriageState::NeedsFollowUp.as_str(),
        ) {
            Ok(count) => count as usize,
            Err(err) => {
                return error_response(format!("failed to count needs follow up findings: {err}"));
            }
        }
    } else {
        0
    };

    let outbound_counts = if let Some(run) = latest_run.as_ref() {
        match store.outbound_state_counts_for_run(&session.id, &run.id) {
            Ok(counts) => counts,
            Err(err) => {
                return error_response(format!(
                    "failed to project outbound approval state for status: {err}"
                ));
            }
        }
    } else {
        roger_storage::OutboundStateCounts::default()
    };
    let outbound_recovery = if let Some(run) = latest_run.as_ref() {
        match store.materialized_findings_for_run(&session.id, &run.id) {
            Ok(findings) => {
                let mut projections = Vec::with_capacity(findings.len());
                for finding in findings {
                    let projection = match store.outbound_surface_projection_for_finding(
                        &finding.id,
                        &finding.outbound_state,
                    ) {
                        Ok(projection) => projection,
                        Err(err) => {
                            return error_response(format!(
                                "failed to project outbound recovery state for status finding {}: {err}",
                                finding.id
                            ));
                        }
                    };
                    projections.push(projection);
                }
                outbound_recovery_summary(projections)
            }
            Err(err) => {
                return error_response(format!(
                    "failed to load findings for outbound recovery summary: {err}"
                ));
            }
        }
    } else {
        outbound_recovery_summary(Vec::new())
    };

    let branch = infer_git_branch(&runtime.cwd);
    let provider_tier = runtime_provider_tier(runtime, &session.provider);
    let provider_capability = runtime_provider_capability(runtime, &session.provider);
    let routine_surface = runtime_routine_surface_projection(
        runtime,
        &session.provider,
        binding
            .as_ref()
            .and_then(|entry| entry.worktree_root.as_deref()),
    );
    let mut warnings: Vec<String> = provider_support_warning(&session.provider, "rr status")
        .into_iter()
        .collect();
    let mut repair_actions = Vec::new();
    let reconciliation = if session.attention_state == "refresh_recommended" {
        if let Some(warning) = persisted_readback_warning(&session.attention_state) {
            warnings.push(warning);
        }
        repair_actions = persisted_readback_repair_actions(&session.id, &session.review_target);
        persisted_readback_reconciliation(&session.id, &session.review_target, session.updated_at)
    } else {
        json!({
            "mode": "automatic_background",
            "fractional_staleness_allowed": true,
            "stale_target_detected": false,
        })
    };

    // Semantic/memory posture block: retrieval mode, semantic asset state, and
    // the count of pending memory review requests awaiting operator resolution
    // in this repo scope (contract: rr status surfaces semantic asset posture).
    let memory_posture = {
        let component_state = store.semantic_component_state().ok();
        let scope_key = format!("repo:{}", session.review_target.repository);
        let pending_review_count = store
            .count_pending_memory_review_requests(Some(&scope_key))
            .unwrap_or(0);
        let operational = component_state
            .as_ref()
            .map(|state| state.operational)
            .unwrap_or(false);
        json!({
            "scope_key": scope_key,
            "retrieval_mode": if operational { "hybrid" } else { "lexical_only" },
            "semantic_operational": operational,
            "assets_verified": component_state
                .as_ref()
                .map(|state| state.assets_verified)
                .unwrap_or(false),
            "embedder_available": component_state
                .as_ref()
                .map(|state| state.embedder_available)
                .unwrap_or(false),
            "embedder_backend": component_state
                .as_ref()
                .and_then(|state| state.embedder_backend.clone()),
            "degraded_reasons": component_state
                .as_ref()
                .map(|state| state.degraded_reasons.clone())
                .unwrap_or_default(),
            "pending_review_count": pending_review_count,
        })
    };

    CommandResponse {
        outcome: OutcomeKind::Complete,
        data: json!({
            "repo": {
                "root": runtime.cwd.to_string_lossy(),
                "branch": branch,
                "repository": session.review_target.repository,
            },
            "memory": memory_posture,
            "session": {
                "id": session.id,
                "resume_mode": if session.provider == "opencode" { "opencode_bound" } else { "bounded_provider" },
                "provider": session.provider,
            },
            "target": {
                "provider": "github",
                "pull_request": session.review_target.pull_request_number,
            },
            "attention": {
                "state": session.attention_state,
                "updated_at": session.updated_at,
            },
            "reconciliation": reconciliation,
            "findings": {
                "total": findings_count,
                "needs_follow_up": needs_follow_up_count,
            },
            "drafts": {
                "awaiting_approval": outbound_counts.awaiting_approval,
                "approved": outbound_counts.approved,
                "invalidated": outbound_counts.invalidated,
                "posted": outbound_counts.posted,
                "failed": outbound_counts.failed,
            },
            "outbound": {
                "state_counts": {
                    "not_drafted": outbound_counts.not_drafted,
                    "awaiting_approval": outbound_counts.awaiting_approval,
                    "approved": outbound_counts.approved,
                    "invalidated": outbound_counts.invalidated,
                    "posted": outbound_counts.posted,
                    "failed": outbound_counts.failed,
                },
                "posting_gate": {
                    "ready_count": outbound_counts.approved,
                    "visibly_elevated": outbound_counts.approved > 0,
                },
                "recovery": outbound_recovery,
            },
            "continuity": {
                "tier": provider_tier,
                "resume_locator_present": session.session_locator.is_some(),
                "state": session.continuity_state,
            },
            "provider_capability": provider_capability,
            "routine_surface": routine_surface,
        }),
        warnings,
        repair_actions,
        message: "status loaded".to_owned(),
    }
}

const TUI_INTERACTIVE_ONLY_MESSAGE: &str = "rr tui requires an interactive terminal";

fn tui_interactive_only_response(reason_code: &str) -> CommandResponse {
    blocked_response(
        TUI_INTERACTIVE_ONLY_MESSAGE.to_owned(),
        vec![
            "run rr tui from an interactive terminal".to_owned(),
            "use rr status --robot / rr findings --robot for machine-readable session state"
                .to_owned(),
        ],
        json!({
            "reason_code": reason_code,
            "interactive_only": true,
        }),
    )
}

/// Map storage session-finder disambiguation candidates into the TUI cockpit's
/// picker candidates. The finder projection does not carry a continuity tier,
/// so `continuity_tier` is `None`; every other field maps one-to-one. Kept as a
/// standalone pure function so it can be unit-tested without a TTY.
fn picker_candidates_from_finder_entries(
    entries: &[SessionFinderEntry],
) -> Vec<roger_tui::PickerCandidate> {
    entries
        .iter()
        .map(|entry| roger_tui::PickerCandidate {
            session_id: entry.session_id.clone(),
            repository: entry.repository.clone(),
            pull_request: entry.pull_request_number,
            provider: entry.provider.clone(),
            attention_state: entry.attention_state.clone(),
            continuity_tier: None,
            updated_at: entry.updated_at,
        })
        .collect()
}

fn handle_tui(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    // The cockpit is interactive-only by contract: robot callers fail closed
    // toward the existing machine-readable surfaces.
    if parsed.robot {
        return tui_interactive_only_response("robot_mode_unsupported");
    }
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return tui_interactive_only_response("non_interactive_terminal");
    }

    // Same fail-closed store gate (including migration posture guidance) as
    // every other store-backed command.
    let store = match open_store_or_response(runtime, "rr tui") {
        Ok(store) => store,
        Err(response) => return response,
    };

    // Same session resolution as rr status; ambiguity does not fail the TUI —
    // the sessions list is the cockpit's entry screen in that case.
    let binding_context = LaunchBindingContext::for_cwd(&runtime.cwd);
    let repository = resolve_repository(parsed.repo.clone(), &runtime.cwd);
    // Resolved → open on that single session; PickerRequired → hand the
    // disambiguation candidates to the cockpit picker (rather than collapsing
    // to the full finder). Both build a CockpitEntry passed to
    // run_cockpit_with_entry.
    let entry = match store.resolve_session_reentry_with_context(
        ResolveSessionReentry {
            explicit_session_id: parsed.session_id.clone(),
            repository: repository.clone(),
            pull_request_number: parsed.pr,
            source_surface: LaunchSurface::Tui,
            ui_target: Some(cli_config::UI_TARGET.to_owned()),
            instance_preference: Some(cli_config::INSTANCE_PREFERENCE.to_owned()),
        },
        binding_context.storage_local_root(),
    ) {
        Ok(SessionReentryResolution::Resolved { session, .. }) => {
            roger_tui::CockpitEntry::from_session(Some(session.id))
        }
        Ok(SessionReentryResolution::PickerRequired { candidates, .. }) => {
            roger_tui::CockpitEntry {
                initial_session_id: None,
                picker_candidates: Some(picker_candidates_from_finder_entries(&candidates)),
            }
        }
        Err(err) => return error_response(format!("failed to resolve tui session: {err}")),
    };
    drop(store);

    match roger_tui::run_cockpit_with_entry(
        roger_tui::RogerTuiConfig {
            store_root: runtime.store_root.clone(),
            repo: parsed.repo.clone(),
            pr: parsed.pr,
            initial_session_id: None,
        },
        entry,
    ) {
        Ok(roger_tui::TuiExit::Quit) => CommandResponse {
            outcome: OutcomeKind::Complete,
            data: json!({
                "exit": "quit",
                "queryable_surfaces": {
                    "status_command": "rr status",
                    "sessions_command": "rr sessions",
                },
            }),
            warnings: Vec::new(),
            repair_actions: Vec::new(),
            message: "rr tui session ended".to_owned(),
        },
        Err(roger_tui::TuiError::StoreMigrationBlocked { reason }) => {
            store_migration_blocked_response("rr tui", &reason)
        }
        Err(err) => error_response(format!("rr tui failed: {err}")),
    }
}

fn handle_findings(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    let store = match open_store_or_response(runtime, "rr findings") {
        Ok(store) => store,
        Err(response) => return response,
    };

    let binding_context = LaunchBindingContext::for_cwd(&runtime.cwd);
    let repository = resolve_repository(parsed.repo.clone(), &runtime.cwd);
    let resolution = match store.resolve_session_reentry_with_context(
        ResolveSessionReentry {
            explicit_session_id: parsed.session_id.clone(),
            repository,
            pull_request_number: parsed.pr,
            source_surface: LaunchSurface::Cli,
            ui_target: Some(cli_config::UI_TARGET.to_owned()),
            instance_preference: Some(cli_config::INSTANCE_PREFERENCE.to_owned()),
        },
        binding_context.storage_local_root(),
    ) {
        Ok(resolution) => resolution,
        Err(err) => return error_response(format!("failed to resolve findings context: {err}")),
    };

    let (session, binding) = match resolution {
        SessionReentryResolution::Resolved { session, binding } => (session, binding),
        SessionReentryResolution::PickerRequired { reason, candidates } => {
            if candidates.is_empty() {
                // No session yet for this target: align with the rr status /
                // rr sessions reads-model and surface an empty, exit-0 result
                // rather than blocking. Genuine pickers (multiple candidates)
                // still block below.
                return CommandResponse {
                    outcome: OutcomeKind::Empty,
                    data: json!({"reason": reason, "items": [], "count": 0, "candidates": []}),
                    warnings: Vec::new(),
                    repair_actions: vec![
                        "run rr review --pr <number> to create a new session".to_owned(),
                    ],
                    message: "no findings available: no session found for this target".to_owned(),
                };
            }
            return blocked_picker_response(reason, candidates);
        }
    };
    let provider_capability = runtime_provider_capability(runtime, &session.provider);
    let routine_surface = runtime_routine_surface_projection(
        runtime,
        &session.provider,
        binding
            .as_ref()
            .and_then(|entry| entry.worktree_root.as_deref()),
    );

    let Some(run) = (match store.latest_review_run(&session.id) {
        Ok(run) => run,
        Err(err) => return error_response(format!("failed to load latest run: {err}")),
    }) else {
        return CommandResponse {
            outcome: OutcomeKind::Empty,
            data: json!({
                "session_id": session.id,
                "items": [],
                "count": 0,
                "filters_applied": {
                    "repository": session.review_target.repository,
                    "pull_request": session.review_target.pull_request_number,
                },
                "provider_capability": provider_capability.clone(),
                "routine_surface": routine_surface.clone(),
            }),
            warnings: Vec::new(),
            repair_actions: Vec::new(),
            message: "no findings available for this session".to_owned(),
        };
    };

    let findings = match store.materialized_findings_for_run(&session.id, &run.id) {
        Ok(findings) => findings,
        Err(err) => {
            return error_response(format!("failed to load findings for latest run: {err}"));
        }
    };
    let mut warnings: Vec<String> = provider_support_warning(&session.provider, "rr findings")
        .into_iter()
        .collect();
    let mut repair_actions = Vec::new();
    let reconciliation = if session.attention_state == "refresh_recommended" {
        if let Some(warning) = persisted_readback_warning(&session.attention_state) {
            warnings.push(warning);
        }
        repair_actions = persisted_readback_repair_actions(&session.id, &session.review_target);
        persisted_readback_reconciliation(&session.id, &session.review_target, session.updated_at)
    } else {
        json!({
            "mode": "automatic_background",
            "fractional_staleness_allowed": true,
            "stale_target_detected": false,
        })
    };

    let mut items = Vec::with_capacity(findings.len());
    for finding in &findings {
        // Full locations (not just a count) so surfaces that mirror this
        // projection — the extension staging view in particular — get a real
        // file anchor for the primary evidence location.
        let evidence_locations = match store.code_evidence_locations_for_finding(&finding.id) {
            Ok(locations) => locations,
            Err(err) => {
                return error_response(format!(
                    "failed to load evidence locations for finding {}: {err}",
                    finding.id
                ));
            }
        };
        let evidence_count = evidence_locations.len();
        let file_anchor = evidence_locations.first().map(|location| {
            json!({
                "path": location.repo_rel_path,
                "start_line": location.start_line,
                "end_line": location.end_line,
            })
        });

        let outbound_projection = match store
            .outbound_surface_projection_for_finding(&finding.id, &finding.outbound_state)
        {
            Ok(projection) => projection,
            Err(err) => {
                return error_response(format!(
                    "failed to project outbound approval state for finding {}: {err}",
                    finding.id
                ));
            }
        };

        items.push(json!({
            "finding_id": finding.id,
            "fingerprint": finding.fingerprint,
            "title": finding.title,
            "severity": finding.severity,
            "summary": finding.normalized_summary,
            "file_anchor": file_anchor,
            "triage_state": finding.triage_state,
            "outbound_state": outbound_projection.state,
            "outbound_detail": {
                "source": outbound_projection.source,
                "draft_id": outbound_projection.draft_id,
                "draft_batch_id": outbound_projection.draft_batch_id,
                "approval_id": outbound_projection.approval_id,
                "posted_action_id": outbound_projection.posted_action_id,
                "posted_action_status": outbound_projection.posted_action_status,
                "posted_action_item_id": outbound_projection.posted_action_item_id,
                "posted_action_item_status": outbound_projection.posted_action_item_status,
                "remote_identifier": outbound_projection.remote_identifier,
                "failure_code": outbound_projection.failure_code,
                "invalidation_reason_code": outbound_projection.invalidation_reason_code,
                "retry_needed": outbound_retry_needed(&outbound_projection),
                "recovery_state": outbound_recovery_state(&outbound_projection),
                "mutation_elevated": outbound_projection.mutation_elevated,
            },
            "evidence_count": evidence_count,
        }));
    }

    let count = items.len();
    CommandResponse {
        outcome: if count == 0 {
            OutcomeKind::Empty
        } else {
            OutcomeKind::Complete
        },
        data: json!({
            "session_id": session.id,
            "items": items,
            "count": count,
            "filters_applied": {
                "repository": session.review_target.repository,
                "pull_request": session.review_target.pull_request_number,
            },
            "reconciliation": reconciliation,
            "provider_capability": provider_capability,
            "routine_surface": routine_surface,
        }),
        warnings,
        repair_actions,
        message: if count == 0 {
            "no findings available for this session".to_owned()
        } else {
            format!("loaded {count} findings")
        },
    }
}

/// Resolve the session for a read/mutation command exactly the way handle_status
/// and handle_findings do (explicit --session, else repo/PR inference against the
/// current worktree). Returns the resolved session, or a ready-to-return
/// CommandResponse for the picker/error cases.
fn resolve_session_for_command(
    parsed: &ParsedArgs,
    runtime: &CliRuntime,
    store: &RogerStore,
    context_label: &str,
) -> std::result::Result<ReviewSessionRecord, CommandResponse> {
    let binding_context = LaunchBindingContext::for_cwd(&runtime.cwd);
    let repository = resolve_repository(parsed.repo.clone(), &runtime.cwd);
    let resolution = match store.resolve_session_reentry_with_context(
        ResolveSessionReentry {
            explicit_session_id: parsed.session_id.clone(),
            repository,
            pull_request_number: parsed.pr,
            source_surface: LaunchSurface::Cli,
            ui_target: Some(cli_config::UI_TARGET.to_owned()),
            instance_preference: Some(cli_config::INSTANCE_PREFERENCE.to_owned()),
        },
        binding_context.storage_local_root(),
    ) {
        Ok(resolution) => resolution,
        Err(err) => {
            return Err(error_response(format!(
                "failed to resolve {context_label} context: {err}"
            )));
        }
    };
    match resolution {
        SessionReentryResolution::Resolved { session, .. } => Ok(session),
        SessionReentryResolution::PickerRequired { reason, candidates } => {
            Err(blocked_picker_response(reason, candidates))
        }
    }
}

fn handle_memory(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    let store = match open_store_or_response(runtime, "rr memory") {
        Ok(store) => store,
        Err(response) => return response,
    };
    match parsed.memory_command {
        Some(MemoryCommandKind::Review) => handle_memory_review(parsed, runtime, &store),
        Some(MemoryCommandKind::Accept) => {
            handle_memory_resolution(parsed, &store, MemoryReviewDecision::Accept)
        }
        Some(MemoryCommandKind::Reject) => {
            handle_memory_resolution(parsed, &store, MemoryReviewDecision::Reject)
        }
        None => error_response("rr memory reached its handler without a subcommand".to_owned()),
    }
}

/// Render a durable memory-review request row for the listing/resolution envelope.
fn memory_review_request_json(request: &MemoryReviewRequestRecord) -> Value {
    json!({
        "id": request.id,
        "kind": request.request_kind,
        "statement": request.statement,
        "scope": request.scope_key,
        "rationale": request.rationale,
        "source": request.source,
        "status": request.status,
        "memory_class": request.memory_class,
        "normalized_key": request.normalized_key,
        "review_session_id": request.review_session_id,
        "review_run_id": request.review_run_id,
        "created_at": request.created_at,
    })
}

fn handle_memory_review(
    parsed: &ParsedArgs,
    runtime: &CliRuntime,
    store: &RogerStore,
) -> CommandResponse {
    // Read-only listing of pending MemoryReviewRequest rows. Scope by the
    // resolved repository (repo:<repository>), matching rr status' memory
    // posture block; an explicit --session narrows the rows further.
    let repository = resolve_repository(parsed.repo.clone(), &runtime.cwd);
    let scope_key = repository.as_ref().map(|repo| format!("repo:{repo}"));
    let limit = parsed.limit.unwrap_or(200);
    let mut pending = match store.pending_memory_review_requests(scope_key.as_deref(), limit) {
        Ok(pending) => pending,
        Err(err) => {
            return error_response(format!(
                "failed to load pending memory review requests: {err}"
            ));
        }
    };
    if let Some(session_id) = parsed.session_id.as_deref() {
        pending.retain(|request| request.review_session_id == session_id);
    }

    let items = pending
        .iter()
        .map(memory_review_request_json)
        .collect::<Vec<_>>();
    let count = items.len();
    CommandResponse {
        outcome: if count == 0 {
            OutcomeKind::Empty
        } else {
            OutcomeKind::Complete
        },
        data: json!({
            "items": items,
            "count": count,
            "scope_key": scope_key,
            "filters_applied": {
                "repository": repository,
                "session_id": parsed.session_id,
            },
            "queryable_surfaces": {
                "accept_command": "rr memory accept --request <id> --robot",
                "reject_command": "rr memory reject --request <id> --robot",
            },
        }),
        warnings: Vec::new(),
        repair_actions: Vec::new(),
        message: if count == 0 {
            "no pending memory review requests".to_owned()
        } else {
            format!("{count} pending memory review requests")
        },
    }
}

fn handle_memory_resolution(
    parsed: &ParsedArgs,
    store: &RogerStore,
    decision: MemoryReviewDecision,
) -> CommandResponse {
    let decision_label = match decision {
        MemoryReviewDecision::Accept => "accept",
        MemoryReviewDecision::Reject => "reject",
    };
    let Some(request_id) = parsed.request_id.as_deref() else {
        return blocked_response(
            format!("rr memory {decision_label} requires --request <id>"),
            vec![
                "pass --request <id> naming a pending memory review request".to_owned(),
                "list pending requests with rr memory review --robot".to_owned(),
            ],
            json!({"reason_code": "request_id_required"}),
        );
    };

    // Fail closed on unknown or already-resolved request ids with a precise
    // reason code before touching the shared resolution op.
    let existing = match store.memory_review_request(request_id) {
        Ok(existing) => existing,
        Err(err) => {
            return error_response(format!("failed to load memory review request: {err}"));
        }
    };
    let Some(existing) = existing else {
        return blocked_response(
            format!("rr memory {decision_label} could not find request {request_id}"),
            vec!["list pending requests with rr memory review --robot".to_owned()],
            json!({
                "reason_code": "unknown_request_id",
                "request_id": request_id,
            }),
        );
    };
    if existing.status != roger_storage::MEMORY_REVIEW_STATUS_PENDING {
        return blocked_response(
            format!(
                "rr memory {decision_label} is blocked because request {request_id} is already resolved as {}",
                existing.status
            ),
            vec!["list still-pending requests with rr memory review --robot".to_owned()],
            json!({
                "reason_code": "already_resolved",
                "request_id": request_id,
                "status": existing.status,
                "resolution_actor": existing.resolution_actor,
                "resolved_at": existing.resolved_at,
            }),
        );
    }

    match roger_review_ops::resolve_memory_review(store, request_id, decision, "operator:cli") {
        Ok(outcome) => CommandResponse {
            outcome: OutcomeKind::Complete,
            data: json!({
                "decision": decision_label,
                "request": memory_review_request_json(&outcome.request),
                "resulting_memory_item_id": outcome.resulting_memory_item_id,
                "materialized_new_item": outcome.materialized_new_item,
                "mutation_guard": {
                    "github_posture": "blocked",
                    "local_only": true,
                },
            }),
            warnings: Vec::new(),
            repair_actions: Vec::new(),
            message: match decision {
                MemoryReviewDecision::Accept => format!(
                    "accepted memory review request {request_id}{}",
                    outcome
                        .resulting_memory_item_id
                        .as_deref()
                        .map(|id| format!(" -> memory item {id}"))
                        .unwrap_or_default()
                ),
                MemoryReviewDecision::Reject => {
                    format!("rejected memory review request {request_id}")
                }
            },
        },
        Err(roger_review_ops::ReviewOpError::Invalid(message)) => blocked_response(
            message,
            vec!["list pending requests with rr memory review --robot".to_owned()],
            json!({
                "reason_code": "invalid_memory_review_resolution",
                "request_id": request_id,
            }),
        ),
        Err(roger_review_ops::ReviewOpError::Storage(message)) => error_response(format!(
            "failed to resolve memory review request: {message}"
        )),
    }
}

fn handle_timeline(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    let store = match open_store_or_response(runtime, "rr timeline") {
        Ok(store) => store,
        Err(response) => return response,
    };

    // Resolve the session the same way rr status / rr findings do. A target with
    // no session yet is an honest Empty, not a block.
    let binding_context = LaunchBindingContext::for_cwd(&runtime.cwd);
    let repository = resolve_repository(parsed.repo.clone(), &runtime.cwd);
    let resolution = match store.resolve_session_reentry_with_context(
        ResolveSessionReentry {
            explicit_session_id: parsed.session_id.clone(),
            repository,
            pull_request_number: parsed.pr,
            source_surface: LaunchSurface::Cli,
            ui_target: Some(cli_config::UI_TARGET.to_owned()),
            instance_preference: Some(cli_config::INSTANCE_PREFERENCE.to_owned()),
        },
        binding_context.storage_local_root(),
    ) {
        Ok(resolution) => resolution,
        Err(err) => return error_response(format!("failed to resolve timeline context: {err}")),
    };

    let session = match resolution {
        SessionReentryResolution::Resolved { session, .. } => session,
        SessionReentryResolution::PickerRequired { reason, candidates } => {
            if candidates.is_empty() {
                return CommandResponse {
                    outcome: OutcomeKind::Empty,
                    data: json!({
                        "reason": reason,
                        "runs": [],
                        "posted_actions": [],
                        "run_count": 0,
                        "posted_action_count": 0,
                        "candidates": [],
                    }),
                    warnings: Vec::new(),
                    repair_actions: vec![
                        "run rr review --pr <number> to create a new session".to_owned(),
                    ],
                    message: "no timeline available: no session found for this target".to_owned(),
                };
            }
            return blocked_picker_response(reason, candidates);
        }
    };

    // review_runs_for_session returns newest-first; present the timeline
    // oldest-first so the run -> stage -> posted-action history reads
    // chronologically (mirrors the TUI Timeline screen's data set).
    let mut runs = match store.review_runs_for_session(&session.id) {
        Ok(runs) => runs,
        Err(err) => return error_response(format!("failed to load review runs: {err}")),
    };
    runs.reverse();

    let mut run_items = Vec::with_capacity(runs.len());
    for run in &runs {
        let stages = match store.worker_stage_results_for_run(&session.id, &run.id) {
            Ok(stages) => stages,
            Err(err) => {
                return error_response(format!(
                    "failed to load stage results for run {}: {err}",
                    run.id
                ));
            }
        };
        let stage_items = stages
            .iter()
            .map(|stage| {
                json!({
                    "stage": stage.stage,
                    "task_kind": stage.task_kind,
                    "outcome": stage.outcome,
                    "summary": stage.summary,
                    "review_task_id": stage.review_task_id,
                    "created_at": stage.created_at,
                })
            })
            .collect::<Vec<_>>();
        run_items.push(json!({
            "run_id": run.id,
            "run_kind": run.run_kind,
            "continuity_quality": run.continuity_quality,
            "created_at": run.created_at,
            "stages": stage_items,
        }));
    }

    let batches = match store.outbound_draft_batches_for_session(&session.id) {
        Ok(batches) => batches,
        Err(err) => return error_response(format!("failed to load draft batches: {err}")),
    };
    let mut posted_actions = Vec::new();
    for batch in &batches {
        let actions = match store.posted_actions_for_batch(&batch.id) {
            Ok(actions) => actions,
            Err(err) => {
                return error_response(format!(
                    "failed to load posted actions for batch {}: {err}",
                    batch.id
                ));
            }
        };
        for action in actions {
            posted_actions.push(action);
        }
    }
    posted_actions.sort_by(|left, right| {
        left.posted_at
            .cmp(&right.posted_at)
            .then_with(|| left.id.cmp(&right.id))
    });
    let posted_items = posted_actions
        .iter()
        .map(|action| {
            // Emit a stable lowercase status string so the robot envelope stays
            // consistent with the rest of the outbound surface (the enum itself
            // has no snake_case wire rename).
            let status = match action.status {
                roger_app_core::PostedActionStatus::Succeeded => "succeeded",
                roger_app_core::PostedActionStatus::Failed => "failed",
                roger_app_core::PostedActionStatus::Partial => "partial",
            };
            json!({
                "action_id": action.id,
                "batch_id": action.draft_batch_id,
                "remote_identifier": action.remote_identifier,
                "status": status,
                "failure_code": action.failure_code,
                "posted_at": action.posted_at,
            })
        })
        .collect::<Vec<_>>();

    let run_count = run_items.len();
    let posted_action_count = posted_items.len();
    CommandResponse {
        outcome: if run_count == 0 && posted_action_count == 0 {
            OutcomeKind::Empty
        } else {
            OutcomeKind::Complete
        },
        data: json!({
            "session_id": session.id,
            "runs": run_items,
            "posted_actions": posted_items,
            "run_count": run_count,
            "posted_action_count": posted_action_count,
            "filters_applied": {
                "repository": session.review_target.repository,
                "pull_request": session.review_target.pull_request_number,
            },
        }),
        warnings: Vec::new(),
        repair_actions: Vec::new(),
        message: if run_count == 0 && posted_action_count == 0 {
            "no timeline history for this session".to_owned()
        } else {
            format!("timeline: {run_count} runs, {posted_action_count} posted actions")
        },
    }
}

/// Render a durable clarification request row for the clarify envelope.
fn clarification_request_json(request: &roger_storage::ClarificationRequestRecord) -> Value {
    json!({
        "id": request.id,
        "review_session_id": request.review_session_id,
        "review_run_id": request.review_run_id,
        "finding_id": request.finding_id,
        "source": request.source,
        "body": request.body,
        "status": request.status,
        "created_at": request.created_at,
        "resolved_at": request.resolved_at,
    })
}

fn handle_clarify(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    let store = match open_store_or_response(runtime, "rr clarify") {
        Ok(store) => store,
        Err(response) => return response,
    };

    if parsed.clarify_list {
        // Read-only listing of open clarifications, optionally scoped to an
        // explicit --session.
        let limit = parsed.limit.unwrap_or(200);
        let requests = match store.list_clarification_requests(ClarificationRequestQuery {
            review_session_id: parsed.session_id.as_deref(),
            status: Some(roger_storage::CLARIFICATION_STATUS_OPEN),
            limit,
        }) {
            Ok(requests) => requests,
            Err(err) => {
                return error_response(format!("failed to list clarification requests: {err}"));
            }
        };
        let items = requests
            .iter()
            .map(clarification_request_json)
            .collect::<Vec<_>>();
        let count = items.len();
        return CommandResponse {
            outcome: if count == 0 {
                OutcomeKind::Empty
            } else {
                OutcomeKind::Complete
            },
            data: json!({
                "items": items,
                "count": count,
                "filters_applied": {
                    "session_id": parsed.session_id,
                    "status": "open",
                },
            }),
            warnings: Vec::new(),
            repair_actions: Vec::new(),
            message: if count == 0 {
                "no open clarification requests".to_owned()
            } else {
                format!("{count} open clarification requests")
            },
        };
    }

    // Create path: requires exactly one --finding <id> and a non-empty body
    // (from --body or --body-file). Required-argument validation lives here so
    // --robot callers get a blocked JSON envelope instead of plain text.
    let Some(finding_id) = parsed.draft_finding_ids.first().cloned() else {
        return blocked_response(
            "rr clarify requires --finding <id> to create a clarification (or pass --list to list open clarifications)".to_owned(),
            vec![
                "pass --finding <id> naming the finding to clarify".to_owned(),
                "or run rr clarify --list to list open clarifications".to_owned(),
            ],
            json!({"reason_code": "finding_required"}),
        );
    };
    if parsed.draft_finding_ids.len() > 1 {
        return blocked_response(
            "rr clarify creates one clarification against a single --finding <id>".to_owned(),
            vec!["pass a single --finding <id>".to_owned()],
            json!({
                "reason_code": "single_finding_required",
                "finding_ids": parsed.draft_finding_ids,
            }),
        );
    }

    let body = match (parsed.clarify_body.as_ref(), parsed.edit_body_file.as_ref()) {
        (Some(body), None) => body.clone(),
        (None, Some(path)) => match fs::read_to_string(path) {
            Ok(body) => body,
            Err(err) => {
                return blocked_response(
                    format!(
                        "rr clarify could not read --body-file {}: {err}",
                        path.display()
                    ),
                    vec!["pass a readable --body-file <path> or use --body <text>".to_owned()],
                    json!({
                        "reason_code": "body_file_unreadable",
                        "body_file": path.display().to_string(),
                    }),
                );
            }
        },
        (None, None) => {
            return blocked_response(
                "rr clarify requires a clarification body via --body <text> or --body-file <path>"
                    .to_owned(),
                vec!["pass --body <text> or --body-file <path>".to_owned()],
                json!({"reason_code": "clarification_body_required"}),
            );
        }
        (Some(_), Some(_)) => {
            // Rejected at parse time; kept fail-closed for parity.
            return blocked_response(
                "rr clarify accepts either --body or --body-file, not both".to_owned(),
                vec!["pass exactly one of --body or --body-file".to_owned()],
                json!({"reason_code": "conflicting_body_sources"}),
            );
        }
    };
    if body.trim().is_empty() {
        return blocked_response(
            "rr clarify requires a non-empty clarification body".to_owned(),
            vec!["pass a non-empty --body <text> or --body-file <path>".to_owned()],
            json!({"reason_code": "clarification_body_required"}),
        );
    }

    let session = match resolve_session_for_command(parsed, runtime, &store, "clarify") {
        Ok(session) => session,
        Err(response) => return response,
    };

    // Fail closed if the finding does not exist or is not bound to the resolved
    // session, so a clarification never links to a foreign finding.
    let finding = match store.materialized_finding(&finding_id) {
        Ok(finding) => finding,
        Err(err) => return error_response(format!("failed to load finding {finding_id}: {err}")),
    };
    let Some(finding) = finding else {
        return blocked_response(
            format!("rr clarify could not find finding {finding_id}"),
            vec![format!(
                "inspect rr findings --session {} --robot for the current finding ids",
                session.id
            )],
            json!({
                "reason_code": "unknown_finding_id",
                "session_id": session.id,
                "finding_id": finding_id,
            }),
        );
    };
    if finding.session_id != session.id {
        return blocked_response(
            format!(
                "rr clarify could not bind finding {finding_id} to the resolved session {}",
                session.id
            ),
            vec![format!(
                "inspect rr findings --session {} --robot for the current finding ids",
                session.id
            )],
            json!({
                "reason_code": "unknown_finding_id",
                "session_id": session.id,
                "finding_id": finding_id,
                "finding_session_id": finding.session_id,
            }),
        );
    }

    let review_run_id = match store.latest_review_run(&session.id) {
        Ok(run) => run.map(|run| run.id),
        Err(err) => return error_response(format!("failed to load latest run: {err}")),
    };

    match roger_review_ops::create_clarification(
        &store,
        CreateClarificationRequest {
            review_session_id: &session.id,
            review_run_id: review_run_id.as_deref(),
            finding_id: Some(&finding_id),
            source: ClarificationSource::Operator,
            body: &body,
            external_ref: None,
        },
    ) {
        Ok(request) => CommandResponse {
            outcome: OutcomeKind::Complete,
            data: json!({
                "clarification": clarification_request_json(&request),
                "mutation_guard": {
                    "github_posture": "blocked",
                    "local_only": true,
                },
                "queryable_surfaces": {
                    "list_command": format!("rr clarify --list --session {} --robot", session.id),
                },
            }),
            warnings: Vec::new(),
            repair_actions: Vec::new(),
            message: format!(
                "created clarification {} against finding {finding_id}",
                request.id
            ),
        },
        Err(roger_review_ops::ReviewOpError::Invalid(message)) => blocked_response(
            message,
            vec!["pass a non-empty --body <text> or --body-file <path>".to_owned()],
            json!({"reason_code": "clarification_body_required"}),
        ),
        Err(roger_review_ops::ReviewOpError::Storage(message)) => {
            error_response(format!("failed to create clarification: {message}"))
        }
    }
}

fn render_output(parsed: &ParsedArgs, mut response: CommandResponse) -> CliRunResult {
    if parsed.command == CommandKind::Agent {
        let stdout = match serde_json::to_string_pretty(&response.data) {
            Ok(text) => format!("{text}\n"),
            Err(err) => {
                return CliRunResult {
                    exit_code: 1,
                    stdout: String::new(),
                    stderr: format!("failed to serialize rr agent output: {err}\n"),
                };
            }
        };

        let mut stderr = String::new();
        if !response.warnings.is_empty() {
            stderr.push_str(&response.warnings.join("\n"));
            stderr.push('\n');
        }

        return CliRunResult {
            exit_code: response.outcome.exit_code(),
            stdout,
            stderr,
        };
    }

    if parsed.robot
        && (parsed.robot_format == RobotFormat::Compact || parsed.robot_format == RobotFormat::Toon)
    {
        response.data = compact_data(parsed.command, response.data);
    }

    if parsed.robot {
        let exit_code = response.outcome.exit_code();
        let envelope = RobotEnvelope {
            schema_id: parsed.command.schema_id().to_owned(),
            command: parsed.command.as_rr_command(parsed.dry_run).to_owned(),
            robot_format: parsed.robot_format.as_str().to_owned(),
            outcome: response.outcome.as_str().to_owned(),
            generated_at: time::now_ts().to_string(),
            exit_code,
            warnings: response.warnings.clone(),
            repair_actions: response.repair_actions.clone(),
            data: response.data,
        };

        let stdout = match parsed.robot_format {
            RobotFormat::Json | RobotFormat::Compact => {
                match serde_json::to_string_pretty(&envelope) {
                    Ok(text) => format!("{text}\n"),
                    Err(err) => {
                        return CliRunResult {
                            exit_code: 1,
                            stdout: String::new(),
                            stderr: format!("failed to serialize robot output: {err}\n"),
                        };
                    }
                }
            }
            RobotFormat::Toon => match encode_toon_default(&envelope) {
                Ok(text) => format!("{text}\n"),
                Err(err) => {
                    return CliRunResult {
                        exit_code: 1,
                        stdout: String::new(),
                        stderr: format!("failed to serialize robot output as toon: {err}\n"),
                    };
                }
            },
        };

        let mut diagnostics = String::new();
        if !response.warnings.is_empty() {
            diagnostics.push_str(&response.warnings.join("\n"));
            diagnostics.push('\n');
        }

        return CliRunResult {
            exit_code,
            stdout,
            stderr: diagnostics,
        };
    }

    let mut stdout = String::new();
    stdout.push_str(&response.message);
    stdout.push('\n');

    if parsed.command == CommandKind::Prs
        && !matches!(response.outcome, OutcomeKind::Blocked | OutcomeKind::Error)
    {
        stdout.push_str(&render_prs_table(&response.data));
    }

    if matches!(
        parsed.command,
        CommandKind::Init
            | CommandKind::Doctor
            | CommandKind::Status
            | CommandKind::Findings
            | CommandKind::Sessions
            | CommandKind::Search
            | CommandKind::Draft
            | CommandKind::Edit
            | CommandKind::Approve
            | CommandKind::Post
            | CommandKind::RobotDocs
            | CommandKind::Memory
            | CommandKind::Timeline
            | CommandKind::Clarify
    ) || response.outcome == OutcomeKind::Blocked
    {
        // Session candidates and `rr sessions` listings render as a grouped,
        // age-annotated, capped table for humans; raw JSON stays the transport
        // for --robot consumers.
        let now = time::now_ts();
        let candidates = response.data.get("candidates").and_then(Value::as_array);
        let session_items = if parsed.command == CommandKind::Sessions {
            response.data.get("items").and_then(Value::as_array)
        } else {
            None
        };
        if let Some(candidates) = candidates.filter(|list| !list.is_empty()) {
            stdout.push_str("Sessions:\n");
            stdout.push_str(&render_grouped_session_lines(
                candidates,
                parsed.show_all,
                now,
            ));
            // "Pick one" only makes sense for a genuine multi-candidate
            // picker. A single blocked candidate cannot be resolved by
            // selection, so the primary message (which surfaces the concrete
            // blocking reason) and repair actions stand on their own.
            if candidates.len() >= 2 {
                stdout.push_str("Re-run with --session <id> to pick one.\n");
            }
        } else if let Some(items) = session_items.filter(|list| !list.is_empty()) {
            stdout.push_str("Sessions:\n");
            stdout.push_str(&render_grouped_session_lines(items, parsed.show_all, now));
        } else if candidates.is_some() || session_items.is_some() {
            // Zero candidates/items: the message and repair actions already say
            // everything a human needs; no JSON blob.
        } else if let Ok(pretty) = serde_json::to_string_pretty(&response.data) {
            stdout.push_str(&pretty);
            stdout.push('\n');
        }
    }

    let failure_outcome = matches!(
        response.outcome,
        OutcomeKind::Blocked | OutcomeKind::RepairNeeded | OutcomeKind::Error
    );

    let mut stderr = String::new();
    if failure_outcome {
        // Launch/review failures must be loud: one clear line on stderr
        // naming the reason, so failures are visible even when stdout is
        // piped, captured, or ignored.
        stderr.push_str("error: ");
        stderr.push_str(&response.message);
        stderr.push('\n');
    }
    if !response.warnings.is_empty() {
        stderr.push_str(&response.warnings.join("\n"));
        stderr.push('\n');
    }
    if !response.repair_actions.is_empty() {
        stderr.push_str(if failure_outcome {
            "Try:\n"
        } else {
            "Suggested next steps:\n"
        });
        for action in &response.repair_actions {
            stderr.push_str("- ");
            stderr.push_str(action);
            stderr.push('\n');
        }
    }

    let exit_code = if response.outcome == OutcomeKind::Degraded
        && matches!(parsed.command, CommandKind::Review | CommandKind::Resume)
    {
        0
    } else {
        response.outcome.exit_code()
    };

    CliRunResult {
        exit_code,
        stdout,
        stderr,
    }
}

fn resolve_repository(explicit: Option<String>, cwd: &Path) -> Option<String> {
    explicit.or_else(|| infer_repository_from_git(cwd))
}

#[derive(Clone, Debug, Default)]
struct GitLookupSnapshot {
    repository: Option<String>,
    branch: Option<String>,
    worktree_root: Option<String>,
}

#[derive(Clone, Debug)]
struct LaunchBindingContext {
    cwd: String,
    worktree_root: Option<String>,
}

impl LaunchBindingContext {
    fn for_cwd(cwd: &Path) -> Self {
        Self {
            cwd: git_cache_key(cwd).to_string_lossy().into_owned(),
            worktree_root: infer_git_worktree_root(cwd),
        }
    }

    fn storage_local_root(&self) -> ResolveSessionLocalRoot<'_> {
        ResolveSessionLocalRoot {
            cwd: Some(self.cwd.as_str()),
            worktree_root: self.worktree_root.as_deref(),
        }
    }
}

/// Resolve the effective launch surface for a launch-attempt command, defaulting
/// to the CLI surface when `--surface` was not supplied.
fn resolved_launch_surface(parsed: &ParsedArgs) -> LaunchSurface {
    parsed.surface.unwrap_or(LaunchSurface::Cli)
}

/// Choose the durable local-root context stored on a launch binding.
///
/// Bridge-origin launches inherit the browser's cwd, which is frequently a
/// poisoned path under NativeMessagingHosts. Binding a repo-local cwd/worktree
/// there would later trip the stale-binding invariant and block `rr status
/// --session` / `rr resume --session` readback. So for the bridge surface we
/// deliberately bind no repo-local root (the validator recognizes the bridge
/// surface and exempts it); every other surface keeps its real local-root
/// context so the invariant continues to protect CLI/TUI launches.
fn binding_local_root_for_surface(
    surface: LaunchSurface,
    binding_context: &LaunchBindingContext,
) -> (Option<&str>, Option<&str>) {
    if surface == LaunchSurface::Bridge {
        (None, None)
    } else {
        (
            Some(binding_context.cwd.as_str()),
            binding_context.worktree_root.as_deref(),
        )
    }
}

fn git_lookup_cache() -> &'static Mutex<HashMap<PathBuf, GitLookupSnapshot>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, GitLookupSnapshot>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn git_cache_key(cwd: &Path) -> PathBuf {
    std::fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf())
}

fn cached_git_snapshot(cwd: &Path) -> GitLookupSnapshot {
    let key = git_cache_key(cwd);

    if let Ok(cache) = git_lookup_cache().lock()
        && let Some(snapshot) = cache.get(&key)
    {
        return snapshot.clone();
    }

    let snapshot = GitLookupSnapshot {
        repository: infer_repository_from_git_uncached(cwd),
        branch: infer_git_branch_uncached(cwd),
        worktree_root: infer_git_worktree_root_uncached(cwd),
    };

    if let Ok(mut cache) = git_lookup_cache().lock() {
        cache.insert(key, snapshot.clone());
    }

    snapshot
}

fn infer_repository_from_git(cwd: &Path) -> Option<String> {
    cached_git_snapshot(cwd).repository
}

fn infer_repository_from_git_uncached(cwd: &Path) -> Option<String> {
    let output = ProcessCommand::new("git")
        .arg("-C")
        .arg(cwd)
        .arg("config")
        .arg("--get")
        .arg("remote.origin.url")
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let remote = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    parse_repository_from_remote(&remote)
}

fn parse_repository_from_remote(remote: &str) -> Option<String> {
    let without_prefix = remote
        .strip_prefix("git@github.com:")
        .or_else(|| remote.strip_prefix("https://github.com/"))
        .or_else(|| remote.strip_prefix("ssh://git@github.com/"))?;

    let cleaned = without_prefix.trim_end_matches(".git").trim_matches('/');
    let mut parts = cleaned.split('/');
    let owner = parts.next()?;
    let repo = parts.next()?;
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some(format!("{owner}/{repo}"))
}

fn infer_git_branch(cwd: &Path) -> Option<String> {
    cached_git_snapshot(cwd).branch
}

fn infer_git_worktree_root(cwd: &Path) -> Option<String> {
    cached_git_snapshot(cwd).worktree_root
}

fn infer_git_branch_uncached(cwd: &Path) -> Option<String> {
    let output = ProcessCommand::new("git")
        .arg("-C")
        .arg(cwd)
        .arg("rev-parse")
        .arg("--abbrev-ref")
        .arg("HEAD")
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    Some(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn infer_git_worktree_root_uncached(cwd: &Path) -> Option<String> {
    let output = ProcessCommand::new("git")
        .arg("-C")
        .arg(cwd)
        .arg("rev-parse")
        .arg("--show-toplevel")
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    Some(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn launch_intent(action: LaunchAction, runtime: &CliRuntime) -> LaunchIntent {
    let binding_context = LaunchBindingContext::for_cwd(&runtime.cwd);
    LaunchIntent {
        action,
        source_surface: Surface::Cli,
        objective: None,
        launch_profile_id: Some(cli_config::PROFILE_ID.to_owned()),
        cwd: Some(binding_context.cwd),
        worktree_root: binding_context.worktree_root,
    }
}

fn build_review_target(repository: &str, pull_request_number: u64) -> ReviewTarget {
    ReviewTarget {
        repository: repository.to_owned(),
        pull_request_number,
        base_ref: "main".to_owned(),
        head_ref: format!("pr-{pull_request_number}"),
        base_commit: "unknown-base".to_owned(),
        head_commit: "unknown-head".to_owned(),
    }
}

fn build_resume_bundle(
    profile: ResumeBundleProfile,
    target: ReviewTarget,
    launch_intent: LaunchIntent,
    provider: String,
    continuity_quality: ContinuityQuality,
    stage_summary: &str,
    artifact_refs: Vec<String>,
) -> ResumeBundle {
    ResumeBundle {
        schema_version: 1,
        profile,
        review_target: target,
        launch_intent,
        provider,
        continuity_quality,
        stage_summary: stage_summary.to_owned(),
        unresolved_finding_ids: Vec::new(),
        outbound_draft_ids: Vec::new(),
        attention_summary: "awaiting_user_input".to_owned(),
        artifact_refs,
    }
}

fn classify_reopen_outcome_for_return(
    adapter: &OpenCodeAdapter,
    target: &ReviewTarget,
    locator: Option<&roger_app_core::SessionLocator>,
) -> ResumeAttemptOutcome {
    let Some(locator) = locator else {
        return ResumeAttemptOutcome::ReopenUnavailable;
    };

    match adapter.reopen_by_locator(locator) {
        Ok(()) => match adapter.report_continuity_quality(locator, target) {
            Ok(ContinuityQuality::Usable) => ResumeAttemptOutcome::ReopenedUsable,
            Ok(ContinuityQuality::Degraded) | Ok(ContinuityQuality::Unusable) => {
                ResumeAttemptOutcome::ReopenedDegraded
            }
            Err(err) => classify_reopen_error(&err),
        },
        Err(err) => classify_reopen_error(&err),
    }
}

fn classify_reopen_error(err: &AppError) -> ResumeAttemptOutcome {
    let lower = err.to_string().to_lowercase();
    if lower.contains("target mismatch") {
        ResumeAttemptOutcome::TargetMismatch
    } else if lower.contains("missing")
        || lower.contains("compacted")
        || lower.contains("not found")
        || lower.contains("stale")
    {
        ResumeAttemptOutcome::MissingHarnessState
    } else {
        ResumeAttemptOutcome::ReopenUnavailable
    }
}

/// Demote a provisionally committed launch session to `review_failed` after a
/// post-provision launch failure. Best-effort: the launch-attempt record
/// carries the authoritative failure detail, so a demotion error is logged to
/// stderr rather than masking the original failure.
fn demote_provisional_session(store: &RogerStore, session_id: &str) {
    let record = match store.review_session(session_id) {
        Ok(Some(record)) => record,
        Ok(None) => return,
        Err(err) => {
            eprintln!(
                "warning: failed to load provisional session {session_id} for demotion: {err}"
            );
            return;
        }
    };
    if let Err(err) =
        store.update_review_session_attention(session_id, record.row_version, "review_failed")
    {
        eprintln!("warning: failed to demote provisional session {session_id}: {err}");
    }
}

fn continuity_state_label(quality: &ContinuityQuality) -> &'static str {
    match quality {
        ContinuityQuality::Usable => "usable",
        ContinuityQuality::Degraded => "degraded",
        ContinuityQuality::Unusable => "unusable",
    }
}

fn provider_tier(provider: &str) -> &'static str {
    match provider {
        "opencode" => "tier_b",
        "codex" | "gemini" | "claude" => "tier_a",
        _ => "unavailable",
    }
}

fn runtime_provider_tier(_runtime: &CliRuntime, provider: &str) -> &'static str {
    if copilot_feature_gated_launch_enabled(provider) {
        COPILOT_FEATURE_GATED_TIER
    } else {
        provider_tier(provider)
    }
}

fn provider_support_status(provider: &str) -> &'static str {
    match provider {
        "opencode" => "first_class_live",
        "codex" | "gemini" | "claude" => "bounded_live",
        "copilot" => "planned_not_live",
        _ => "not_supported",
    }
}

fn provider_display_name(provider: &str) -> &'static str {
    match provider {
        "opencode" => "OpenCode",
        "codex" => "Codex",
        "gemini" => "Gemini",
        "claude" => "Claude Code",
        "copilot" => "GitHub Copilot CLI",
        "pi-agent" => "Pi-Agent",
        _ => "Unknown provider",
    }
}

fn provider_live_support_notes(provider: &str) -> &'static str {
    match provider {
        "opencode" => "first-class tier-b continuity path with locator reopen and rr return",
        "codex" | "gemini" | "claude" => {
            "bounded tier-a start/reseed/raw-capture path only; no locator reopen or rr return"
        }
        "copilot" => "planned target, not yet a live rr review --provider value",
        "pi-agent" => "not part of the current live CLI surface",
        _ => "provider is not part of the current live rr review surface",
    }
}

fn provider_capability(provider: &str) -> Value {
    json!({
        "provider": provider,
        "display_name": provider_display_name(provider),
        "status": provider_support_status(provider),
        "tier": provider_tier(provider),
        "support_tier": provider_tier(provider),
        "denied_capabilities": [],
        "audit_artifact_classes": [],
        "notes": provider_live_support_notes(provider),
        "supports": {
            "review_start": SUPPORTED_REVIEW_PROVIDERS.contains(&provider),
            "resume_reseed": SUPPORTED_REVIEW_PROVIDERS.contains(&provider),
            "resume_reopen": provider == "opencode",
            "return": provider == "opencode",
            "status": true,
            "findings": true,
            "sessions": true,
        }
    })
}

fn resolved_routine_surface_baseline(
    runtime: &CliRuntime,
    provider: &str,
) -> Result<ResolvedRoutineSurfaceBaseline, String> {
    cli_config::resolved_cli_config(&runtime.cwd)
        .routine_surface_baseline(Some(provider))
        .map_err(|err| {
            format!(
                "failed to resolve routine surface baseline for provider '{provider}': {}",
                err.message
            )
        })
}

fn provider_supports_json(provider: &ResolvedProviderCapability) -> Value {
    json!({
        "review_start": provider.supports.review_start,
        "resume_reseed": provider.supports.resume_reseed,
        "resume_reopen": provider.supports.resume_reopen,
        "return": provider.supports.rr_return,
        "status": provider.supports.status,
        "findings": provider.supports.findings,
        "sessions": provider.supports.sessions,
        "doctor": provider.supports.doctor,
    })
}

fn stable_digest_payload(payload: &Value) -> String {
    sha256_prefixed_json(payload)
        .unwrap_or_else(|_| format!("sha256:{}", sha256_hex(payload.to_string().as_bytes())))
}

fn mcp_posture_projection(provider: &ResolvedProviderCapability) -> Value {
    let builtin_denied = provider
        .denied_capabilities
        .iter()
        .any(|capability| capability == "builtin_github_mcp");
    let broad_denied = provider
        .denied_capabilities
        .iter()
        .any(|capability| capability == "broad_mcp_access");
    let posture = if builtin_denied || broad_denied {
        "restricted"
    } else {
        "not_declared"
    };

    json!({
        "posture": posture,
        "source": "provider.policy_profile.denied_capabilities",
        "builtin_github_mcp": {
            "state": if builtin_denied { "denied" } else { "not_denied" },
            "denied": builtin_denied,
        },
        "broad_mcp_access": {
            "state": if broad_denied { "denied" } else { "not_denied" },
            "denied": broad_denied,
        },
    })
}

fn routine_surface_with_worktree_root(
    mut routine_surface: Value,
    cwd: &Path,
    worktree_root_override: Option<&str>,
) -> Value {
    let worktree_root = worktree_root_override
        .map(ToOwned::to_owned)
        .or_else(|| infer_git_worktree_root(cwd));
    if let Some(surface_obj) = routine_surface.as_object_mut() {
        surface_obj.insert(
            "worktree_root".to_owned(),
            worktree_root.map(Value::String).unwrap_or(Value::Null),
        );
    }
    routine_surface
}

fn provider_capability_projection(
    provider: &ResolvedProviderCapability,
    status_reason: Option<&str>,
) -> Value {
    let policy_profile = json!({
        "id": provider.policy_profile.id.clone(),
        "summary": provider.policy_profile.summary.clone(),
        "mutation_posture": provider.policy_profile.mutation_posture.clone(),
        "continuity_mode": provider.policy_profile.continuity_mode.clone(),
    });
    let policy_profile_digest_sha256 = stable_digest_payload(&json!({
        "provider": provider.provider.clone(),
        "policy_profile": policy_profile.clone(),
    }));
    let hook_profile = json!({
        "id": format!("{}.hooks", provider.policy_profile.id),
        "contract_version": provider.hook_contract_version.value.clone(),
        "contract_provenance": provider.hook_contract_version.provenance.clone(),
    });
    let hook_profile_digest_sha256 = stable_digest_payload(&json!({
        "provider": provider.provider.clone(),
        "policy_profile_id": provider.policy_profile.id.clone(),
        "hook_profile": hook_profile.clone(),
    }));
    let custom_instructions = json!({
        "id": format!("{}.instructions", provider.policy_profile.id),
        "contract_version": provider.instruction_contract_version.value.clone(),
        "contract_provenance": provider.instruction_contract_version.provenance.clone(),
    });
    let custom_instructions_digest_sha256 = stable_digest_payload(&json!({
        "provider": provider.provider.clone(),
        "policy_profile_id": provider.policy_profile.id.clone(),
        "custom_instructions": custom_instructions.clone(),
    }));

    json!({
        "provider": provider.provider,
        "display_name": provider.display_name,
        "status": provider.status,
        "tier": provider.support_tier,
        "support_tier": provider.support_tier,
        "surface_class": provider.surface_class,
        "capability_provenance": provider.capability_provenance,
        "policy_profile": policy_profile,
        "policy_profile_digest_sha256": policy_profile_digest_sha256,
        "hook_profile": hook_profile,
        "hook_profile_digest_sha256": hook_profile_digest_sha256,
        "custom_instructions": custom_instructions,
        "custom_instructions_digest_sha256": custom_instructions_digest_sha256,
        "mcp_posture": mcp_posture_projection(provider),
        "denied_capabilities": provider.denied_capabilities,
        "audit_artifact_classes": provider.audit_artifact_classes,
        "status_reason": status_reason,
        "supports": provider_supports_json(provider),
        "notes": provider.notes,
    })
}

fn routine_surface_baseline_projection(baseline: &ResolvedRoutineSurfaceBaseline) -> Value {
    let launch_profile_id = baseline.launch_profile_id.value.clone();
    let launch_profile_provenance = baseline.launch_profile_id.provenance.clone();
    let ui_target = baseline.ui_target.value.clone();
    let ui_target_provenance = baseline.ui_target.provenance.clone();
    let instance_preference = baseline.instance_preference.value.clone();
    let instance_preference_provenance = baseline.instance_preference.provenance.clone();
    let isolation_mode = baseline.isolation_mode.value.clone();
    let isolation_mode_provenance = baseline.isolation_mode.provenance.clone();
    let named_instance_on_collision = baseline.named_instance_on_collision.value;
    let named_instance_on_collision_provenance =
        baseline.named_instance_on_collision.provenance.clone();

    json!({
        "surface": baseline.surface,
        "launch_profile_id": launch_profile_id.clone(),
        "launch_profile": {
            "id": launch_profile_id,
            "provenance": launch_profile_provenance.clone(),
        },
        "provider": provider_capability_projection(&baseline.provider, baseline.status_reason.as_deref()),
        "ui_target": ui_target.clone(),
        "instance_preference": instance_preference.clone(),
        "isolation_mode": isolation_mode.clone(),
        "named_instance_on_collision": named_instance_on_collision,
        "provenance": {
            "launch_profile_id": launch_profile_provenance,
            "ui_target": ui_target_provenance,
            "instance_preference": instance_preference_provenance,
            "isolation_mode": isolation_mode_provenance,
            "named_instance_on_collision": named_instance_on_collision_provenance,
        },
        "repair_overrides_active": baseline.repair_overrides_active,
        "active_repair_override_keys": baseline.active_repair_override_keys,
        "status_reason": baseline.status_reason,
    })
}

fn runtime_provider_capability(runtime: &CliRuntime, provider: &str) -> Value {
    if copilot_feature_gated_launch_enabled(provider) {
        return copilot_projected_provider_capability(runtime);
    }

    // Copilot with the gate OFF is a documented feature-gated tier-b provider
    // that is disabled-but-enableable, not a genuinely-planned tier-a provider.
    if provider == session_copilot::PROVIDER_ID {
        return copilot_feature_gated_disabled_provider_capability(runtime);
    }

    match resolved_routine_surface_baseline(runtime, provider) {
        Ok(baseline) => {
            provider_capability_projection(&baseline.provider, baseline.status_reason.as_deref())
        }
        Err(_) => provider_capability(provider),
    }
}

fn runtime_routine_surface_projection(
    runtime: &CliRuntime,
    provider: &str,
    worktree_root_override: Option<&str>,
) -> Option<Value> {
    if copilot_feature_gated_launch_enabled(provider) {
        return copilot_projected_routine_surface(runtime, worktree_root_override);
    }

    resolved_routine_surface_baseline(runtime, provider)
        .ok()
        .map(|baseline| {
            routine_surface_with_worktree_root(
                routine_surface_baseline_projection(&baseline),
                &runtime.cwd,
                worktree_root_override,
            )
        })
}

fn runtime_supported_review_providers(_runtime: &CliRuntime) -> Vec<&'static str> {
    let mut providers = SUPPORTED_REVIEW_PROVIDERS.to_vec();
    if copilot_feature_gated_launch_enabled(session_copilot::PROVIDER_ID) {
        providers.push(session_copilot::PROVIDER_ID);
    }
    providers
}

fn runtime_planned_not_live_review_providers(_runtime: &CliRuntime) -> Vec<&'static str> {
    // Copilot is feature-gated, not "planned but not live": with the gate off it
    // is reported through runtime_feature_gated_disabled_review_providers so every
    // surface agrees with the doctor classification. No genuinely-planned review
    // provider remains on the current live CLI surface, so this list is empty.
    Vec::new()
}

fn runtime_feature_gated_disabled_review_providers(_runtime: &CliRuntime) -> Vec<&'static str> {
    if copilot_feature_gated_launch_enabled(session_copilot::PROVIDER_ID) {
        Vec::new()
    } else {
        PLANNED_REVIEW_PROVIDERS.to_vec()
    }
}

fn runtime_review_provider_support_summary(_runtime: &CliRuntime) -> String {
    if copilot_feature_gated_launch_enabled(session_copilot::PROVIDER_ID) {
        "OpenCode is the only default live tier-b continuity path on the current live CLI surface. Codex, Gemini, and Claude Code are exposed as bounded tier-a start/reseed/raw-capture providers only. GitHub Copilot CLI is feature-gated as a bounded tier-b continuity path with verified start, locator/session-id reopen, rr return, and ResumeBundle reseed fallback, but Roger still withholds a default public live claim for Copilot. Pi-Agent remains out of scope for now."
            .to_owned()
    } else {
        "OpenCode is the only live tier-b continuity path on the current live CLI surface. Codex, Gemini, and Claude Code are exposed as bounded tier-a start/reseed/raw-capture providers only. Copilot is feature-gated and currently disabled; enable RR_ENABLE_COPILOT_PROVIDER=1 for its bounded tier-b continuity path. Pi-Agent remains out of scope for now."
            .to_owned()
    }
}

fn runtime_review_provider_support_matrix(runtime: &CliRuntime) -> Vec<Value> {
    runtime_supported_review_providers(runtime)
        .into_iter()
        .map(|provider| {
            let capability = runtime_provider_capability(runtime, provider);
            json!({
                "provider": provider,
                "display_name": capability["display_name"].clone(),
                "tier": capability["support_tier"].clone(),
                "status": capability["status"].clone(),
                "supports": capability["supports"].clone(),
                "notes": capability["notes"].clone(),
            })
        })
        .collect()
}

fn provider_support_warning(provider: &str, command: &str) -> Option<String> {
    if provider == "opencode" {
        None
    } else if copilot_feature_gated_launch_enabled(provider) {
        Some(format!(
            "provider '{}' is feature-gated as a bounded tier-b path; '{}' supports verified start, locator/session-id reopen, rr return, and ResumeBundle reseed fallback, but Roger still withholds a default public live claim",
            provider, command
        ))
    } else if provider == "codex" || provider == "gemini" || provider == "claude" {
        Some(format!(
            "provider '{}' has bounded support (tier-a start/reseed/raw-capture only); '{}' does not support locator reopen or rr return",
            provider, command
        ))
    } else {
        Some(format!(
            "provider '{}' has bounded support (tier-a); '{}' may offer reduced continuity behavior",
            provider, command
        ))
    }
}

fn persisted_readback_reconciliation(
    session_id: &str,
    target: &ReviewTarget,
    updated_at: i64,
) -> Value {
    json!({
        "mode": "persisted_readback",
        "manual_refresh_supported": false,
        "stale_target_detected": true,
        "repair_required": true,
        "freshness_basis": "persisted_attention_state",
        "attention_updated_at": updated_at,
        "recommended_reentry_command": format!("rr resume --session {session_id}"),
        "recommended_fresh_pass_command": format!(
            "rr review --repo {} --pr {}",
            target.repository, target.pull_request_number
        ),
    })
}

fn persisted_readback_warning(attention_state: &str) -> Option<String> {
    if attention_state == "refresh_recommended" {
        Some(
            "Roger is showing the last persisted review state; reopen the Roger session or start a fresh pass to reconcile stale target context."
                .to_owned(),
        )
    } else {
        None
    }
}

fn persisted_readback_repair_actions(session_id: &str, target: &ReviewTarget) -> Vec<String> {
    vec![
        format!("run rr resume --session {session_id} to reopen the Roger session locally"),
        format!(
            "run rr review --repo {} --pr {} to start a fresh pass if target drift invalidated the older review",
            target.repository, target.pull_request_number
        ),
    ]
}

fn session_path_label(path: &OpenCodeSessionPath) -> &'static str {
    match path {
        OpenCodeSessionPath::StartedFresh => "started_fresh",
        OpenCodeSessionPath::ReopenedByLocator => "reopened_by_locator",
        OpenCodeSessionPath::ReseededFromBundle => "reseeded_from_bundle",
    }
}

fn codex_session_path_label(path: &CodexSessionPath) -> &'static str {
    match path {
        CodexSessionPath::StartedFresh => "started_fresh",
        CodexSessionPath::ReseededFromBundle => "reseeded_from_bundle",
    }
}

fn claude_session_path_label(path: &ClaudeSessionPath) -> &'static str {
    match path {
        ClaudeSessionPath::StartedFresh => "started_fresh",
        ClaudeSessionPath::ReseededFromBundle => "reseeded_from_bundle",
    }
}

fn gemini_session_path_label(path: &GeminiSessionPath) -> &'static str {
    match path {
        GeminiSessionPath::StartedFresh => "started_fresh",
        GeminiSessionPath::ReseededFromBundle => "reseeded_from_bundle",
    }
}

fn return_path_label(path: OpenCodeReturnPath) -> &'static str {
    match path {
        OpenCodeReturnPath::ReboundExistingSession => "rebound_existing_session",
        OpenCodeReturnPath::ReseededSession => "reseeded_session",
    }
}

/// How a picker block should be described to the operator.
///
/// The session finder fails closed for three distinct reasons, and each one
/// warrants a different, honest message. Conflating the last two produces a
/// dishonest "ambiguous; pick one with --session" claim for a unique match
/// that is actually blocked for a concrete reason (e.g. a stale launch
/// binding), with a repair the operator has already satisfied.
enum PickerBlockKind {
    /// No session matches the requested target at all.
    NoMatch,
    /// Two or more sessions match and explicit selection genuinely resolves it.
    Ambiguous,
    /// Exactly one (or otherwise non-ambiguous) match exists, but it is blocked
    /// for a specific reason that `--session` selection cannot resolve.
    SingleBlocked,
}

fn classify_picker_block(reason: &str, candidates: &[SessionFinderEntry]) -> PickerBlockKind {
    if candidates.is_empty() || reason.contains("no matching repo-local session found") {
        PickerBlockKind::NoMatch
    } else if reason.contains("ambiguous repo-local session match")
        || reason.contains("multiple repo-local sessions")
    {
        PickerBlockKind::Ambiguous
    } else {
        PickerBlockKind::SingleBlocked
    }
}

fn blocked_picker_response(reason: String, candidates: Vec<SessionFinderEntry>) -> CommandResponse {
    let kind = classify_picker_block(&reason, &candidates);
    let warnings = match kind {
        PickerBlockKind::NoMatch => {
            vec!["no matching session found for the requested target".to_owned()]
        }
        PickerBlockKind::Ambiguous => {
            vec!["session inference is ambiguous; explicit selection is required".to_owned()]
        }
        PickerBlockKind::SingleBlocked => {
            vec![
                "the matching review session is blocked and cannot be auto-selected; see reason"
                    .to_owned(),
            ]
        }
    };
    let repair_actions = match kind {
        PickerBlockKind::NoMatch => vec![
            "run rr review --pr <number> to create a new session".to_owned(),
            "run rr sessions --robot to inspect available sessions".to_owned(),
        ],
        PickerBlockKind::Ambiguous => {
            vec!["re-run with --session <id> or pass --pr <number> for a unique match".to_owned()]
        }
        PickerBlockKind::SingleBlocked => vec![
            "run rr sessions --robot to inspect the blocked session".to_owned(),
            "resolve the condition shown in reason (for a stale launch binding, run rr review --pr <number> from the repository worktree root to establish a fresh session binding)".to_owned(),
        ],
    };

    // The message names the actual situation: a picker only exists when there
    // are multiple candidates to pick from; with zero matches the truthful
    // message is that no review exists yet, and with a single blocked match the
    // truthful message surfaces the concrete blocking reason rather than
    // claiming a non-existent ambiguity.
    let message = match kind {
        PickerBlockKind::NoMatch => "no review session exists for this target yet".to_owned(),
        PickerBlockKind::Ambiguous => format!(
            "multiple review sessions match; pick one with --session <id> ({} candidates)",
            candidates.len()
        ),
        PickerBlockKind::SingleBlocked => {
            format!("the matching review session is blocked: {reason}")
        }
    };
    CommandResponse {
        outcome: OutcomeKind::Blocked,
        data: json!({
            "reason": reason,
            "candidates": candidates
                .into_iter()
                .map(|entry| json!({
                    "session_id": entry.session_id,
                    "repository": entry.repository,
                    "pull_request": entry.pull_request_number,
                    "attention_state": entry.attention_state,
                    "provider": entry.provider,
                    "updated_at": entry.updated_at,
                }))
                .collect::<Vec<_>>(),
        }),
        warnings,
        repair_actions,
        message,
    }
}

fn blocked_response(message: String, repair_actions: Vec<String>, data: Value) -> CommandResponse {
    CommandResponse {
        outcome: OutcomeKind::Blocked,
        data,
        warnings: Vec::new(),
        repair_actions,
        message,
    }
}

fn error_response(message: String) -> CommandResponse {
    CommandResponse {
        outcome: OutcomeKind::Error,
        data: json!({"reason": message}),
        warnings: Vec::new(),
        repair_actions: Vec::new(),
        message,
    }
}

/// Binary that must exist locally before a launch command may truthfully
/// claim it launched a provider session.
///
/// Scope is intentionally bounded to OpenCode in this slice: tier-a
/// providers (codex/gemini/claude) use bounded start/reseed semantics that
/// do not spawn a local binary at `rr review` time, and Copilot already has
/// its own verified, fail-closed launch path.
fn provider_launch_binary_for(runtime: &CliRuntime, provider: &str) -> Option<String> {
    match provider {
        "opencode" => Some(runtime.opencode_bin.clone()),
        _ => None,
    }
}

fn binary_resolves_locally(binary: &str) -> bool {
    let candidate = Path::new(binary);
    if candidate.components().count() > 1 {
        return candidate.is_file();
    }

    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|dir| {
        if dir.as_os_str().is_empty() {
            return false;
        }
        dir.join(binary).is_file()
    })
}

fn provider_binary_missing_response(
    command_name: &str,
    provider: &str,
    binary: &str,
) -> CommandResponse {
    let mut repair_actions = vec![format!(
        "install the {provider} CLI so `{binary}` resolves on PATH"
    )];
    if provider == "opencode" {
        repair_actions.push(format!(
            "or set {ENV_OPENCODE_BIN} to the full path of the opencode binary"
        ));
    }
    repair_actions.push(format!(
        "run rr doctor --provider {provider} for full launch preflight detail"
    ));

    blocked_response(
        format!(
            "{command_name} cannot launch provider '{provider}': binary '{binary}' was not found or is not executable"
        ),
        repair_actions,
        json!({
            "reason_code": "provider_binary_missing",
            "provider": provider,
            "binary": binary,
        }),
    )
}

/// Outcome of the best-effort GitHub-side review-target preflight.
enum ReviewTargetPreflight {
    /// GitHub confirmed the pull request exists.
    Verified,
    /// GitHub truth could not be obtained (gh missing, unauthenticated,
    /// network failure, or inaccessible repository). The launch proceeds,
    /// but the gap is surfaced loudly as a warning.
    Unverified { warning: String },
    /// GitHub definitively reported the pull request does not exist.
    Blocked(Box<CommandResponse>),
}

fn pr_not_found_blocked_response(
    command_name: &str,
    repository: &str,
    pull_request: u64,
    detail: &str,
) -> CommandResponse {
    blocked_response(
        format!(
            "{command_name} blocked: pull request {repository}#{pull_request} was not found on GitHub"
        ),
        vec![
            format!("list open pull requests with rr prs --repo {repository}"),
            format!("or run gh pr list --repo {repository} to verify the PR number"),
            "pass --repo owner/repo if the inferred repository is wrong".to_owned(),
        ],
        json!({
            "reason_code": "pr_not_found",
            "repository": repository,
            "pull_request": pull_request,
            "detail": detail,
        }),
    )
}

fn first_nonempty_line(text: &str) -> &str {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("unknown gh failure")
}

/// Best-effort, fail-closed-on-definitive-negative GitHub preflight for
/// `rr review`. A PR that GitHub definitively reports as missing blocks the
/// launch; every unverifiable condition proceeds with a loud warning so the
/// hermetic/offline paths keep working.
fn github_review_target_preflight(repository: &str, pull_request: u64) -> ReviewTargetPreflight {
    let Some((owner, repo_name)) = repository.split_once('/') else {
        return ReviewTargetPreflight::Unverified {
            warning: format!(
                "could not verify review target: repository slug '{repository}' is not in owner/repo form"
            ),
        };
    };

    let adapter = GhCliAdapter::new();
    match adapter.resolve_pr(owner, repo_name, pull_request) {
        Ok(_) => ReviewTargetPreflight::Verified,
        Err(GitHubAdapterError::TargetNotFound { .. }) => {
            ReviewTargetPreflight::Blocked(Box::new(pr_not_found_blocked_response(
                "rr review",
                repository,
                pull_request,
                "gh reported the pull request target as not found",
            )))
        }
        Err(GitHubAdapterError::GhNotFound) => ReviewTargetPreflight::Unverified {
            warning: format!(
                "could not verify {repository}#{pull_request} on GitHub: the GitHub CLI (gh) was not found; install gh and run gh auth login, then re-run rr review for a verified target"
            ),
        },
        Err(GitHubAdapterError::GhCommandFailed { stderr }) => {
            let lower = stderr.to_ascii_lowercase();
            let pr_definitively_missing = lower.contains("could not resolve to a pullrequest")
                || lower.contains("no pull requests found")
                || (lower.contains("pull request") && lower.contains("not found"));
            if pr_definitively_missing {
                return ReviewTargetPreflight::Blocked(Box::new(pr_not_found_blocked_response(
                    "rr review",
                    repository,
                    pull_request,
                    first_nonempty_line(&stderr),
                )));
            }

            let unauthenticated = lower.contains("gh auth login")
                || lower.contains("not logged in")
                || lower.contains("authentication");
            if unauthenticated {
                return ReviewTargetPreflight::Unverified {
                    warning: format!(
                        "could not verify {repository}#{pull_request} on GitHub: gh is not authenticated; run gh auth login, then re-run rr review for a verified target"
                    ),
                };
            }

            ReviewTargetPreflight::Unverified {
                warning: format!(
                    "could not verify {repository}#{pull_request} on GitHub: {}",
                    first_nonempty_line(&stderr)
                ),
            }
        }
        Err(err) => ReviewTargetPreflight::Unverified {
            warning: format!("could not verify {repository}#{pull_request} on GitHub: {err}"),
        },
    }
}

fn store_migration_blocked_response(command_name: &str, blocked_reason: &str) -> CommandResponse {
    blocked_response(
        format!("{command_name} blocked by store migration posture: {blocked_reason}"),
        vec![
            "run rr update --dry-run --robot to inspect migration posture details".to_owned(),
            "back up or export the local Roger store before any repair or reinstall step"
                .to_owned(),
            "if this is a local/unpublished build, install a published Roger release before relying on rr update"
                .to_owned(),
        ],
        json!({
            "reason_code": "store_migration_blocked",
            "command": command_name,
            "blocked_reason": blocked_reason,
            "migration": migration_policy_payload(),
        }),
    )
}

fn open_store_or_response(
    runtime: &CliRuntime,
    command_name: &str,
) -> std::result::Result<RogerStore, CommandResponse> {
    match RogerStore::open(&runtime.store_root) {
        Ok(store) => Ok(store),
        Err(StorageError::Conflict { entity, id }) if entity == "store_migration_policy" => {
            Err(store_migration_blocked_response(command_name, &id))
        }
        Err(err) => Err(error_response(format!("failed to open Roger store: {err}"))),
    }
}

fn find_workspace_root(start: &Path) -> Option<PathBuf> {
    for candidate in start.ancestors() {
        let extension_marker = candidate.join("apps/extension/src");
        let bridge_marker = candidate.join("packages/bridge/src/lib.rs");
        if extension_marker.exists() && bridge_marker.exists() {
            return Some(candidate.to_path_buf());
        }
    }
    None
}

fn copy_dir_recursive(source: &Path, destination: &Path) -> std::io::Result<()> {
    if !source.exists() {
        return Ok(());
    }
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&source_path, &destination_path)?;
        } else {
            if let Some(parent) = destination_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&source_path, &destination_path)?;
        }
    }
    Ok(())
}

/// Returns true when a packaged extension file is not needed at runtime and
/// should be excluded from the published bundle: unit-test files (`*.test.js`)
/// and the generated TypeScript bridge contract (`src/generated/bridge.ts`),
/// which is a verify-contracts artifact the browser never loads.
fn is_non_runtime_extension_file(relative: &Path) -> bool {
    if relative == Path::new("src/generated/bridge.ts") {
        return true;
    }
    relative
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".test.js"))
}

/// Removes non-runtime files from an assembled extension package directory so
/// the checksum manifest, asset manifest, and zip all describe the slim,
/// runtime-only file set.
fn prune_non_runtime_extension_files(package_dir: &Path) -> std::io::Result<()> {
    let files = collect_relative_files(package_dir)?;
    for rel in files {
        if is_non_runtime_extension_file(&rel) {
            fs::remove_file(package_dir.join(&rel))?;
        }
    }
    Ok(())
}

fn collect_relative_files(root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_relative_files_inner(root, root, &mut files)?;
    Ok(files)
}

fn collect_relative_files_inner(
    base: &Path,
    current: &Path,
    output: &mut Vec<PathBuf>,
) -> std::io::Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            collect_relative_files_inner(base, &path, output)?;
        } else {
            let rel = path
                .strip_prefix(base)
                .map_err(std::io::Error::other)?
                .to_path_buf();
            output.push(rel);
        }
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut text = String::with_capacity(digest.len() * 2);
    for byte in digest {
        text.push_str(&format!("{byte:02x}"));
    }
    text
}

fn sha256_prefixed_json<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_vec(value)
        .map(|bytes| format!("sha256:{}", sha256_hex(&bytes)))
        .map_err(|err| format!("failed to serialize draft payload: {err}"))
}

fn bridge_contract_snapshot() -> &'static str {
    r#"// Generated bridge contract snapshot for extension-side typing.
// Source of truth: packages/bridge/src/lib.rs (BridgeLaunchIntent / BridgeResponse).

export type BridgeAction =
  | 'start_review'
  | 'resume_review'
  | 'show_findings';

export interface BridgeLaunchIntent {
  action: BridgeAction;
  owner: string;
  repo: string;
  pr_number: number;
  head_ref?: string;
  instance?: string;
  session_id?: string;
}

export interface BridgeResponse {
  ok: boolean;
  action: string;
  message: string;
  session_id?: string;
  guidance?: string;
  warnings?: string[];
  candidates?: unknown;
  auto_selected_session?: boolean;
}
"#
}

fn compact_data(command: CommandKind, data: Value) -> Value {
    match command {
        CommandKind::Status => json!({
            "session_id": data
                .get("session")
                .and_then(|session| session.get("id"))
                .cloned()
                .unwrap_or(Value::Null),
            "repository": data
                .get("repo")
                .and_then(|repo| repo.get("repository"))
                .cloned()
                .unwrap_or(Value::Null),
            "pull_request": data
                .get("target")
                .and_then(|target| target.get("pull_request"))
                .cloned()
                .unwrap_or(Value::Null),
            "attention_state": data
                .get("attention")
                .and_then(|attention| attention.get("state"))
                .cloned()
                .unwrap_or(Value::Null),
            "findings_total": data
                .get("findings")
                .and_then(|findings| findings.get("total"))
                .cloned()
                .unwrap_or(Value::Null),
        }),
        CommandKind::Findings => json!({
            "session_id": data.get("session_id").cloned().unwrap_or(Value::Null),
            "count": data.get("count").cloned().unwrap_or(Value::Null),
            "items": data
                .get("items")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .map(|item| {
                            json!({
                                "finding_id": item.get("finding_id").cloned().unwrap_or(Value::Null),
                                "title": item.get("title").cloned().unwrap_or(Value::Null),
                                "triage_state": item.get("triage_state").cloned().unwrap_or(Value::Null),
                                "outbound_state": item.get("outbound_state").cloned().unwrap_or(Value::Null),
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
        }),
        CommandKind::Sessions => json!({
            "count": data.get("count").cloned().unwrap_or(Value::Null),
            "truncated": data.get("truncated").cloned().unwrap_or(Value::Null),
            "items": data
                .get("items")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .map(|item| {
                            json!({
                                "session_id": item.get("session_id").cloned().unwrap_or(Value::Null),
                                "repo": item.get("repo").cloned().unwrap_or(Value::Null),
                                "pull_request": item
                                    .get("target")
                                    .and_then(|target| target.get("pull_request"))
                                    .cloned()
                                    .unwrap_or(Value::Null),
                                "attention_state": item.get("attention_state").cloned().unwrap_or(Value::Null),
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
        }),
        CommandKind::Prs => json!({
            "repository": data.get("repository").cloned().unwrap_or(Value::Null),
            "count": data.get("count").cloned().unwrap_or(Value::Null),
            "items": data
                .get("items")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .map(|item| {
                            json!({
                                "pr_number": item.get("pr_number").cloned().unwrap_or(Value::Null),
                                "roger_state": item.get("roger_state").cloned().unwrap_or(Value::Null),
                                "session_id": item.get("session_id").cloned().unwrap_or(Value::Null),
                                "next_command": item.get("next_command").cloned().unwrap_or(Value::Null),
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
        }),
        CommandKind::Search => json!({
            "query": data.get("query").cloned().unwrap_or(Value::Null),
            "requested_query_mode": data.get("requested_query_mode").cloned().unwrap_or(Value::Null),
            "resolved_query_mode": data.get("resolved_query_mode").cloned().unwrap_or(Value::Null),
            "search_plan": data.get("search_plan").cloned().unwrap_or(Value::Null),
            "retrieval_mode": data.get("retrieval_mode").cloned().unwrap_or(Value::Null),
            "mode": data.get("mode").cloned().unwrap_or(Value::Null),
            "scope_bucket": data.get("scope_bucket").cloned().unwrap_or(Value::Null),
            "candidate_included": data.get("candidate_included").cloned().unwrap_or(Value::Null),
            "count": data.get("count").cloned().unwrap_or(Value::Null),
            "truncated": data.get("truncated").cloned().unwrap_or(Value::Null),
            "items": data
                .get("items")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .map(|item| {
                            json!({
                                "kind": item.get("kind").cloned().unwrap_or(Value::Null),
                                "id": item.get("id").cloned().unwrap_or(Value::Null),
                                "score": item.get("score").cloned().unwrap_or(Value::Null),
                                "title": item.get("title").cloned().unwrap_or(Value::Null),
                                "memory_lane": item.get("memory_lane").cloned().unwrap_or(Value::Null),
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
        }),
        CommandKind::RobotDocs => json!({
            "topic": data.get("topic").cloned().unwrap_or(Value::Null),
            "version": data.get("version").cloned().unwrap_or(Value::Null),
            "count": data
                .get("items")
                .and_then(Value::as_array)
                .map(|items| items.len())
                .unwrap_or_default(),
            "items": data.get("items").cloned().unwrap_or(Value::Array(Vec::new())),
        }),
        _ => data,
    }
}

fn next_id(prefix: &str) -> String {
    let seq = ID_SEQ.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    format!("{prefix}-{}-{pid}-{seq}", time::now_ts())
}

/// Rewrite the container verbs (`send`, `setup`, `api`) into the underlying
/// command's argv so parse_args, the flag loop, every check, and every handler
/// stay byte-identical to the old direct invocations (and emit the same schema
/// ids). `send edit` maps to the `edit` command (a local, human-only draft
/// revision action that rejects `--robot`).
fn normalize_container_argv(argv: &[String]) -> Result<Vec<String>, String> {
    fn rebuild(head: &[&str], rest: &[String]) -> Vec<String> {
        let mut out: Vec<String> = head.iter().map(|token| (*token).to_owned()).collect();
        out.extend_from_slice(rest);
        out
    }

    match argv[0].as_str() {
        "send" => {
            let mapped = match argv.get(1).map(String::as_str) {
                Some("triage") => "triage",
                Some("draft") => "draft",
                Some("edit") => "edit",
                Some("approve") => "approve",
                Some("post") => "post",
                Some(other) if !other.starts_with('-') => {
                    return Err(format!(
                        "unknown send subcommand: {other} (expected triage, draft, edit, approve, or post)"
                    ));
                }
                _ => {
                    return Err(
                        "rr send requires a subcommand: triage, draft, edit, approve, or post".to_owned(),
                    );
                }
            };
            Ok(rebuild(&[mapped], &argv[2..]))
        }
        "setup" => match argv.get(1).map(String::as_str) {
            Some("extension") => Ok(rebuild(&["extension", "setup"], &argv[2..])),
            Some("doctor") => Ok(rebuild(&["extension", "doctor"], &argv[2..])),
            Some("fetch") => Ok(rebuild(&["extension", "fetch"], &argv[2..])),
            Some("uninstall") => Ok(rebuild(&["extension", "uninstall"], &argv[2..])),
            Some("update") => Ok(rebuild(&["update"], &argv[2..])),
            Some("assets") => Ok(rebuild(&["assets"], &argv[2..])),
            Some(other) if !other.starts_with('-') => Err(format!(
                "unknown setup subcommand: {other} (expected extension, doctor, fetch, update, uninstall, or assets)"
            )),
            _ => Err(
                "rr setup requires a subcommand: extension, doctor, fetch, update, uninstall, or assets"
                    .to_owned(),
            ),
        },
        "api" => match argv.get(1).map(String::as_str) {
            Some("docs") => Ok(rebuild(&["robot-docs"], &argv[2..])),
            Some(other) if !other.starts_with('-') => {
                Err(format!("unknown api subcommand: {other} (expected docs)"))
            }
            _ => Err("rr api requires a subcommand: docs".to_owned()),
        },
        _ => Ok(argv.to_vec()),
    }
}

/// Map the first argv token to a per-command help topic. Aliases collapse onto
/// their preferred name (`prs`->queue, `tui`->open). Returns None for unknown
/// leading tokens so parse_args falls through to its normal unknown-command
/// error instead of printing help.
fn help_topic_for(argv: &[String]) -> Option<&'static str> {
    match argv[0].as_str() {
        "doctor" => Some("doctor"),
        "queue" | "prs" => Some("queue"),
        "review" => Some("review"),
        "resume" => Some("resume"),
        "return" => Some("return"),
        "open" | "tui" => Some("open"),
        "findings" => Some("findings"),
        "search" => Some("search"),
        "sessions" => Some("sessions"),
        "status" => Some("status"),
        "timeline" => Some("timeline"),
        "memory" => Some("memory"),
        "clarify" => Some("clarify"),
        "send" => Some("send"),
        "triage" => Some("triage"),
        "draft" => Some("draft"),
        "edit" => Some("edit"),
        "approve" => Some("approve"),
        "post" => Some("post"),
        "setup" => Some("setup"),
        "extension" => Some("extension"),
        "assets" => Some("assets"),
        "update" => Some("update"),
        "api" => Some("api"),
        "robot-docs" => Some("robot-docs"),
        "agent" => Some("agent"),
        "init" => Some("init"),
        "bridge" => Some("bridge"),
        _ => None,
    }
}

/// Focused per-command usage block. `send`/`setup`/`api` show their container
/// help (subcommands grouped). Unknown topics fall back to the global usage.
fn command_usage(topic: &str) -> String {
    let body = match topic {
        "doctor" => {
            "rr doctor — check whether Roger can run.\n\nUsage:\n  rr doctor [--provider opencode|codex|gemini|claude|copilot] [--robot]"
        }
        "queue" => {
            "rr queue — list open pull requests as a review queue (alias: rr prs).\n\nUsage:\n  rr queue [--repo owner/repo] [--limit <n>] [--robot]"
        }
        "review" => {
            "rr review — start or re-enter a review.\n\nUsage:\n  rr review --pr <number> [--repo owner/repo] [--provider opencode|codex|gemini|claude|copilot] [--fresh] [--dry-run] [--robot]\n  rr review --resume [--pr <number> | --session <id>] [--repo owner/repo] [--dry-run] [--robot]\n\nBy default rr review reuses a non-terminal session for the same repo/PR; pass --fresh to force a new session.\nNote: --resume routes to the resume handler (rr resume)."
        }
        "resume" => {
            "rr resume — re-enter an existing review (preferred form: rr review --resume).\n\nUsage:\n  rr resume [--repo owner/repo] [--pr <number>] [--session <id>] [--dry-run] [--robot]"
        }
        "return" => {
            "rr return — deliberate control handoff back to Roger from a provider session.\n\nUsage:\n  rr return [--repo owner/repo] [--pr <number>] [--session <id>] [--robot]"
        }
        "open" => {
            "rr open — open the local review cockpit (alias: rr tui).\n\nUsage:\n  rr open [--repo owner/repo] [--pr <number>] [--session <id>]"
        }
        "findings" => {
            "rr findings — inspect and search review output.\n\nUsage:\n  rr findings [--repo owner/repo] [--pr <number>] [--session <id>] [--robot]\n  rr findings --query <text> [--repo owner/repo] [--limit <n>] [--robot]   prior-review search\n  rr findings --sessions [--repo owner/repo] [--pr <number>] [--robot]      session listing\n\nNote: --query routes to the search handler, --sessions to the sessions handler."
        }
        "search" => {
            "rr search — prior-review corpus search (preferred form: rr findings --query).\n\nUsage:\n  rr search --query <text> [--query-mode auto|exact_lookup|recall|related_context|candidate_audit] [--repo owner/repo] [--limit <n>] [--robot]"
        }
        "sessions" => {
            "rr sessions — list local review sessions (preferred form: rr findings --sessions).\n\nUsage:\n  rr sessions [--repo owner/repo] [--pr <number>] [--attention <state[,state...]>] [--limit <n>] [--all] [--robot]\n\nThe default human view groups by repo/PR and shows the five most-recent per PR; pass --all to list every session."
        }
        "status" => {
            "rr status — session attention snapshot.\n\nUsage:\n  rr status [--repo owner/repo] [--pr <number>] [--session <id>] [--robot]"
        }
        "timeline" => {
            "rr timeline — chronological run -> stage -> posted-action history for a session (the same data the TUI Timeline screen shows).\n\nUsage:\n  rr timeline [--repo owner/repo] [--pr <number>] [--session <id>] [--robot]"
        }
        "memory" => {
            "rr memory — review and resolve durable memory-review candidates.\n\nUsage:\n  rr memory review [--repo owner/repo] [--pr <number>] [--session <id>] [--limit <n>] [--robot]   list pending requests\n  rr memory accept --request <id> [--robot]   materialize an accepted memory item\n  rr memory reject --request <id> [--robot]   resolve (reject) a candidate\n\naccept/reject fail closed on unknown or already-resolved request ids."
        }
        "clarify" => {
            "rr clarify — create and list durable clarification requests.\n\nUsage:\n  rr clarify --finding <id> (--body <text> | --body-file <path>) [--repo owner/repo] [--pr <number>] [--session <id>] [--robot]   create\n  rr clarify --list [--session <id>] [--limit <n>] [--robot]                                                                    list open clarifications\n\nCreation binds the clarification to the finding's session lineage and fails closed on a missing or unknown --finding."
        }
        "send" => {
            "rr send — explicitly triage/draft/edit/approve/post outbound communication.\n\nUsage:\n  rr send triage --finding <id>... --state accepted|ignored|needs_follow_up|resolved [--session <id>] [--robot]\n  rr send draft (--finding <id>... | --all-findings) [--session <id>] [--robot]\n  rr send edit --draft <draft-id> (--body-file <path> | --editor)\n  rr send approve --batch <draft-batch-id> [--session <id>] [--robot]\n  rr send post --batch <draft-batch-id> [--session <id>] [--robot]\n\nrr send edit revises a local outbound draft body; editing an approved batch revokes its approval and forces re-approval. It is a local human action and does not support --robot.\nThe other subcommands route to the same fail-closed handler as the old rr triage/draft/approve/post name and emit the same robot schema id."
        }
        "triage" => {
            "rr triage — record a local triage decision (preferred form: rr send triage).\n\nUsage:\n  rr triage --finding <id>... --state accepted|ignored|needs_follow_up|resolved [--repo owner/repo] [--pr <number>] [--session <id>] [--robot]"
        }
        "draft" => {
            "rr draft — materialize local outbound draft batches (preferred form: rr send draft).\n\nUsage:\n  rr draft (--finding <id>... | --all-findings) [--repo owner/repo] [--pr <number>] [--session <id>] [--robot]"
        }
        "edit" => {
            "rr send edit — revise a local outbound draft body before approval/posting.\n\nUsage:\n  rr send edit --draft <draft-id> --body-file <path>\n  rr send edit --draft <draft-id> --editor\n\n--body-file reads the replacement body from a file; --editor opens $VISUAL/$EDITOR/vi on a temp file seeded with the current body. Exactly one is required. An empty body is rejected, and an unchanged body is a no-op. Editing a draft whose batch was already approved revokes that approval and forces re-approval (rr send approve --batch <id>); a posted batch cannot be edited. Local human action: --robot is not supported."
        }
        "approve" => {
            "rr approve — record a local approval token (preferred form: rr send approve).\n\nUsage:\n  rr approve --batch <draft-batch-id> [--repo owner/repo] [--pr <number>] [--session <id>] [--robot]"
        }
        "post" => {
            "rr post — post one exact approved batch to GitHub (preferred form: rr send post).\n\nUsage:\n  rr post --batch <draft-batch-id> [--repo owner/repo] [--pr <number>] [--session <id>] [--robot]"
        }
        "setup" => {
            "rr setup — install, update, and repair local integrations.\n\nUsage:\n  rr setup extension [--browser edge|chrome|brave] [--robot]     set up the browser companion\n  rr setup doctor [--browser b] [--robot]                       verify the companion path\n  rr setup fetch [--version YYYY.MM.DD] [--robot]               download the published package\n  rr setup uninstall [--robot]                                  remove host-registration assets\n  rr setup update [--channel stable|rc] [--version <v>] [--yes|-y] [--dry-run] [--robot]   self-update rr\n  rr setup assets install|status|verify [--robot]              manage semantic assets\n\nEach form routes to the same handler as the old rr extension/update/assets name."
        }
        "extension" => {
            "rr extension — browser companion setup (preferred form: rr setup ...).\n\nUsage:\n  rr extension setup|doctor|fetch|uninstall [--browser edge|chrome|brave] [--package-dir <dir>] [--install-root <dir>] [--robot]\n  rr extension fetch [--version <YYYY.MM.DD[-rc.N]>] [--download-root <dir>] [--robot]"
        }
        "assets" => {
            "rr assets — manage semantic assets (preferred form: rr setup assets ...).\n\nUsage:\n  rr assets install|status|verify [--robot]"
        }
        "update" => {
            "rr update — self-update rr (preferred form: rr setup update).\n\nUsage:\n  rr update [--channel stable|rc] [--version <YYYY.MM.DD[-rc.N]>] [--api-root <url>] [--download-root <dir>] [--target <tag>] [--yes|-y] [--dry-run] [--robot]"
        }
        "api" => {
            "rr api — machine-contract documentation surface.\n\nUsage:\n  rr api docs guide|commands|schemas|workflows [--robot]\n\nRoutes to the robot-docs handler."
        }
        "robot-docs" => {
            "rr robot-docs — machine-readable command/schema reference (preferred form: rr api docs).\n\nUsage:\n  rr robot-docs [guide|commands|schemas|workflows] [--robot]"
        }
        "agent" => {
            "rr agent — in-session worker transport (separate from --robot).\n\nUsage:\n  rr agent <operation> --task-file <path> [--request-file <path>] [--context-file <path>] [--capability-file <path>]"
        }
        "init" => {
            "rr init — bootstrap the local Roger store (usually auto-bootstrapped).\n\nUsage:\n  rr init [--robot]"
        }
        "bridge" => {
            "rr bridge — dev/repair surface for native-host registration and contracts.\n\nUsage:\n  rr bridge export-contracts|verify-contracts|pack-extension|install|uninstall [--extension-id <id>] [--bridge-binary <path>] [--install-root <dir>] [--output-dir <dir>] [--robot]\n\nPrefer rr setup ... for normal operator flows."
        }
        _ => return format!("{}\n", usage_text()),
    };
    body.to_owned()
}

fn usage_text() -> &'static str {
    r#"Roger Reviewer (rr) — local-first pull request review for GitHub.
Durable sessions, structured findings, and an explicit approval gate before
anything is posted back to GitHub.

Usage:
  rr <command> [options]

The seven verbs:
  rr doctor                                    check whether Roger can run
  rr queue                                     choose review work
  rr review                                    start or re-enter review work
  rr open                                      use the local cockpit
  rr findings                                  inspect and search review output
  rr send                                      triage/draft/approve/post outbound comms
  rr setup                                     install, update, and repair integrations

Primary flow:
  rr doctor --provider <name>                  check local + provider setup
  rr queue                                     list open PRs needing review
  rr review --pr <number>                      start a review
  rr review --resume --pr <number>             re-enter an existing review
  rr open                                      open the local review cockpit

Choose and enter review work:
  rr queue [--repo owner/repo] [--limit <n>] [--robot]
  rr review --pr <number> [--repo owner/repo] [--provider opencode|codex|gemini|claude|copilot] [--interactive] [--surface cli|tui|extension|bridge] [--dry-run] [--robot]
  rr review --resume [--pr <number> | --session <id>] [--repo owner/repo] [--interactive] [--surface cli|tui|extension|bridge] [--dry-run] [--robot]
  rr open [--repo owner/repo] [--pr <number>] [--session <id>]
  rr return [--repo owner/repo] [--pr <number>] [--session <id>] [--interactive] [--surface cli|tui|extension|bridge] [--robot]

Inspect and search review output:
  rr status [--repo owner/repo] [--pr <number>] [--session <id>] [--robot]
  rr findings [--repo owner/repo] [--pr <number>] [--session <id>] [--robot]
  rr findings --query <text> [--query-mode auto|exact_lookup|recall|related_context|candidate_audit] [--repo owner/repo] [--limit <n>] [--robot]
  rr findings --sessions [--repo owner/repo] [--pr <number>] [--attention <state[,state...]>] [--limit <n>] [--robot]
  rr timeline [--repo owner/repo] [--pr <number>] [--session <id>] [--robot]   run -> stage -> posted-action history

Resolve memory candidates and clarifications:
  rr memory review [--repo owner/repo] [--pr <number>] [--session <id>] [--limit <n>] [--robot]   list pending memory-review requests
  rr memory accept --request <id> [--robot]                                                        materialize an accepted memory candidate
  rr memory reject --request <id> [--robot]                                                        resolve (reject) a memory candidate
  rr clarify --finding <id> (--body <text> | --body-file <path>) [--session <id>] [--robot]         create a durable clarification
  rr clarify --list [--session <id>] [--robot]                                                      list open clarifications

Send to GitHub, explicitly gated (rr send <sub>):
  rr send triage --finding <id>... --state accepted|ignored|needs_follow_up|resolved [--session <id>] [--robot]
  rr send draft (--finding <id>... | --all-findings) [--session <id>] [--robot]
  rr send edit --draft <draft-id> (--body-file <path> | --editor)
  rr send approve --batch <draft-batch-id> [--session <id>] [--robot]
  rr send post --batch <draft-batch-id> [--session <id>] [--robot]

Set up, update, and repair (rr setup <sub>):
  rr setup extension [--browser edge|chrome|brave] [--robot]
  rr setup doctor [--browser edge|chrome|brave] [--live] [--robot]
  rr setup fetch [--version <YYYY.MM.DD[-rc.N]>] [--robot]
  rr setup uninstall [--robot]
  rr setup update [--channel stable|rc] [--version <YYYY.MM.DD[-rc.N]>] [--yes|-y] [--dry-run] [--robot]
  rr setup assets install|status|verify [--robot]

Machine interfaces:
  rr api docs guide|commands|schemas|workflows [--robot]
  rr agent <operation> --task-file <path> [--request-file <path>] [--context-file <path>] [--capability-file <path>]

Compatibility names (still supported; prefer the forms above):
  rr prs = rr queue    rr tui = rr open    rr resume = rr review --resume    rr search = rr findings --query
  rr sessions = rr findings --sessions    rr triage|draft|approve|post = rr send <sub>
  rr extension|update|assets = rr setup <sub>    rr robot-docs = rr api docs
  rr tui [--repo owner/repo] [--pr <number>] [--session <id>]
  rr search --query <text> [--query-mode auto|exact_lookup|recall|related_context|candidate_audit] [--repo owner/repo] [--limit <n>] [--robot]

Agent transport:
  - rr agent is the dedicated in-session worker transport; it is separate from --robot
  - current live rr agent operations cover context/status/search/finding/artifact reads, advisory clarification or follow-up proposals, and worker.submit_stage_result
  - rr agent emits rr.agent.response.v1 envelopes over the canonical worker operation response payload instead of reusing the --robot surface

Provider support on the current live CLI surface:
  - opencode is the first-class tier-b continuity path; rr resume can reopen and rr return is supported
  - codex, gemini, and claude are bounded tier-a providers; start/reseed/raw-capture only, no locator reopen or rr return
  - copilot is feature-gated bounded tier-b support; enable with RR_ENABLE_COPILOT_PROVIDER=1 for verified start, locator/session-id reopen, rr return, and honest ResumeBundle reseed fallback
  - copilot review/resume/return accept --interactive (Copilot only, gate on, not with --robot) to hand the terminal to Copilot with inherited stdio; Roger still runs verified-start checks + session binding after exit and records hook audit events
  - pi-agent is not part of the current live CLI surface

Safety notes:
  - rr triage records local triage only
  - rr draft materializes local draft batches only
  - rr send edit revises a local draft body only; editing an approved batch revokes the approval and forces re-approval, and a posted batch cannot be edited
  - rr approve records a local approval token only
  - rr post executes only one exact Roger-approved stored batch
  - stale persisted review state fails closed before Roger derives, approves, or posts outbound payloads

Browser note:
  - Chrome 137+ ignores --load-extension; load the unpacked package once via chrome://extensions
  - Edge 150+ ignores --load-extension too; load the unpacked package once via edge://extensions (Brave still honored the flag-based launch at last verification)

Update note:
  - after a successful binary replacement, if extension integration was ever set up, rr setup update also refreshes the extension package and rewrites native-messaging host manifests; failures here degrade to extension_refresh_failed warnings and never roll back the binary update

More:
  - machine-readable command and schema reference: rr api docs [guide|commands|schemas|workflows]
  - per-command help: rr <command> --help
"#
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use tempfile::tempdir;

    const TEST_EXTENSION_MANIFEST_KEY: &str = "MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEA2FDtjF8sDdzic557+0PBHZDHc0NoxOFpmh3YFtXyvMxDRqF4TeujY6y4SC5JjqUjnpbUYjMm7lNJFvd2kiauFYFBcAyJLeGGKUSzfgrr6LhpP8SRvd7+lZO6KsjsIkrJxOr8aL8uMmkwAIaC7owRO7CRjKgqaRcEublt6Xk3WfW/UKSxVry286T2DKlH+2zhz5xbnpldnWgnPEo1tdO/7Z1RfYYWZCZ47bFudhBc5Q54diUeIWtYgeSmsPmWu2gxHcaji2gIGRwtgsoTR+Fsnm1wB0XX7PsmR8iF17YgJIXeit464GQbzLt6o5tYFFXxzuU4Mrbyla0Dw76shE4eEQIDAQAB";
    const TEST_EXTENSION_MANIFEST_ID: &str = "djbjigobohmlljboggckmhhnoeldinlp";

    fn finder_entry(pull_request_number: u64) -> SessionFinderEntry {
        SessionFinderEntry {
            session_id: format!("session-{pull_request_number}"),
            repository: "owner/repo".to_owned(),
            pull_request_number,
            attention_state: "findings_ready".to_owned(),
            provider: "opencode".to_owned(),
            updated_at: 0,
        }
    }

    #[test]
    fn picker_block_for_single_stale_binding_is_not_called_ambiguous() {
        // Regression: a unique match blocked by a stale launch binding used to
        // be mislabeled "session inference is ambiguous; ... ({} candidates)"
        // with a self-referential "--session <id>" repair the caller had
        // already satisfied. The truthful block surfaces the concrete reason.
        let reason =
            "launch binding is stale: binding cwd is outside current worktree root".to_owned();
        assert!(matches!(
            classify_picker_block(&reason, &[finder_entry(2)]),
            PickerBlockKind::SingleBlocked
        ));

        let response = blocked_picker_response(reason.clone(), vec![finder_entry(2)]);
        assert_eq!(response.outcome, OutcomeKind::Blocked);
        let blob = serde_json::to_string(&response.warnings).unwrap()
            + &serde_json::to_string(&response.repair_actions).unwrap()
            + &response.message;
        assert!(
            !blob.contains("ambiguous") && !blob.contains("multiple review sessions"),
            "single blocked session must not claim ambiguity: {blob}"
        );
        assert!(
            response.message.contains("launch binding is stale"),
            "message must surface the concrete blocking reason: {}",
            response.message
        );
    }

    #[test]
    fn picker_block_classification_covers_no_match_and_genuine_ambiguity() {
        assert!(matches!(
            classify_picker_block(
                "no matching repo-local session found for pull request 9",
                &[]
            ),
            PickerBlockKind::NoMatch
        ));
        let multi = vec![finder_entry(2), finder_entry(6)];
        assert!(matches!(
            classify_picker_block(
                "multiple repo-local sessions found; open session picker",
                &multi
            ),
            PickerBlockKind::Ambiguous
        ));
        let response = blocked_picker_response(
            "ambiguous repo-local session match; picker required".to_owned(),
            multi,
        );
        assert!(
            response.message.contains("multiple review sessions match"),
            "genuine ambiguity must still offer the picker: {}",
            response.message
        );
    }

    fn test_runtime(cwd: PathBuf, store_root: PathBuf) -> CliRuntime {
        CliRuntime {
            cwd,
            store_root,
            opencode_bin: DEFAULT_OPENCODE_BIN.to_owned(),
        }
    }

    #[test]
    fn batch_flag_rejects_a_flag_shaped_value() {
        // Regression: `rr approve --pr 2 --batch --robot` used to swallow
        // --robot as the batch id and silently drop out of robot mode.
        let argv: Vec<String> = ["approve", "--pr", "2", "--batch", "--robot"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let err = parse_args(&argv).expect_err("flag-shaped --batch value must be rejected");
        assert!(
            err.contains("--batch requires"),
            "unexpected parse error: {err}"
        );
    }

    fn argv(tokens: &[&str]) -> Vec<String> {
        tokens.iter().map(|token| (*token).to_owned()).collect()
    }

    #[test]
    fn send_edit_parses_body_file_form() {
        let parsed = parse_args(&argv(&[
            "send",
            "edit",
            "--draft",
            "draft-1",
            "--body-file",
            "/tmp/body.md",
        ]))
        .expect("send edit --body-file must parse");
        assert_eq!(parsed.command, CommandKind::Edit);
        assert_eq!(parsed.edit_draft_id.as_deref(), Some("draft-1"));
        assert_eq!(
            parsed.edit_body_file.as_deref(),
            Some(Path::new("/tmp/body.md"))
        );
        assert!(!parsed.edit_editor);
    }

    #[test]
    fn send_edit_parses_editor_form() {
        let parsed = parse_args(&argv(&["send", "edit", "--draft", "draft-1", "--editor"]))
            .expect("send edit --editor must parse");
        assert_eq!(parsed.command, CommandKind::Edit);
        assert_eq!(parsed.edit_draft_id.as_deref(), Some("draft-1"));
        assert!(parsed.edit_editor);
        assert!(parsed.edit_body_file.is_none());
    }

    #[test]
    fn send_edit_requires_a_body_source() {
        let err = parse_args(&argv(&["send", "edit", "--draft", "draft-1"]))
            .expect_err("send edit without a body source must be rejected");
        assert!(
            err.contains("requires --body-file <path> or --editor"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn send_edit_rejects_both_body_sources() {
        let err = parse_args(&argv(&[
            "send",
            "edit",
            "--draft",
            "draft-1",
            "--body-file",
            "/tmp/body.md",
            "--editor",
        ]))
        .expect_err("send edit with both body sources must be rejected");
        assert!(
            err.contains("either --body-file or --editor, not both"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn send_edit_requires_a_draft_id() {
        let err = parse_args(&argv(&["send", "edit", "--body-file", "/tmp/body.md"]))
            .expect_err("send edit without --draft must be rejected");
        assert!(err.contains("requires --draft"), "unexpected error: {err}");
    }

    #[test]
    fn send_edit_rejects_robot() {
        // Same posture as rr agent: a local editing action, not a --robot
        // transport, so --robot is rejected at parse time.
        let err = parse_args(&argv(&[
            "send",
            "edit",
            "--draft",
            "draft-1",
            "--body-file",
            "/tmp/body.md",
            "--robot",
        ]))
        .expect_err("send edit must reject --robot");
        assert!(
            err.contains("does not support --robot"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn send_edit_rejects_foreign_flags() {
        let err = parse_args(&argv(&[
            "send",
            "edit",
            "--draft",
            "draft-1",
            "--body-file",
            "/tmp/body.md",
            "--session",
            "session-1",
        ]))
        .expect_err("send edit must reject unrelated flags");
        assert!(
            err.contains("rr send edit only supports --draft, --body-file, and --editor"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn edit_flags_are_rejected_on_other_commands() {
        let err = parse_args(&argv(&[
            "approve", "--draft", "draft-1", "--batch", "batch-1",
        ]))
        .expect_err("--draft must be edit-only");
        assert!(
            err.contains("--draft/--editor are only supported by rr send edit"),
            "unexpected error: {err}"
        );

        // --body-file is now shared by rr send edit and rr clarify, so it is
        // rejected on unrelated commands with the shared message.
        let body_err = parse_args(&argv(&["approve", "--body-file", "/tmp/body.md"]))
            .expect_err("--body-file must be edit/clarify-only");
        assert!(
            body_err.contains("--body-file is only supported by rr send edit and rr clarify"),
            "unexpected error: {body_err}"
        );
    }

    #[test]
    fn picker_candidates_map_finder_entries_one_to_one() {
        let entries = vec![finder_entry(2), finder_entry(6)];
        let candidates = picker_candidates_from_finder_entries(&entries);
        assert_eq!(candidates.len(), 2);
        for (entry, candidate) in entries.iter().zip(candidates.iter()) {
            assert_eq!(candidate.session_id, entry.session_id);
            assert_eq!(candidate.repository, entry.repository);
            assert_eq!(candidate.pull_request, entry.pull_request_number);
            assert_eq!(candidate.provider, entry.provider);
            assert_eq!(candidate.attention_state, entry.attention_state);
            assert_eq!(candidate.updated_at, entry.updated_at);
            // The finder projection does not carry a continuity tier.
            assert_eq!(candidate.continuity_tier, None);
        }
    }

    #[test]
    fn picker_candidates_from_empty_entries_are_empty() {
        assert!(picker_candidates_from_finder_entries(&[]).is_empty());
    }

    #[test]
    fn copilot_is_never_classified_as_a_planned_not_live_review_provider() {
        // Regression for the cross-surface honesty fix: copilot is feature-gated,
        // not "planned but not live". The planned list must stay empty so
        // rr review / rr resume / robot-docs never contradict the doctor
        // classification (which reports copilot as feature_gated_disabled).
        let runtime = test_runtime(PathBuf::from("."), PathBuf::from("."));
        assert!(
            runtime_planned_not_live_review_providers(&runtime).is_empty(),
            "no review provider is genuinely planned-not-live on the current live CLI surface; copilot is feature-gated"
        );
        // The feature-gated-disabled list only ever names copilot (or nothing
        // once the gate is enabled), regardless of the current gate state.
        let gated = runtime_feature_gated_disabled_review_providers(&runtime);
        assert!(
            gated.iter().all(|p| *p == session_copilot::PROVIDER_ID),
            "feature-gated-disabled list must only ever contain copilot: {gated:?}"
        );
    }

    #[test]
    fn copilot_gate_off_capability_is_feature_gated_tier_b_not_planned() {
        // Regression (rr-doctor-copilot-gate-honesty): with the feature gate OFF
        // copilot must be classified as feature-gated, disabled-but-enableable
        // tier-b support, NOT the planned_not_live/tier_a_planned/admission_pending
        // classification reserved for genuinely-planned providers.
        let tmp = tempdir().expect("tempdir");
        let runtime = test_runtime(tmp.path().to_path_buf(), tmp.path().join("store"));
        let capability = copilot_feature_gated_disabled_provider_capability(&runtime);

        assert_eq!(capability["status"], "feature_gated_disabled");
        assert_eq!(capability["tier"], "tier_b_feature_gated");
        assert_eq!(capability["support_tier"], "tier_b_feature_gated");
        assert_eq!(capability["surface_class"], "review_bounded");
        assert_ne!(capability["status"], "planned_not_live");
        assert_ne!(capability["tier"], "tier_a_planned");
        assert_ne!(capability["surface_class"], "admission_pending");

        // It is enableable but not yet live: doctor may inspect prerequisites,
        // but the live-launch capabilities stay false until the gate is enabled.
        assert_eq!(capability["supports"]["doctor"], true);
        assert_eq!(capability["supports"]["review_start"], false);
        assert_eq!(capability["supports"]["resume_reopen"], false);

        // The honest classification names the documented enable step.
        let notes = capability["notes"].as_str().unwrap_or_default();
        let status_reason = capability["status_reason"].as_str().unwrap_or_default();
        assert!(
            notes.contains("RR_ENABLE_COPILOT_PROVIDER"),
            "notes must name the documented enable env var: {notes}"
        );
        assert!(
            status_reason.contains("rr_enable_copilot_provider"),
            "status_reason must reference the documented gate: {status_reason}"
        );
    }

    #[test]
    fn doctor_unknown_provider_does_not_recommend_pi_agent() {
        // Regression (rr-doctor-piagent-recommendation-honesty, leg B): the
        // unknown-provider recommendation must not list pi-agent, which would
        // immediately fail closed (not_supported, supports.doctor=false) if
        // followed.
        let tmp = tempdir().expect("tempdir");
        let runtime = test_runtime(tmp.path().to_path_buf(), tmp.path().join("store"));
        let parsed = parse_args(&[
            "doctor".to_owned(),
            "--provider".to_owned(),
            "bogusxyz".to_owned(),
            "--robot".to_owned(),
        ])
        .expect("parse doctor bogusxyz");
        let response = handle_doctor(&parsed, &runtime);

        assert_eq!(response.outcome, OutcomeKind::Blocked);
        let supported = response.data["supported_providers"]
            .as_array()
            .expect("supported_providers array");
        assert!(
            !supported.iter().any(|p| p == "pi-agent"),
            "unknown-provider recommendation must not list pi-agent: {supported:?}"
        );
        let non_live = response.data["non_live_providers"]
            .as_array()
            .expect("non_live_providers array");
        assert!(
            non_live.iter().any(|p| p == "pi-agent"),
            "pi-agent must be surfaced as a non-live provider: {non_live:?}"
        );
        let repair_blob = response.repair_actions.join(" ");
        assert!(
            !repair_blob.contains("pi-agent"),
            "repair action must not steer the operator to pi-agent: {repair_blob}"
        );
    }

    #[test]
    fn pi_agent_capability_is_not_a_live_review_lane() {
        // Regression (rr-doctor-piagent-recommendation-honesty, leg A): the
        // auth-preflight guidance keys off supports.review_start; pi-agent is
        // not_supported and not a live review provider, so the "run rr review
        // --provider <p>" guidance must be suppressed for it.
        let tmp = tempdir().expect("tempdir");
        let runtime = test_runtime(tmp.path().to_path_buf(), tmp.path().join("store"));
        let capability = runtime_provider_capability(&runtime, "pi-agent");
        assert_eq!(capability["status"], "not_supported");
        assert_eq!(capability["supports"]["review_start"], false);
        assert_eq!(capability["supports"]["doctor"], false);
    }

    #[test]
    fn search_rejects_command_irrelevant_session_and_pr_flags() {
        // Regression (rr-search-inert-session-pr-flags): rr search is
        // corpus-scoped and must reject --session/--pr as command-irrelevant
        // instead of silently accepting them inert.
        let session_err = parse_args(&[
            "search".to_owned(),
            "--query".to_owned(),
            "auth".to_owned(),
            "--session".to_owned(),
            "foo".to_owned(),
        ])
        .expect_err("rr search --session must fail closed");
        assert!(
            session_err.contains("not valid for rr search"),
            "rejection must name the command-irrelevant flag: {session_err}"
        );

        let pr_err = parse_args(&[
            "search".to_owned(),
            "--query".to_owned(),
            "auth".to_owned(),
            "--pr".to_owned(),
            "99999".to_owned(),
        ])
        .expect_err("rr search --pr must fail closed");
        assert!(
            pr_err.contains("not valid for rr search"),
            "rejection must name the command-irrelevant flag: {pr_err}"
        );

        // A plain rr search still parses; --session/--pr remain valid for the
        // commands that legitimately use them.
        parse_args(&["search".to_owned(), "--query".to_owned(), "auth".to_owned()])
            .expect("plain rr search must still parse");
        parse_args(&[
            "status".to_owned(),
            "--session".to_owned(),
            "foo".to_owned(),
        ])
        .expect("rr status --session must still parse");
        parse_args(&["resume".to_owned(), "--pr".to_owned(), "42".to_owned()])
            .expect("rr resume --pr must still parse");
    }

    fn run_git(repo: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .expect("run git command");
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn init_repo_with_remote(remote: &str) -> (tempfile::TempDir, PathBuf) {
        let tmp = tempdir().expect("tempdir");
        let repo = tmp.path().join("repo");
        fs::create_dir_all(&repo).expect("create repo");
        run_git(&repo, &["init"]);
        run_git(&repo, &["remote", "add", "origin", remote]);
        fs::write(repo.join("README.md"), "seed").expect("write seed file");
        run_git(&repo, &["add", "README.md"]);
        run_git(
            &repo,
            &[
                "-c",
                "user.name=Roger Test",
                "-c",
                "user.email=roger@example.com",
                "commit",
                "-m",
                "seed",
            ],
        );
        (tmp, repo)
    }

    #[test]
    fn prs_queue_state_derivation_matrix() {
        // (attention_state, draft_count, posted_action_count, expected)
        let cases: &[(&str, i64, i64, &str)] = &[
            // outbound evidence wins over attention state
            ("awaiting_user_input", 0, 1, "posted"),
            ("review_launched", 2, 3, "posted"),
            ("refresh_recommended", 1, 0, "drafted"),
            ("awaiting_return", 4, 0, "drafted"),
            // needs_attention bucket
            ("awaiting_user_input", 0, 0, "needs_attention"),
            ("refresh_recommended", 0, 0, "needs_attention"),
            ("review_failed", 0, 0, "needs_attention"),
            ("outbound_approval_required", 0, 0, "needs_attention"),
            // in_review bucket
            ("review_launched", 0, 0, "in_review"),
            ("review_resumed", 0, 0, "in_review"),
            ("awaiting_return", 0, 0, "in_review"),
            ("returned_to_roger", 0, 0, "in_review"),
            // ambiguous derivation surfaces the persisted state as-is
            ("some_future_state", 0, 0, "some_future_state"),
            ("", 0, 0, ""),
        ];

        for (attention_state, draft_count, posted_action_count, expected) in cases {
            assert_eq!(
                derive_prs_queue_state(attention_state, *draft_count, *posted_action_count),
                *expected,
                "attention_state={attention_state} draft_count={draft_count} posted_action_count={posted_action_count}"
            );
        }
    }

    #[test]
    fn prs_queue_next_command_routes_by_state() {
        assert_eq!(
            prs_queue_next_command("not_started", 42),
            "rr review --pr 42 --provider opencode"
        );
        for state in [
            "in_review",
            "needs_attention",
            "drafted",
            "posted",
            "some_future_state",
        ] {
            assert_eq!(prs_queue_next_command(state, 7), "rr resume --pr 7");
        }
    }

    #[test]
    fn prs_table_renders_aligned_header_and_rows() {
        let data = json!({
            "items": [
                {
                    "pr_number": 42,
                    "title": "Fix widget alignment",
                    "author": "dev",
                    "is_draft": false,
                    "updated_at": "2026-06-02T10:00:00Z",
                    "roger_state": "not_started",
                    "session_id": null,
                    "next_command": "rr review --pr 42 --provider opencode",
                },
            ],
        });
        let table = render_prs_table(&data);
        let lines: Vec<&str> = table.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("PR"), "header row: {}", lines[0]);
        assert!(lines[0].contains("STATE"));
        assert!(lines[1].contains("#42"));
        assert!(lines[1].contains("not_started"));
        assert!(lines[1].ends_with("rr review --pr 42 --provider opencode"));
    }

    #[test]
    fn repository_lookup_is_cached_per_repo_path() {
        if let Ok(mut cache) = git_lookup_cache().lock() {
            cache.clear();
        }

        let (_tmp, repo) = init_repo_with_remote("https://github.com/owner/repo.git");
        let first = infer_repository_from_git(&repo);
        assert_eq!(first.as_deref(), Some("owner/repo"));

        run_git(
            &repo,
            &[
                "remote",
                "set-url",
                "origin",
                "https://github.com/other/new.git",
            ],
        );

        let second = infer_repository_from_git(&repo);
        assert_eq!(second.as_deref(), Some("owner/repo"));
    }

    #[test]
    fn branch_lookup_is_cached_per_repo_path() {
        if let Ok(mut cache) = git_lookup_cache().lock() {
            cache.clear();
        }

        let (_tmp, repo) = init_repo_with_remote("https://github.com/owner/repo.git");
        let first = infer_git_branch(&repo).expect("first branch");

        run_git(&repo, &["checkout", "-b", "cache-branch"]);

        let second = infer_git_branch(&repo).expect("second branch");
        assert_eq!(second, first);
    }

    fn setup_bridge_workspace() -> (tempfile::TempDir, CliRuntime, PathBuf) {
        let tmp = tempdir().expect("tempdir");
        let root = tmp.path().join("workspace");
        let generated = root.join("apps/extension/src/generated/bridge.ts");
        let extension_src = root.join("apps/extension/src");
        let guided_browser_script = root.join("scripts/extension/launch_preloaded_browser.sh");
        let background = extension_src.join("background/main.js");
        let content = extension_src.join("content/main.js");
        let manifest_template = root.join("apps/extension/manifest.template.json");
        let static_root = root.join("apps/extension/static");
        let assets_root = root.join("apps/extension/assets");
        let bridge_src = root.join("packages/bridge/src/lib.rs");
        fs::create_dir_all(generated.parent().expect("generated parent")).expect("mkdir generated");
        fs::create_dir_all(background.parent().expect("background parent"))
            .expect("mkdir background");
        fs::create_dir_all(content.parent().expect("content parent")).expect("mkdir content");
        fs::create_dir_all(
            guided_browser_script
                .parent()
                .expect("guided browser script parent"),
        )
        .expect("mkdir guided browser script parent");
        fs::create_dir_all(&static_root).expect("mkdir static");
        fs::create_dir_all(&assets_root).expect("mkdir assets");
        fs::create_dir_all(bridge_src.parent().expect("bridge src parent"))
            .expect("mkdir bridge src");
        fs::write(&bridge_src, "// bridge marker\n").expect("write bridge marker");
        fs::write(&generated, bridge_contract_snapshot()).expect("write generated bridge contract");
        fs::write(&background, "export const background = true;\n").expect("write background");
        fs::write(&content, "export const content = true;\n").expect("write content");
        fs::write(assets_root.join("icon-16.png"), b"icon16").expect("write icon16");
        fs::write(assets_root.join("icon-32.png"), b"icon32").expect("write icon32");
        let manifest_template_json = json!({
            "manifest_version": 3,
            "name": "Roger Reviewer",
            "version": "0.1.0",
            "description": "Launch local Roger review flows from GitHub PR pages.",
            "key": TEST_EXTENSION_MANIFEST_KEY,
            "icons": {
                "16": "assets/icon-16.png",
                "32": "assets/icon-32.png",
            },
            "permissions": ["nativeMessaging"],
            "background": {
                "service_worker": "src/background/main.js",
                "type": "module",
            },
            "content_scripts": [
                {
                    "matches": ["https://github.com/*/*/pull/*"],
                    "js": ["src/content/main.js"],
                }
            ],
            "action": {
                "default_icon": {
                    "16": "assets/icon-16.png",
                    "32": "assets/icon-32.png",
                }
            }
        });
        fs::write(
            &manifest_template,
            serde_json::to_string_pretty(&manifest_template_json)
                .expect("serialize manifest template")
                + "\n",
        )
        .expect("write manifest template");
        fs::write(static_root.join(".gitkeep"), "").expect("write static marker");
        fs::write(
            &guided_browser_script,
            "#!/usr/bin/env bash\nset -euo pipefail\necho guided-browser\n",
        )
        .expect("write guided browser script");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let mut permissions = fs::metadata(&guided_browser_script)
                .expect("guided browser metadata")
                .permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&guided_browser_script, permissions)
                .expect("chmod guided browser script");
        }

        let runtime = CliRuntime {
            cwd: root.clone(),
            store_root: root.join(".roger"),
            opencode_bin: "opencode".to_owned(),
        };

        (tmp, runtime, generated)
    }

    fn parse_robot(stdout: &str) -> Value {
        serde_json::from_str(stdout).expect("robot payload")
    }

    #[test]
    fn continuity_inference_rank_prefers_usable_over_degraded_and_unusable() {
        assert_eq!(continuity_inference_rank("review:usable"), 2);
        assert_eq!(continuity_inference_rank("resume:degraded"), 1);
        assert_eq!(continuity_inference_rank("resume:reseeded"), 1);
        assert_eq!(continuity_inference_rank("resume:unusable"), 0);
        assert_eq!(continuity_inference_rank("resume:stale_locator"), 0);
    }

    #[test]
    fn select_unique_strongest_score_index_returns_none_for_tied_best_candidates() {
        let scores = vec![
            ReentryInferenceScore {
                pr_match_rank: 0,
                binding_quality_rank: 2,
                continuity_quality_rank: 2,
                updated_at: 200,
            },
            ReentryInferenceScore {
                pr_match_rank: 0,
                binding_quality_rank: 2,
                continuity_quality_rank: 2,
                updated_at: 200,
            },
        ];

        assert_eq!(select_unique_strongest_score_index(&scores), None);
    }

    #[test]
    fn select_unique_strongest_score_index_prefers_binding_then_continuity_then_freshness() {
        let scores = vec![
            ReentryInferenceScore {
                pr_match_rank: 0,
                binding_quality_rank: 1,
                continuity_quality_rank: 2,
                updated_at: 300,
            },
            ReentryInferenceScore {
                pr_match_rank: 0,
                binding_quality_rank: 2,
                continuity_quality_rank: 1,
                updated_at: 100,
            },
            ReentryInferenceScore {
                pr_match_rank: 0,
                binding_quality_rank: 2,
                continuity_quality_rank: 2,
                updated_at: 250,
            },
        ];

        assert_eq!(select_unique_strongest_score_index(&scores), Some(2));
    }

    fn review_target(repository: &str, pull_request: u64) -> ReviewTarget {
        ReviewTarget {
            repository: repository.to_owned(),
            pull_request_number: pull_request,
            base_ref: "main".to_owned(),
            head_ref: format!("feature-{pull_request}"),
            base_commit: "aaa".to_owned(),
            head_commit: "bbb".to_owned(),
        }
    }

    #[test]
    fn infer_strongest_reentry_selection_prefers_binding_and_continuity_quality() {
        let tmp = tempdir().expect("tempdir");
        let store = RogerStore::open(tmp.path()).expect("open store");

        let weaker_target = review_target("owner/repo", 40);
        store
            .create_review_session(CreateReviewSession {
                id: "session-weaker",
                review_target: &weaker_target,
                provider: "opencode",
                session_locator: None,
                resume_bundle_artifact_id: None,
                continuity_state: "resume:degraded",
                attention_state: "awaiting_user_input",
                launch_profile_id: None,
            })
            .expect("create weaker session");

        let stronger_target = review_target("owner/repo", 41);
        store
            .create_review_session(CreateReviewSession {
                id: "session-stronger",
                review_target: &stronger_target,
                provider: "opencode",
                session_locator: None,
                resume_bundle_artifact_id: None,
                continuity_state: "resume:usable",
                attention_state: "awaiting_user_input",
                launch_profile_id: None,
            })
            .expect("create stronger session");
        store
            .put_session_launch_binding(CreateSessionLaunchBinding {
                id: "binding-stronger",
                session_id: "session-stronger",
                repo_locator: &stronger_target.repository,
                review_target: Some(&stronger_target),
                surface: LaunchSurface::Cli,
                launch_profile_id: Some(cli_config::PROFILE_ID),
                ui_target: Some(cli_config::UI_TARGET),
                instance_preference: Some(cli_config::INSTANCE_PREFERENCE),
                cwd: Some("/tmp/repo"),
                worktree_root: None,
            })
            .expect("bind stronger session");

        let candidates = store
            .session_finder(SessionFinderQuery {
                repository: Some("owner/repo".to_owned()),
                pull_request_number: None,
                attention_states: Vec::new(),
                limit: 25,
            })
            .expect("session finder");

        let inferred = infer_strongest_reentry_selection(
            &store,
            &candidates,
            None,
            LaunchSurface::Cli,
            ResolveSessionLocalRoot::default(),
            Some(cli_config::UI_TARGET),
            Some(cli_config::INSTANCE_PREFERENCE),
        )
        .expect("infer strongest")
        .expect("expected strongest candidate");

        assert_eq!(inferred.0, "session-stronger");
        assert_eq!(inferred.1.expect("binding").id, "binding-stronger");
        assert_eq!(inferred.2.binding_quality_rank, 2);
        assert_eq!(inferred.2.continuity_quality_rank, 2);
    }

    #[test]
    fn infer_strongest_reentry_selection_returns_none_when_scores_are_tied() {
        let tmp = tempdir().expect("tempdir");
        let store = RogerStore::open(tmp.path()).expect("open store");

        for session_id in ["session-a", "session-b"] {
            let target = review_target("owner/repo", 42);
            store
                .create_review_session(CreateReviewSession {
                    id: session_id,
                    review_target: &target,
                    provider: "opencode",
                    session_locator: None,
                    resume_bundle_artifact_id: None,
                    continuity_state: "resume:usable",
                    attention_state: "awaiting_user_input",
                    launch_profile_id: None,
                })
                .expect("create tied session");
        }

        let candidates = vec![
            SessionFinderEntry {
                session_id: "session-a".to_owned(),
                repository: "owner/repo".to_owned(),
                pull_request_number: 42,
                attention_state: "awaiting_user_input".to_owned(),
                provider: "opencode".to_owned(),
                updated_at: 123,
            },
            SessionFinderEntry {
                session_id: "session-b".to_owned(),
                repository: "owner/repo".to_owned(),
                pull_request_number: 42,
                attention_state: "awaiting_user_input".to_owned(),
                provider: "opencode".to_owned(),
                updated_at: 123,
            },
        ];

        let inferred = infer_strongest_reentry_selection(
            &store,
            &candidates,
            None,
            LaunchSurface::Cli,
            ResolveSessionLocalRoot::default(),
            Some(cli_config::UI_TARGET),
            Some(cli_config::INSTANCE_PREFERENCE),
        )
        .expect("infer strongest");

        assert_eq!(inferred, None);
    }

    fn write_extension_identity_state(runtime: &CliRuntime, extension_id: &str) {
        persist_extension_id(runtime, extension_id).expect("persist extension identity");
    }

    // One shared lock for every test that mutates or depends on ambient
    // process env (HOME, RR_STORE_ROOT, RR_EXTENSION_PROFILE_ROOT). Separate
    // per-test locks do not serialize against each other, and discovery tests
    // that need the env to stay clean must hold the same lock as mutators.
    static SHARED_ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn shared_env_guard() -> std::sync::MutexGuard<'static, ()> {
        SHARED_ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    // Caller must hold shared_env_guard() for the duration of its test; this
    // helper mutates RR_STORE_ROOT and restores it before returning.
    fn register_extension_identity_via_bridge(
        runtime: &CliRuntime,
        browser: &str,
        extension_id: &str,
    ) {
        let previous_store_root = std::env::var_os("RR_STORE_ROOT");
        // SAFETY: tests serialize RR_STORE_ROOT mutation via ENV_LOCK and restore it before return.
        unsafe {
            std::env::set_var("RR_STORE_ROOT", &runtime.store_root);
        }

        let intent = roger_bridge::BridgeLaunchIntent {
            action: "register_extension_identity".to_owned(),
            owner: "roger".to_owned(),
            repo: "roger-reviewer".to_owned(),
            pr_number: 0,
            head_ref: None,
            instance: None,
            session_id: None,
            extension_id: Some(extension_id.to_owned()),
            browser: Some(browser.to_owned()),
        };
        let preflight = roger_bridge::BridgePreflight {
            roger_binary_found: false,
            roger_data_dir_exists: false,
            gh_available: false,
        };

        let response = roger_bridge::handle_bridge_intent(&intent, &preflight, Path::new("rr"));

        match previous_store_root {
            Some(value) => {
                // SAFETY: tests serialize RR_STORE_ROOT mutation via ENV_LOCK and restore it before return.
                unsafe {
                    std::env::set_var("RR_STORE_ROOT", value);
                }
            }
            None => {
                // SAFETY: tests serialize RR_STORE_ROOT mutation via ENV_LOCK and restore it before return.
                unsafe {
                    std::env::remove_var("RR_STORE_ROOT");
                }
            }
        }

        assert!(
            response.ok,
            "bridge registration intent failed: {} / {:?}",
            response.message, response.guidance
        );
    }

    fn write_extension_profile_discovery_state(
        runtime: &CliRuntime,
        browser: SupportedBrowser,
        extension_id: &str,
    ) {
        let profile_root = extension_guided_profile_root(runtime, &browser);
        let preferences_path = profile_root.join("Default/Secure Preferences");
        fs::create_dir_all(preferences_path.parent().expect("preferences parent"))
            .expect("create preferences parent");
        let package_dir = runtime
            .cwd
            .join("target/bridge/extension/roger-extension-unpacked");
        let preferences = json!({
            "extensions": {
                "settings": {
                    extension_id: {
                        "path": package_dir.to_string_lossy().to_string()
                    }
                }
            }
        });
        fs::write(
            preferences_path,
            serde_json::to_vec_pretty(&preferences).expect("serialize preferences"),
        )
        .expect("write secure preferences");
    }

    #[test]
    fn bridge_export_contracts_writes_generated_snapshot() {
        let (_tmp, runtime, generated) = setup_bridge_workspace();
        let result = run(
            &[
                "bridge".to_owned(),
                "export-contracts".to_owned(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(result.exit_code, 0, "{}", result.stderr);

        let payload = parse_robot(&result.stdout);
        assert_eq!(payload["outcome"], "complete");
        assert_eq!(payload["data"]["subcommand"], "export-contracts");

        let written = fs::read_to_string(&generated).expect("read generated contract");
        assert_eq!(written, bridge_contract_snapshot());
    }

    #[test]
    fn bridge_verify_contracts_reports_drift_with_repair_guidance() {
        let (_tmp, runtime, generated) = setup_bridge_workspace();
        fs::write(&generated, "// stale\n").expect("write stale contract");

        let result = run(
            &[
                "bridge".to_owned(),
                "verify-contracts".to_owned(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(result.exit_code, 4, "{}", result.stderr);

        let payload = parse_robot(&result.stdout);
        assert_eq!(payload["outcome"], "repair_needed");
        assert_eq!(payload["data"]["reason_code"], "bridge_contract_drift");
        assert!(
            payload["repair_actions"]
                .as_array()
                .expect("repair actions")
                .iter()
                .any(|action| action.as_str() == Some("rr bridge export-contracts"))
        );
    }

    #[test]
    fn bridge_verify_contracts_passes_after_export() {
        let (_tmp, runtime, _generated) = setup_bridge_workspace();
        let export = run(
            &["bridge".to_owned(), "export-contracts".to_owned()],
            &runtime,
        );
        assert_eq!(export.exit_code, 0, "{}", export.stderr);

        let verify = run(
            &[
                "bridge".to_owned(),
                "verify-contracts".to_owned(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(verify.exit_code, 0, "{}", verify.stderr);
        let payload = parse_robot(&verify.stdout);
        assert_eq!(payload["outcome"], "complete");
        assert_eq!(payload["data"]["matches_expected"], true);
    }

    #[test]
    fn bridge_pack_extension_emits_checksum_asset_manifest() {
        let (tmp, runtime, _generated) = setup_bridge_workspace();
        let output_dir = tmp.path().join("pack-output");
        let result = run(
            &[
                "bridge".to_owned(),
                "pack-extension".to_owned(),
                "--output-dir".to_owned(),
                output_dir.to_string_lossy().to_string(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(result.exit_code, 0, "{}", result.stderr);
        let payload = parse_robot(&result.stdout);
        assert_eq!(payload["outcome"], "complete");
        assert_eq!(payload["data"]["subcommand"], "pack-extension");
        assert_eq!(payload["data"]["installs_browser_extension"], false);

        let package_dir = PathBuf::from(
            payload["data"]["package_dir"]
                .as_str()
                .expect("package dir should be present"),
        );
        assert_eq!(
            package_dir.file_name().and_then(|value| value.to_str()),
            Some("roger-extension-unpacked")
        );
        let manifest: Value = serde_json::from_str(
            &fs::read_to_string(package_dir.join("manifest.json")).expect("read packaged manifest"),
        )
        .expect("parse packaged manifest");
        assert_eq!(manifest["version"], "0.1.0.0");
        assert_eq!(manifest["version_name"], "0.1.0-dev.0+nogit");
        assert_eq!(payload["data"]["version"], "0.1.0.0");
        assert_eq!(payload["data"]["version_name"], "0.1.0-dev.0+nogit");
        assert!(package_dir.exists());
        assert!(package_dir.join("manifest.json").exists());
        assert!(package_dir.join("src/background/main.js").exists());
        assert!(package_dir.join("assets/icon-16.png").exists());
        assert!(package_dir.join("assets/icon-32.png").exists());
        assert!(package_dir.join("SHA256SUMS").exists());
        assert!(package_dir.join("asset-manifest.json").exists());
        for icon_path in collect_manifest_icon_paths(&manifest) {
            assert!(
                package_dir.join(&icon_path).exists(),
                "missing packaged manifest icon path: {icon_path}"
            );
        }

        let asset_manifest: Value = serde_json::from_str(
            &fs::read_to_string(package_dir.join("asset-manifest.json"))
                .expect("read asset manifest"),
        )
        .expect("parse asset manifest");
        assert_eq!(asset_manifest["version"], "0.1.0.0");
        assert_eq!(asset_manifest["version_name"], "0.1.0-dev.0+nogit");
    }

    #[test]
    fn bridge_pack_extension_fails_when_manifest_icon_asset_is_missing() {
        let (_tmp, runtime, _generated) = setup_bridge_workspace();
        let missing_icon = runtime.cwd.join("apps/extension/assets/icon-16.png");
        fs::remove_file(&missing_icon).expect("remove icon-16 to reproduce load failure");

        let result = run(
            &[
                "bridge".to_owned(),
                "pack-extension".to_owned(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_ne!(
            result.exit_code, 0,
            "pack-extension should fail when manifest icon assets are missing"
        );
        let payload = parse_robot(&result.stdout);
        assert_eq!(payload["outcome"], "error");
        let reason = payload["data"]["reason"]
            .as_str()
            .expect("error payload should include a reason");
        assert!(
            reason.contains("assets/icon-16.png"),
            "reason should include missing icon path: {reason}"
        );
        assert!(
            reason.contains("Chrome/Edge"),
            "reason should preserve cross-browser repro context: {reason}"
        );
    }

    #[test]
    fn extension_build_version_uses_release_tag_for_stable() {
        let build = derive_extension_build_version_from_probe(
            "0.1.0",
            &ExtensionVersionProbe {
                exact_tag: Some("v2026.04.08".to_owned()),
                ..ExtensionVersionProbe::default()
            },
        );
        assert_eq!(build.manifest_version, "2026.4.8.1000");
        assert_eq!(build.version_name, "2026.04.08");
    }

    #[test]
    fn extension_build_version_uses_release_tag_for_rc() {
        let build = derive_extension_build_version_from_probe(
            "0.1.0",
            &ExtensionVersionProbe {
                exact_tag: Some("v2026.04.08-rc.3".to_owned()),
                ..ExtensionVersionProbe::default()
            },
        );
        assert_eq!(build.manifest_version, "2026.4.8.3");
        assert_eq!(build.version_name, "2026.04.08-rc.3");
    }

    #[test]
    fn extension_build_version_uses_local_dev_postfix_for_dirty_worktree() {
        let build = derive_extension_build_version_from_probe(
            "0.1.0",
            &ExtensionVersionProbe {
                rev_count: Some("42".to_owned()),
                short_sha: Some("abc123def456".to_owned()),
                dirty_fingerprint: Some("deadbeef".to_owned()),
                ..ExtensionVersionProbe::default()
            },
        );
        assert_eq!(build.manifest_version, "0.1.0.0");
        assert_eq!(
            build.version_name,
            "0.1.0-dev.42+abc123def456.dirty.deadbeef"
        );
    }

    #[test]
    fn bridge_install_blocks_when_extension_id_discovery_is_missing() {
        let (tmp, runtime, _generated) = setup_bridge_workspace();
        let install_root = tmp.path().join("install-root");

        let result = run(
            &[
                "bridge".to_owned(),
                "install".to_owned(),
                "--install-root".to_owned(),
                install_root.to_string_lossy().to_string(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(result.exit_code, 3, "{}", result.stderr);
        let payload = parse_robot(&result.stdout);
        assert_eq!(payload["outcome"], "blocked");
        assert_eq!(
            payload["data"]["reason_code"],
            "extension_id_discovery_failed"
        );
    }

    #[test]
    fn bridge_install_uses_discovered_identity_without_manual_flag() {
        let (tmp, runtime, _generated) = setup_bridge_workspace();
        let install_root = tmp.path().join("install-root");
        write_extension_identity_state(&runtime, "abcdefghijklmnopabcdefghijklmnop");

        let install = run(
            &[
                "bridge".to_owned(),
                "install".to_owned(),
                "--install-root".to_owned(),
                install_root.to_string_lossy().to_string(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(install.exit_code, 0, "{}", install.stderr);
        let payload = parse_robot(&install.stdout);
        assert_eq!(payload["outcome"], "complete");
        assert_eq!(payload["data"]["extension_id_source"], "store_registry");
        assert_eq!(
            payload["data"]["bridge_binary_source"],
            "installed_rr_current_exe"
        );
        let host_binary = payload["data"]["bridge_host_binary"]
            .as_str()
            .expect("bridge host binary path should exist");
        assert!(
            Path::new(host_binary).exists(),
            "expected installed rr host binary to exist at {}",
            host_binary
        );
    }

    #[test]
    fn bridge_install_and_uninstall_manage_assets_with_checksums() {
        let (tmp, runtime, _generated) = setup_bridge_workspace();
        let install_root = tmp.path().join("install-root");

        let install = run(
            &[
                "bridge".to_owned(),
                "install".to_owned(),
                "--extension-id".to_owned(),
                "abcdefghijklmnopabcdefghijklmnop".to_owned(),
                "--install-root".to_owned(),
                install_root.to_string_lossy().to_string(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(install.exit_code, 0, "{}", install.stderr);
        let install_payload = parse_robot(&install.stdout);
        assert_eq!(install_payload["outcome"], "complete");
        assert_eq!(install_payload["data"]["subcommand"], "install");
        assert_eq!(install_payload["data"]["installs_browser_extension"], false);
        let assets = install_payload["data"]["assets"]
            .as_array()
            .expect("install assets should be an array");
        assert!(assets.len() >= 3);
        assert!(assets.iter().all(|asset| {
            asset["sha256"]
                .as_str()
                .is_some_and(|checksum| checksum.len() == 64)
        }));

        let os = SupportedOs::current().expect("supported host os");
        for browser in [
            SupportedBrowser::Chrome,
            SupportedBrowser::Edge,
            SupportedBrowser::Brave,
        ] {
            let manifest_path = native_host_install_path_for(&browser, os, &install_root);
            assert!(
                manifest_path.exists(),
                "missing {}",
                manifest_path.display()
            );
        }
        let uninstall = run(
            &[
                "extension".to_owned(),
                "uninstall".to_owned(),
                "--install-root".to_owned(),
                install_root.to_string_lossy().to_string(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(uninstall.exit_code, 0, "{}", uninstall.stderr);
        let uninstall_payload = parse_robot(&uninstall.stdout);
        assert_eq!(uninstall_payload["outcome"], "complete");
        assert_eq!(uninstall_payload["data"]["subcommand"], "uninstall");
        assert_eq!(uninstall_payload["data"]["surface"], "extension");
        let removed = uninstall_payload["data"]["removed"]
            .as_array()
            .expect("removed list");
        assert!(removed.len() >= 3);

        let second_uninstall = run(
            &[
                "extension".to_owned(),
                "uninstall".to_owned(),
                "--install-root".to_owned(),
                install_root.to_string_lossy().to_string(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(second_uninstall.exit_code, 0, "{}", second_uninstall.stderr);
        let second_uninstall_payload = parse_robot(&second_uninstall.stdout);
        assert_eq!(second_uninstall_payload["outcome"], "complete");
        assert_eq!(
            second_uninstall_payload["data"]["removed"]
                .as_array()
                .expect("removed list on second uninstall")
                .len(),
            0
        );
        assert!(
            second_uninstall_payload["data"]["missing"]
                .as_array()
                .expect("missing list on second uninstall")
                .len()
                >= 3
        );

        for browser in [
            SupportedBrowser::Chrome,
            SupportedBrowser::Edge,
            SupportedBrowser::Brave,
        ] {
            let manifest_path = native_host_install_path_for(&browser, os, &install_root);
            assert!(
                !manifest_path.exists(),
                "still present {}",
                manifest_path.display()
            );
        }

        let reinstall = run(
            &[
                "bridge".to_owned(),
                "install".to_owned(),
                "--extension-id".to_owned(),
                "abcdefghijklmnopabcdefghijklmnop".to_owned(),
                "--install-root".to_owned(),
                install_root.to_string_lossy().to_string(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(reinstall.exit_code, 0, "{}", reinstall.stderr);
        let reinstall_payload = parse_robot(&reinstall.stdout);
        assert_eq!(reinstall_payload["outcome"], "complete");
        assert_eq!(reinstall_payload["data"]["subcommand"], "install");
        assert!(
            reinstall_payload["data"]["assets"]
                .as_array()
                .expect("reinstall assets")
                .len()
                >= 3
        );
    }

    #[test]
    fn bridge_uninstall_is_demoted_to_repair_alias() {
        let (tmp, runtime, _generated) = setup_bridge_workspace();
        let install_root = tmp.path().join("install-root");

        let install = run(
            &[
                "bridge".to_owned(),
                "install".to_owned(),
                "--extension-id".to_owned(),
                "abcdefghijklmnopabcdefghijklmnop".to_owned(),
                "--install-root".to_owned(),
                install_root.to_string_lossy().to_string(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(install.exit_code, 0, "{}", install.stderr);

        let uninstall = run(
            &[
                "bridge".to_owned(),
                "uninstall".to_owned(),
                "--install-root".to_owned(),
                install_root.to_string_lossy().to_string(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(uninstall.exit_code, 0, "{}", uninstall.stderr);
        let uninstall_payload = parse_robot(&uninstall.stdout);
        assert_eq!(uninstall_payload["outcome"], "complete");
        assert_eq!(uninstall_payload["data"]["surface"], "bridge");
        assert_eq!(
            uninstall_payload["data"]["preferred_surface"],
            "rr extension uninstall"
        );
        let warnings = uninstall_payload["warnings"]
            .as_array()
            .expect("bridge uninstall warnings");
        assert!(warnings.iter().any(|warning| {
            warning
                .as_str()
                .is_some_and(|text| text.contains("prefer rr extension uninstall"))
        }));
    }

    #[test]
    fn extension_setup_uses_packaged_manifest_key_before_browser_registration() {
        let _env_guard = shared_env_guard();
        let (tmp, runtime, _generated) = setup_bridge_workspace();
        let install_root = tmp.path().join("install-root");
        let result = run(
            &[
                "extension".to_owned(),
                "setup".to_owned(),
                "--browser".to_owned(),
                "edge".to_owned(),
                "--install-root".to_owned(),
                install_root.to_string_lossy().to_string(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(result.exit_code, 0, "{}", result.stderr);
        let payload = parse_robot(&result.stdout);
        assert_eq!(payload["outcome"], "complete");
        assert_eq!(payload["data"]["subcommand"], "setup");
        assert_eq!(payload["data"]["extension_id"], TEST_EXTENSION_MANIFEST_ID);
        assert_eq!(
            payload["data"]["extension_id_source"],
            "packaged_manifest_key"
        );
        assert_eq!(payload["data"]["browser"], "edge");
        assert!(
            payload["data"]["manual_browser_step"]
                .as_str()
                .unwrap_or_default()
                .contains("open edge://extensions")
        );
        assert!(
            payload["data"]["guided_browser_command"]
                .as_str()
                .unwrap_or_default()
                .contains("scripts/extension/launch_preloaded_browser.sh")
        );
        assert!(
            payload["data"]["guided_browser_command"]
                .as_str()
                .unwrap_or_default()
                .contains("--browser 'edge'")
        );
        let warnings = payload["warnings"]
            .as_array()
            .expect("warnings should be an array");
        assert!(warnings.iter().any(|warning| {
            warning
                .as_str()
                .unwrap_or_default()
                .contains("deterministic extension id")
        }));
        let repair_actions = payload["repair_actions"]
            .as_array()
            .expect("repair actions should be an array");
        assert!(
            repair_actions
                .first()
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .contains("launch_preloaded_browser.sh")
        );
        assert!(
            repair_actions
                .get(1)
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .contains("open edge://extensions")
        );
    }

    #[test]
    fn extension_setup_and_doctor_succeed_with_discovered_identity() {
        let _env_guard = shared_env_guard();
        let (tmp, runtime, _generated) = setup_bridge_workspace();
        let install_root = tmp.path().join("install-root");
        write_extension_profile_discovery_state(
            &runtime,
            SupportedBrowser::Chrome,
            "abcdefghijklmnopabcdefghijklmnop",
        );

        let setup = run(
            &[
                "extension".to_owned(),
                "setup".to_owned(),
                "--browser".to_owned(),
                "chrome".to_owned(),
                "--install-root".to_owned(),
                install_root.to_string_lossy().to_string(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(setup.exit_code, 0, "{}", setup.stderr);
        let setup_payload = parse_robot(&setup.stdout);
        assert_eq!(setup_payload["outcome"], "complete");
        assert_eq!(setup_payload["data"]["subcommand"], "setup");
        assert_eq!(setup_payload["data"]["browser"], "chrome");
        assert_eq!(
            setup_payload["data"]["extension_id_source"],
            "browser_profile_preferences"
        );
        assert_eq!(setup_payload["data"]["doctor"]["subcommand"], "doctor");

        let os = SupportedOs::current().expect("supported host os");
        let chrome_manifest_path =
            native_host_install_path_for(&SupportedBrowser::Chrome, os, &install_root);
        assert!(
            chrome_manifest_path.exists(),
            "{}",
            chrome_manifest_path.display()
        );
        let doctor = run(
            &[
                "extension".to_owned(),
                "doctor".to_owned(),
                "--browser".to_owned(),
                "chrome".to_owned(),
                "--install-root".to_owned(),
                install_root.to_string_lossy().to_string(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(doctor.exit_code, 0, "{}", doctor.stderr);
        let doctor_payload = parse_robot(&doctor.stdout);
        assert_eq!(doctor_payload["outcome"], "complete");
        assert_eq!(doctor_payload["data"]["subcommand"], "doctor");
        assert!(
            doctor_payload["data"]["checks"]
                .as_array()
                .expect("doctor checks")
                .iter()
                .all(|entry| entry["ok"].as_bool().unwrap_or(false))
        );
    }

    #[test]
    fn extension_setup_and_doctor_succeed_after_bridge_registration_event() {
        let _env_guard = shared_env_guard();
        let (tmp, runtime, _generated) = setup_bridge_workspace();
        let install_root = tmp.path().join("install-root");
        let extension_id = "abcdefghijklmnopabcdefghijklmnop";

        register_extension_identity_via_bridge(&runtime, "edge", extension_id);

        let setup = run(
            &[
                "extension".to_owned(),
                "setup".to_owned(),
                "--browser".to_owned(),
                "edge".to_owned(),
                "--install-root".to_owned(),
                install_root.to_string_lossy().to_string(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(setup.exit_code, 0, "{}", setup.stderr);
        let setup_payload = parse_robot(&setup.stdout);
        assert_eq!(setup_payload["outcome"], "complete");
        assert_eq!(
            setup_payload["data"]["extension_id"],
            TEST_EXTENSION_MANIFEST_ID
        );
        assert_eq!(
            setup_payload["data"]["extension_id_source"],
            "packaged_manifest_key"
        );

        let doctor = run(
            &[
                "extension".to_owned(),
                "doctor".to_owned(),
                "--browser".to_owned(),
                "edge".to_owned(),
                "--install-root".to_owned(),
                install_root.to_string_lossy().to_string(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(doctor.exit_code, 0, "{}", doctor.stderr);
        let doctor_payload = parse_robot(&doctor.stdout);
        assert_eq!(doctor_payload["outcome"], "complete");
        assert_eq!(
            doctor_payload["data"]["extension_id"],
            TEST_EXTENSION_MANIFEST_ID
        );
        assert_eq!(
            doctor_payload["data"]["extension_id_source"],
            "packaged_manifest_key"
        );
        assert!(
            doctor_payload["data"]["checks"]
                .as_array()
                .expect("doctor checks")
                .iter()
                .all(|entry| entry["ok"].as_bool().unwrap_or(false))
        );
    }

    #[test]
    fn extension_setup_prefers_browser_profile_identity_over_stale_store_registry() {
        let _env_guard = shared_env_guard();
        let (tmp, runtime, _generated) = setup_bridge_workspace();
        let install_root = tmp.path().join("install-root");
        let stale_store_id = "abcdefghijklmnopabcdefghijklmnop";
        let profile_id = "bcdefghijklmnopabcdefghijklmnopa";
        write_extension_identity_state(&runtime, stale_store_id);
        write_extension_profile_discovery_state(&runtime, SupportedBrowser::Edge, profile_id);

        let setup = run(
            &[
                "extension".to_owned(),
                "setup".to_owned(),
                "--browser".to_owned(),
                "edge".to_owned(),
                "--install-root".to_owned(),
                install_root.to_string_lossy().to_string(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(setup.exit_code, 0, "{}", setup.stderr);
        let setup_payload = parse_robot(&setup.stdout);
        assert_eq!(setup_payload["outcome"], "complete");
        assert_eq!(setup_payload["data"]["extension_id"], profile_id);
        assert_eq!(
            setup_payload["data"]["extension_id_source"],
            "browser_profile_preferences"
        );

        let manifest_path = PathBuf::from(
            setup_payload["data"]["native_manifest_path"]
                .as_str()
                .expect("setup manifest path"),
        );
        let manifest: Value =
            serde_json::from_str(&fs::read_to_string(&manifest_path).expect("read setup manifest"))
                .expect("parse setup manifest");
        assert_eq!(
            manifest["allowed_origins"][0],
            format!("chrome-extension://{profile_id}/")
        );

        let persisted = fs::read_to_string(extension_id_registry_path(&runtime.store_root))
            .expect("persisted extension identity should exist");
        assert_eq!(persisted.trim(), profile_id);
    }

    #[test]
    fn extension_setup_with_explicit_profile_root_ignores_default_profile_stale_identity() {
        let _env_guard = shared_env_guard();

        let (tmp, runtime, _generated) = setup_bridge_workspace();
        let install_root = tmp.path().join("install-root");
        let explicit_profile_root = tmp.path().join("isolated-edge-profile");
        let fake_home = tmp.path().join("fake-home");

        let stale_default_id = "bcdefghijklmnopabcdefghijklmnopa";
        let default_pref =
            fake_home.join("Library/Application Support/Microsoft Edge/Default/Secure Preferences");
        fs::create_dir_all(default_pref.parent().expect("default pref parent"))
            .expect("create default pref parent");
        let package_dir = runtime
            .cwd
            .join("target/bridge/extension/roger-extension-unpacked");
        let default_preferences = json!({
            "extensions": {
                "settings": {
                    stale_default_id: {
                        "path": package_dir.to_string_lossy().to_string()
                    }
                }
            }
        });
        fs::write(
            &default_pref,
            serde_json::to_vec_pretty(&default_preferences).expect("serialize default prefs"),
        )
        .expect("write default preferences");

        let previous_home = std::env::var_os("HOME");
        let previous_profile_root = std::env::var_os("RR_EXTENSION_PROFILE_ROOT");
        // SAFETY: tests serialize HOME/RR_EXTENSION_PROFILE_ROOT mutation via ENV_LOCK and restore before return.
        unsafe {
            std::env::set_var("HOME", &fake_home);
            std::env::set_var("RR_EXTENSION_PROFILE_ROOT", &explicit_profile_root);
        }

        let setup = run(
            &[
                "extension".to_owned(),
                "setup".to_owned(),
                "--browser".to_owned(),
                "edge".to_owned(),
                "--install-root".to_owned(),
                install_root.to_string_lossy().to_string(),
                "--robot".to_owned(),
            ],
            &runtime,
        );

        match previous_profile_root {
            Some(value) => {
                // SAFETY: tests serialize RR_EXTENSION_PROFILE_ROOT mutation via ENV_LOCK and restore before return.
                unsafe {
                    std::env::set_var("RR_EXTENSION_PROFILE_ROOT", value);
                }
            }
            None => {
                // SAFETY: tests serialize RR_EXTENSION_PROFILE_ROOT mutation via ENV_LOCK and restore before return.
                unsafe {
                    std::env::remove_var("RR_EXTENSION_PROFILE_ROOT");
                }
            }
        }
        match previous_home {
            Some(value) => {
                // SAFETY: tests serialize HOME mutation via ENV_LOCK and restore before return.
                unsafe {
                    std::env::set_var("HOME", value);
                }
            }
            None => {
                // SAFETY: tests serialize HOME mutation via ENV_LOCK and restore before return.
                unsafe {
                    std::env::remove_var("HOME");
                }
            }
        }

        assert_eq!(setup.exit_code, 0, "{}", setup.stderr);
        let setup_payload = parse_robot(&setup.stdout);
        assert_eq!(setup_payload["outcome"], "complete");
        assert_eq!(
            setup_payload["data"]["extension_id"],
            TEST_EXTENSION_MANIFEST_ID
        );
        assert_eq!(
            setup_payload["data"]["extension_id_source"],
            "packaged_manifest_key"
        );
    }

    #[test]
    fn extension_setup_and_doctor_ignore_store_registry_without_browser_registration() {
        let _env_guard = shared_env_guard();
        let (tmp, runtime, _generated) = setup_bridge_workspace();
        let install_root = tmp.path().join("install-root");
        write_extension_identity_state(&runtime, "abcdefghijklmnopabcdefghijklmnop");

        let setup = run(
            &[
                "extension".to_owned(),
                "setup".to_owned(),
                "--browser".to_owned(),
                "chrome".to_owned(),
                "--install-root".to_owned(),
                install_root.to_string_lossy().to_string(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(setup.exit_code, 0, "{}", setup.stderr);
        let setup_payload = parse_robot(&setup.stdout);
        assert_eq!(setup_payload["outcome"], "complete");
        assert_eq!(
            setup_payload["data"]["extension_id"],
            TEST_EXTENSION_MANIFEST_ID
        );
        assert_eq!(
            setup_payload["data"]["extension_id_source"],
            "packaged_manifest_key"
        );
        assert_eq!(
            setup_payload["data"]["doctor"]["extension_id_source"],
            "packaged_manifest_key"
        );

        let doctor = run(
            &[
                "extension".to_owned(),
                "doctor".to_owned(),
                "--browser".to_owned(),
                "chrome".to_owned(),
                "--install-root".to_owned(),
                install_root.to_string_lossy().to_string(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(doctor.exit_code, 0, "{}", doctor.stderr);
        let doctor_payload = parse_robot(&doctor.stdout);
        assert_eq!(doctor_payload["outcome"], "complete");
        assert_eq!(
            doctor_payload["data"]["extension_id"],
            TEST_EXTENSION_MANIFEST_ID
        );
        assert_eq!(
            doctor_payload["data"]["extension_id_source"],
            "packaged_manifest_key"
        );
    }

    #[test]
    fn extension_doctor_prefers_browser_profile_identity_over_stale_store_registry() {
        let _env_guard = shared_env_guard();
        let (tmp, runtime, _generated) = setup_bridge_workspace();
        let install_root = tmp.path().join("install-root");
        let stale_store_id = "abcdefghijklmnopabcdefghijklmnop";
        let profile_id = "bcdefghijklmnopabcdefghijklmnopa";
        write_extension_identity_state(&runtime, stale_store_id);
        write_extension_profile_discovery_state(&runtime, SupportedBrowser::Chrome, profile_id);

        let pack = run(
            &[
                "bridge".to_owned(),
                "pack-extension".to_owned(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(pack.exit_code, 0, "{}", pack.stderr);

        let install = run(
            &[
                "bridge".to_owned(),
                "install".to_owned(),
                "--extension-id".to_owned(),
                profile_id.to_owned(),
                "--install-root".to_owned(),
                install_root.to_string_lossy().to_string(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(install.exit_code, 0, "{}", install.stderr);

        let doctor = run(
            &[
                "extension".to_owned(),
                "doctor".to_owned(),
                "--browser".to_owned(),
                "chrome".to_owned(),
                "--install-root".to_owned(),
                install_root.to_string_lossy().to_string(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(doctor.exit_code, 0, "{}", doctor.stderr);
        let doctor_payload = parse_robot(&doctor.stdout);
        assert_eq!(doctor_payload["outcome"], "complete");
        assert_eq!(doctor_payload["data"]["extension_id"], profile_id);
        assert_eq!(
            doctor_payload["data"]["extension_id_source"],
            "browser_profile_preferences"
        );
        assert!(
            doctor_payload["data"]["checks"]
                .as_array()
                .expect("doctor checks")
                .iter()
                .all(|entry| entry["ok"].as_bool().unwrap_or(false))
        );
    }

    #[test]
    fn extension_doctor_distinguishes_registration_missing_from_manifest_missing() {
        let _env_guard = shared_env_guard();
        let (tmp, runtime, _generated) = setup_bridge_workspace();
        let install_root = tmp.path().join("install-root");

        let blocked_missing_registration = run(
            &[
                "extension".to_owned(),
                "doctor".to_owned(),
                "--browser".to_owned(),
                "chrome".to_owned(),
                "--install-root".to_owned(),
                install_root.to_string_lossy().to_string(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(
            blocked_missing_registration.exit_code, 3,
            "{}",
            blocked_missing_registration.stderr
        );
        let blocked_registration_payload = parse_robot(&blocked_missing_registration.stdout);
        assert_eq!(blocked_registration_payload["outcome"], "blocked");
        assert_eq!(
            blocked_registration_payload["data"]["reason_code"],
            "extension_registration_missing"
        );
        assert!(
            blocked_registration_payload["data"]["guided_browser_command"]
                .as_str()
                .unwrap_or_default()
                .contains("launch_preloaded_browser.sh")
        );

        let pack = run(
            &[
                "bridge".to_owned(),
                "pack-extension".to_owned(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(pack.exit_code, 0, "{}", pack.stderr);
        write_extension_identity_state(&runtime, "abcdefghijklmnopabcdefghijklmnop");

        let blocked_missing_manifest = run(
            &[
                "extension".to_owned(),
                "doctor".to_owned(),
                "--browser".to_owned(),
                "chrome".to_owned(),
                "--install-root".to_owned(),
                install_root.to_string_lossy().to_string(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(
            blocked_missing_manifest.exit_code, 3,
            "{}",
            blocked_missing_manifest.stderr
        );
        let blocked_manifest_payload = parse_robot(&blocked_missing_manifest.stdout);
        assert_eq!(blocked_manifest_payload["outcome"], "blocked");
        assert_eq!(
            blocked_manifest_payload["data"]["reason_code"],
            "native_host_manifest_missing"
        );
    }

    #[test]
    fn extension_setup_discovers_identity_from_guided_profile_preferences() {
        let _env_guard = shared_env_guard();
        let (tmp, runtime, _generated) = setup_bridge_workspace();
        let install_root = tmp.path().join("install-root");
        let extension_id = "abcdefghijklmnopabcdefghijklmnop";
        write_extension_profile_discovery_state(&runtime, SupportedBrowser::Chrome, extension_id);

        let setup = run(
            &[
                "extension".to_owned(),
                "setup".to_owned(),
                "--browser".to_owned(),
                "chrome".to_owned(),
                "--install-root".to_owned(),
                install_root.to_string_lossy().to_string(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(setup.exit_code, 0, "{}", setup.stderr);
        let setup_payload = parse_robot(&setup.stdout);
        assert_eq!(setup_payload["outcome"], "complete");
        assert_eq!(
            setup_payload["data"]["extension_id_source"],
            "browser_profile_preferences"
        );
        assert_eq!(setup_payload["data"]["extension_id"], extension_id);
        let persisted = fs::read_to_string(extension_id_registry_path(&runtime.store_root))
            .expect("persisted extension identity should exist");
        assert_eq!(persisted.trim(), extension_id);
    }

    #[test]
    fn update_rejects_non_update_flags() {
        let runtime = CliRuntime {
            cwd: PathBuf::from("."),
            store_root: PathBuf::from(".roger-test"),
            opencode_bin: "opencode".to_owned(),
        };
        let result = run(
            &["update".to_owned(), "--pr".to_owned(), "12".to_owned()],
            &runtime,
        );
        assert_eq!(result.exit_code, 2);
        assert!(
            result.stderr.contains("rr update only supports"),
            "{}",
            result.stderr
        );
    }

    #[test]
    fn update_usage_text_lists_yes_confirmation_flags() {
        assert!(
            usage_text().contains("rr setup update")
                && usage_text().contains("[--yes|-y] [--dry-run] [--robot]"),
            "{}",
            usage_text()
        );
    }

    #[test]
    fn draft_and_approve_usage_text_and_flag_contract_are_explicit() {
        assert!(
            usage_text().contains("rr send draft")
                && usage_text().contains("(--finding <id>... | --all-findings)")
                && usage_text().contains("rr send approve")
                && usage_text().contains("rr send post")
                && usage_text().contains("--batch <draft-batch-id>"),
            "{}",
            usage_text()
        );

        let runtime = CliRuntime {
            cwd: PathBuf::from("."),
            store_root: PathBuf::from(".roger-test"),
            opencode_bin: "opencode".to_owned(),
        };
        let result = run(
            &[
                "draft".to_owned(),
                "--dry-run".to_owned(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(result.exit_code, 2);
        assert!(
            result
                .stderr
                .contains("rr draft does not support --dry-run"),
            "{}",
            result.stderr
        );

        let approve_result = run(
            &[
                "approve".to_owned(),
                "--dry-run".to_owned(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(approve_result.exit_code, 2);
        assert!(
            approve_result
                .stderr
                .contains("rr approve does not support --dry-run"),
            "{}",
            approve_result.stderr
        );

        let post_result = run(
            &[
                "post".to_owned(),
                "--dry-run".to_owned(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(post_result.exit_code, 2);
        assert!(
            post_result
                .stderr
                .contains("rr post does not support --dry-run"),
            "{}",
            post_result.stderr
        );
    }
    #[test]
    fn usage_text_summarizes_live_provider_tiers_truthfully() {
        assert!(
            usage_text().contains("opencode is the first-class tier-b continuity path"),
            "{}",
            usage_text()
        );
        assert!(
            usage_text().contains("codex, gemini, and claude are bounded tier-a providers"),
            "{}",
            usage_text()
        );
        assert!(
            usage_text().contains(
                "copilot is feature-gated bounded tier-b support; enable with RR_ENABLE_COPILOT_PROVIDER=1"
            ),
            "{}",
            usage_text()
        );
        assert!(
            usage_text().contains("pi-agent is not part of the current live CLI surface"),
            "{}",
            usage_text()
        );
    }

    #[test]
    fn update_fails_closed_for_local_build_without_release_metadata() {
        let runtime = CliRuntime {
            cwd: PathBuf::from("."),
            store_root: PathBuf::from(".roger-test"),
            opencode_bin: "opencode".to_owned(),
        };
        let result = run(&["update".to_owned(), "--robot".to_owned()], &runtime);
        assert_eq!(result.exit_code, 3, "{}", result.stderr);
        let payload = parse_robot(&result.stdout);
        assert_eq!(payload["outcome"], "blocked");
        assert_eq!(payload["data"]["reason_code"], "local_or_unpublished_build");
        assert_eq!(payload["data"]["migration"]["policy"], "binary_only");
        assert_eq!(
            payload["data"]["migration"]["schema_migrations_supported"],
            false
        );
        assert_eq!(payload["data"]["migration"]["status"], "deferred_for_now");
    }

    #[test]
    fn yes_flag_is_update_only() {
        let runtime = CliRuntime {
            cwd: PathBuf::from("."),
            store_root: PathBuf::from(".roger-test"),
            opencode_bin: "opencode".to_owned(),
        };
        let result = run(&["status".to_owned(), "--yes".to_owned()], &runtime);
        assert_eq!(result.exit_code, 2);
        assert!(
            result
                .stderr
                .contains("--channel/--version/--api-root/--download-root/--target/--yes are update-only flags"),
            "{}",
            result.stderr
        );
    }

    #[test]
    fn update_accepts_yes_and_short_yes_flags() {
        let runtime = CliRuntime {
            cwd: PathBuf::from("."),
            store_root: PathBuf::from(".roger-test"),
            opencode_bin: "opencode".to_owned(),
        };
        let long_flag = run(
            &[
                "update".to_owned(),
                "--yes".to_owned(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(long_flag.exit_code, 3, "{}", long_flag.stderr);
        let long_payload = parse_robot(&long_flag.stdout);
        assert_eq!(
            long_payload["data"]["reason_code"],
            "local_or_unpublished_build"
        );

        let short_flag = run(
            &["update".to_owned(), "-y".to_owned(), "--robot".to_owned()],
            &runtime,
        );
        assert_eq!(short_flag.exit_code, 3, "{}", short_flag.stderr);
        let short_payload = parse_robot(&short_flag.stdout);
        assert_eq!(
            short_payload["data"]["reason_code"],
            "local_or_unpublished_build"
        );
    }

    #[test]
    fn update_confirmation_requirement_matrix_is_truthful() {
        let parsed_plain = parse_args(&["update".to_owned()]).expect("parse update");
        assert_eq!(
            evaluate_update_confirmation_requirement(&parsed_plain, true),
            UpdateConfirmationRequirement::NeedsPrompt
        );
        assert_eq!(
            evaluate_update_confirmation_requirement(&parsed_plain, false),
            UpdateConfirmationRequirement::BlockedNonInteractive
        );

        let parsed_yes =
            parse_args(&["update".to_owned(), "--yes".to_owned()]).expect("parse update --yes");
        assert_eq!(
            evaluate_update_confirmation_requirement(&parsed_yes, false),
            UpdateConfirmationRequirement::BypassedByYes
        );

        let parsed_robot =
            parse_args(&["update".to_owned(), "--robot".to_owned()]).expect("parse update --robot");
        assert_eq!(
            evaluate_update_confirmation_requirement(&parsed_robot, true),
            UpdateConfirmationRequirement::BlockedRobotMode
        );

        let parsed_dry_run = parse_args(&["update".to_owned(), "--dry-run".to_owned()])
            .expect("parse update --dry-run");
        assert_eq!(
            evaluate_update_confirmation_requirement(&parsed_dry_run, false),
            UpdateConfirmationRequirement::NotRequired("dry_run")
        );
    }

    #[test]
    fn parse_args_canonicalizes_copilot_provider_aliases() {
        let parsed_alias = parse_args(&[
            "review".to_owned(),
            "--pr".to_owned(),
            "42".to_owned(),
            "--provider".to_owned(),
            "GitHub-Copilot".to_owned(),
        ])
        .expect("parse review alias");
        assert_eq!(parsed_alias.provider, "copilot");

        let parsed_mixed_case = parse_args(&[
            "review".to_owned(),
            "--pr".to_owned(),
            "42".to_owned(),
            "--provider".to_owned(),
            "CoPiLoT".to_owned(),
        ])
        .expect("parse review mixed case");
        assert_eq!(parsed_mixed_case.provider, "copilot");
    }

    #[test]
    fn confirmation_parser_accepts_yes_and_rejects_cancel_variants() {
        assert!(confirmation_response_is_affirmative("y"));
        assert!(confirmation_response_is_affirmative("Y"));
        assert!(confirmation_response_is_affirmative(" yes "));
        assert!(!confirmation_response_is_affirmative(""));
        assert!(!confirmation_response_is_affirmative("n"));
        assert!(!confirmation_response_is_affirmative("no"));
        assert!(!confirmation_response_is_affirmative("anything else"));
    }

    #[test]
    fn migration_policy_is_explicitly_deferred_for_now() {
        let policy = migration_policy_payload();
        assert_eq!(policy["policy"], "binary_only");
        assert_eq!(policy["schema_migrations_supported"], false);
        assert_eq!(policy["status"], "deferred_for_now");
        assert!(
            policy["guidance"]
                .as_str()
                .unwrap_or_default()
                .contains("fail closed")
        );
    }

    fn sample_store_compatibility(policy: &str) -> StoreCompatibilityEnvelope {
        StoreCompatibilityEnvelope {
            envelope_version: 1,
            store_schema_version: 10,
            min_supported_store_schema: 0,
            auto_migrate_from: 8,
            migration_policy: policy.to_owned(),
            migration_class_max_auto: "class_b".to_owned(),
            sidecar_generation: "v1".to_owned(),
            backup_required: true,
        }
    }

    #[test]
    fn migration_preflight_reports_no_migration_when_schema_matches_target() {
        let envelope = sample_store_compatibility("binary_only");
        let preflight = assess_migration_preflight(10, &envelope, true);
        assert_eq!(preflight.status, "no_migration_needed");
        assert_eq!(preflight.classification, "none");
        assert!(preflight.apply_allowed);
        assert!(preflight.blocked_reason.is_none());
    }

    #[test]
    fn migration_preflight_reports_auto_safe_posture_when_policy_allows_window() {
        // store_schema=10, current=9: a single-version (class_a) jump. Even
        // though the envelope's ceiling is class_b, the honest classification of
        // the ACTUAL delta is class_a — preflight must report class_a, not echo
        // the ceiling.
        let envelope = sample_store_compatibility("auto_safe");
        let preflight = assess_migration_preflight(9, &envelope, true);
        assert_eq!(preflight.status, "auto_safe_migration_after_update");
        assert_eq!(preflight.classification, "class_a");
        assert!(preflight.apply_allowed);
        assert!(preflight.blocked_reason.is_none());
    }

    #[test]
    fn migration_preflight_reports_class_a_for_single_version_bump_within_window() {
        // class_a ceiling, single-version bump (9 -> 10) at the auto floor.
        let mut envelope = sample_store_compatibility("auto_safe");
        envelope.migration_class_max_auto = "class_a".to_owned();
        envelope.auto_migrate_from = 9;
        let preflight = assess_migration_preflight(9, &envelope, true);
        assert_eq!(preflight.status, "auto_safe_migration_after_update");
        assert_eq!(preflight.classification, "class_a");
        assert!(preflight.apply_allowed);
        assert!(preflight.blocked_reason.is_none());
    }

    #[test]
    fn migration_preflight_blocks_class_b_delta_under_class_a_ceiling() {
        // store_schema=10, current=8: a two-version (class_b) jump that exceeds
        // the release's class_a ceiling. Honest classification must NOT claim
        // class_a; it fences the auto path off to an explicit operator gate.
        let mut envelope = sample_store_compatibility("auto_safe");
        envelope.migration_class_max_auto = "class_a".to_owned();
        envelope.auto_migrate_from = 8;
        let preflight = assess_migration_preflight(8, &envelope, true);
        assert_eq!(
            preflight.status,
            "migration_requires_explicit_operator_gate"
        );
        assert_eq!(preflight.classification, "class_b");
        assert!(!preflight.apply_allowed);
        assert_eq!(
            preflight.blocked_reason.as_deref(),
            Some("auto_migration_class_exceeds_published_ceiling")
        );
    }

    #[test]
    fn migration_preflight_blocks_bump_below_auto_migrate_window_floor() {
        // current=7 is below auto_migrate_from=8: outside the auto window.
        let envelope = sample_store_compatibility("auto_safe");
        let preflight = assess_migration_preflight(7, &envelope, true);
        assert_eq!(
            preflight.status,
            "migration_requires_explicit_operator_gate"
        );
        assert!(!preflight.apply_allowed);
        assert_eq!(
            preflight.blocked_reason.as_deref(),
            Some("local_store_schema_outside_auto_migrate_window")
        );
    }

    #[test]
    fn migration_preflight_blocks_class_d_delta_even_within_window() {
        // A wide jump (schema 5 -> 10, delta 5) classifies class_d: no proven
        // auto path, so it fails closed as unsupported even inside the window.
        let mut envelope = sample_store_compatibility("auto_safe");
        envelope.migration_class_max_auto = "class_b".to_owned();
        envelope.auto_migrate_from = 0;
        let preflight = assess_migration_preflight(5, &envelope, true);
        assert_eq!(preflight.status, "migration_unsupported");
        assert_eq!(preflight.classification, "class_d");
        assert!(!preflight.apply_allowed);
        assert_eq!(
            preflight.blocked_reason.as_deref(),
            Some("auto_migration_class_unsupported_for_delta")
        );
    }

    #[test]
    fn migration_preflight_blocks_store_newer_than_target_release() {
        let envelope = sample_store_compatibility("auto_safe");
        let preflight = assess_migration_preflight(11, &envelope, true);
        assert_eq!(preflight.status, "migration_unsupported");
        assert_eq!(preflight.classification, "class_d");
        assert!(!preflight.apply_allowed);
        assert_eq!(
            preflight.blocked_reason.as_deref(),
            Some("local_store_schema_newer_than_target_release")
        );
    }

    #[test]
    fn migration_preflight_blocks_store_below_min_supported() {
        let mut envelope = sample_store_compatibility("auto_safe");
        envelope.min_supported_store_schema = 5;
        let preflight = assess_migration_preflight(4, &envelope, true);
        assert_eq!(preflight.status, "migration_unsupported");
        assert_eq!(preflight.classification, "class_d");
        assert!(!preflight.apply_allowed);
        assert_eq!(
            preflight.blocked_reason.as_deref(),
            Some("local_store_schema_below_min_supported")
        );
    }

    #[test]
    fn envelope_format_compat_ignores_schema_and_policy_differences() {
        // The core of the semantic comparison: an installed binary (auto_safe,
        // newer schema, class_a ceiling) vs an older target release envelope
        // (binary_only, older schema) share the same envelope_version and
        // sidecar_generation, so they are format-COMPATIBLE. The schema/policy
        // delta is assessed by preflight, not treated as an envelope mismatch.
        let embedded = StoreCompatibilityEnvelope {
            envelope_version: 1,
            store_schema_version: 18,
            min_supported_store_schema: 0,
            auto_migrate_from: 17,
            migration_policy: "auto_safe".to_owned(),
            migration_class_max_auto: "class_a".to_owned(),
            sidecar_generation: "v1".to_owned(),
            backup_required: true,
        };
        let published = sample_store_compatibility("binary_only");
        assert!(envelope_formats_compatible(&embedded, &published));
    }

    #[test]
    fn envelope_format_compat_fails_on_version_or_sidecar_mismatch() {
        let embedded = sample_store_compatibility("auto_safe");
        let mut newer_format = embedded.clone();
        newer_format.envelope_version = 2;
        assert!(!envelope_formats_compatible(&embedded, &newer_format));

        let mut newer_sidecar = embedded.clone();
        newer_sidecar.sidecar_generation = "v2".to_owned();
        assert!(!envelope_formats_compatible(&embedded, &newer_sidecar));
    }

    #[test]
    fn migration_preflight_reports_explicit_gate_when_policy_requires_it() {
        let envelope = sample_store_compatibility("explicit_operator_gate");
        let preflight = assess_migration_preflight(9, &envelope, true);
        assert_eq!(
            preflight.status,
            "migration_requires_explicit_operator_gate"
        );
        assert_eq!(preflight.classification, "class_c");
        assert!(!preflight.apply_allowed);
        assert_eq!(
            preflight.blocked_reason.as_deref(),
            Some("target_release_requires_explicit_operator_gate")
        );
    }

    #[test]
    fn migration_preflight_blocks_binary_only_schema_drift_as_unsupported() {
        let envelope = sample_store_compatibility("binary_only");
        let preflight = assess_migration_preflight(9, &envelope, true);
        assert_eq!(preflight.status, "migration_unsupported");
        assert_eq!(preflight.classification, "class_d");
        assert!(!preflight.apply_allowed);
        assert_eq!(
            preflight.blocked_reason.as_deref(),
            Some("binary_only_policy_blocks_schema_migration")
        );
    }

    #[test]
    fn migration_preflight_blocks_when_embedded_and_published_envelopes_mismatch() {
        let envelope = sample_store_compatibility("auto_safe");
        let preflight = assess_migration_preflight(10, &envelope, false);
        assert_eq!(preflight.status, "migration_unsupported");
        assert_eq!(preflight.classification, "class_d");
        assert!(!preflight.apply_allowed);
        assert_eq!(
            preflight.blocked_reason.as_deref(),
            Some("embedded_and_published_envelope_mismatch")
        );
    }

    #[test]
    fn migration_preflight_blocks_when_target_declares_unsupported_policy() {
        let envelope = sample_store_compatibility("unsupported");
        let preflight = assess_migration_preflight(9, &envelope, true);
        assert_eq!(preflight.status, "migration_unsupported");
        assert_eq!(preflight.classification, "class_d");
        assert!(!preflight.apply_allowed);
        assert_eq!(
            preflight.blocked_reason.as_deref(),
            Some("target_release_declares_unsupported_migration_policy")
        );
    }

    fn seed_unsupported_store_schema(root: &Path, schema_version: i64) {
        let layout = StorageLayout::under(root);
        fs::create_dir_all(&layout.root).expect("create store root");
        let conn = SqliteConnection::open(&layout.db_path).expect("open sqlite db");
        conn.pragma_update(None, "user_version", schema_version)
            .expect("set user_version");
    }

    #[test]
    fn sessions_fail_closed_with_store_migration_guidance() {
        let temp = tempdir().expect("tempdir");
        seed_unsupported_store_schema(temp.path(), 9);
        let runtime = CliRuntime {
            cwd: PathBuf::from("."),
            store_root: temp.path().to_path_buf(),
            opencode_bin: "opencode".to_owned(),
        };

        let result = run(&["sessions".to_owned(), "--robot".to_owned()], &runtime);
        assert_eq!(result.exit_code, 3, "{}", result.stderr);
        let payload = parse_robot(&result.stdout);
        assert_eq!(payload["outcome"], "blocked");
        assert_eq!(payload["data"]["reason_code"], "store_migration_blocked");
        assert_eq!(payload["data"]["command"], "rr sessions");
        let expected_block = format!(
            "unsupported automatic migration class class_d from schema v9 to v{}",
            roger_storage::CURRENT_SCHEMA_VERSION
        );
        assert!(
            payload["data"]["blocked_reason"]
                .as_str()
                .unwrap_or_default()
                .contains(&expected_block)
        );
        assert!(
            payload["repair_actions"]
                .as_array()
                .expect("repair actions")
                .iter()
                .any(|value| value
                    .as_str()
                    .is_some_and(|text| text.contains("rr update --dry-run --robot")))
        );
    }

    #[test]
    fn review_resume_and_status_fail_closed_with_store_migration_guidance() {
        let temp = tempdir().expect("tempdir");
        seed_unsupported_store_schema(temp.path(), 9);
        let runtime = CliRuntime {
            cwd: PathBuf::from("."),
            store_root: temp.path().to_path_buf(),
            opencode_bin: "opencode".to_owned(),
        };

        let review = run(
            &[
                "review".to_owned(),
                "--repo".to_owned(),
                "owner/repo".to_owned(),
                "--pr".to_owned(),
                "42".to_owned(),
                "--provider".to_owned(),
                "opencode".to_owned(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(review.exit_code, 3, "{}", review.stderr);
        let review_payload = parse_robot(&review.stdout);
        assert_eq!(review_payload["outcome"], "blocked");
        assert_eq!(
            review_payload["data"]["reason_code"],
            "store_migration_blocked"
        );
        assert_eq!(review_payload["data"]["command"], "rr review");

        let resume = run(
            &[
                "resume".to_owned(),
                "--repo".to_owned(),
                "owner/repo".to_owned(),
                "--pr".to_owned(),
                "42".to_owned(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(resume.exit_code, 3, "{}", resume.stderr);
        let resume_payload = parse_robot(&resume.stdout);
        assert_eq!(resume_payload["outcome"], "blocked");
        assert_eq!(
            resume_payload["data"]["reason_code"],
            "store_migration_blocked"
        );
        assert_eq!(resume_payload["data"]["command"], "rr resume");

        let status = run(
            &[
                "status".to_owned(),
                "--repo".to_owned(),
                "owner/repo".to_owned(),
                "--pr".to_owned(),
                "42".to_owned(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(status.exit_code, 3, "{}", status.stderr);
        let status_payload = parse_robot(&status.stdout);
        assert_eq!(status_payload["outcome"], "blocked");
        assert_eq!(
            status_payload["data"]["reason_code"],
            "store_migration_blocked"
        );
        assert_eq!(status_payload["data"]["command"], "rr status");
    }

    fn write_test_binary(path: &Path, body: &str) {
        fs::write(path, body).expect("write binary fixture");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(path).expect("stat fixture").permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(path, permissions).expect("chmod fixture");
        }
    }

    fn create_update_fixture_archive(
        root: &Path,
        payload_dir: &str,
        binary_name: &str,
        binary_body: &str,
    ) -> (PathBuf, String) {
        let payload_root = root.join("payload-root");
        let payload_path = payload_root.join(payload_dir);
        fs::create_dir_all(&payload_path).expect("create payload dir");
        write_test_binary(&payload_path.join(binary_name), binary_body);

        let archive_name = "fixture-update.tar.gz";
        let archive_path = root.join(archive_name);
        let output = Command::new("tar")
            .arg("-czf")
            .arg(&archive_path)
            .arg("-C")
            .arg(&payload_root)
            .arg(payload_dir)
            .output()
            .expect("run tar for fixture archive");
        assert!(
            output.status.success(),
            "tar fixture archive failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let sha = sha256_for_file(&archive_path).expect("compute fixture sha");
        (archive_path, sha)
    }

    #[test]
    fn update_apply_replaces_binary_in_place_from_fixture_archive() {
        let tmp = tempdir().expect("tempdir");
        let install_dir = tmp.path().join("install");
        fs::create_dir_all(&install_dir).expect("create install dir");
        let binary_name = if cfg!(windows) { "rr.exe" } else { "rr" };
        let install_path = install_dir.join(binary_name);
        write_test_binary(&install_path, "old-binary\n");

        let (archive_path, archive_sha) =
            create_update_fixture_archive(tmp.path(), "payload", binary_name, "new-binary\n");
        let archive_url = format!("file://{}", archive_path.to_string_lossy());
        let outcome = apply_update_archive_in_place(
            &archive_url,
            "fixture-update.tar.gz",
            &archive_sha,
            "payload",
            binary_name,
            &install_path,
            "2026.04.08",
        )
        .expect("apply fixture update");

        assert_eq!(outcome.install_path, install_path);
        let installed = fs::read_to_string(&install_path).expect("read installed binary");
        assert_eq!(installed, "new-binary\n");

        let backup_name = outcome
            .backup_path
            .file_name()
            .expect("backup file name")
            .to_string_lossy()
            .to_string();
        assert!(
            !outcome.backup_path.exists(),
            "expected backup to be removed after successful apply: {}",
            outcome.backup_path.display()
        );
        assert!(backup_name.contains(".backup-"));
    }

    #[test]
    fn update_apply_rolls_back_when_replacement_fails_after_backup() {
        let tmp = tempdir().expect("tempdir");
        let install_dir = tmp.path().join("install");
        fs::create_dir_all(&install_dir).expect("create install dir");
        let binary_name = if cfg!(windows) { "rr.exe" } else { "rr" };
        let install_path = install_dir.join(binary_name);
        write_test_binary(&install_path, "old-binary\n");

        let missing_staged = install_dir.join("missing-staged-binary");
        let err = apply_binary_replacement_with_rollback(&install_path, &missing_staged, "fixture")
            .expect_err("replacement should fail when staged binary is missing");
        assert!(
            err.contains("rollback restored previous binary"),
            "unexpected rollback error: {err}"
        );
        let installed = fs::read_to_string(&install_path).expect("read installed binary");
        assert_eq!(installed, "old-binary\n");
    }

    #[test]
    fn update_install_layout_rejects_mismatched_binary_name() {
        let tmp = tempdir().expect("tempdir");
        let install_path = tmp.path().join("not-rr-binary");
        write_test_binary(&install_path, "binary\n");

        let err = resolve_update_install_path(&install_path, "rr").expect_err("layout should fail");
        assert!(err.contains("does not match expected release binary"));
    }

    /// Builds an installed-mode runtime whose cwd has no Roger workspace
    /// markers, with an unpacked extension package pre-seeded under the
    /// installed layout <store_root>/bridge/extension-package/<version>/.
    fn setup_installed_extension_runtime(
        version: &str,
    ) -> (tempfile::TempDir, CliRuntime, PathBuf) {
        let tmp = tempdir().expect("tempdir");
        let cwd = tmp.path().join("plain-user-dir");
        fs::create_dir_all(&cwd).expect("create plain cwd");
        let store_root = tmp.path().join("store");
        let runtime = CliRuntime {
            cwd,
            store_root: store_root.clone(),
            opencode_bin: "opencode".to_owned(),
        };
        let package_dir = installed_extension_package_dir(&store_root, version);
        fs::create_dir_all(&package_dir).expect("create installed package dir");
        let manifest = json!({
            "manifest_version": 3,
            "name": "Roger Reviewer",
            "version": "2026.6.1.1000",
            "key": TEST_EXTENSION_MANIFEST_KEY,
        });
        fs::write(
            package_dir.join("manifest.json"),
            serde_json::to_string_pretty(&manifest).expect("serialize manifest") + "\n",
        )
        .expect("write installed manifest");
        (tmp, runtime, package_dir)
    }

    struct RegistrationWaitOverride {
        previous: Option<std::ffi::OsString>,
    }

    impl RegistrationWaitOverride {
        // Caller must hold shared_env_guard() for the duration of its test.
        fn set(value: &str) -> Self {
            let previous = std::env::var_os("RR_EXTENSION_SETUP_REGISTRATION_WAIT_MS");
            // SAFETY: tests serialize env mutation via ENV_LOCK and restore on drop.
            unsafe {
                std::env::set_var("RR_EXTENSION_SETUP_REGISTRATION_WAIT_MS", value);
            }
            Self { previous }
        }
    }

    impl Drop for RegistrationWaitOverride {
        fn drop(&mut self) {
            match self.previous.take() {
                // SAFETY: tests serialize env mutation via ENV_LOCK.
                Some(value) => unsafe {
                    std::env::set_var("RR_EXTENSION_SETUP_REGISTRATION_WAIT_MS", value);
                },
                None => unsafe {
                    std::env::remove_var("RR_EXTENSION_SETUP_REGISTRATION_WAIT_MS");
                },
            }
        }
    }

    #[test]
    fn extension_setup_and_doctor_resolve_installed_layout_without_workspace_markers() {
        let _env_guard = shared_env_guard();
        let _wait = RegistrationWaitOverride::set("1");
        let (tmp, runtime, package_dir) = setup_installed_extension_runtime("2026.06.01");
        let install_root = tmp.path().join("install-root");

        let setup = run(
            &[
                "extension".to_owned(),
                "setup".to_owned(),
                "--browser".to_owned(),
                "edge".to_owned(),
                "--install-root".to_owned(),
                install_root.to_string_lossy().to_string(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(setup.exit_code, 0, "{}\n{}", setup.stdout, setup.stderr);
        let setup_payload = parse_robot(&setup.stdout);
        assert_eq!(setup_payload["outcome"], "complete");
        assert_eq!(setup_payload["data"]["subcommand"], "setup");
        assert_eq!(
            setup_payload["data"]["package_source"],
            "installed_layout_newest_available"
        );
        assert_eq!(
            setup_payload["data"]["package_dir"],
            package_dir.to_string_lossy().to_string()
        );
        assert_eq!(
            setup_payload["data"]["extension_id"],
            TEST_EXTENSION_MANIFEST_ID
        );
        assert!(setup_payload["data"]["guided_browser_script_path"].is_null());
        let guided_command = setup_payload["data"]["guided_browser_command"]
            .as_str()
            .expect("guided browser command");
        assert!(
            !guided_command.contains("launch_preloaded_browser.sh"),
            "installed mode must not reference the dev guided-browser script: {guided_command}"
        );
        assert!(
            guided_command.contains("Load unpacked")
                && guided_command.contains("edge://extensions"),
            "edge guidance must direct a manual Load-unpacked pass (Edge 150+ ignores --load-extension, live-verified 2026-07-07): {guided_command}"
        );
        let rendered = setup.stdout.to_string();
        assert!(
            !rendered.contains("run rr extension from the Roger repository root"),
            "installed mode must not demand the Roger repository root: {rendered}"
        );

        let doctor = run(
            &[
                "extension".to_owned(),
                "doctor".to_owned(),
                "--browser".to_owned(),
                "edge".to_owned(),
                "--install-root".to_owned(),
                install_root.to_string_lossy().to_string(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(doctor.exit_code, 0, "{}\n{}", doctor.stdout, doctor.stderr);
        let doctor_payload = parse_robot(&doctor.stdout);
        assert_eq!(doctor_payload["outcome"], "complete");
        assert_eq!(
            doctor_payload["data"]["package_source"],
            "installed_layout_newest_available"
        );
        assert!(doctor_payload["data"]["guided_browser_script_path"].is_null());
        assert!(
            doctor_payload["data"]["checks"]
                .as_array()
                .expect("doctor checks")
                .iter()
                .all(|entry| entry.get("ok").and_then(Value::as_bool).unwrap_or(false))
        );
    }

    #[test]
    fn extension_chrome_installed_mode_guidance_requires_manual_unpacked_load() {
        let _env_guard = shared_env_guard();
        let (tmp, runtime, _package_dir) = setup_installed_extension_runtime("2026.06.01");
        let install_root = tmp.path().join("install-root");

        let doctor = run(
            &[
                "extension".to_owned(),
                "doctor".to_owned(),
                "--browser".to_owned(),
                "chrome".to_owned(),
                "--install-root".to_owned(),
                install_root.to_string_lossy().to_string(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        let doctor_payload = parse_robot(&doctor.stdout);
        let guided_command = doctor_payload["data"]["guided_browser_command"]
            .as_str()
            .expect("guided browser command");
        assert!(
            guided_command.contains("chrome://extensions")
                && guided_command.contains("Load unpacked"),
            "Chrome installed-mode guidance must route through chrome://extensions manual load: {guided_command}"
        );
        assert!(
            guided_command.contains("ignores --load-extension"),
            "Chrome guidance must carry the branded Chrome 137+ flag truth note: {guided_command}"
        );
    }

    #[test]
    fn extension_setup_and_doctor_fail_closed_with_fetch_guidance_when_no_package_exists() {
        let _env_guard = shared_env_guard();
        let tmp = tempdir().expect("tempdir");
        let cwd = tmp.path().join("plain-user-dir");
        fs::create_dir_all(&cwd).expect("create plain cwd");
        let runtime = CliRuntime {
            cwd,
            store_root: tmp.path().join("store"),
            opencode_bin: "opencode".to_owned(),
        };

        for subcommand in ["setup", "doctor"] {
            let result = run(
                &[
                    "extension".to_owned(),
                    subcommand.to_owned(),
                    "--browser".to_owned(),
                    "edge".to_owned(),
                    "--robot".to_owned(),
                ],
                &runtime,
            );
            assert_eq!(result.exit_code, 3, "{}\n{}", result.stdout, result.stderr);
            let payload = parse_robot(&result.stdout);
            assert_eq!(payload["outcome"], "blocked");
            assert_eq!(payload["data"]["reason_code"], "extension_package_missing");
            assert!(
                payload["repair_actions"]
                    .as_array()
                    .expect("repair actions")
                    .iter()
                    .any(|action| action
                        .as_str()
                        .unwrap_or_default()
                        .contains("rr extension fetch")),
                "missing-package guidance must name rr extension fetch: {}",
                result.stdout
            );
            assert!(
                !result
                    .stdout
                    .contains("run rr extension from the Roger repository root"),
                "installed mode must not demand the Roger repository root: {}",
                result.stdout
            );
        }
    }

    #[test]
    fn extension_uninstall_works_outside_roger_workspace() {
        let _env_guard = shared_env_guard();
        let tmp = tempdir().expect("tempdir");
        let cwd = tmp.path().join("plain-user-dir");
        fs::create_dir_all(&cwd).expect("create plain cwd");
        let runtime = CliRuntime {
            cwd,
            store_root: tmp.path().join("store"),
            opencode_bin: "opencode".to_owned(),
        };
        let install_root = tmp.path().join("install-root");

        let result = run(
            &[
                "extension".to_owned(),
                "uninstall".to_owned(),
                "--install-root".to_owned(),
                install_root.to_string_lossy().to_string(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(result.exit_code, 0, "{}\n{}", result.stdout, result.stderr);
        let payload = parse_robot(&result.stdout);
        assert_eq!(payload["outcome"], "complete");
        assert_eq!(payload["data"]["surface"], "extension");
    }

    #[test]
    fn extension_fetch_blocks_for_local_build_without_release_metadata() {
        let tmp = tempdir().expect("tempdir");
        let runtime = CliRuntime {
            cwd: tmp.path().to_path_buf(),
            store_root: tmp.path().join("store"),
            opencode_bin: "opencode".to_owned(),
        };

        let result = run(
            &[
                "extension".to_owned(),
                "fetch".to_owned(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(result.exit_code, 3, "{}\n{}", result.stdout, result.stderr);
        let payload = parse_robot(&result.stdout);
        assert_eq!(payload["outcome"], "blocked");
        assert_eq!(payload["data"]["reason_code"], "local_or_unpublished_build");
        assert!(
            payload["repair_actions"]
                .as_array()
                .expect("repair actions")
                .iter()
                .any(|action| action
                    .as_str()
                    .unwrap_or_default()
                    .contains("rr extension setup")),
            "local-build guidance must point at the dev workspace path: {}",
            result.stdout
        );
    }

    /// Writes a minimal fixture release directory (install metadata,
    /// SHA256SUMS, extension zip bytes) and returns the file:// download root.
    fn write_extension_fetch_fixture_release(
        root: &Path,
        version: &str,
        archive_bytes: &[u8],
        corrupt_checksum: bool,
    ) -> String {
        let tag = format!("v{version}");
        let release_dir = root.join("releases").join(&tag);
        fs::create_dir_all(&release_dir).expect("create fixture release dir");
        let artifact_stem = format!("roger-reviewer-{version}");
        let archive_name = format!("{artifact_stem}-extension.zip");
        fs::write(release_dir.join(&archive_name), archive_bytes).expect("write fixture zip");
        let archive_sha = if corrupt_checksum {
            "0".repeat(64)
        } else {
            sha256_hex(archive_bytes)
        };
        fs::write(
            release_dir.join("SHA256SUMS"),
            format!("{archive_sha}  {archive_name}\n"),
        )
        .expect("write fixture checksums");
        let install_metadata = json!({
            "schema": "roger.release.install-metadata.v1",
            "release": {
                "channel": "stable",
                "version": version,
                "tag": tag,
                "prerelease": false,
                "artifact_stem": artifact_stem,
            },
            "checksums_name": "SHA256SUMS",
            "core_manifest_name": format!("release-core-manifest-{version}.json"),
            "targets": [],
        });
        fs::write(
            release_dir.join(format!("release-install-metadata-{version}.json")),
            serde_json::to_string_pretty(&install_metadata).expect("serialize install metadata"),
        )
        .expect("write fixture install metadata");
        format!("file://{}/releases", root.to_string_lossy())
    }

    #[test]
    fn extension_fetch_fails_closed_on_checksum_mismatch_from_fixture_release() {
        let tmp = tempdir().expect("tempdir");
        let runtime = CliRuntime {
            cwd: tmp.path().to_path_buf(),
            store_root: tmp.path().join("store"),
            opencode_bin: "opencode".to_owned(),
        };
        let version = "2026.06.02";
        let download_root = write_extension_fetch_fixture_release(
            tmp.path(),
            version,
            b"not-a-real-zip-but-checksum-fails-first",
            true,
        );

        let result = run(
            &[
                "extension".to_owned(),
                "fetch".to_owned(),
                "--version".to_owned(),
                version.to_owned(),
                "--download-root".to_owned(),
                download_root,
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(result.exit_code, 3, "{}\n{}", result.stdout, result.stderr);
        let payload = parse_robot(&result.stdout);
        assert_eq!(payload["outcome"], "blocked");
        assert_eq!(
            payload["data"]["reason_code"],
            "extension_archive_checksum_mismatch"
        );
        let package_dir = installed_extension_package_dir(&runtime.store_root, version);
        assert!(
            !package_dir.exists(),
            "checksum mismatch must not install a package: {}",
            package_dir.display()
        );
        let version_root = installed_extension_package_root(&runtime.store_root).join(version);
        if version_root.exists() {
            let leftovers: Vec<_> = fs::read_dir(&version_root)
                .expect("read version root")
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().to_string())
                .collect();
            assert!(
                leftovers.is_empty(),
                "staging leftovers after fail-closed fetch: {leftovers:?}"
            );
        }
    }

    #[test]
    fn extension_fetch_blocks_when_release_has_no_extension_asset() {
        let tmp = tempdir().expect("tempdir");
        let runtime = CliRuntime {
            cwd: tmp.path().to_path_buf(),
            store_root: tmp.path().join("store"),
            opencode_bin: "opencode".to_owned(),
        };
        let version = "2026.06.03";
        let download_root =
            write_extension_fetch_fixture_release(tmp.path(), version, b"zip-bytes", false);
        // Remove the extension entry from the checksums manifest.
        let checksums_path = tmp
            .path()
            .join("releases")
            .join(format!("v{version}"))
            .join("SHA256SUMS");
        fs::write(&checksums_path, "0000  some-other-asset.tar.gz\n")
            .expect("rewrite checksums without extension entry");

        let result = run(
            &[
                "extension".to_owned(),
                "fetch".to_owned(),
                "--version".to_owned(),
                version.to_owned(),
                "--download-root".to_owned(),
                download_root,
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(result.exit_code, 3, "{}\n{}", result.stdout, result.stderr);
        let payload = parse_robot(&result.stdout);
        assert_eq!(payload["data"]["reason_code"], "extension_asset_missing");
    }

    fn create_fixture_extension_zip(root: &Path) -> Vec<u8> {
        let source_dir = root.join("zip-source");
        fs::create_dir_all(source_dir.join("src")).expect("create zip source tree");
        let manifest = json!({
            "manifest_version": 3,
            "name": "Roger Reviewer",
            "version": "2026.6.4.1000",
            "key": TEST_EXTENSION_MANIFEST_KEY,
        });
        fs::write(
            source_dir.join("manifest.json"),
            serde_json::to_string_pretty(&manifest).expect("serialize zip manifest") + "\n",
        )
        .expect("write zip manifest");
        fs::write(source_dir.join("src/main.js"), "export const ok = true;\n")
            .expect("write zip src");

        let archive_path = root.join("fixture-extension.zip");
        let output = Command::new("python3")
            .arg("-m")
            .arg("zipfile")
            .arg("-c")
            .arg(&archive_path)
            .arg("manifest.json")
            .arg("src")
            .current_dir(&source_dir)
            .output()
            .expect("run python3 zipfile for fixture zip");
        assert!(
            output.status.success(),
            "fixture zip creation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        fs::read(&archive_path).expect("read fixture zip bytes")
    }

    #[test]
    fn extension_fetch_installs_verified_package_then_doctor_resolves_it() {
        let _env_guard = shared_env_guard();
        let tmp = tempdir().expect("tempdir");
        let cwd = tmp.path().join("plain-user-dir");
        fs::create_dir_all(&cwd).expect("create plain cwd");
        let runtime = CliRuntime {
            cwd,
            store_root: tmp.path().join("store"),
            opencode_bin: "opencode".to_owned(),
        };
        let version = "2026.06.04";
        let archive_bytes = create_fixture_extension_zip(tmp.path());
        let download_root =
            write_extension_fetch_fixture_release(tmp.path(), version, &archive_bytes, false);

        let fetch = run(
            &[
                "extension".to_owned(),
                "fetch".to_owned(),
                "--version".to_owned(),
                version.to_owned(),
                "--download-root".to_owned(),
                download_root,
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(fetch.exit_code, 0, "{}\n{}", fetch.stdout, fetch.stderr);
        let fetch_payload = parse_robot(&fetch.stdout);
        assert_eq!(fetch_payload["outcome"], "complete");
        assert_eq!(fetch_payload["data"]["subcommand"], "fetch");
        let package_dir = PathBuf::from(
            fetch_payload["data"]["package_dir"]
                .as_str()
                .expect("fetched package dir"),
        );
        assert_eq!(
            package_dir,
            installed_extension_package_dir(&runtime.store_root, version)
        );
        assert!(package_dir.join("manifest.json").is_file());
        assert!(package_dir.join("src/main.js").is_file());
        let fetch_manifest_path = PathBuf::from(
            fetch_payload["data"]["fetch_manifest_path"]
                .as_str()
                .expect("fetch manifest path"),
        );
        assert!(fetch_manifest_path.is_file());

        let install_root = tmp.path().join("install-root");
        let doctor = run(
            &[
                "extension".to_owned(),
                "doctor".to_owned(),
                "--browser".to_owned(),
                "edge".to_owned(),
                "--install-root".to_owned(),
                install_root.to_string_lossy().to_string(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        let doctor_payload = parse_robot(&doctor.stdout);
        assert_eq!(
            doctor_payload["data"]["package_dir"],
            package_dir.to_string_lossy().to_string()
        );
        assert_eq!(
            doctor_payload["data"]["package_source"],
            "installed_layout_newest_available"
        );
        let checks = doctor_payload["data"]["checks"]
            .as_array()
            .expect("doctor checks");
        let package_check = checks
            .iter()
            .find(|entry| entry["name"] == "extension_package_present")
            .expect("package presence check");
        assert_eq!(package_check["ok"], true);
    }

    // ---- CLI surface simplification: parser + help + whitelist coverage ----

    fn pa(args: &[&str]) -> Result<ParsedArgs, String> {
        let owned: Vec<String> = args.iter().map(|s| (*s).to_owned()).collect();
        parse_args(&owned)
    }

    fn debug_of(args: &[&str]) -> String {
        format!("{:?}", pa(args).expect("parse ok"))
    }

    #[test]
    fn fresh_flag_is_only_accepted_by_review() {
        assert!(pa(&["review", "--pr", "5", "--fresh"]).is_ok());
        assert_eq!(
            pa(&["status", "--fresh"]).unwrap_err(),
            "--fresh is only supported by rr review"
        );
        assert_eq!(
            pa(&["sessions", "--fresh"]).unwrap_err(),
            "--fresh is only supported by rr review"
        );
    }

    #[test]
    fn all_flag_is_only_accepted_by_sessions() {
        assert!(pa(&["sessions", "--all"]).is_ok());
        assert!(pa(&["findings", "--sessions", "--all"]).is_ok());
        assert_eq!(
            pa(&["review", "--pr", "5", "--all"]).unwrap_err(),
            "--all is only supported by rr sessions"
        );
    }

    #[test]
    fn reuse_decision_treats_only_failed_as_terminal() {
        assert!(attention_state_is_reusable("review_launched"));
        assert!(attention_state_is_reusable("awaiting_user_input"));
        assert!(attention_state_is_reusable("findings_ready"));
        assert!(attention_state_is_reusable("returned_to_roger"));
        assert!(!attention_state_is_reusable("review_failed"));
    }

    fn reuse_finder_entry(id: &str, attention: &str, updated_at: i64) -> SessionFinderEntry {
        SessionFinderEntry {
            session_id: id.to_owned(),
            repository: "owner/repo".to_owned(),
            pull_request_number: 42,
            attention_state: attention.to_owned(),
            provider: "opencode".to_owned(),
            updated_at,
        }
    }

    #[test]
    fn select_reusable_session_picks_newest_non_terminal() {
        let candidates = vec![
            reuse_finder_entry("old", "review_launched", 100),
            reuse_finder_entry("failed-newest", "review_failed", 900),
            reuse_finder_entry("newest-live", "awaiting_user_input", 500),
        ];
        let picked = select_reusable_session(&candidates).expect("a reusable session");
        assert_eq!(picked.session_id, "newest-live");
    }

    #[test]
    fn select_reusable_session_returns_none_when_all_terminal() {
        let candidates = vec![reuse_finder_entry("a", "review_failed", 100)];
        assert!(select_reusable_session(&candidates).is_none());
    }

    #[test]
    fn relative_age_labels_are_compact() {
        assert_eq!(format_relative_age(1_000, 990), "10s ago");
        assert_eq!(format_relative_age(1_000, 400), "10m ago");
        assert_eq!(format_relative_age(10_000, 2_800), "2h ago");
        assert_eq!(format_relative_age(200_000, 5_600), "2d ago");
        // Clock skew never produces a negative age.
        assert_eq!(format_relative_age(100, 500), "0s ago");
    }

    #[test]
    fn grouped_session_render_caps_at_five_with_older_note() {
        let now = 1_000_000;
        let entries: Vec<Value> = (0..7)
            .map(|i| {
                json!({
                    "session_id": format!("s{i}"),
                    "repository": "owner/repo",
                    "pull_request": 42,
                    "provider": "opencode",
                    "attention_state": "review_launched",
                    "updated_at": now - (i as i64) * 60,
                })
            })
            .collect();
        let default_view = render_grouped_session_lines(&entries, false, now);
        assert!(default_view.contains("owner/repo#42:"));
        // Newest first, capped at five.
        assert!(default_view.contains("s0  opencode  review_launched  0s ago"));
        assert!(default_view.contains("s4  opencode  review_launched"));
        assert!(!default_view.contains("s5  "));
        assert!(default_view.contains("and 2 older sessions (rr sessions --all to list)"));

        let all_view = render_grouped_session_lines(&entries, true, now);
        assert!(all_view.contains("s6  opencode  review_launched"));
        assert!(!all_view.contains("older sessions"));
    }

    #[test]
    fn review_next_commands_add_copilot_interactive_hint_only_when_batch() {
        let batch = review_next_commands(
            "sess-1",
            session_copilot::PROVIDER_ID,
            "owner/repo",
            42,
            false,
        );
        assert!(batch.iter().any(|c| c == "rr open --session sess-1"));
        assert!(batch.iter().any(|c| c.contains("--interactive")));
        let interactive = review_next_commands(
            "sess-1",
            session_copilot::PROVIDER_ID,
            "owner/repo",
            42,
            true,
        );
        assert!(!interactive.iter().any(|c| c.contains("--interactive")));
        let opencode = review_next_commands("sess-1", "opencode", "owner/repo", 42, false);
        assert!(!opencode.iter().any(|c| c.contains("--interactive")));
    }

    #[test]
    fn copilot_seed_embeds_worker_task_path_and_protocol() {
        let target = ReviewTarget {
            repository: "owner/repo".to_owned(),
            pull_request_number: 42,
            base_ref: "main".to_owned(),
            head_ref: "feature".to_owned(),
            base_commit: "aaa".to_owned(),
            head_commit: "bbb".to_owned(),
        };
        let seed = copilot_worker_seed_prompt(&target, "/store/sessions/sess-1/worker-task.json");
        assert!(seed.contains("/store/sessions/sess-1/worker-task.json"));
        assert!(seed.contains("worker.get_review_context"));
        assert!(seed.contains("worker.submit_stage_result"));
        assert!(seed.contains("worker.search_memory"));
        assert!(seed.contains("task_nonce"));
        assert!(seed.contains("do not post to GitHub"));
    }

    #[test]
    fn worker_task_file_path_is_canonical() {
        let path = worker_task_file_path(Path::new("/store"), "sess-1");
        assert_eq!(
            path,
            Path::new("/store/sessions/sess-1/worker-task.json").to_path_buf()
        );
    }

    #[test]
    fn send_container_parses_identically_to_underlying_commands() {
        assert_eq!(
            debug_of(&["send", "triage", "--finding", "f1", "--state", "accepted"]),
            debug_of(&["triage", "--finding", "f1", "--state", "accepted"]),
        );
        assert_eq!(
            debug_of(&["send", "draft", "--all-findings"]),
            debug_of(&["draft", "--all-findings"]),
        );
        assert_eq!(
            debug_of(&["send", "approve", "--batch", "b1"]),
            debug_of(&["approve", "--batch", "b1"]),
        );
        assert_eq!(
            debug_of(&["send", "post", "--batch", "b1", "--robot"]),
            debug_of(&["post", "--batch", "b1", "--robot"]),
        );
    }

    #[test]
    fn setup_container_parses_identically_to_underlying_commands() {
        assert_eq!(
            debug_of(&["setup", "extension", "--browser", "edge"]),
            debug_of(&["extension", "setup", "--browser", "edge"]),
        );
        assert_eq!(
            debug_of(&["setup", "doctor", "--browser", "brave"]),
            debug_of(&["extension", "doctor", "--browser", "brave"]),
        );
        assert_eq!(
            debug_of(&["setup", "fetch", "--version", "2026.07.01"]),
            debug_of(&["extension", "fetch", "--version", "2026.07.01"]),
        );
        assert_eq!(
            debug_of(&["setup", "uninstall"]),
            debug_of(&["extension", "uninstall"]),
        );
        assert_eq!(
            debug_of(&["setup", "update", "--yes"]),
            debug_of(&["update", "--yes"]),
        );
        assert_eq!(
            debug_of(&["setup", "assets", "verify", "--robot"]),
            debug_of(&["assets", "verify", "--robot"]),
        );
    }

    #[test]
    fn api_docs_parses_identically_to_robot_docs() {
        for topic in ["guide", "commands", "schemas", "workflows"] {
            assert_eq!(
                debug_of(&["api", "docs", topic, "--robot"]),
                debug_of(&["robot-docs", topic, "--robot"]),
            );
        }
    }

    #[test]
    fn alias_forms_route_to_the_underlying_schema_ids() {
        assert_eq!(
            pa(&["send", "post", "--batch", "b1"])
                .unwrap()
                .command
                .schema_id(),
            "rr.robot.post.v1"
        );
        assert_eq!(
            pa(&["review", "--resume", "--pr", "5"])
                .unwrap()
                .command
                .schema_id(),
            "rr.robot.resume.v1"
        );
        assert_eq!(
            pa(&["findings", "--query", "auth"])
                .unwrap()
                .command
                .schema_id(),
            "rr.robot.search.v1"
        );
        assert_eq!(
            pa(&["findings", "--sessions"]).unwrap().command.schema_id(),
            "rr.robot.sessions.v1"
        );
        assert_eq!(
            pa(&["setup", "update", "--yes"])
                .unwrap()
                .command
                .schema_id(),
            "rr.robot.update.v1"
        );
    }

    #[test]
    fn review_resume_flips_command_and_findings_flips_route() {
        assert_eq!(
            pa(&["review", "--resume"]).unwrap().command,
            CommandKind::Resume
        );
        assert_eq!(
            pa(&["findings", "--query", "x"]).unwrap().command,
            CommandKind::Search
        );
        assert_eq!(
            pa(&["findings", "--sessions"]).unwrap().command,
            CommandKind::Sessions
        );
        assert_eq!(pa(&["findings"]).unwrap().command, CommandKind::Findings);
        // findings cannot combine --query and --sessions.
        assert!(pa(&["findings", "--query", "x", "--sessions"]).is_err());
        // routing flags are rejected on unrelated commands.
        assert!(pa(&["status", "--resume"]).is_err());
        assert!(pa(&["status", "--sessions"]).is_err());
    }

    #[test]
    fn send_edit_routes_to_edit_and_unknown_subcommands_are_named() {
        // `rr send edit` is now a real command (routes to CommandKind::Edit).
        let edit = pa(&["send", "edit", "--draft", "d1", "--body-file", "/tmp/b"]).unwrap();
        assert_eq!(edit.command, CommandKind::Edit);
        assert_eq!(edit.edit_draft_id.as_deref(), Some("d1"));
        let bad_send = pa(&["send", "bogus"]).unwrap_err();
        assert!(bad_send.contains("unknown send subcommand"), "{bad_send}");
        let bad_setup = pa(&["setup", "bogus"]).unwrap_err();
        assert!(
            bad_setup.contains("unknown setup subcommand"),
            "{bad_setup}"
        );
        let bad_api = pa(&["api", "bogus"]).unwrap_err();
        assert!(bad_api.contains("unknown api subcommand"), "{bad_api}");
    }

    #[test]
    fn per_command_help_prints_focused_usage_at_any_position() {
        let runtime = CliRuntime {
            cwd: PathBuf::from("."),
            store_root: PathBuf::from(".roger-test"),
            opencode_bin: "opencode".to_owned(),
        };
        let review = run(&["review".to_owned(), "--help".to_owned()], &runtime);
        assert_eq!(review.exit_code, 0, "{}", review.stderr);
        assert!(review.stdout.contains("rr review"), "{}", review.stdout);
        assert!(review.stderr.is_empty(), "{}", review.stderr);

        // --help anywhere after the command, not only first.
        let review_mid = run(
            &[
                "review".to_owned(),
                "--pr".to_owned(),
                "5".to_owned(),
                "--help".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(review_mid.exit_code, 0, "{}", review_mid.stderr);
        assert!(
            review_mid.stdout.contains("rr review"),
            "{}",
            review_mid.stdout
        );

        let send_help = run(&["send".to_owned(), "-h".to_owned()], &runtime);
        assert_eq!(send_help.exit_code, 0, "{}", send_help.stderr);
        assert!(send_help.stdout.contains("rr send"), "{}", send_help.stdout);

        let send_sub_help = run(
            &["send".to_owned(), "post".to_owned(), "--help".to_owned()],
            &runtime,
        );
        assert_eq!(send_sub_help.exit_code, 0, "{}", send_sub_help.stderr);
        assert!(
            send_sub_help.stdout.contains("rr send"),
            "{}",
            send_sub_help.stdout
        );

        let setup_help = run(&["setup".to_owned(), "--help".to_owned()], &runtime);
        assert_eq!(setup_help.exit_code, 0, "{}", setup_help.stderr);
        assert!(
            setup_help.stdout.contains("rr setup"),
            "{}",
            setup_help.stdout
        );

        let api_help = run(
            &["api".to_owned(), "docs".to_owned(), "--help".to_owned()],
            &runtime,
        );
        assert_eq!(api_help.exit_code, 0, "{}", api_help.stderr);
        assert!(
            api_help.stdout.contains("rr api docs"),
            "{}",
            api_help.stdout
        );

        // Bare global help still works.
        let global = run(&["--help".to_owned()], &runtime);
        assert_eq!(global.exit_code, 0, "{}", global.stderr);
        assert!(
            global.stdout.contains("The seven verbs:"),
            "{}",
            global.stdout
        );
    }

    #[test]
    fn new_command_whitelists_reject_foreign_flags() {
        assert!(
            pa(&["review", "--session", "s1"])
                .unwrap_err()
                .contains("rr review only supports")
        );
        assert!(
            pa(&["resume", "--provider", "codex"])
                .unwrap_err()
                .contains("rr resume only supports")
        );
        assert!(
            pa(&["return", "--limit", "3"])
                .unwrap_err()
                .contains("rr return only supports")
        );
        assert!(
            pa(&["sessions", "--session", "s1"])
                .unwrap_err()
                .contains("rr sessions only supports")
        );
        assert!(
            pa(&["search", "--query", "x", "--provider", "codex"])
                .unwrap_err()
                .contains("rr search only supports")
        );
        assert!(
            pa(&["status", "--limit", "3"])
                .unwrap_err()
                .contains("rr status only supports")
        );
        assert!(
            pa(&["findings", "--attention", "stale"])
                .unwrap_err()
                .contains("rr findings only supports")
        );
        assert!(
            pa(&["bridge", "verify-contracts", "--pr", "5"])
                .unwrap_err()
                .contains("rr bridge only supports")
        );
        assert!(
            pa(&["extension", "setup", "--pr", "5"])
                .unwrap_err()
                .contains("rr extension only supports")
        );
        assert!(
            pa(&["robot-docs", "guide", "--pr", "5"])
                .unwrap_err()
                .contains("rr robot-docs only supports")
        );
    }

    #[test]
    fn dry_run_is_rejected_by_commands_that_do_not_implement_it() {
        for (args, needle) in [
            (
                &["return", "--dry-run"][..],
                "rr return does not support --dry-run",
            ),
            (
                &["sessions", "--dry-run"][..],
                "rr sessions does not support --dry-run",
            ),
            (
                &["search", "--query", "x", "--dry-run"][..],
                "rr search does not support --dry-run",
            ),
            (
                &["status", "--dry-run"][..],
                "rr status does not support --dry-run",
            ),
            (
                &["findings", "--dry-run"][..],
                "rr findings does not support --dry-run",
            ),
            (
                &["bridge", "verify-contracts", "--dry-run"][..],
                "rr bridge does not support --dry-run",
            ),
            (
                &["extension", "doctor", "--dry-run"][..],
                "rr extension does not support --dry-run",
            ),
            (
                &["robot-docs", "guide", "--dry-run"][..],
                "rr robot-docs does not support --dry-run",
            ),
        ] {
            let err = pa(args).unwrap_err();
            assert!(err.contains(needle), "args={args:?} err={err}");
        }
        // review/resume/update DO support --dry-run.
        assert!(pa(&["review", "--pr", "5", "--dry-run"]).is_ok());
        assert!(pa(&["resume", "--pr", "5", "--dry-run"]).is_ok());
        assert!(pa(&["update", "--dry-run"]).is_ok());
    }
}
