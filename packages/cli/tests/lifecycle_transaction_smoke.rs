#![cfg(unix)]

use roger_cli::{CliRuntime, run};
use roger_storage::{
    CreateMaterializedFinding, CreateReviewRun, CreateReviewSession, LaunchAttemptState, RogerStore,
};
use rusqlite::{Connection, params};
use serde_json::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::{TempDir, tempdir};

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

fn init_repo(temp: &TempDir) -> PathBuf {
    init_repo_named(temp, "repo")
}

fn init_repo_named(temp: &TempDir, name: &str) -> PathBuf {
    let repo = temp.path().join(name);
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

fn write_stub_binary(reopen_fails: bool) -> (TempDir, PathBuf) {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("opencode-stub");
    let script = if reopen_fails {
        r#"#!/bin/sh
if [ "$1" = "--session" ]; then
  exit 1
fi
if [ "$1" = "export" ]; then
  echo "{}"
  exit 0
fi
exit 0
"#
    } else {
        r#"#!/bin/sh
if [ "$1" = "--session" ]; then
  exit 0
fi
if [ "$1" = "export" ]; then
  echo "{}"
  exit 0
fi
exit 0
"#
    };

    fs::write(&path, script).expect("write stub binary");
    let mut perms = fs::metadata(&path).expect("metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).expect("chmod stub binary");
    (dir, path)
}

fn latest_launch_attempt_id(store_root: &Path, action: &str, requested_session_id: &str) -> String {
    let conn = Connection::open(store_root.join("roger.db")).expect("open roger.db");
    conn.query_row(
        "SELECT id
         FROM launch_attempts
         WHERE action = ?1 AND requested_session_id = ?2
         ORDER BY created_at DESC, rowid DESC
         LIMIT 1",
        params![action, requested_session_id],
        |row| row.get(0),
    )
    .expect("latest launch attempt id")
}

fn count_launch_attempts(store_root: &Path, action: &str, requested_session_id: &str) -> i64 {
    let conn = Connection::open(store_root.join("roger.db")).expect("open roger.db");
    conn.query_row(
        "SELECT COUNT(*)
         FROM launch_attempts
         WHERE action = ?1 AND requested_session_id = ?2",
        params![action, requested_session_id],
        |row| row.get(0),
    )
    .expect("launch attempt count")
}

fn seed_stale_readback_session(runtime: &CliRuntime, session_id: &str, review_run_id: &str) {
    let store = RogerStore::open(&runtime.store_root).expect("open store");
    store
        .create_review_session(CreateReviewSession {
            id: session_id,
            review_target: &roger_app_core::ReviewTarget {
                repository: "owner/repo".to_owned(),
                pull_request_number: 42,
                base_ref: "main".to_owned(),
                head_ref: "feature/stale".to_owned(),
                base_commit: "aaa".to_owned(),
                head_commit: "ccc".to_owned(),
            },
            provider: "opencode",
            session_locator: None,
            resume_bundle_artifact_id: None,
            continuity_state: "findings:stale",
            attention_state: "refresh_recommended",
            launch_profile_id: Some("profile-open-pr"),
        })
        .expect("create stale readback session");
    store
        .create_review_run(CreateReviewRun {
            id: review_run_id,
            session_id,
            run_kind: "review",
            repo_snapshot: "owner/repo#42@head=bbb",
            continuity_quality: "usable",
            session_locator_artifact_id: None,
        })
        .expect("create stale readback run");
    store
        .upsert_materialized_finding(CreateMaterializedFinding {
            id: "finding-stale-1",
            session_id,
            review_run_id,
            stage: "deep_review",
            fingerprint: "fp-stale-1",
            title: "Persisted stale finding",
            normalized_summary: "Persisted stale finding",
            severity: "medium",
            confidence: "medium",
            triage_state: "needs_follow_up",
            outbound_state: "not_drafted",
        })
        .expect("create stale finding");
}

#[test]
fn resume_reseed_records_retry_safe_launch_attempt_and_updates_locator() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_ok_stub_dir, ok_bin) = write_stub_binary(false);
    let (_fail_stub_dir, fail_bin) = write_stub_binary(true);

    let stable_runtime = CliRuntime {
        cwd: repo.clone(),
        store_root: temp.path().join("roger-store"),
        opencode_bin: ok_bin.to_string_lossy().to_string(),
    };

    let review = run_rr(&["review", "--pr", "42", "--robot"], &stable_runtime);
    assert_eq!(review.exit_code, 0, "{}", review.stderr);
    let review_payload = parse_robot_payload(&review.stdout);
    let session_id = review_payload["data"]["session_id"]
        .as_str()
        .expect("session id")
        .to_owned();

    let store = RogerStore::open(&stable_runtime.store_root).expect("open store");
    let original_session = store
        .review_session(&session_id)
        .expect("read review session")
        .expect("session record");
    let original_locator = original_session
        .session_locator
        .as_ref()
        .expect("original locator")
        .session_id
        .clone();
    let original_binding_id = store
        .launch_bindings_for_session(&session_id)
        .expect("launch bindings")
        .into_iter()
        .next()
        .expect("launch binding")
        .id;
    drop(store);

    let stale_runtime = CliRuntime {
        cwd: repo,
        store_root: stable_runtime.store_root.clone(),
        opencode_bin: fail_bin.to_string_lossy().to_string(),
    };

    let resume = run_rr(&["resume", "--pr", "42"], &stale_runtime);
    assert_eq!(resume.exit_code, 5, "{}", resume.stderr);
    assert!(
        resume.stdout.contains("resume completed"),
        "unexpected resume stdout: {}",
        resume.stdout
    );

    let attempt_id =
        latest_launch_attempt_id(&stale_runtime.store_root, "resume_review", &session_id);
    let store = RogerStore::open(&stale_runtime.store_root).expect("reopen store");
    let attempt = store
        .launch_attempt(&attempt_id)
        .expect("read launch attempt")
        .expect("launch attempt");
    assert_eq!(attempt.state, LaunchAttemptState::VerifiedReseeded);
    assert_eq!(
        attempt.requested_session_id.as_deref(),
        Some(session_id.as_str())
    );
    assert_eq!(
        attempt.final_session_id.as_deref(),
        Some(session_id.as_str())
    );
    assert_eq!(
        attempt.launch_binding_id.as_deref(),
        Some(original_binding_id.as_str())
    );

    let session = store
        .review_session(&session_id)
        .expect("updated session lookup")
        .expect("updated session");
    let updated_locator = session
        .session_locator
        .as_ref()
        .expect("updated locator")
        .session_id
        .clone();
    assert_ne!(updated_locator, original_locator);
    assert!(updated_locator.starts_with("oc-reseed-"));
    assert!(session.continuity_state.starts_with("resume:"));
    assert_eq!(session.attention_state, "review_resumed");
    assert!(session.row_version > original_session.row_version);
    assert_eq!(
        attempt.provider_session_id.as_deref(),
        Some(updated_locator.as_str())
    );
    assert_eq!(
        attempt
            .verified_locator
            .as_ref()
            .expect("verified locator")
            .session_id,
        updated_locator
    );

    let run = store
        .latest_review_run(&session_id)
        .expect("latest review run")
        .expect("resume run");
    assert_eq!(run.run_kind, "resume");
}

