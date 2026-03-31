use roger_app_core::{
    AppError, ContinuityQuality, HarnessAdapter, LaunchAction, LaunchIntent, ResumeAttemptOutcome,
    ResumeBundle, ResumeBundleProfile, ReviewTarget, RogerCommand, RogerCommandId,
    RogerCommandInvocationSurface, RogerCommandResult, RogerCommandRouteStatus, Surface,
    now_ts, route_harness_command, safe_harness_command_bindings,
};
use roger_app_core::time;
use roger_app_core::cli_config;
use roger_config::cli_defaults::{
    DEFAULT_INSTANCE_PREFERENCE, DEFAULT_LAUNCH_PROFILE_ID, DEFAULT_OPENCODE_BIN,
    DEFAULT_UI_TARGET, ENV_OPENCODE_BIN, ENV_STORE_ROOT,
};
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
use std::collections::HashMap;
use std::path::{Path, PathBuf};
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
            Self::RobotDocs => "rr.robot.robot_docs.v1",
            Self::Findings => "rr.robot.findings.v1",
            Self::Status => "rr.robot.status.v1",
            Self::Refresh => "rr.robot.refresh.v1",
        }
    }
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
    repo: Option<String>,
    pr: Option<u64>,
    session_id: Option<String>,
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
    Partial,
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
            Self::Partial => "partial",
            Self::Degraded => "degraded",
            Self::Blocked => "blocked",
            Self::RepairNeeded => "repair_needed",
            Self::Error => "error",
        }
    }

    fn exit_code(self) -> i32 {
        match self {
            Self::Complete | Self::Empty => 0,
            Self::Partial | Self::Degraded => 5,
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
        repo: None,
        pr: None,
        session_id: None,
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
        CommandKind::RobotDocs => handle_robot_docs(parsed),
        CommandKind::Findings => handle_findings(parsed, runtime),
        CommandKind::Status => handle_status(parsed, runtime),
        CommandKind::Refresh => {
            handle_resume_or_refresh(parsed, runtime, LaunchAction::RefreshFindings)
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

    if let Err(err) = store.store_resume_bundle(&bundle_id, &bundle) {
        return error_response(format!("failed to persist ResumeBundle: {err}"));
    }

    if let Err(err) = store.create_review_session(CreateReviewSession {
        id: &session_id,
        review_target: &target,
        provider: &parsed.provider,
        session_locator: Some(&session_locator),
        resume_bundle_artifact_id: Some(&bundle_id),
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
            "resume_bundle_artifact_id": bundle_id,
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
            warnings: provider_support_warning(&session.provider, "rr resume")
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

fn handle_robot_docs(parsed: &ParsedArgs) -> CommandResponse {
    let topic = parsed.robot_docs_topic.as_deref().unwrap_or("guide");

    let (items, version) = match topic {
        "guide" => (
            vec![
                json!({"command": "rr status --robot", "purpose": "session attention snapshot"}),
                json!({"command": "rr sessions --robot", "purpose": "global session finder"}),
                json!({"command": "rr findings --robot", "purpose": "structured findings list"}),
                json!({"command": "rr search --query <text> --robot", "purpose": "prior-review lookup"}),
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
                "needs_follow_up": 0,
            },
            "drafts": {
                "awaiting_approval": 0,
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

    let items: Vec<Value> = findings
        .iter()
        .map(|finding| {
            json!({
                "finding_id": finding.id,
                "fingerprint": finding.fingerprint,
                "title": finding.title,
                "triage_state": finding.triage_state,
                "outbound_state": finding.outbound_state,
                "evidence_count": 0,
            })
        })
        .collect();

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
        warnings: vec!["session inference is ambiguous; explicit selection is required".to_owned()],
        repair_actions: vec![
            "re-run with --session <id> or pass --pr <number> for a unique match".to_owned(),
        ],
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

fn utc_timestamp() -> String {
    match ProcessCommand::new("date")
        .arg("-u")
        .arg("+%Y-%m-%dT%H:%M:%SZ")
        .output()
    {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_owned()
        }
        _ => {
            let now = now_ts();
            format!("{now}")
        }
    }
}

fn next_id(prefix: &str) -> String {
    let seq = ID_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}-{}-{seq}", now_ts())
}

fn usage_text() -> &'static str {
    "Usage:\n  rr review --pr <number> [--repo owner/repo] [--provider opencode|codex] [--dry-run] [--robot]\n  rr resume [--repo owner/repo] [--pr <number>] [--session <id>] [--dry-run] [--robot]\n  rr return [--repo owner/repo] [--pr <number>] [--session <id>] [--robot]\n  rr sessions [--repo owner/repo] [--pr <number>] [--attention <state[,state...]>] [--limit <n>] [--robot]\n  rr search --query <text> [--repo owner/repo] [--limit <n>] [--robot]\n  rr robot-docs [guide|commands|schemas|workflows] [--robot]\n  rr findings [--repo owner/repo] [--pr <number>] [--session <id>] [--robot]\n  rr status [--repo owner/repo] [--pr <number>] [--session <id>] [--robot]\n  rr refresh [--repo owner/repo] [--pr <number>] [--session <id>] [--robot]"
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
}
