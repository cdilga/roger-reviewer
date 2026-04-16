#![cfg(unix)]

use roger_app_core::{PostedActionStatus, ReviewTarget};
use roger_cli::{CliRuntime, run};
use roger_storage::{
    CreateMaterializedFinding, CreateReviewRun, CreateReviewSession, RogerStore, StorageLayout,
};
use rusqlite::{Connection, params};
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

fn seed_session_with_findings(
    runtime: &CliRuntime,
    session_id: &str,
    run_id: &str,
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

    for (finding_id, title, summary, severity, confidence) in [
        (
            "finding-1",
            "First outbound finding",
            "first draftable finding summary",
            "high",
            "medium",
        ),
        (
            "finding-2",
            "Second outbound finding",
            "second draftable finding summary",
            "medium",
            "high",
        ),
    ] {
        store
            .upsert_materialized_finding(CreateMaterializedFinding {
                id: finding_id,
                session_id,
                review_run_id: run_id,
                stage: "deep_review",
                fingerprint: &format!("fp:{finding_id}"),
                title,
                normalized_summary: summary,
                severity,
                confidence,
                triage_state: "accepted",
                outbound_state: "not_drafted",
            })
            .expect("seed materialized finding");
    }
}

fn draft_batch(runtime: &CliRuntime, session_id: &str) -> Value {
    let draft = run_rr(
        &[
            "draft",
            "--session",
            session_id,
            "--finding",
            "finding-1",
            "--finding",
            "finding-2",
            "--robot",
        ],
        runtime,
    );
    assert_eq!(draft.exit_code, 0, "{}", draft.stderr);
    parse_robot_payload(&draft.stdout)
}

fn approve_batch(runtime: &CliRuntime, session_id: &str, batch_id: &str) -> Value {
    let approve = run_rr(
        &[
            "approve",
            "--session",
            session_id,
            "--batch",
            batch_id,
            "--robot",
        ],
        runtime,
    );
    assert_eq!(approve.exit_code, 0, "{}", approve.stderr);
    parse_robot_payload(&approve.stdout)
}

fn seed_approved_batch(runtime: &CliRuntime, session_id: &str) -> (String, Value) {
    let draft_payload = draft_batch(runtime, session_id);
    let batch_id = draft_payload["data"]["draft_batch"]["id"]
        .as_str()
        .expect("draft batch id")
        .to_owned();
    let approve_payload = approve_batch(runtime, session_id, &batch_id);
    (batch_id, approve_payload)
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
    old_counter: Option<OsString>,
    old_fail_on: Option<OsString>,
}

impl Drop for FakeGhEnvGuard {
    fn drop(&mut self) {
        restore_env("PATH", self.old_path.clone());
        restore_env("RR_TEST_GH_LOG", self.old_log.clone());
        restore_env("RR_TEST_GH_COUNTER", self.old_counter.clone());
        restore_env("RR_TEST_GH_FAIL_ON_SUBSTRING", self.old_fail_on.clone());
    }
}

fn install_fake_gh(temp: &TempDir, fail_on_substring: Option<&str>) -> (FakeGhEnvGuard, PathBuf) {
    let lock = ENV_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());

    let bin_dir = temp.path().join("fake-gh-bin");
    fs::create_dir_all(&bin_dir).expect("create fake gh dir");
    let script_path = bin_dir.join("gh");
    let script = r#"#!/bin/sh
set -eu
log_file="${RR_TEST_GH_LOG:?}"
counter_file="${RR_TEST_GH_COUNTER:?}"
fail_on="${RR_TEST_GH_FAIL_ON_SUBSTRING:-}"
printf '%s\n' "$*" >> "$log_file"
body=""
for arg in "$@"; do
  case "$arg" in
    body=*) body="${arg#body=}" ;;
  esac
done
if [ -n "$fail_on" ] && printf '%s' "$body" | grep -F -- "$fail_on" >/dev/null 2>&1; then
  echo "503 Service Unavailable" >&2
  exit 1
fi
count=0
if [ -f "$counter_file" ]; then
  count="$(cat "$counter_file")"
