#![cfg(unix)]

//! Integration coverage for `rr send edit` over local outbound draft revisions.
//!
//! `rr send edit` is a local, human-only action (no `--robot` envelope), so the
//! assertions read the store directly to prove the revision lineage, the
//! approval-revocation fail-closed behavior, and the posted-batch rejection.
//! The store seeding mirrors the draft/approve smoke tests.

use roger_app_core::{ApprovalState, ReviewTarget};
use roger_cli::{CliRuntime, run};
use roger_storage::{
    CreateMaterializedFinding, CreateReviewRun, CreateReviewSession,
    OUTBOUND_APPROVAL_REVOKED_REASON_DRAFT_REVISED, RogerStore,
};
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

    for (finding_id, title, summary) in [
        ("finding-1", "First outbound finding", "first summary"),
        ("finding-2", "Second outbound finding", "second summary"),
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
                severity: "high",
                confidence: "medium",
                triage_state: "accepted",
                outbound_state: "not_drafted",
            })
            .expect("seed materialized finding");
    }
}

/// Draft a batch through the CLI robot surface and return its batch id.
fn draft_batch(runtime: &CliRuntime, session_id: &str) -> String {
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
    parse_robot_payload(&draft.stdout)["data"]["draft_batch"]["id"]
        .as_str()
        .expect("draft batch id")
        .to_owned()
}

fn first_draft_id(runtime: &CliRuntime, batch_id: &str) -> (String, String) {
    let store = RogerStore::open(&runtime.store_root).expect("open store");
    let mut drafts = store
        .outbound_draft_items_for_batch(batch_id)
        .expect("draft items lookup");
    drafts.sort_by(|a, b| a.id.cmp(&b.id));
    let draft = drafts.into_iter().next().expect("at least one draft item");
    (draft.id, draft.body)
}

#[test]
fn send_edit_writes_revision_and_reports_next_step() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };
    seed_session_with_findings(
        &runtime,
        "session-1",
        "run-1",
        &sample_target("owner/repo", 42),
    );

    let batch_id = draft_batch(&runtime, "session-1");
    let (draft_id, original_body) = first_draft_id(&runtime, &batch_id);

    let body_path = temp.path().join("new-body.md");
    fs::write(&body_path, "Operator-revised outbound body.\n").expect("write body file");

    let edit = run_rr(
        &[
            "send",
            "edit",
            "--draft",
            &draft_id,
            "--body-file",
            body_path.to_str().expect("body path utf8"),
        ],
        &runtime,
    );
    assert_eq!(edit.exit_code, 0, "{}", edit.stderr);
    assert!(
        edit.stdout.contains("revision 1"),
        "human output must name the revision index: {}",
        edit.stdout
    );
    // No prior approval existed, so nothing should have been revoked; the next
    // step is a plain approve.
    let combined = format!("{}{}", edit.stdout, edit.stderr);
    assert!(
        combined.contains(&format!("rr send approve --batch {batch_id}")),
        "next-step guidance must name the approve command: {combined}"
    );
    assert!(
        !combined.contains("revoked the batch approval"),
        "no approval existed, so nothing should be revoked: {combined}"
    );
    assert!(
        combined.contains("\"approval_revoked\": false"),
        "data must record that no approval was revoked: {combined}"
    );

    // The store now carries the revision lineage and the current body.
    let store = RogerStore::open(&runtime.store_root).expect("reopen store");
    let current = store
        .current_outbound_draft_body(&draft_id)
        .expect("current body lookup")
        .expect("current body present");
    assert_eq!(current, "Operator-revised outbound body.\n");
    assert_ne!(current, original_body);

    let revisions = store
        .outbound_draft_revisions(&draft_id)
        .expect("revision lineage");
    // Revision 0 preserves the original generated body; revision 1 is the edit.
    assert_eq!(revisions.len(), 2);
    assert_eq!(revisions[0].revision_index, 0);
    assert_eq!(revisions[0].body, original_body);
    assert_eq!(revisions[1].revision_index, 1);
    assert_eq!(revisions[1].body, "Operator-revised outbound body.\n");
}

