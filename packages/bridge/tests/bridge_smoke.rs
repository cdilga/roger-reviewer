//! Integration smoke tests for the browser launch bridge.

use std::io::Cursor;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use roger_bridge::{
    BridgeFailureKind, BridgeLaunchIntent, BridgeLaunchPath, BridgePreflight, BridgeResponse,
    NativeHostManifest, SupportedBrowser, choose_launch_path, handle_bridge_intent,
    read_native_message, required_launch_artifacts, write_native_message,
};
#[cfg(unix)]
use tempfile::tempdir;

#[test]
fn native_messaging_end_to_end() {
    let intent = BridgeLaunchIntent {
        action: "resume_review".to_owned(),
        owner: "acme".to_owned(),
        repo: "widgets".to_owned(),
        pr_number: 99,
        head_ref: None,
        instance: Some("my-inst".to_owned()),
        extension_id: None,
        browser: None,
    };

    // Encode as Native Messaging.
    let json = serde_json::to_vec(&intent).unwrap();
    let len = json.len() as u32;
    let mut wire = Vec::new();
    wire.extend_from_slice(&len.to_le_bytes());
    wire.extend_from_slice(&json);

    // Decode.
    let mut reader = Cursor::new(wire);
    let parsed = read_native_message(&mut reader).unwrap();
    assert_eq!(parsed.action, "resume_review");
    assert_eq!(parsed.pr_number, 99);
    assert_eq!(parsed.instance, Some("my-inst".to_owned()));
}

#[test]
fn launch_path_uses_native_messaging_when_registered() {
    let launch_path = choose_launch_path(true, true).expect("native path should be selected");
    assert_eq!(launch_path, BridgeLaunchPath::NativeMessaging);
}

#[test]
fn launch_path_fails_closed_when_native_messaging_is_unavailable_even_with_legacy_fallback() {
    let err = choose_launch_path(false, true).expect_err("native messaging is required");
    assert!(
        err.to_string()
            .contains("Native Messaging host registration is missing"),
        "unexpected error: {err}"
    );
}

#[test]
fn launch_path_fails_closed_when_no_bridge_registration_is_available() {
    let err = choose_launch_path(false, false).expect_err("missing bridge registration must fail");
    assert!(
        err.to_string()
            .contains("Native Messaging host registration is missing"),
        "unexpected error: {err}"
    );
}

#[test]
fn native_messaging_response_roundtrip() {
    let resp = BridgeResponse::success("start_review", "launched", Some("sess-42".to_owned()));

    let mut buf = Vec::new();
    write_native_message(&mut buf, &resp).unwrap();

    // Verify wire format: 4-byte LE length + JSON.
    let len = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
    assert_eq!(buf.len(), 4 + len);

    let decoded: BridgeResponse = serde_json::from_slice(&buf[4..]).unwrap();
    assert!(decoded.ok);
    assert_eq!(decoded.session_id, Some("sess-42".to_owned()));
}

#[test]
fn native_path_artifacts_include_envelopes_and_transcript() {
    let artifacts = required_launch_artifacts(BridgeLaunchPath::NativeMessaging);
    assert_eq!(
        artifacts,
        [
            "native_request_envelope.json",
            "native_response_envelope.json",
            "bridge_launch_transcript.json",
        ]
    );
}

#[test]
fn fail_closed_when_roger_not_installed() {
    let intent = BridgeLaunchIntent {
        action: "start_review".to_owned(),
        owner: "acme".to_owned(),
        repo: "widgets".to_owned(),
        pr_number: 1,
        head_ref: None,
        instance: None,
        extension_id: None,
        browser: None,
    };
    let preflight = BridgePreflight {
        roger_binary_found: false,
        roger_data_dir_exists: false,
        gh_available: false,
    };
    let resp = handle_bridge_intent(&intent, &preflight, Path::new("/missing/rr"));
    assert!(!resp.ok);
    assert_eq!(resp.failure_kind, Some(BridgeFailureKind::PreflightFailed));
    let guidance = resp.guidance.unwrap();
    assert!(guidance.contains("Roger binary not found"));
    assert!(guidance.contains("data directory"));
    assert!(guidance.contains("gh auth login"));
}

