#![cfg(unix)]

use serde_json::Value;
use std::fs;
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

fn hook_config() -> Value {
    serde_json::from_slice(
        &fs::read(workspace_root().join(".github/hooks/roger-review.json"))
            .expect("read hook config"),
    )
    .expect("parse hook config")
}

fn shell_script(path: &str) -> PathBuf {
    workspace_root().join(path)
}

fn ensure_executable(path: &Path) {
    let mut permissions = fs::metadata(path).expect("stat hook script").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).expect("chmod hook script");
}

#[test]
fn repo_contains_roger_owned_copilot_instruction_and_hook_assets() {
    let root = workspace_root();
    assert!(root.join(".github/copilot-instructions.md").is_file());
    assert!(
        root.join(".github/instructions/rust.instructions.md")
            .is_file(),
        "missing Rust instructions"
    );
    assert!(
        root.join(".github/instructions/extension.instructions.md")
            .is_file(),
        "missing extension instructions"
    );

    let hook_config = hook_config();
    assert_eq!(hook_config["version"], 1);
    let hooks = hook_config["hooks"].as_object().expect("hooks object");

    for hook_name in [
        "sessionStart",
        "userPromptSubmitted",
        "preToolUse",
        "postToolUse",
        "agentStop",
        "sessionEnd",
    ] {
        let entries = hooks.get(hook_name).and_then(Value::as_array);
        assert!(entries.is_some(), "missing {hook_name} entries");
        let entries = entries.expect("entries presence asserted above");
        assert_eq!(entries.len(), 1, "{hook_name} should have one command");
        let entry = &entries[0];
        assert_eq!(entry["type"], "command");
        assert!(
            entry["timeoutSec"].as_u64().unwrap_or_default() >= 10,
            "{hook_name} must have a real timeout"
        );

        let bash = entry["bash"].as_str().expect("bash path");
        let powershell = entry["powershell"].as_str().expect("powershell path");
        assert!(
            root.join(bash).is_file(),
            "missing bash script for {hook_name}"
        );
        assert!(
            root.join(powershell).is_file(),
            "missing powershell script for {hook_name}"
        );
    }
}

#[test]
fn session_start_hook_emits_roger_readable_artifact_with_detected_session_id() {
    let temp = tempdir().expect("tempdir");
    let repo = temp.path().join("repo");
    let copilot_home = temp.path().join("copilot-home");
    let artifact_path = temp.path().join("artifacts/session-start.json");
    let state_root = copilot_home.join("session-state");
    let session_dir = state_root.join("cp-live-001");

    fs::create_dir_all(&repo).expect("create repo");
    fs::create_dir_all(&session_dir).expect("create session dir");

    let script = shell_script("scripts/copilot-hooks/session-start.sh");
    ensure_executable(&script);

    let input = r#"{"timestamp":1704614400000,"cwd":"/tmp/repo","source":"new","initialPrompt":"Review PR 42"}"#;
    let output = Command::new(&script)
        .current_dir(&repo)
        .env("COPILOT_HOME", &copilot_home)
        .env("RR_COPILOT_SESSION_START_ARTIFACT", &artifact_path)
        .env("RR_COPILOT_ATTEMPT_ID", "attempt-123")
        .env("RR_COPILOT_POLICY_PROFILE_DIGEST", "sha256:policy")
        .env(
            "RR_COPILOT_WORKTREE_ROOT",
            repo.to_string_lossy().to_string(),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child
                .stdin
                .as_mut()
                .expect("stdin")
                .write_all(input.as_bytes())?;
            child.wait_with_output()
        })
        .expect("run session-start hook");

    assert!(
        output.status.success(),
        "session-start hook failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let artifact: Value =
        serde_json::from_slice(&fs::read(&artifact_path).expect("read artifact")).expect("json");
    assert_eq!(artifact["hook"], "session-start");
    assert_eq!(artifact["payload"]["provider"], "copilot");
    assert_eq!(artifact["payload"]["session_id"], "cp-live-001");
    assert_eq!(
        artifact["payload"]["worktree_root"],
        repo.to_string_lossy().to_string()
    );
    assert_eq!(artifact["payload"]["launch_profile_id"], "profile-open-pr");
    assert_eq!(artifact["payload"]["attempt_nonce"], "attempt-123");
    assert_eq!(artifact["payload"]["policy_digest"], "sha256:policy");
}

#[test]
fn pre_tool_use_hook_denies_shell_execution_in_review_mode() {
    let temp = tempdir().expect("tempdir");
    let repo = temp.path().join("repo");
    fs::create_dir_all(&repo).expect("create repo");

    let script = shell_script("scripts/copilot-hooks/pre-tool-use.sh");
    ensure_executable(&script);

    let input = r#"{"timestamp":1704614600000,"cwd":"/tmp/repo","toolName":"bash","toolArgs":"{\"command\":\"git status\",\"description\":\"Inspect repo\"}"}"#;
    let output = Command::new(&script)
        .current_dir(&repo)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child
                .stdin
                .as_mut()
                .expect("stdin")
                .write_all(input.as_bytes())?;
            child.wait_with_output()
        })
        .expect("run pre-tool-use hook");

    assert!(
        output.status.success(),
        "pre-tool-use hook failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let decision: Value = serde_json::from_slice(&output.stdout).expect("decision json");
    assert_eq!(decision["permissionDecision"], "deny");
    assert!(
        decision["permissionDecisionReason"]
            .as_str()
            .unwrap_or_default()
            .contains("denies shell execution")
    );
}
