#![cfg(unix)]
//! Regression coverage for the 0.2 CLI-polish slice:
//! - `rr sessions --attention <state>` validates against the canonical
//!   attention-state vocabulary and fails closed on a typo instead of
//!   returning an empty exit-0 result.
//! - `rr findings` returns empty/exit-0 (not blocked/exit-3) for a target with
//!   no session yet, matching the `rr status`/`rr sessions` reads model.
//! - `rr triage` brings its required-argument validation into `--robot`
//!   envelope conformance (blocked JSON, exit 3) like draft/approve/post.

use roger_cli::{CliRuntime, run};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::{TempDir, tempdir};

fn init_repo(temp: &TempDir) -> PathBuf {
    let repo = temp.path().join("repo");
    fs::create_dir_all(&repo).expect("create repo dir");

    let init = Command::new("git")
        .arg("-C")
        .arg(&repo)
        .arg("init")
        .output()
        .expect("git init");
    assert!(init.status.success(), "git init failed");

    let remote = Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args([
            "remote",
            "add",
            "origin",
            "https://github.com/owner/repo.git",
        ])
        .output()
        .expect("git remote add");
    assert!(remote.status.success(), "git remote add failed");

    repo
}

fn run_rr(args: &[&str], runtime: &CliRuntime) -> roger_cli::CliRunResult {
    let argv = args
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    run(&argv, runtime)
}

fn parse_robot_payload(stdout: &str) -> Value {
    serde_json::from_str(stdout).expect("robot payload json")
}

fn runtime_for(temp: &TempDir) -> CliRuntime {
    CliRuntime {
        cwd: init_repo(temp),
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    }
}

#[test]
fn sessions_unknown_attention_state_fails_closed_with_blocked_envelope() {
    let temp = tempdir().expect("tempdir");
    let runtime = runtime_for(&temp);

    let blocked = run_rr(
        &["sessions", "--attention", "bogus_state", "--robot"],
        &runtime,
    );
    assert_eq!(blocked.exit_code, 3, "{}", blocked.stderr);
    let payload = parse_robot_payload(&blocked.stdout);
    assert_eq!(payload["schema_id"], "rr.robot.sessions.v1");
    assert_eq!(payload["command"], "rr sessions");
    assert_eq!(payload["outcome"], "blocked");
    assert_eq!(payload["data"]["reason_code"], "unknown_attention_state");
    assert_eq!(
        payload["data"]["unknown_attention_states"],
        serde_json::json!(["bogus_state"])
    );
    // The supported vocabulary must be advertised so the operator can recover.
    let supported = payload["data"]["supported_attention_states"]
        .as_array()
        .expect("supported attention states list");
    assert!(supported.iter().any(|state| state == "findings_ready"));
    assert!(supported.iter().any(|state| state == "awaiting_user_input"));
}