#[test]
fn manifest_covers_all_supported_browsers() {
    for browser in [
        SupportedBrowser::Chrome,
        SupportedBrowser::Edge,
        SupportedBrowser::Brave,
    ] {
        let path = NativeHostManifest::install_path(&browser);
        assert!(
            path.to_string_lossy()
                .contains("com.roger_reviewer.bridge.json"),
            "missing manifest filename for {browser:?}"
        );
    }

    let manifest =
        NativeHostManifest::for_roger(Path::new("/usr/local/bin/rr"), "test-extension-id");
    assert_eq!(manifest.host_type, "stdio");
    assert!(manifest.allowed_origins[0].contains("test-extension-id"));
}

#[cfg(unix)]
fn write_stub_roger_binary(
    review_payload: &str,
    review_exit: i32,
    status_payload: &str,
    status_exit: i32,
) -> tempfile::TempDir {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("rr-stub");
    let script = format!(
        r#"#!/bin/sh
case "$1" in
  review|resume|findings)
    cat <<'EOF'
{review_payload}
EOF
    exit {review_exit}
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
    std::fs::write(&path, script).expect("write rr stub");
    let mut perms = std::fs::metadata(&path).expect("metadata").permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&path, perms).expect("chmod rr stub");
    dir
}

#[test]
fn unknown_action_rejected() {
    let intent = BridgeLaunchIntent {
        action: "deploy_to_prod".to_owned(),
        owner: "acme".to_owned(),
        repo: "widgets".to_owned(),
        pr_number: 1,
        head_ref: None,
        instance: None,
        extension_id: None,
        browser: None,
    };
    let preflight = BridgePreflight {
        roger_binary_found: true,
        roger_data_dir_exists: true,
        gh_available: true,
    };
    let resp = handle_bridge_intent(&intent, &preflight, Path::new("/usr/local/bin/rr"));
    assert!(!resp.ok);
    assert!(resp.guidance.unwrap().contains("Supported actions"));
}

#[test]
fn refresh_review_action_is_rejected() {
    let intent = BridgeLaunchIntent {
        action: "refresh_review".to_owned(),
        owner: "acme".to_owned(),
        repo: "widgets".to_owned(),
        pr_number: 7,
        head_ref: None,
        instance: None,
        extension_id: None,
        browser: None,
    };
    let preflight = BridgePreflight {
        roger_binary_found: true,
        roger_data_dir_exists: true,
        gh_available: true,
    };
    let resp = handle_bridge_intent(&intent, &preflight, Path::new("/usr/local/bin/rr"));
    assert!(!resp.ok);
    assert_eq!(resp.action, "refresh_review");
    assert!(resp.guidance.unwrap().contains("Supported actions"));
}

