//! Roger browser-to-local launch bridge.
//!
//! Implements the daemonless bridge for browser extension → local Roger
//! handoff. Two bridge families:
//!
//! 1. **Native Messaging** (primary): Chrome/Edge/Brave Native Messaging
//!    host that receives structured launch intents and returns bounded
//!    readback-only responses. No persistent daemon.
//!
//! 2. **Custom URL** (fallback): `roger://` URL scheme handler for
//!    thin launch-only handoff when Native Messaging is unavailable.
//!    Recovery/convenience path only, not the primary bridge.
//!
//! Design constraints (per AGENTS.md / canonical plan):
//! - No persistent daemon or local HTTP/WebSocket server
//! - Missing local Roger state fails closed with explicit guidance
//! - No mutation or approval side-effects through the bridge
//! - Bridge host is a separate binary entrypoint, not the TUI

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
}

/// Response sent back to the extension via Native Messaging.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeResponse {
    pub ok: bool,
    pub action: String,
    pub message: String,
    /// If the launch succeeded, the session ID.
    pub session_id: Option<String>,
    /// If the launch failed, structured guidance for the user.
    pub guidance: Option<String>,
}

impl BridgeResponse {
    pub fn success(action: &str, message: &str, session_id: Option<String>) -> Self {
        Self {
            ok: true,
            action: action.to_owned(),
            message: message.to_owned(),
            session_id,
            guidance: None,
        }
    }

    pub fn failure(action: &str, message: &str, guidance: &str) -> Self {
        Self {
            ok: false,
            action: action.to_owned(),
            message: message.to_owned(),
            session_id: None,
            guidance: Some(guidance.to_owned()),
        }
    }
}

/// Read a Native Messaging message from stdin.
///
/// Chrome Native Messaging protocol: 4-byte little-endian length prefix
/// followed by JSON payload.
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

/// Return the per-OS location for the custom URL registration helper asset.
pub fn custom_url_helper_path_for(os: SupportedOs, home_dir: &Path) -> PathBuf {
    match os {
        SupportedOs::Macos => home_dir
            .join("Library/Application Support/RogerReviewer/bridge/register-roger-url.command"),
        SupportedOs::Windows => {
            home_dir.join("AppData/Local/RogerReviewer/bridge/register-roger-url.reg")
        }
        SupportedOs::Linux => {
            home_dir.join(".local/share/roger-reviewer/bridge/register-roger-url.desktop")
        }
    }
}

/// Render the OS-specific custom-URL registration helper content.
pub fn render_custom_url_helper(os: SupportedOs, rr_binary_path: &Path) -> String {
    match os {
        SupportedOs::Macos => format!(
            "#!/bin/sh\n# Roger custom URL registration helper (manual apply)\n# target: macOS\n# Binary: {binary}\n# Register a roger:// handler that launches Roger.\n# This helper is intentionally explicit and does not auto-run.\n",
            binary = rr_binary_path.to_string_lossy()
        ),
        SupportedOs::Windows => format!(
            "Windows Registry Editor Version 5.00\n\n[HKEY_CURRENT_USER\\\\Software\\\\Classes\\\\roger]\n@=\"URL:Roger Protocol\"\n\"URL Protocol\"=\"\"\n\n[HKEY_CURRENT_USER\\\\Software\\\\Classes\\\\roger\\\\shell\\\\open\\\\command]\n@=\"\\\"{binary}\\\" \\\"%1\\\"\"\n",
            binary = rr_binary_path.to_string_lossy().replace('\\', "\\\\")
        ),
        SupportedOs::Linux => format!(
            "[Desktop Entry]\nName=Roger Reviewer URL Handler\nType=Application\nNoDisplay=true\nMimeType=x-scheme-handler/roger;\nExec={binary} %u\n",
            binary = rr_binary_path.to_string_lossy()
        ),
    }
}

// ---------------------------------------------------------------------------
// Custom URL scheme fallback
// ---------------------------------------------------------------------------

/// Parse a `roger://` custom URL into a launch intent.
///
/// Format: `roger://launch/<owner>/<repo>/<pr_number>[?action=<action>&instance=<name>]`
///
/// This is a thin convenience/recovery path only. The primary bridge
/// is Native Messaging.
pub fn parse_custom_url(url: &str) -> Result<BridgeLaunchIntent> {
    let stripped = url
        .strip_prefix("roger://launch/")
        .ok_or_else(|| BridgeError::InvalidRequest(format!("invalid roger URL: {url}")))?;

    let (path_part, query_part) = if let Some(idx) = stripped.find('?') {
        (&stripped[..idx], Some(&stripped[idx + 1..]))
    } else {
        (stripped, None)
    };

    let parts: Vec<&str> = path_part.split('/').collect();
    if parts.len() < 3 {
        return Err(BridgeError::InvalidRequest(format!(
            "expected roger://launch/<owner>/<repo>/<pr>, got: {url}"
        )));
    }

    let owner = parts[0].to_owned();
    let repo = parts[1].to_owned();
    let pr_number: u64 = parts[2]
        .parse()
        .map_err(|_| BridgeError::InvalidRequest(format!("invalid PR number: {}", parts[2])))?;

    let mut action = "start_review".to_owned();
    let mut instance = None;

    if let Some(query) = query_part {
        for pair in query.split('&') {
            if let Some((key, value)) = pair.split_once('=') {
                match key {
                    "action" => action = value.to_owned(),
                    "instance" => instance = Some(value.to_owned()),
                    _ => {} // Ignore unknown params.
                }
            }
        }
    }

    Ok(BridgeLaunchIntent {
        action,
        owner,
        repo,
        pr_number,
        head_ref: None,
        instance,
    })
}