#[test]
fn send_edit_is_a_noop_when_body_is_unchanged() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };
    seed_session_with_findings(
        &runtime,
        "session-noop",
        "run-noop",
        &sample_target("owner/repo", 42),
    );

    let batch_id = draft_batch(&runtime, "session-noop");
    let (draft_id, original_body) = first_draft_id(&runtime, &batch_id);

    let body_path = temp.path().join("same-body.md");
    fs::write(&body_path, &original_body).expect("write identical body file");

    let edit = run_rr(
        &[
            "send",
            "edit",
            "--draft",
            &draft_id,
            "--body-file",
            body_path.to_str().expect("body path utf8"),
        ],
        &runtime,
    );
    assert_eq!(edit.exit_code, 0, "{}", edit.stderr);
    assert!(
        edit.stdout.contains("no change"),
        "unchanged body must be a clear no-op: {}",
        edit.stdout
    );

    let store = RogerStore::open(&runtime.store_root).expect("reopen store");
    // No revision written for a no-op edit.
    assert!(
        store
            .outbound_draft_revisions(&draft_id)
            .expect("revision lineage")
            .is_empty(),
        "an unchanged body must not append a revision"
    );
}

#[test]
fn send_edit_rejects_empty_body() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };
    seed_session_with_findings(
        &runtime,
        "session-empty",
        "run-empty",
        &sample_target("owner/repo", 42),
    );

    let batch_id = draft_batch(&runtime, "session-empty");
    let (draft_id, _) = first_draft_id(&runtime, &batch_id);

    let body_path = temp.path().join("empty-body.md");
    fs::write(&body_path, "   \n\t\n").expect("write blank body file");

    let edit = run_rr(
        &[
            "send",
            "edit",
            "--draft",
            &draft_id,
            "--body-file",
            body_path.to_str().expect("body path utf8"),
        ],
        &runtime,
    );
    assert_eq!(edit.exit_code, 3, "{}", edit.stderr);
    assert!(
        format!("{}{}", edit.stdout, edit.stderr).contains("empty"),
        "empty body must be rejected with a clear message: {} {}",
        edit.stdout,
        edit.stderr
    );
}