#[cfg(unix)]
#[test]
fn bridge_launch_response_returns_verified_session_and_status_snapshot() {
    let intent = BridgeLaunchIntent {
        action: "start_review".to_owned(),
        owner: "acme".to_owned(),
        repo: "widgets".to_owned(),
        pr_number: 12,
        head_ref: None,
        instance: None,
        extension_id: None,
        browser: None,
    };
    let preflight = BridgePreflight {
        roger_binary_found: true,
        roger_data_dir_exists: true,
        gh_available: true,
    };
    let review_payload = serde_json::to_string_pretty(&serde_json::json!({
        "schema_id": "rr.robot.review.v1",
        "command": "rr review",
        "robot_format": "json",
        "outcome": "complete",
        "generated_at": "2026-04-15T00:00:00Z",
        "exit_code": 0,
        "warnings": [],
        "repair_actions": [],
        "data": {
            "session_id": "session-bridge-42",
            "launch_attempt_id": "attempt-bridge-42",
        }
    }))
    .expect("serialize review payload");
    let status_payload = serde_json::to_string_pretty(&serde_json::json!({
        "schema_id": "rr.robot.status.v1",
        "command": "rr status",
        "robot_format": "json",
        "outcome": "complete",
        "generated_at": "2026-04-15T00:00:01Z",
        "exit_code": 0,
        "warnings": [],
        "repair_actions": [],
        "data": {
            "session": {"id": "session-bridge-42"},
            "attention": {"state": "review_launched"}
        }
    }))
    .expect("serialize status payload");
    let stub_dir = write_stub_roger_binary(&review_payload, 0, &status_payload, 0);
    let stub_rr = stub_dir.path().join("rr-stub");

    let resp = handle_bridge_intent(&intent, &preflight, &stub_rr);
    assert!(resp.ok);
    assert_eq!(resp.session_id.as_deref(), Some("session-bridge-42"));
    assert_eq!(resp.attention_state.as_deref(), Some("review_launched"));
    assert_eq!(resp.guidance, None);
    assert_eq!(
        resp.status.as_ref().map(|status| status.schema_id.as_str()),
        Some("rr.robot.status.v1")
    );
    assert_eq!(resp.launch_outcome, None);

    let message = resp.message.to_ascii_lowercase();
    assert!(message.contains("completed"));
    assert!(!message.contains("approval"));
    assert!(!message.contains("ready to post"));
}

#[cfg(unix)]
#[test]
fn bridge_launch_success_carries_repair_guidance_for_refresh_recommended_status() {
    let intent = BridgeLaunchIntent {
        action: "resume_review".to_owned(),
        owner: "acme".to_owned(),
        repo: "widgets".to_owned(),
        pr_number: 12,
        head_ref: None,
        instance: None,
        extension_id: None,
        browser: None,
    };
    let preflight = BridgePreflight {
        roger_binary_found: true,
        roger_data_dir_exists: true,
        gh_available: true,
    };
    let review_payload = serde_json::to_string_pretty(&serde_json::json!({
        "schema_id": "rr.robot.resume.v1",
        "command": "rr resume",
        "robot_format": "json",
        "outcome": "complete",
        "generated_at": "2026-04-15T00:00:00Z",
        "exit_code": 0,
        "warnings": [],
        "repair_actions": [],
        "data": {
            "session_id": "session-bridge-99",
            "launch_attempt_id": "attempt-bridge-99",
        }
    }))
    .expect("serialize resume payload");
    let status_payload = serde_json::to_string_pretty(&serde_json::json!({
        "schema_id": "rr.robot.status.v1",
        "command": "rr status",
        "robot_format": "json",
        "outcome": "complete",
        "generated_at": "2026-04-15T00:00:01Z",
        "exit_code": 0,
        "warnings": [
            "Roger preserved the last persisted review state because this readback path cannot safely reconcile target drift automatically. Re-enter Roger locally to run or inspect the next truthful pass."
        ],
        "repair_actions": [
            "run rr resume --session session-bridge-99 to reopen the Roger session locally",
            "run rr review --repo acme/widgets --pr 12 to start a fresh pass if target drift invalidated the older review"
        ],
        "data": {
            "session": {"id": "session-bridge-99"},
            "attention": {"state": "refresh_recommended"}
        }
    }))
    .expect("serialize status payload");
    let stub_dir = write_stub_roger_binary(&review_payload, 0, &status_payload, 0);
    let stub_rr = stub_dir.path().join("rr-stub");

    let resp = handle_bridge_intent(&intent, &preflight, &stub_rr);
    assert!(resp.ok);
    assert_eq!(resp.attention_state.as_deref(), Some("refresh_recommended"));
    let guidance = resp.guidance.expect("repair guidance");
    assert!(guidance.contains("persisted review state"), "{guidance}");
    assert!(guidance.contains("rr resume --session session-bridge-99"));
    assert!(guidance.contains("rr review --repo acme/widgets --pr 12"));
}

