use roger_app_core::cli_config;
use roger_app_core::time;
use roger_app_core::{
    AppError, ContinuityQuality, HarnessAdapter, LaunchAction, LaunchIntent, ResumeAttemptOutcome,
    ResumeBundle, ResumeBundleProfile, ReviewTarget, RogerCommand, RogerCommandId,
    RogerCommandInvocationSurface, RogerCommandResult, RogerCommandRouteStatus, Surface,
    route_harness_command, safe_harness_command_bindings, FindingTriageState, FindingOutboundState,
};
use roger_bridge::{
    NativeHostManifest, SupportedBrowser, SupportedOs, custom_url_helper_path_for,
    native_host_install_path_for, render_custom_url_helper,
};
use roger_config::cli_defaults::{DEFAULT_OPENCODE_BIN, ENV_OPENCODE_BIN, ENV_STORE_ROOT};
use roger_session_codex::{CodexAdapter, CodexSessionPath};
use roger_session_opencode::{
    OpenCodeAdapter, OpenCodeReturnPath, OpenCodeSessionPath, rr_return_to_roger_session,
};
use roger_storage::{
    CreateReviewRun, CreateReviewSession, CreateSessionLaunchBinding, LaunchSurface,
    PriorReviewLookupQuery, PriorReviewRetrievalMode, ResolveSessionLaunchBinding,
    ResolveSessionReentry, RogerStore, SessionFinderEntry, SessionFinderQuery,
    SessionReentryResolution,
};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::result::Result; // Added this line
use std::process::Command as ProcessCommand;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use toon_format::encode_default as encode_toon_default;

static ID_SEQ: AtomicU64 = AtomicU64::new(1);

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
    Review,
    Resume,
    Return,
    Sessions,
    Search,
    Update,
    Bridge,
    RobotDocs,
    Findings,
    Status,
    Refresh,
}

