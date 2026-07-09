//! Roger browser-to-local launch bridge.
//!
//! Implements the daemonless bridge for browser extension → local Roger
//! handoff. `0.1.0` bridge support is Native Messaging only:
//!
//! **Native Messaging**: Chrome/Edge/Brave Native Messaging host that receives
//! structured launch intents and returns bounded readback-only responses. No
//! persistent daemon.
//!
//! Design constraints (per AGENTS.md / canonical plan):
//! - No persistent daemon or local HTTP/WebSocket server
//! - Missing local Roger state fails closed with explicit guidance
//! - No mutation or approval side-effects through the bridge
//! - Bridge host is a separate binary entrypoint, not the TUI

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    #[error("roger binary not found at {path}")]
    RogerNotFound { path: String },
    #[error("native messaging read error: {0}")]
    NativeMessagingReadError(String),
    #[error("native messaging write error: {0}")]
    NativeMessagingWriteError(String),
    #[error("invalid bridge request: {0}")]
    InvalidRequest(String),
    #[error("local roger state missing: {detail}")]
    LocalStateMissing { detail: String },
    #[error("bridge mode not supported: {mode}")]
    UnsupportedMode { mode: String },
    #[error("io error: {0}")]
    IoError(#[from] io::Error),
    #[error("json error: {0}")]
    JsonError(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, BridgeError>;

// ---------------------------------------------------------------------------
// Native Messaging protocol
// ---------------------------------------------------------------------------

/// A launch intent received from the browser extension via Native Messaging.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeLaunchIntent {
    /// The action the user wants: "start_review", "resume_review", "show_findings".
    pub action: String,
    /// GitHub owner.
    pub owner: String,
    /// GitHub repo name.
    pub repo: String,
    /// PR number.
    pub pr_number: u64,
    /// Optional branch hint from the extension.
    pub head_ref: Option<String>,
    /// Optional explicit instance name.
    pub instance: Option<String>,
    /// Optional explicit session id. When present on a `resume_review` intent
    /// the bridge dispatches `rr resume --session <id>` so a specific candidate
    /// is reopened instead of relying on the CLI's auto-selection heuristic.
    #[serde(default)]
    pub session_id: Option<String>,
    /// Optional browser extension runtime ID for identity-registration events.
    #[serde(default)]
    pub extension_id: Option<String>,
    /// Optional browser label for identity-registration events.
    #[serde(default)]
    pub browser: Option<String>,
    /// Bounded-local-parity action inputs (deliberate asymmetry 1: these initiate
    /// local mutations or read-only mirrors, never post/approve).
    /// Target finding id for `triage_finding` / `request_clarification`.
    #[serde(default)]
    pub finding_id: Option<String>,
    /// Requested triage state for `triage_finding`
    /// (accepted|ignored|needs_follow_up|resolved).
    #[serde(default)]
    pub state: Option<String>,
    /// Target outbound draft id for `revise_draft`.
    #[serde(default)]
    pub draft_id: Option<String>,
    /// Free-text body for `revise_draft` (new draft body) or
    /// `request_clarification` (clarification prompt).
    #[serde(default)]
    pub body: Option<String>,
    /// Search text for the read-only `search` action.
    #[serde(default)]
    pub query: Option<String>,
}

/// Response sent back to the extension via Native Messaging.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeStatusSnapshot {
    pub schema_id: String,
    pub outcome: String,
    pub generated_at: String,
    pub session_id: String,
    pub attention_state: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BridgeFailureKind {
    PreflightFailed,
    CliSpawnFailed,
    RobotSchemaMismatch,
    MissingSessionId,
    CliOutcomeNotSafe,
    /// The resume command could not pick a single session and returned a
    /// disambiguation picker. This is NOT an unsafe outcome: it carries a
    /// bounded `candidates` list the extension renders so the user can pick a
    /// session and resume it explicitly.
    PickerRequired,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeResponse {
    pub ok: bool,
    pub action: String,
    pub message: String,
    /// If the launch succeeded, the session ID.
    pub session_id: Option<String>,
    /// If the launch failed, structured guidance for the user.
    pub guidance: Option<String>,
    /// Canonical Roger attention-state mirror derived from `rr status --robot`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attention_state: Option<String>,
    /// Timestamp from the canonical `rr status --robot` envelope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated_at: Option<String>,
    /// Bounded status snapshot used for truthful extension-side mirroring.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<BridgeStatusSnapshot>,
    /// Launch command outcome when the bridge reached a canonical robot envelope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch_outcome: Option<String>,
    /// Bounded bridge failure vocabulary for extension-side launch handling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_kind: Option<BridgeFailureKind>,
    /// Bounded findings mirror relayed only for `show_findings`: the
    /// `rr findings --robot` envelope's `{items, count}` plus its warnings, so
    /// the extension staging view can render real findings without pretending
    /// to be a source of truth.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub findings: Option<serde_json::Value>,
    /// Canonical Roger warnings forwarded verbatim from the launch command's
    /// robot envelope (resume auto-selection notice, provider-support caveats,
    /// etc). Previously the bridge discarded these; the extension now renders
    /// them so silent auto-picks become visible.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    /// Bounded disambiguation candidates relayed for a `resume_review` picker
    /// (the `blocked_picker_response` envelope's `data.candidates`). Present only
    /// with `failure_kind = picker_required`; the extension renders a per-session
    /// resume affordance instead of a generic error.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidates: Option<serde_json::Value>,
    /// True when the resume command auto-selected a session from multiple
    /// candidates (detected from the leading "auto-selected session" warning).
    /// Lets the extension surface a visible "choose another" notice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_selected_session: Option<bool>,
    /// Bounded local-parity drafts mirror relayed for `show_drafts`: the
    /// `rr findings --robot` items carrying per-finding `outbound_state` and
    /// `outbound_detail` (draft_id, draft_batch_id) so the extension draft
    /// staging view can render batches and offer edit-as-revision. Read-only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drafts: Option<serde_json::Value>,
    /// Read-only search mirror relayed for `search`: the `rr search --robot`
    /// envelope's `data` (matches/count) so the extension can render bounded
    /// prior-review search results.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_results: Option<serde_json::Value>,
    /// Read-only timeline mirror relayed for `timeline`: the `rr timeline
    /// --robot` envelope's `data`. Relays the `rr timeline` command (landed 2026-07-09) in
    /// the CLI (a parallel workstream); until then the dispatch fails closed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeline: Option<serde_json::Value>,
    /// Clarification acknowledgement relayed for `request_clarification`: the
    /// `rr clarify --robot` envelope's `data`. Relays the `rr clarify` command (landed 2026-07-09), the
    /// landing in the CLI (a parallel workstream); until then the dispatch fails
    /// closed. Never posts to GitHub — a clarification is a local durable row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clarification_ack: Option<serde_json::Value>,
}

impl BridgeResponse {
    pub fn success(action: &str, message: &str, session_id: Option<String>) -> Self {
        Self {
            ok: true,
            action: action.to_owned(),
            message: message.to_owned(),
            session_id,
            guidance: None,
            attention_state: None,
            generated_at: None,
            status: None,
            launch_outcome: None,
            failure_kind: None,
            findings: None,
            warnings: Vec::new(),
            candidates: None,
            auto_selected_session: None,
            drafts: None,
            search_results: None,
            timeline: None,
            clarification_ack: None,
        }
    }

    pub fn success_with_status(
        action: &str,
        message: &str,
        session_id: String,
        status: BridgeStatusSnapshot,
        guidance: Option<String>,
        launch_outcome: Option<&str>,
    ) -> Self {
        Self {
            ok: true,
            action: action.to_owned(),
            message: message.to_owned(),
            attention_state: Some(status.attention_state.clone()),
            generated_at: Some(status.generated_at.clone()),
            session_id: Some(session_id),
            guidance,
            status: Some(status),
            launch_outcome: launch_outcome.map(str::to_owned),
            failure_kind: None,
            findings: None,
            warnings: Vec::new(),
            candidates: None,
            auto_selected_session: None,
            drafts: None,
            search_results: None,
            timeline: None,
            clarification_ack: None,
        }
    }

    /// Build a `resume_review` picker response: `ok = false` but NOT an unsafe
    /// outcome — it carries the bounded `candidates` list so the extension can
    /// render a per-session resume affordance instead of a generic error.
    pub fn picker_required(
        action: &str,
        message: &str,
        guidance: &str,
        candidates: serde_json::Value,
        warnings: Vec<String>,
    ) -> Self {
        Self {
            ok: false,
            action: action.to_owned(),
            message: message.to_owned(),
            session_id: None,
            guidance: Some(guidance.to_owned()),
            attention_state: None,
            generated_at: None,
            status: None,
            launch_outcome: Some("blocked".to_owned()),
            failure_kind: Some(BridgeFailureKind::PickerRequired),
            findings: None,
            warnings,
            candidates: Some(candidates),
            auto_selected_session: None,
            drafts: None,
            search_results: None,
            timeline: None,
            clarification_ack: None,
        }
    }

    pub fn failure(action: &str, message: &str, guidance: &str) -> Self {
        Self::failure_with_kind(action, message, guidance, None, None)
    }

    pub fn failure_with_kind(
        action: &str,
        message: &str,
        guidance: &str,
        failure_kind: impl Into<Option<BridgeFailureKind>>,
        launch_outcome: Option<&str>,
    ) -> Self {
        Self {
            ok: false,
            action: action.to_owned(),
            message: message.to_owned(),
            session_id: None,
            guidance: Some(guidance.to_owned()),
            attention_state: None,
            generated_at: None,
            status: None,
            launch_outcome: launch_outcome.map(str::to_owned),
            failure_kind: failure_kind.into(),
            findings: None,
            warnings: Vec::new(),
            candidates: None,
            auto_selected_session: None,
            drafts: None,
            search_results: None,
            timeline: None,
            clarification_ack: None,
        }
    }
}

/// Read a Native Messaging message from stdin.
///
/// Chrome Native Messaging protocol: 4-byte little-endian length prefix
/// followed by JSON payload.
/// A bounded status probe from the extension's companion tier. Unlike launch
/// intents it carries no action; the host answers from persisted local state
/// only and must never mutate anything.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BridgeStatusProbe {
    pub owner: String,
    pub repo: String,
    pub pr_number: u64,
}

/// The two native message families the host accepts.
#[derive(Debug, Clone, PartialEq)]
pub enum NativeBridgeMessage {
    Launch(BridgeLaunchIntent),
    StatusProbe(BridgeStatusProbe),
}

/// Read one length-prefixed native message and classify it by its `type`
/// discriminator: `roger_bridge_status` probes are read-only companion-tier
/// requests; everything else parses as a launch intent.
pub fn read_native_bridge_message<R: Read>(reader: &mut R) -> Result<NativeBridgeMessage> {
    let value = read_native_value(reader)?;
    if value.get("type").and_then(|v| v.as_str()) == Some("roger_bridge_status") {
        let probe: BridgeStatusProbe = serde_json::from_value(value).map_err(|e| {
            BridgeError::NativeMessagingReadError(format!("invalid status probe payload: {e}"))
        })?;
        return Ok(NativeBridgeMessage::StatusProbe(probe));
    }
    let intent: BridgeLaunchIntent = serde_json::from_value(value).map_err(|e| {
        BridgeError::NativeMessagingReadError(format!("invalid launch intent payload: {e}"))
    })?;
    Ok(NativeBridgeMessage::Launch(intent))
}

fn read_native_value<R: Read>(reader: &mut R) -> Result<serde_json::Value> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).map_err(|e| {
        BridgeError::NativeMessagingReadError(format!("failed to read length prefix: {e}"))
    })?;
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > 1_048_576 {
        return Err(BridgeError::NativeMessagingReadError(format!(
            "message length {len} exceeds 1MiB limit"
        )));
    }
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body).map_err(|e| {
        BridgeError::NativeMessagingReadError(format!("failed to read message body: {e}"))
    })?;
    serde_json::from_slice(&body)
        .map_err(|e| BridgeError::NativeMessagingReadError(format!("invalid message JSON: {e}")))
}