#[test]
fn resume_blocks_cross_root_binding_reuse_even_when_repo_and_pr_match() {
    let temp = tempdir().expect("tempdir");
    let repo_a = init_repo_named(&temp, "repo-a");
    let repo_b = init_repo_named(&temp, "repo-b");
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let store_root = temp.path().join("roger-store");
    let review_runtime = CliRuntime {
        cwd: repo_a,
        store_root: store_root.clone(),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let review = run_rr(&["review", "--pr", "42", "--robot"], &review_runtime);
    assert_eq!(review.exit_code, 0, "{}", review.stderr);
    let review_payload = parse_robot_payload(&review.stdout);
    let session_id = review_payload["data"]["session_id"]
        .as_str()
        .expect("session id")
        .to_owned();

    let resume_runtime = CliRuntime {
        cwd: repo_b,
        store_root: store_root.clone(),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let resume = run_rr(&["resume", "--pr", "42", "--robot"], &resume_runtime);
    assert_eq!(resume.exit_code, 3, "{}", resume.stderr);
    let payload = parse_robot_payload(&resume.stdout);
    assert_eq!(payload["outcome"], "blocked");
    assert!(
        payload["data"]["reason"]
            .as_str()
            .is_some_and(|text| text.contains("launch binding is stale"))
    );
    assert!(
        payload["data"]["reason"]
            .as_str()
            .is_some_and(|text| text.contains("worktree root mismatch"))
    );
    assert_eq!(
        payload["data"]["candidates"][0]["session_id"],
        Value::String(session_id.clone())
    );
    assert_eq!(
        count_launch_attempts(&store_root, "resume_review", &session_id),
        0
    );
}

#[test]
fn return_records_verified_launch_attempt_and_single_committed_update() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let review = run_rr(&["review", "--pr", "42", "--robot"], &runtime);
    assert_eq!(review.exit_code, 0, "{}", review.stderr);
    let review_payload = parse_robot_payload(&review.stdout);
    let session_id = review_payload["data"]["session_id"]
        .as_str()
        .expect("session id")
        .to_owned();

    let ret = run_rr(&["return", "--pr", "42", "--robot"], &runtime);
    assert_eq!(ret.exit_code, 0, "{}", ret.stderr);
    let payload = parse_robot_payload(&ret.stdout);
    assert_eq!(payload["data"]["return_path"], "rebound_existing_session");
    let attempt_id = payload["data"]["launch_attempt_id"]
        .as_str()
        .expect("launch attempt id")
        .to_owned();

    let store = RogerStore::open(&runtime.store_root).expect("open store");
    let attempt = store
        .launch_attempt(&attempt_id)
        .expect("read launch attempt")
        .expect("launch attempt");
    assert_eq!(attempt.state, LaunchAttemptState::VerifiedReopened);
    assert_eq!(
        attempt.requested_session_id.as_deref(),
        Some(session_id.as_str())
    );
    assert_eq!(
        attempt.final_session_id.as_deref(),
        Some(session_id.as_str())
    );

    let session = store
        .review_session(&session_id)
        .expect("updated session lookup")
        .expect("updated session");
    assert_eq!(session.attention_state, "returned_to_roger");
    assert_eq!(session.continuity_state, "return:usable");
    assert_eq!(
        attempt.provider_session_id.as_deref(),
        Some(
            session
                .session_locator
                .as_ref()
                .expect("session locator")
                .session_id
                .as_str()
        )
    );
    assert_eq!(
        attempt
            .verified_locator
            .as_ref()
            .expect("verified locator")
            .session_id,
        session
            .session_locator
            .as_ref()
            .expect("session locator")
            .session_id
    );

    let run = store
        .latest_review_run(&session_id)
        .expect("latest review run")
        .expect("return run");
    assert_eq!(run.run_kind, "return");
}

#[test]
fn stale_readback_surfaces_report_persisted_state_with_reentry_guidance() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    seed_stale_readback_session(&runtime, "session-stale", "run-stale");

    let status = run_rr(
        &["status", "--session", "session-stale", "--robot"],
        &runtime,
    );
    assert_eq!(status.exit_code, 0, "{}", status.stderr);
    assert!(
        !status.stdout.contains("automatic_background"),
        "status should not claim automatic background reconciliation: {}",
        status.stdout
    );
    let status_payload = parse_robot_payload(&status.stdout);
    assert_eq!(
        status_payload["data"]["reconciliation"]["mode"],
        "persisted_readback"
    );
    assert_eq!(
        status_payload["data"]["reconciliation"]["manual_refresh_supported"],
        serde_json::json!(false)
    );
    assert_eq!(
        status_payload["data"]["reconciliation"]["stale_target_detected"],
        serde_json::json!(true)
    );
    assert_eq!(
        status_payload["data"]["reconciliation"]["repair_required"],
        serde_json::json!(true)
    );
    assert_eq!(
        status_payload["data"]["reconciliation"]["recommended_reentry_command"],
        "rr resume --session session-stale"
    );
    assert_eq!(
        status_payload["data"]["reconciliation"]["recommended_fresh_pass_command"],
        "rr review --repo owner/repo --pr 42"
    );
    let status_warnings = status_payload["warnings"]
        .as_array()
        .expect("status warnings");
    assert!(status_warnings.iter().any(|value| {
        value
            .as_str()
            .is_some_and(|text| text.contains("last persisted review state"))
    }));
    let status_repair_actions = status_payload["repair_actions"]
        .as_array()
        .expect("status repair actions");
    assert!(status_repair_actions.iter().any(|value| {
        value
            .as_str()
            .is_some_and(|text| text.contains("rr resume --session session-stale"))
    }));

    let findings = run_rr(
        &["findings", "--session", "session-stale", "--robot"],
        &runtime,
    );
    assert_eq!(findings.exit_code, 0, "{}", findings.stderr);
    assert!(
        !findings.stdout.contains("automatic_background"),
        "findings should not claim automatic background reconciliation: {}",
        findings.stdout
    );
    let findings_payload = parse_robot_payload(&findings.stdout);
    assert_eq!(
        findings_payload["data"]["reconciliation"]["mode"],
        "persisted_readback"
    );
    assert_eq!(findings_payload["data"]["count"], 1);
    assert_eq!(
        findings_payload["data"]["items"][0]["finding_id"],
        "finding-stale-1"
    );
    let findings_repair_actions = findings_payload["repair_actions"]
        .as_array()
        .expect("findings repair actions");
    assert!(findings_repair_actions.iter().any(|value| {
        value
            .as_str()
            .is_some_and(|text| text.contains("rr review --repo owner/repo --pr 42"))
    }));

    let sessions = run_rr(
        &["sessions", "--attention", "refresh_recommended", "--robot"],
        &runtime,
    );
    assert_eq!(sessions.exit_code, 0, "{}", sessions.stderr);
    assert!(
        !sessions.stdout.contains("automatic_background"),
        "sessions should not claim automatic background reconciliation: {}",
        sessions.stdout
    );
    let sessions_payload = parse_robot_payload(&sessions.stdout);
    assert_eq!(sessions_payload["data"]["count"], 1);
    assert_eq!(
        sessions_payload["data"]["items"][0]["follow_on"]["reconciliation_mode"],
        "reentry_required"
    );
    assert_eq!(
        sessions_payload["data"]["items"][0]["follow_on"]["manual_refresh_supported"],
        serde_json::json!(false)
    );
    assert_eq!(
        sessions_payload["data"]["items"][0]["follow_on"]["stale_target_detected"],
        serde_json::json!(true)
    );
}

