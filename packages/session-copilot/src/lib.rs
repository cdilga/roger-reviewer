use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const PROVIDER_ID: &str = "copilot";
pub const DISPLAY_NAME: &str = "GitHub Copilot CLI";

pub const ENV_COPILOT_BIN: &str = "RR_COPILOT_BIN";
pub const DEFAULT_COPILOT_BIN: &str = "copilot";

pub const ENV_COPILOT_ADMISSION_GATE: &str = "RR_ENABLE_COPILOT_PROVIDER";
pub const ENV_COPILOT_SESSION_START_ARTIFACT: &str = "RR_COPILOT_SESSION_START_ARTIFACT";
pub const ENV_COPILOT_ATTEMPT_ID: &str = "RR_COPILOT_ATTEMPT_ID";
pub const ENV_COPILOT_REPOSITORY: &str = "RR_COPILOT_REPOSITORY";
pub const ENV_COPILOT_PULL_REQUEST: &str = "RR_COPILOT_PULL_REQUEST";
pub const ENV_COPILOT_WORKTREE_ROOT: &str = "RR_COPILOT_WORKTREE_ROOT";
pub const ENV_COPILOT_POLICY_PROFILE_DIGEST: &str = "RR_COPILOT_POLICY_PROFILE_DIGEST";
pub const ENV_COPILOT_HOOK_PROFILE_DIGEST: &str = "RR_COPILOT_HOOK_PROFILE_DIGEST";
pub const ENV_COPILOT_CUSTOM_INSTRUCTIONS_DIGEST: &str = "RR_COPILOT_CUSTOM_INSTRUCTIONS_DIGEST";
pub const COPILOT_ADMISSION_FEATURE_GATE: &str = "copilot_provider_admission";
pub const REVIEW_READONLY_POLICY_PROFILE_ID: &str = "review_readonly";
pub const REVIEW_READONLY_POLICY_SUMMARY: &str = "review-safe Copilot posture: read-oriented prompts only, Roger-owned audit artifacts for denials/transcript references, and provider state treated as continuity evidence only";
pub const REVIEW_READONLY_CONTINUITY_MODE: &str = "provider_state_evidence_only";
pub const REVIEW_READONLY_HOOK_CONTRACT_VERSION: &str = "copilot_review_readonly_hooks.v2";
pub const REVIEW_READONLY_INSTRUCTION_CONTRACT_VERSION: &str =
    "copilot_review_readonly_instructions.v1";
pub const REVIEW_READONLY_REQUIRED_ISOLATION_MODE: &str = "worktree";
pub const REVIEW_READONLY_ISOLATION_SOURCE: &str =
    "providers.copilot.review_readonly.isolation_mode";
pub const REVIEW_READONLY_AUDIT_ARTIFACT_TOOL_DENIAL: &str = "copilot_tool_denial";
pub const REVIEW_READONLY_AUDIT_ARTIFACT_TRANSCRIPT_REFERENCE: &str =
    "copilot_transcript_reference";

pub const COPILOT_HOME_ENV: &str = "COPILOT_HOME";
pub const SESSION_STATE_DIR_NAME: &str = "session-state";
pub const HOOK_EVENT_SCHEMA_ID: &str = "roger.copilot.hook-event.v1";
pub const VERIFICATION_SOURCE_HOOK_EVENT: &str = "roger_hook_event";
pub const VERIFICATION_SOURCE_SESSION_STATE_DIFF: &str = "copilot_session_state_diff";

const REVIEW_READONLY_DENIED_CAPABILITIES: &[&str] = &[
    "builtin_github_mcp",
    "broad_mcp_access",
    "shell_execution",
    "write_tool",
    "external_url_access",
    "provider_memory_write",
    "allow_all_mode",
    "raw_gh_write",
    "remote_delegation",
];

