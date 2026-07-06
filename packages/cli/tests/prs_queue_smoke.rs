//! Smoke tests for the read-only `rr prs` review-queue surface.
//!
//! `gh` is stubbed with a fake script on PATH (same pattern as
//! post_contract_smoke.rs) so no live GitHub CLI or network is required.

#![cfg(unix)]

use roger_app_core::ReviewTarget;
use roger_cli::{CliRuntime, run};
use roger_storage::{CreateMaterializedFinding, CreateReviewRun, CreateReviewSession, RogerStore};
use serde_json::Value;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, MutexGuard, OnceLock};
use tempfile::{TempDir, tempdir};

fn sample_target(repository: &str, pr_number: u64) -> ReviewTarget {
    ReviewTarget {
        repository: repository.to_owned(),
        pull_request_number: pr_number,
        base_ref: "main".to_owned(),
        head_ref: format!("feature-{pr_number}"),
        base_commit: "aaa".to_owned(),
        head_commit: "bbb".to_owned(),
    }
}

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
        .map(|value| (*value).to_owned())
        .collect::<Vec<_>>();
    run(&argv, runtime)
}

fn parse_robot_payload(stdout: &str) -> Value {
    serde_json::from_str(stdout).expect("robot payload json")
}

fn seed_session(
    runtime: &CliRuntime,
    session_id: &str,
    target: &ReviewTarget,
    attention_state: &str,
) {
    let store = RogerStore::open(&runtime.store_root).expect("open store");
    store
        .create_review_session(CreateReviewSession {
            id: session_id,
            review_target: target,
            provider: "opencode",
            session_locator: None,
            resume_bundle_artifact_id: None,
            continuity_state: "resume:usable",
            attention_state,
            launch_profile_id: Some("profile-open-pr"),
        })
        .expect("create review session");
}

fn seed_session_with_draftable_finding(
    runtime: &CliRuntime,
    session_id: &str,
    run_id: &str,
    target: &ReviewTarget,
) {
    seed_session(runtime, session_id, target, "awaiting_user_input");
    let store = RogerStore::open(&runtime.store_root).expect("open store");
    store
        .create_review_run(CreateReviewRun {
            id: run_id,
            session_id,
            run_kind: "deep_review",
            repo_snapshot: "{\"head\":\"bbb\"}",
            continuity_quality: "usable",
            session_locator_artifact_id: None,
        })
        .expect("create review run");
    store
        .upsert_materialized_finding(CreateMaterializedFinding {
            id: "finding-queue-1",
            session_id,
            review_run_id: run_id,
            stage: "deep_review",
            fingerprint: "fp:finding-queue-1",
            title: "Queue smoke finding",
            normalized_summary: "queue smoke draftable finding summary",
            severity: "high",
            confidence: "medium",
            triage_state: "accepted",
            outbound_state: "not_drafted",
        })
        .expect("seed materialized finding");
}

static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn restore_env(key: &str, previous: Option<OsString>) {
    // SAFETY: test-only scoped environment mutation guarded by a global mutex.
    unsafe {
        if let Some(value) = previous {
            std::env::set_var(key, value);
        } else {
            std::env::remove_var(key);
        }
    }
}

struct FakeGhEnvGuard {
    _lock: MutexGuard<'static, ()>,
    old_path: Option<OsString>,
    old_log: Option<OsString>,
    old_prs_file: Option<OsString>,
    old_fail_stderr: Option<OsString>,
}

impl Drop for FakeGhEnvGuard {
    fn drop(&mut self) {
        restore_env("PATH", self.old_path.clone());
        restore_env("RR_TEST_GH_LOG", self.old_log.clone());
        restore_env("RR_TEST_GH_PRS_FILE", self.old_prs_file.clone());
        restore_env("RR_TEST_GH_FAIL_STDERR", self.old_fail_stderr.clone());
    }
}

fn env_lock() -> MutexGuard<'static, ()> {
    ENV_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
}