fi
count=$((count + 1))
printf '%s' "$count" > "$counter_file"
printf '{"html_url":"https://github.com/owner/repo/pull/42#issuecomment-%s"}\n' "$count"
"#;
    fs::write(&script_path, script).expect("write fake gh script");
    let mut permissions = fs::metadata(&script_path)
        .expect("fake gh metadata")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&script_path, permissions).expect("chmod fake gh");

    let log_path = temp.path().join("fake-gh.log");
    let counter_path = temp.path().join("fake-gh.counter");

    let old_path = std::env::var_os("PATH");
    let old_log = std::env::var_os("RR_TEST_GH_LOG");
    let old_counter = std::env::var_os("RR_TEST_GH_COUNTER");
    let old_fail_on = std::env::var_os("RR_TEST_GH_FAIL_ON_SUBSTRING");

    let joined_path = match old_path.as_ref() {
        Some(existing) => {
            let mut value = OsString::from(bin_dir.as_os_str());
            value.push(":");
            value.push(existing);
            value
        }
        None => OsString::from(bin_dir.as_os_str()),
    };

    // SAFETY: test-only scoped environment mutation guarded by a global mutex.
    unsafe {
        std::env::set_var("PATH", joined_path);
        std::env::set_var("RR_TEST_GH_LOG", &log_path);
        std::env::set_var("RR_TEST_GH_COUNTER", &counter_path);
        if let Some(fail_on_substring) = fail_on_substring {
            std::env::set_var("RR_TEST_GH_FAIL_ON_SUBSTRING", fail_on_substring);
        } else {
            std::env::remove_var("RR_TEST_GH_FAIL_ON_SUBSTRING");
        }
    }

    (
        FakeGhEnvGuard {
            _lock: lock,
            old_path,
            old_log,
            old_counter,
            old_fail_on,
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

#[test]
fn robot_docs_advertise_rr_post_surface() {
    let temp = tempdir().expect("tempdir");
    let runtime = CliRuntime {
        cwd: PathBuf::from("."),
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };

    let commands = run_rr(&["robot-docs", "commands", "--robot"], &runtime);
    assert_eq!(commands.exit_code, 0, "{}", commands.stderr);
    let commands_payload = parse_robot_payload(&commands.stdout);
    let command_items = commands_payload["data"]["items"]
        .as_array()
        .expect("command items");
    assert!(
        command_items
            .iter()
            .any(|item| item["command"] == "rr post")
    );

    let schemas = run_rr(&["robot-docs", "schemas", "--robot"], &runtime);
    assert_eq!(schemas.exit_code, 0, "{}", schemas.stderr);
    let schemas_payload = parse_robot_payload(&schemas.stdout);
    let schema_items = schemas_payload["data"]["items"]
        .as_array()
        .expect("schema items");
    assert!(
        schema_items.iter().any(|item| {
            item["command"] == "rr post" && item["schema_id"] == "rr.robot.post.v1"
        })
    );

    let workflows = run_rr(&["robot-docs", "workflows", "--robot"], &runtime);
    assert_eq!(workflows.exit_code, 0, "{}", workflows.stderr);
    let workflows_payload = parse_robot_payload(&workflows.stdout);
    let workflow_items = workflows_payload["data"]["items"]
        .as_array()
        .expect("workflow items");
    let post_workflow = workflow_items
        .iter()
        .find(|item| item["name"] == "local_outbound_post")
        .expect("local outbound post workflow");
    let steps = post_workflow["steps"].as_array().expect("workflow steps");
    assert!(
        steps
            .iter()
            .any(|step| { step == "rr post --session <id> --batch <draft-batch-id> --robot" })
    );
}

#[test]
fn post_executes_exact_approved_batch_and_records_posted_action() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };
    seed_session_with_findings(
        &runtime,
        "session-post-happy",
        "run-post-happy",
        &sample_target("owner/repo", 42),
        "awaiting_user_input",
    );

    let (batch_id, approve_payload) = seed_approved_batch(&runtime, "session-post-happy");
    assert_eq!(
        approve_payload["data"]["queryable_surfaces"]["post_command"],
        Value::String(format!(
            "rr post --session session-post-happy --batch {batch_id}"
        ))
    );

    let (_guard, log_path) = install_fake_gh(&temp, None);

    let post = run_rr(
        &[
            "post",
            "--session",
            "session-post-happy",
            "--batch",
            &batch_id,
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(post.exit_code, 0, "{}", post.stderr);
    let payload = parse_robot_payload(&post.stdout);
    assert_eq!(payload["schema_id"], "rr.robot.post.v1");
    assert_eq!(payload["outcome"], "complete");
    assert_eq!(payload["data"]["draft_batch"]["id"], batch_id);
    assert_eq!(payload["data"]["posting_result"]["outcome"], "posted");
    assert_eq!(
        payload["data"]["mutation_guard"]["github_posture"],
        "posted"
    );
    assert_eq!(
        payload["data"]["mutation_guard"]["posted"],
        Value::Bool(true)
    );
    let item_results = payload["data"]["item_results"]
        .as_array()
        .expect("item results");
    assert_eq!(item_results.len(), 2);
    assert!(item_results.iter().all(|item| item["status"] == "posted"));
    assert_eq!(payload["data"]["retry_draft_ids"], Value::Array(Vec::new()));
    assert_eq!(payload["data"]["posted_action"]["status"], "Succeeded");

    let store = RogerStore::open(&runtime.store_root).expect("open store");
    let actions = store
        .posted_actions_for_batch(&batch_id)
        .expect("posted actions lookup");
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].status, PostedActionStatus::Succeeded);
    assert_eq!(
        actions[0].posted_payload_digest,
        payload["data"]["draft_batch"]["payload_digest"]
            .as_str()
            .expect("posted payload digest")
    );

    let status = run_rr(
        &["status", "--session", "session-post-happy", "--robot"],
        &runtime,
    );
    assert_eq!(status.exit_code, 0, "{}", status.stderr);
    let status_payload = parse_robot_payload(&status.stdout);
    assert_eq!(
        status_payload["data"]["outbound"]["state_counts"]["approved"],
        0
    );
    assert_eq!(
        status_payload["data"]["outbound"]["state_counts"]["posted"],
        2
    );

    let findings = run_rr(
        &["findings", "--session", "session-post-happy", "--robot"],
        &runtime,
    );
    assert_eq!(findings.exit_code, 0, "{}", findings.stderr);
    let findings_payload = parse_robot_payload(&findings.stdout);
    let finding_items = findings_payload["data"]["items"]
        .as_array()
        .expect("finding items");
    assert!(
        finding_items
            .iter()
            .all(|item| item["outbound_state"] == "posted")
    );

    let log_lines = read_log_lines(&log_path);
    assert_eq!(
        log_lines
            .iter()
            .filter(|line| line.contains("repos/owner/repo/issues/42/comments"))
            .count(),
        2
    );
}

