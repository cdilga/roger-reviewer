#![cfg(unix)]

use roger_test_harness::BrowserHarnessOutcome;
use serde_json::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use tempfile::tempdir;

fn parse_report(stdout: &[u8]) -> Value {
    serde_json::from_slice(stdout).expect("browser harness report json")
}

fn write_stub_browser(script_body: &str) -> PathBuf {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("chromium-stub");
    fs::write(&path, script_body).expect("write stub browser");
    let mut perms = fs::metadata(&path).expect("metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).expect("chmod stub browser");
    // Keep the tempdir alive by leaking it for the duration of the test process.
    std::mem::forget(temp);
    path
}

fn write_unpacked_extension(root: &PathBuf) {
    fs::create_dir_all(root).expect("create extension dir");
    fs::write(
        root.join("manifest.json"),
        r#"{"manifest_version": 3, "name": "Roger", "version": "0.0.0"}"#,
    )
    .expect("write manifest");
}

#[test]
fn deterministic_browser_harness_bin_launches_stubbed_runtime_and_records_artifacts() {
    let temp = tempdir().expect("tempdir");
    let extension_dir = temp.path().join("unpacked-extension");
    write_unpacked_extension(&extension_dir);
    let browser_binary = write_stub_browser(
        r#"#!/bin/sh
printf '%s\n' "$@"
exit 0
"#,
    );
    let artifact_root = temp.path().join("artifacts");

    let output = Command::new(env!("CARGO_BIN_EXE_deterministic_browser_harness"))
        .args([
            "--browser-binary",
            browser_binary.to_str().expect("browser binary path"),
            "--extension-dir",
            extension_dir.to_str().expect("extension dir"),
            "--artifact-root",
            artifact_root.to_str().expect("artifact root"),
            "--start-url",
            "https://github.com/owner/repo/pull/42",
            "--startup-probe-ms",
            "1",
        ])
        .output()
        .expect("run browser harness");
    assert_eq!(output.status.code(), Some(0), "{:?}", output);

    let report = parse_report(&output.stdout);
    assert_eq!(
        report["outcome"],
        serde_json::json!(BrowserHarnessOutcome::Launched)
    );
    assert_eq!(report["runtime"], "deterministic_chromium");
    assert_eq!(report["evidence_class"], "canonical_automation");
    let startup_state = report["startup_state"]
        .as_str()
        .expect("startup state string");
    assert!(
        startup_state == "exited_cleanly" || startup_state == "running_after_probe",
        "unexpected startup_state: {startup_state}"
    );
    assert_eq!(
        report["start_url"],
        serde_json::json!("https://github.com/owner/repo/pull/42")
    );

    let launch_args = report["launch_args"].as_array().expect("launch args");
    assert!(launch_args.iter().any(|arg| {
        arg.as_str()
            .expect("launch arg")
            .starts_with("--disable-extensions-except=")
    }));
    assert!(launch_args.iter().any(|arg| {
        arg.as_str()
            .expect("launch arg")
            .starts_with("--load-extension=")
    }));
    assert!(launch_args.iter().any(|arg| {
        arg.as_str()
            .expect("launch arg")
            .starts_with("--user-data-dir=")
    }));

    let report_path = artifact_root.join("browser_harness_report.json");
    let launch_command_path = artifact_root.join("browser_launch_command.json");
    let launch_transcript_path = artifact_root.join("browser_launch_transcript.log");
    let native_request_path = artifact_root.join("native_request_envelope.json");
    let native_response_path = artifact_root.join("native_response_envelope.json");
    let bridge_transcript_path = artifact_root.join("bridge_launch_transcript.json");

    assert!(report_path.is_file(), "missing report path");
    assert!(launch_command_path.is_file(), "missing launch command path");
    assert!(
        launch_transcript_path.is_file(),
        "missing launch transcript path"
    );
    assert_eq!(
        report["artifacts"]["native_request_envelope_path"],
        serde_json::json!(native_request_path)
    );
    assert_eq!(
        report["artifacts"]["native_response_envelope_path"],
        serde_json::json!(native_response_path)
    );
    assert_eq!(
        report["artifacts"]["bridge_launch_transcript_path"],
        serde_json::json!(bridge_transcript_path)
    );

    let transcript = fs::read_to_string(&launch_transcript_path).expect("read launch transcript");
    assert!(
        transcript.contains("--load-extension="),
        "launch transcript should capture extension arg"
    );
    assert!(
        transcript.contains("https://github.com/owner/repo/pull/42"),
        "launch transcript should capture start url"
    );
}

#[test]
fn deterministic_browser_harness_bin_fails_closed_when_manifest_is_missing() {
    let temp = tempdir().expect("tempdir");
    let browser_binary = write_stub_browser(
        r#"#!/bin/sh
printf 'unexpected launch\n' >&2
exit 0
"#,
    );
    let artifact_root = temp.path().join("artifacts");
    let extension_dir = temp.path().join("missing-manifest-extension");
    fs::create_dir_all(&extension_dir).expect("create extension dir");

    let output = Command::new(env!("CARGO_BIN_EXE_deterministic_browser_harness"))
        .args([
            "--browser-binary",
            browser_binary.to_str().expect("browser binary path"),
            "--extension-dir",
            extension_dir.to_str().expect("extension dir"),
            "--artifact-root",
            artifact_root.to_str().expect("artifact root"),
            "--startup-probe-ms",
            "1",
        ])
        .output()
        .expect("run browser harness");
    assert_eq!(output.status.code(), Some(3), "{:?}", output);

    let report = parse_report(&output.stdout);
    assert_eq!(
        report["outcome"],
        serde_json::json!(BrowserHarnessOutcome::Blocked)
    );
    assert_eq!(
        report["reason_code"],
        serde_json::json!("extension_manifest_missing")
    );
    assert_eq!(
        report["startup_state"],
        serde_json::json!("preflight_failed")
    );
    assert!(
        report["repair_guidance"]
            .as_str()
            .expect("repair guidance")
            .contains("manifest.json"),
        "repair guidance should explain missing unpacked manifest"
    );

    let repair_transcript_path = artifact_root.join("browser_repair_transcript.json");
    assert!(
        repair_transcript_path.is_file(),
        "missing repair transcript path"
    );
    let repair_payload: Value =
        serde_json::from_slice(&fs::read(&repair_transcript_path).expect("read repair transcript"))
            .expect("parse repair transcript");
    assert_eq!(
        repair_payload["reason_code"],
        serde_json::json!("extension_manifest_missing")
    );
}