enum FakeGhMode<'a> {
    /// Serve the given `gh pr list` JSON payload.
    ListPayload(&'a str),
    /// Exit non-zero with the given stderr line.
    FailWithStderr(&'a str),
    /// Do not install any `gh` binary; PATH keeps only `git`.
    Missing,
}

fn install_fake_gh(temp: &TempDir, mode: FakeGhMode<'_>) -> (FakeGhEnvGuard, PathBuf) {
    let lock = env_lock();

    let bin_dir = temp.path().join("fake-gh-bin");
    fs::create_dir_all(&bin_dir).expect("create fake gh dir");
    let log_path = temp.path().join("fake-gh.log");
    let prs_path = temp.path().join("fake-gh-prs.json");

    if !matches!(mode, FakeGhMode::Missing) {
        let script_path = bin_dir.join("gh");
        let script = r#"#!/bin/sh
set -eu
log_file="${RR_TEST_GH_LOG:?}"
printf '%s\n' "$*" >> "$log_file"
fail_stderr="${RR_TEST_GH_FAIL_STDERR:-}"
if [ -n "$fail_stderr" ]; then
  printf '%s\n' "$fail_stderr" >&2
  exit 1
fi
cat "${RR_TEST_GH_PRS_FILE:?}"
"#;
        fs::write(&script_path, script).expect("write fake gh script");
        let mut permissions = fs::metadata(&script_path)
            .expect("fake gh metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script_path, permissions).expect("chmod fake gh");
    }

    let old_path = std::env::var_os("PATH");
    let old_log = std::env::var_os("RR_TEST_GH_LOG");
    let old_prs_file = std::env::var_os("RR_TEST_GH_PRS_FILE");
    let old_fail_stderr = std::env::var_os("RR_TEST_GH_FAIL_STDERR");

    let joined_path = match (&mode, old_path.as_ref()) {
        // Missing mode: restrict PATH to the (gh-less) fake bin dir so the
        // adapter fails closed with "gh not found".
        (FakeGhMode::Missing, _) => OsString::from(bin_dir.as_os_str()),
        (_, Some(existing)) => {
            let mut value = OsString::from(bin_dir.as_os_str());
            value.push(":");
            value.push(existing);
            value
        }
        (_, None) => OsString::from(bin_dir.as_os_str()),
    };

    // SAFETY: test-only scoped environment mutation guarded by a global mutex.
    unsafe {
        std::env::set_var("PATH", joined_path);
        std::env::set_var("RR_TEST_GH_LOG", &log_path);
        match mode {
            FakeGhMode::ListPayload(payload) => {
                fs::write(&prs_path, payload).expect("write fake gh prs payload");
                std::env::set_var("RR_TEST_GH_PRS_FILE", &prs_path);
                std::env::remove_var("RR_TEST_GH_FAIL_STDERR");
            }
            FakeGhMode::FailWithStderr(stderr) => {
                std::env::set_var("RR_TEST_GH_FAIL_STDERR", stderr);
                std::env::remove_var("RR_TEST_GH_PRS_FILE");
            }
            FakeGhMode::Missing => {
                std::env::remove_var("RR_TEST_GH_PRS_FILE");
                std::env::remove_var("RR_TEST_GH_FAIL_STDERR");
            }
        }
    }

    (
        FakeGhEnvGuard {
            _lock: lock,
            old_path,
            old_log,
            old_prs_file,
            old_fail_stderr,
        },
        log_path,
    )
}

fn read_log_lines(log_path: &Path) -> Vec<String> {
    match fs::read_to_string(log_path) {
        Ok(log) => log.lines().map(|line| line.to_owned()).collect(),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(err) => vec![format!("__read_log_error__:{err}")],
    }
}

const TWO_OPEN_PRS_PAYLOAD: &str = r#"[
  {"number":42,"title":"Fix widget alignment","headRefName":"fix/widget","updatedAt":"2026-06-02T10:00:00Z","author":{"login":"dev"},"isDraft":false,"url":"https://github.com/owner/repo/pull/42"},
  {"number":43,"title":"Refactor adapter seam","headRefName":"chore/adapter","updatedAt":"2026-06-01T09:00:00Z","author":{"login":"other"},"isDraft":true,"url":"https://github.com/owner/repo/pull/43"}
]"#;

#[test]
fn prs_robot_joins_open_prs_with_local_session_state() {
    let temp = tempdir().expect("tempdir");
    let (_guard, log_path) = install_fake_gh(&temp, FakeGhMode::ListPayload(TWO_OPEN_PRS_PAYLOAD));
    let repo = init_repo(&temp);
    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };

    // PR 43 has a local session awaiting user input; PR 42 has none.
    seed_session(
        &runtime,
        "session-queue-43",
        &sample_target("owner/repo", 43),
        "awaiting_user_input",
    );

    let result = run_rr(&["prs", "--robot"], &runtime);
    assert_eq!(result.exit_code, 0, "{}", result.stderr);
    let payload = parse_robot_payload(&result.stdout);
    assert_eq!(payload["schema_id"], "rr.robot.prs.v1");
    assert_eq!(payload["command"], "rr prs");
    assert_eq!(payload["outcome"], "complete");
    assert_eq!(payload["data"]["repository"], "owner/repo");
    assert_eq!(payload["data"]["count"], 2);

    let items = payload["data"]["items"].as_array().expect("items array");
    assert_eq!(items.len(), 2);

    let not_started = &items[0];
    assert_eq!(not_started["pr_number"], 42);
    assert_eq!(not_started["title"], "Fix widget alignment");
    assert_eq!(not_started["author"], "dev");
    assert_eq!(not_started["is_draft"], false);
    assert_eq!(not_started["updated_at"], "2026-06-02T10:00:00Z");
    assert_eq!(not_started["roger_state"], "not_started");
    assert!(not_started["session_id"].is_null());
    assert_eq!(
        not_started["next_command"],
        "rr review --pr 42 --provider opencode"
    );

    let joined = &items[1];
    assert_eq!(joined["pr_number"], 43);
    assert_eq!(joined["is_draft"], true);
    assert_eq!(joined["roger_state"], "needs_attention");
    assert_eq!(joined["session_id"], "session-queue-43");
    assert_eq!(joined["next_command"], "rr resume --pr 43");

    // The adapter must use the read-only gh pr list surface.
    let log_lines = read_log_lines(&log_path);
    assert_eq!(log_lines.len(), 1, "gh invocations: {log_lines:?}");
    assert_eq!(
        log_lines[0],
        "pr list --repo owner/repo --state open --limit 25 --json number,title,headRefName,updatedAt,author,isDraft,url"
    );
}

