#![cfg(unix)]

use roger_cli::{CliRuntime, run};
use roger_session_copilot::ENV_COPILOT_ADMISSION_GATE;
use serde_json::Value;
use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use tempfile::tempdir;

fn parse_robot(stdout: &str) -> Value {
    serde_json::from_str(stdout).expect("robot payload")
}

fn init_repo(path: &Path) {
    fs::create_dir_all(path).expect("create repo path");
    let init = Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("init")
        .output()
        .expect("git init");
    assert!(
        init.status.success(),
        "git init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );
    let remote = Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("remote")
        .arg("add")
        .arg("origin")
        .arg("git@github.com:owner/repo.git")
        .output()
        .expect("git remote add origin");
    assert!(
        remote.status.success(),
        "git remote add origin failed: {}",
        String::from_utf8_lossy(&remote.stderr)
    );
}

fn run_with_env_overrides<T>(
    overrides: &[(&str, Option<&str>)],
    run_block: impl FnOnce() -> T,
) -> T {
    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = ENV_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("env lock");

    let previous: Vec<(String, Option<OsString>)> = overrides
        .iter()
        .map(|(key, _)| ((*key).to_owned(), std::env::var_os(key)))
        .collect();

    for (key, value) in overrides {
        match value {
            Some(value) => {
                // SAFETY: tests serialize environment mutation via ENV_LOCK and restore before return.
                unsafe {
                    std::env::set_var(key, value);
                }
            }
            None => {
                // SAFETY: tests serialize environment mutation via ENV_LOCK and restore before return.
                unsafe {
                    std::env::remove_var(key);
                }
            }
        }
    }

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(run_block));

    for (key, value) in previous {
        match value {
            Some(value) => {
                // SAFETY: tests serialize environment mutation via ENV_LOCK and restore before return.
                unsafe {
                    std::env::set_var(&key, value);
                }
            }
            None => {
                // SAFETY: tests serialize environment mutation via ENV_LOCK and restore before return.
                unsafe {
                    std::env::remove_var(&key);
                }
            }
        }
    }

    match result {
        Ok(value) => value,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

#[test]
fn copilot_gate_enabled_updates_robot_provider_support_surfaces() {
    let temp = tempdir().expect("tempdir");
    let repo = temp.path().join("repo");
    init_repo(&repo);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };

    let gate = "1".to_owned();
    run_with_env_overrides(&[(ENV_COPILOT_ADMISSION_GATE, Some(gate.as_str()))], || {
        let commands = run(
            &[
                "robot-docs".to_owned(),
                "commands".to_owned(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(commands.exit_code, 0, "{}", commands.stderr);
        let commands_payload = parse_robot(&commands.stdout);
        let command_items = commands_payload["data"]["items"]
            .as_array()
            .expect("command items");
        let review_dry_run = command_items
            .iter()
            .find(|item| item["command"] == "rr review --dry-run")
            .expect("review dry-run command item");
        assert_eq!(
            review_dry_run["supported_providers"],
            serde_json::json!(["opencode", "codex", "gemini", "claude", "copilot"])
        );
        assert_eq!(
            review_dry_run["planned_not_live_providers"],
            serde_json::json!([])
        );
        assert_eq!(
            review_dry_run["not_supported_providers"],
            serde_json::json!(["pi-agent"])
        );

        let guide = run(
            &[
                "robot-docs".to_owned(),
                "guide".to_owned(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(guide.exit_code, 0, "{}", guide.stderr);
        let guide_payload = parse_robot(&guide.stdout);
        let guide_items = guide_payload["data"]["items"]
            .as_array()
            .expect("guide items");
        let provider_support = guide_items
            .iter()
            .find(|item| item["kind"] == "provider_support")
            .expect("provider support guide item");
        assert_eq!(
            provider_support["planned_not_live_providers"],
            serde_json::json!([])
        );
        let summary = provider_support["summary"]
            .as_str()
            .expect("provider support summary");
        assert!(
            summary.contains(
                "GitHub Copilot CLI is feature-gated as a bounded tier-b continuity path"
            ),
            "{summary}"
        );
        let live_review_providers = provider_support["live_review_providers"]
            .as_array()
            .expect("live review providers");
        let copilot = live_review_providers
            .iter()
            .find(|item| item["provider"] == "copilot")
            .expect("copilot support entry");
        assert_eq!(copilot["display_name"], "GitHub Copilot CLI");
        assert_eq!(copilot["status"], "feature_gated_bounded_live");
        assert_eq!(copilot["tier"], "tier_b_feature_gated");
        assert_eq!(
            copilot["notes"],
            "feature-gated bounded tier-b continuity path: verified start, locator/session-id reopen, rr return, and honest ResumeBundle reseed fallback; no default public live claim"
        );
        assert_eq!(copilot["supports"]["review_start"], true);
        assert_eq!(copilot["supports"]["resume_reseed"], true);
        assert_eq!(copilot["supports"]["resume_reopen"], true);
        assert_eq!(copilot["supports"]["return"], true);
        assert_eq!(copilot["supports"]["doctor"], true);

        let blocked = run(
            &[
                "review".to_owned(),
                "--pr".to_owned(),
                "42".to_owned(),
                "--provider".to_owned(),
                "pi-agent".to_owned(),
                "--robot".to_owned(),
            ],
            &runtime,
        );
        assert_eq!(blocked.exit_code, 3, "{}", blocked.stderr);
        let blocked_payload = parse_robot(&blocked.stdout);
        assert_eq!(
            blocked_payload["data"]["supported_providers"],
            serde_json::json!(["opencode", "codex", "gemini", "claude", "copilot"])
        );
        assert_eq!(
            blocked_payload["data"]["planned_not_live_providers"],
            serde_json::json!([])
        );
        let blocked_support = blocked_payload["data"]["live_review_provider_support"]
            .as_array()
            .expect("blocked live review provider support");
        assert!(
            blocked_support
                .iter()
                .any(|item| item["provider"] == "copilot"
                    && item["status"] == "feature_gated_bounded_live")
        );
    });
}