/// Wire schema id for the incremental launch-progress frames the native host
/// streams ahead of the final [`BridgeResponse`].
///
/// One-shot messaging on Edge's MV3 module service worker was torn down before
/// a slow launch settled, so the panel waited (up to 120s) with no feedback.
/// The host now emits length-prefixed progress frames — an immediate ack the
/// moment the launch parses, then a preflight-passed marker — before the final
/// response, so the extension can render loud, incremental feedback and a
/// first-frame watchdog can fail fast when the host never answers at all.
///
/// These frames are strictly additive: any frame whose `schema` equals this
/// value is progress and must not settle the extension-side launch promise; the
/// first frame whose `schema` is anything else is the final response frame.
pub const LAUNCH_PROGRESS_SCHEMA: &str = "roger.bridge.launch-progress.v1";

/// Build one launch-progress frame value for the given `stage`
/// (`host_started` immediately after parse, `preflight_ok` after preflight
/// passes). Kept here so the wire contract has a single Rust-owned source.
pub fn launch_progress_frame(stage: &str) -> serde_json::Value {
    serde_json::json!({ "schema": LAUNCH_PROGRESS_SCHEMA, "stage": stage })
}

/// Write one length-prefixed launch-progress frame to `writer` using the same
/// framing as every other native message.
pub fn write_launch_progress<W: Write>(writer: &mut W, stage: &str) -> Result<()> {
    write_native_value(writer, &launch_progress_frame(stage))
}

/// Write an arbitrary serializable native message (used for status-probe
/// replies whose shape is owned by the companion-tier readback contract).
pub fn write_native_value<W: Write>(writer: &mut W, value: &serde_json::Value) -> Result<()> {
    let json = serde_json::to_vec(value)?;
    let len = json.len() as u32;
    writer.write_all(&len.to_le_bytes()).map_err(|e| {
        BridgeError::NativeMessagingWriteError(format!("failed to write length prefix: {e}"))
    })?;
    writer.write_all(&json).map_err(|e| {
        BridgeError::NativeMessagingWriteError(format!("failed to write message body: {e}"))
    })?;
    writer.flush().map_err(|e| {
        BridgeError::NativeMessagingWriteError(format!("failed to flush message: {e}"))
    })
}

pub fn read_native_message<R: Read>(reader: &mut R) -> Result<BridgeLaunchIntent> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).map_err(|e| {
        BridgeError::NativeMessagingReadError(format!("failed to read length prefix: {e}"))
    })?;
    let len = u32::from_le_bytes(len_buf) as usize;

    if len > 1_048_576 {
        return Err(BridgeError::NativeMessagingReadError(format!(
            "message too large: {len} bytes"
        )));
    }

    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).map_err(|e| {
        BridgeError::NativeMessagingReadError(format!("failed to read message body: {e}"))
    })?;

    let intent: BridgeLaunchIntent = serde_json::from_slice(&buf)?;
    Ok(intent)
}

/// Write a Native Messaging response to stdout.
pub fn write_native_message<W: Write>(writer: &mut W, response: &BridgeResponse) -> Result<()> {
    let json = serde_json::to_vec(response)?;
    let len = json.len() as u32;
    writer.write_all(&len.to_le_bytes()).map_err(|e| {
        BridgeError::NativeMessagingWriteError(format!("failed to write length prefix: {e}"))
    })?;
    writer.write_all(&json).map_err(|e| {
        BridgeError::NativeMessagingWriteError(format!("failed to write message body: {e}"))
    })?;
    writer
        .flush()
        .map_err(|e| BridgeError::NativeMessagingWriteError(format!("failed to flush: {e}")))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Native Messaging host manifest
// ---------------------------------------------------------------------------

/// Supported browsers for Native Messaging host registration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SupportedBrowser {
    Chrome,
    Edge,
    Brave,
}

/// Supported host operating systems for bridge registration assets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SupportedOs {
    Macos,
    Windows,
    Linux,
}

impl SupportedOs {
    pub fn current() -> Option<Self> {
        if cfg!(target_os = "macos") {
            Some(Self::Macos)
        } else if cfg!(target_os = "windows") {
            Some(Self::Windows)
        } else if cfg!(target_os = "linux") {
            Some(Self::Linux)
        } else {
            None
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Macos => "macos",
            Self::Windows => "windows",
            Self::Linux => "linux",
        }
    }
}

/// A Native Messaging host manifest for browser registration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NativeHostManifest {
    pub name: String,
    pub description: String,
    pub path: String,
    #[serde(rename = "type")]
    pub host_type: String,
    pub allowed_origins: Vec<String>,
}

impl NativeHostManifest {
    /// Create a manifest for the Roger bridge host binary.
    pub fn for_roger(bridge_binary_path: &Path, extension_id: &str) -> Self {
        Self {
            name: "com.roger_reviewer.bridge".to_owned(),
            description: "Roger Reviewer browser-to-local launch bridge".to_owned(),
            path: bridge_binary_path.to_string_lossy().to_string(),
            host_type: "stdio".to_owned(),
            allowed_origins: vec![format!("chrome-extension://{extension_id}/")],
        }
    }

    /// Return the platform-specific path where this manifest should be installed.
    pub fn install_path(browser: &SupportedBrowser) -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_owned());
        let home = PathBuf::from(home);
        let os = SupportedOs::current().unwrap_or(SupportedOs::Linux);
        native_host_install_path_for(browser, os, &home)
    }
}

/// Return the Native Messaging manifest install path for a specific OS.
pub fn native_host_install_path_for(
    browser: &SupportedBrowser,
    os: SupportedOs,
    home_dir: &Path,
) -> PathBuf {
    let manifest_name = "com.roger_reviewer.bridge.json";
    match (browser, os) {
        (SupportedBrowser::Chrome, SupportedOs::Macos) => {
            home_dir.join("Library/Application Support/Google/Chrome/NativeMessagingHosts")
        }
        (SupportedBrowser::Edge, SupportedOs::Macos) => {
            home_dir.join("Library/Application Support/Microsoft Edge/NativeMessagingHosts")
        }
        (SupportedBrowser::Brave, SupportedOs::Macos) => home_dir
            .join("Library/Application Support/BraveSoftware/Brave-Browser/NativeMessagingHosts"),
        (SupportedBrowser::Chrome, SupportedOs::Windows) => {
            home_dir.join("AppData/Local/Google/Chrome/User Data/NativeMessagingHosts")
        }
        (SupportedBrowser::Edge, SupportedOs::Windows) => {
            home_dir.join("AppData/Local/Microsoft/Edge/User Data/NativeMessagingHosts")
        }
        (SupportedBrowser::Brave, SupportedOs::Windows) => home_dir
            .join("AppData/Local/BraveSoftware/Brave-Browser/User Data/NativeMessagingHosts"),
        (SupportedBrowser::Chrome, SupportedOs::Linux) => {
            home_dir.join(".config/google-chrome/NativeMessagingHosts")
        }
        (SupportedBrowser::Edge, SupportedOs::Linux) => {
            home_dir.join(".config/microsoft-edge/NativeMessagingHosts")
        }
        (SupportedBrowser::Brave, SupportedOs::Linux) => {
            home_dir.join(".config/BraveSoftware/Brave-Browser/NativeMessagingHosts")
        }
    }
    .join(manifest_name)
}

/// Launch path selected for browser → local bridge handoff.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BridgeLaunchPath {
    NativeMessaging,
}

const NATIVE_MESSAGING_LAUNCH_ARTIFACTS: [&str; 3] = [
    "native_request_envelope.json",
    "native_response_envelope.json",
    "bridge_launch_transcript.json",
];

/// Resolve the launch path from local bridge registration state.
///
/// Native Messaging is required for the supported browser launch path.
pub fn choose_launch_path(
    native_messaging_registered: bool,
    _legacy_fallback_registered: bool,
) -> Result<BridgeLaunchPath> {
    if native_messaging_registered {
        return Ok(BridgeLaunchPath::NativeMessaging);
    }
    Err(BridgeError::LocalStateMissing {
        detail: "Native Messaging host registration is missing. Run `rr extension setup` and rerun `rr extension doctor`.".to_owned(),
    })
}

/// Return artifact filenames expected for bridge launch smoke/failure capture.
///
/// Browser-smoke runners can use this helper to assert transcript and envelope
/// capture requirements without relying on docs-only guidance.
pub fn required_launch_artifacts(path: BridgeLaunchPath) -> &'static [&'static str] {
    match path {
        BridgeLaunchPath::NativeMessaging => &NATIVE_MESSAGING_LAUNCH_ARTIFACTS,
    }
}

// ---------------------------------------------------------------------------
// Bridge host preflight
// ---------------------------------------------------------------------------

/// Check whether the local Roger environment is ready for bridge handoff.
pub struct BridgePreflight {
    pub roger_binary_found: bool,
    pub roger_data_dir_exists: bool,
    pub gh_available: bool,
}

impl BridgePreflight {
    /// Run preflight checks. Does not mutate anything.
    pub fn check(roger_binary_path: &Path, roger_data_dir: &Path) -> Self {
        Self {
            roger_binary_found: roger_binary_path.exists(),
            roger_data_dir_exists: roger_data_dir.exists(),
            gh_available: Command::new("gh")
                .arg("auth")
                .arg("status")
                .output()
                .is_ok_and(|o| o.status.success()),
        }
    }

    /// Return a fail-closed guidance message if something is missing.
    pub fn guidance(&self, roger_binary_path: &Path) -> Option<String> {
        let mut issues = Vec::new();

        if !self.roger_binary_found {
            issues.push(format!(
                "Roger binary not found at {}. Install Roger Reviewer first.",
                roger_binary_path.display()
            ));
        }
        if !self.roger_data_dir_exists {
            issues.push("Roger data directory not found. Run `rr init` to set up.".to_owned());
        }
        if !self.gh_available {
            issues.push("GitHub CLI (gh) not authenticated. Run `gh auth login`.".to_owned());
        }

        if issues.is_empty() {
            None
        } else {
            Some(issues.join("\n"))
        }
    }

    pub fn is_ready(&self) -> bool {
        self.roger_binary_found && self.roger_data_dir_exists && self.gh_available
    }
}