#[test]
fn post_blocks_when_stored_approval_payload_digest_drifted() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };
    seed_session_with_findings(
        &runtime,
        "session-post-stale",
        "run-post-stale",
        &sample_target("owner/repo", 42),
        "awaiting_user_input",
    );

    let (batch_id, _) = seed_approved_batch(&runtime, "session-post-stale");
    let layout = StorageLayout::under(&runtime.store_root);
    let conn = Connection::open(&layout.db_path).expect("open sqlite db");
    conn.execute(
        "UPDATE outbound_batch_approval_tokens
         SET payload_digest = ?1
         WHERE draft_batch_id = ?2",
        params!["sha256:tampered-approval-digest", &batch_id],
    )
    .expect("tamper approval payload digest");

    let (_guard, log_path) = install_fake_gh(&temp, None);

    let post = run_rr(
        &[
            "post",
            "--session",
            "session-post-stale",
            "--batch",
            &batch_id,
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(post.exit_code, 3, "{}", post.stderr);
    let payload = parse_robot_payload(&post.stdout);
    assert_eq!(payload["schema_id"], "rr.robot.post.v1");
    assert_eq!(payload["outcome"], "blocked");
    assert_eq!(
        payload["data"]["reason_code"],
        Value::String("approval_payload_digest_mismatch".to_owned())
    );

    let store = RogerStore::open(&runtime.store_root).expect("open store");
    assert!(
        store
            .posted_actions_for_batch(&batch_id)
            .expect("posted actions lookup")
            .is_empty()
    );
    assert!(read_log_lines(&log_path).is_empty());
}

#[test]
fn post_blocks_when_batch_target_drifted() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };
    seed_session_with_findings(
        &runtime,
        "session-post-target-drift",
        "run-post-target-drift",
        &sample_target("owner/repo", 42),
        "awaiting_user_input",
    );

    let (batch_id, _) = seed_approved_batch(&runtime, "session-post-target-drift");
    let layout = StorageLayout::under(&runtime.store_root);
    let conn = Connection::open(&layout.db_path).expect("open sqlite db");
    conn.execute(
        "UPDATE outbound_draft_batches
         SET remote_review_target_id = ?1
         WHERE id = ?2",
        params!["pr-99", &batch_id],
    )
    .expect("tamper batch target");

    let (_guard, log_path) = install_fake_gh(&temp, None);

    let post = run_rr(
        &[
            "post",
            "--session",
            "session-post-target-drift",
            "--batch",
            &batch_id,
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(post.exit_code, 3, "{}", post.stderr);
    let payload = parse_robot_payload(&post.stdout);
    assert_eq!(payload["schema_id"], "rr.robot.post.v1");
    assert_eq!(payload["outcome"], "blocked");
    assert_eq!(
        payload["data"]["reason_code"],
        Value::String("approval_invalidated:target_drift".to_owned())
    );
    assert!(read_log_lines(&log_path).is_empty());
}

#[test]
fn post_surfaces_partial_failure_and_retry_candidates() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };
    seed_session_with_findings(
        &runtime,
        "session-post-partial",
        "run-post-partial",
        &sample_target("owner/repo", 42),
        "awaiting_user_input",
    );

    let (batch_id, _) = seed_approved_batch(&runtime, "session-post-partial");
    let (_guard, log_path) = install_fake_gh(&temp, Some("Second outbound finding"));

    let post = run_rr(
        &[
            "post",
            "--session",
            "session-post-partial",
            "--batch",
            &batch_id,
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(post.exit_code, 5, "{}", post.stderr);
    let payload = parse_robot_payload(&post.stdout);
    assert_eq!(payload["schema_id"], "rr.robot.post.v1");
    assert_eq!(payload["outcome"], "degraded");
    assert_eq!(payload["data"]["posting_result"]["outcome"], "partial");
    assert_eq!(
        payload["data"]["posted_action"]["status"],
        Value::String("Partial".to_owned())
    );
    assert_eq!(
        payload["data"]["mutation_guard"]["github_posture"],
        Value::String("partial_failure".to_owned())
    );
    let item_results = payload["data"]["item_results"]
        .as_array()
        .expect("item results");
    assert_eq!(item_results.len(), 2);
    assert!(item_results.iter().any(|item| item["status"] == "posted"));
    let failed_item = item_results
        .iter()
        .find(|item| item["status"] == "failed")
        .expect("failed item result");
    assert_eq!(
        failed_item["failure_code"],
        Value::String("retryable:service_unavailable".to_owned())
    );
    let retry_draft_ids = payload["data"]["retry_draft_ids"]
        .as_array()
        .expect("retry draft ids");
    assert_eq!(retry_draft_ids.len(), 1);

    let store = RogerStore::open(&runtime.store_root).expect("open store");
    let actions = store
        .posted_actions_for_batch(&batch_id)
        .expect("posted actions lookup");
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].status, PostedActionStatus::Partial);

    let status = run_rr(
        &["status", "--session", "session-post-partial", "--robot"],
        &runtime,
    );
    assert_eq!(status.exit_code, 0, "{}", status.stderr);
    let status_payload = parse_robot_payload(&status.stdout);
    assert_eq!(
        status_payload["data"]["outbound"]["state_counts"]["approved"],
        0
    );
    assert_eq!(
        status_payload["data"]["outbound"]["state_counts"]["posted"],
        0
    );
    assert_eq!(
        status_payload["data"]["outbound"]["state_counts"]["failed"],
        2
    );

    let log_lines = read_log_lines(&log_path);
    assert_eq!(
        log_lines
            .iter()
            .filter(|line| line.contains("repos/owner/repo/issues/42/comments"))
            .count(),
        2
    );
}