#[test]
fn send_edit_revokes_live_approval_and_forces_reapproval() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };
    seed_session_with_findings(
        &runtime,
        "session-approved",
        "run-approved",
        &sample_target("owner/repo", 42),
    );

    let batch_id = draft_batch(&runtime, "session-approved");
    let (draft_id, _) = first_draft_id(&runtime, &batch_id);

    let approve = run_rr(
        &[
            "approve",
            "--session",
            "session-approved",
            "--batch",
            &batch_id,
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(approve.exit_code, 0, "{}", approve.stderr);

    // Pre-condition: the batch holds a live (non-revoked) approval token.
    {
        let store = RogerStore::open(&runtime.store_root).expect("open store");
        let approval = store
            .approval_token_for_batch(&batch_id)
            .expect("approval lookup")
            .expect("approval present");
        assert!(approval.revoked_at.is_none(), "approval must start live");
    }

    let body_path = temp.path().join("revised-after-approval.md");
    fs::write(&body_path, "Revised after approval.\n").expect("write body file");

    let edit = run_rr(
        &[
            "send",
            "edit",
            "--draft",
            &draft_id,
            "--body-file",
            body_path.to_str().expect("body path utf8"),
        ],
        &runtime,
    );
    assert_eq!(edit.exit_code, 0, "{}", edit.stderr);
    let combined = format!("{}{}", edit.stdout, edit.stderr);
    assert!(
        combined.contains("revoked"),
        "editing an approved batch must report the revocation: {combined}"
    );
    assert!(
        combined.contains(&format!("rr send approve --batch {batch_id}")),
        "guidance must point at re-approval: {combined}"
    );

    // Fail-closed: the approval token is revoked with the typed reason and the
    // batch fell back to Drafted so the revised body cannot be posted.
    let store = RogerStore::open(&runtime.store_root).expect("reopen store");
    let approval = store
        .approval_token_for_batch(&batch_id)
        .expect("approval lookup")
        .expect("approval present");
    assert!(
        approval.revoked_at.is_some(),
        "editing an approved batch must revoke the approval"
    );
    assert_eq!(
        store
            .outbound_approval_revocation_reason(&batch_id)
            .expect("revocation reason lookup")
            .as_deref(),
        Some(OUTBOUND_APPROVAL_REVOKED_REASON_DRAFT_REVISED)
    );
    let batch = store
        .outbound_draft_batch(&batch_id)
        .expect("batch lookup")
        .expect("batch present");
    assert!(
        matches!(batch.approval_state, ApprovalState::Drafted),
        "revised batch must fall back to Drafted, got {:?}",
        batch.approval_state
    );
    drop(store);

    // The documented recovery path must actually work: re-running rr send
    // approve after a revision-revocation reissues the token bound to the
    // revised payload instead of blocking on the revoked one (P3 live-proof
    // defect: the batch used to be permanently wedged here).
    let reapprove = run_rr(
        &[
            "send",
            "approve",
            "--session",
            "session-approved",
            "--batch",
            &batch_id,
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(
        reapprove.exit_code, 0,
        "re-approval after revision must succeed: {}{}",
        reapprove.stdout, reapprove.stderr
    );
    let store = RogerStore::open(&runtime.store_root).expect("reopen store");
    let reissued = store
        .approval_token_for_batch(&batch_id)
        .expect("approval lookup")
        .expect("approval present");
    assert!(
        reissued.revoked_at.is_none(),
        "re-approval must re-arm the token"
    );
    let rebound_batch = store
        .outbound_draft_batch(&batch_id)
        .expect("batch lookup")
        .expect("batch present");
    assert_eq!(
        reissued.payload_digest, rebound_batch.payload_digest,
        "reissued token must bind the revised payload digest"
    );
    assert_ne!(
        reissued.payload_digest, approval.payload_digest,
        "revised binding must differ from the pre-edit binding"
    );
    assert!(
        matches!(rebound_batch.approval_state, ApprovalState::Approved),
        "batch must be approvable again after re-approval"
    );
}

#[test]
fn send_edit_rejects_a_posted_batch() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };
    seed_session_with_findings(
        &runtime,
        "session-posted",
        "run-posted",
        &sample_target("owner/repo", 42),
    );

    let batch_id = draft_batch(&runtime, "session-posted");
    let (draft_id, _) = first_draft_id(&runtime, &batch_id);

    // Force the batch into a posted state so the edit must fail closed.
    {
        let store = RogerStore::open(&runtime.store_root).expect("open store");
        let mut batch = store
            .outbound_draft_batch(&batch_id)
            .expect("batch lookup")
            .expect("batch present");
        batch.approval_state = ApprovalState::Posted;
        batch.row_version += 1;
        store
            .store_outbound_draft_batch(&batch)
            .expect("store posted batch");
    }

    let body_path = temp.path().join("body-for-posted.md");
    fs::write(&body_path, "Attempted edit of a posted batch.\n").expect("write body file");

    let edit = run_rr(
        &[
            "send",
            "edit",
            "--draft",
            &draft_id,
            "--body-file",
            body_path.to_str().expect("body path utf8"),
        ],
        &runtime,
    );
    assert_eq!(edit.exit_code, 3, "{}", edit.stderr);
    let combined = format!("{}{}", edit.stdout, edit.stderr);
    assert!(
        combined.contains("posted"),
        "editing a posted batch must be rejected with a clear message: {combined}"
    );

    // No revision was written.
    let store = RogerStore::open(&runtime.store_root).expect("reopen store");
    assert!(
        store
            .outbound_draft_revisions(&draft_id)
            .expect("revision lineage")
            .is_empty(),
        "a posted batch must not accept a new revision"
    );
}
