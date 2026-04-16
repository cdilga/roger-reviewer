#![recursion_limit = "256"]

use roger_app_core::cli_config;
use roger_app_core::time;
use roger_app_core::{
    AGENT_TRANSPORT_REQUEST_SCHEMA_V1, AGENT_TRANSPORT_RESPONSE_SCHEMA_V1, AgentTransportErrorCode,
    AgentTransportRequestEnvelope, AgentTransportResponseEnvelope, AgentTransportResponseStatus,
    AppError, ApprovalState, ContinuityQuality, ExplicitPostingInput, ExplicitPostingOutcome,
    FindingTriageState, HarnessAdapter, LaunchAction, LaunchIntent, OutboundApprovalToken,
    OutboundDraft, OutboundDraftBatch, RecallSourceRef, ResumeAttemptOutcome, ResumeBundle,
    ResumeBundleProfile, ReviewTarget, ReviewTask, RogerCommand, RogerCommandId,
    RogerCommandInvocationSurface, RogerCommandResult, RogerCommandRouteStatus, SearchPlanError,
    SearchPlanInput, SearchQueryPlanError, SearchRetrievalClass, SessionLocator, Surface,
    WorkerArtifactExcerpt, WorkerArtifactExcerptRequest, WorkerCapabilityProfile,
    WorkerContextPacket, WorkerEvidenceLocation, WorkerFindingDetail, WorkerFindingDetailRequest,
    WorkerFindingListResponse, WorkerFindingSummary, WorkerGatewaySnapshot, WorkerGitHubPosture,
    WorkerMutationPosture, WorkerOperation, WorkerOperationRequestEnvelope, WorkerRecallEnvelope,
    WorkerSearchMemoryRequest, WorkerSearchMemoryResponse, WorkerStatusSnapshot,
    WorkerTransportKind, execute_agent_transport_request, execute_explicit_posting_flow,
    materialize_search_plan, outbound_target_tuple_json, route_harness_command,
    safe_harness_command_bindings, validate_outbound_draft_batch_linkage,
};
use roger_bridge::{
    NativeHostManifest, SupportedBrowser, SupportedOs, native_host_install_path_for,
};
use roger_config::cli_defaults::{DEFAULT_OPENCODE_BIN, ENV_OPENCODE_BIN, ENV_STORE_ROOT};
use roger_config::{ResolvedProviderCapability, ResolvedRoutineSurfaceBaseline};
use roger_github_adapter::GhCliAdapter;
use roger_session_claude::{ClaudeAdapter, ClaudeSessionPath};
use roger_session_codex::{CodexAdapter, CodexSessionPath};
use roger_session_gemini::{GeminiAdapter, GeminiSessionPath};
use roger_session_opencode::{
    OpenCodeAdapter, OpenCodeReturnPath, OpenCodeSessionPath, rr_return_to_roger_session,
};
use roger_storage::{
    CreateLaunchAttempt, CreateReviewRun, CreateReviewSession, CreateSessionLaunchBinding,
    FinalizeReviewLaunchAttempt, LaunchAttemptAction, LaunchAttemptState, LaunchSurface,
    PriorReviewLookupQuery, PriorReviewRetrievalMode, ResolveSessionLaunchBinding,
    ResolveSessionLocalRoot,
    ResolveSessionReentry, ReviewLaunchFinalizationError, RogerStore, SessionBindingResolution,
    SessionFinderEntry, SessionFinderQuery, SessionLaunchBindingRecord, SessionReentryResolution,
    StorageLayout, UpdateLaunchAttempt,
};
use rusqlite::Connection as SqliteConnection;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::result::Result;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};
use toon_format::encode_default as encode_toon_default;

static ID_SEQ: AtomicU64 = AtomicU64::new(1);
const SUPPORTED_REVIEW_PROVIDERS: [&str; 4] = ["opencode", "codex", "gemini", "claude"];
const PLANNED_REVIEW_PROVIDERS: [&str; 1] = ["copilot"];
const NOT_LIVE_REVIEW_PROVIDERS: [&str; 1] = ["pi-agent"];

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
            .unwrap_or_else(|| cwd.join(".roger"));
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
    Review,
    Resume,
    Return,
    Sessions,
    Search,
    Draft,
    Approve,
    Post,
    Update,
    Bridge,
    Extension,
    RobotDocs,
    Findings,
    Status,
}