const REVIEW_READONLY_AUDIT_ARTIFACT_CLASSES: &[&str] = &[
    REVIEW_READONLY_AUDIT_ARTIFACT_TOOL_DENIAL,
    REVIEW_READONLY_AUDIT_ARTIFACT_TRANSCRIPT_REFERENCE,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CopilotInvocationMode {
    Start,
    Resume,
    Reseed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CopilotReviewTarget {
    pub repository: String,
    pub pull_request_number: u64,
    pub base_ref: String,
    pub head_ref: String,
    pub base_commit: String,
    pub head_commit: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CopilotInvocationContext {
    pub mode: CopilotInvocationMode,
    pub review_target: CopilotReviewTarget,
    pub worktree_root: String,
    pub policy_profile_id: String,
    pub hook_contract_version: String,
    pub instruction_contract_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch_profile_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attempt_nonce: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copilot_home: Option<String>,
    pub verification_source: String,
    pub built_in_mcp_disabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CopilotSessionLocator {
    pub provider: String,
    pub session_id: String,
    pub invocation_context_json: String,
    pub captured_at: i64,
    pub last_tested_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RogerCopilotHookEventPayload {
    pub provider: String,
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch_profile_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_result: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub artifact_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attempt_nonce: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RogerCopilotHookEvent {
    pub id: String,
    pub hook: String,
    pub payload: RogerCopilotHookEventPayload,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RogerCopilotNegativeHookCase {
    pub id: String,
    pub hook: String,
    pub payload: serde_json::Value,
    pub expected_reason_code: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RogerCopilotHookFixture {
    pub schema_id: String,
    #[serde(default)]
    pub events: Vec<RogerCopilotHookEvent>,
    #[serde(default)]
    pub negative_events: Vec<RogerCopilotNegativeHookCase>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CopilotTranscriptCaptureExpected {
    pub raw_capture_present: bool,
    pub transcript_lookup_state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CopilotTranscriptCaptureRecord {
    pub id: String,
    pub session_id: String,
    #[serde(default)]
    pub artifact_refs: Vec<String>,
    pub expected: CopilotTranscriptCaptureExpected,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CopilotTranscriptCaptureFixture {
    pub schema_id: String,
    #[serde(default)]
    pub records: Vec<CopilotTranscriptCaptureRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CopilotVerificationEnvelope {
    pub provider: String,
    pub provider_session_id: String,
    pub verification_source: String,
    pub hook: String,
    pub captured_at: i64,
    pub worktree_root: Option<String>,
    pub launch_profile_id: Option<String>,
    pub attempt_nonce: Option<String>,
    pub artifact_refs: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CopilotTranscriptCaptureResolution {
    pub raw_capture_present: bool,
    pub transcript_lookup_state: String,
    pub reason_code: Option<String>,
    pub resolved_artifact_ref: Option<String>,
    pub resolved_path: Option<PathBuf>,
}

#[derive(Debug, thiserror::Error)]
pub enum CopilotAdapterError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CopilotVerificationError {
    #[error("missing verified session id")]
    MissingVerifiedSessionId,
    #[error("hook payload schema invalid: {0}")]
    HookPayloadSchemaInvalid(String),
    #[error("unexpected provider '{0}'")]
    UnexpectedProvider(String),
    #[error("worktree root mismatch: expected '{expected}', got '{actual}'")]
    WorktreeRootMismatch { expected: String, actual: String },
    #[error("session-state discovery found no new session directories")]
    NoSessionStateDelta,
    #[error("session-state discovery found multiple new session directories: {0:?}")]
    AmbiguousSessionStateDelta(Vec<String>),
}

impl CopilotVerificationError {
    pub fn reason_code(&self) -> &'static str {
        match self {
            Self::MissingVerifiedSessionId => "missing_verified_session_id",
            Self::HookPayloadSchemaInvalid(_) => "hook_payload_schema_invalid",
            Self::UnexpectedProvider(_) => "unexpected_provider",
            Self::WorktreeRootMismatch { .. } => "worktree_root_mismatch",
            Self::NoSessionStateDelta => "missing_verified_session_id",
            Self::AmbiguousSessionStateDelta(_) => "ambiguous_session_state_delta",
        }
    }
}

pub type Result<T> = std::result::Result<T, CopilotAdapterError>;

pub fn parse_provider_identifier(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "copilot" | "github-copilot" | "github_copilot" => Some(PROVIDER_ID),
        _ => None,
    }
}

pub fn copilot_admission_gate_enabled_from_value(value: Option<&str>) -> bool {
    value.is_some_and(|raw| {
        matches!(
            raw.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on" | "enabled"
        )
    })
}

pub fn review_readonly_denied_capabilities() -> &'static [&'static str] {
    REVIEW_READONLY_DENIED_CAPABILITIES
}

pub fn review_readonly_audit_artifact_classes() -> &'static [&'static str] {
    REVIEW_READONLY_AUDIT_ARTIFACT_CLASSES
}

pub fn copilot_session_state_root(copilot_home: &Path) -> PathBuf {
    copilot_home.join(SESSION_STATE_DIR_NAME)
}

pub fn list_copilot_session_state_ids(copilot_home: &Path) -> Result<BTreeSet<String>> {
    let state_root = copilot_session_state_root(copilot_home);
    if !state_root.exists() {
        return Ok(BTreeSet::new());
    }

    let mut session_ids = BTreeSet::new();
    for entry in fs::read_dir(&state_root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            session_ids.insert(entry.file_name().to_string_lossy().into_owned());
        }
    }

    Ok(session_ids)
}

pub fn infer_new_session_state_id(
    before: &BTreeSet<String>,
    after: &BTreeSet<String>,
) -> std::result::Result<String, CopilotVerificationError> {
    let new_ids = after.difference(before).cloned().collect::<Vec<String>>();

    match new_ids.as_slice() {
        [] => Err(CopilotVerificationError::NoSessionStateDelta),
        [only] => Ok(only.clone()),
        _ => Err(CopilotVerificationError::AmbiguousSessionStateDelta(
            new_ids,
        )),
    }
}

pub fn build_start_review_command(seed_prompt: &str) -> Vec<String> {
    vec![
        DEFAULT_COPILOT_BIN.to_owned(),
        "--interactive".to_owned(),
        seed_prompt.to_owned(),
    ]
}

pub fn build_resume_command(session_id: &str) -> Vec<String> {
    vec![
        DEFAULT_COPILOT_BIN.to_owned(),
        "--resume".to_owned(),
        session_id.to_owned(),
    ]
}

pub fn build_invocation_context(
    mode: CopilotInvocationMode,
    review_target: &CopilotReviewTarget,
    worktree_root: &Path,
    launch_profile_id: Option<&str>,
    attempt_nonce: Option<&str>,
    copilot_home: Option<&Path>,
    verification_source: &str,
) -> Result<String> {
    let context = CopilotInvocationContext {
        mode,
        review_target: review_target.clone(),
        worktree_root: worktree_root.to_string_lossy().into_owned(),
        policy_profile_id: REVIEW_READONLY_POLICY_PROFILE_ID.to_owned(),
        hook_contract_version: REVIEW_READONLY_HOOK_CONTRACT_VERSION.to_owned(),
        instruction_contract_version: REVIEW_READONLY_INSTRUCTION_CONTRACT_VERSION.to_owned(),
        launch_profile_id: launch_profile_id.map(str::to_owned),
        attempt_nonce: attempt_nonce.map(str::to_owned),
        copilot_home: copilot_home.map(|path| path.to_string_lossy().into_owned()),
        verification_source: verification_source.to_owned(),
        built_in_mcp_disabled: true,
    };
    serde_json::to_string(&context).map_err(CopilotAdapterError::Serialization)
}

pub fn build_session_locator(
    review_target: &CopilotReviewTarget,
    mode: CopilotInvocationMode,
    verified_session_id: &str,
    worktree_root: &Path,
    launch_profile_id: Option<&str>,
    attempt_nonce: Option<&str>,
    copilot_home: Option<&Path>,
    verification_source: &str,
) -> Result<CopilotSessionLocator> {
    let invocation_context_json = build_invocation_context(
        mode,
        review_target,
        worktree_root,
        launch_profile_id,
        attempt_nonce,
        copilot_home,
        verification_source,
    )?;
    let now = now_ts();
    Ok(CopilotSessionLocator {
        provider: PROVIDER_ID.to_owned(),
        session_id: verified_session_id.trim().to_owned(),
        invocation_context_json,
        captured_at: now,
        last_tested_at: Some(now),
    })
}

pub fn parse_hook_fixture(document: &str) -> Result<RogerCopilotHookFixture> {
    serde_json::from_str(document).map_err(CopilotAdapterError::Serialization)
}

pub fn parse_transcript_capture_fixture(document: &str) -> Result<CopilotTranscriptCaptureFixture> {
    serde_json::from_str(document).map_err(CopilotAdapterError::Serialization)
}

pub fn resolve_transcript_capture(
    fixture_root: &Path,
    artifact_refs: &[String],
) -> Result<CopilotTranscriptCaptureResolution> {
    for artifact_ref in artifact_refs {
        let path = fixture_root.join(artifact_ref);
        if !path.is_file() {
            continue;
        }

        let raw = fs::read(&path)?;
        let text = String::from_utf8_lossy(&raw);

        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            if serde_json::from_str::<serde_json::Value>(trimmed).is_err() {
                return Ok(CopilotTranscriptCaptureResolution {
                    raw_capture_present: true,
                    transcript_lookup_state: "degraded".to_owned(),
                    reason_code: Some("transcript_payload_truncated".to_owned()),
                    resolved_artifact_ref: Some(artifact_ref.clone()),
                    resolved_path: Some(path),
                });
            }
        }

        if !text.is_empty() && !text.ends_with('\n') {
            return Ok(CopilotTranscriptCaptureResolution {
                raw_capture_present: true,
                transcript_lookup_state: "degraded".to_owned(),
                reason_code: Some("transcript_payload_truncated".to_owned()),
                resolved_artifact_ref: Some(artifact_ref.clone()),
                resolved_path: Some(path),
            });
        }

        return Ok(CopilotTranscriptCaptureResolution {
            raw_capture_present: true,
            transcript_lookup_state: "available".to_owned(),
            reason_code: None,
            resolved_artifact_ref: Some(artifact_ref.clone()),
            resolved_path: Some(path),
        });
    }

    Ok(CopilotTranscriptCaptureResolution {
        raw_capture_present: false,
        transcript_lookup_state: "missing".to_owned(),
        reason_code: Some("transcript_artifact_missing".to_owned()),
        resolved_artifact_ref: artifact_refs.first().cloned(),
        resolved_path: None,
    })
}

pub fn verification_from_hook_event(
    event: &RogerCopilotHookEvent,
    expected_worktree_root: Option<&Path>,
) -> std::result::Result<CopilotVerificationEnvelope, CopilotVerificationError> {
    verification_from_hook_payload(
        serde_json::to_value(&event.payload)
            .map_err(|err| CopilotVerificationError::HookPayloadSchemaInvalid(err.to_string()))?,
        &event.hook,
        expected_worktree_root,
    )
}

pub fn verification_from_hook_payload(
    raw_payload: serde_json::Value,
    hook: &str,
    expected_worktree_root: Option<&Path>,
) -> std::result::Result<CopilotVerificationEnvelope, CopilotVerificationError> {
    let payload: RogerCopilotHookEventPayload = serde_json::from_value(raw_payload)
        .map_err(|err| CopilotVerificationError::HookPayloadSchemaInvalid(err.to_string()))?;

    if payload.provider != PROVIDER_ID {
        return Err(CopilotVerificationError::UnexpectedProvider(
            payload.provider,
        ));
    }

    let verified_session_id = payload.session_id.trim();
    if verified_session_id.is_empty() {
        return Err(CopilotVerificationError::MissingVerifiedSessionId);
    }

    if let Some(expected_root) = expected_worktree_root {
        let actual_root = payload.worktree_root.as_deref().ok_or_else(|| {
            CopilotVerificationError::HookPayloadSchemaInvalid(
                "missing worktree_root for verification".to_owned(),
            )
        })?;
        let expected_root = expected_root.to_string_lossy().into_owned();
        if actual_root != expected_root {
            return Err(CopilotVerificationError::WorktreeRootMismatch {
                expected: expected_root,
                actual: actual_root.to_owned(),
            });
        }
    }

    Ok(CopilotVerificationEnvelope {
        provider: payload.provider,
        provider_session_id: verified_session_id.to_owned(),
        verification_source: VERIFICATION_SOURCE_HOOK_EVENT.to_owned(),
        hook: hook.to_owned(),
        captured_at: now_ts(),
        worktree_root: payload.worktree_root,
        launch_profile_id: payload.launch_profile_id,
        attempt_nonce: payload.attempt_nonce,
        artifact_refs: payload.artifact_refs,
    })
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

// --- User-level hook installation -----------------------------------------
//
// GitHub Copilot CLI only honors repo-level `.github/hooks/*.json` once the
// file is merged into the reviewed repo's default branch, which Roger cannot
// require of arbitrary review targets. Roger therefore installs its hook
// assets at the user level (`$COPILOT_HOME/hooks`), where they run for every
// session and no-op unless Roger's launch env (the session-start artifact
// path) is present.

pub const USER_LEVEL_HOOK_CONFIG_NAME: &str = "roger-review.json";
pub const USER_LEVEL_HOOK_SCRIPT_DIR_NAME: &str = "roger";

const USER_LEVEL_HOOK_SCRIPTS: &[(&str, &str)] = &[
    (
        "common.sh",
        include_str!("../../../scripts/copilot-hooks/common.sh"),
    ),
    (
        "session-start.sh",
        include_str!("../../../scripts/copilot-hooks/session-start.sh"),
    ),
    (
        "user-prompt.sh",
        include_str!("../../../scripts/copilot-hooks/user-prompt.sh"),
    ),
    (
        "pre-tool-use.sh",
        include_str!("../../../scripts/copilot-hooks/pre-tool-use.sh"),
    ),
    (
        "post-tool-use.sh",
        include_str!("../../../scripts/copilot-hooks/post-tool-use.sh"),
    ),
    (
        "agent-stop.sh",
        include_str!("../../../scripts/copilot-hooks/agent-stop.sh"),
    ),
    (
        "session-end.sh",
        include_str!("../../../scripts/copilot-hooks/session-end.sh"),
    ),
    (
        "session-start.ps1",
        include_str!("../../../scripts/copilot-hooks/session-start.ps1"),
    ),
    (
        "user-prompt.ps1",
        include_str!("../../../scripts/copilot-hooks/user-prompt.ps1"),
    ),
    (
        "pre-tool-use.ps1",
        include_str!("../../../scripts/copilot-hooks/pre-tool-use.ps1"),
    ),
    (
        "post-tool-use.ps1",
        include_str!("../../../scripts/copilot-hooks/post-tool-use.ps1"),
    ),
    (
        "agent-stop.ps1",
        include_str!("../../../scripts/copilot-hooks/agent-stop.ps1"),
    ),
    (
        "session-end.ps1",
        include_str!("../../../scripts/copilot-hooks/session-end.ps1"),
    ),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserLevelHookState {
    Installed,
    Missing,
    Stale,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserLevelHookStatus {
    pub state: UserLevelHookState,
    pub config_path: String,
    pub script_dir: String,
    pub stale_entries: Vec<String>,
}

pub fn user_level_hooks_dir(copilot_home: &Path) -> PathBuf {
    copilot_home.join("hooks")
}

fn user_level_hook_event(script_dir: &Path, stem: &str, timeout_sec: u32) -> serde_json::Value {
    serde_json::json!([{
        "type": "command",
        "bash": script_dir.join(format!("{stem}.sh")).to_string_lossy(),
        "powershell": script_dir.join(format!("{stem}.ps1")).to_string_lossy(),
        "cwd": ".",
        "timeoutSec": timeout_sec,
    }])
}

pub fn user_level_hook_config_json(script_dir: &Path) -> String {
    let config = serde_json::json!({
        "version": 1,
        "hooks": {
            "sessionStart": user_level_hook_event(script_dir, "session-start", 15),
            "userPromptSubmitted": user_level_hook_event(script_dir, "user-prompt", 10),
            "preToolUse": user_level_hook_event(script_dir, "pre-tool-use", 10),
            "postToolUse": user_level_hook_event(script_dir, "post-tool-use", 10),
            "agentStop": user_level_hook_event(script_dir, "agent-stop", 10),
            "sessionEnd": user_level_hook_event(script_dir, "session-end", 10),
        },
    });
    let mut rendered = serde_json::to_string_pretty(&config).unwrap_or_default();
    rendered.push('\n');
    rendered
}

pub fn verify_user_level_hooks(copilot_home: &Path) -> UserLevelHookStatus {
    let hooks_dir = user_level_hooks_dir(copilot_home);
    let config_path = hooks_dir.join(USER_LEVEL_HOOK_CONFIG_NAME);
    let script_dir = hooks_dir.join(USER_LEVEL_HOOK_SCRIPT_DIR_NAME);
    let mut stale_entries = Vec::new();
    let mut missing = false;

    match fs::read_to_string(&config_path) {
        Ok(current) if current == user_level_hook_config_json(&script_dir) => {}
        Ok(_) => stale_entries.push(USER_LEVEL_HOOK_CONFIG_NAME.to_owned()),
        Err(_) => missing = true,
    }
    for (name, expected) in USER_LEVEL_HOOK_SCRIPTS {
        match fs::read_to_string(script_dir.join(name)) {
            Ok(current) if current == *expected => {}
            Ok(_) => stale_entries.push((*name).to_owned()),
            Err(_) => missing = true,
        }
    }

    let state = if missing {
        UserLevelHookState::Missing
    } else if stale_entries.is_empty() {
        UserLevelHookState::Installed
    } else {
        UserLevelHookState::Stale
    };
    UserLevelHookStatus {
        state,
        config_path: config_path.to_string_lossy().into_owned(),
        script_dir: script_dir.to_string_lossy().into_owned(),
        stale_entries,
    }
}

pub fn install_user_level_hooks(copilot_home: &Path) -> Result<UserLevelHookStatus> {
    let hooks_dir = user_level_hooks_dir(copilot_home);
    let script_dir = hooks_dir.join(USER_LEVEL_HOOK_SCRIPT_DIR_NAME);
    fs::create_dir_all(&script_dir)?;

    for (name, contents) in USER_LEVEL_HOOK_SCRIPTS {
        let path = script_dir.join(name);
        let current = fs::read_to_string(&path).ok();
        if current.as_deref() != Some(*contents) {
            fs::write(&path, contents)?;
        }
        #[cfg(unix)]
        if name.ends_with(".sh") {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
        }
    }

    let config_path = hooks_dir.join(USER_LEVEL_HOOK_CONFIG_NAME);
    let expected_config = user_level_hook_config_json(&script_dir);
    let current_config = fs::read_to_string(&config_path).ok();
    if current_config.as_deref() != Some(expected_config.as_str()) {
        fs::write(&config_path, expected_config)?;
    }

    Ok(verify_user_level_hooks(copilot_home))
}

#[cfg(test)]
mod tests {
    use super::{
        COPILOT_ADMISSION_FEATURE_GATE, COPILOT_HOME_ENV, CopilotInvocationContext,
        CopilotInvocationMode, CopilotReviewTarget, DEFAULT_COPILOT_BIN, DISPLAY_NAME,
        ENV_COPILOT_ADMISSION_GATE, ENV_COPILOT_ATTEMPT_ID, ENV_COPILOT_CUSTOM_INSTRUCTIONS_DIGEST,
        ENV_COPILOT_HOOK_PROFILE_DIGEST, ENV_COPILOT_POLICY_PROFILE_DIGEST,
        ENV_COPILOT_PULL_REQUEST, ENV_COPILOT_REPOSITORY, ENV_COPILOT_SESSION_START_ARTIFACT,
        ENV_COPILOT_WORKTREE_ROOT, HOOK_EVENT_SCHEMA_ID, PROVIDER_ID,
        REVIEW_READONLY_AUDIT_ARTIFACT_TOOL_DENIAL,
        REVIEW_READONLY_AUDIT_ARTIFACT_TRANSCRIPT_REFERENCE, REVIEW_READONLY_CONTINUITY_MODE,
        REVIEW_READONLY_HOOK_CONTRACT_VERSION, REVIEW_READONLY_INSTRUCTION_CONTRACT_VERSION,
        REVIEW_READONLY_POLICY_PROFILE_ID, REVIEW_READONLY_REQUIRED_ISOLATION_MODE,
        RogerCopilotNegativeHookCase, build_resume_command, build_session_locator,
        build_start_review_command, copilot_admission_gate_enabled_from_value,
        copilot_session_state_root, infer_new_session_state_id, list_copilot_session_state_ids,
        parse_hook_fixture, parse_provider_identifier, parse_transcript_capture_fixture,
        resolve_transcript_capture, review_readonly_audit_artifact_classes,
        review_readonly_denied_capabilities, verification_from_hook_event,
        verification_from_hook_payload,
    };
    use serde::Deserialize;
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[derive(Debug, Deserialize)]
    struct LaunchResumeFixture {
        schema_id: String,
        cases: Vec<LaunchResumeCase>,
    }

    #[derive(Debug, Deserialize)]
    struct LaunchResumeCase {
        id: String,
        command: Vec<String>,
        expected: LaunchResumeExpected,
    }

    #[derive(Debug, Deserialize)]
    struct LaunchResumeExpected {
        verified_session_id: String,
        continuity_quality: String,
    }

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("packages parent")
            .parent()
            .expect("workspace root")
            .to_path_buf()
    }

    fn read_fixture(path: &str) -> String {
        fs::read_to_string(workspace_root().join(path)).expect("read fixture")
    }

    fn sample_target(pr_number: u64) -> CopilotReviewTarget {
        CopilotReviewTarget {
            repository: "owner/repo".to_owned(),
            pull_request_number: pr_number,
            base_ref: "main".to_owned(),
            head_ref: "feature".to_owned(),
            base_commit: "aaa".to_owned(),
            head_commit: "bbb".to_owned(),
        }
    }

    #[test]
    fn parses_canonical_and_alias_provider_identifiers() {
        assert_eq!(parse_provider_identifier("copilot"), Some(PROVIDER_ID));
        assert_eq!(
            parse_provider_identifier("GitHub-Copilot"),
            Some(PROVIDER_ID)
        );
        assert_eq!(
            parse_provider_identifier(" github_copilot "),
            Some(PROVIDER_ID)
        );
        assert_eq!(parse_provider_identifier("codex"), None);
    }

    #[test]
    fn parses_admission_gate_truthy_values() {
        for value in ["1", "true", "TRUE", " yes ", "On", "enabled"] {
            assert!(
                copilot_admission_gate_enabled_from_value(Some(value)),
                "expected value {value:?} to be truthy"
            );
        }
    }

    #[test]
    fn parses_admission_gate_falsey_values() {
        for value in [
            None,
            Some(""),
            Some("0"),
            Some("false"),
            Some("no"),
            Some("off"),
        ] {
            assert!(
                !copilot_admission_gate_enabled_from_value(value),
                "expected value {value:?} to be falsey"
            );
        }
    }

    #[test]
    fn constants_match_expected_contract_words() {
        assert_eq!(PROVIDER_ID, "copilot");
        assert_eq!(DISPLAY_NAME, "GitHub Copilot CLI");
        assert_eq!(DEFAULT_COPILOT_BIN, "copilot");
        assert_eq!(ENV_COPILOT_ADMISSION_GATE, "RR_ENABLE_COPILOT_PROVIDER");
        assert_eq!(
            ENV_COPILOT_SESSION_START_ARTIFACT,
            "RR_COPILOT_SESSION_START_ARTIFACT"
        );
        assert_eq!(ENV_COPILOT_ATTEMPT_ID, "RR_COPILOT_ATTEMPT_ID");
        assert_eq!(ENV_COPILOT_REPOSITORY, "RR_COPILOT_REPOSITORY");
        assert_eq!(ENV_COPILOT_PULL_REQUEST, "RR_COPILOT_PULL_REQUEST");
        assert_eq!(ENV_COPILOT_WORKTREE_ROOT, "RR_COPILOT_WORKTREE_ROOT");
        assert_eq!(
            ENV_COPILOT_POLICY_PROFILE_DIGEST,
            "RR_COPILOT_POLICY_PROFILE_DIGEST"
        );
        assert_eq!(
            ENV_COPILOT_HOOK_PROFILE_DIGEST,
            "RR_COPILOT_HOOK_PROFILE_DIGEST"
        );
        assert_eq!(
            ENV_COPILOT_CUSTOM_INSTRUCTIONS_DIGEST,
            "RR_COPILOT_CUSTOM_INSTRUCTIONS_DIGEST"
        );
        assert_eq!(COPILOT_ADMISSION_FEATURE_GATE, "copilot_provider_admission");
        assert_eq!(COPILOT_HOME_ENV, "COPILOT_HOME");
        assert_eq!(HOOK_EVENT_SCHEMA_ID, "roger.copilot.hook-event.v1");
        assert_eq!(REVIEW_READONLY_POLICY_PROFILE_ID, "review_readonly");
        assert_eq!(
            REVIEW_READONLY_CONTINUITY_MODE,
            "provider_state_evidence_only"
        );
        assert_eq!(
            REVIEW_READONLY_HOOK_CONTRACT_VERSION,
            "copilot_review_readonly_hooks.v2"
        );
        assert_eq!(
            REVIEW_READONLY_INSTRUCTION_CONTRACT_VERSION,
            "copilot_review_readonly_instructions.v1"
        );
        assert_eq!(REVIEW_READONLY_REQUIRED_ISOLATION_MODE, "worktree");
    }

    #[test]
    fn review_readonly_lists_explicit_denials_and_audit_classes() {
        let denied = review_readonly_denied_capabilities();
        assert!(denied.contains(&"write_tool"));
        assert!(denied.contains(&"raw_gh_write"));
        assert!(denied.contains(&"external_url_access"));

        let audit_classes = review_readonly_audit_artifact_classes();
        assert_eq!(
            audit_classes,
            &[
                REVIEW_READONLY_AUDIT_ARTIFACT_TOOL_DENIAL,
                REVIEW_READONLY_AUDIT_ARTIFACT_TRANSCRIPT_REFERENCE,
            ]
        );
    }

    #[test]
    fn launch_and_resume_commands_match_fixture_contract() {
        let fixture: LaunchResumeFixture = serde_json::from_str(&read_fixture(
            "tests/fixtures/fixture_copilot_launch_resume/copilot_launch_resume_cases.json",
        ))
        .expect("parse launch resume fixture");
        assert_eq!(fixture.schema_id, "fixture-copilot-launch-resume.v1");
        assert_eq!(fixture.cases.len(), 3);

        let launch_case = fixture
            .cases
            .iter()
            .find(|case| case.id == "launch_review_verified_session_id")
            .expect("launch case");
        assert_eq!(
            launch_case.command,
            build_start_review_command("<roger-seed-prompt>")
        );
        assert_eq!(launch_case.expected.verified_session_id, "cp-live-001");
        assert_eq!(launch_case.expected.continuity_quality, "degraded");

        let reseed_case = fixture
            .cases
            .iter()
            .find(|case| case.id == "resume_stale_locator_reseed")
            .expect("reseed case");
        assert_eq!(
            reseed_case.command,
            build_start_review_command("<roger-reseed-prompt>")
        );
        assert_eq!(reseed_case.expected.verified_session_id, "cp-reseed-002");
        assert_eq!(reseed_case.expected.continuity_quality, "degraded");

        let reopen_case = fixture
            .cases
            .iter()
            .find(|case| case.id == "resume_live_locator_reopen")
            .expect("reopen case");
        assert_eq!(reopen_case.command, build_resume_command("cp-live-001"));
        assert_eq!(reopen_case.expected.verified_session_id, "cp-live-001");
        assert_eq!(reopen_case.expected.continuity_quality, "usable");
        assert_eq!(
            build_resume_command("cp-live-001"),
            vec![
                "copilot".to_owned(),
                "--resume".to_owned(),
                "cp-live-001".to_owned()
            ]
        );
    }

    #[test]
    fn hook_stream_fixture_yields_verified_session_start_event() {
        let fixture = parse_hook_fixture(&read_fixture(
            "tests/fixtures/fixture_copilot_hook_stream/copilot_hook_events.json",
        ))
        .expect("parse hook stream fixture");
        assert_eq!(fixture.schema_id, "fixture-copilot-hook-stream.v1");

        let event = fixture
            .events
            .iter()
            .find(|event| event.id == "session_start_verified_id")
            .expect("session start event");

        let expected_root = PathBuf::from("/tmp/repo");
        let envelope = verification_from_hook_event(event, Some(expected_root.as_path()))
            .expect("verified start envelope");
        assert_eq!(envelope.provider, PROVIDER_ID);
        assert_eq!(envelope.provider_session_id, "cp-live-001");
        assert_eq!(envelope.worktree_root.as_deref(), Some("/tmp/repo"));
        assert_eq!(
            envelope.launch_profile_id.as_deref(),
            Some("profile-open-pr")
        );
        assert_eq!(envelope.hook, "session-start");
    }

    #[test]
    fn transcript_capture_fixture_resolves_available_missing_and_degraded_states() {
        let fixture = parse_transcript_capture_fixture(&read_fixture(
            "tests/fixtures/fixture_copilot_transcript_capture/copilot_transcript_refs.json",
        ))
        .expect("parse transcript capture fixture");
        assert_eq!(fixture.schema_id, "fixture-copilot-transcript-capture.v1");

        let fixture_root =
            workspace_root().join("tests/fixtures/fixture_copilot_transcript_capture");
        for record in fixture.records {
            let resolution = resolve_transcript_capture(&fixture_root, &record.artifact_refs)
                .expect("resolve transcript fixture");
            assert_eq!(
                resolution.raw_capture_present, record.expected.raw_capture_present,
                "unexpected raw-capture presence for {}",
                record.id
            );
            assert_eq!(
                resolution.transcript_lookup_state, record.expected.transcript_lookup_state,
                "unexpected transcript state for {}",
                record.id
            );
            assert_eq!(
                resolution.reason_code, record.expected.reason_code,
                "unexpected reason code for {}",
                record.id
            );
        }
    }

    #[test]
    fn negative_hook_cases_surface_expected_reason_codes() {
        let fixture = parse_hook_fixture(&read_fixture(
            "tests/fixtures/fixture_copilot_hook_stream/copilot_hook_events.json",
        ))
        .expect("parse hook stream fixture");

        for RogerCopilotNegativeHookCase {
            id,
            hook,
            payload,
            expected_reason_code,
        } in fixture.negative_events
        {
            let err = verification_from_hook_payload(payload, &hook, None)
                .expect_err("negative hook fixture should fail");
            assert_eq!(
                err.reason_code(),
                expected_reason_code,
                "negative hook case {id} returned unexpected reason code"
            );
        }
    }

    #[test]
    fn session_state_helpers_detect_single_new_session_directory() {
        let temp = tempdir().expect("tempdir");
        let copilot_home = temp.path().join("copilot-home");
        let state_root = copilot_session_state_root(&copilot_home);
        fs::create_dir_all(state_root.join("cp-existing-001")).expect("create existing session");

        let before = list_copilot_session_state_ids(&copilot_home).expect("list before");
        assert_eq!(before, BTreeSet::from([String::from("cp-existing-001")]));

        fs::create_dir_all(state_root.join("cp-live-002")).expect("create new session");

        let after = list_copilot_session_state_ids(&copilot_home).expect("list after");
        assert_eq!(
            infer_new_session_state_id(&before, &after).expect("discover one new session"),
            "cp-live-002"
        );
    }

    #[test]
    fn session_state_helpers_reject_ambiguous_new_directories() {
        let before = BTreeSet::from([String::from("cp-existing-001")]);
        let after = BTreeSet::from([
            String::from("cp-existing-001"),
            String::from("cp-live-002"),
            String::from("cp-live-003"),
        ]);

        let err = infer_new_session_state_id(&before, &after)
            .expect_err("multiple new directories should fail");
        assert_eq!(err.reason_code(), "ambiguous_session_state_delta");
    }

    #[test]
    fn session_locator_captures_repo_root_policy_and_verification_source() {
        let temp = tempdir().expect("tempdir");
        let review_target = sample_target(42);
        let worktree_root = temp.path().join("repo");
        let copilot_home = temp.path().join("copilot-home");
        let locator = build_session_locator(
            &review_target,
            CopilotInvocationMode::Start,
            "cp-live-001",
            &worktree_root,
            Some("profile-open-pr"),
            Some("attempt-123"),
            Some(&copilot_home),
            "roger_hook_event",
        )
        .expect("build locator");

        assert_eq!(locator.provider, "copilot");
        assert_eq!(locator.session_id, "cp-live-001");
        let context: CopilotInvocationContext =
            serde_json::from_str(&locator.invocation_context_json).expect("parse locator context");
        assert_eq!(context.mode, CopilotInvocationMode::Start);
        assert_eq!(context.review_target, review_target);
        assert_eq!(
            context.worktree_root,
            worktree_root.to_string_lossy().as_ref()
        );
        assert_eq!(context.policy_profile_id, REVIEW_READONLY_POLICY_PROFILE_ID);
        assert_eq!(
            context.hook_contract_version,
            REVIEW_READONLY_HOOK_CONTRACT_VERSION
        );
        assert_eq!(
            context.instruction_contract_version,
            REVIEW_READONLY_INSTRUCTION_CONTRACT_VERSION
        );
        assert_eq!(
            context.launch_profile_id.as_deref(),
            Some("profile-open-pr")
        );
        assert_eq!(context.attempt_nonce.as_deref(), Some("attempt-123"));
        assert_eq!(
            context.copilot_home.as_deref(),
            Some(copilot_home.to_string_lossy().as_ref())
        );
        assert_eq!(context.verification_source, "roger_hook_event");
        assert!(context.built_in_mcp_disabled);
    }

    #[test]
    fn user_level_hook_install_is_idempotent_and_verifiable() {
        let temp = tempfile::tempdir().expect("tempdir");
        let home = temp.path().join("copilot-home");

        let missing = super::verify_user_level_hooks(&home);
        assert_eq!(missing.state, super::UserLevelHookState::Missing);

        let installed = super::install_user_level_hooks(&home).expect("install hooks");
        assert_eq!(installed.state, super::UserLevelHookState::Installed);
        assert!(installed.stale_entries.is_empty());

        let config_path = std::path::PathBuf::from(&installed.config_path);
        let config: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&config_path).expect("read config"))
                .expect("parse config");
        assert_eq!(config["version"], 1);
        let session_start = &config["hooks"]["sessionStart"][0];
        assert!(
            session_start["bash"]
                .as_str()
                .expect("bash path")
                .ends_with("session-start.sh")
        );

        let reinstalled = super::install_user_level_hooks(&home).expect("reinstall hooks");
        assert_eq!(reinstalled.state, super::UserLevelHookState::Installed);
    }

    #[test]
    fn user_level_hook_verify_reports_stale_script() {
        let temp = tempfile::tempdir().expect("tempdir");
        let home = temp.path().join("copilot-home");
        let installed = super::install_user_level_hooks(&home).expect("install hooks");
        let script_dir = std::path::PathBuf::from(&installed.script_dir);
        std::fs::write(script_dir.join("session-start.sh"), "#!/bin/sh\nexit 0\n")
            .expect("tamper script");

        let status = super::verify_user_level_hooks(&home);
        assert_eq!(status.state, super::UserLevelHookState::Stale);
        assert_eq!(status.stale_entries, vec!["session-start.sh".to_owned()]);
    }
}
