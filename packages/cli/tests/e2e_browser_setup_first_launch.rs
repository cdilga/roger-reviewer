#![cfg(unix)]

use roger_bridge::{
    BridgeFailureKind, BridgeLaunchIntent, BridgeLaunchPath, BridgePreflight, handle_bridge_intent,
    required_launch_artifacts,
};
use roger_cli::{CliRuntime, run};
use roger_validation::{discover_suite_metadata, failure_artifact_paths};
use serde_json::{Value, json};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tempfile::{TempDir, tempdir};

const PACKAGED_MANIFEST_KEY_EXTENSION_ID: &str = "djbjigobohmlljboggckmhhnoeldinlp";

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("packages parent")
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn parse_robot(stdout: &str) -> Value {
    serde_json::from_str(stdout).expect("robot payload")
}

fn run_rr(args: &[&str], runtime: &CliRuntime) -> roger_cli::CliRunResult {
    let argv = args
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    run(&argv, runtime)
}

fn write_stub_rr_binary(session_id: &str) -> (TempDir, PathBuf) {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("rr-stub");
    let review_payload = serde_json::to_string_pretty(&json!({
        "schema_id": "rr.robot.review.v1",
        "command": "rr review",
        "robot_format": "json",
        "outcome": "complete",
        "generated_at": "2026-04-17T00:00:00Z",
        "exit_code": 0,
        "warnings": [],
        "repair_actions": [],
        "data": {
            "session_id": session_id,
            "launch_attempt_id": "attempt-e2e-05",
        },
    }))
    .expect("serialize review payload");
    let status_payload = serde_json::to_string_pretty(&json!({
        "schema_id": "rr.robot.status.v1",
        "command": "rr status",
        "robot_format": "json",
        "outcome": "complete",
        "generated_at": "2026-04-17T00:00:01Z",
        "exit_code": 0,
        "warnings": [],
        "repair_actions": [],
        "data": {
            "session": { "id": session_id },
            "attention": { "state": "review_launched" },
        },
    }))
    .expect("serialize status payload");
    let script = format!(
        r#"#!/bin/sh
case "$1" in
  review)
    cat <<'EOF'
{review_payload}
EOF
    exit 0
    ;;
  status)
    cat <<'EOF'
{status_payload}
EOF
    exit 0
    ;;
  *)
    echo "unexpected args: $@" >&2
    exit 64
    ;;
esac
"#
    );
    fs::write(&path, script).expect("write rr stub");
    let mut perms = fs::metadata(&path).expect("metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).expect("chmod rr stub");
    (dir, path)
}