#[derive(Debug, Deserialize)]
struct RobotEnvelope {
    schema_id: String,
    outcome: String,
    generated_at: String,
    exit_code: i32,
    #[serde(default)]
    warnings: Vec<String>,
    #[serde(default)]
    repair_actions: Vec<String>,
    data: Value,
}

struct BridgeDispatchSpec {
    command_name: &'static str,
    argv: Vec<String>,
    allowed_outcomes: &'static [&'static str],
    /// When true, a `blocked` outcome carrying a non-empty `data.candidates`
    /// array is captured as a disambiguation picker (relayed to the extension)
    /// instead of failing the allow-list as an unsafe outcome. Only `rr resume`
    /// opts in; every other command keeps `blocked` fatal.
    capture_picker_block: bool,
}

fn bridge_dispatch_spec(intent: &BridgeLaunchIntent) -> Option<BridgeDispatchSpec> {
    let repo_locator = format!("{}/{}", intent.owner, intent.repo);
    let pr_number = intent.pr_number.to_string();

    // `accepts_surface` marks the launch-attempt commands (review/resume) that
    // record a surface-typed launch attempt/binding; `rr findings` does not, so
    // it must not receive the bridge-only `--surface` flag.
    let (command_name, mut argv, allowed_outcomes, accepts_surface, capture_picker_block) =
        match intent.action.as_str() {
            "start_review" => (
                "rr review",
                vec!["review".to_owned()],
                &["complete", "degraded"][..],
                true,
                false,
            ),
            "resume_review" => (
                "rr resume",
                vec!["resume".to_owned()],
                &["complete", "degraded"][..],
                true,
                true,
            ),
            "show_findings" => (
                "rr findings",
                vec!["findings".to_owned()],
                &["complete", "empty"][..],
                false,
                false,
            ),
            _ => return None,
        };

    argv.push("--repo".to_owned());
    argv.push(repo_locator);
    argv.push("--pr".to_owned());
    argv.push(pr_number);
    // An explicit session id makes resume deterministic: dispatch it so the CLI
    // reopens exactly that candidate instead of auto-selecting.
    if intent.action == "resume_review"
        && let Some(session_id) = intent
            .session_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    {
        argv.push("--session".to_owned());
        argv.push(session_id.to_owned());
    }
    if accepts_surface {
        // Record the true launch origin so the persisted launch attempt/binding
        // carry surface=bridge instead of masquerading as a CLI launch.
        argv.push("--surface".to_owned());
        argv.push("bridge".to_owned());
    }
    argv.push("--robot".to_owned());
    argv.push("--robot-format".to_owned());
    argv.push("json".to_owned());

    Some(BridgeDispatchSpec {
        command_name,
        argv,
        allowed_outcomes,
        capture_picker_block,
    })
}

fn bridge_guidance_from_robot_envelope(
    command_name: &str,
    rerun_command: &str,
    envelope: &RobotEnvelope,
    stderr: &str,
) -> String {
    let mut lines = Vec::new();

    if let Some(reason_code) = envelope.data.get("reason_code").and_then(Value::as_str) {
        lines.push(format!(
            "{command_name} reported outcome '{}' with reason_code={reason_code}.",
            envelope.outcome
        ));
    }
    if !envelope.repair_actions.is_empty() {
        lines.push(format!(
            "Repair actions: {}",
            envelope.repair_actions.join("; ")
        ));
    }
    if !envelope.warnings.is_empty() {
        lines.push(format!("Warnings: {}", envelope.warnings.join("; ")));
    }
    let stderr = stderr.trim();
    if !stderr.is_empty() {
        lines.push(format!("Diagnostics: {stderr}"));
    }
    if lines.is_empty() {
        lines.push(format!(
            "Open Roger locally and rerun `{rerun_command}` for authoritative details."
        ));
    }

    lines.join("\n")
}

fn format_rr_command(argv: &[String]) -> String {
    let mut parts = Vec::with_capacity(argv.len() + 1);
    parts.push("rr".to_owned());
    parts.extend(argv.iter().cloned());
    parts.join(" ")
}

/// Resolve a deliberate, stable directory to anchor bridge-spawned rr children.
///
/// The native host inherits the browser's cwd, which is frequently a poisoned
/// path under NativeMessagingHosts. Spawning rr children there causes the CLI to
/// infer a browser-controlled repo-local context and record poisoned launch
/// bindings. Anchoring the child to a neutral root (the store root or the user
/// home) keeps that inference honest. Store resolution itself stays env/HOME
/// driven, so relocating the child's cwd never moves the canonical store.
fn neutral_bridge_launch_dir() -> PathBuf {
    for candidate in [
        std::env::var_os("HOME").map(PathBuf::from),
        std::env::var_os("RR_STORE_ROOT").map(PathBuf::from),
    ]
    .into_iter()
    .flatten()
    {
        if candidate.is_dir() {
            return candidate;
        }
    }
    std::env::temp_dir()
}

fn envelope_has_candidates(envelope: &RobotEnvelope) -> bool {
    envelope
        .data
        .get("candidates")
        .and_then(Value::as_array)
        .is_some_and(|candidates| !candidates.is_empty())
}

fn execute_rr_robot_command(
    action: &str,
    roger_binary_path: &Path,
    command_name: &str,
    argv: &[String],
    allowed_outcomes: &[&str],
    capture_picker_block: bool,
) -> std::result::Result<RobotEnvelope, BridgeResponse> {
    let rerun_command = format_rr_command(argv);
    let output = match Command::new(roger_binary_path)
        .args(argv)
        .current_dir(neutral_bridge_launch_dir())
        .output()
    {
        Ok(output) => output,
        Err(err) => {
            return Err(BridgeResponse::failure_with_kind(
                action,
                &format!("Failed to invoke {command_name} through Roger bridge."),
                &format!(
                    "{command_name} could not be executed via {}: {err}\nRun `rr doctor` to inspect local setup, then retry `{rerun_command}`.",
                    roger_binary_path.display(),
                ),
                BridgeFailureKind::CliSpawnFailed,
                None,
            ));
        }
    };

    let envelope: RobotEnvelope = match serde_json::from_slice(&output.stdout) {
        Ok(envelope) => envelope,
        Err(err) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(BridgeResponse::failure_with_kind(
                action,
                &format!("{command_name} returned a non-canonical --robot payload."),
                &format!(
                    "Expected machine-readable JSON from {command_name}: {err}\nOpen Roger locally and rerun `{rerun_command}` for authoritative details.\nstdout: {}\nstderr: {}",
                    stdout.trim(),
                    stderr.trim(),
                ),
                BridgeFailureKind::RobotSchemaMismatch,
                None,
            ));
        }
    };

    let process_exit = output.status.code().unwrap_or(1);
    if process_exit != envelope.exit_code {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(BridgeResponse::failure_with_kind(
            action,
            &format!("{command_name} returned a non-canonical exit/result pairing."),
            &format!(
                "{command_name} exited with {process_exit}, but the robot payload declared {}.\n{}",
                envelope.exit_code,
                bridge_guidance_from_robot_envelope(
                    command_name,
                    &rerun_command,
                    &envelope,
                    &stderr
                )
            ),
            BridgeFailureKind::RobotSchemaMismatch,
            None,
        ));
    }

    if !allowed_outcomes.contains(&envelope.outcome.as_str()) {
        // Resume's disambiguation picker surfaces as a `blocked` outcome that
        // carries a bounded candidates list. That is a legitimate, actionable
        // response — not an unsafe outcome — so hand the envelope back for the
        // caller to relay instead of failing the allow-list.
        if capture_picker_block
            && envelope.outcome == "blocked"
            && envelope_has_candidates(&envelope)
        {
            return Ok(envelope);
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(BridgeResponse::failure_with_kind(
            action,
            &format!(
                "{command_name} reported bridge-unsafe outcome '{}'.",
                envelope.outcome
            ),
            &bridge_guidance_from_robot_envelope(command_name, &rerun_command, &envelope, &stderr),
            BridgeFailureKind::CliOutcomeNotSafe,
            Some(envelope.outcome.as_str()),
        ));
    }

    Ok(envelope)
}

fn envelope_session_id(envelope: &RobotEnvelope) -> Option<&str> {
    envelope.data.get("session_id").and_then(Value::as_str)
}

fn build_bridge_status_snapshot(
    envelope: RobotEnvelope,
    expected_session_id: &str,
) -> std::result::Result<BridgeStatusSnapshot, String> {
    let session_id = envelope
        .data
        .get("session")
        .and_then(|session| session.get("id"))
        .and_then(Value::as_str)
        .ok_or_else(|| "rr status payload is missing data.session.id".to_owned())?;
    if session_id != expected_session_id {
        return Err(format!(
            "rr status returned session '{session_id}', expected '{expected_session_id}'"
        ));
    }

    let attention_state = envelope
        .data
        .get("attention")
        .and_then(|attention| attention.get("state"))
        .and_then(Value::as_str)
        .ok_or_else(|| "rr status payload is missing data.attention.state".to_owned())?;

    Ok(BridgeStatusSnapshot {
        schema_id: envelope.schema_id,
        outcome: envelope.outcome,
        generated_at: envelope.generated_at,
        session_id: session_id.to_owned(),
        attention_state: attention_state.to_owned(),
    })
}

fn bridge_success_guidance_from_status_envelope(envelope: &RobotEnvelope) -> Option<String> {
    if envelope.repair_actions.is_empty() {
        return None;
    }

    let mut details = Vec::new();
    if !envelope.warnings.is_empty() {
        details.push(envelope.warnings.join(" "));
    }
    details.push(format!(
        "Repair actions: {}",
        envelope.repair_actions.join("; ")
    ));
    Some(details.join(" "))
}

