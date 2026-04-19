#![cfg(unix)]

use std::fs;
use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("packages parent")
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn assert_contains(text: &str, expected: &str, context: &str) {
    assert!(
        text.contains(expected),
        "{context} missing expected fragment:\n{expected}\n\nFull text:\n{text}"
    );
}

fn assert_not_contains(text: &str, unexpected: &str, context: &str) {
    assert!(
        !text.contains(unexpected),
        "{context} unexpectedly contained fragment:\n{unexpected}\n\nFull text:\n{text}"
    );
}

#[test]
fn agent_mail_endpoint_defaults_use_mcp_path() {
    let ci_intake = fs::read_to_string(workspace_root().join(".github/ci-failure-intake.json"))
        .expect("read ci failure intake config");
    assert_contains(
        &ci_intake,
        "\"api_url\": \"http://127.0.0.1:8765/mcp/\"",
        ".github/ci-failure-intake.json",
    );
    assert_not_contains(
        &ci_intake,
        "\"api_url\": \"http://127.0.0.1:8765/api/\"",
        ".github/ci-failure-intake.json",
    );

    let ingest_script = fs::read_to_string(
        workspace_root().join("scripts/swarm/ingest_failed_actions_runs.py"),
    )
    .expect("read ingest_failed_actions_runs.py");
    assert_contains(
        &ingest_script,
        "DEFAULT_AGENT_MAIL_API = \"http://127.0.0.1:8765/mcp/\"",
        "scripts/swarm/ingest_failed_actions_runs.py",
    );
    assert_not_contains(
        &ingest_script,
        "DEFAULT_AGENT_MAIL_API = \"http://127.0.0.1:8765/api/\"",
        "scripts/swarm/ingest_failed_actions_runs.py",
    );
}