#[test]
fn e2e_browser_setup_first_launch_runs_deterministic_extension_loaded_path() {
    let temp = tempdir().expect("tempdir");
    let runtime = CliRuntime {
        cwd: workspace_root(),
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };
    let install_root = temp.path().join("install-root");
    let extension_id = PACKAGED_MANIFEST_KEY_EXTENSION_ID;

    let setup = run_rr(
        &[
            "extension",
            "setup",
            "--browser",
            "chrome",
            "--install-root",
            &install_root.to_string_lossy(),
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(setup.exit_code, 0, "{}", setup.stderr);
    let setup_payload = parse_robot(&setup.stdout);
    assert_eq!(setup_payload["outcome"], "complete");
    assert!(
        setup_payload["repair_actions"]
            .as_array()
            .expect("repair actions")
            .iter()
            .any(|action| {
                action
                    .as_str()
                    .unwrap_or_default()
                    .contains("chrome://extensions")
            }),
        "setup should still surface the manual browser load step"
    );
    assert_eq!(setup_payload["data"]["subcommand"], "setup");
    assert_eq!(setup_payload["data"]["extension_id"], extension_id);
    assert_eq!(
        setup_payload["data"]["extension_id_source"],
        "packaged_manifest_key"
    );
    assert_eq!(setup_payload["data"]["doctor"]["subcommand"], "doctor");
    let native_manifest_path = setup_payload["data"]["native_manifest_path"]
        .as_str()
        .expect("native manifest path from setup");
    assert!(
        PathBuf::from(native_manifest_path).exists(),
        "native manifest should exist after setup"
    );

    let doctor = run_rr(
        &[
            "extension",
            "doctor",
            "--browser",
            "chrome",
            "--install-root",
            &install_root.to_string_lossy(),
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(doctor.exit_code, 0, "{}", doctor.stderr);
    let doctor_payload = parse_robot(&doctor.stdout);
    assert_eq!(doctor_payload["outcome"], "complete");
    assert_eq!(doctor_payload["data"]["subcommand"], "doctor");
    assert!(
        doctor_payload["data"]["checks"]
            .as_array()
            .expect("doctor checks")
            .iter()
            .all(|check| check["ok"].as_bool().unwrap_or(false))
    );

    let session_id = "session-e2e-browser-05";
    let (_stub_dir, rr_stub) = write_stub_rr_binary(session_id);
    let response = handle_bridge_intent(
        &BridgeLaunchIntent {
            action: "start_review".to_owned(),
            owner: "owner".to_owned(),
            repo: "repo".to_owned(),
            pr_number: 42,
            head_ref: None,
            instance: None,
            extension_id: Some(extension_id.to_owned()),
            browser: Some("chrome".to_owned()),
        },
        &BridgePreflight {
            roger_binary_found: true,
            roger_data_dir_exists: true,
            gh_available: true,
        },
        &rr_stub,
    );

    assert!(response.ok, "bridge response failed: {:?}", response);
    assert_eq!(response.failure_kind, None);
    assert_eq!(response.launch_outcome, None);
    assert_eq!(response.session_id.as_deref(), Some(session_id));
    let status = response.status.expect("bridge status snapshot");
    assert_eq!(status.session_id, session_id);
    assert_eq!(status.attention_state, "review_launched");
    assert!(
        response
            .message
            .contains("rr review completed for owner/repo#42"),
        "unexpected bridge success message: {}",
        response.message
    );

    let required = required_launch_artifacts(BridgeLaunchPath::NativeMessaging);
    assert_eq!(
        required,
        [
            "native_request_envelope.json",
            "native_response_envelope.json",
            "bridge_launch_transcript.json",
        ]
    );

    let metadata_dir = workspace_root().join("tests/suites");
    let suites = discover_suite_metadata(&metadata_dir).expect("discover suite metadata");
    let suite = suites
        .iter()
        .find(|item| item.id == "e2e_browser_setup_first_launch")
        .expect("E2E-05 suite metadata");
    assert_eq!(suite.budget_id.as_deref(), Some("E2E-05"));
    assert_eq!(suite.support_tier, "deterministic_chromium_harness");
    assert_eq!(
        suite.fixture_families,
        vec![
            "fixture_bridge_launch_only_no_status",
            "fixture_bridge_transcripts"
        ]
    );

    let failing_ids = vec!["e2e_browser_setup_first_launch".to_owned()];
    let failure_paths = failure_artifact_paths(
        &metadata_dir,
        temp.path().join("test-artifacts"),
        &failing_ids,
    )
    .expect("failure artifact paths");
    assert_eq!(failure_paths.len(), 1);
    assert!(
        failure_paths[0]
            .to_string_lossy()
            .contains("failures/e2e_browser_setup_first_launch/sample_failure"),
        "failure artifact namespace should preserve deterministic e2e suite identity"
    );
}

#[test]
fn e2e_browser_setup_first_launch_fails_closed_when_bridge_preflight_is_missing() {
    let response = handle_bridge_intent(
        &BridgeLaunchIntent {
            action: "start_review".to_owned(),
            owner: "owner".to_owned(),
            repo: "repo".to_owned(),
            pr_number: 42,
            head_ref: None,
            instance: None,
            extension_id: None,
            browser: Some("chrome".to_owned()),
        },
        &BridgePreflight {
            roger_binary_found: false,
            roger_data_dir_exists: true,
            gh_available: true,
        },
        Path::new("/missing/rr"),
    );

    assert!(!response.ok);
    assert_eq!(
        response.failure_kind,
        Some(BridgeFailureKind::PreflightFailed)
    );
    assert!(
        response
            .guidance
            .as_deref()
            .unwrap_or_default()
            .contains("Roger binary not found"),
        "bridge preflight failure should surface fail-closed install guidance"
    );
}