#[cfg(unix)]
#[test]
fn bridge_launch_failure_reports_cli_spawn_failure() {
    let intent = BridgeLaunchIntent {
        action: "start_review".to_owned(),
        owner: "acme".to_owned(),
        repo: "widgets".to_owned(),
        pr_number: 12,
        head_ref: None,
        instance: None,
        extension_id: None,
        browser: None,
    };
    let preflight = BridgePreflight {
        roger_binary_found: true,
        roger_data_dir_exists: true,
        gh_available: true,
    };

    let resp = handle_bridge_intent(&intent, &preflight, Path::new("/missing/rr"));
    assert!(!resp.ok);
    assert_eq!(resp.failure_kind, Some(BridgeFailureKind::CliSpawnFailed));
    assert!(
        resp.guidance
            .as_deref()
            .is_some_and(|guidance| guidance.contains("rr doctor"))
    );
}

#[cfg(unix)]
#[test]
fn bridge_launch_failure_reports_robot_schema_mismatch_for_invalid_payload() {
    let intent = BridgeLaunchIntent {
        action: "start_review".to_owned(),
        owner: "acme".to_owned(),
        repo: "widgets".to_owned(),
        pr_number: 12,
        head_ref: None,
        instance: None,
        extension_id: None,
        browser: None,
    };
    let preflight = BridgePreflight {
        roger_binary_found: true,
        roger_data_dir_exists: true,
        gh_available: true,
    };
    let status_payload = serde_json::to_string_pretty(&serde_json::json!({
        "schema_id": "rr.robot.status.v1",
        "command": "rr status",
        "robot_format": "json",
        "outcome": "complete",
        "generated_at": "2026-04-15T00:00:01Z",
        "exit_code": 0,
        "warnings": [],
        "repair_actions": [],
        "data": {
            "session": {"id": "session-bridge-42"},
            "attention": {"state": "review_launched"}
        }
    }))
    .expect("serialize status payload");
    let stub_dir = write_stub_roger_binary("not-json", 0, &status_payload, 0);
    let stub_rr = stub_dir.path().join("rr-stub");

    let resp = handle_bridge_intent(&intent, &preflight, &stub_rr);
    assert!(!resp.ok);
    assert_eq!(
        resp.failure_kind,
        Some(BridgeFailureKind::RobotSchemaMismatch)
    );
    assert!(
        resp.guidance
            .as_deref()
            .is_some_and(|guidance| guidance.contains("rr review"))
    );
}

#[cfg(unix)]
#[test]
fn bridge_launch_failure_reports_missing_session_id_distinctly() {
    let intent = BridgeLaunchIntent {
        action: "start_review".to_owned(),
        owner: "acme".to_owned(),
        repo: "widgets".to_owned(),
        pr_number: 12,
        head_ref: None,
        instance: None,
        extension_id: None,
        browser: None,
    };
    let preflight = BridgePreflight {
        roger_binary_found: true,
        roger_data_dir_exists: true,
        gh_available: true,
    };
    let review_payload = serde_json::to_string_pretty(&serde_json::json!({
        "schema_id": "rr.robot.review.v1",
        "command": "rr review",
        "robot_format": "json",
        "outcome": "complete",
        "generated_at": "2026-04-15T00:00:00Z",
        "exit_code": 0,
        "warnings": [],
        "repair_actions": [],
        "data": {
            "launch_attempt_id": "attempt-bridge-42"
        }
    }))
    .expect("serialize review payload");
    let status_payload = serde_json::to_string_pretty(&serde_json::json!({
        "schema_id": "rr.robot.status.v1",
        "command": "rr status",
        "robot_format": "json",
        "outcome": "complete",
        "generated_at": "2026-04-15T00:00:01Z",
        "exit_code": 0,
        "warnings": [],
        "repair_actions": [],
        "data": {
            "session": {"id": "session-bridge-42"},
            "attention": {"state": "review_launched"}
        }
    }))
    .expect("serialize status payload");
    let stub_dir = write_stub_roger_binary(&review_payload, 0, &status_payload, 0);
    let stub_rr = stub_dir.path().join("rr-stub");

    let resp = handle_bridge_intent(&intent, &preflight, &stub_rr);
    assert!(!resp.ok);
    assert_eq!(resp.failure_kind, Some(BridgeFailureKind::MissingSessionId));
    assert!(resp.message.contains("canonical Roger session id"));
    assert!(
        resp.guidance
            .as_deref()
            .is_some_and(|guidance| guidance.contains("rr review"))
    );
}

