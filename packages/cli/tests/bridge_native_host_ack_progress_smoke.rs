#![cfg(unix)]

//! Drives the built `rr` binary in native-host mode over stdin and asserts the
//! ack/progress framing contract (bead rr-bridge-ack-progress-protocol-hsgu):
//!
//! - a Launch intent streams `host_started` (immediate ack) then, when preflight
//!   passes, `preflight_ok`, then the final `BridgeResponse` frame — in that
//!   order, each length-prefixed;
//! - a StatusProbe stays a single frame (it runs under a tight extension-side
//!   budget and must not spend it on ack/progress);
//! - when preflight fails, the ack still arrives before the failure response and
//!   no `preflight_ok` frame is emitted.

use roger_bridge::BridgeLaunchIntent;
use serde_json::{Value, json};
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

fn rr_binary() -> PathBuf {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_rr") {
        return PathBuf::from(path);
    }
    workspace_root().join("target/debug/rr")
}

fn frame_bytes(value: &Value) -> Vec<u8> {
    let json = serde_json::to_vec(value).expect("serialize native frame");
    let len = json.len() as u32;
    let mut wire = Vec::with_capacity(4 + json.len());
    wire.extend_from_slice(&len.to_le_bytes());
    wire.extend_from_slice(&json);
    wire
}

fn split_frames(stdout: &[u8]) -> Vec<Value> {
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
            "frame length prefix overruns buffer"
        );
        frames.push(
            serde_json::from_slice::<Value>(&stdout[offset..offset + len])
                .expect("decode native frame payload"),
        );
        offset += len;
    }
    assert_eq!(offset, stdout.len(), "trailing bytes after final frame");
    frames
}

fn stage_of(frame: &Value) -> Option<&str> {
    frame.get("stage").and_then(|s| s.as_str())
}

fn is_progress(frame: &Value) -> bool {
    frame.get("schema").and_then(|s| s.as_str()) == Some("roger.bridge.launch-progress.v1")
}