/// Process a bridge launch intent and return a response.
///
/// This is the main bridge host handler. It validates the intent,
/// checks preflight, and dispatches to the local Roger binary.
/// No mutation or approval side-effects occur in this path.
pub fn handle_bridge_intent(
    intent: &BridgeLaunchIntent,
    preflight: &BridgePreflight,
    roger_binary_path: &Path,
) -> BridgeResponse {
    if intent.action == "register_extension_identity" {
        return handle_extension_registration_intent(intent);
    }

    if !preflight.is_ready() {
        let guidance = preflight
            .guidance(roger_binary_path)
            .unwrap_or_else(|| "Unknown setup issue".to_owned());
        return BridgeResponse::failure_with_kind(
            &intent.action,
            "Roger bridge preflight failed.",
            &guidance,
            BridgeFailureKind::PreflightFailed,
            None,
        );
    }

    // Bounded local-parity actions (triage/drafts/revise/clarify/search/timeline)
    // are thin relays to their matching rr command; they don't follow the
    // launch→status-readback flow below, so route them first.
    if let Some(response) = handle_local_parity_action(intent, roger_binary_path) {
        return response;
    }

    let Some(dispatch) = bridge_dispatch_spec(intent) else {
        return BridgeResponse::failure(
            &intent.action,
            &format!("Unknown bridge action: {}", intent.action),
            "Supported actions: start_review, resume_review, show_findings, triage_finding, show_drafts, revise_draft, request_clarification, search, timeline",
        );
    };

    let launch_envelope = match execute_rr_robot_command(
        &intent.action,
        roger_binary_path,
        dispatch.command_name,
        &dispatch.argv,
        dispatch.allowed_outcomes,
        dispatch.capture_picker_block,
    ) {
        Ok(envelope) => envelope,
        Err(response) => return response,
    };

    let launch_command = format_rr_command(&dispatch.argv);

    // Resume disambiguation picker: relay the candidates as a typed
    // picker-required response so the extension can render per-session resume
    // buttons rather than a generic "unsafe outcome" error.
    if dispatch.capture_picker_block && launch_envelope.outcome == "blocked" {
        let candidates = launch_envelope
            .data
            .get("candidates")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new()));
        let guidance = bridge_guidance_from_robot_envelope(
            dispatch.command_name,
            &launch_command,
            &launch_envelope,
            "",
        );
        return BridgeResponse::picker_required(
            &intent.action,
            "Multiple Roger review sessions match this pull request — choose one to resume.",
            &guidance,
            candidates,
            launch_envelope.warnings.clone(),
        );
    }

    let Some(session_id) = envelope_session_id(&launch_envelope).map(str::to_owned) else {
        return BridgeResponse::failure_with_kind(
            &intent.action,
            &format!(
                "{} completed without a canonical Roger session id.",
                dispatch.command_name
            ),
            &format!(
                "{} returned outcome '{}' but omitted data.session_id. Open Roger locally and rerun `{launch_command}` for authoritative recovery.",
                dispatch.command_name, launch_envelope.outcome,
            ),
            BridgeFailureKind::MissingSessionId,
            None,
        );
    };

    let status_argv = vec![
        "status".to_owned(),
        "--session".to_owned(),
        session_id.clone(),
        "--robot".to_owned(),
        "--robot-format".to_owned(),
        "json".to_owned(),
    ];
    let status_envelope = match execute_rr_robot_command(
        &intent.action,
        roger_binary_path,
        "rr status",
        &status_argv,
        &["complete"],
        false,
    ) {
        Ok(envelope) => envelope,
        Err(response) => return response,
    };
    let success_guidance = bridge_success_guidance_from_status_envelope(&status_envelope);

    let status = match build_bridge_status_snapshot(status_envelope, &session_id) {
        Ok(status) => status,
        Err(detail) => {
            return BridgeResponse::failure_with_kind(
                &intent.action,
                "Roger bridge status readback was incomplete.",
                &format!(
                    "rr status succeeded for session '{session_id}' but returned a non-canonical payload: {detail}\nOpen Roger locally and rerun `{}` for authoritative detail.",
                    format_rr_command(&status_argv)
                ),
                BridgeFailureKind::RobotSchemaMismatch,
                Some(launch_envelope.outcome.as_str()),
            );
        }
    };

    let launch_outcome = match launch_envelope.outcome.as_str() {
        "degraded" => Some("degraded"),
        _ => None,
    };
    let message = if launch_outcome == Some("degraded") {
        format!(
            "{} completed in degraded mode for {}/{}#{}. Open Roger locally with `{}` for authoritative detail.",
            dispatch.command_name,
            intent.owner,
            intent.repo,
            intent.pr_number,
            format_rr_command(&status_argv)
        )
    } else {
        format!(
            "{} completed for {}/{}#{}",
            dispatch.command_name, intent.owner, intent.repo, intent.pr_number
        )
    };

    let mut response = BridgeResponse::success_with_status(
        &intent.action,
        &message,
        session_id,
        status,
        success_guidance,
        launch_outcome,
    );
    if intent.action == "show_findings" {
        response.findings = Some(serde_json::json!({
            "items": launch_envelope.data.get("items").cloned().unwrap_or(Value::Array(Vec::new())),
            "count": launch_envelope.data.get("count").cloned().unwrap_or(Value::from(0)),
            "warnings": launch_envelope.warnings,
        }));
    } else if intent.action == "resume_review" {
        // Forward the resume envelope's warnings so a silent auto-pick becomes
        // visible, and echo the auto-selection signal detected from the CLI's
        // "auto-selected session ..." warning prefix.
        if launch_envelope
            .warnings
            .iter()
            .any(|warning| warning.trim_start().starts_with("auto-selected session"))
        {
            response.auto_selected_session = Some(true);
        }
        response.warnings = launch_envelope.warnings;
    }
    response
}

fn normalize_extension_id(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.len() == 32 && trimmed.chars().all(|ch| ch.is_ascii_lowercase()) {
        Some(trimmed.to_owned())
    } else {
        None
    }
}

fn resolve_store_root() -> PathBuf {
    std::env::var("RR_STORE_ROOT")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(".roger")
        })
}

fn extension_registry_path(store_root: &Path) -> PathBuf {
    store_root.join("bridge/extension-id")
}

fn persist_extension_identity(store_root: &Path, extension_id: &str) -> Result<PathBuf> {
    let registry_path = extension_registry_path(store_root);
    if let Some(parent) = registry_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&registry_path, format!("{extension_id}\n"))?;
    Ok(registry_path)
}

fn handle_extension_registration_intent(intent: &BridgeLaunchIntent) -> BridgeResponse {
    let action = "register_extension_identity";
    let Some(raw_extension_id) = intent.extension_id.as_deref() else {
        return BridgeResponse::failure(
            action,
            "Missing extension identity in registration intent.",
            "Reload the unpacked extension and rerun `rr extension setup --browser <edge|chrome|brave>`.",
        );
    };
    let Some(extension_id) = normalize_extension_id(raw_extension_id) else {
        return BridgeResponse::failure(
            action,
            "Invalid extension identity format in registration intent.",
            "Expected a 32-character lowercase extension runtime ID.",
        );
    };

    let store_root = resolve_store_root();
    match persist_extension_identity(&store_root, &extension_id) {
        Ok(registry_path) => {
            let browser = intent.browser.as_deref().unwrap_or("unknown");
            BridgeResponse::success(
                action,
                &format!(
                    "Registered extension identity for {browser} at {}",
                    registry_path.display()
                ),
                None,
            )
        }
        Err(err) => BridgeResponse::failure(
            action,
            "Failed to persist extension identity registration.",
            &format!(
                "Could not write extension-id registry: {err}. Rerun `rr extension setup --browser <edge|chrome|brave>` and reload the extension."
            ),
        ),
    }
}

// ---------------------------------------------------------------------------
// Bounded local-parity actions (deliberate asymmetry 1)
//
// These actions bring the extension to bounded local parity: they surface state
// and initiate LOCAL mutations, but never post or approve. Each one stays a thin
// relay that dispatches the matching `rr` command (which already calls the shared
// review-ops fail-closed logic) and forwards its robot envelope. The bridge adds
// no domain rules of its own.
// ---------------------------------------------------------------------------

/// Trim an optional intent field to a non-empty value, or `None`.
fn non_empty(value: &Option<String>) -> Option<&str> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// Build a fail-closed "missing required field" response for a local-parity
/// action.
fn missing_field(action: &str, field: &str, hint: &str) -> BridgeResponse {
    BridgeResponse::failure(
        action,
        &format!("{action} requires a '{field}' value."),
        hint,
    )
}

fn repo_locator(intent: &BridgeLaunchIntent) -> String {
    format!("{}/{}", intent.owner, intent.repo)
}

/// `triage_finding`: dispatch `rr triage --repo <r> --pr <n> --finding <id>
/// --state <state> --robot` (the canonical command; `rr send triage` is an alias
/// of it) and relay the mutated findings mirror. Mutating-local: it never posts
/// or approves; the shared review-ops triage op enforces the fail-closed rules.
fn handle_triage_finding_intent(
    intent: &BridgeLaunchIntent,
    roger_binary_path: &Path,
) -> BridgeResponse {
    let action = intent.action.as_str();
    let Some(finding_id) = non_empty(&intent.finding_id) else {
        return missing_field(action, "finding_id", "Provide the finding id to triage.");
    };
    let Some(state) = non_empty(&intent.state) else {
        return missing_field(
            action,
            "state",
            "Provide a triage state: accepted, ignored, needs_follow_up, or resolved.",
        );
    };
    let argv = vec![
        "triage".to_owned(),
        "--repo".to_owned(),
        repo_locator(intent),
        "--pr".to_owned(),
        intent.pr_number.to_string(),
        "--finding".to_owned(),
        finding_id.to_owned(),
        "--state".to_owned(),
        state.to_owned(),
        "--robot".to_owned(),
        "--robot-format".to_owned(),
        "json".to_owned(),
    ];
    let envelope = match execute_rr_robot_command(
        action,
        roger_binary_path,
        "rr triage",
        &argv,
        &["complete"],
        false,
    ) {
        Ok(envelope) => envelope,
        Err(response) => return response,
    };

    let mut response = BridgeResponse::success(
        action,
        &format!(
            "rr triage set finding {finding_id} to '{state}' for {}/{}#{} (local-only; nothing posted).",
            intent.owner, intent.repo, intent.pr_number
        ),
        None,
    );
    response.findings = Some(serde_json::json!({
        "items": envelope.data.get("items").cloned().unwrap_or(Value::Array(Vec::new())),
        "count": envelope.data.get("count").cloned().unwrap_or(Value::from(0)),
        "triage_state": envelope.data.get("triage_state").cloned().unwrap_or(Value::Null),
        "warnings": envelope.warnings,
    }));
    response
}

/// `show_drafts`: read-only. Dispatch `rr findings --repo <r> --pr <n> --robot`
/// and relay the items (each carries `outbound_state` + `outbound_detail`
/// {draft_id, draft_batch_id}) so the extension draft-staging view can render
/// batches, offer edit-as-revision, and show the approve/post handoff command.
fn handle_show_drafts_intent(
    intent: &BridgeLaunchIntent,
    roger_binary_path: &Path,
) -> BridgeResponse {
    let action = intent.action.as_str();
    let argv = vec![
        "findings".to_owned(),
        "--repo".to_owned(),
        repo_locator(intent),
        "--pr".to_owned(),
        intent.pr_number.to_string(),
        "--robot".to_owned(),
        "--robot-format".to_owned(),
        "json".to_owned(),
    ];
    let envelope = match execute_rr_robot_command(
        action,
        roger_binary_path,
        "rr findings",
        &argv,
        &["complete", "empty"],
        false,
    ) {
        Ok(envelope) => envelope,
        Err(response) => return response,
    };

    let mut response = BridgeResponse::success(
        action,
        &format!(
            "Relayed local outbound draft state for {}/{}#{} (read-only mirror).",
            intent.owner, intent.repo, intent.pr_number
        ),
        None,
    );
    response.drafts = Some(serde_json::json!({
        "items": envelope.data.get("items").cloned().unwrap_or(Value::Array(Vec::new())),
        "count": envelope.data.get("count").cloned().unwrap_or(Value::from(0)),
        "warnings": envelope.warnings,
    }));
    response
}

