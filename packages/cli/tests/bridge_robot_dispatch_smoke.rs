#![cfg(unix)]

use roger_bridge::{BridgeLaunchIntent, BridgeResponse};
use roger_cli::CliRuntime;
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
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

fn prepend_path(dir: &Path) -> OsString {
    let mut value = OsString::from(dir.as_os_str());
    if let Some(existing) = std::env::var_os("PATH") {
        value.push(":");
        value.push(existing);
    }
    value
}

fn write_opencode_stub(dir: &Path) -> PathBuf {
    let path = dir.join("opencode-stub");
    fs::write(&path, "#!/bin/sh\nexit 0\n").expect("write opencode stub");
    let mut perms = fs::metadata(&path)
        .expect("opencode metadata")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).expect("chmod opencode stub");
    path
}

fn write_gh_status_stub(dir: &Path) -> PathBuf {
    let path = dir.join("gh");
    let script = r#"#!/bin/sh
if [ "$1" = "auth" ] && [ "$2" = "status" ]; then
  exit 0
fi
exit 1
"#;
    fs::write(&path, script).expect("write gh stub");
    let mut perms = fs::metadata(&path).expect("gh metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).expect("chmod gh stub");
    path
}

fn run_rr_process_with_stdin(
    runtime: &CliRuntime,
    stdin_bytes: &[u8],
    path_override_dir: &Path,
    args: &[&str],
) -> std::process::Output {
    let mut cmd = if let Ok(rr_bin) = std::env::var("CARGO_BIN_EXE_rr") {
        let mut cmd = Command::new(rr_bin);
        cmd.current_dir(&runtime.cwd);
        cmd
    } else {
        let workspace = workspace_root();
        let local_rr = workspace.join("target/debug/rr");
        if local_rr.exists() {
            let mut cmd = Command::new(local_rr);
            cmd.current_dir(&runtime.cwd);
            cmd
        } else {
            let mut cmd = Command::new("cargo");
            cmd.arg("run")
                .arg("-q")
                .arg("-p")
                .arg("roger-cli")
                .arg("--bin")
                .arg("rr")
                .arg("--")
                .current_dir(workspace);
            cmd
        }
    };

    cmd.env("RR_STORE_ROOT", &runtime.store_root)
        .env("RR_OPENCODE_BIN", &runtime.opencode_bin)
        .env("PATH", prepend_path(path_override_dir))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().expect("spawn rr process");
    {
        let stdin = child.stdin.as_mut().expect("child stdin");
        stdin
            .write_all(stdin_bytes)
            .expect("write native messaging request");
    }
    child
        .wait_with_output()
        .expect("wait for rr process output")
}

#[test]
fn native_host_bridge_dispatches_real_robot_review_and_status() {
    let temp = tempdir().expect("tempdir");
    let stub_dir = temp.path().join("bin");
    fs::create_dir_all(&stub_dir).expect("create stub dir");
    write_gh_status_stub(&stub_dir);
    let opencode_bin = write_opencode_stub(&stub_dir);

    let runtime = CliRuntime {
        cwd: temp.path().to_path_buf(),
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };
    fs::create_dir_all(&runtime.store_root).expect("create store root");

    let intent = BridgeLaunchIntent {
        action: "start_review".to_owned(),
        owner: "owner".to_owned(),
        repo: "repo".to_owned(),
        pr_number: 42,
        head_ref: None,
        instance: None,
        extension_id: None,
        browser: None,
    };
    let output =
        run_rr_process_with_stdin(&runtime, &encode_native_intent(&intent), &stub_dir, &[]);

    assert!(
        output.status.success(),
        "native host dispatch failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response = decode_native_response(&output.stdout);
    assert!(response.ok, "bridge response was not ok: {:?}", response);
    let session_id = response
        .session_id
        .clone()
        .expect("bridge response should include a session id");
    assert!(
        session_id.starts_with("session-"),
        "unexpected session id: {session_id}"
    );
    assert_eq!(response.attention_state.as_deref(), Some("review_launched"));
    let status = response
        .status
        .expect("bridge response should include status");
    assert_eq!(status.schema_id, "rr.robot.status.v1");
    assert_eq!(status.session_id, session_id);
    assert_eq!(status.attention_state, "review_launched");
    assert!(
        response.message.contains("rr review"),
        "unexpected bridge success message: {}",
        response.message
    );
}

#[test]
fn native_host_bridge_dispatches_when_browser_passes_extension_origin_arg() {
    let temp = tempdir().expect("tempdir");
    let stub_dir = temp.path().join("bin");
    fs::create_dir_all(&stub_dir).expect("create stub dir");
    write_gh_status_stub(&stub_dir);
    let opencode_bin = write_opencode_stub(&stub_dir);

    let runtime = CliRuntime {
        cwd: temp.path().to_path_buf(),
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };
    fs::create_dir_all(&runtime.store_root).expect("create store root");

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
    let output = run_rr_process_with_stdin(
        &runtime,
        &encode_native_intent(&intent),
        &stub_dir,
        &[
            "chrome-extension://djbjigobohmlljboggckmhhnoeldinlp/",
            "--parent-window=0",
        ],
    );

    assert!(
        output.status.success(),
        "native host dispatch with extension-origin argv failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response = decode_native_response(&output.stdout);
    assert!(response.ok, "bridge response was not ok: {:?}", response);
    assert_eq!(response.action, "start_review");
    assert_eq!(
        response
            .status
            .as_ref()
            .map(|status| status.attention_state.as_str()),
        Some("review_launched")
    );
    assert!(
        response
            .message
            .contains("rr review completed for owner/repo#42"),
        "unexpected bridge success message: {}",
        response.message
    );
}