#[test]
fn prs_joins_in_review_and_drafted_states_truthfully() {
    let temp = tempdir().expect("tempdir");
    let (_guard, _log_path) = install_fake_gh(&temp, FakeGhMode::ListPayload(TWO_OPEN_PRS_PAYLOAD));
    let repo = init_repo(&temp);
    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };

    // PR 42: session mid-review.
    seed_session(
        &runtime,
        "session-queue-42",
        &sample_target("owner/repo", 42),
        "review_launched",
    );
    // PR 43: session with a real local draft batch (materialized via rr draft).
    seed_session_with_draftable_finding(
        &runtime,
        "session-queue-43",
        "run-queue-43",
        &sample_target("owner/repo", 43),
    );
    let draft = run_rr(
        &[
            "draft",
            "--session",
            "session-queue-43",
            "--finding",
            "finding-queue-1",
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(draft.exit_code, 0, "{}", draft.stderr);

    let result = run_rr(&["prs", "--robot"], &runtime);
    assert_eq!(result.exit_code, 0, "{}", result.stderr);
    let payload = parse_robot_payload(&result.stdout);
    let items = payload["data"]["items"].as_array().expect("items array");
    assert_eq!(items[0]["pr_number"], 42);
    assert_eq!(items[0]["roger_state"], "in_review");
    assert_eq!(items[0]["next_command"], "rr resume --pr 42");
    assert_eq!(items[1]["pr_number"], 43);
    assert_eq!(items[1]["roger_state"], "drafted");
    assert_eq!(items[1]["next_command"], "rr resume --pr 43");
}

#[test]
fn prs_passes_explicit_repo_and_limit_to_gh() {
    let temp = tempdir().expect("tempdir");
    let (_guard, log_path) = install_fake_gh(&temp, FakeGhMode::ListPayload("[]"));
    let runtime = CliRuntime {
        cwd: temp.path().to_path_buf(),
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };

    let result = run_rr(
        &["prs", "--repo", "acme/widgets", "--limit", "5", "--robot"],
        &runtime,
    );
    assert_eq!(result.exit_code, 0, "{}", result.stderr);
    let payload = parse_robot_payload(&result.stdout);
    assert_eq!(payload["outcome"], "empty");
    assert_eq!(payload["data"]["count"], 0);
    assert_eq!(payload["data"]["filters_applied"]["limit"], 5);

    let log_lines = read_log_lines(&log_path);
    assert_eq!(log_lines.len(), 1, "gh invocations: {log_lines:?}");
    assert_eq!(
        log_lines[0],
        "pr list --repo acme/widgets --state open --limit 5 --json number,title,headRefName,updatedAt,author,isDraft,url"
    );
}

#[test]
fn prs_human_output_renders_compact_aligned_table() {
    let temp = tempdir().expect("tempdir");
    let (_guard, _log_path) = install_fake_gh(&temp, FakeGhMode::ListPayload(TWO_OPEN_PRS_PAYLOAD));
    let repo = init_repo(&temp);
    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };

    let result = run_rr(&["prs"], &runtime);
    assert_eq!(result.exit_code, 0, "{}", result.stderr);
    assert!(
        result
            .stdout
            .contains("loaded 2 open pull requests for owner/repo"),
        "{}",
        result.stdout
    );
    let lines: Vec<&str> = result.stdout.lines().collect();
    let header = lines
        .iter()
        .find(|line| line.starts_with("PR"))
        .expect("table header row");
    assert!(header.contains("STATE"), "{header}");
    assert!(header.contains("NEXT"), "{header}");
    assert!(
        lines.iter().any(|line| line.starts_with("#42")
            && line.contains("not_started")
            && line.contains("rr review --pr 42 --provider opencode")),
        "{}",
        result.stdout
    );
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("#43") && line.contains("not_started")),
        "{}",
        result.stdout
    );
}