impl CommandKind {
    fn as_rr_command(self, dry_run: bool) -> &'static str {
        match (self, dry_run) {
            (Self::Review, true) => "rr review --dry-run",
            (Self::Resume, true) => "rr resume --dry-run",
            (Self::Review, false) => "rr review",
            (Self::Resume, false) => "rr resume",
            (Self::Return, _) => "rr return",
            (Self::Sessions, _) => "rr sessions",
            (Self::Search, _) => "rr search",
            (Self::Update, _) => "rr update",
            (Self::Bridge, _) => "rr bridge",
            (Self::RobotDocs, _) => "rr robot-docs",
            (Self::Findings, _) => "rr findings",
            (Self::Status, _) => "rr status",
            (Self::Refresh, _) => "rr refresh",
        }
    }

    fn schema_id(self) -> &'static str {
        match self {
            Self::Review => "rr.robot.review.v1",
            Self::Resume => "rr.robot.resume.v1",
            Self::Return => "rr.robot.return.v1",
            Self::Sessions => "rr.robot.sessions.v1",
            Self::Search => "rr.robot.search.v1",
            Self::Update => "rr.robot.update.v1",
            Self::Bridge => "rr.robot.bridge.v1",
            Self::RobotDocs => "rr.robot.robot_docs.v1",
            Self::Findings => "rr.robot.findings.v1",
            Self::Status => "rr.robot.status.v1",
            Self::Refresh => "rr.robot.refresh.v1",
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
    bridge_command: Option<BridgeCommandKind>,
    bridge_extension_id: Option<String>,
    bridge_binary_path: Option<PathBuf>,
    bridge_install_root: Option<PathBuf>,
    bridge_output_dir: Option<PathBuf>,
    repo: Option<String>,
    pr: Option<u64>,
    session_id: Option<String>,
    update_channel: String,
    update_version: Option<String>,
    update_api_root: Option<String>,
    update_download_root: Option<String>,
    update_target: Option<String>,
    attention_states: Vec<String>,
    limit: Option<usize>,
    query_text: Option<String>,
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
        "review" => CommandKind::Review,
        "resume" => CommandKind::Resume,
        "return" => CommandKind::Return,
        "sessions" => CommandKind::Sessions,
        "search" => CommandKind::Search,
        "update" => CommandKind::Update,
        "bridge" => CommandKind::Bridge,
        "robot-docs" => CommandKind::RobotDocs,
        "findings" => CommandKind::Findings,
        "status" => CommandKind::Status,
        "refresh" => CommandKind::Refresh,
        "-h" | "--help" | "help" => {
            return Err("help requested".to_owned());
        }
        other => return Err(format!("unknown command: {other}")),
    };

    let mut parsed = ParsedArgs {
        command,
        bridge_command: None,
        bridge_extension_id: None,
        bridge_binary_path: None,
        bridge_install_root: None,
        bridge_output_dir: None,
        repo: None,
        pr: None,
        session_id: None,
        update_channel: "stable".to_owned(),
        update_version: None,
        update_api_root: None,
        update_download_root: None,
        update_target: None,
        attention_states: Vec::new(),
        limit: None,
        query_text: None,
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

    if parsed.command != CommandKind::Bridge
        && (parsed.bridge_extension_id.is_some()
            || parsed.bridge_binary_path.is_some()
            || parsed.bridge_install_root.is_some()
            || parsed.bridge_output_dir.is_some())
    {
        return Err(
            "--extension-id/--bridge-binary/--install-root/--output-dir are bridge-only flags"
                .to_owned(),
        );
    }

    if parsed.command != CommandKind::Update
        && (parsed.update_channel != "stable"
            || parsed.update_version.is_some()
            || parsed.update_api_root.is_some()
            || parsed.update_download_root.is_some()
            || parsed.update_target.is_some())
    {
        return Err(
            "--channel/--version/--api-root/--download-root/--target are update-only flags"
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
            || parsed.robot_docs_topic.is_some()
            || parsed.provider != "opencode"
            || parsed.bridge_command.is_some()
            || parsed.bridge_extension_id.is_some()
            || parsed.bridge_binary_path.is_some()
            || parsed.bridge_install_root.is_some()
            || parsed.bridge_output_dir.is_some()
        {
            return Err(
                "rr update only supports --repo, --channel, --version, --api-root, --download-root, --target, --dry-run, and --robot".to_owned(),
            );
        }
    }

    Ok(parsed)
}

fn execute_command(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    match parsed.command {
        CommandKind::Review => handle_review(parsed, runtime),
        CommandKind::Resume => {
            handle_resume_or_refresh(parsed, runtime, LaunchAction::ResumeReview)
        }
        CommandKind::Return => handle_return(parsed, runtime),
        CommandKind::Sessions => handle_sessions(parsed, runtime),
        CommandKind::Search => handle_search(parsed, runtime),
        CommandKind::Update => handle_update(parsed, runtime),
        CommandKind::Bridge => handle_bridge(parsed, runtime),
        CommandKind::RobotDocs => handle_robot_docs(parsed),
        CommandKind::Findings => handle_findings(parsed, runtime),
        CommandKind::Status => handle_status(parsed, runtime),
        CommandKind::Refresh => {
            handle_resume_or_refresh(parsed, runtime, LaunchAction::RefreshFindings)
        }
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
            let manifest_template = match fs::read_to_string(&manifest_template_path) {
                Ok(text) => text,
                Err(err) => {
                    return error_response(format!(
                        "failed to read extension manifest template: {err}"
                    ));
                }
            };
            let manifest_json: Value = match serde_json::from_str(&manifest_template) {
                Ok(value) => value,
                Err(err) => {
                    return error_response(format!(
                        "failed to parse extension manifest template: {err}"
                    ));
                }
            };
            let version = manifest_json
                .get("version")
                .and_then(Value::as_str)
                .unwrap_or("0.0.0")
                .to_owned();

            let output_root = parsed
                .bridge_output_dir
                .clone()
                .unwrap_or_else(|| workspace_root.join("target/bridge/extension"));
            let package_dir = output_root.join(format!("roger-extension-{version}-unpacked"));
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

            let extension_id = parsed
                .bridge_extension_id
                .clone()
                .or_else(|| std::env::var("RR_BRIDGE_EXTENSION_ID").ok())
                .filter(|value| !value.trim().is_empty());
            let Some(extension_id) = extension_id else {
                return blocked_response(
                    "rr bridge install requires an explicit extension id".to_owned(),
                    vec![
                        "pass --extension-id <chrome-extension-id>".to_owned(),
                        "or set RR_BRIDGE_EXTENSION_ID".to_owned(),
                    ],
                    json!({"reason_code": "extension_id_required"}),
                );
            };

            let bridge_binary = parsed
                .bridge_binary_path
                .clone()
                .or_else(|| {
                    std::env::var("RR_BRIDGE_HOST_BINARY")
                        .ok()
                        .map(PathBuf::from)
                })
                .unwrap_or_else(|| workspace_root.join("target/release/rr-bridge"));
            if !bridge_binary.exists() {
                return blocked_response(
                    format!(
                        "bridge host binary was not found at {}",
                        bridge_binary.display()
                    ),
                    vec![
                        "pass --bridge-binary <path-to-rr-bridge>".to_owned(),
                        "or set RR_BRIDGE_HOST_BINARY".to_owned(),
                    ],
                    json!({
                        "reason_code": "bridge_binary_missing",
                        "bridge_binary": bridge_binary.to_string_lossy().to_string(),
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

            let helper_path = custom_url_helper_path_for(host_os, &install_root);
            let helper_contents = render_custom_url_helper(host_os, &bridge_binary);
            if let Some(parent) = helper_path.parent() {
                if let Err(err) = fs::create_dir_all(parent) {
                    return error_response(format!(
                        "failed to create custom URL helper directory {}: {err}",
                        parent.display()
                    ));
                }
            }
            if let Err(err) = fs::write(&helper_path, helper_contents.as_bytes()) {
                return error_response(format!(
                    "failed to write custom URL helper {}: {err}",
                    helper_path.display()
                ));
            }
            installed_assets.push(json!({
                "asset_kind": "custom_url_helper",
                "platform": host_os.as_str(),
                "path": helper_path.to_string_lossy().to_string(),
                "sha256": sha256_hex(helper_contents.as_bytes()),
                "bytes": helper_contents.len(),
            }));

            CommandResponse {
                outcome: OutcomeKind::Complete,
                data: json!({
                    "subcommand": "install",
                    "platform": host_os.as_str(),
                    "install_root": install_root.to_string_lossy().to_string(),
                    "assets": installed_assets,
                    "installs_browser_extension": false,
                }),
                warnings: vec![
                    "bridge install registers host assets only; browser extension install remains manual".to_owned(),
                ],
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

            let helper_path = custom_url_helper_path_for(host_os, &install_root);
            if helper_path.exists() {
                match fs::remove_file(&helper_path) {
                    Ok(()) => removed.push(helper_path.to_string_lossy().to_string()),
                    Err(err) => {
                        return error_response(format!(
                            "failed to remove custom URL helper {}: {err}",
                            helper_path.display()
                        ));
                    }
                }
            } else {
                missing.push(helper_path.to_string_lossy().to_string());
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

fn handle_review(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    if parsed.provider != "opencode" && parsed.provider != "codex" {
        return blocked_response(
            format!(
                "provider '{}' is not supported for rr review in this slice",
                parsed.provider
            ),
            vec![
                "use --provider opencode for tier-b CLI continuity in 0.1.0".to_owned(),
                "use --provider codex for bounded tier-a start/reseed support".to_owned(),
            ],
            json!({
                "provider": parsed.provider,
                "supported_providers": ["opencode", "codex"],
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
                "provider_capability": {
                    "provider": parsed.provider,
                    "tier": provider_tier(&parsed.provider),
                    "supports": {
                        "review_start": true,
                        "resume_reseed": parsed.provider == "codex" || parsed.provider == "opencode",
                        "resume_reopen": parsed.provider == "opencode",
                        "return": parsed.provider == "opencode",
                    }
                }
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

    let intent = launch_intent(LaunchAction::StartReview, runtime);
    let (session_locator, session_path, continuity_quality, warnings) =
        match parsed.provider.as_str() {
            "opencode" => {
                let adapter = OpenCodeAdapter::with_binary(runtime.opencode_bin.clone());
                let linkage = match adapter.link_session(&target, &intent, None, None) {
                    Ok(linkage) => linkage,
                    Err(err) => {
                        return blocked_response(
                            format!("failed to start OpenCode session: {err}"),
                            vec!["verify OpenCode is installed and reachable".to_owned()],
                            json!({"reason_code": "opencode_start_failed"}),
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
                        return blocked_response(
                            format!("failed to start Codex session: {err}"),
                            vec!["verify Codex CLI is installed and reachable".to_owned()],
                            json!({"reason_code": "codex_start_failed"}),
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
            _ => unreachable!("provider validated above"),
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
        Err(err) => return error_response(format!("failed to serialize ResumeBundle: {err}")),
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
                        return error_response(
                            "failed to persist ResumeBundle: duplicate digest detected but no stored artifact could be resolved".to_owned(),
                        );
                    }
                    Err(lookup_err) => {
                        return error_response(format!(
                            "failed to persist ResumeBundle: duplicate digest lookup failed: {lookup_err}"
                        ));
                    }
                }
            }
            Err(err) => return error_response(format!("failed to persist ResumeBundle: {err}")),
        },
        Err(err) => {
            return error_response(format!(
                "failed to resolve existing ResumeBundle artifact by digest: {err}"
            ));
        }
    };

    if let Err(err) = store.create_review_session(CreateReviewSession {
        id: &session_id,
        review_target: &target,
        provider: &parsed.provider,
        session_locator: Some(&session_locator),
        resume_bundle_artifact_id: Some(&bundle_artifact_id),
        continuity_state: continuity_state_label(&continuity_quality),
        attention_state: "review_launched",
        launch_profile_id: Some(cli_config::PROFILE_ID),
    }) {
        return error_response(format!("failed to create review session: {err}"));
    }

    if let Err(err) = store.create_review_run(CreateReviewRun {
        id: &run_id,
        session_id: &session_id,
        run_kind: "review",
        repo_snapshot: &format!("{}#{}", target.repository, target.pull_request_number),
        continuity_quality: continuity_state_label(&continuity_quality),
        session_locator_artifact_id: None,
    }) {
        return error_response(format!("failed to create review run: {err}"));
    }

    if let Err(err) = store.put_session_launch_binding(CreateSessionLaunchBinding {
        id: &binding_id,
        session_id: &session_id,
        repo_locator: &target.repository,
        review_target: Some(&target),
        surface: LaunchSurface::Cli,
        launch_profile_id: Some(cli_config::PROFILE_ID),
        ui_target: Some(cli_config::UI_TARGET),
        instance_preference: Some(cli_config::INSTANCE_PREFERENCE),
        cwd: Some(runtime.cwd.to_string_lossy().as_ref()),
        worktree_root: None,
    }) {
        return error_response(format!("failed to persist launch binding: {err}"));
    }

    let outcome = if matches!(continuity_quality, ContinuityQuality::Usable) {
        OutcomeKind::Complete
    } else {
        OutcomeKind::Degraded
    };

    CommandResponse {
        outcome,
        data: json!({
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

fn handle_resume_or_refresh(
    parsed: &ParsedArgs,
    runtime: &CliRuntime,
    action: LaunchAction,
) -> CommandResponse {
    let store = match RogerStore::open(&runtime.store_root) {
        Ok(store) => store,
        Err(err) => return error_response(format!("failed to open Roger store: {err}")),
    };

    let repository = resolve_repository(parsed.repo.clone(), &runtime.cwd);
    let resolution = match store.resolve_session_reentry(ResolveSessionReentry {
        explicit_session_id: parsed.session_id.clone(),
        repository,
        pull_request_number: parsed.pr,
        source_surface: LaunchSurface::Cli,
        ui_target: Some(cli_config::UI_TARGET.to_owned()),
        instance_preference: Some(cli_config::INSTANCE_PREFERENCE.to_owned()),
    }) {
        Ok(resolution) => resolution,
        Err(err) => return error_response(format!("failed to resolve session re-entry: {err}")),
    };

    let (session, binding) = match resolution {
        SessionReentryResolution::Resolved { session, binding } => (session, binding),
        SessionReentryResolution::PickerRequired { reason, candidates } => {
            return blocked_picker_response(reason, candidates);
        }
    };

    if session.provider != "opencode" && session.provider != "codex" {
        return blocked_response(
            format!(
                "session {} uses provider '{}' which cannot be resumed by this CLI slice",
                session.id, session.provider
            ),
            vec![
                "resume/refresh is currently available for opencode and codex sessions".to_owned(),
            ],
            json!({
                "session_id": session.id,
                "provider": session.provider,
                "supported_providers": ["opencode", "codex"],
            }),
        );
    }

    let command_name = if matches!(action, LaunchAction::RefreshFindings) {
        "rr refresh"
    } else {
        "rr resume"
    };

    if parsed.robot {
        let continuity_state = session.continuity_state.to_ascii_lowercase();
        let degraded = continuity_state.contains("degraded")
            || continuity_state.contains("reseed")
            || continuity_state.contains("unusable");
        let continuity_quality = if continuity_state.contains("unusable") {
            "unusable"
        } else if degraded {
            "degraded"
        } else {
            "usable"
        };
        let inferred_resume_path =
            if session.provider == "codex" || continuity_state.contains("reseed") {
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
                "command": if matches!(action, LaunchAction::RefreshFindings) { "refresh" } else { "resume" },
                "resume_path": inferred_resume_path,
                "continuity_quality": continuity_quality,
                "continuity_state_snapshot": session.continuity_state,
            }),
            warnings: provider_support_warning(&session.provider, command_name)
                .into_iter()
                .collect(),
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
                "command": if matches!(action, LaunchAction::RefreshFindings) { "refresh" } else { "resume" },
            }),
            warnings: provider_support_warning(&session.provider, command_name)
                .into_iter()
                .collect(),
            repair_actions: Vec::new(),
            message: "resume/refresh plan generated (dry-run)".to_owned(),
        };
    }

    let intent = launch_intent(action.clone(), runtime);

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

    let (resume_path, continuity_quality, decision_reason, warnings) = match session
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
        _ => unreachable!("provider validated above"),
    };

    let run_kind = if matches!(action, LaunchAction::RefreshFindings) {
        "refresh"
    } else {
        "resume"
    };
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
        if run_kind == "refresh" {
            "refresh_requested"
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

    let repository = resolve_repository(parsed.repo.clone(), &runtime.cwd);
    let resolution = match store.resolve_session_reentry(ResolveSessionReentry {
        explicit_session_id: parsed.session_id.clone(),
        repository,
        pull_request_number: parsed.pr,
        source_surface: LaunchSurface::Cli,
        ui_target: Some(cli_config::UI_TARGET.to_owned()),
        instance_preference: Some(cli_config::INSTANCE_PREFERENCE.to_owned()),
    }) {
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
        return blocked_response(
            format!(
                "rr return is unsupported for provider '{}' in 0.1.0",
                session.provider
            ),
            vec!["rr return is only blessed on OpenCode tier-b sessions".to_owned()],
            json!({
                "session_id": session.id,
                "provider": session.provider,
                "provider_capability": {
                    "provider": session.provider,
                    "tier": provider_tier(&session.provider),
                    "supports_rr_return": false,
                    "required_tier_for_return": "tier_b",
                }
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
        return error_response(format!("failed to refresh launch binding: {err}"));
    }

    let degraded = !matches!(outcome.continuity_quality, ContinuityQuality::Usable)
        || matches!(outcome.path, OpenCodeReturnPath::ReseededSession);

    CommandResponse {
        outcome: if degraded {
            OutcomeKind::Degraded
        } else {
            OutcomeKind::Complete
        },
        data: json!({
            "session_id": outcome.session_id,
            "review_run_id": run_id,
            "provider_capability": {
                "provider": session.provider,
                "tier": provider_tier(&session.provider),
                "supports_rr_return": true,
            },
            "return_path": return_path_label(outcome.path),
            "continuity_quality": continuity_state_label(&outcome.continuity_quality),
            "decision_reason": format!("{:?}", outcome.decision.reason_code),
        }),
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
                "updated_at": entry.updated_at,
                "follow_on": {
                    "requires_explicit_session": true,
                    "resume_command": format!("rr resume --session {}", entry.session_id),
                    "refresh_command": format!("rr refresh --session {}", entry.session_id),
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
    let lookup = match store.prior_review_lookup(PriorReviewLookupQuery {
        scope_key: &format!("repo:{repository}"),
        repository: &repository,
        query_text,
        limit: limit.saturating_add(1),
        include_tentative_candidates: false,
        allow_project_scope: false,
        allow_org_scope: false,
        semantic_assets_verified: false,
        semantic_candidates: Vec::new(),
    }) {
        Ok(result) => result,
        Err(err) => return error_response(format!("failed to run prior-review lookup: {err}")),
    };

    let mut items = Vec::new();
    for hit in lookup.evidence_hits {
        items.push(json!({
            "kind": "evidence_finding",
            "id": hit.finding_id,
            "title": hit.title,
            "score": hit.fused_score,
            "locator": {
                "session_id": hit.session_id,
                "review_run_id": hit.review_run_id,
                "repository": hit.repository,
                "pull_request": hit.pull_request_number,
            },
            "snippet": hit.normalized_summary,
        }));
    }
    for hit in lookup.promoted_memory {
        items.push(json!({
            "kind": "promoted_memory",
            "id": hit.memory_id,
            "title": hit.statement,
            "score": hit.fused_score,
            "locator": {
                "scope_key": hit.scope_key,
                "memory_class": hit.memory_class,
            },
            "snippet": hit.statement,
        }));
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

    let mode = match lookup.mode {
        PriorReviewRetrievalMode::Hybrid => "hybrid",
        PriorReviewRetrievalMode::LexicalOnly => "lexical_only",
    };
    let count = items.len();
    let degraded = mode == "lexical_only" && !lookup.degraded_reasons.is_empty();
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
            "mode": mode,
            "items": items,
            "count": count,
            "truncated": truncated,
            "degraded_reasons": lookup.degraded_reasons,
            "scope_bucket": lookup.scope_bucket,
        }),
        warnings: Vec::new(),
        repair_actions: Vec::new(),
        message: format!("search completed with mode {mode}"),
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

fn handle_update(parsed: &ParsedArgs, _runtime: &CliRuntime) -> CommandResponse {
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
            }),
            warnings: Vec::new(),
            repair_actions: Vec::new(),
            message: "rr is already on the requested release".to_owned(),
        };
    }

    let recommended_command = if cfg!(target_os = "windows") {
        format!(
            "powershell -ExecutionPolicy Bypass -File scripts/release/rr-install.ps1 -Version {target_version} -Repo {repo}"
        )
    } else {
        format!("bash scripts/release/rr-install.sh --version {target_version} --repo {repo}")
    };

    let message = if parsed.dry_run {
        "rr update dry-run metadata validation complete".to_owned()
    } else {
        "rr update metadata validation complete (manual install step required)".to_owned()
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
            },
            "recommended_install_command": recommended_command,
        }),
        warnings: Vec::new(),
        repair_actions: vec!["run the recommended_install_command to apply update".to_owned()],
        message,
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
                json!({"command": "rr search --query <text> --robot", "purpose": "prior-review lookup"}),
                json!({"command": "rr bridge verify-contracts --robot", "purpose": "bridge contract drift check"}),
                json!({"command": "rr bridge pack-extension --robot", "purpose": "assemble unpacked browser sideload artifact"}),
                json!({"command": "rr bridge install --extension-id <id> --robot", "purpose": "register native host + custom-url helper assets"}),
                json!({"command": "rr bridge uninstall --robot", "purpose": "remove bridge registration assets"}),
                json!({"command": "rr robot-docs schemas --robot", "purpose": "schema inventory"}),
            ],
            "0.1.0",
        ),
        "commands" => (
            vec![
                json!({"command": "rr status", "required_formats": ["json"], "optional_formats": ["compact"]}),
                json!({"command": "rr sessions", "required_formats": ["json"], "optional_formats": ["compact"]}),
                json!({"command": "rr findings", "required_formats": ["json"], "optional_formats": ["compact"]}),
                json!({"command": "rr search", "required_formats": ["json"], "optional_formats": ["compact"]}),
                json!({"command": "rr review --dry-run", "required_formats": ["json"], "optional_formats": []}),
                json!({"command": "rr resume --dry-run", "required_formats": ["json"], "optional_formats": []}),
                json!({"command": "rr bridge export-contracts", "required_formats": ["json"], "optional_formats": []}),
                json!({"command": "rr bridge verify-contracts", "required_formats": ["json"], "optional_formats": []}),
                json!({"command": "rr bridge pack-extension", "required_formats": ["json"], "optional_formats": []}),
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
                json!({"command": "rr bridge", "schema_id": "rr.robot.bridge.v1"}),
                json!({"command": "rr findings", "schema_id": "rr.robot.findings.v1"}),
                json!({"command": "rr status", "schema_id": "rr.robot.status.v1"}),
                json!({"command": "rr refresh", "schema_id": "rr.robot.refresh.v1"}),
                json!({"command": "rr robot-docs", "schema_id": "rr.robot.robot_docs.v1"}),
            ],
            "0.1.0",
        ),
        "workflows" => (
            vec![
                json!({"name": "resume_loop", "steps": ["rr sessions --robot", "rr resume --session <id> --robot", "rr findings --session <id> --robot"]}),
                json!({"name": "search_followup", "steps": ["rr search --query <text> --robot", "rr status --session <id> --robot"]}),
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

fn handle_status(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    let store = match RogerStore::open(&runtime.store_root) {
        Ok(store) => store,
        Err(err) => return error_response(format!("failed to open Roger store: {err}")),
    };

    let repository = resolve_repository(parsed.repo.clone(), &runtime.cwd);
    let resolution = match store.resolve_session_reentry(ResolveSessionReentry {
        explicit_session_id: parsed.session_id.clone(),
        repository,
        pull_request_number: parsed.pr,
        source_surface: LaunchSurface::Cli,
        ui_target: Some(cli_config::UI_TARGET.to_owned()),
        instance_preference: Some(cli_config::INSTANCE_PREFERENCE.to_owned()),
    }) {
        Ok(resolution) => resolution,
        Err(err) => return error_response(format!("failed to resolve status context: {err}")),
    };

    let (session, _binding) = match resolution {
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
            Err(err) => return error_response(format!("failed to count needs follow up findings: {err}")),
        }
    } else {
        0
    };

    let awaiting_approval_count = if let Some(run) = latest_run.as_ref() {
        match store.count_findings_by_outbound_state(
            &session.id,
            &run.id,
            FindingOutboundState::Approved.as_str(),
        ) {
            Ok(count) => count as usize,
            Err(err) => return error_response(format!("failed to count awaiting approval findings: {err}")),
        }
    } else {
        0
    };

    let branch = infer_git_branch(&runtime.cwd);
    let provider_tier = provider_tier(&session.provider);
    let provider_warning = provider_support_warning(&session.provider, "rr status");

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
            "findings": {
                "total": findings_count,
                "needs_follow_up": needs_follow_up_count,
            },
            "drafts": {
                "awaiting_approval": awaiting_approval_count,
            },
            "continuity": {
                "tier": provider_tier,
                "resume_locator_present": session.session_locator.is_some(),
                "state": session.continuity_state,
            },
            "provider_capability": {
                "provider": session.provider,
                "tier": provider_tier,
                "supports": {
                    "status": true,
                    "findings": true,
                    "return": session.provider == "opencode",
                },
            }
        }),
        warnings: provider_warning.into_iter().collect(),
        repair_actions: Vec::new(),
        message: "status loaded".to_owned(),
    }
}

fn handle_findings(parsed: &ParsedArgs, runtime: &CliRuntime) -> CommandResponse {
    let store = match RogerStore::open(&runtime.store_root) {
        Ok(store) => store,
        Err(err) => return error_response(format!("failed to open Roger store: {err}")),
    };

    let repository = resolve_repository(parsed.repo.clone(), &runtime.cwd);
    let resolution = match store.resolve_session_reentry(ResolveSessionReentry {
        explicit_session_id: parsed.session_id.clone(),
        repository,
        pull_request_number: parsed.pr,
        source_surface: LaunchSurface::Cli,
        ui_target: Some(cli_config::UI_TARGET.to_owned()),
        instance_preference: Some(cli_config::INSTANCE_PREFERENCE.to_owned()),
    }) {
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
    let provider_tier = provider_tier(&session.provider);
    let provider_warning = provider_support_warning(&session.provider, "rr findings");

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

        items.push(json!({
            "finding_id": finding.id,
            "fingerprint": finding.fingerprint,
            "title": finding.title,
            "triage_state": finding.triage_state,
            "outbound_state": finding.outbound_state,
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
            "provider_capability": {
                "provider": session.provider,
                "tier": provider_tier,
                "supports": {
                    "findings": true,
                    "status": true,
                    "return": session.provider == "opencode",
                },
            },
        }),
        warnings: provider_warning.into_iter().collect(),
        repair_actions: Vec::new(),
        message: if count == 0 {
            "no findings available for this session".to_owned()
        } else {
            format!("loaded {count} findings")
        },
    }
}

fn render_output(parsed: &ParsedArgs, mut response: CommandResponse) -> CliRunResult {
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

fn launch_intent(action: LaunchAction, runtime: &CliRuntime) -> LaunchIntent {
    LaunchIntent {
        action,
        source_surface: Surface::Cli,
        objective: None,
        launch_profile_id: Some(cli_config::PROFILE_ID.to_owned()),
        cwd: Some(runtime.cwd.to_string_lossy().into_owned()),
        worktree_root: None,
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
    if provider == "opencode" {
        "tier_b"
    } else {
        "tier_a"
    }
}

fn provider_support_warning(provider: &str, command: &str) -> Option<String> {
    if provider == "opencode" {
        None
    } else if provider == "codex" {
        Some(format!(
            "provider '{}' has bounded support (tier-a start/reseed/raw-capture); '{}' does not support locator reopen or rr return",
            provider, command
        ))
    } else {
        Some(format!(
            "provider '{}' has bounded support (tier-a); '{}' may offer reduced continuity behavior",
            provider, command
        ))
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

fn bridge_contract_snapshot() -> &'static str {
    r#"// Generated bridge contract snapshot for extension-side typing.
// Source of truth: packages/bridge/src/lib.rs (BridgeLaunchIntent / BridgeResponse).

export type BridgeAction =
  | 'start_review'
  | 'resume_review'
  | 'show_findings'
  | 'refresh_review';

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
            "mode": data.get("mode").cloned().unwrap_or(Value::Null),
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
    " ____                              ____            _\n|  _ \\ ___   __ _  ___ _ __       |  _ \\ _____   _(_) _____      _____ _ __\n| |_) / _ \\ / _` |/ _ \\ '__| _____| |_) / _ \\ \\ / / |/ _ \\ \\ /\\ / / _ \\ '__|\n|  _ < (_) | (_| |  __/ |   |_____|  _ <  __/\\ V /| |  __/\\ V  V /  __/ |\n|_| \\_\\___/ \\__, |\\___|_|         |_| \\_\\___| \\_/ |_|\\___| \\_/\\_/ \\___|_|\n            |___/\n\nRoger Reviewer\n\nUsage:\n  rr review --pr <number> [--repo owner/repo] [--provider opencode|codex] [--dry-run] [--robot]\n  rr resume [--repo owner/repo] [--pr <number>] [--session <id>] [--dry-run] [--robot]\n  rr return [--repo owner/repo] [--pr <number>] [--session <id>] [--robot]\n  rr sessions [--repo owner/repo] [--pr <number>] [--attention <state[,state...]>] [--limit <n>] [--robot]\n  rr search --query <text> [--repo owner/repo] [--limit <n>] [--robot]\n  rr update [--repo owner/repo] [--channel stable|rc] [--version <YYYY.MM.DD[-rc.N]>] [--api-root <url>] [--download-root <url>] [--target <triple>] [--dry-run] [--robot]\n  rr bridge export-contracts [--robot]\n  rr bridge verify-contracts [--robot]\n  rr bridge pack-extension [--output-dir <path>] [--robot]\n  rr bridge install --extension-id <id> [--bridge-binary <path>] [--install-root <path>] [--robot]\n  rr bridge uninstall [--install-root <path>] [--robot]\n  rr robot-docs [guide|commands|schemas|workflows] [--robot]\n  rr findings [--repo owner/repo] [--pr <number>] [--session <id>] [--robot]\n  rr status [--repo owner/repo] [--pr <number>] [--session <id>] [--robot]\n  rr refresh [--repo owner/repo] [--pr <number>] [--session <id>] [--robot]"
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

    fn write_mock_bridge_binary(root: &Path) -> PathBuf {
        let bridge_binary = root.join("target/release/rr-bridge");
        fs::create_dir_all(
            bridge_binary
                .parent()
                .expect("bridge binary parent should exist"),
        )
        .expect("mkdir bridge binary parent");
        fs::write(&bridge_binary, b"#!/bin/sh\nexit 0\n").expect("write bridge binary");
        bridge_binary
    }

    fn parse_robot(stdout: &str) -> Value {
        serde_json::from_str(stdout).expect("robot payload")
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
        assert!(package_dir.exists());
        assert!(package_dir.join("manifest.json").exists());
        assert!(package_dir.join("src/background/main.js").exists());
        assert!(package_dir.join("SHA256SUMS").exists());
        assert!(package_dir.join("asset-manifest.json").exists());
    }

    #[test]
    fn bridge_install_fails_closed_without_extension_id() {
        let (tmp, runtime, _generated) = setup_bridge_workspace();
        let install_root = tmp.path().join("install-root");
        let bridge_binary = write_mock_bridge_binary(&runtime.cwd);

        let result = run(
            &[
                "bridge".to_owned(),
                "install".to_owned(),
                "--bridge-binary".to_owned(),
                bridge_binary.to_string_lossy().to_string(),
                "--install-root".to_owned(),
                install_root.to_string_lossy().to_string(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(result.exit_code, 3, "{}", result.stderr);
        let payload = parse_robot(&result.stdout);
        assert_eq!(payload["outcome"], "blocked");
        assert_eq!(payload["data"]["reason_code"], "extension_id_required");
    }

    #[test]
    fn bridge_install_and_uninstall_manage_assets_with_checksums() {
        let (tmp, runtime, _generated) = setup_bridge_workspace();
        let install_root = tmp.path().join("install-root");
        let bridge_binary = write_mock_bridge_binary(&runtime.cwd);

        let install = run(
            &[
                "bridge".to_owned(),
                "install".to_owned(),
                "--extension-id".to_owned(),
                "abcdefghijklmnopabcdefghijklmnop".to_owned(),
                "--bridge-binary".to_owned(),
                bridge_binary.to_string_lossy().to_string(),
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
        assert!(assets.len() >= 4);
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
        let helper_path = custom_url_helper_path_for(os, &install_root);
        assert!(helper_path.exists(), "missing {}", helper_path.display());

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
        assert!(removed.len() >= 4);

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
        assert!(
            !helper_path.exists(),
            "still present {}",
            helper_path.display()
        );
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
    }
}