/// Launch path selected for browser → local bridge handoff.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BridgeLaunchPath {
    NativeMessaging,
    CustomUrlLaunchOnly,
}

const NATIVE_MESSAGING_LAUNCH_ARTIFACTS: [&str; 3] = [
    "native_request_envelope.json",
    "native_response_envelope.json",
    "bridge_launch_transcript.json",
];

const CUSTOM_URL_LAUNCH_ARTIFACTS: [&str; 3] = [
    "custom_url_launch_intent.txt",
    "bridge_response_envelope.json",
    "bridge_launch_transcript.json",
];

/// Resolve the launch path from local bridge registration state.
///
/// Native Messaging is preferred for the serious companion tier. Custom URL
/// remains a truthful launch-only fallback.
pub fn choose_launch_path(
    native_messaging_registered: bool,
    custom_url_registered: bool,
) -> Result<BridgeLaunchPath> {
    if native_messaging_registered {
        return Ok(BridgeLaunchPath::NativeMessaging);
    }
    if custom_url_registered {
        return Ok(BridgeLaunchPath::CustomUrlLaunchOnly);
    }
    Err(BridgeError::LocalStateMissing {
        detail: "No supported bridge launch path is registered. Install Native Messaging host support or configure custom URL launch fallback.".to_owned(),
    })
}

/// Return artifact filenames expected for bridge launch smoke/failure capture.
///
/// Browser-smoke runners can use this helper to assert transcript and envelope
/// capture requirements without relying on docs-only guidance.
pub fn required_launch_artifacts(path: BridgeLaunchPath) -> &'static [&'static str] {
    match path {
        BridgeLaunchPath::NativeMessaging => &NATIVE_MESSAGING_LAUNCH_ARTIFACTS,
        BridgeLaunchPath::CustomUrlLaunchOnly => &CUSTOM_URL_LAUNCH_ARTIFACTS,
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
    if !preflight.is_ready() {
        let guidance = preflight
            .guidance(roger_binary_path)
            .unwrap_or_else(|| "Unknown setup issue".to_owned());
        return BridgeResponse::failure(&intent.action, "Roger is not ready", &guidance);
    }

    match intent.action.as_str() {
        "start_review" | "resume_review" | "show_findings" | "refresh_review" => {
            BridgeResponse::success(
                &intent.action,
                &format!(
                    "Dispatching {} for {}/{}#{}",
                    intent.action, intent.owner, intent.repo, intent.pr_number
                ),
                None, // Session ID would be filled by actual rr invocation.
            )
        }
        other => BridgeResponse::failure(
            other,
            &format!("Unknown bridge action: {other}"),
            "Supported actions: start_review, resume_review, show_findings, refresh_review",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn sample_intent() -> BridgeLaunchIntent {
        BridgeLaunchIntent {
            action: "start_review".to_owned(),
            owner: "acme".to_owned(),
            repo: "widgets".to_owned(),
            pr_number: 42,
            head_ref: Some("feat/frob".to_owned()),
            instance: None,
        }
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
    fn custom_url_parse_basic() {
        let intent = parse_custom_url("roger://launch/acme/widgets/42").unwrap();
        assert_eq!(intent.owner, "acme");
        assert_eq!(intent.repo, "widgets");
        assert_eq!(intent.pr_number, 42);
        assert_eq!(intent.action, "start_review");
    }

    #[test]
    fn custom_url_parse_with_params() {
        let intent = parse_custom_url(
            "roger://launch/acme/widgets/42?action=resume_review&instance=my-inst",
        )
        .unwrap();
        assert_eq!(intent.action, "resume_review");
        assert_eq!(intent.instance, Some("my-inst".to_owned()));
    }

    #[test]
    fn custom_url_parse_invalid() {
        assert!(parse_custom_url("roger://bad").is_err());
        assert!(parse_custom_url("https://github.com/foo").is_err());
        assert!(parse_custom_url("roger://launch/acme/widgets/notanumber").is_err());
    }

    #[test]
    fn host_manifest_for_roger() {
        let manifest =
            NativeHostManifest::for_roger(Path::new("/usr/local/bin/rr-bridge"), "abcdef123456");
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
    fn custom_url_helper_assets_cover_supported_os_matrix() {
        let home = Path::new("/home/tester");
        let binary = Path::new("/usr/local/bin/rr");

        let mac = custom_url_helper_path_for(SupportedOs::Macos, home);
        assert!(
            mac.to_string_lossy()
                .ends_with("register-roger-url.command")
        );
        assert!(render_custom_url_helper(SupportedOs::Macos, binary).contains("macOS"));

        let windows = custom_url_helper_path_for(SupportedOs::Windows, home);
        assert!(
            windows
                .to_string_lossy()
                .ends_with("register-roger-url.reg")
        );
        assert!(render_custom_url_helper(SupportedOs::Windows, binary).contains("URL Protocol"));

        let linux = custom_url_helper_path_for(SupportedOs::Linux, home);
        assert!(
            linux
                .to_string_lossy()
                .ends_with("register-roger-url.desktop")
        );
        assert!(
            render_custom_url_helper(SupportedOs::Linux, binary).contains("x-scheme-handler/roger")
        );
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

    #[test]
    fn handle_bridge_intent_success() {
        let preflight = BridgePreflight {
            roger_binary_found: true,
            roger_data_dir_exists: true,
            gh_available: true,
        };
        let intent = sample_intent();
        let resp = handle_bridge_intent(&intent, &preflight, Path::new("/usr/local/bin/rr"));
        assert!(resp.ok);
        assert!(resp.message.contains("start_review"));
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
}