#[test]
fn queue_alias_routes_to_prs_review_queue() {
    let temp = tempdir().expect("tempdir");
    let (_guard, _log_path) = install_fake_gh(&temp, FakeGhMode::ListPayload(TWO_OPEN_PRS_PAYLOAD));
    let repo = init_repo(&temp);
    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };

    let result = run_rr(&["queue", "--robot"], &runtime);
    assert_eq!(result.exit_code, 0, "{}", result.stderr);
    let payload = parse_robot_payload(&result.stdout);
    assert_eq!(payload["schema_id"], "rr.robot.prs.v1");
    assert_eq!(payload["command"], "rr prs");
    assert_eq!(payload["data"]["items"].as_array().expect("items").len(), 2);
}

#[test]
fn prs_robot_compact_format_projects_queue_essentials() {
    let temp = tempdir().expect("tempdir");
    let (_guard, _log_path) = install_fake_gh(&temp, FakeGhMode::ListPayload(TWO_OPEN_PRS_PAYLOAD));
    let repo = init_repo(&temp);
    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };

    let result = run_rr(&["prs", "--robot", "--robot-format", "compact"], &runtime);
    assert_eq!(result.exit_code, 0, "{}", result.stderr);
    let payload = parse_robot_payload(&result.stdout);
    assert_eq!(payload["schema_id"], "rr.robot.prs.v1");
    assert_eq!(payload["robot_format"], "compact");
    assert_eq!(payload["data"]["count"], 2);
    let items = payload["data"]["items"].as_array().expect("compact items");
    assert_eq!(items[0]["pr_number"], 42);
    assert_eq!(items[0]["roger_state"], "not_started");
    assert!(items[0].get("title").is_none(), "compact drops title");
}

