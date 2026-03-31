//! Integration smoke tests for the browser launch bridge.

use std::io::Cursor;
use std::path::Path;

use roger_bridge::{
    BridgeLaunchIntent, BridgePreflight, BridgeResponse, NativeHostManifest, SupportedBrowser,
    handle_bridge_intent, parse_custom_url, read_native_message, write_native_message,
};

#[test]
fn native_messaging_end_to_end() {
    let intent = BridgeLaunchIntent {
        action: "resume_review".to_owned(),
        owner: "acme".to_owned(),
        repo: "widgets".to_owned(),
        pr_number: 99,
        head_ref: None,
        instance: Some("my-inst".to_owned()),
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
fn custom_url_launch_flow() {
    let intent = parse_custom_url("roger://launch/acme/widgets/42?action=show_findings").unwrap();
    assert_eq!(intent.owner, "acme");
    assert_eq!(intent.repo, "widgets");
    assert_eq!(intent.pr_number, 42);
    assert_eq!(intent.action, "show_findings");

    // Process through handler with a ready preflight.
    let preflight = BridgePreflight {
        roger_binary_found: true,
        roger_data_dir_exists: true,
        gh_available: true,
    };
    let resp = handle_bridge_intent(&intent, &preflight, Path::new("/usr/local/bin/rr"));
    assert!(resp.ok);
    assert!(resp.message.contains("show_findings"));
}

#[test]
fn fail_closed_when_roger_not_installed() {
    let intent = parse_custom_url("roger://launch/acme/widgets/1").unwrap();
    let preflight = BridgePreflight {
        roger_binary_found: false,
        roger_data_dir_exists: false,
        gh_available: false,
    };
    let resp = handle_bridge_intent(&intent, &preflight, Path::new("/missing/rr"));
    assert!(!resp.ok);
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
        NativeHostManifest::for_roger(Path::new("/usr/local/bin/rr-bridge"), "test-extension-id");
    assert_eq!(manifest.host_type, "stdio");
    assert!(manifest.allowed_origins[0].contains("test-extension-id"));
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
fn refresh_review_action_is_accepted() {
    let intent = BridgeLaunchIntent {
        action: "refresh_review".to_owned(),
        owner: "acme".to_owned(),
        repo: "widgets".to_owned(),
        pr_number: 7,
        head_ref: None,
        instance: None,
    };
    let preflight = BridgePreflight {
        roger_binary_found: true,
        roger_data_dir_exists: true,
        gh_available: true,
    };
    let resp = handle_bridge_intent(&intent, &preflight, Path::new("/usr/local/bin/rr"));
    assert!(resp.ok);
    assert_eq!(resp.action, "refresh_review");
}

#[test]
fn bridge_launch_response_stays_launch_only_without_posting_readiness_signals() {
    let intent = BridgeLaunchIntent {
        action: "start_review".to_owned(),
        owner: "acme".to_owned(),
        repo: "widgets".to_owned(),
        pr_number: 12,
        head_ref: None,
        instance: None,
    };
    let preflight = BridgePreflight {
        roger_binary_found: true,
        roger_data_dir_exists: true,
        gh_available: true,
    };

    let resp = handle_bridge_intent(&intent, &preflight, Path::new("/usr/local/bin/rr"));
    assert!(resp.ok);
    assert_eq!(resp.session_id, None);
    assert_eq!(resp.guidance, None);

    let message = resp.message.to_ascii_lowercase();
    assert!(message.contains("dispatching"));
    assert!(!message.contains("approval"));
    assert!(!message.contains("ready to post"));
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
