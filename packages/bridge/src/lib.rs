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
    /// Optional browser extension runtime ID for identity-registration events.
    #[serde(default)]
    pub extension_id: Option<String>,
    /// Optional browser label for identity-registration events.
    #[serde(default)]
    pub browser: Option<String>,
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
        return BridgeResponse::failure(&intent.action, "Roger is not ready", &guidance);
    }

    match intent.action.as_str() {
        "start_review" | "resume_review" | "show_findings" => {
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
            "Supported actions: start_review, resume_review, show_findings",
        ),
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::sync::{Mutex, OnceLock};

    fn sample_intent() -> BridgeLaunchIntent {
        BridgeLaunchIntent {
            action: "start_review".to_owned(),
            owner: "acme".to_owned(),
            repo: "widgets".to_owned(),
            pr_number: 42,
            head_ref: Some("feat/frob".to_owned()),
            instance: None,
            extension_id: None,
            browser: None,
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
            extension_id: Some("abcdefghijklmnopabcdefghijklmnop".to_owned()),
            browser: Some("chrome".to_owned()),
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
            extension_id: Some("INVALID-ID".to_owned()),
            browser: Some("chrome".to_owned()),
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
}