#[test]
fn robot_docs_reconciliation_contract_reports_persisted_readback_truthfully() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let guide = run_rr(&["robot-docs", "guide", "--robot"], &runtime);
    assert_eq!(guide.exit_code, 0, "{}", guide.stderr);
    assert!(
        !guide.stdout.contains("automatic_background"),
        "robot-docs guide should not advertise automatic background reconciliation: {}",
        guide.stdout
    );
    let guide_payload = parse_robot_payload(&guide.stdout);
    let guide_items = guide_payload["data"]["items"]
        .as_array()
        .expect("guide items");
    let reconciliation_contract = guide_items
        .iter()
        .find(|item| item["kind"] == "reconciliation_contract")
        .expect("reconciliation contract item");
    assert_eq!(reconciliation_contract["mode"], "persisted_readback");
    assert_eq!(
        reconciliation_contract["manual_refresh_supported"],
        serde_json::json!(false)
    );
    assert!(
        reconciliation_contract["summary"]
            .as_str()
            .is_some_and(|text| text.contains("no standalone refresh command"))
    );

    let workflows = run_rr(&["robot-docs", "workflows", "--robot"], &runtime);
    assert_eq!(workflows.exit_code, 0, "{}", workflows.stderr);
    assert!(
        !workflows.stdout.contains("automatic_background"),
        "robot-docs workflows should not advertise automatic background reconciliation: {}",
        workflows.stdout
    );
    let workflows_payload = parse_robot_payload(&workflows.stdout);
    let workflow_items = workflows_payload["data"]["items"]
        .as_array()
        .expect("workflow items");
    let resume_loop = workflow_items
        .iter()
        .find(|item| item["name"] == "resume_loop")
        .expect("resume_loop workflow");
    assert!(
        resume_loop["notes"]
            .as_str()
            .is_some_and(|text| text.contains("There is no standalone refresh action"))
    );
}