/// `revise_draft`: dispatch `rr send edit --draft <id> --body-file <tmp>`, a
/// LOCAL human-only revision that never posts (editing an approved batch revokes
/// its approval, enforced by the CLI). `rr send edit` is intentionally not a
/// `--robot` surface, so this is an exit-code relay (not a robot envelope). The
/// body is written to a host-side temp file the CLI reads and we delete.
fn handle_revise_draft_intent(
    intent: &BridgeLaunchIntent,
    roger_binary_path: &Path,
) -> BridgeResponse {
    let action = intent.action.as_str();
    let Some(draft_id) = non_empty(&intent.draft_id) else {
        return missing_field(
            action,
            "draft_id",
            "Provide the outbound draft id to revise.",
        );
    };
    let Some(body) = intent
        .body
        .as_deref()
        .filter(|body| !body.trim().is_empty())
    else {
        return missing_field(
            action,
            "body",
            "Provide the replacement draft body (non-empty).",
        );
    };

    // Write the new body to a neutral host-side temp file the CLI reads.
    let temp_path = std::env::temp_dir().join(format!(
        "rr-bridge-revise-{}-{}.txt",
        std::process::id(),
        draft_id.replace(|c: char| !c.is_ascii_alphanumeric(), "_")
    ));
    if let Err(err) = fs::write(&temp_path, body) {
        return BridgeResponse::failure(
            action,
            "Failed to stage the revised draft body for rr send edit.",
            &format!(
                "Could not write a temp body file at {}: {err}. Retry, or run `rr send edit --draft {draft_id} --body-file <path>` locally.",
                temp_path.display()
            ),
        );
    }

    let argv = vec![
        "send".to_owned(),
        "edit".to_owned(),
        "--draft".to_owned(),
        draft_id.to_owned(),
        "--body-file".to_owned(),
        temp_path.to_string_lossy().to_string(),
    ];
    let rerun = format!("rr send edit --draft {draft_id} --body-file <path>");
    let output = Command::new(roger_binary_path)
        .args(&argv)
        .current_dir(neutral_bridge_launch_dir())
        .output();
    let _ = fs::remove_file(&temp_path);

    let output = match output {
        Ok(output) => output,
        Err(err) => {
            return BridgeResponse::failure_with_kind(
                action,
                "Failed to invoke rr send edit through Roger bridge.",
                &format!(
                    "rr send edit could not be executed via {}: {err}\nRun `rr doctor`, then retry `{rerun}`.",
                    roger_binary_path.display()
                ),
                BridgeFailureKind::CliSpawnFailed,
                None,
            );
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if output.status.success() {
        let message = {
            let trimmed = stdout.trim();
            if trimmed.is_empty() {
                format!(
                    "rr send edit recorded a local revision for draft {draft_id} (nothing posted)."
                )
            } else {
                trimmed.to_owned()
            }
        };
        BridgeResponse::success(action, &message, None)
    } else {
        let mut detail = Vec::new();
        if !stdout.trim().is_empty() {
            detail.push(stdout.trim().to_owned());
        }
        if !stderr.trim().is_empty() {
            detail.push(stderr.trim().to_owned());
        }
        detail.push(format!(
            "Re-run `{rerun}` locally for authoritative detail."
        ));
        BridgeResponse::failure_with_kind(
            action,
            "rr send edit refused the local draft revision.",
            &detail.join("\n"),
            BridgeFailureKind::CliOutcomeNotSafe,
            None,
        )
    }
}

/// `request_clarification`: dispatch `rr clarify --finding <id> --body <text>
/// --repo <r> --pr <n> --robot`, creating a durable LOCAL clarification (never a
/// GitHub post). GATED: `rr clarify` is being added by a parallel CLI
/// workstream; until it lands in main the dispatch fails closed with the CLI's
/// own error. The relay is forward-compatible — it works the moment `rr clarify`
/// ships.
fn handle_request_clarification_intent(
    intent: &BridgeLaunchIntent,
    roger_binary_path: &Path,
) -> BridgeResponse {
    let action = intent.action.as_str();
    let Some(finding_id) = non_empty(&intent.finding_id) else {
        return missing_field(
            action,
            "finding_id",
            "Provide the finding id to request clarification on.",
        );
    };
    let Some(body) = intent
        .body
        .as_deref()
        .filter(|body| !body.trim().is_empty())
    else {
        return missing_field(action, "body", "Provide the clarification prompt text.");
    };
    let argv = vec![
        "clarify".to_owned(),
        "--finding".to_owned(),
        finding_id.to_owned(),
        "--body".to_owned(),
        body.to_owned(),
        "--repo".to_owned(),
        repo_locator(intent),
        "--pr".to_owned(),
        intent.pr_number.to_string(),
        "--robot".to_owned(),
        "--robot-format".to_owned(),
        "json".to_owned(),
    ];
    let envelope = match execute_rr_robot_command(
        action,
        roger_binary_path,
        "rr clarify",
        &argv,
        &["complete"],
        false,
    ) {
        Ok(envelope) => envelope,
        Err(response) => return response,
    };

    let mut response = BridgeResponse::success(
        action,
        &format!(
            "rr clarify recorded a local clarification on finding {finding_id} (nothing posted)."
        ),
        None,
    );
    response.clarification_ack = Some(envelope.data);
    response
}

/// `search`: read-only. Dispatch `rr search --query <q> --repo <r> --robot` and
/// relay the search data (matches/count) for the extension's bounded prior-review
/// search surface.
fn handle_search_intent(intent: &BridgeLaunchIntent, roger_binary_path: &Path) -> BridgeResponse {
    let action = intent.action.as_str();
    let Some(query) = non_empty(&intent.query) else {
        return missing_field(action, "query", "Provide the search text.");
    };
    let argv = vec![
        "search".to_owned(),
        "--query".to_owned(),
        query.to_owned(),
        "--repo".to_owned(),
        repo_locator(intent),
        "--robot".to_owned(),
        "--robot-format".to_owned(),
        "json".to_owned(),
    ];
    let envelope = match execute_rr_robot_command(
        action,
        roger_binary_path,
        "rr search",
        &argv,
        &["complete", "empty"],
        false,
    ) {
        Ok(envelope) => envelope,
        Err(response) => return response,
    };

    let mut response = BridgeResponse::success(
        action,
        &format!("rr search returned prior-review matches for \"{query}\" (read-only)."),
        None,
    );
    let mut data = envelope.data;
    if let Value::Object(ref mut map) = data {
        map.insert("warnings".to_owned(), Value::from(envelope.warnings));
    }
    response.search_results = Some(data);
    response
}

/// `timeline`: read-only. Dispatch `rr timeline --repo <r> --pr <n> --robot` and
/// relay the timeline data. GATED: `rr timeline` is not yet in main (a parallel
/// CLI workstream owns it); until it lands the dispatch fails closed. The relay
/// is forward-compatible.
fn handle_timeline_intent(intent: &BridgeLaunchIntent, roger_binary_path: &Path) -> BridgeResponse {
    let action = intent.action.as_str();
    let argv = vec![
        "timeline".to_owned(),
        "--repo".to_owned(),
        repo_locator(intent),
        "--pr".to_owned(),
        intent.pr_number.to_string(),
        "--robot".to_owned(),
        "--robot-format".to_owned(),
        "json".to_owned(),
    ];
    let envelope = match execute_rr_robot_command(
        action,
        roger_binary_path,
        "rr timeline",
        &argv,
        &["complete", "empty"],
        false,
    ) {
        Ok(envelope) => envelope,
        Err(response) => return response,
    };

    let mut response = BridgeResponse::success(
        action,
        &format!(
            "rr timeline relayed the run/stage/posted view for {}/{}#{} (read-only).",
            intent.owner, intent.repo, intent.pr_number
        ),
        None,
    );
    let mut data = envelope.data;
    if let Value::Object(ref mut map) = data {
        map.insert("warnings".to_owned(), Value::from(envelope.warnings));
    }
    response.timeline = Some(data);
    response
}

/// Route a bounded local-parity action to its handler, if the action is one.
/// Returns `None` for the launch-family actions (start/resume/show_findings)
/// handled by the existing dispatch path.
fn handle_local_parity_action(
    intent: &BridgeLaunchIntent,
    roger_binary_path: &Path,
) -> Option<BridgeResponse> {
    match intent.action.as_str() {
        "triage_finding" => Some(handle_triage_finding_intent(intent, roger_binary_path)),
        "show_drafts" => Some(handle_show_drafts_intent(intent, roger_binary_path)),
        "revise_draft" => Some(handle_revise_draft_intent(intent, roger_binary_path)),
        "request_clarification" => Some(handle_request_clarification_intent(
            intent,
            roger_binary_path,
        )),
        "search" => Some(handle_search_intent(intent, roger_binary_path)),
        "timeline" => Some(handle_timeline_intent(intent, roger_binary_path)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use std::sync::{Mutex, OnceLock};

    fn sample_intent() -> BridgeLaunchIntent {
        BridgeLaunchIntent {
            action: "start_review".to_owned(),
            owner: "acme".to_owned(),
            repo: "widgets".to_owned(),
            pr_number: 42,
            head_ref: Some("feat/frob".to_owned()),
            instance: None,
            session_id: None,
            extension_id: None,
            browser: None,
            finding_id: None,
            state: None,
            draft_id: None,
            body: None,
            query: None,
        }
    }

    #[cfg(unix)]
    fn write_stub_roger_binary(
        primary_command: &str,
        primary_payload: &str,
        primary_exit: i32,
        status_payload: &str,
        status_exit: i32,
    ) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("rr-stub");
        let script = format!(
            r#"#!/bin/sh
case "$1" in
  {primary_command})
    cat <<'EOF'
{primary_payload}
EOF
    exit {primary_exit}
    ;;
  status)
    cat <<'EOF'
{status_payload}
EOF
    exit {status_exit}
    ;;
  *)
    echo "unexpected args: $@" >&2
    exit 64
    ;;
esac
"#
        );
        fs::write(&path, script).expect("write rr stub");
        let mut perms = fs::metadata(&path).expect("rr stub metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&path, perms).expect("chmod rr stub");
        (dir, path)
    }

    #[test]
    fn native_messaging_roundtrip() {
        let intent = sample_intent();
        let json = serde_json::to_vec(&intent).unwrap();
        let len = json.len() as u32;

        let mut buf = Vec::new();
        buf.extend_from_slice(&len.to_le_bytes());
        buf.extend_from_slice(&json);

        let mut reader = Cursor::new(buf);
        let parsed = read_native_message(&mut reader).unwrap();
        assert_eq!(parsed, intent);
    }

    #[test]
    fn native_messaging_write_read() {
        let response = BridgeResponse::success("start_review", "ok", Some("sess-1".to_owned()));

        let mut buf = Vec::new();
        write_native_message(&mut buf, &response).unwrap();

        // Read back: 4-byte length prefix + JSON.
        assert!(buf.len() > 4);
        let len = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
        let json: BridgeResponse = serde_json::from_slice(&buf[4..4 + len]).unwrap();
        assert_eq!(json.ok, true);
        assert_eq!(json.session_id, Some("sess-1".to_owned()));
    }

    #[test]
    fn launch_progress_frame_shape_is_stable() {
        let frame = launch_progress_frame("host_started");
        assert_eq!(
            frame["schema"].as_str(),
            Some("roger.bridge.launch-progress.v1")
        );
        assert_eq!(frame["stage"].as_str(), Some("host_started"));

        // A progress frame is distinguishable from a final BridgeResponse by its
        // schema (a BridgeResponse carries no `schema` field, an `ok` bool).
        let response = BridgeResponse::success("start_review", "ok", None);
        let response_value = serde_json::to_value(&response).unwrap();
        assert!(response_value.get("schema").is_none());
        assert_ne!(
            response_value.get("schema").and_then(|v| v.as_str()),
            Some(LAUNCH_PROGRESS_SCHEMA)
        );
    }

    #[test]
    fn write_launch_progress_is_length_prefixed_and_readable() {
        let mut buf = Vec::new();
        write_launch_progress(&mut buf, "preflight_ok").unwrap();
        assert!(buf.len() > 4);
        let len = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
        assert_eq!(buf.len(), 4 + len);
        let value: serde_json::Value = serde_json::from_slice(&buf[4..]).unwrap();
        assert_eq!(value["schema"].as_str(), Some(LAUNCH_PROGRESS_SCHEMA));
        assert_eq!(value["stage"].as_str(), Some("preflight_ok"));
    }

    #[test]
    fn native_messaging_too_large() {
        let mut buf = Vec::new();
        let len: u32 = 2_000_000;
        buf.extend_from_slice(&len.to_le_bytes());
        buf.extend(vec![0u8; 100]); // Doesn't matter, length check first.

        let mut reader = Cursor::new(buf);
        let result = read_native_message(&mut reader);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too large"));
    }

    #[test]
    fn host_manifest_for_roger() {
        let manifest =
            NativeHostManifest::for_roger(Path::new("/usr/local/bin/rr"), "abcdef123456");
        assert_eq!(manifest.name, "com.roger_reviewer.bridge");
        assert_eq!(manifest.host_type, "stdio");
        assert!(manifest.allowed_origins[0].contains("abcdef123456"));
    }

    #[test]
    fn host_manifest_install_paths() {
        let chrome_path = NativeHostManifest::install_path(&SupportedBrowser::Chrome);
        assert!(
            chrome_path
                .to_string_lossy()
                .contains("com.roger_reviewer.bridge.json")
        );

        let edge_path = NativeHostManifest::install_path(&SupportedBrowser::Edge);
        assert!(
            edge_path.to_string_lossy().contains("Edge")
                || edge_path.to_string_lossy().contains("microsoft-edge")
        );

        let brave_path = NativeHostManifest::install_path(&SupportedBrowser::Brave);
        assert!(
            brave_path.to_string_lossy().contains("Brave")
                || brave_path.to_string_lossy().contains("BraveSoftware")
        );
    }

    #[test]
    fn host_manifest_install_paths_cover_supported_os_matrix() {
        let home = Path::new("/home/tester");
        let matrix = vec![
            (
                SupportedBrowser::Chrome,
                SupportedOs::Macos,
                "Google/Chrome/NativeMessagingHosts/com.roger_reviewer.bridge.json",
            ),
            (
                SupportedBrowser::Edge,
                SupportedOs::Windows,
                "Microsoft/Edge/User Data/NativeMessagingHosts/com.roger_reviewer.bridge.json",
            ),
            (
                SupportedBrowser::Brave,
                SupportedOs::Linux,
                "BraveSoftware/Brave-Browser/NativeMessagingHosts/com.roger_reviewer.bridge.json",
            ),
        ];

        for (browser, os, expected_suffix) in matrix {
            let path = native_host_install_path_for(&browser, os, home);
            assert!(
                path.to_string_lossy().contains(expected_suffix),
                "expected {expected_suffix}, got {}",
                path.display()
            );
        }
    }

    #[test]
    fn preflight_guidance_when_not_ready() {
        let preflight = BridgePreflight {
            roger_binary_found: false,
            roger_data_dir_exists: false,
            gh_available: false,
        };
        let guidance = preflight.guidance(Path::new("/usr/local/bin/rr")).unwrap();
        assert!(guidance.contains("Roger binary not found"));
        assert!(guidance.contains("data directory"));
        assert!(guidance.contains("gh auth login"));
    }

    #[test]
    fn preflight_no_guidance_when_ready() {
        let preflight = BridgePreflight {
            roger_binary_found: true,
            roger_data_dir_exists: true,
            gh_available: true,
        };
        assert!(preflight.guidance(Path::new("/usr/local/bin/rr")).is_none());
        assert!(preflight.is_ready());
    }

    #[test]
    fn handle_bridge_intent_not_ready() {
        let preflight = BridgePreflight {
            roger_binary_found: false,
            roger_data_dir_exists: true,
            gh_available: true,
        };
        let intent = sample_intent();
        let resp = handle_bridge_intent(&intent, &preflight, Path::new("/usr/local/bin/rr"));
        assert!(!resp.ok);
        assert!(resp.guidance.unwrap().contains("not found"));
    }

    #[cfg(unix)]
    #[test]
    fn handle_bridge_intent_success() {
        let (_stub_dir, stub_rr) = write_stub_roger_binary(
            "review",
            &serde_json::to_string_pretty(&serde_json::json!({
                "schema_id": "rr.robot.review.v1",
                "command": "rr review",
                "robot_format": "json",
                "outcome": "complete",
                "generated_at": "2026-04-15T00:00:00Z",
                "exit_code": 0,
                "warnings": [],
                "repair_actions": [],
                "data": {
                    "session_id": "session-bridge-1",
                    "launch_attempt_id": "attempt-1"
                }
            }))
            .expect("serialize review envelope"),
            0,
            &serde_json::to_string_pretty(&serde_json::json!({
                "schema_id": "rr.robot.status.v1",
                "command": "rr status",
                "robot_format": "json",
                "outcome": "complete",
                "generated_at": "2026-04-15T00:00:01Z",
                "exit_code": 0,
                "warnings": [],
                "repair_actions": [],
                "data": {
                    "session": {"id": "session-bridge-1"},
                    "attention": {"state": "review_launched"}
                }
            }))
            .expect("serialize status envelope"),
            0,
        );
        let preflight = BridgePreflight {
            roger_binary_found: true,
            roger_data_dir_exists: true,
            gh_available: true,
        };
        let intent = sample_intent();
        let resp = handle_bridge_intent(&intent, &preflight, &stub_rr);
        assert!(resp.ok);
        assert_eq!(resp.session_id.as_deref(), Some("session-bridge-1"));
        assert_eq!(resp.attention_state.as_deref(), Some("review_launched"));
        assert_eq!(
            resp.status.as_ref().map(|status| status.schema_id.as_str()),
            Some("rr.robot.status.v1")
        );
        assert!(resp.message.contains("rr review"));
        assert!(
            resp.findings.is_none(),
            "launch responses must not carry a findings mirror"
        );
    }

    #[cfg(unix)]
    #[test]
    fn handle_bridge_intent_show_findings_relays_items() {
        let (_stub_dir, stub_rr) = write_stub_roger_binary(
            "findings",
            &serde_json::to_string_pretty(&serde_json::json!({
                "schema_id": "rr.robot.findings.v1",
                "command": "rr findings",
                "robot_format": "json",
                "outcome": "complete",
                "generated_at": "2026-04-15T00:00:00Z",
                "exit_code": 0,
                "warnings": ["semantic assets unverified"],
                "repair_actions": [],
                "data": {
                    "session_id": "session-bridge-1",
                    "items": [
                        {
                            "finding_id": "finding-1",
                            "title": "unchecked unwrap",
                            "severity": "high",
                            "triage_state": "new",
                            "outbound_state": "not_drafted",
                            "file_anchor": {"path": "src/lib.rs", "start_line": 10, "end_line": 12},
                            "evidence_count": 1
                        }
                    ],
                    "count": 1
                }
            }))
            .expect("serialize findings envelope"),
            0,
            &serde_json::to_string_pretty(&serde_json::json!({
                "schema_id": "rr.robot.status.v1",
                "command": "rr status",
                "robot_format": "json",
                "outcome": "complete",
                "generated_at": "2026-04-15T00:00:01Z",
                "exit_code": 0,
                "warnings": [],
                "repair_actions": [],
                "data": {
                    "session": {"id": "session-bridge-1"},
                    "attention": {"state": "review_launched"}
                }
            }))
            .expect("serialize status envelope"),
            0,
        );
        let preflight = BridgePreflight {
            roger_binary_found: true,
            roger_data_dir_exists: true,
            gh_available: true,
        };
        let mut intent = sample_intent();
        intent.action = "show_findings".to_owned();
        let resp = handle_bridge_intent(&intent, &preflight, &stub_rr);
        assert!(resp.ok, "guidance: {:?}", resp.guidance);
        let findings = resp
            .findings
            .expect("show_findings must relay a findings mirror");
        assert_eq!(findings["count"], 1);
        assert_eq!(findings["items"][0]["finding_id"], "finding-1");
        assert_eq!(findings["items"][0]["severity"], "high");
        assert_eq!(
            findings["warnings"][0], "semantic assets unverified",
            "envelope warnings must ride along for honest degrade rendering"
        );
    }

    #[cfg(unix)]
    #[test]
    fn handle_bridge_intent_fails_when_dispatch_omits_session_identity() {
        let (_stub_dir, stub_rr) = write_stub_roger_binary(
            "review",
            &serde_json::to_string_pretty(&serde_json::json!({
                "schema_id": "rr.robot.review.v1",
                "command": "rr review",
                "robot_format": "json",
                "outcome": "complete",
                "generated_at": "2026-04-15T00:00:00Z",
                "exit_code": 0,
                "warnings": [],
                "repair_actions": [],
                "data": {
                    "launch_attempt_id": "attempt-1"
                }
            }))
            .expect("serialize review envelope"),
            0,
            &serde_json::to_string_pretty(&serde_json::json!({
                "schema_id": "rr.robot.status.v1",
                "command": "rr status",
                "robot_format": "json",
                "outcome": "complete",
                "generated_at": "2026-04-15T00:00:01Z",
                "exit_code": 0,
                "warnings": [],
                "repair_actions": [],
                "data": {
                    "session": {"id": "session-bridge-1"},
                    "attention": {"state": "review_launched"}
                }
            }))
            .expect("serialize status envelope"),
            0,
        );
        let preflight = BridgePreflight {
            roger_binary_found: true,
            roger_data_dir_exists: true,
            gh_available: true,
        };
        let intent = sample_intent();
        let resp = handle_bridge_intent(&intent, &preflight, &stub_rr);
        assert!(!resp.ok);
        assert_eq!(resp.failure_kind, Some(BridgeFailureKind::MissingSessionId));
        assert!(resp.message.contains("canonical Roger session id"));
    }

    #[test]
    fn resume_dispatch_passes_explicit_session_id_in_argv() {
        let mut intent = sample_intent();
        intent.action = "resume_review".to_owned();
        intent.session_id = Some("session-explicit-9".to_owned());
        let dispatch = bridge_dispatch_spec(&intent).expect("resume dispatch spec");
        assert_eq!(dispatch.command_name, "rr resume");
        assert!(dispatch.capture_picker_block);
        let session_flag = dispatch.argv.windows(2).find(|pair| pair[0] == "--session");
        assert_eq!(
            session_flag.map(|pair| pair[1].as_str()),
            Some("session-explicit-9"),
            "resume argv must carry the explicit session id: {:?}",
            dispatch.argv
        );
    }

    #[test]
    fn resume_dispatch_omits_session_flag_without_explicit_id() {
        let mut intent = sample_intent();
        intent.action = "resume_review".to_owned();
        intent.session_id = None;
        let dispatch = bridge_dispatch_spec(&intent).expect("resume dispatch spec");
        assert!(
            !dispatch.argv.iter().any(|arg| arg == "--session"),
            "resume argv must not carry --session when no id is provided: {:?}",
            dispatch.argv
        );
    }

    #[cfg(unix)]
    #[test]
    fn handle_bridge_intent_resume_forwards_auto_select_warning() {
        let (_stub_dir, stub_rr) = write_stub_roger_binary(
            "resume",
            &serde_json::to_string_pretty(&serde_json::json!({
                "schema_id": "rr.robot.resume.v1",
                "command": "rr resume",
                "robot_format": "json",
                "outcome": "complete",
                "generated_at": "2026-04-15T00:00:00Z",
                "exit_code": 0,
                "warnings": [
                    "auto-selected session session-bridge-1 from 3 candidates (pr_rank=1, binding_rank=2, continuity_rank=2, updated_at=1000)"
                ],
                "repair_actions": [],
                "data": {
                    "session_id": "session-bridge-1",
                    "resume_path": "reopened_by_locator"
                }
            }))
            .expect("serialize resume envelope"),
            0,
            &serde_json::to_string_pretty(&serde_json::json!({
                "schema_id": "rr.robot.status.v1",
                "command": "rr status",
                "robot_format": "json",
                "outcome": "complete",
                "generated_at": "2026-04-15T00:00:01Z",
                "exit_code": 0,
                "warnings": [],
                "repair_actions": [],
                "data": {
                    "session": {"id": "session-bridge-1"},
                    "attention": {"state": "awaiting_user_input"}
                }
            }))
            .expect("serialize status envelope"),
            0,
        );
        let preflight = BridgePreflight {
            roger_binary_found: true,
            roger_data_dir_exists: true,
            gh_available: true,
        };
        let mut intent = sample_intent();
        intent.action = "resume_review".to_owned();
        let resp = handle_bridge_intent(&intent, &preflight, &stub_rr);
        assert!(resp.ok, "guidance: {:?}", resp.guidance);
        assert_eq!(resp.auto_selected_session, Some(true));
        assert!(
            resp.warnings
                .iter()
                .any(|warning| warning.starts_with("auto-selected session")),
            "resume must forward the CLI auto-select warning: {:?}",
            resp.warnings
        );
    }

    #[cfg(unix)]
    #[test]
    fn handle_bridge_intent_resume_relays_picker_candidates() {
        let (_stub_dir, stub_rr) = write_stub_roger_binary(
            "resume",
            &serde_json::to_string_pretty(&serde_json::json!({
                "schema_id": "rr.robot.resume.v1",
                "command": "rr resume",
                "robot_format": "json",
                "outcome": "blocked",
                "generated_at": "2026-04-15T00:00:00Z",
                "exit_code": 3,
                "warnings": ["session inference is ambiguous; explicit selection is required"],
                "repair_actions": ["re-run with --session <id> or pass --pr <number> for a unique match"],
                "data": {
                    "reason": "ambiguous repo-local session match",
                    "candidates": [
                        {
                            "session_id": "session-a",
                            "repository": "acme/widgets",
                            "pull_request": 42,
                            "attention_state": "awaiting_user_input",
                            "provider": "opencode",
                            "updated_at": 1000
                        },
                        {
                            "session_id": "session-b",
                            "repository": "acme/widgets",
                            "pull_request": 42,
                            "attention_state": "refresh_recommended",
                            "provider": "codex",
                            "updated_at": 2000
                        }
                    ]
                }
            }))
            .expect("serialize blocked resume envelope"),
            3,
            &serde_json::to_string_pretty(&serde_json::json!({
                "schema_id": "rr.robot.status.v1",
                "command": "rr status",
                "robot_format": "json",
                "outcome": "complete",
                "generated_at": "2026-04-15T00:00:01Z",
                "exit_code": 0,
                "warnings": [],
                "repair_actions": [],
                "data": {"session": {"id": "unused"}, "attention": {"state": "awaiting_user_input"}}
            }))
            .expect("serialize status envelope"),
            0,
        );
        let preflight = BridgePreflight {
            roger_binary_found: true,
            roger_data_dir_exists: true,
            gh_available: true,
        };
        let mut intent = sample_intent();
        intent.action = "resume_review".to_owned();
        let resp = handle_bridge_intent(&intent, &preflight, &stub_rr);
        assert!(!resp.ok);
        assert_eq!(resp.failure_kind, Some(BridgeFailureKind::PickerRequired));
        let candidates = resp
            .candidates
            .expect("picker response must relay candidates");
        let array = candidates.as_array().expect("candidates array");
        assert_eq!(array.len(), 2);
        assert_eq!(array[0]["session_id"], "session-a");
        assert_eq!(array[1]["provider"], "codex");
        assert!(
            resp.session_id.is_none(),
            "a picker response has no single resolved session"
        );
    }

    #[cfg(unix)]
    #[test]
    fn handle_bridge_intent_resume_blocked_without_candidates_stays_unsafe() {
        let (_stub_dir, stub_rr) = write_stub_roger_binary(
            "resume",
            &serde_json::to_string_pretty(&serde_json::json!({
                "schema_id": "rr.robot.resume.v1",
                "command": "rr resume",
                "robot_format": "json",
                "outcome": "blocked",
                "generated_at": "2026-04-15T00:00:00Z",
                "exit_code": 3,
                "warnings": ["no matching session found for the requested target"],
                "repair_actions": ["run rr review --pr <number> to create a new session"],
                "data": {"reason": "no review session exists", "candidates": []}
            }))
            .expect("serialize blocked resume envelope"),
            3,
            "{}",
            0,
        );
        let preflight = BridgePreflight {
            roger_binary_found: true,
            roger_data_dir_exists: true,
            gh_available: true,
        };
        let mut intent = sample_intent();
        intent.action = "resume_review".to_owned();
        let resp = handle_bridge_intent(&intent, &preflight, &stub_rr);
        assert!(!resp.ok);
        assert_eq!(
            resp.failure_kind,
            Some(BridgeFailureKind::CliOutcomeNotSafe)
        );
        assert!(resp.candidates.is_none());
    }

    #[test]
    fn handle_bridge_intent_unknown_action() {
        let preflight = BridgePreflight {
            roger_binary_found: true,
            roger_data_dir_exists: true,
            gh_available: true,
        };
        let mut intent = sample_intent();
        intent.action = "delete_repo".to_owned();
        let resp = handle_bridge_intent(&intent, &preflight, Path::new("/usr/local/bin/rr"));
        assert!(!resp.ok);
        assert!(resp.guidance.unwrap().contains("Supported actions"));
    }

    #[test]
    fn bridge_response_serialization() {
        let resp = BridgeResponse::failure("start_review", "not ready", "install Roger first");
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: BridgeResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.ok, false);
        assert_eq!(parsed.guidance, Some("install Roger first".to_owned()));
    }

    #[test]
    fn persist_extension_identity_writes_standard_registry_path() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store_root = temp.path().join(".roger");
        let extension_id = "abcdefghijklmnopabcdefghijklmnop";

        let path =
            persist_extension_identity(&store_root, extension_id).expect("persisted extension id");

        assert_eq!(path, store_root.join("bridge/extension-id"));
        let contents = fs::read_to_string(path).expect("registry file contents");
        assert_eq!(contents.trim(), extension_id);
    }

    #[test]
    fn registration_action_is_accepted_without_launch_preflight() {
        let temp = tempfile::tempdir().expect("tempdir");
        static CWD_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        let _guard = CWD_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("lock current_dir guard");
        let previous_dir = std::env::current_dir().expect("current dir");
        std::env::set_current_dir(temp.path()).expect("set current dir");

        let intent = BridgeLaunchIntent {
            action: "register_extension_identity".to_owned(),
            owner: "roger".to_owned(),
            repo: "roger-reviewer".to_owned(),
            pr_number: 0,
            head_ref: None,
            instance: None,
            session_id: None,
            extension_id: Some("abcdefghijklmnopabcdefghijklmnop".to_owned()),
            browser: Some("chrome".to_owned()),
            finding_id: None,
            state: None,
            draft_id: None,
            body: None,
            query: None,
        };
        let preflight = BridgePreflight {
            roger_binary_found: false,
            roger_data_dir_exists: false,
            gh_available: false,
        };

        let resp = handle_bridge_intent(&intent, &preflight, Path::new("/missing/rr"));
        std::env::set_current_dir(previous_dir).expect("restore current dir");

        assert!(resp.ok);
        assert_eq!(resp.action, "register_extension_identity");
    }

    #[test]
    fn registration_action_fails_closed_on_invalid_extension_id() {
        let intent = BridgeLaunchIntent {
            action: "register_extension_identity".to_owned(),
            owner: "roger".to_owned(),
            repo: "roger-reviewer".to_owned(),
            pr_number: 0,
            head_ref: None,
            instance: None,
            session_id: None,
            extension_id: Some("INVALID-ID".to_owned()),
            browser: Some("chrome".to_owned()),
            finding_id: None,
            state: None,
            draft_id: None,
            body: None,
            query: None,
        };
        let preflight = BridgePreflight {
            roger_binary_found: true,
            roger_data_dir_exists: true,
            gh_available: true,
        };
        let resp = handle_bridge_intent(&intent, &preflight, Path::new("/usr/local/bin/rr"));

        assert!(!resp.ok);
        assert_eq!(resp.action, "register_extension_identity");
        assert!(
            resp.guidance
                .as_deref()
                .is_some_and(|guidance| guidance.contains("32-character lowercase"))
        );
    }

    // -- Bounded local-parity action tests -------------------------------------

    /// Write an rr stub that records the argv it was invoked with (to a sibling
    /// `argv.txt`) and prints `payload` for the given first-arg `command`.
    #[cfg(unix)]
    fn write_recording_stub(
        command: &str,
        payload: &str,
        exit: i32,
    ) -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("rr-stub");
        let argv_file = dir.path().join("argv.txt");
        // Match on a single leading token for simple commands, or the two-token
        // "send edit" alias when command is "send".
        let script = format!(
            r#"#!/bin/sh
printf '%s\n' "$*" > "{argv_path}"
case "$1" in
  {command})
    cat <<'EOF'
{payload}
EOF
    exit {exit}
    ;;
  *)
    echo "unexpected args: $@" >&2
    exit 64
    ;;