#[test]
fn prs_fails_closed_when_repo_not_inferable() {
    let temp = tempdir().expect("tempdir");
    let not_a_repo = temp.path().join("not-a-repo");
    fs::create_dir_all(&not_a_repo).expect("create non-repo dir");
    let runtime = CliRuntime {
        cwd: not_a_repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };

    let result = run_rr(&["prs", "--robot"], &runtime);
    assert_eq!(result.exit_code, 3, "{}", result.stdout);
    let payload = parse_robot_payload(&result.stdout);
    assert_eq!(payload["outcome"], "blocked");
    assert_eq!(payload["data"]["reason_code"], "repo_context_missing");
    let repair_actions = payload["repair_actions"]
        .as_array()
        .expect("repair actions");
    assert!(
        repair_actions
            .iter()
            .any(|action| action.as_str().unwrap_or_default().contains("--repo")),
        "{repair_actions:?}"
    );
}

#[test]
fn prs_fails_closed_when_gh_is_missing() {
    let temp = tempdir().expect("tempdir");
    let (_guard, _log_path) = install_fake_gh(&temp, FakeGhMode::Missing);
    let runtime = CliRuntime {
        cwd: temp.path().to_path_buf(),
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };

    // Explicit --repo avoids needing git on the restricted PATH.
    let result = run_rr(&["prs", "--repo", "owner/repo", "--robot"], &runtime);
    assert_eq!(result.exit_code, 3, "{}", result.stdout);
    let payload = parse_robot_payload(&result.stdout);
    assert_eq!(payload["outcome"], "blocked");
    assert_eq!(payload["data"]["reason_code"], "gh_unavailable");
    let repair_actions = payload["repair_actions"]
        .as_array()
        .expect("repair actions");
    assert!(
        repair_actions
            .iter()
            .any(|action| action.as_str().unwrap_or_default().contains("gh")),
        "{repair_actions:?}"
    );
}

#[test]
fn prs_fails_closed_and_surfaces_gh_auth_error_truthfully() {
    let temp = tempdir().expect("tempdir");
    let (_guard, _log_path) = install_fake_gh(
        &temp,
        FakeGhMode::FailWithStderr("gh: To get started with GitHub CLI, please run: gh auth login"),
    );
    let repo = init_repo(&temp);
    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };

    let result = run_rr(&["prs", "--robot"], &runtime);
    assert_eq!(result.exit_code, 3, "{}", result.stdout);
    let payload = parse_robot_payload(&result.stdout);
    assert_eq!(payload["outcome"], "blocked");
    assert_eq!(payload["data"]["reason_code"], "gh_command_failed");
    let adapter_error = payload["data"]["adapter_error"]
        .as_str()
        .expect("adapter error string");
    assert!(
        adapter_error.contains("gh auth login"),
        "adapter error must surface gh stderr truthfully: {adapter_error}"
    );
    let repair_actions = payload["repair_actions"]
        .as_array()
        .expect("repair actions");
    assert!(
        repair_actions
            .iter()
            .any(|action| action.as_str().unwrap_or_default().contains("gh auth")),
        "{repair_actions:?}"
    );
}