#[cfg(unix)]
#[test]
fn bridge_launch_failure_reports_blocked_cli_outcome_with_real_repair_guidance() {
    let intent = BridgeLaunchIntent {
        action: "start_review".to_owned(),
        owner: "acme".to_owned(),
        repo: "widgets".to_owned(),
        pr_number: 12,
        head_ref: None,
        instance: None,
        extension_id: None,
        browser: None,
    };
    let preflight = BridgePreflight {
        roger_binary_found: true,
        roger_data_dir_exists: true,
        gh_available: true,
    };
    let review_payload = serde_json::to_string_pretty(&serde_json::json!({
        "schema_id": "rr.robot.review.v1",
        "command": "rr review",
        "robot_format": "json",
        "outcome": "blocked",
        "generated_at": "2026-04-15T00:00:00Z",
        "exit_code": 3,
        "warnings": ["resume bundle missing"],
        "repair_actions": ["rr resume --repo acme/widgets --pr 12 --robot --robot-format json"],
        "data": {
            "reason_code": "resume_failed_closed"
        }
    }))
    .expect("serialize review payload");
    let status_payload = serde_json::to_string_pretty(&serde_json::json!({
        "schema_id": "rr.robot.status.v1",
        "command": "rr status",
        "robot_format": "json",
        "outcome": "complete",
        "generated_at": "2026-04-15T00:00:01Z",
        "exit_code": 0,
        "warnings": [],
        "repair_actions": [],
        "data": {
            "session": {"id": "session-bridge-42"},
            "attention": {"state": "review_launched"}
        }
    }))
    .expect("serialize status payload");
    let stub_dir = write_stub_roger_binary(&review_payload, 3, &status_payload, 0);
    let stub_rr = stub_dir.path().join("rr-stub");

    let resp = handle_bridge_intent(&intent, &preflight, &stub_rr);
    assert!(!resp.ok);
    assert_eq!(
        resp.failure_kind,
        Some(BridgeFailureKind::CliOutcomeNotSafe)
    );
    assert_eq!(resp.launch_outcome.as_deref(), Some("blocked"));
    assert!(resp.message.contains("bridge-unsafe outcome 'blocked'"));
    assert!(
        resp.guidance
            .as_deref()
            .is_some_and(|guidance| guidance.contains("Repair actions"))
    );
}

#[cfg(unix)]
#[test]
fn bridge_launch_success_keeps_degraded_outcome_explicit() {
    let intent = BridgeLaunchIntent {
        action: "resume_review".to_owned(),
        owner: "acme".to_owned(),
        repo: "widgets".to_owned(),
        pr_number: 12,
        head_ref: None,
        instance: None,
        extension_id: None,
        browser: None,
    };
    let preflight = BridgePreflight {
        roger_binary_found: true,
        roger_data_dir_exists: true,
        gh_available: true,
    };
    let review_payload = serde_json::to_string_pretty(&serde_json::json!({
        "schema_id": "rr.robot.resume.v1",
        "command": "rr resume",
        "robot_format": "json",
        "outcome": "degraded",
        "generated_at": "2026-04-15T00:00:00Z",
        "exit_code": 5,
        "warnings": ["reopened locator degraded; reseed suggested"],
        "repair_actions": [],
        "data": {
            "session_id": "session-bridge-42"
        }
    }))
    .expect("serialize review payload");
    let status_payload = serde_json::to_string_pretty(&serde_json::json!({
        "schema_id": "rr.robot.status.v1",
        "command": "rr status",
        "robot_format": "json",
        "outcome": "complete",
        "generated_at": "2026-04-15T00:00:01Z",
        "exit_code": 0,
        "warnings": [],
        "repair_actions": [],
        "data": {
            "session": {"id": "session-bridge-42"},
            "attention": {"state": "review_failed"}
        }
    }))
    .expect("serialize status payload");
    let stub_dir = write_stub_roger_binary(&review_payload, 5, &status_payload, 0);
    let stub_rr = stub_dir.path().join("rr-stub");

    let resp = handle_bridge_intent(&intent, &preflight, &stub_rr);
    assert!(resp.ok);
    assert_eq!(resp.launch_outcome.as_deref(), Some("degraded"));
    assert_eq!(resp.attention_state.as_deref(), Some("review_failed"));
    assert!(resp.message.contains("degraded mode"));
    assert!(
        resp.message
            .contains("rr status --session session-bridge-42 --robot --robot-format json")
    );
    let lowered = resp.message.to_ascii_lowercase();
    assert!(!lowered.contains("approval"));
    assert!(!lowered.contains("ready to post"));
}

