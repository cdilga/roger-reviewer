#![cfg(unix)]

use roger_bridge::{BridgeLaunchIntent, BridgeResponse, NativeHostManifest};
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tempfile::tempdir;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("packages parent")
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn rr_binary() -> PathBuf {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_rr") {
        return PathBuf::from(path);
    }
    workspace_root().join("target/debug/rr")
}

fn parse_robot(stdout: &[u8]) -> Value {
    serde_json::from_slice(stdout).expect("parse robot output")
}

fn encode_native_intent(intent: &BridgeLaunchIntent) -> Vec<u8> {
    let json = serde_json::to_vec(intent).expect("serialize native intent");
    let len = json.len() as u32;
    let mut wire = Vec::with_capacity(4 + json.len());
    wire.extend_from_slice(&len.to_le_bytes());
    wire.extend_from_slice(&json);
    wire
}

/// Split a native-messaging byte stream into its length-prefixed JSON frames.
fn decode_native_frames(stdout: &[u8]) -> Vec<Value> {
    let mut frames = Vec::new();
    let mut offset = 0;
    while offset + 4 <= stdout.len() {
        let len = u32::from_le_bytes([
            stdout[offset],
            stdout[offset + 1],
            stdout[offset + 2],
            stdout[offset + 3],
        ]) as usize;
        offset += 4;
        assert!(
            offset + len <= stdout.len(),
            "native host frame length prefix overruns buffer"
        );
        let frame: Value = serde_json::from_slice(&stdout[offset..offset + len])
            .expect("decode native host frame payload");
        frames.push(frame);
        offset += len;
    }
    assert_eq!(offset, stdout.len(), "trailing bytes after final frame");
    frames
}

/// The launch path streams an ack/progress frame ahead of the final response.
/// The first frame is the `host_started` ack; the final frame is the
/// `BridgeResponse`. Returns the decoded final response.
fn decode_native_response(stdout: &[u8]) -> BridgeResponse {
    let frames = decode_native_frames(stdout);
    assert!(!frames.is_empty(), "native host produced no frames");
    let (last, progress) = frames.split_last().expect("at least one frame");
    assert_eq!(
        progress
            .first()
            .and_then(|f| f.get("stage"))
            .and_then(|s| s.as_str()),
        Some("host_started"),
        "first frame must be the host_started ack"
    );
    assert!(
        last.get("schema").is_none(),
        "final response frame must not carry the launch-progress schema"
    );
    serde_json::from_value(last.clone()).expect("decode native host response payload")
}

#[test]
fn extension_setup_writes_native_host_launcher_that_normalizes_browser_argv() {
    let temp = tempdir().expect("tempdir");
    let store_root = temp.path().join("roger-store");
    let install_root = temp.path().join("install-root");
    let profile_root = temp.path().join("profile-root");
    fs::create_dir_all(&profile_root).expect("create profile root");

    let rr = rr_binary();
    assert!(rr.exists(), "expected rr binary at {}", rr.display());

    let setup_output = Command::new(&rr)
        .arg("extension")
        .arg("setup")
        .arg("--browser")
        .arg("edge")
        .arg("--install-root")
        .arg(&install_root)
        .arg("--robot")
        .env("RR_STORE_ROOT", &store_root)
        .env("RR_EXTENSION_PROFILE_ROOT", &profile_root)
        .current_dir(workspace_root())
        .output()
        .expect("run rr extension setup");
    assert!(
        setup_output.status.success(),
        "setup failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&setup_output.stdout),
        String::from_utf8_lossy(&setup_output.stderr)
    );

    let setup_payload = parse_robot(&setup_output.stdout);
    assert_eq!(setup_payload["outcome"], "complete");
    let manifest_path = PathBuf::from(
        setup_payload["data"]["native_manifest_path"]
            .as_str()
            .expect("native_manifest_path"),
    );
    assert!(
        manifest_path.exists(),
        "native host manifest missing: {}",
        manifest_path.display()
    );
    let manifest: NativeHostManifest = serde_json::from_str(
        &fs::read_to_string(&manifest_path).expect("read native host manifest"),
    )
    .expect("parse native host manifest");
    let launcher_path = PathBuf::from(&manifest.path);
    assert!(
        launcher_path.exists(),
        "native host launcher missing: {}",
        launcher_path.display()
    );
    assert!(
        launcher_path.extension().and_then(|value| value.to_str()) == Some("sh"),
        "expected shell launcher path, got {}",
        launcher_path.display()
    );

    let launcher_contents = fs::read_to_string(&launcher_path).expect("read launcher script");
    assert!(
        launcher_contents.contains("--native-host"),
        "launcher should force native-host mode: {launcher_contents}"
    );
    assert!(
        launcher_contents.contains("RR_STORE_ROOT"),
        "launcher should pin a stable default store root: {launcher_contents}"
    );
    assert!(
        launcher_contents.contains(".roger"),
        "launcher should default RR_STORE_ROOT to HOME/.roger: {launcher_contents}"
    );
    assert!(
        launcher_contents.contains("/opt/homebrew/bin:/usr/local/bin"),
        "launcher should prepend common gh/toolchain paths for browser-launched hosts: {launcher_contents}"
    );
    assert!(
        !launcher_contents.contains("$@"),
        "launcher must not forward browser argv directly: {launcher_contents}"
    );

    let intent = BridgeLaunchIntent {
        action: "start_review".to_owned(),
        owner: "owner".to_owned(),
        repo: "repo".to_owned(),
        pr_number: 42,
        head_ref: None,
        instance: None,
        session_id: None,
        extension_id: None,
        browser: Some("edge".to_owned()),
        finding_id: None,
        state: None,
        draft_id: None,
        body: None,
        query: None,
    };
    let mut child = Command::new(Path::new(&manifest.path))
        .arg("chrome-extension://djbjigobohmlljboggckmhhnoeldinlp/")
        .arg("--parent-window=0")
        .arg("--unsupported-edge-launch-arg=1")
        .env("RR_STORE_ROOT", temp.path().join("missing-store"))
        .current_dir(temp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn native host launcher");
    child
        .stdin
        .as_mut()
        .expect("native host stdin")
        .write_all(&encode_native_intent(&intent))
        .expect("write native host request");
    let host_output = child.wait_with_output().expect("wait for native host");
    let response = decode_native_response(&host_output.stdout);
    assert_eq!(response.action, "start_review");
    assert!(
        !response.ok,
        "preflight should fail closed for missing store root"
    );
    assert!(
        !String::from_utf8_lossy(&host_output.stderr).contains("unknown command:"),
        "launcher should normalize browser argv instead of triggering CLI parse"
    );
}