#[test]
fn prs_is_read_only_and_does_not_mutate_session_state() {
    let temp = tempdir().expect("tempdir");
    let (_guard, log_path) = install_fake_gh(&temp, FakeGhMode::ListPayload(TWO_OPEN_PRS_PAYLOAD));
    let repo = init_repo(&temp);
    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };

    seed_session(
        &runtime,
        "session-queue-43",
        &sample_target("owner/repo", 43),
        "awaiting_user_input",
    );
    let before = {
        let store = RogerStore::open(&runtime.store_root).expect("open store");
        store
            .review_session("session-queue-43")
            .expect("read session")
            .expect("session present")
    };

    let result = run_rr(&["prs", "--robot"], &runtime);
    assert_eq!(result.exit_code, 0, "{}", result.stderr);

    let after = {
        let store = RogerStore::open(&runtime.store_root).expect("open store");
        store
            .review_session("session-queue-43")
            .expect("read session")
            .expect("session present")
    };
    assert_eq!(after.row_version, before.row_version);
    assert_eq!(after.attention_state, before.attention_state);
    assert_eq!(after.updated_at, before.updated_at);

    // Read-only on the GitHub side too: exactly one gh call, and it is pr list.
    let log_lines = read_log_lines(&log_path);
    assert_eq!(log_lines.len(), 1, "gh invocations: {log_lines:?}");
    assert!(log_lines[0].starts_with("pr list "), "{}", log_lines[0]);
}

#[test]
fn usage_and_robot_docs_advertise_rr_prs() {
    let temp = tempdir().expect("tempdir");
    let runtime = CliRuntime {
        cwd: temp.path().to_path_buf(),
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };

    let help = run_rr(&["--help"], &runtime);
    assert_eq!(help.exit_code, 0);
    assert!(
        help.stdout
            .contains("rr prs [--repo owner/repo] [--limit <n>] [--robot]"),
        "{}",
        help.stdout
    );

    let commands = run_rr(&["robot-docs", "commands", "--robot"], &runtime);
    assert_eq!(commands.exit_code, 0, "{}", commands.stderr);
    let commands_payload = parse_robot_payload(&commands.stdout);
    let command_items = commands_payload["data"]["items"]
        .as_array()
        .expect("commands items");
    let prs_entry = command_items
        .iter()
        .find(|item| item["command"] == "rr prs")
        .expect("rr prs entry in robot-docs commands");
    assert_eq!(prs_entry["required_formats"], serde_json::json!(["json"]));
    assert_eq!(
        prs_entry["optional_formats"],
        serde_json::json!(["compact"])
    );

    let schemas = run_rr(&["robot-docs", "schemas", "--robot"], &runtime);
    assert_eq!(schemas.exit_code, 0, "{}", schemas.stderr);
    let schemas_payload = parse_robot_payload(&schemas.stdout);
    let schema_items = schemas_payload["data"]["items"]
        .as_array()
        .expect("schema items");
    assert!(
        schema_items
            .iter()
            .any(|item| item["command"] == "rr prs" && item["schema_id"] == "rr.robot.prs.v1"),
        "rr prs schema missing from robot-docs schemas"
    );

    let workflows = run_rr(&["robot-docs", "workflows", "--robot"], &runtime);
    assert_eq!(workflows.exit_code, 0, "{}", workflows.stderr);
    let workflows_payload = parse_robot_payload(&workflows.stdout);
    let workflow_items = workflows_payload["data"]["items"]
        .as_array()
        .expect("workflow items");
    let review_queue = workflow_items
        .iter()
        .find(|item| item["name"] == "review_queue")
        .expect("review_queue workflow entry");
    assert_eq!(
        review_queue["steps"],
        serde_json::json!([
            "rr prs --robot",
            "rr review --pr <n> --provider <p> --robot"
        ])
    );
}

#[test]
fn prs_rejects_unsupported_flags() {
    let temp = tempdir().expect("tempdir");
    let runtime = CliRuntime {
        cwd: temp.path().to_path_buf(),
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };

    let with_pr = run_rr(&["prs", "--pr", "42"], &runtime);
    assert_eq!(with_pr.exit_code, 2);
    assert!(
        with_pr
            .stderr
            .contains("rr prs only supports --repo, --limit, and --robot"),
        "{}",
        with_pr.stderr
    );

    let with_session = run_rr(&["prs", "--session", "session-1"], &runtime);
    assert_eq!(with_session.exit_code, 2);
}
