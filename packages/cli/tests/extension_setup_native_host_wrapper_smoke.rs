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

fn decode_native_response(stdout: &[u8]) -> BridgeResponse {
    assert!(
        stdout.len() >= 4,
        "native host output missing length prefix: {} bytes",
        stdout.len()
    );
    let len = u32::from_le_bytes([stdout[0], stdout[1], stdout[2], stdout[3]]) as usize;
    assert_eq!(
        stdout.len(),
        4 + len,
        "native host output length prefix mismatch"
    );
    serde_json::from_slice(&stdout[4..]).expect("decode native host response payload")
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
        extension_id: None,
        browser: Some("edge".to_owned()),
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