#[cfg(unix)]
#[test]
fn bridge_launch_failure_reports_noncanonical_status_readback() {
    let intent = BridgeLaunchIntent {
        action: "start_review".to_owned(),
        owner: "acme".to_owned(),
        repo: "widgets".to_owned(),
        pr_number: 12,
        head_ref: None,
        instance: None,
        extension_id: None,
        browser: None,
    };
    let preflight = BridgePreflight {
        roger_binary_found: true,
        roger_data_dir_exists: true,
        gh_available: true,
    };
    let review_payload = serde_json::to_string_pretty(&serde_json::json!({
        "schema_id": "rr.robot.review.v1",
        "command": "rr review",
        "robot_format": "json",
        "outcome": "complete",
        "generated_at": "2026-04-15T00:00:00Z",
        "exit_code": 0,
        "warnings": [],
        "repair_actions": [],
        "data": {
            "session_id": "session-bridge-42",
            "launch_attempt_id": "attempt-bridge-42",
        }
    }))
    .expect("serialize review payload");
    let status_payload = serde_json::to_string_pretty(&serde_json::json!({
        "schema_id": "rr.robot.status.v1",
        "command": "rr status",
        "robot_format": "json",
        "outcome": "complete",
        "generated_at": "2026-04-15T00:00:01Z",
        "exit_code": 0,
        "warnings": [],
        "repair_actions": [],
        "data": {
            "session": {"id": "session-bridge-42"}
        }
    }))
    .expect("serialize status payload");
    let stub_dir = write_stub_roger_binary(&review_payload, 0, &status_payload, 0);
    let stub_rr = stub_dir.path().join("rr-stub");

    let resp = handle_bridge_intent(&intent, &preflight, &stub_rr);
    assert!(!resp.ok);
    assert_eq!(
        resp.failure_kind,
        Some(BridgeFailureKind::RobotSchemaMismatch)
    );
    assert!(resp.guidance.as_deref().is_some_and(|guidance| {
        guidance.contains("rr status --session session-bridge-42 --robot --robot-format json")
    }));
}

#[test]
fn bridge_not_ready_guidance_is_setup_only_not_approval_or_posting_status() {
    let intent = BridgeLaunchIntent {
        action: "resume_review".to_owned(),
        owner: "acme".to_owned(),
        repo: "widgets".to_owned(),
        pr_number: 13,
        head_ref: None,
        instance: None,
        extension_id: None,
        browser: None,
    };
    let preflight = BridgePreflight {
        roger_binary_found: true,
        roger_data_dir_exists: true,
        gh_available: false,
    };

    let resp = handle_bridge_intent(&intent, &preflight, Path::new("/usr/local/bin/rr"));
    assert!(!resp.ok);
    let guidance = resp
        .guidance
        .as_deref()
        .expect("bridge should return setup guidance");
    assert!(guidance.contains("gh auth login"));

    let lowered = guidance.to_ascii_lowercase();
    assert!(!lowered.contains("approval"));
    assert!(!lowered.contains("ready to post"));
}