#[test]
fn sessions_canonical_attention_states_are_accepted() {
    let temp = tempdir().expect("tempdir");
    let runtime = runtime_for(&temp);

    // Every state the system can actually produce must be a valid filter and
    // never be rejected as unknown (empty exit-0 is fine; blocked is not).
    for state in [
        "awaiting_return",
        "awaiting_user_input",
        "findings_ready",
        "outbound_approval_required",
        "refresh_recommended",
        "returned_to_roger",
        "review_failed",
        "review_launched",
        "review_resumed",
    ] {
        let result = run_rr(&["sessions", "--attention", state, "--robot"], &runtime);
        assert_eq!(result.exit_code, 0, "state {state}: {}", result.stderr);
        let payload = parse_robot_payload(&result.stdout);
        assert_eq!(payload["outcome"], "empty", "state {state}");
    }

    // A comma-separated list with one typo still fails closed and names the typo.
    let mixed = run_rr(
        &[
            "sessions",
            "--attention",
            "findings_ready,oops_typo",
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(mixed.exit_code, 3, "{}", mixed.stderr);
    let mixed_payload = parse_robot_payload(&mixed.stdout);
    assert_eq!(mixed_payload["outcome"], "blocked");
    assert_eq!(
        mixed_payload["data"]["unknown_attention_states"],
        serde_json::json!(["oops_typo"])
    );
}

#[test]
fn findings_with_no_session_yet_returns_empty_exit_zero() {
    let temp = tempdir().expect("tempdir");
    let runtime = runtime_for(&temp);

    // No session exists for this target yet. rr findings must mirror rr status /
    // rr sessions and return empty/exit-0 rather than blocking with exit 3.
    let findings = run_rr(
        &["findings", "--repo", "owner/repo", "--pr", "99", "--robot"],
        &runtime,
    );
    assert_eq!(findings.exit_code, 0, "{}", findings.stderr);
    let findings_payload = parse_robot_payload(&findings.stdout);
    assert_eq!(findings_payload["outcome"], "empty");
    assert_eq!(findings_payload["data"]["count"], 0);

    // rr status agrees: empty/exit-0 for the same no-session-yet target.
    let status = run_rr(
        &["status", "--repo", "owner/repo", "--pr", "99", "--robot"],
        &runtime,
    );
    assert_eq!(status.exit_code, 0, "{}", status.stderr);
    assert_eq!(parse_robot_payload(&status.stdout)["outcome"], "empty");
}

#[test]
fn triage_robot_arg_validation_matches_outbound_command_conformance() {
    let temp = tempdir().expect("tempdir");
    let runtime = runtime_for(&temp);

    // Missing --finding: blocked envelope, not plain text + exit 2.
    let no_finding = run_rr(&["triage", "--state", "accepted", "--robot"], &runtime);
    assert_eq!(no_finding.exit_code, 3, "{}", no_finding.stderr);
    let no_finding_payload = parse_robot_payload(&no_finding.stdout);
    assert_eq!(no_finding_payload["schema_id"], "rr.robot.triage.v1");
    assert_eq!(no_finding_payload["outcome"], "blocked");
    assert_eq!(
        no_finding_payload["data"]["reason_code"],
        "finding_selection_required"
    );

    // Missing --state: blocked envelope.
    let no_state = run_rr(&["triage", "--finding", "finding-1", "--robot"], &runtime);
    assert_eq!(no_state.exit_code, 3, "{}", no_state.stderr);
    assert_eq!(
        parse_robot_payload(&no_state.stdout)["data"]["reason_code"],
        "triage_state_required"
    );

    // Unsupported --state: blocked envelope that names the requested value.
    let bad_state = run_rr(
        &[
            "triage",
            "--finding",
            "finding-1",
            "--state",
            "approved",
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(bad_state.exit_code, 3, "{}", bad_state.stderr);
    let bad_state_payload = parse_robot_payload(&bad_state.stdout);
    assert_eq!(bad_state_payload["outcome"], "blocked");
    assert_eq!(
        bad_state_payload["data"]["reason_code"],
        "unsupported_triage_state"
    );
    assert_eq!(bad_state_payload["data"]["requested_state"], "approved");
}

#[test]
fn global_help_leads_with_the_seven_verbs_and_hides_bridge() {
    let temp = tempdir().expect("tempdir");
    let runtime = runtime_for(&temp);

    let help = run_rr(&["--help"], &runtime);
    assert_eq!(help.exit_code, 0, "{}", help.stderr);
    assert!(help.stdout.contains("The seven verbs:"), "{}", help.stdout);
    for verb in [
        "rr doctor",
        "rr queue",
        "rr review",
        "rr open",
        "rr findings",
        "rr send",
        "rr setup",
    ] {
        assert!(
            help.stdout.contains(verb),
            "missing {verb} in help:\n{}",
            help.stdout
        );
    }
    // Container forms are advertised.
    assert!(help.stdout.contains("rr send post"), "{}", help.stdout);
    assert!(
        help.stdout.contains("rr setup extension"),
        "{}",
        help.stdout
    );
    assert!(help.stdout.contains("rr api docs"), "{}", help.stdout);
    // bridge disappears from operator help, and the aspirational line is gone.
    assert!(
        !help.stdout.contains("rr bridge "),
        "bridge leaked into help:\n{}",
        help.stdout
    );
    assert!(!help.stdout.contains("moving toward"), "{}", help.stdout);
}

#[test]
fn per_command_help_is_available_at_any_position_exit_zero() {
    let temp = tempdir().expect("tempdir");
    let runtime = runtime_for(&temp);

    for (args, needle) in [
        (&["review", "--help"][..], "rr review"),
        (&["review", "--pr", "5", "-h"][..], "rr review"),
        (&["send", "--help"][..], "rr send"),
        (&["send", "post", "--help"][..], "rr send"),
        (&["setup", "--help"][..], "rr setup"),
        (&["setup", "assets", "-h"][..], "rr setup"),
        (&["api", "docs", "--help"][..], "rr api docs"),
        (&["findings", "--help"][..], "rr findings"),
    ] {
        let result = run_rr(args, &runtime);
        assert_eq!(
            result.exit_code, 0,
            "args={args:?} stderr={}",
            result.stderr
        );
        assert!(
            result.stderr.is_empty(),
            "args={args:?} stderr={}",
            result.stderr
        );
        assert!(
            result.stdout.contains(needle),
            "args={args:?} missing {needle}:\n{}",
            result.stdout
        );
    }
}

#[test]
fn send_and_findings_containers_route_to_underlying_robot_schema_ids() {
    let temp = tempdir().expect("tempdir");
    let runtime = runtime_for(&temp);

    // rr send post emits the post schema id (blocked on missing draft, but the
    // envelope proves the routing).
    let post = run_rr(&["send", "post", "--batch", "missing", "--robot"], &runtime);
    let post_payload = parse_robot_payload(&post.stdout);
    assert_eq!(post_payload["schema_id"], "rr.robot.post.v1");

    // rr findings --sessions routes to the sessions handler.
    let sessions = run_rr(&["findings", "--sessions", "--robot"], &runtime);
    let sessions_payload = parse_robot_payload(&sessions.stdout);
    assert_eq!(sessions_payload["schema_id"], "rr.robot.sessions.v1");

    // rr findings --query routes to the search handler.
    let search = run_rr(&["findings", "--query", "auth", "--robot"], &runtime);
    let search_payload = parse_robot_payload(&search.stdout);
    assert_eq!(search_payload["schema_id"], "rr.robot.search.v1");
}

#[test]
fn send_edit_routes_to_a_real_command() {
    let temp = tempdir().expect("tempdir");
    let runtime = runtime_for(&temp);

    // `rr send edit` is a real command now. Missing a body source is a usage
    // (exit 2) error, not the old "not yet available" reservation.
    let missing_body = run_rr(&["send", "edit", "--draft", "d1"], &runtime);
    assert_eq!(missing_body.exit_code, 2, "{}", missing_body.stdout);
    assert!(
        missing_body
            .stderr
            .contains("requires --body-file <path> or --editor"),
        "{}",
        missing_body.stderr
    );

    // It is a local human action, not a --robot transport.
    let robot = run_rr(
        &[
            "send",
            "edit",
            "--draft",
            "d1",
            "--body-file",
            "/tmp/b",
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(robot.exit_code, 2, "{}", robot.stdout);
    assert!(
        robot.stderr.contains("does not support --robot"),
        "{}",
        robot.stderr
    );
}