impl CommandKind {
    fn as_rr_command(self, dry_run: bool) -> &'static str {
        match (self, dry_run) {
            (Self::Agent, _) => "rr agent",
            (Self::Review, true) => "rr review --dry-run",
            (Self::Resume, true) => "rr resume --dry-run",
            (Self::Review, false) => "rr review",
            (Self::Resume, false) => "rr resume",
            (Self::Return, _) => "rr return",
            (Self::Sessions, _) => "rr sessions",
            (Self::Search, _) => "rr search",
            (Self::Draft, _) => "rr draft",
            (Self::Approve, _) => "rr approve",
            (Self::Post, _) => "rr post",
            (Self::Update, _) => "rr update",
            (Self::Bridge, _) => "rr bridge",
            (Self::Extension, _) => "rr extension",
            (Self::RobotDocs, _) => "rr robot-docs",
            (Self::Findings, _) => "rr findings",
            (Self::Status, _) => "rr status",
        }
    }

    fn schema_id(self) -> &'static str {
        match self {
            Self::Agent => "rr.agent.transport.v1",
            Self::Review => "rr.robot.review.v1",
            Self::Resume => "rr.robot.resume.v1",
            Self::Return => "rr.robot.return.v1",
            Self::Sessions => "rr.robot.sessions.v1",
            Self::Search => "rr.robot.search.v1",
            Self::Draft => "rr.robot.draft.v1",
            Self::Approve => "rr.robot.approve.v1",
            Self::Post => "rr.robot.post.v1",
            Self::Update => "rr.robot.update.v1",
            Self::Bridge => "rr.robot.bridge.v1",
            Self::Extension => "rr.robot.extension.v1",
            Self::RobotDocs => "rr.robot.robot_docs.v1",
            Self::Findings => "rr.robot.findings.v1",
            Self::Status => "rr.robot.status.v1",
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
    agent_context_file: Option<PathBuf>,
    agent_capability_file: Option<PathBuf>,
    bridge_command: Option<BridgeCommandKind>,
    extension_command: Option<ExtensionCommandKind>,
    extension_browser: Option<SupportedBrowser>,
    bridge_extension_id: Option<String>,
    bridge_binary_path: Option<PathBuf>,
    bridge_install_root: Option<PathBuf>,
    bridge_output_dir: Option<PathBuf>,
    repo: Option<String>,
    pr: Option<u64>,
    session_id: Option<String>,
    draft_finding_ids: Vec<String>,
    draft_all_findings: bool,
    batch_id: Option<String>,
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
    robot_docs_topic: Option<String>,
    robot: bool,
    robot_format: RobotFormat,
    dry_run: bool,
    provider: String,
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

fn parse_args(argv: &[String]) -> Result<ParsedArgs, String> {
    if argv.is_empty() {
        return Err("missing command".to_owned());
    }

    let command = match argv[0].as_str() {
        "agent" => CommandKind::Agent,
        "review" => CommandKind::Review,
        "resume" => CommandKind::Resume,
        "return" => CommandKind::Return,
        "sessions" => CommandKind::Sessions,
        "search" => CommandKind::Search,
        "draft" => CommandKind::Draft,
        "approve" => CommandKind::Approve,
        "post" => CommandKind::Post,
        "update" => CommandKind::Update,
        "bridge" => CommandKind::Bridge,
        "extension" => CommandKind::Extension,
        "robot-docs" => CommandKind::RobotDocs,
        "findings" => CommandKind::Findings,
        "status" => CommandKind::Status,
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
        agent_context_file: None,
        agent_capability_file: None,
        bridge_command: None,
        extension_command: None,
        extension_browser: None,
        bridge_extension_id: None,
        bridge_binary_path: None,
        bridge_install_root: None,
        bridge_output_dir: None,
        repo: None,
        pr: None,
        session_id: None,
        draft_finding_ids: Vec::new(),
        draft_all_findings: false,
        batch_id: None,
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
        robot_docs_topic: None,
        robot: false,
        robot_format: RobotFormat::Json,
        dry_run: false,
        provider: "opencode".to_owned(),
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
            "--batch" => {
                let value = argv
                    .get(i + 1)
                    .ok_or_else(|| "--batch requires a value".to_owned())?;
                parsed.batch_id = Some(value.clone());
                i += 2;
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
                parsed.provider = value.clone();
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
                    _ => {
                        return Err(format!("unexpected positional argument: {positional}"));
                    }
                }
            }
        }
    }

    match parsed.robot_format {
        RobotFormat::Compact
            if !matches!(
                parsed.command,
                CommandKind::Status
                    | CommandKind::Findings
                    | CommandKind::Sessions
                    | CommandKind::Search
                    | CommandKind::RobotDocs
            ) =>
        {
            return Err(
                "compact format is only supported for status/findings/sessions/search/robot-docs in this slice".to_owned(),
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

    if parsed.command == CommandKind::Bridge && parsed.bridge_command.is_none() {
        return Err(
            "rr bridge requires a subcommand: export-contracts, verify-contracts, pack-extension, install, or uninstall".to_owned(),
        );
    }

    if parsed.command == CommandKind::Agent && parsed.agent_operation.is_none() {
        return Err("rr agent requires an operation name".to_owned());
    }

    if parsed.command == CommandKind::Extension && parsed.extension_command.is_none() {
        return Err("rr extension requires a subcommand: setup or doctor".to_owned());
    }

    if parsed.command != CommandKind::Search && parsed.query_mode.is_some() {
        return Err("--query-mode is only supported by rr search".to_owned());
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

    if parsed.command != CommandKind::Draft
        && (parsed.draft_all_findings || !parsed.draft_finding_ids.is_empty())
    {
        return Err("--finding/--all-findings are only supported by rr draft".to_owned());
    }

    if !matches!(parsed.command, CommandKind::Approve | CommandKind::Post)
        && parsed.batch_id.is_some()
    {
        return Err("--batch is only supported by rr approve and rr post".to_owned());
    }

    if parsed.command != CommandKind::Extension && parsed.extension_browser.is_some() {
        return Err("--browser is only supported by rr extension".to_owned());
    }

    if parsed.command != CommandKind::Agent
        && (parsed.agent_task_file.is_some()
            || parsed.agent_request_file.is_some()
            || parsed.agent_context_file.is_some()
            || parsed.agent_capability_file.is_some())
    {
        return Err(
            "--task-file/--request-file/--context-file/--capability-file are only supported by rr agent"
                .to_owned(),
        );
    }

    if parsed.command != CommandKind::Update
        && (parsed.update_channel != "stable"
            || parsed.update_version.is_some()
            || parsed.update_api_root.is_some()
            || parsed.update_download_root.is_some()
            || parsed.update_target.is_some()
            || parsed.update_yes)
    {
        return Err(
            "--channel/--version/--api-root/--download-root/--target/--yes are update-only flags"
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
                "rr agent only supports <operation> plus --task-file, --request-file, --context-file, and --capability-file"
                    .to_owned(),
            );
        }
    }

    Ok(parsed)
}

fn execute_command(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    match parsed.command {
        CommandKind::Agent => handle_agent(parsed, runtime),
        CommandKind::Review => handle_review(parsed, runtime),
        CommandKind::Resume => handle_resume(parsed, runtime),
        CommandKind::Return => handle_return(parsed, runtime),
        CommandKind::Sessions => handle_sessions(parsed, runtime),
        CommandKind::Search => handle_search(parsed, runtime),
        CommandKind::Draft => handle_draft(parsed, runtime),
        CommandKind::Approve => handle_approve(parsed, runtime),
        CommandKind::Update => handle_update(parsed, runtime),
        CommandKind::Bridge => handle_bridge(parsed, runtime),
        CommandKind::Extension => handle_extension(parsed, runtime),
        CommandKind::RobotDocs => handle_robot_docs(parsed),
        CommandKind::Findings => handle_findings(parsed, runtime),
        CommandKind::Status => handle_status(parsed, runtime),
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

fn synthesize_agent_context(
    session: &roger_storage::ReviewSessionRecord,
    task: &ReviewTask,
    unresolved_findings: Vec<WorkerFindingSummary>,
) -> WorkerContextPacket {
    WorkerContextPacket {
        review_target: session.review_target.clone(),
        review_session_id: task.review_session_id.clone(),
        review_run_id: task.review_run_id.clone(),
        review_task_id: task.id.clone(),
        task_nonce: task.task_nonce.clone(),
        baseline_snapshot_ref: None,
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
    }
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
        semantic_assets_verified: false,
    })
    .map_err(|err| format!("failed to plan rr agent search intent: {err}"))?;

    let repository = &session.review_target.repository;
    let scope_key = format!("repo:{repository}");
    let lookup = store
        .prior_review_lookup(PriorReviewLookupQuery {
            scope_key: &scope_key,
            repository,
            query_text: &search_request.query_text,
            limit: 25,
            include_tentative_candidates: search_plan.includes_tentative_candidates(),
            allow_project_scope: false,
            allow_org_scope: false,
            semantic_assets_verified: false,
            semantic_candidates: Vec::new(),
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

    let request_bytes = match read_json_bytes_from_stdin_or_file(
        parsed.agent_request_file.as_ref(),
        "rr agent request envelope",
    ) {
        Ok(bytes) => bytes,
        Err(message) => {
            return agent_error_response(AgentTransportErrorCode::PayloadMissing, message);
        }
    };
    let request: WorkerOperationRequestEnvelope = match serde_json::from_slice(&request_bytes) {
        Ok(request) => request,
        Err(err) => {
            return agent_error_response(
                AgentTransportErrorCode::PayloadInvalid,
                format!("failed to parse rr agent request envelope as JSON: {err}"),
            );
        }
    };

    let Some(expected_operation) = parsed.agent_operation.as_deref() else {
        return agent_error_response(
            AgentTransportErrorCode::ValidationFailed,
            "rr agent operation is missing",
        );
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
        None => synthesize_agent_context(&session, &task, findings.clone()),
    };
    let gateway_snapshot =
        match build_agent_gateway_snapshot(&store, &session, &task, &request, &findings) {
            Ok(snapshot) => snapshot,
            Err(message) => {
                return agent_error_response(AgentTransportErrorCode::ValidationFailed, message);
            }
        };

    agent_command_response(execute_agent_transport_request(
        &AgentTransportRequestEnvelope {
            schema_id: AGENT_TRANSPORT_REQUEST_SCHEMA_V1.to_owned(),
            review_task: task,
            worker_context,
            capability_profile,
            operation_request: request,
            gateway_snapshot,
        },
    ))
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

fn normalize_extension_id(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn discover_extension_id(
    parsed: &ParsedArgs,
    runtime: &CliRuntime,
) -> Option<(String, &'static str)> {
    if let Some(value) = parsed
        .bridge_extension_id
        .as_deref()
        .and_then(normalize_extension_id)
    {
        return Some((value, "explicit_flag"));
    }

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

fn extension_id_looks_valid(value: &str) -> bool {
    value.len() == 32 && value.chars().all(|ch| ch.is_ascii_lowercase())
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

fn extension_default_profile_root(browser: &SupportedBrowser) -> Option<PathBuf> {
    let host_os = SupportedOs::current()?;
    let home = std::env::var("HOME").ok().map(PathBuf::from);
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
    if let Ok(path) = std::env::var("RR_EXTENSION_PROFILE_ROOT") {
        if let Some(trimmed) = normalize_extension_id(&path) {
            roots.push(PathBuf::from(trimmed));
        }
    }
    roots.push(extension_guided_profile_root(runtime, browser));
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
    if let Some(discovered) = discover_extension_id(parsed, runtime) {
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

    None
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

fn extension_browser_url(browser: SupportedBrowser) -> &'static str {
    match browser {
        SupportedBrowser::Chrome => "chrome://extensions",
        SupportedBrowser::Edge => "edge://extensions",
        SupportedBrowser::Brave => "brave://extensions",
    }
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

fn handle_extension(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    let Some(subcommand) = parsed.extension_command else {
        return error_response("rr extension missing subcommand".to_owned());
    };

    let Some(workspace_root) = find_workspace_root(&runtime.cwd) else {
        return blocked_response(
            "failed to resolve Roger workspace root for extension setup commands".to_owned(),
            vec![
                "run rr extension from the Roger repository root (or a child directory)".to_owned(),
            ],
            json!({"reason_code": "workspace_root_not_found"}),
        );
    };

    match subcommand {
        ExtensionCommandKind::Setup => handle_extension_setup(parsed, runtime, &workspace_root),
        ExtensionCommandKind::Doctor => handle_extension_doctor(parsed, runtime, &workspace_root),
    }
}

fn handle_extension_setup(
    parsed: &ParsedArgs,
    runtime: &CliRuntime,
    workspace_root: &Path,
) -> CommandResponse {
    let browser = parsed
        .extension_browser
        .clone()
        .unwrap_or(SupportedBrowser::Chrome);

    let mut pack_parsed = parsed.clone();
    pack_parsed.command = CommandKind::Bridge;
    pack_parsed.bridge_command = Some(BridgeCommandKind::PackExtension);
    pack_parsed.bridge_extension_id = None;
    pack_parsed.bridge_binary_path = None;
    let pack = handle_bridge(&pack_parsed, runtime);
    if pack.outcome != OutcomeKind::Complete {
        return pack;
    }

    let package_dir = match pack
        .data
        .get("package_dir")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
    {
        Some(path) => path,
        None => {
            return error_response(
                "extension setup failed to resolve package path from pack-extension output"
                    .to_owned(),
            );
        }
    };

    let load_step = format!(
        "open {} and load unpacked extension from {}",
        extension_browser_url(browser.clone()),
        package_dir
    );
    let guided_profile_root = extension_guided_profile_root(runtime, &browser);
    let profile_hint_step =
        extension_profile_launch_hint(&browser, &guided_profile_root, &package_dir);

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
                "extension_id_registry_path": extension_id_registry_path(&runtime.store_root)
                    .to_string_lossy()
                    .to_string(),
                "guided_profile_root": guided_profile_root.to_string_lossy().to_string(),
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
                load_step,
                profile_hint_step,
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
        .or_else(|| std::env::var("HOME").ok().map(PathBuf::from));
    let Some(install_root) = install_root else {
        return blocked_response(
            "failed to determine install root; HOME is missing".to_owned(),
            vec!["pass --install-root <path> for recovery".to_owned()],
            json!({"reason_code": "install_root_missing"}),
        );
    };

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
    let manifest = NativeHostManifest::for_roger(&bridge_binary, &extension_id);
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

    CommandResponse {
        outcome: OutcomeKind::Complete,
        data: json!({
            "subcommand": "setup",
            "browser": supported_browser_label(browser.clone()),
            "package_dir": package_dir,
            "extension_id": extension_id,
            "extension_id_source": extension_id_source,
            "registration_wait_budget_ms": registration_wait_budget_ms,
            "registration_observed_during_setup_wait": observed_during_setup_wait,
            "install_root": install_root.to_string_lossy().to_string(),
            "host_binary": bridge_binary.to_string_lossy().to_string(),
            "native_manifest_path": manifest_path.to_string_lossy().to_string(),
            "doctor": doctor.data,
        }),
        warnings: Vec::new(),
        repair_actions: vec![format!(
            "rerun rr extension doctor --browser {} after browser or install changes",
            supported_browser_label(browser)
        )],
        message: "extension setup completed".to_owned(),
    }
}

fn handle_extension_doctor(
    parsed: &ParsedArgs,
    runtime: &CliRuntime,
    workspace_root: &Path,
) -> CommandResponse {
    let browser = parsed
        .extension_browser
        .clone()
        .unwrap_or(SupportedBrowser::Chrome);
    let browser_label = supported_browser_label(browser.clone());

    let package_dir = match resolve_extension_package_dir(workspace_root) {
        Ok(path) => path,
        Err(err) => return error_response(err),
    };
    let discovered_identity = discover_extension_id(parsed, runtime).or_else(|| {
        discover_extension_id_from_browser_profiles(&browser, runtime, &package_dir)
            .map(|value| (value, "browser_profile_preferences"))
    });
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
        .or_else(|| std::env::var("HOME").ok().map(PathBuf::from));
    let Some(install_root) = install_root else {
        return blocked_response(
            "failed to determine install root; HOME is missing".to_owned(),
            vec!["pass --install-root <path> for recovery".to_owned()],
            json!({"reason_code": "install_root_missing"}),
        );
    };

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
    if manifest_exists {
        if let Ok(text) = fs::read_to_string(&manifest_path) {
            if let Ok(manifest) = serde_json::from_str::<NativeHostManifest>(&text) {
                host_binary_exists = Path::new(&manifest.path).exists();
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
        "detail": manifest_path.to_string_lossy().to_string(),
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

        return CommandResponse {
            outcome: OutcomeKind::Blocked,
            data: json!({
                "subcommand": "doctor",
                "reason_code": reason_code,
                "browser": browser_label,
                "package_dir": package_dir.to_string_lossy().to_string(),
                "install_root": install_root.to_string_lossy().to_string(),
                "checks": checks,
            }),
            warnings: vec![warning],
            repair_actions,
            message: "extension doctor failed closed".to_owned(),
        };
    }

    CommandResponse {
        outcome: OutcomeKind::Complete,
        data: json!({
            "subcommand": "doctor",
            "browser": browser_label,
            "package_dir": package_dir.to_string_lossy().to_string(),
            "install_root": install_root.to_string_lossy().to_string(),
            "checks": checks,
        }),
        warnings: Vec::new(),
        repair_actions: Vec::new(),
        message: "extension doctor checks passed".to_owned(),
    }
}

fn handle_bridge(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    let Some(subcommand) = parsed.bridge_command else {
        return error_response("rr bridge missing subcommand".to_owned());
    };

    let Some(workspace_root) = find_workspace_root(&runtime.cwd) else {
        return blocked_response(
            "failed to resolve Roger workspace root for bridge contract commands".to_owned(),
            vec!["run rr bridge from the Roger repository root (or a child directory)".to_owned()],
            json!({"reason_code": "workspace_root_not_found"}),
        );
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
            if let Err(err) = copy_dir_recursive(&src_root, &package_dir.join("src")) {
                return error_response(format!("failed to copy extension src tree: {err}"));
            }
            if static_root.exists() {
                if let Err(err) = copy_dir_recursive(&static_root, &package_dir.join("static")) {
                    return error_response(format!("failed to copy extension static tree: {err}"));
                }
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
                let manifest = NativeHostManifest::for_roger(&bridge_binary, &extension_id);
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
                    "platform": host_os.as_str(),
                    "install_root": install_root.to_string_lossy().to_string(),
                    "removed": removed,
                    "missing": missing,
                    "installs_browser_extension": false,
                }),
                warnings: Vec::new(),
                repair_actions: Vec::new(),
                message: "bridge registration assets removed".to_owned(),
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

fn handle_review(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    if !SUPPORTED_REVIEW_PROVIDERS.contains(&parsed.provider.as_str()) {
        return blocked_response(
            format!(
                "provider '{}' is not supported for rr review in this slice",
                parsed.provider
            ),
            vec![
                "use --provider opencode for tier-b CLI continuity in 0.1.0".to_owned(),
                "use --provider codex for bounded tier-a start/reseed support".to_owned(),
                "use --provider gemini for bounded tier-a start/reseed support".to_owned(),
                "use --provider claude for bounded tier-a start/reseed support".to_owned(),
            ],
            json!({
                "provider": parsed.provider,
                "supported_providers": SUPPORTED_REVIEW_PROVIDERS,
                "planned_not_live_providers": PLANNED_REVIEW_PROVIDERS,
                "not_supported_providers": NOT_LIVE_REVIEW_PROVIDERS,
                "live_review_provider_support": review_provider_support_matrix(),
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
                "provider_capability": provider_capability(&parsed.provider),
            }),
            warnings: provider_support_warning(&parsed.provider, "rr review")
                .into_iter()
                .collect(),
            repair_actions: Vec::new(),
            message: "review launch plan generated (dry-run)".to_owned(),
        };
    }

    let store = match RogerStore::open(&runtime.store_root) {
        Ok(store) => store,
        Err(err) => return error_response(format!("failed to open Roger store: {err}")),
    };

    let attempt_id = next_id("attempt");
    if let Err(err) = store.create_launch_attempt(CreateLaunchAttempt {
        id: &attempt_id,
        action: LaunchAttemptAction::StartReview,
        provider: &parsed.provider,
        source_surface: LaunchSurface::Cli,
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
    let (session_locator, session_path, continuity_quality, warnings) =
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

    let session_id = next_id("session");
    let run_id = next_id("run");
    let bundle_id = next_id("bundle");
    let binding_id = next_id("binding");

    let bundle = build_resume_bundle(
        ResumeBundleProfile::ReseedResume,
        target.clone(),
        intent,
        parsed.provider.clone(),
        continuity_quality.clone(),
        "review launched via rr review",
    );

    let bundle_payload = match serde_json::to_vec(&bundle) {
        Ok(payload) => payload,
        Err(err) => {
            let detail = format!("failed to serialize ResumeBundle: {err}");
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
            surface: LaunchSurface::Cli,
            launch_profile_id: Some(cli_config::PROFILE_ID),
            ui_target: Some(cli_config::UI_TARGET),
            instance_preference: Some(cli_config::INSTANCE_PREFERENCE),
            cwd: Some(binding_context.cwd.as_str()),
            worktree_root: binding_context.worktree_root.as_deref(),
        },
    }) {
        let detail = format!("failed to finalize review launch: {err}");
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

    CommandResponse {
        outcome,
        data: json!({
            "launch_attempt_id": attempt_id,
            "session_id": session_id,
            "review_run_id": run_id,
            "resume_bundle_artifact_id": bundle_artifact_id,
            "repository": target.repository,
            "pull_request": target.pull_request_number,
            "provider": parsed.provider,
            "session_path": session_path,
            "continuity_quality": continuity_state_label(&continuity_quality),
        }),
        warnings,
        repair_actions: Vec::new(),
        message: "review session launched".to_owned(),
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
    let store = match RogerStore::open(&runtime.store_root) {
        Ok(store) => store,
        Err(err) => return error_response(format!("failed to open Roger store: {err}")),
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
                LaunchSurface::Cli,
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

    if !SUPPORTED_REVIEW_PROVIDERS.contains(&session.provider.as_str()) {
        return blocked_response(
            format!(
                "session {} uses provider '{}' which cannot be resumed by this CLI slice",
                session.id, session.provider
            ),
            vec![
                "resume is currently available for opencode, codex, gemini, and claude sessions"
                    .to_owned(),
            ],
            json!({
                "session_id": session.id,
                "provider": session.provider,
                "supported_providers": SUPPORTED_REVIEW_PROVIDERS,
                "planned_not_live_providers": PLANNED_REVIEW_PROVIDERS,
                "not_supported_providers": NOT_LIVE_REVIEW_PROVIDERS,
                "live_review_provider_support": review_provider_support_matrix(),
            }),
        );
    }

    let command_name = "rr resume";

    if parsed.robot {
        let continuity_state = session.continuity_state.to_ascii_lowercase();
        let provider_is_bounded = session.provider != "opencode";
        let degraded = provider_is_bounded
            || continuity_state.contains("degraded")
            || continuity_state.contains("reseed")
            || continuity_state.contains("unusable");
        let continuity_quality = if continuity_state.contains("unusable") {
            "unusable"
        } else if degraded {
            "degraded"
        } else {
            "usable"
        };
        let inferred_resume_path = if session.provider == "codex"
            || session.provider == "claude"
            || session.provider == "gemini"
            || continuity_state.contains("reseed")
        {
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
                "provider_capability": provider_capability(&session.provider),
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
                "provider_capability": provider_capability(&session.provider),
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

    let intent = launch_intent(LaunchAction::ResumeReview, runtime);

    let resume_bundle = match session.resume_bundle_artifact_id.as_deref() {
        Some(id) => match store.load_resume_bundle(id) {
            Ok(bundle) => Some(bundle),
            Err(err) => {
                return blocked_response(
                    format!("resume bundle could not be loaded: {err}"),
                    vec!["re-run rr review to regenerate ResumeBundle".to_owned()],
                    json!({"reason_code": "resume_bundle_missing_or_invalid", "session_id": session.id}),
                );
            }
        },
        None => None,
    };

    let (resume_path, continuity_quality, decision_reason, mut warnings) = match session
        .provider
        .as_str()
    {
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
                    return blocked_response(
                        format!("resume failed: {err}"),
                        vec![
                            "ensure a valid ResumeBundle exists or launch a new review with rr review"
                                .to_owned(),
                        ],
                        json!({
                            "reason_code": "resume_failed_closed",
                            "session_id": session.id,
                            "error": err.to_string(),
                        }),
                    );
                }
            };
            (
                session_path_label(&linkage.path).to_owned(),
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
                    return blocked_response(
                        format!("resume failed: {err}"),
                        vec![
                            "ensure a valid ResumeBundle exists or launch a new review with rr review --provider codex"
                                .to_owned(),
                        ],
                        json!({
                            "reason_code": "resume_failed_closed",
                            "session_id": session.id,
                            "error": err.to_string(),
                        }),
                    );
                }
            };
            (
                codex_session_path_label(&linkage.path).to_owned(),
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
                    return blocked_response(
                        format!("resume failed: {err}"),
                        vec![
                            "ensure a valid ResumeBundle exists or launch a new review with rr review --provider claude"
                                .to_owned(),
                        ],
                        json!({
                            "reason_code": "resume_failed_closed",
                            "session_id": session.id,
                            "error": err.to_string(),
                        }),
                    );
                }
            };
            (
                claude_session_path_label(&linkage.path).to_owned(),
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
                    return blocked_response(
                        format!("resume failed: {err}"),
                        vec![
                            "ensure a valid ResumeBundle exists or launch a new review with rr review --provider gemini"
                                .to_owned(),
                        ],
                        json!({
                            "reason_code": "resume_failed_closed",
                            "session_id": session.id,
                            "error": err.to_string(),
                        }),
                    );
                }
            };
            (
                gemini_session_path_label(&linkage.path).to_owned(),
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
        _ => unreachable!("provider validated above"),
    };
    if let Some(warning) = inferred_selection_warning {
        warnings.insert(0, warning);
    }

    let run_kind = "resume";
    let run_id = next_id("run");

    if let Err(err) = store.create_review_run(CreateReviewRun {
        id: &run_id,
        session_id: &session.id,
        run_kind,
        repo_snapshot: &format!(
            "{}#{}",
            session.review_target.repository, session.review_target.pull_request_number
        ),
        continuity_quality: continuity_state_label(&continuity_quality),
        session_locator_artifact_id: None,
    }) {
        return error_response(format!("failed to create {run_kind} run: {err}"));
    }

    let continuity_state = format!(
        "{}:{}",
        run_kind,
        continuity_state_label(&continuity_quality)
    );
    let updated_session = match store.update_review_session_continuity(
        &session.id,
        session.row_version,
        &continuity_state,
    ) {
        Ok(session) => session,
        Err(err) => {
            return error_response(format!("failed to update session continuity: {err}"));
        }
    };

    if let Err(err) = store.update_review_session_attention(
        &updated_session.id,
        updated_session.row_version,
        if session.attention_state == "refresh_recommended" {
            "refresh_recommended"
        } else {
            "review_resumed"
        },
    ) {
        return error_response(format!("failed to update session attention state: {err}"));
    }

    let binding_id = binding
        .map(|record| record.id)
        .unwrap_or_else(|| next_id("binding"));
    if let Err(err) = store.put_session_launch_binding(CreateSessionLaunchBinding {
        id: &binding_id,
        session_id: &session.id,
        repo_locator: &session.review_target.repository,
        review_target: Some(&session.review_target),
        surface: LaunchSurface::Cli,
        launch_profile_id: Some(cli_config::PROFILE_ID),
        ui_target: Some(cli_config::UI_TARGET),
        instance_preference: Some(cli_config::INSTANCE_PREFERENCE),
        cwd: Some(runtime.cwd.to_string_lossy().as_ref()),
        worktree_root: None,
    }) {
        return error_response(format!("failed to persist launch binding: {err}"));
    }

    let degraded = !matches!(continuity_quality, ContinuityQuality::Usable)
        || resume_path == "reseeded_from_bundle";

    CommandResponse {
        outcome: if degraded {
            OutcomeKind::Degraded
        } else {
            OutcomeKind::Complete
        },
        data: json!({
            "session_id": session.id,
            "review_run_id": run_id,
            "repository": session.review_target.repository,
            "pull_request": session.review_target.pull_request_number,
            "provider": session.provider,
            "resume_path": resume_path,
            "continuity_quality": continuity_state_label(&continuity_quality),
            "decision_reason": decision_reason,
        }),
        warnings,
        repair_actions: Vec::new(),
        message: format!("{run_kind} completed"),
    }
}

fn handle_return(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    let store = match RogerStore::open(&runtime.store_root) {
        Ok(store) => store,
        Err(err) => return error_response(format!("failed to open Roger store: {err}")),
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

    if session.provider != "opencode" {
        let mut capability = provider_capability(&session.provider);
        capability["required_tier_for_return"] = json!("tier_b");
        capability["supports_rr_return"] = json!(false);
        return blocked_response(
            format!(
                "rr return is unsupported for provider '{}' in 0.1.0",
                session.provider
            ),
            vec!["rr return is only blessed on OpenCode tier-b sessions".to_owned()],
            json!({
                "session_id": session.id,
                "provider": session.provider,
                "provider_capability": capability,
            }),
        );
    }

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
            surface: LaunchSurface::Cli,
            repo_locator: &session.review_target.repository,
            review_target: Some(&session.review_target),
            ui_target: Some(cli_config::UI_TARGET),
            instance_preference: Some(cli_config::INSTANCE_PREFERENCE),
        },
        reopen_outcome,
    ) {
        Ok(outcome) => outcome,
        Err(err) => {
            return blocked_response(
                format!("rr return failed: {err}"),
                vec!["ensure a valid binding and ResumeBundle exist for this repo".to_owned()],
                json!({"reason_code": "rr_return_failed", "session_id": session.id}),
            );
        }
    };

    let run_id = next_id("run");
    if let Err(err) = store.create_review_run(CreateReviewRun {
        id: &run_id,
        session_id: &outcome.session_id,
        run_kind: "return",
        repo_snapshot: &format!(
            "{}#{}",
            session.review_target.repository, session.review_target.pull_request_number
        ),
        continuity_quality: continuity_state_label(&outcome.continuity_quality),
        session_locator_artifact_id: None,
    }) {
        return error_response(format!("failed to record return run: {err}"));
    }

    let continuity_state = format!(
        "return:{}",
        continuity_state_label(&outcome.continuity_quality)
    );
    let updated = match store.update_review_session_continuity(
        &session.id,
        session.row_version,
        &continuity_state,
    ) {
        Ok(updated) => updated,
        Err(err) => return error_response(format!("failed to update session continuity: {err}")),
    };

    if let Err(err) =
        store.update_review_session_attention(&updated.id, updated.row_version, "returned_to_roger")
    {
        return error_response(format!("failed to update session attention: {err}"));
    }

    let binding_id = binding
        .map(|record| record.id)
        .unwrap_or_else(|| next_id("binding"));
    if let Err(err) = store.put_session_launch_binding(CreateSessionLaunchBinding {
        id: &binding_id,
        session_id: &session.id,
        repo_locator: &session.review_target.repository,
        review_target: Some(&session.review_target),
        surface: LaunchSurface::Cli,
        launch_profile_id: Some(cli_config::PROFILE_ID),
        ui_target: Some(cli_config::UI_TARGET),
        instance_preference: Some(cli_config::INSTANCE_PREFERENCE),
        cwd: Some(runtime.cwd.to_string_lossy().as_ref()),
        worktree_root: None,
    }) {
        return error_response(format!("failed to update launch binding: {err}"));
    }

    let degraded = !matches!(outcome.continuity_quality, ContinuityQuality::Usable)
        || matches!(outcome.path, OpenCodeReturnPath::ReseededSession);

    CommandResponse {
        outcome: if degraded {
            OutcomeKind::Degraded
        } else {
            OutcomeKind::Complete
        },
        data: {
            let mut capability = provider_capability(&session.provider);
            capability["supports_rr_return"] = json!(true);
            capability["required_tier_for_return"] = json!("tier_b");
            json!({
            "session_id": outcome.session_id,
            "review_run_id": run_id,
            "provider_capability": capability,
            "return_path": return_path_label(outcome.path),
            "continuity_quality": continuity_state_label(&outcome.continuity_quality),
            "decision_reason": format!("{:?}", outcome.decision.reason_code),
        })
        },
        warnings: Vec::new(),
        repair_actions: Vec::new(),
        message: "rr return completed".to_owned(),
    }
}

fn handle_sessions(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    let store = match RogerStore::open(&runtime.store_root) {
        Ok(store) => store,
        Err(err) => return error_response(format!("failed to open Roger store: {err}")),
    };

    let limit = parsed.limit.unwrap_or(25).min(250);
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
                    "reconciliation_mode": "automatic_background",
                    "fractional_staleness_allowed": true,
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

    let store = match RogerStore::open(&runtime.store_root) {
        Ok(store) => store,
        Err(err) => return error_response(format!("failed to open Roger store: {err}")),
    };

    let limit = parsed.limit.unwrap_or(10).min(100);
    let granted_scopes = vec!["repo".to_owned()];
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
        supports_promotion_review: false,
        semantic_assets_verified: false,
    }) {
        Ok(plan) => plan,
        Err(err) => {
            let repair_actions = match &err {
                SearchPlanError::QueryPlanning(SearchQueryPlanError::MissingSearchInputs) => {
                    vec!["pass --query \"<search text>\"".to_owned()]
                }
                SearchPlanError::QueryPlanning(SearchQueryPlanError::UnsupportedQueryMode { .. }) => vec![
                    "pass --query-mode auto, exact_lookup, recall, related_context, candidate_audit, or promotion_review".to_owned(),
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
    let lookup = match store.prior_review_lookup(PriorReviewLookupQuery {
        scope_key: &scope_key,
        repository: &repository,
        query_text,
        limit: limit.saturating_add(1),
        include_tentative_candidates: search_plan.includes_tentative_candidates(),
        allow_project_scope: false,
        allow_org_scope: false,
        semantic_assets_verified: false,
        semantic_candidates: Vec::new(),
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
    let degraded =
        mode == "recovery_scan" || (mode == "lexical_only" && !degraded_reasons.is_empty());
    let outcome = if degraded {
        OutcomeKind::Degraded
    } else if count == 0 {
        OutcomeKind::Empty
    } else {
        OutcomeKind::Complete
    };

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
            "scope_bucket": scope_bucket,
            "lane_counts": lane_counts,
        }),
        warnings: Vec::new(),
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
        if candidate_name == archive_name {
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
        store_schema_version: 10,
        min_supported_store_schema: 0,
        auto_migrate_from: 0,
        migration_policy: "binary_only".to_owned(),
        migration_class_max_auto: "none".to_owned(),
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

fn assess_migration_preflight(
    current_store_schema: i64,
    published: &StoreCompatibilityEnvelope,
    embedded_matches_published: bool,
) -> MigrationPreflight {
    if !embedded_matches_published {
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
            if current_store_schema >= published.auto_migrate_from {
                let classification = match published.migration_class_max_auto.as_str() {
                    "class_a" => "class_a",
                    "class_b" => "class_b",
                    _ => {
                        return MigrationPreflight {
                            status: "migration_requires_explicit_operator_gate",
                            classification: "class_c",
                            apply_allowed: false,
                            blocked_reason: Some(
                                "auto_safe_policy_missing_auto_migration_class".to_owned(),
                            ),
                        };
                    }
                };
                MigrationPreflight {
                    status: "auto_safe_migration_after_update",
                    classification,
                    apply_allowed: true,
                    blocked_reason: None,
                }
            } else {
                MigrationPreflight {
                    status: "migration_requires_explicit_operator_gate",
                    classification: "class_c",
                    apply_allowed: false,
                    blocked_reason: Some(
                        "local_store_schema_outside_auto_migrate_window".to_owned(),
                    ),
                }
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
    let embedded_matches_published = embedded == published;
    let preflight =
        assess_migration_preflight(current_store_schema, published, embedded_matches_published);

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
        "embedded_envelope_matches_metadata": embedded_matches_published,
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
        "status": "deferred_in_0_1_x",
        "guidance": "if a future release requires local-state/schema migration, fail closed and use explicit backup/export + reinstall guidance",
    })
}

fn handle_update(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    let Some(current_version) = option_env!("ROGER_RELEASE_VERSION").map(str::to_owned) else {
        return blocked_response(
            "rr update is disabled for local/unpublished builds without embedded release metadata"
                .to_owned(),
            vec![
                "install a published Roger release artifact before running rr update".to_owned(),
                "or run scripts/release/rr-install.sh directly with an explicit --version"
                    .to_owned(),
            ],
            json!({
                "reason_code": "local_or_unpublished_build",
                "migration": migration_policy_payload(),
            }),
        );
    };
    let current_channel = option_env!("ROGER_RELEASE_CHANNEL")
        .map(str::to_owned)
        .unwrap_or_else(|| "unknown".to_owned());
    let current_tag = option_env!("ROGER_RELEASE_TAG")
        .map(str::to_owned)
        .unwrap_or_else(|| format!("v{current_version}"));

    let repo = parsed
        .repo
        .clone()
        .unwrap_or_else(|| "cdilga/roger-reviewer".to_owned());
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

    let checksums_url = format!("{download_root}/{target_tag}/{checksums_name}");
    let checksums_text = match fetch_url_with_curl(&checksums_url) {
        Ok(text) => text,
        Err(err) => {
            return blocked_response(
                format!("failed to fetch checksums: {err}"),
                vec!["rebuild/upload checksums for this tag".to_owned()],
                json!({"reason_code": "checksums_missing", "url": checksums_url}),
            );
        }
    };
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
                "target_version": target_version,
                "target_tag": target_tag,
                "target": target,
                "up_to_date": true,
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
    let recommended_command = if cfg!(target_os = "windows") {
        format!(
            "powershell -ExecutionPolicy Bypass -File scripts/release/rr-install.ps1 -Version {target_version} -Repo {repo}"
        )
    } else {
        format!("bash scripts/release/rr-install.sh --version {target_version} --repo {repo}")
    };

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
        return blocked_response(
            format!("rr update apply blocked by migration posture: {blocked_reason}"),
            vec![
                "run rr update --dry-run --robot to inspect migration posture details".to_owned(),
                "apply is allowed only when migration.apply_allowed=true".to_owned(),
            ],
            json!({
                "reason_code": "migration_preflight_blocked",
                "target_version": target_version,
                "target_tag": target_tag,
                "target": target,
                "migration": migration_policy.clone(),
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
            return blocked_response(
                format!("failed to resolve current executable path: {err}"),
                vec!["run scripts/release/rr-install.sh with an explicit --version".to_owned()],
                json!({"reason_code": "current_exe_resolution_failed"}),
            );
        }
    };
    let install_path = match resolve_update_install_path(&current_exe, &binary_name) {
        Ok(path) => path,
        Err(err) => {
            return blocked_response(
                err,
                vec![
                    "install Roger to a direct rr binary path before running rr update".to_owned(),
                    "or run scripts/release/rr-install.sh with an explicit --version".to_owned(),
                ],
                json!({"reason_code": "unsupported_install_layout"}),
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
            return blocked_response(
                format!("failed to apply in-place update: {err}"),
                vec![
                    "re-run rr update after resolving install path and permissions".to_owned(),
                    format!("or run {recommended_command}"),
                ],
                json!({
                    "reason_code": "in_place_apply_failed",
                    "install_path": install_path.to_string_lossy(),
                }),
            );
        }
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
            "recommended_install_command": recommended_command,
        }),
        warnings: Vec::new(),
        repair_actions: Vec::new(),
        message: format!("rr updated from {} to {}", current_version, target_version),
    }
}

fn handle_robot_docs(parsed: &ParsedArgs) -> CommandResponse {
    let topic = parsed.robot_docs_topic.as_deref().unwrap_or("guide");

    let (items, version) = match topic {
        "guide" => (
            vec![
                json!({"command": "rr status --robot", "purpose": "session attention snapshot"}),
                json!({"command": "rr sessions --robot", "purpose": "global session finder"}),
                json!({"command": "rr findings --robot", "purpose": "structured findings list"}),
                json!({"command": "rr search --query <text> --query-mode recall --robot", "purpose": "prior-review lookup"}),
                json!({"command": "rr draft --session <id> --finding <finding-id> --robot", "purpose": "materialize local outbound drafts bound to the current review target without posting to GitHub"}),
                json!({"command": "rr approve --session <id> --batch <draft-batch-id> --robot", "purpose": "record an explicit local approval token bound to one exact batch payload and target without posting to GitHub"}),
                json!({
                    "kind": "provider_support",
                    "command": "rr review --provider <name>",
                    "summary": "OpenCode is the only live tier-b continuity path in 0.1.0. Codex, Gemini, and Claude Code are exposed as bounded tier-a start/reseed/raw-capture providers only. Copilot is planned but not yet live, and Pi-Agent remains out of scope for 0.1.0.",
                    "live_review_providers": review_provider_support_matrix(),
                    "planned_not_live_providers": PLANNED_REVIEW_PROVIDERS,
                    "not_supported_providers": NOT_LIVE_REVIEW_PROVIDERS,
                }),
                json!({"command": "rr update --channel stable --dry-run --robot", "purpose": "update metadata preflight (non-mutating)"}),
                json!({"command": "rr update --channel stable --yes --robot", "purpose": "non-interactive in-place apply after explicit confirmation bypass"}),
                json!({"command": "rr bridge verify-contracts --robot", "purpose": "bridge contract drift check"}),
                json!({"command": "rr bridge pack-extension --robot", "purpose": "assemble unpacked browser sideload artifact"}),
                json!({"command": "rr extension setup --browser <edge|chrome|brave> --robot", "purpose": "guided package/setup flow with fail-closed identity + host checks"}),
                json!({"command": "rr extension doctor --browser <edge|chrome|brave> --robot", "purpose": "verify package, identity, native host registration, and bridge reachability"}),
                json!({"command": "rr bridge install [--extension-id <id>] --robot", "purpose": "repair/dev host registration override when guided setup cannot discover identity"}),
                json!({"command": "rr bridge uninstall --robot", "purpose": "remove bridge registration assets"}),
                json!({"command": "rr robot-docs schemas --robot", "purpose": "schema inventory"}),
                json!({"kind": "reconciliation_contract", "mode": "automatic_background", "summary": "Roger reconciles stale review state automatically during ordinary review, resume, return, status, findings, TUI, and extension flows; bounded fractional staleness is allowed while background reconciliation catches up."}),
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
                json!({"command": "rr status", "required_formats": ["json"], "optional_formats": ["compact"]}),
                json!({"command": "rr sessions", "required_formats": ["json"], "optional_formats": ["compact"]}),
                json!({"command": "rr findings", "required_formats": ["json"], "optional_formats": ["compact"]}),
                json!({"command": "rr search", "required_formats": ["json"], "optional_formats": ["compact"]}),
                json!({"command": "rr draft", "required_formats": ["json"], "optional_formats": []}),
                json!({"command": "rr approve", "required_formats": ["json"], "optional_formats": []}),
                json!({"command": "rr update", "required_formats": ["json"], "optional_formats": []}),
                json!({
                    "command": "rr review --dry-run",
                    "required_formats": ["json"],
                    "optional_formats": [],
                    "supported_providers": SUPPORTED_REVIEW_PROVIDERS,
                    "planned_not_live_providers": PLANNED_REVIEW_PROVIDERS,
                    "not_supported_providers": NOT_LIVE_REVIEW_PROVIDERS,
                }),
                json!({"command": "rr resume --dry-run", "required_formats": ["json"], "optional_formats": []}),
                json!({"command": "rr bridge export-contracts", "required_formats": ["json"], "optional_formats": []}),
                json!({"command": "rr bridge verify-contracts", "required_formats": ["json"], "optional_formats": []}),
                json!({"command": "rr bridge pack-extension", "required_formats": ["json"], "optional_formats": []}),
                json!({"command": "rr extension setup", "required_formats": ["json"], "optional_formats": []}),
                json!({"command": "rr extension doctor", "required_formats": ["json"], "optional_formats": []}),
                json!({"command": "rr bridge install", "required_formats": ["json"], "optional_formats": []}),
                json!({"command": "rr bridge uninstall", "required_formats": ["json"], "optional_formats": []}),
                json!({"command": "rr robot-docs guide", "required_formats": ["json"], "optional_formats": ["compact"]}),
                json!({"command": "rr robot-docs commands", "required_formats": ["json"], "optional_formats": ["compact"]}),
                json!({"command": "rr robot-docs schemas", "required_formats": ["json"], "optional_formats": ["compact"]}),
                json!({"command": "rr robot-docs workflows", "required_formats": ["json"], "optional_formats": ["compact"]}),
            ],
            "0.1.0",
        ),
        "schemas" => (
            vec![
                json!({"command": "rr review", "schema_id": "rr.robot.review.v1"}),
                json!({"command": "rr resume", "schema_id": "rr.robot.resume.v1"}),
                json!({"command": "rr return", "schema_id": "rr.robot.return.v1"}),
                json!({"command": "rr sessions", "schema_id": "rr.robot.sessions.v1"}),
                json!({"command": "rr search", "schema_id": "rr.robot.search.v1"}),
                json!({"command": "rr draft", "schema_id": "rr.robot.draft.v1"}),
                json!({"command": "rr approve", "schema_id": "rr.robot.approve.v1"}),
                json!({"command": "rr update", "schema_id": "rr.robot.update.v1"}),
                json!({"command": "rr bridge", "schema_id": "rr.robot.bridge.v1"}),
                json!({"command": "rr extension", "schema_id": "rr.robot.extension.v1"}),
                json!({"command": "rr findings", "schema_id": "rr.robot.findings.v1"}),
                json!({"command": "rr status", "schema_id": "rr.robot.status.v1"}),
                json!({"command": "rr robot-docs", "schema_id": "rr.robot.robot_docs.v1"}),
                json!({"command": "rr agent", "schema_id": AGENT_TRANSPORT_RESPONSE_SCHEMA_V1, "surface": "dedicated_worker_transport"}),
            ],
            "0.1.0",
        ),
        "workflows" => (
            vec![
                json!({"name": "resume_loop", "steps": ["rr sessions --robot", "rr resume --session <id> --robot", "rr findings --session <id> --robot"], "notes": "There is no standalone refresh action. Readback surfaces expose persisted attention state and repair guidance; re-entry surfaces remain the place where Roger can safely reconcile stale review context."}),
                json!({"name": "search_followup", "steps": ["rr search --query <text> --query-mode recall --robot", "rr status --session <id> --robot"]}),
                json!({"name": "local_outbound_draft", "steps": ["rr findings --session <id> --robot", "rr draft --session <id> --finding <finding-id> [--finding <finding-id>] --robot", "rr status --session <id> --robot"], "notes": "rr draft materializes local Roger-owned draft batches only. It does not approve or post anything to GitHub, and it fails closed if the session target or persisted review state is stale."}),
                json!({"name": "local_outbound_approve", "steps": ["rr findings --session <id> --robot", "rr draft --session <id> --finding <finding-id> [--finding <finding-id>] --robot", "rr approve --session <id> --batch <draft-batch-id> --robot", "rr status --session <id> --robot"], "notes": "rr approve records a local approval token for one exact stored batch payload and target tuple. It remains local-only and blocks when drift or invalidation revoked approval eligibility."}),
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

fn handle_draft(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    #[derive(Clone, Serialize)]
    struct DraftPreviewDescriptor {
        finding_id: String,
        fingerprint: String,
        title: String,
        normalized_summary: String,
        severity: String,
        confidence: String,
        anchor_digest: String,
        target_locator: String,
        body: String,
    }

    let store = match RogerStore::open(&runtime.store_root) {
        Ok(store) => store,
        Err(err) => return error_response(format!("failed to open Roger store: {err}")),
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

    let (session, _binding) = match resolution {
        SessionReentryResolution::Resolved { session, binding } => (session, binding),
        SessionReentryResolution::PickerRequired { reason, candidates } => {
            return blocked_picker_response(reason, candidates);
        }
    };

    if session.attention_state == "refresh_recommended" {
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
        return blocked_response(
            "rr draft is blocked because the persisted review state requires explicit reconciliation before outbound material can be derived".to_owned(),
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
        );
    }

    if session.review_target.repository.trim().is_empty()
        || session.review_target.pull_request_number == 0
    {
        return blocked_response(
            "rr draft requires a concrete review target before Roger can bind local outbound state"
                .to_owned(),
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
        );
    }

    let Some(run) = (match store.latest_review_run(&session.id) {
        Ok(run) => run,
        Err(err) => return error_response(format!("failed to load latest run: {err}")),
    }) else {
        return blocked_response(
            "rr draft requires persisted local review state for the selected target".to_owned(),
            vec![format!(
                "run rr review --repo {} --pr {} to materialize a local review first",
                session.review_target.repository, session.review_target.pull_request_number
            )],
            json!({
                "reason_code": "missing_local_state",
                "session_id": session.id,
            }),
        );
    };

    let findings = match store.materialized_findings_for_run(&session.id, &run.id) {
        Ok(findings) => findings,
        Err(err) => {
            return error_response(format!("failed to load findings for latest run: {err}"));
        }
    };

    if findings.is_empty() {
        return blocked_response(
            "rr draft requires persisted findings from the latest local review run".to_owned(),
            vec![format!(
                "run rr review --repo {} --pr {} to materialize findings before drafting",
                session.review_target.repository, session.review_target.pull_request_number
            )],
            json!({
                "reason_code": "missing_local_state",
                "session_id": session.id,
                "review_run_id": run.id,
            }),
        );
    }

    if !parsed.draft_all_findings && parsed.draft_finding_ids.is_empty() {
        return blocked_response(
            "rr draft requires explicit finding selection in this slice".to_owned(),
            vec![
                "pass --finding <id> one or more times".to_owned(),
                "or pass --all-findings to group every finding in the latest run".to_owned(),
            ],
            json!({
                "reason_code": "finding_selection_required",
                "session_id": session.id,
                "review_run_id": run.id,
                "available_finding_ids": findings
                    .iter()
                    .map(|finding| finding.id.clone())
                    .collect::<Vec<_>>(),
            }),
        );
    }

    let findings_by_id = findings
        .iter()
        .map(|finding| (finding.id.as_str(), finding))
        .collect::<HashMap<_, _>>();
    let selection_mode = if parsed.draft_all_findings {
        "all_findings"
    } else {
        "explicit_findings"
    };

    let mut selected_findings = if parsed.draft_all_findings {
        findings.clone()
    } else {
        let mut selected = Vec::new();
        let mut missing = Vec::new();
        for finding_id in &parsed.draft_finding_ids {
            match findings_by_id.get(finding_id.as_str()) {
                Some(finding) => selected.push((*finding).clone()),
                None => missing.push(finding_id.clone()),
            }
        }
        if !missing.is_empty() {
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
                    "review_run_id": run.id,
                    "missing_finding_ids": missing,
                }),
            );
        }
        selected
    };
    selected_findings.sort_by(|left, right| left.id.cmp(&right.id));
    selected_findings.dedup_by(|left, right| left.id == right.id);

    let mut selection_issues = Vec::new();
    for finding in &selected_findings {
        let projection = match store
            .outbound_surface_projection_for_finding(&finding.id, &finding.outbound_state)
        {
            Ok(projection) => projection,
            Err(err) => {
                return error_response(format!(
                    "failed to inspect outbound state for finding {}: {err}",
                    finding.id
                ));
            }
        };

        if !finding.triage_state.eq_ignore_ascii_case("accepted") {
            selection_issues.push(json!({
                "finding_id": finding.id.clone(),
                "reason_code": "triage_state_not_accepted",
                "triage_state": finding.triage_state.clone(),
            }));
        }
        if projection.state != "not_drafted" {
            selection_issues.push(json!({
                "finding_id": finding.id.clone(),
                "reason_code": "existing_outbound_state",
                "current_outbound_state": projection.state,
                "draft_id": projection.draft_id,
                "draft_batch_id": projection.draft_batch_id,
                "approval_id": projection.approval_id,
                "posted_action_id": projection.posted_action_id,
            }));
        }
    }

    if !selection_issues.is_empty() {
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
                "review_run_id": run.id,
                "selection_issues": selection_issues,
            }),
        );
    }

    let repo_id = session.review_target.repository.clone();
    let remote_review_target_id = format!("pr-{}", session.review_target.pull_request_number);
    let mut draft_previews = Vec::with_capacity(selected_findings.len());
    for finding in &selected_findings {
        let body = render_local_outbound_draft_body(finding);
        let anchor_digest = match outbound_draft_anchor_digest(finding) {
            Ok(digest) => digest,
            Err(err) => {
                return error_response(format!(
                    "failed to build draft anchor digest for finding {}: {err}",
                    finding.id
                ));
            }
        };
        draft_previews.push(DraftPreviewDescriptor {
            finding_id: finding.id.clone(),
            fingerprint: finding.fingerprint.clone(),
            title: finding.title.clone(),
            normalized_summary: finding.normalized_summary.clone(),
            severity: finding.severity.clone(),
            confidence: finding.confidence.clone(),
            anchor_digest,
            target_locator: outbound_draft_target_locator(&session.review_target, &finding.id),
            body,
        });
    }

    let payload_digest = match sha256_prefixed_json(&json!({
        "repo_id": repo_id.clone(),
        "remote_review_target_id": remote_review_target_id.clone(),
        "drafts": draft_previews.clone(),
    })) {
        Ok(digest) => digest,
        Err(err) => return error_response(format!("failed to build batch payload digest: {err}")),
    };

    let batch = OutboundDraftBatch {
        id: next_id("draft-batch"),
        review_session_id: session.id.clone(),
        review_run_id: run.id.clone(),
        repo_id: repo_id.clone(),
        remote_review_target_id: remote_review_target_id.clone(),
        payload_digest: payload_digest.clone(),
        approval_state: ApprovalState::Drafted,
        approved_at: None,
        invalidated_at: None,
        invalidation_reason_code: None,
        row_version: 0,
    };

    if let Err(err) = store.store_outbound_draft_batch(&batch) {
        return error_response(format!("failed to store outbound draft batch: {err}"));
    }

    let mut stored_drafts = Vec::with_capacity(draft_previews.len());
    for preview in &draft_previews {
        let draft = OutboundDraft {
            id: next_id("draft"),
            review_session_id: session.id.clone(),
            review_run_id: run.id.clone(),
            finding_id: Some(preview.finding_id.clone()),
            draft_batch_id: batch.id.clone(),
            repo_id: repo_id.clone(),
            remote_review_target_id: remote_review_target_id.clone(),
            payload_digest: payload_digest.clone(),
            approval_state: ApprovalState::Drafted,
            anchor_digest: preview.anchor_digest.clone(),
            target_locator: preview.target_locator.clone(),
            body: preview.body.clone(),
            row_version: 0,
        };
        if let Err(err) = store.store_outbound_draft_item(&draft) {
            return error_response(format!(
                "failed to store outbound draft item for finding {}: {err}",
                preview.finding_id
            ));
        }
        stored_drafts.push(draft);
    }

    let state_counts = match store.outbound_state_counts_for_run(&session.id, &run.id) {
        Ok(counts) => counts,
        Err(err) => {
            return error_response(format!(
                "failed to project outbound approval state after drafting: {err}"
            ));
        }
    };

    let routine_surface = runtime_routine_surface_projection(runtime, &session.provider);
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
            "review_run_id": run.id.clone(),
            "selection": {
                "mode": selection_mode,
                "grouped": stored_drafts.len() > 1,
                "finding_ids": selected_findings
                    .iter()
                    .map(|finding| finding.id.clone())
                    .collect::<Vec<_>>(),
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

fn approval_invalidation_reason_for_linkage_issues(
    validation: &roger_app_core::OutboundDraftBatchValidation,
) -> &'static str {
    if validation.issues.iter().any(|issue| {
        matches!(
            issue.reason_code.as_str(),
            "target_mismatch" | "payload_digest_mismatch"
        )
    }) {
        "target_or_payload_drift"
    } else if validation.issues.iter().any(|issue| {
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

fn awaiting_approval_batch_ids_for_run(
    store: &RogerStore,
    review_session_id: &str,
    review_run_id: &str,
) -> Result<Vec<String>, String> {
    let findings = store
        .materialized_findings_for_run(review_session_id, review_run_id)
        .map_err(|err| format!("failed to load findings for latest run: {err}"))?;
    let mut batch_ids = Vec::new();
    for finding in findings {
        let projection = store
            .outbound_surface_projection_for_finding(&finding.id, &finding.outbound_state)
            .map_err(|err| {
                format!(
                    "failed to inspect outbound approval state for finding {}: {err}",
                    finding.id
                )
            })?;
        if projection.state == "awaiting_approval" {
            if let Some(batch_id) = projection.draft_batch_id {
                batch_ids.push(batch_id);
            }
        }
    }
    batch_ids.sort();
    batch_ids.dedup();
    Ok(batch_ids)
}

fn handle_approve(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    let store = match RogerStore::open(&runtime.store_root) {
        Ok(store) => store,
        Err(err) => return error_response(format!("failed to open Roger store: {err}")),
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

    let (session, _binding) = match resolution {
        SessionReentryResolution::Resolved { session, binding } => (session, binding),
        SessionReentryResolution::PickerRequired { reason, candidates } => {
            return blocked_picker_response(reason, candidates);
        }
    };

    if session.attention_state == "refresh_recommended" {
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
        return blocked_response(
            "rr approve is blocked because the persisted review state requires explicit reconciliation before approval can be granted"
                .to_owned(),
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
        );
    }

    if session.review_target.repository.trim().is_empty()
        || session.review_target.pull_request_number == 0
    {
        return blocked_response(
            "rr approve requires a concrete review target before Roger can bind local approval state"
                .to_owned(),
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
        );
    }

    let Some(run) = (match store.latest_review_run(&session.id) {
        Ok(run) => run,
        Err(err) => return error_response(format!("failed to load latest run: {err}")),
    }) else {
        return blocked_response(
            "rr approve requires persisted local review state for the selected target".to_owned(),
            vec![format!(
                "run rr review --repo {} --pr {} to materialize a local review first",
                session.review_target.repository, session.review_target.pull_request_number
            )],
            json!({
                "reason_code": "missing_local_state",
                "session_id": session.id,
            }),
        );
    };

    let candidate_batch_ids =
        match awaiting_approval_batch_ids_for_run(&store, &session.id, &run.id) {
            Ok(ids) => ids,
            Err(err) => return error_response(err),
        };

    let Some(batch_id) = parsed.batch_id.as_deref() else {
        return blocked_response(
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
                "review_run_id": run.id,
                "available_batch_ids": candidate_batch_ids,
            }),
        );
    };

    let Some(batch) = (match store.outbound_draft_batch(batch_id) {
        Ok(batch) => batch,
        Err(err) => return error_response(format!("failed to load outbound draft batch: {err}")),
    }) else {
        return blocked_response(
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
                "review_run_id": run.id,
                "draft_batch_id": batch_id,
                "available_batch_ids": candidate_batch_ids,
            }),
        );
    };

    if batch.review_session_id != session.id {
        return blocked_response(
            "rr approve refused to bind approval because the requested batch belongs to a different Roger session".to_owned(),
            vec![
                format!("inspect rr status --session {} --robot", session.id),
                "use the batch id returned by rr draft for this exact session".to_owned(),
            ],
            json!({
                "reason_code": "approval_invalidated:local_state_drift",
                "session_id": session.id,
                "review_run_id": run.id,
                "draft_batch_id": batch.id,
                "batch_review_session_id": batch.review_session_id,
            }),
        );
    }

    if batch.review_run_id != run.id {
        return blocked_response(
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
                "latest_review_run_id": run.id,
                "draft_batch_id": batch.id,
                "batch_review_run_id": batch.review_run_id,
            }),
        );
    }

    let expected_remote_review_target_id =
        format!("pr-{}", session.review_target.pull_request_number);
    if batch.repo_id != session.review_target.repository
        || batch.remote_review_target_id != expected_remote_review_target_id
    {
        return blocked_response(
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
                "draft_batch_id": batch.id,
                "expected_repo_id": session.review_target.repository,
                "expected_remote_review_target_id": expected_remote_review_target_id,
                "stored_repo_id": batch.repo_id,
                "stored_remote_review_target_id": batch.remote_review_target_id,
            }),
        );
    }

    let posted_actions = match store.posted_actions_for_batch(&batch.id) {
        Ok(actions) => actions,
        Err(err) => {
            return error_response(format!("failed to inspect prior posted actions: {err}"));
        }
    };
    if let Some(latest_posted_action) = posted_actions.last() {
        return blocked_response(
            "rr approve is no longer available because Roger already recorded a post attempt for this batch".to_owned(),
            vec![format!(
                "inspect rr status --session {} --robot for the current outbound posting state",
                session.id
            )],
            json!({
                "reason_code": "existing_posted_action",
                "session_id": session.id,
                "draft_batch_id": batch.id,
                "posted_action_id": latest_posted_action.id,
                "posted_action_status": format!("{:?}", latest_posted_action.status),
                "failure_code": latest_posted_action.failure_code.clone(),
            }),
        );
    }

    let drafts = match store.outbound_draft_items_for_batch(&batch.id) {
        Ok(drafts) => drafts,
        Err(err) => return error_response(format!("failed to load outbound draft items: {err}")),
    };
    if drafts.is_empty() {
        return blocked_response(
            "rr approve requires persisted local draft items for the selected batch".to_owned(),
            vec![format!(
                "re-run rr draft --session {} --finding <finding-id> to materialize the batch again",
                session.id
            )],
            json!({
                "reason_code": "missing_local_state",
                "session_id": session.id,
                "draft_batch_id": batch.id,
            }),
        );
    }

    let validation = validate_outbound_draft_batch_linkage(&batch, &drafts);
    if !validation.valid {
        return blocked_response(
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
                "reason_code": format!(
                    "approval_invalidated:{}",
                    approval_invalidation_reason_for_linkage_issues(&validation)
                ),
                "session_id": session.id,
                "draft_batch_id": batch.id,
                "validation_issues": validation
                    .issues
                    .iter()
                    .map(|issue| json!({
                        "draft_id": issue.draft_id.clone(),
                        "reason_code": issue.reason_code.clone(),
                    }))
                    .collect::<Vec<_>>(),
            }),
        );
    }

    if batch.invalidated_at.is_some() || matches!(&batch.approval_state, ApprovalState::Invalidated)
    {
        return blocked_response(
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
                    batch
                        .invalidation_reason_code
                        .clone()
                        .unwrap_or_else(|| "unspecified".to_owned())
                ),
                "session_id": session.id,
                "draft_batch_id": batch.id,
                "approval_state": "invalidated",
                "invalidation_reason_code": batch.invalidation_reason_code,
                "invalidated_at": batch.invalidated_at,
            }),
        );
    }

    let current_draft_state_issues = drafts
        .iter()
        .filter_map(|draft| match &draft.approval_state {
            ApprovalState::Drafted | ApprovalState::Approved => None,
            ApprovalState::Invalidated => Some(json!({
                "draft_id": draft.id,
                "finding_id": draft.finding_id,
                "reason_code": "draft_invalidated",
            })),
            ApprovalState::Posted => Some(json!({
                "draft_id": draft.id,
                "finding_id": draft.finding_id,
                "reason_code": "draft_already_posted",
            })),
            ApprovalState::Failed => Some(json!({
                "draft_id": draft.id,
                "finding_id": draft.finding_id,
                "reason_code": "draft_already_failed",
            })),
            ApprovalState::NotDrafted => Some(json!({
                "draft_id": draft.id,
                "finding_id": draft.finding_id,
                "reason_code": "draft_state_not_materialized",
            })),
        })
        .collect::<Vec<_>>();
    if !current_draft_state_issues.is_empty() {
        return blocked_response(
            "rr approve is blocked because the stored draft items are no longer all in an approvable state".to_owned(),
            vec![format!(
                "inspect rr findings --session {} --robot to review the current outbound state",
                session.id
            )],
            json!({
                "reason_code": "stale_local_state",
                "session_id": session.id,
                "draft_batch_id": batch.id,
                "draft_state_issues": current_draft_state_issues,
            }),
        );
    }

    let expected_target_tuple_json = outbound_target_tuple_json(&batch);
    let existing_approval = match store.approval_token_for_batch(&batch.id) {
        Ok(approval) => approval,
        Err(err) => {
            return error_response(format!("failed to inspect existing approval token: {err}"));
        }
    };
    if let Some(approval) = &existing_approval {
        if approval.revoked_at.is_some() {
            return blocked_response(
                "rr approve is blocked because the stored approval token was already revoked"
                    .to_owned(),
                vec![format!(
                    "re-run rr draft --session {} --finding <finding-id> after reviewing the revoked batch state",
                    session.id
                )],
                json!({
                    "reason_code": "approval_revoked",
                    "session_id": session.id,
                    "draft_batch_id": batch.id,
                    "approval_id": approval.id,
                    "revoked_at": approval.revoked_at,
                }),
            );
        }
        if approval.payload_digest != batch.payload_digest {
            return blocked_response(
                "rr approve refused to reuse the stored approval token because its payload digest no longer matches the batch".to_owned(),
                vec![format!(
                    "re-run rr draft --session {} --finding <finding-id> to materialize a fresh batch",
                    session.id
                )],
                json!({
                    "reason_code": "approval_payload_digest_mismatch",
                    "session_id": session.id,
                    "draft_batch_id": batch.id,
                    "approval_id": approval.id,
                    "expected_payload_digest": batch.payload_digest,
                    "stored_payload_digest": approval.payload_digest,
                }),
            );
        }
        if approval.target_tuple_json != expected_target_tuple_json {
            return blocked_response(
                "rr approve refused to reuse the stored approval token because its target tuple no longer matches the batch".to_owned(),
                vec![format!(
                    "re-run rr draft --session {} --finding <finding-id> after reconciling target drift",
                    session.id
                )],
                json!({
                    "reason_code": "approval_target_tuple_mismatch",
                    "session_id": session.id,
                    "draft_batch_id": batch.id,
                    "approval_id": approval.id,
                    "expected_target_tuple_json": expected_target_tuple_json,
                    "stored_target_tuple_json": approval.target_tuple_json,
                }),
            );
        }
    }

    let batch_already_approved = matches!(&batch.approval_state, ApprovalState::Approved);
    if !matches!(
        &batch.approval_state,
        ApprovalState::Drafted | ApprovalState::Approved
    ) {
        return blocked_response(
            "rr approve is blocked because the stored batch is no longer in an approvable state"
                .to_owned(),
            vec![format!(
                "inspect rr status --session {} --robot for the current outbound state",
                session.id
            )],
            json!({
                "reason_code": "stale_local_state",
                "session_id": session.id,
                "draft_batch_id": batch.id,
                "approval_state": format!("{:?}", batch.approval_state),
            }),
        );
    }

    let approval_needs_insert = existing_approval.is_none();
    let approval = existing_approval.unwrap_or_else(|| OutboundApprovalToken {
        id: next_id("approval"),
        draft_batch_id: batch.id.clone(),
        payload_digest: batch.payload_digest.clone(),
        target_tuple_json: expected_target_tuple_json.clone(),
        approved_at: time::now_ts(),
        revoked_at: None,
    });
    let approval_created = !batch_already_approved;

    for draft in &drafts {
        if matches!(&draft.approval_state, ApprovalState::Approved) {
            continue;
        }

        let mut approved_draft = draft.clone();
        approved_draft.approval_state = ApprovalState::Approved;
        approved_draft.row_version += 1;
        if let Err(err) = store.store_outbound_draft_item(&approved_draft) {
            return error_response(format!(
                "failed to store approved outbound draft item for finding {}: {err}",
                draft.finding_id.as_deref().unwrap_or("<unknown>")
            ));
        }
    }

    if approval_needs_insert {
        if let Err(err) = store.store_outbound_approval_token(&approval) {
            return error_response(format!("failed to store outbound approval token: {err}"));
        }
    }

    if !batch_already_approved || batch.approved_at != Some(approval.approved_at) {
        let mut approved_batch = batch.clone();
        approved_batch.approval_state = ApprovalState::Approved;
        approved_batch.approved_at = Some(approval.approved_at);
        approved_batch.invalidated_at = None;
        approved_batch.invalidation_reason_code = None;
        approved_batch.row_version += 1;
        if let Err(err) = store.store_outbound_draft_batch(&approved_batch) {
            return error_response(format!(
                "failed to store approved outbound draft batch: {err}"
            ));
        }
    }

    let state_counts = match store.outbound_state_counts_for_run(&session.id, &run.id) {
        Ok(counts) => counts,
        Err(err) => {
            return error_response(format!(
                "failed to project outbound approval state after approving: {err}"
            ));
        }
    };

    let routine_surface = runtime_routine_surface_projection(runtime, &session.provider);
    let provider_capability = runtime_provider_capability(runtime, &session.provider);
    let warnings: Vec<String> = provider_support_warning(&session.provider, "rr approve")
        .into_iter()
        .collect();

    CommandResponse {
        outcome: OutcomeKind::Complete,
        data: json!({
            "session_id": session.id.clone(),
            "review_run_id": run.id.clone(),
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

fn handle_post(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    let store = match RogerStore::open(&runtime.store_root) {
        Ok(store) => store,
        Err(err) => return error_response(format!("failed to open Roger store: {err}")),
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

    let (session, _binding) = match resolution {
        SessionReentryResolution::Resolved { session, binding } => (session, binding),
        SessionReentryResolution::PickerRequired { reason, candidates } => {
            return blocked_picker_response(reason, candidates);
        }
    };

    if session.attention_state == "refresh_recommended" {
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
        return blocked_response(
            "rr post is blocked because the persisted review state requires explicit reconciliation before GitHub mutation can run"
                .to_owned(),
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
        );
    }

    if session.review_target.repository.trim().is_empty()
        || session.review_target.pull_request_number == 0
    {
        return blocked_response(
            "rr post requires a concrete review target before Roger can execute outbound mutation"
                .to_owned(),
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
        );
    }

    let Some(run) = (match store.latest_review_run(&session.id) {
        Ok(run) => run,
        Err(err) => return error_response(format!("failed to load latest run: {err}")),
    }) else {
        return blocked_response(
            "rr post requires persisted local review state for the selected target".to_owned(),
            vec![format!(
                "run rr review --repo {} --pr {} to materialize a local review first",
                session.review_target.repository, session.review_target.pull_request_number
            )],
            json!({
                "reason_code": "missing_local_state",
                "session_id": session.id,
            }),
        );
    };

    let candidate_batch_ids = match approved_batch_ids_for_run(&store, &session.id, &run.id) {
        Ok(ids) => ids,
        Err(err) => return error_response(err),
    };

    let Some(batch_id) = parsed.batch_id.as_deref() else {
        return blocked_response(
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
                "review_run_id": run.id,
                "available_batch_ids": candidate_batch_ids,
            }),
        );
    };

    let branch = infer_git_branch(&runtime.cwd);
    let provider_tier = provider_tier(&session.provider);
    let mut warnings: Vec<String> = provider_support_warning(&session.provider, "rr status")
        .into_iter()
        .collect();
    if let Some(warning) = automatic_reconciliation_warning(&session.attention_state) {
        warnings.push(warning);
    }

    CommandResponse {
        outcome: OutcomeKind::Complete,
        data: json!({
            "repo": {
                "root": runtime.cwd.to_string_lossy(),
                "branch": branch,
                "repository": session.review_target.repository,
            },
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
            "reconciliation": {
                "mode": "automatic_background",
                "fractional_staleness_allowed": true,
                "stale_target_detected": session.attention_state == "refresh_recommended",
            },
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
            },
            "continuity": {
                "tier": provider_tier,
                "resume_locator_present": session.session_locator.is_some(),
                "state": session.continuity_state,
            },
            "provider_capability": provider_capability(&session.provider)
        }),
        warnings,
        repair_actions: Vec::new(),
        message: "status loaded".to_owned(),
    }
}

fn handle_findings(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    let store = match RogerStore::open(&runtime.store_root) {
        Ok(store) => store,
        Err(err) => return error_response(format!("failed to open Roger store: {err}")),
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

    let (session, _binding) = match resolution {
        SessionReentryResolution::Resolved { session, binding } => (session, binding),
        SessionReentryResolution::PickerRequired { reason, candidates } => {
            return blocked_picker_response(reason, candidates);
        }
    };

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
                }
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
    if let Some(warning) = automatic_reconciliation_warning(&session.attention_state) {
        warnings.push(warning);
    }

    let mut items = Vec::with_capacity(findings.len());
    for finding in &findings {
        let evidence_count = match store.count_code_evidence_locations_for_finding(&finding.id) {
            Ok(count) => count as usize,
            Err(err) => {
                return error_response(format!(
                    "failed to count evidence locations for finding {}: {err}",
                    finding.id
                ));
            }
        };

        let outbound_projection =
            match store.outbound_surface_projection_for_finding(&finding.id, &finding.outbound_state)
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
            "triage_state": finding.triage_state,
            "outbound_state": outbound_projection.state,
            "outbound_detail": {
                "source": outbound_projection.source,
                "draft_id": outbound_projection.draft_id,
                "draft_batch_id": outbound_projection.draft_batch_id,
                "approval_id": outbound_projection.approval_id,
                "posted_action_id": outbound_projection.posted_action_id,
                "posted_action_status": outbound_projection.posted_action_status,
                "invalidation_reason_code": outbound_projection.invalidation_reason_code,
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
            "reconciliation": {
                "mode": "automatic_background",
                "fractional_staleness_allowed": true,
                "stale_target_detected": session.attention_state == "refresh_recommended",
            },
            "provider_capability": provider_capability(&session.provider),
        }),
        warnings,
        repair_actions: Vec::new(),
        message: if count == 0 {
            "no findings available for this session".to_owned()
        } else {
            format!("loaded {count} findings")
        },
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

    if matches!(
        parsed.command,
        CommandKind::Status
            | CommandKind::Findings
            | CommandKind::Sessions
            | CommandKind::Search
            | CommandKind::Draft
            | CommandKind::Approve
            | CommandKind::RobotDocs
    ) || response.outcome == OutcomeKind::Blocked
    {
        if let Ok(pretty) = serde_json::to_string_pretty(&response.data) {
            stdout.push_str(&pretty);
            stdout.push('\n');
        }
    }

    let mut stderr = String::new();
    if !response.warnings.is_empty() {
        stderr.push_str(&response.warnings.join("\n"));
        stderr.push('\n');
    }
    if !response.repair_actions.is_empty() {
        stderr.push_str("Suggested next steps:\n");
        for action in &response.repair_actions {
            stderr.push_str("- ");
            stderr.push_str(action);
            stderr.push('\n');
        }
    }

    CliRunResult {
        exit_code: response.outcome.exit_code(),
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
        artifact_refs: Vec::new(),
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
        "pi-agent" => "not part of the 0.1.0 live CLI surface",
        _ => "provider is not part of the current live rr review surface",
    }
}

fn provider_capability(provider: &str) -> Value {
    json!({
        "provider": provider,
        "display_name": provider_display_name(provider),
        "status": provider_support_status(provider),
        "tier": provider_tier(provider),
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

fn provider_capability_projection(
    provider: &ResolvedProviderCapability,
    status_reason: Option<&str>,
) -> Value {
    json!({
        "provider": provider.provider,
        "display_name": provider.display_name,
        "status": provider.status,
        "tier": provider.support_tier,
        "support_tier": provider.support_tier,
        "surface_class": provider.surface_class,
        "policy_profile": {
            "id": provider.policy_profile.id,
            "summary": provider.policy_profile.summary,
            "mutation_posture": provider.policy_profile.mutation_posture,
            "continuity_mode": provider.policy_profile.continuity_mode,
        },
        "status_reason": status_reason,
        "supports": provider_supports_json(provider),
        "notes": provider.notes,
    })
}

fn routine_surface_baseline_projection(baseline: &ResolvedRoutineSurfaceBaseline) -> Value {
    json!({
        "surface": baseline.surface,
        "launch_profile_id": baseline.launch_profile_id.value,
        "provider": provider_capability_projection(&baseline.provider, baseline.status_reason.as_deref()),
        "ui_target": baseline.ui_target.value,
        "instance_preference": baseline.instance_preference.value,
        "isolation_mode": baseline.isolation_mode.value,
        "named_instance_on_collision": baseline.named_instance_on_collision.value,
        "repair_overrides_active": baseline.repair_overrides_active,
        "active_repair_override_keys": baseline.active_repair_override_keys,
        "status_reason": baseline.status_reason,
    })
}

fn runtime_provider_capability(runtime: &CliRuntime, provider: &str) -> Value {
    match resolved_routine_surface_baseline(runtime, provider) {
        Ok(baseline) => {
            provider_capability_projection(&baseline.provider, baseline.status_reason.as_deref())
        }
        Err(_) => provider_capability(provider),
    }
}

fn runtime_routine_surface_projection(runtime: &CliRuntime, provider: &str) -> Option<Value> {
    resolved_routine_surface_baseline(runtime, provider)
        .ok()
        .map(|baseline| routine_surface_baseline_projection(&baseline))
}

fn review_provider_support_matrix() -> Vec<Value> {
    SUPPORTED_REVIEW_PROVIDERS
        .iter()
        .map(|provider| {
            let provider = *provider;
            let capability = provider_capability(provider);
            json!({
                "provider": provider,
                "display_name": provider_display_name(provider),
                "tier": provider_tier(provider),
                "status": provider_support_status(provider),
                "supports": capability["supports"].clone(),
                "notes": provider_live_support_notes(provider),
            })
        })
        .collect()
}

fn provider_support_warning(provider: &str, command: &str) -> Option<String> {
    if provider == "opencode" {
        None
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

fn automatic_reconciliation_warning(attention_state: &str) -> Option<String> {
    if attention_state == "refresh_recommended" {
        Some(
            "Roger reconciles stale review state automatically; current results may be fractionally stale until background reconciliation completes."
                .to_owned(),
        )
    } else {
        None
    }
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

fn blocked_picker_response(reason: String, candidates: Vec<SessionFinderEntry>) -> CommandResponse {
    let no_match = candidates.is_empty() || reason.contains("no matching repo-local session found");
    let warnings = if no_match {
        vec!["no matching session found for the requested target".to_owned()]
    } else {
        vec!["session inference is ambiguous; explicit selection is required".to_owned()]
    };
    let repair_actions = if no_match {
        vec![
            "run rr review --pr <number> to create a new session".to_owned(),
            "run rr sessions --robot to inspect available sessions".to_owned(),
        ]
    } else {
        vec!["re-run with --session <id> or pass --pr <number> for a unique match".to_owned()]
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
        message: "session picker required".to_owned(),
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

fn render_local_outbound_draft_body(finding: &roger_storage::MaterializedFindingRecord) -> String {
    format!(
        "Finding: {}\n\nSummary:\n{}\n\nSeverity: {}\nConfidence: {}\nFingerprint: {}",
        finding.title,
        finding.normalized_summary,
        finding.severity,
        finding.confidence,
        finding.fingerprint
    )
}

fn outbound_draft_anchor_digest(
    finding: &roger_storage::MaterializedFindingRecord,
) -> Result<String, String> {
    sha256_prefixed_json(&json!({
        "finding_id": finding.id,
        "fingerprint": finding.fingerprint,
        "title": finding.title,
        "normalized_summary": finding.normalized_summary,
    }))
}

fn outbound_draft_target_locator(target: &ReviewTarget, finding_id: &str) -> String {
    format!(
        "github:{}#{}:finding/{}",
        target.repository, target.pull_request_number, finding_id
    )
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
}

export interface BridgeResponse {
  ok: boolean;
  action: string;
  message: string;
  session_id?: string;
  guidance?: string;
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

fn usage_text() -> &'static str {
    "Usage:\n  rr agent <operation> --task-file <path> [--request-file <path>] [--context-file <path>] [--capability-file <path>]\n  rr review --pr <number> [--repo owner/repo] [--provider opencode|codex|gemini|claude] [--dry-run] [--robot]\n  rr resume [--repo owner/repo] [--pr <number>] [--session <id>] [--dry-run] [--robot]\n  rr return [--repo owner/repo] [--pr <number>] [--session <id>] [--robot]\n  rr sessions [--repo owner/repo] [--pr <number>] [--attention <state[,state...]>] [--limit <n>] [--robot]\n  rr search --query <text> [--query-mode auto|exact_lookup|recall|related_context|candidate_audit|promotion_review] [--repo owner/repo] [--limit <n>] [--robot]\n  rr draft [--repo owner/repo] [--pr <number>] [--session <id>] (--finding <id>... | --all-findings) [--robot]\n  rr approve [--repo owner/repo] [--pr <number>] [--session <id>] --batch <draft-batch-id> [--robot]\n  rr update [--repo owner/repo] [--channel stable|rc] [--version <YYYY.MM.DD[-rc.N]>] [--api-root <url>] [--download-root <url>] [--target <triple>] [--yes|-y] [--dry-run] [--robot]\n  rr bridge export-contracts [--robot]\n  rr bridge verify-contracts [--robot]\n  rr bridge pack-extension [--output-dir <path>] [--robot]\n  rr bridge install [--extension-id <id>] [--bridge-binary <path>] [--install-root <path>] [--robot]\n  rr extension setup [--browser edge|chrome|brave] [--install-root <path>] [--robot]\n  rr extension doctor [--browser edge|chrome|brave] [--install-root <path>] [--robot]\n  rr bridge uninstall [--install-root <path>] [--robot]\n  rr robot-docs [guide|commands|schemas|workflows] [--robot]\n  rr findings [--repo owner/repo] [--pr <number>] [--session <id>] [--robot]\n  rr status [--repo owner/repo] [--pr <number>] [--session <id>] [--robot]\n\nAgent transport:\n  - rr agent is the dedicated in-session worker transport; it is separate from --robot\n  - current live rr agent operations cover context/status/search/finding/artifact reads, advisory clarification or follow-up proposals, and worker.submit_stage_result\n  - rr agent emits rr.agent.response.v1 envelopes over the canonical worker operation response payload instead of reusing the --robot surface\n\nProvider support in 0.1.0:\n  - opencode is the first-class tier-b continuity path; rr resume can reopen and rr return is supported\n  - codex, gemini, and claude are bounded tier-a providers; start/reseed/raw-capture only, no locator reopen or rr return\n  - copilot is planned but not yet a live --provider value\n  - pi-agent is not part of the 0.1.0 live CLI surface\n\nOutbound notes:\n  - rr draft materializes Roger-owned local draft batches only; it does not approve or post to GitHub\n  - rr approve records a local approval token for one exact stored batch payload and target tuple; it does not post to GitHub\n  - draft selection is explicit in this slice: pass one or more --finding ids or --all-findings, then approve with --batch\n  - stale persisted review state fails closed before Roger derives or approves outbound payloads\n\nUpdate notes:\n  - default rr update apply prompts for confirmation on interactive TTY\n  - pass --yes|-y for non-interactive apply confirmation; --robot apply requires --yes|-y\n  - --dry-run and --robot without --yes are non-mutating metadata checks\n  - local/unpublished builds fail closed; migration-capable updates are deferred in 0.1.x"
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use tempfile::tempdir;

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
        let background = extension_src.join("background/main.js");
        let content = extension_src.join("content/main.js");
        let manifest_template = root.join("apps/extension/manifest.template.json");
        let static_root = root.join("apps/extension/static");
        let bridge_src = root.join("packages/bridge/src/lib.rs");
        fs::create_dir_all(generated.parent().expect("generated parent")).expect("mkdir generated");
        fs::create_dir_all(background.parent().expect("background parent"))
            .expect("mkdir background");
        fs::create_dir_all(content.parent().expect("content parent")).expect("mkdir content");
        fs::create_dir_all(&static_root).expect("mkdir static");
        fs::create_dir_all(bridge_src.parent().expect("bridge src parent"))
            .expect("mkdir bridge src");
        fs::write(&bridge_src, "// bridge marker\n").expect("write bridge marker");
        fs::write(&generated, bridge_contract_snapshot()).expect("write generated bridge contract");
        fs::write(&background, "export const background = true;\n").expect("write background");
        fs::write(&content, "export const content = true;\n").expect("write content");
        fs::write(
            &manifest_template,
            r#"{
  "manifest_version": 3,
  "name": "Roger Reviewer",
  "version": "0.1.0",
  "description": "Launch local Roger review flows from GitHub PR pages.",
  "permissions": ["nativeMessaging"],
  "background": {
    "service_worker": "src/background/main.js",
    "type": "module"
  },
  "content_scripts": [
    {
      "matches": ["https://github.com/*/*/pull/*"],
      "js": ["src/content/main.js"]
    }
  ]
}
"#,
        )
        .expect("write manifest template");
        fs::write(static_root.join(".gitkeep"), "").expect("write static marker");

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

    fn register_extension_identity_via_bridge(
        runtime: &CliRuntime,
        browser: &str,
        extension_id: &str,
    ) {
        static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        let _env_guard = ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("env lock");

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
        assert!(package_dir.join("SHA256SUMS").exists());
        assert!(package_dir.join("asset-manifest.json").exists());

        let asset_manifest: Value = serde_json::from_str(
            &fs::read_to_string(package_dir.join("asset-manifest.json"))
                .expect("read asset manifest"),
        )
        .expect("parse asset manifest");
        assert_eq!(asset_manifest["version"], "0.1.0.0");
        assert_eq!(asset_manifest["version_name"], "0.1.0-dev.0+nogit");
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
        assert_eq!(uninstall_payload["data"]["subcommand"], "uninstall");
        let removed = uninstall_payload["data"]["removed"]
            .as_array()
            .expect("removed list");
        assert!(removed.len() >= 3);

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
    }

    #[test]
    fn extension_setup_blocks_without_discovered_identity() {
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
        assert_eq!(result.exit_code, 3, "{}", result.stderr);
        let payload = parse_robot(&result.stdout);
        assert_eq!(payload["outcome"], "blocked");
        assert_eq!(payload["data"]["subcommand"], "setup");
        assert_eq!(
            payload["data"]["reason_code"],
            "extension_registration_missing"
        );
        assert_eq!(payload["data"]["browser"], "edge");
        let repair_actions = payload["repair_actions"]
            .as_array()
            .expect("repair actions should be an array");
        assert!(
            repair_actions
                .first()
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .contains("open edge://extensions")
        );
        assert!(repair_actions.iter().any(|action| {
            action
                .as_str()
                .unwrap_or_default()
                .contains("RR_BRIDGE_EXTENSION_ID")
        }));
    }

    #[test]
    fn extension_setup_and_doctor_succeed_with_discovered_identity() {
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
        let (tmp, runtime, _generated) = setup_bridge_workspace();
        let install_root = tmp.path().join("install-root");
        let extension_id = "abcdefghijklmnopabcdefghijklmnop";

        let blocked_setup = run(
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
        assert_eq!(blocked_setup.exit_code, 3, "{}", blocked_setup.stderr);
        let blocked_payload = parse_robot(&blocked_setup.stdout);
        assert_eq!(blocked_payload["outcome"], "blocked");
        assert_eq!(
            blocked_payload["data"]["reason_code"],
            "extension_registration_missing"
        );

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
        assert_eq!(setup_payload["data"]["extension_id"], extension_id);
        assert_eq!(
            setup_payload["data"]["extension_id_source"],
            "store_registry"
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
            usage_text().contains("rr update")
                && usage_text().contains("[--yes|-y] [--dry-run] [--robot]"),
            "{}",
            usage_text()
        );
    }

    #[test]
    fn draft_and_approve_usage_text_and_flag_contract_are_explicit() {
        assert!(
            usage_text().contains("rr draft")
                && usage_text().contains("(--finding <id>... | --all-findings)")
                && usage_text().contains("rr approve")
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
            usage_text().contains("copilot is planned but not yet a live --provider value"),
            "{}",
            usage_text()
        );
        assert!(
            usage_text().contains("pi-agent is not part of the 0.1.0 live CLI surface"),
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
        assert_eq!(payload["data"]["migration"]["status"], "deferred_in_0_1_x");
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
    fn migration_policy_is_explicitly_deferred_in_0_1_x() {
        let policy = migration_policy_payload();
        assert_eq!(policy["policy"], "binary_only");
        assert_eq!(policy["schema_migrations_supported"], false);
        assert_eq!(policy["status"], "deferred_in_0_1_x");
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
        let envelope = sample_store_compatibility("auto_safe");
        let preflight = assess_migration_preflight(9, &envelope, true);
        assert_eq!(preflight.status, "auto_safe_migration_after_update");
        assert_eq!(preflight.classification, "class_b");
        assert!(preflight.apply_allowed);
        assert!(preflight.blocked_reason.is_none());
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
}