fn write_gh_status_stub(dir: &Path) {
    let path = dir.join("gh");
    fs::write(
        &path,
        "#!/bin/sh\nif [ \"$1\" = \"auth\" ] && [ \"$2\" = \"status\" ]; then exit 0; fi\nexit 1\n",
    )
    .expect("write gh stub");
    let mut perms = fs::metadata(&path).expect("gh metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).expect("chmod gh stub");
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

fn path_with(dir: &Path) -> std::ffi::OsString {
    let mut value = std::ffi::OsString::from(dir.as_os_str());
    if let Some(existing) = std::env::var_os("PATH") {
        value.push(":");
        value.push(existing);
    }
    value
}

/// Spawn `rr` in native-host mode (no args, stdin not a terminal) and feed it a
/// single length-prefixed request frame.
fn run_native_host(
    stdin_bytes: &[u8],
    store_root: &Path,
    path_dir: &Path,
    extra_env: &[(&str, &str)],
) -> Vec<u8> {
    let rr = rr_binary();
    assert!(rr.exists(), "expected rr binary at {}", rr.display());
    let mut cmd = Command::new(&rr);
    cmd.env("RR_STORE_ROOT", store_root)
        .env("PATH", path_with(path_dir))
        .current_dir(path_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in extra_env {
        cmd.env(key, value);
    }
    let mut child = cmd.spawn().expect("spawn rr native host");
    child
        .stdin
        .as_mut()
        .expect("native host stdin")
        .write_all(stdin_bytes)
        .expect("write native host request");
    child
        .wait_with_output()
        .expect("wait for native host output")
        .stdout
}

#[test]
fn status_probe_stays_a_single_frame() {
    let temp = tempdir().expect("tempdir");
    let store_root = temp.path().join("store");
    fs::create_dir_all(&store_root).expect("create store root");
    let bin_dir = temp.path().join("bin");
    fs::create_dir_all(&bin_dir).expect("create bin dir");
    write_gh_status_stub(&bin_dir);

    let probe = json!({
        "type": "roger_bridge_status",
        "owner": "acme",
        "repo": "widgets",
        "pr_number": 42,
    });
    let stdout = run_native_host(&frame_bytes(&probe), &store_root, &bin_dir, &[]);
    let frames = split_frames(&stdout);
    assert_eq!(
        frames.len(),
        1,
        "status probe must reply with exactly one frame, got {frames:?}"
    );
    assert!(
        !is_progress(&frames[0]),
        "status probe reply must not be a launch-progress frame"
    );
}

#[test]
fn launch_streams_ack_then_preflight_ok_then_final_when_ready() {
    let temp = tempdir().expect("tempdir");
    let store_root = temp.path().join("store");
    fs::create_dir_all(&store_root).expect("create store root");
    let bin_dir = temp.path().join("bin");
    fs::create_dir_all(&bin_dir).expect("create bin dir");
    write_gh_status_stub(&bin_dir);
    let opencode = write_opencode_stub(&bin_dir);

    let intent = BridgeLaunchIntent {
        action: "start_review".to_owned(),
        owner: "owner".to_owned(),
        repo: "repo".to_owned(),
        pr_number: 7,
        head_ref: None,
        instance: None,
        session_id: None,
        extension_id: None,
        browser: None,
    };
    let stdin = frame_bytes(&serde_json::to_value(&intent).expect("intent value"));
    let stdout = run_native_host(
        &stdin,
        &store_root,
        &bin_dir,
        &[("RR_OPENCODE_BIN", &opencode.to_string_lossy())],
    );
    let frames = split_frames(&stdout);

    assert!(
        frames.len() >= 3,
        "ready launch must stream ack + preflight_ok + final, got {frames:?}"
    );
    assert_eq!(stage_of(&frames[0]), Some("host_started"), "frame 0 = ack");
    assert!(
        is_progress(&frames[0]),
        "ack frame carries the progress schema"
    );
    assert_eq!(
        stage_of(&frames[1]),
        Some("preflight_ok"),
        "frame 1 = preflight_ok"
    );
    assert!(
        is_progress(&frames[1]),
        "preflight_ok carries the progress schema"
    );

    let final_frame = frames.last().expect("final frame");
    assert!(
        !is_progress(final_frame),
        "final frame must not be a progress frame"
    );
    assert!(
        final_frame.get("ok").is_some(),
        "final frame must be a BridgeResponse: {final_frame:?}"
    );
}

#[test]
fn launch_still_acks_before_failure_when_preflight_not_ready() {
    let temp = tempdir().expect("tempdir");
    // Deliberately missing store root -> preflight fails closed regardless of
    // whether gh happens to be authed on the host.
    let store_root = temp.path().join("missing-store");
    let bin_dir = temp.path().join("bin");
    fs::create_dir_all(&bin_dir).expect("create bin dir");

    let intent = BridgeLaunchIntent {
        action: "start_review".to_owned(),
        owner: "owner".to_owned(),
        repo: "repo".to_owned(),
        pr_number: 7,
        head_ref: None,
        instance: None,
        session_id: None,
        extension_id: None,
        browser: None,
    };
    let stdin = frame_bytes(&serde_json::to_value(&intent).expect("intent value"));
    let stdout = run_native_host(&stdin, &store_root, &bin_dir, &[]);
    let frames = split_frames(&stdout);

    assert_eq!(
        stage_of(&frames[0]),
        Some("host_started"),
        "the ack must still lead even when preflight will fail"
    );
    assert!(
        frames.iter().all(|f| stage_of(f) != Some("preflight_ok")),
        "no preflight_ok frame when preflight is not ready: {frames:?}"
    );
    let final_frame = frames.last().expect("final frame");
    assert_eq!(
        final_frame.get("ok").and_then(|v| v.as_bool()),
        Some(false),
        "final frame is the fail-closed response: {final_frame:?}"
    );
}
