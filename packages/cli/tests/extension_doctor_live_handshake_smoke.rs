#![cfg(unix)]

//! `rr extension doctor --live` real native-host handshake.
//!
//! Installs a host manifest + launcher into a temp install-root that points at
//! the built rr binary, then drives `rr extension doctor --live` end to end:
//!   - the OK case spawns the real launcher, writes a length-prefixed
//!     StatusProbe, and asserts a well-formed reply with `live_handshake: ok`;
//!   - the failure case swaps the launcher for an unresponsive one (still
//!     present, so the static checks stay green) and asserts a typed live
//!     failure (`malformed_reply`).
//!
//! A truly *removed* launcher is already caught earlier by the static
//! `native_host_binary_present` check, so it never reaches the live section;
//! the unresponsive-launcher case is what exercises the live typed-failure path.

use roger_bridge::NativeHostManifest;
use serde_json::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Mutex, MutexGuard, OnceLock};
use tempfile::tempdir;

/// `rr extension setup` packs the unpacked extension from a workspace-shared dev
/// pack directory, so concurrent setups in this binary race on that tree. Hold
/// this guard across setup to keep the pack step serial.
fn pack_guard() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

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

struct DoctorLiveFixture {
    _temp: tempfile::TempDir,
    rr: PathBuf,
    store_root: PathBuf,
    profile_root: PathBuf,
    install_root: PathBuf,
    launcher_path: PathBuf,
}

fn install_native_host() -> DoctorLiveFixture {
    let temp = tempdir().expect("tempdir");
    let store_root = temp.path().join("roger-store");
    let install_root = temp.path().join("install-root");
    let profile_root = temp.path().join("profile-root");
    fs::create_dir_all(&profile_root).expect("create profile root");

    let rr = rr_binary();
    assert!(rr.exists(), "expected rr binary at {}", rr.display());

    let guard = pack_guard();
    let setup = Command::new(&rr)
        .args(["extension", "setup", "--browser", "edge"])
        .arg("--install-root")
        .arg(&install_root)
        .arg("--robot")
        .env("RR_STORE_ROOT", &store_root)
        .env("RR_EXTENSION_PROFILE_ROOT", &profile_root)
        .current_dir(workspace_root())
        .output()
        .expect("run rr extension setup");
    assert!(
        setup.status.success(),
        "setup failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&setup.stdout),
        String::from_utf8_lossy(&setup.stderr)
    );
    let setup_payload = parse_robot(&setup.stdout);
    assert_eq!(setup_payload["outcome"], "complete");
    drop(guard);

    let manifest_path = PathBuf::from(
        setup_payload["data"]["native_manifest_path"]
            .as_str()
            .expect("native_manifest_path"),
    );
    let manifest: NativeHostManifest = serde_json::from_str(
        &fs::read_to_string(&manifest_path).expect("read native host manifest"),
    )
    .expect("parse native host manifest");
    let launcher_path = PathBuf::from(&manifest.path);
    assert!(launcher_path.exists(), "launcher missing after setup");

    DoctorLiveFixture {
        _temp: temp,
        rr,
        store_root,
        profile_root,
        install_root,
        launcher_path,
    }
}

fn run_doctor_live(fixture: &DoctorLiveFixture) -> std::process::Output {
    Command::new(&fixture.rr)
        .args(["extension", "doctor", "--browser", "edge"])
        .arg("--install-root")
        .arg(&fixture.install_root)
        .arg("--live")
        .arg("--robot")
        .env("RR_STORE_ROOT", &fixture.store_root)
        .env("RR_EXTENSION_PROFILE_ROOT", &fixture.profile_root)
        .current_dir(workspace_root())
        .output()
        .expect("run rr extension doctor --live")
}

#[test]
fn extension_doctor_live_reports_ok_with_real_native_host_reply() {
    let fixture = install_native_host();
    let doctor = run_doctor_live(&fixture);
    let payload = parse_robot(&doctor.stdout);
    assert_eq!(
        payload["outcome"],
        "complete",
        "doctor --live not complete: {}",
        String::from_utf8_lossy(&doctor.stdout)
    );

    let live = &payload["data"]["live"];
    assert_eq!(
        live["live_handshake"], "ok",
        "live handshake not ok: {live}"
    );
    // The reply is the real bounded StatusProbe answer from persisted state.
    assert!(
        live["reply"]["session_count"].is_number(),
        "expected a well-formed native-host reply: {live}"
    );
    // Preflight is reported informationally; the rr binary and store both exist.
    assert_eq!(live["preflight"]["roger_binary_found"], true);
    assert_eq!(live["preflight"]["roger_data_dir_exists"], true);
}

#[test]
fn extension_doctor_without_live_omits_live_section() {
    let fixture = install_native_host();
    let doctor = Command::new(&fixture.rr)
        .args(["extension", "doctor", "--browser", "edge"])
        .arg("--install-root")
        .arg(&fixture.install_root)
        .arg("--robot")
        .env("RR_STORE_ROOT", &fixture.store_root)
        .env("RR_EXTENSION_PROFILE_ROOT", &fixture.profile_root)
        .current_dir(workspace_root())
        .output()
        .expect("run rr extension doctor");
    let payload = parse_robot(&doctor.stdout);
    assert_eq!(payload["outcome"], "complete");
    assert!(
        payload["data"].get("live").is_none(),
        "live section must be absent without --live: {}",
        String::from_utf8_lossy(&doctor.stdout)
    );
}

#[test]
fn extension_doctor_live_reports_typed_failure_for_unresponsive_launcher() {
    let fixture = install_native_host();

    // Swap the launcher for one that is present (static checks stay green) but
    // never writes a native-messaging reply, so the live handshake must fail
    // closed with a typed reason instead of hanging.
    fs::write(
        &fixture.launcher_path,
        "#!/bin/sh\ncat >/dev/null 2>&1 || true\nexit 0\n",
    )
    .expect("overwrite launcher");
    let mut perms = fs::metadata(&fixture.launcher_path)
        .expect("stat launcher")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&fixture.launcher_path, perms).expect("chmod launcher");

    let doctor = run_doctor_live(&fixture);
    let payload = parse_robot(&doctor.stdout);
    assert_eq!(
        payload["outcome"],
        "blocked",
        "unresponsive launcher should fail closed: {}",
        String::from_utf8_lossy(&doctor.stdout)
    );

    let live = &payload["data"]["live"];
    assert_eq!(live["live_handshake"], "failed", "live section: {live}");
    assert_eq!(
        live["reason"], "malformed_reply",
        "expected typed malformed_reply failure: {live}"
    );
    assert!(
        payload["repair_actions"]
            .as_array()
            .is_some_and(|actions| !actions.is_empty()),
        "typed live failure must carry repair actions: {}",
        String::from_utf8_lossy(&doctor.stdout)
    );
}
