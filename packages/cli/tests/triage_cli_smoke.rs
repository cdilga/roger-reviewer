#![cfg(unix)]

use roger_app_core::ReviewTarget;
use roger_cli::{CliRuntime, run};
use roger_storage::{CreateMaterializedFinding, CreateReviewRun, CreateReviewSession, RogerStore};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
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
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    run(&argv, runtime)
}

fn parse_robot_payload(stdout: &str) -> Value {
    serde_json::from_str(stdout).expect("robot payload json")
}

fn seed_session_with_new_finding(
    runtime: &CliRuntime,
    session_id: &str,
    run_id: &str,
    target: &ReviewTarget,
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
            attention_state: "awaiting_user_input",
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
    store
        .upsert_materialized_finding(CreateMaterializedFinding {
            id: "finding-1",
            session_id,
            review_run_id: run_id,
            stage: "deep_review",
            fingerprint: "fp:triage-one",
            title: "Triagable finding",
            normalized_summary: "triagable finding summary",
            severity: "high",
            confidence: "medium",
            triage_state: "new",
            outbound_state: "not_drafted",
        })
        .expect("seed finding one");
}

#[test]
fn triage_accept_then_draft_happy_path() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };
    seed_session_with_new_finding(
        &runtime,
        "session-1",
        "run-1",
        &sample_target("owner/repo", 42),
    );

    // Drafting a finding still in triage_state "new" must fail closed.
    let premature_draft = run_rr(
        &[
            "draft",
            "--session",
            "session-1",
            "--finding",
            "finding-1",
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(premature_draft.exit_code, 3, "{}", premature_draft.stderr);
    let premature_payload = parse_robot_payload(&premature_draft.stdout);
    assert_eq!(premature_payload["outcome"], "blocked");
    assert!(
        premature_payload["data"]["selection_issues"]
            .as_array()
            .expect("selection issues")
            .iter()
            .any(|issue| issue["reason_code"] == "triage_state_not_accepted"),
        "{}",
        premature_draft.stdout
    );

    let triage = run_rr(
        &[
            "triage",
            "--session",
            "session-1",
            "--finding",
            "finding-1",
            "--state",
            "accepted",
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(triage.exit_code, 0, "{}", triage.stderr);
    let triage_payload = parse_robot_payload(&triage.stdout);
    assert_eq!(triage_payload["outcome"], "complete");
    let items = triage_payload["data"]["items"]
        .as_array()
        .expect("triage items");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], "finding-1");
    assert_eq!(items[0]["triage_state"], "accepted");
    assert_eq!(items[0]["outbound_state"], "not_drafted");
    assert_eq!(items[0]["row_version"], Value::from(1));

    let store = RogerStore::open(&runtime.store_root).expect("reopen store");
    let record = store
        .materialized_finding("finding-1")
        .expect("finding lookup")
        .expect("finding exists");
    assert_eq!(record.triage_state, "accepted");
    assert_eq!(record.row_version, 1);

    let draft = run_rr(
        &[
            "draft",
            "--session",
            "session-1",
            "--finding",
            "finding-1",
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(draft.exit_code, 0, "{}", draft.stderr);
    let draft_payload = parse_robot_payload(&draft.stdout);
    assert_eq!(draft_payload["schema_id"], "rr.robot.draft.v1");
    assert_eq!(draft_payload["outcome"], "complete");
    assert_eq!(draft_payload["data"]["selection"]["count"], Value::from(1));
}

#[test]
fn triage_human_output_prints_one_line_per_finding() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };
    seed_session_with_new_finding(
        &runtime,
        "session-human",
        "run-human",
        &sample_target("owner/repo", 42),
    );

    let triage = run_rr(
        &[
            "triage",
            "--session",
            "session-human",
            "--finding",
            "finding-1",
            "--state",
            "needs_follow_up",
        ],
        &runtime,
    );
    assert_eq!(triage.exit_code, 0, "{}", triage.stderr);
    let finding_lines = triage
        .stdout
        .lines()
        .filter(|line| line.starts_with("finding-1 "))
        .collect::<Vec<_>>();
    assert_eq!(finding_lines.len(), 1, "{}", triage.stdout);
    assert!(
        finding_lines[0].contains("triage_state=needs_follow_up"),
        "{}",
        triage.stdout
    );
}

#[test]
fn triage_unknown_finding_id_fails_closed() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };
    seed_session_with_new_finding(
        &runtime,
        "session-unknown",
        "run-unknown",
        &sample_target("owner/repo", 42),
    );

    let triage = run_rr(
        &[
            "triage",
            "--session",
            "session-unknown",
            "--finding",
            "finding-1",
            "--finding",
            "finding-does-not-exist",
            "--state",
            "accepted",
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(triage.exit_code, 3, "{}", triage.stderr);
    let payload = parse_robot_payload(&triage.stdout);
    assert_eq!(payload["outcome"], "blocked");
    assert_eq!(payload["data"]["reason_code"], "unknown_finding_ids");
    assert_eq!(
        payload["data"]["unknown_finding_ids"],
        serde_json::json!(["finding-does-not-exist"])
    );

    // The known finding must not have been mutated: triage is all-or-nothing.
    let store = RogerStore::open(&runtime.store_root).expect("reopen store");
    let record = store
        .materialized_finding("finding-1")
        .expect("finding lookup")
        .expect("finding exists");
    assert_eq!(record.triage_state, "new");
}

#[test]
fn triage_invalid_state_value_fails_closed_with_exit_2() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };

    for state in ["new", "stale", "approved", "bogus"] {
        let triage = run_rr(
            &[
                "triage",
                "--session",
                "session-1",
                "--finding",
                "finding-1",
                "--state",
                state,
                "--robot",
            ],
            &runtime,
        );
        assert_eq!(triage.exit_code, 2, "state {state}: {}", triage.stderr);
        assert!(
            triage.stderr.contains("unsupported --state"),
            "state {state}: {}",
            triage.stderr
        );
    }

    // The rejection message must explain that new/stale are Roger-derived.
    let derived = run_rr(
        &[
            "triage",
            "--session",
            "session-1",
            "--finding",
            "finding-1",
            "--state",
            "new",
        ],
        &runtime,
    );
    assert_eq!(derived.exit_code, 2);
    assert!(
        derived.stderr.contains(
            "new and stale are Roger-derived triage states and cannot be set by the operator"
        ),
        "{}",
        derived.stderr
    );
}

#[test]
fn triage_robot_envelope_uses_triage_schema_id() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };
    seed_session_with_new_finding(
        &runtime,
        "session-schema",
        "run-schema",
        &sample_target("owner/repo", 42),
    );

    let triage = run_rr(
        &[
            "triage",
            "--session",
            "session-schema",
            "--finding",
            "finding-1",
            "--state",
            "ignored",
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(triage.exit_code, 0, "{}", triage.stderr);
    let payload = parse_robot_payload(&triage.stdout);
    assert_eq!(payload["schema_id"], "rr.robot.triage.v1");
    assert_eq!(payload["command"], "rr triage");
    assert_eq!(payload["outcome"], "complete");
    assert_eq!(payload["data"]["items"][0]["triage_state"], "ignored");
}