esac
"#,
            argv_path = argv_file.display(),
        );
        fs::write(&path, script).expect("write rr stub");
        let mut perms = fs::metadata(&path).expect("rr stub metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&path, perms).expect("chmod rr stub");
        (dir, path, argv_file)
    }

    fn ready_preflight() -> BridgePreflight {
        BridgePreflight {
            roger_binary_found: true,
            roger_data_dir_exists: true,
            gh_available: true,
        }
    }

    #[cfg(unix)]
    #[test]
    fn triage_finding_dispatches_rr_triage_argv_and_relays_items() {
        let (_dir, stub, argv_file) = write_recording_stub(
            "triage",
            &serde_json::to_string(&serde_json::json!({
                "schema_id": "rr.robot.triage.v1",
                "command": "rr triage",
                "robot_format": "json",
                "outcome": "complete",
                "generated_at": "2026-04-15T00:00:00Z",
                "exit_code": 0,
                "warnings": [],
                "repair_actions": [],
                "data": {
                    "session_id": "s-1",
                    "triage_state": "accepted",
                    "count": 1,
                    "items": [{"id": "finding-1", "title": "x", "triage_state": "accepted", "outbound_state": "not_drafted"}]
                }
            }))
            .unwrap(),
            0,
        );
        let intent = BridgeLaunchIntent {
            action: "triage_finding".to_owned(),
            owner: "acme".to_owned(),
            repo: "widgets".to_owned(),
            pr_number: 42,
            finding_id: Some("finding-1".to_owned()),
            state: Some("accepted".to_owned()),
            ..Default::default()
        };
        let resp = handle_bridge_intent(&intent, &ready_preflight(), &stub);
        assert!(resp.ok, "guidance: {:?}", resp.guidance);
        let argv = fs::read_to_string(&argv_file).unwrap();
        for expected in [
            "triage",
            "--repo acme/widgets",
            "--pr 42",
            "--finding finding-1",
            "--state accepted",
            "--robot",
        ] {
            assert!(argv.contains(expected), "argv missing {expected:?}: {argv}");
        }
        let findings = resp.findings.expect("triage relays findings mirror");
        assert_eq!(findings["items"][0]["id"], "finding-1");
        assert_eq!(findings["triage_state"], "accepted");
    }

    #[test]
    fn triage_finding_requires_finding_and_state() {
        let intent = BridgeLaunchIntent {
            action: "triage_finding".to_owned(),
            owner: "acme".to_owned(),
            repo: "widgets".to_owned(),
            pr_number: 42,
            ..Default::default()
        };
        let resp =
            handle_bridge_intent(&intent, &ready_preflight(), Path::new("/usr/local/bin/rr"));
        assert!(!resp.ok);
        assert!(resp.guidance.unwrap().to_lowercase().contains("finding"));
    }

    #[cfg(unix)]
    #[test]
    fn show_drafts_relays_findings_items_with_draft_ids() {
        let (_dir, stub, argv_file) = write_recording_stub(
            "findings",
            &serde_json::to_string(&serde_json::json!({
                "schema_id": "rr.robot.findings.v1",
                "command": "rr findings",
                "robot_format": "json",
                "outcome": "complete",
                "generated_at": "2026-04-15T00:00:00Z",
                "exit_code": 0,
                "warnings": [],
                "repair_actions": [],
                "data": {
                    "session_id": "s-1",
                    "count": 1,
                    "items": [{
                        "finding_id": "finding-1",
                        "title": "x",
                        "outbound_state": "awaiting_approval",
                        "outbound_detail": {"draft_id": "draft-1", "draft_batch_id": "batch-1"}
                    }]
                }
            }))
            .unwrap(),
            0,
        );
        let intent = BridgeLaunchIntent {
            action: "show_drafts".to_owned(),
            owner: "acme".to_owned(),
            repo: "widgets".to_owned(),
            pr_number: 42,
            ..Default::default()
        };
        let resp = handle_bridge_intent(&intent, &ready_preflight(), &stub);
        assert!(resp.ok, "guidance: {:?}", resp.guidance);
        let argv = fs::read_to_string(&argv_file).unwrap();
        assert!(argv.contains("findings"));
        assert!(
            !argv.contains("--surface"),
            "findings must not get --surface: {argv}"
        );
        let drafts = resp.drafts.expect("show_drafts relays drafts mirror");
        assert_eq!(drafts["items"][0]["outbound_detail"]["draft_id"], "draft-1");
        assert!(resp.findings.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn revise_draft_dispatches_send_edit_and_relays_success() {
        let (_dir, stub, argv_file) = write_recording_stub(
            "send",
            "rr send edit recorded revision 2 for the outbound draft",
            0,
        );
        let intent = BridgeLaunchIntent {
            action: "revise_draft".to_owned(),
            owner: "acme".to_owned(),
            repo: "widgets".to_owned(),
            pr_number: 42,
            draft_id: Some("draft-1".to_owned()),
            body: Some("new body text".to_owned()),
            ..Default::default()
        };
        let resp = handle_bridge_intent(&intent, &ready_preflight(), &stub);
        assert!(resp.ok, "guidance: {:?}", resp.guidance);
        let argv = fs::read_to_string(&argv_file).unwrap();
        assert!(argv.contains("send edit"), "argv: {argv}");
        assert!(argv.contains("--draft draft-1"), "argv: {argv}");
        assert!(argv.contains("--body-file"), "argv: {argv}");
        assert!(
            !argv.contains("--robot"),
            "send edit is not a robot surface: {argv}"
        );
        assert!(resp.message.contains("recorded revision"));
    }

    #[cfg(unix)]
    #[test]
    fn revise_draft_relays_cli_refusal_as_failure() {
        let (_dir, stub, _argv) = write_recording_stub(
            "send",
            "rr send edit refused to edit a draft whose batch was already posted to GitHub",
            2,
        );
        let intent = BridgeLaunchIntent {
            action: "revise_draft".to_owned(),
            owner: "acme".to_owned(),
            repo: "widgets".to_owned(),
            pr_number: 42,
            draft_id: Some("draft-1".to_owned()),
            body: Some("new body".to_owned()),
            ..Default::default()
        };
        let resp = handle_bridge_intent(&intent, &ready_preflight(), &stub);
        assert!(!resp.ok);
        assert_eq!(
            resp.failure_kind,
            Some(BridgeFailureKind::CliOutcomeNotSafe)
        );
        assert!(resp.guidance.unwrap().contains("already posted"));
    }

    #[cfg(unix)]
    #[test]
    fn search_dispatches_rr_search_and_relays_results() {
        let (_dir, stub, argv_file) = write_recording_stub(
            "search",
            &serde_json::to_string(&serde_json::json!({
                "schema_id": "rr.robot.search.v1",
                "command": "rr search",
                "robot_format": "json",
                "outcome": "complete",
                "generated_at": "2026-04-15T00:00:00Z",
                "exit_code": 0,
                "warnings": [],
                "repair_actions": [],
                "data": {"count": 1, "matches": [{"title": "prior finding"}]}
            }))
            .unwrap(),
            0,
        );
        let intent = BridgeLaunchIntent {
            action: "search".to_owned(),
            owner: "acme".to_owned(),
            repo: "widgets".to_owned(),
            pr_number: 0,
            query: Some("retry loop".to_owned()),
            ..Default::default()
        };
        let resp = handle_bridge_intent(&intent, &ready_preflight(), &stub);
        assert!(resp.ok, "guidance: {:?}", resp.guidance);
        let argv = fs::read_to_string(&argv_file).unwrap();
        assert!(argv.contains("--query retry loop"), "argv: {argv}");
        assert!(argv.contains("--repo acme/widgets"), "argv: {argv}");
        let results = resp.search_results.expect("search relays results");
        assert_eq!(results["matches"][0]["title"], "prior finding");
    }

    #[cfg(unix)]
    #[test]
    fn request_clarification_dispatches_rr_clarify_forward_compatibly() {
        // rr clarify is a parallel workstream; this proves the argv mapping and
        // relay against a stub, so the relay works the moment the command lands.
        let (_dir, stub, argv_file) = write_recording_stub(
            "clarify",
            &serde_json::to_string(&serde_json::json!({
                "schema_id": "rr.robot.clarify.v1",
                "command": "rr clarify",
                "robot_format": "json",
                "outcome": "complete",
                "generated_at": "2026-04-15T00:00:00Z",
                "exit_code": 0,
                "warnings": [],
                "repair_actions": [],
                "data": {"clarification_id": "clar-1", "finding_id": "finding-1"}
            }))
            .unwrap(),
            0,
        );
        let intent = BridgeLaunchIntent {
            action: "request_clarification".to_owned(),
            owner: "acme".to_owned(),
            repo: "widgets".to_owned(),
            pr_number: 42,
            finding_id: Some("finding-1".to_owned()),
            body: Some("why is this unsafe?".to_owned()),
            ..Default::default()
        };
        let resp = handle_bridge_intent(&intent, &ready_preflight(), &stub);
        assert!(resp.ok, "guidance: {:?}", resp.guidance);
        let argv = fs::read_to_string(&argv_file).unwrap();
        assert!(argv.contains("clarify"), "argv: {argv}");
        assert!(argv.contains("--finding finding-1"), "argv: {argv}");
        assert!(argv.contains("--body why is this unsafe?"), "argv: {argv}");
        let ack = resp.clarification_ack.expect("clarify relays ack");
        assert_eq!(ack["clarification_id"], "clar-1");
    }

    #[cfg(unix)]
    #[test]
    fn timeline_dispatches_rr_timeline_forward_compatibly() {
        let (_dir, stub, argv_file) = write_recording_stub(
            "timeline",
            &serde_json::to_string(&serde_json::json!({
                "schema_id": "rr.robot.timeline.v1",
                "command": "rr timeline",
                "robot_format": "json",
                "outcome": "complete",
                "generated_at": "2026-04-15T00:00:00Z",
                "exit_code": 0,
                "warnings": [],
                "repair_actions": [],
                "data": {"events": [{"kind": "run"}]}
            }))
            .unwrap(),
            0,
        );
        let intent = BridgeLaunchIntent {
            action: "timeline".to_owned(),
            owner: "acme".to_owned(),
            repo: "widgets".to_owned(),
            pr_number: 42,
            ..Default::default()
        };
        let resp = handle_bridge_intent(&intent, &ready_preflight(), &stub);
        assert!(resp.ok, "guidance: {:?}", resp.guidance);
        let argv = fs::read_to_string(&argv_file).unwrap();
        assert!(argv.contains("timeline"), "argv: {argv}");
        let timeline = resp.timeline.expect("timeline relays data");
        assert_eq!(timeline["events"][0]["kind"], "run");
    }

    #[cfg(unix)]
    #[test]
    fn local_parity_action_fails_closed_when_command_errors() {
        // Gated commands (clarify/timeline) that aren't in the CLI yet surface a
        // non-canonical payload; the relay must fail closed, not fake success.
        let (_dir, stub, _argv) = write_recording_stub("clarify", "not json", 0);
        let intent = BridgeLaunchIntent {
            action: "request_clarification".to_owned(),
            owner: "acme".to_owned(),
            repo: "widgets".to_owned(),
            pr_number: 42,
            finding_id: Some("finding-1".to_owned()),
            body: Some("q".to_owned()),
            ..Default::default()
        };
        let resp = handle_bridge_intent(&intent, &ready_preflight(), &stub);
        assert!(!resp.ok);
        assert_eq!(
            resp.failure_kind,
            Some(BridgeFailureKind::RobotSchemaMismatch)
        );
    }
}
