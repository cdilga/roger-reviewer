#![cfg(unix)]

//! `rr tui` CLI wiring proof: the cockpit is interactive-only by contract.
//!
//! Robot callers fail closed with the `rr.robot.tui.v1` envelope and human
//! callers without an interactive terminal fail closed with explicit
//! guidance toward the machine-readable surfaces. Under `cargo test`,
//! stdin/stdout are not a TTY, so the non-interactive human path is provable
//! in-process.

use roger_cli::{CliRuntime, run};
use serde_json::Value;
use std::path::PathBuf;
use tempfile::tempdir;

fn runtime(store_root: PathBuf, cwd: PathBuf) -> CliRuntime {
    CliRuntime {
        cwd,
        store_root,
        opencode_bin: "opencode-not-installed".to_owned(),
    }
}

fn run_rr(args: &[&str], runtime: &CliRuntime) -> roger_cli::CliRunResult {
    let argv = args
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    run(&argv, runtime)
}

#[test]
fn tui_robot_mode_fails_closed_with_canonical_envelope() {
    let temp = tempdir().expect("tempdir");
    let runtime = runtime(temp.path().join("store"), temp.path().to_path_buf());

    let result = run_rr(&["tui", "--robot"], &runtime);
    assert_eq!(result.exit_code, 3, "blocked exit code: {}", result.stderr);

    let envelope: Value = serde_json::from_str(&result.stdout).expect("robot envelope json");
    assert_eq!(envelope["schema_id"], "rr.robot.tui.v1");
    assert_eq!(envelope["command"], "rr tui");
    assert_eq!(envelope["outcome"], "blocked");
    assert_eq!(envelope["data"]["reason_code"], "robot_mode_unsupported");
    assert_eq!(envelope["data"]["interactive_only"], true);
    let repair_actions = envelope["repair_actions"]
        .as_array()
        .expect("repair actions");
    assert!(
        repair_actions
            .iter()
            .any(|action| action.as_str().is_some_and(|text| text.contains("--robot"))),
        "points robot callers at the machine-readable surfaces: {repair_actions:?}"
    );
}

#[test]
fn tui_human_mode_fails_closed_without_interactive_terminal() {
    let temp = tempdir().expect("tempdir");
    let runtime = runtime(temp.path().join("store"), temp.path().to_path_buf());

    // Under cargo test, stdin/stdout are not a TTY, so this proves the
    // non-interactive human fail-closed path.
    let result = run_rr(&["tui"], &runtime);
    assert_ne!(result.exit_code, 0, "non-TTY human launch must fail closed");
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(
        combined.contains("rr tui requires an interactive terminal"),
        "{combined}"
    );
    assert!(
        !temp.path().join("store").join("roger.sqlite3").exists(),
        "fail-closed TTY gate must run before any store bootstrap"
    );
}

#[test]
fn tui_rejects_unsupported_flags() {
    let temp = tempdir().expect("tempdir");
    let runtime = runtime(temp.path().join("store"), temp.path().to_path_buf());

    let result = run_rr(&["tui", "--dry-run"], &runtime);
    assert_eq!(result.exit_code, 2, "{}", result.stderr);
    assert!(
        result.stderr.contains("rr tui does not support --dry-run"),
        "{}",
        result.stderr
    );

    let result = run_rr(&["tui", "--limit", "5"], &runtime);
    assert_eq!(result.exit_code, 2, "{}", result.stderr);
    assert!(
        result
            .stderr
            .contains("rr tui only supports --repo, --pr, and --session"),
        "{}",
        result.stderr
    );
}

#[test]
fn usage_text_documents_tui_session_targeting() {
    let temp = tempdir().expect("tempdir");
    let runtime = runtime(temp.path().join("store"), temp.path().to_path_buf());

    let result = run_rr(&["help"], &runtime);
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(
        combined.contains("rr tui [--repo owner/repo] [--pr <number>] [--session <id>]"),
        "{combined}"
    );
}

#[test]
fn robot_docs_guide_describes_tui_as_interactive_only() {
    let temp = tempdir().expect("tempdir");
    let runtime = runtime(temp.path().join("store"), temp.path().to_path_buf());

    let result = run_rr(&["robot-docs", "guide", "--robot"], &runtime);
    assert_eq!(result.exit_code, 0, "{}", result.stderr);
    let envelope: Value = serde_json::from_str(&result.stdout).expect("robot envelope json");
    let items = envelope["data"]["items"].as_array().expect("guide items");
    let tui_item = items
        .iter()
        .find(|item| item["command"].as_str() == Some("rr tui"))
        .expect("rr tui guide item");
    assert_eq!(tui_item["interactive_only"], true);
    assert!(
        tui_item["purpose"]
            .as_str()
            .is_some_and(|purpose| purpose.contains("fails closed")),
        "{tui_item:?}"
    );
}
