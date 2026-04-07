#![cfg(unix)]

use roger_app_core::{
    ApprovalState, ExplicitPostingInput, ExplicitPostingOutcome, OutboundApprovalToken,
    OutboundDraft, OutboundDraftBatch, OutboundPostingAdapter, PostedActionStatus,
    PostingAdapterItemResult, PostingAdapterItemStatus, execute_explicit_posting_flow,
    outbound_target_tuple_json,
};
use roger_cli::{CliRuntime, run};
use roger_storage::{
    CreateCodeEvidenceLocation, CreateMaterializedFinding, CreateOutboundDraft, RogerStore,
};
use roger_validation::{discover_suite_metadata, failure_artifact_paths};
use serde_json::Value;
use std::cell::Cell;
use std::collections::HashSet;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use tempfile::{TempDir, tempdir};

#[derive(Debug)]
struct PostingDouble {
    calls: Cell<u32>,
}

impl PostingDouble {
    fn new() -> Self {
        Self {
            calls: Cell::new(0),
        }
    }
}

impl OutboundPostingAdapter for PostingDouble {
    fn post_approved_draft_batch(
        &self,
        _batch: &OutboundDraftBatch,
        drafts: &[OutboundDraft],
    ) -> std::result::Result<Vec<PostingAdapterItemResult>, String> {
        self.calls.set(self.calls.get() + 1);
        Ok(drafts
            .iter()
            .map(|draft| PostingAdapterItemResult {
                draft_id: draft.id.clone(),
                status: PostingAdapterItemStatus::Posted,
                remote_identifier: Some(format!(
                    "https://api.github.com/repos/owner/repo/pulls/comments/{}",
                    draft.id
                )),
                failure_code: None,
            })
            .collect())
    }
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

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("packages parent")
        .parent()
        .expect("workspace root")
        .to_path_buf()
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

fn write_stub_binary() -> (TempDir, PathBuf) {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("opencode-stub");
    let script = r#"#!/bin/sh
if [ "$1" = "--session" ]; then
  exit 0
fi
if [ "$1" = "export" ]; then
  echo "{}"
  exit 0
fi
exit 0
"#;
    fs::write(&path, script).expect("write stub binary");
    let mut perms = fs::metadata(&path).expect("metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).expect("chmod stub binary");
    (dir, path)
}

#[test]
fn e2e_core_review_happy_path_exercises_real_repo_suite_flow() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary();

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let review = run_rr(
        &["review", "--pr", "42", "--provider", "opencode", "--robot"],
        &runtime,
    );
    assert_eq!(review.exit_code, 0, "{}", review.stderr);
    let review_payload = parse_robot_payload(&review.stdout);
    assert!(
        matches!(
            review_payload["outcome"].as_str(),
            Some("complete") | Some("degraded")
        ),
        "unexpected review outcome payload: {}",
        review_payload
    );
    let session_id = review_payload["data"]["session_id"]
        .as_str()
        .expect("session id")
        .to_owned();
    let review_run_id = review_payload["data"]["review_run_id"]
        .as_str()
        .expect("review run id")
        .to_owned();

    let resume = run_rr(&["resume", "--session", &session_id, "--robot"], &runtime);
    assert_eq!(resume.exit_code, 0, "{}", resume.stderr);
    let resume_payload = parse_robot_payload(&resume.stdout);
    assert_eq!(resume_payload["data"]["provider"], "opencode");

    let store = RogerStore::open(&runtime.store_root).expect("open store");
    let finding = store
        .upsert_materialized_finding(CreateMaterializedFinding {
            id: "finding-e2e-1",
            session_id: &session_id,
            review_run_id: &review_run_id,
            stage: "deep_review",
            fingerprint: "fp-e2e-core-1",
            title: "Potential posting invalidation regression",
            normalized_summary: "approval invalidation path requires explicit reconfirmation",
            severity: "high",
            confidence: "medium",
            triage_state: "new",
            outbound_state: "drafted",
        })
        .expect("materialize finding");
    assert_eq!(finding.id, "finding-e2e-1");

    store
        .add_code_evidence_location(CreateCodeEvidenceLocation {
            id: "evidence-e2e-1",
            finding_id: "finding-e2e-1",
            review_session_id: &session_id,
            review_run_id: &review_run_id,
            evidence_role: "primary",
            repo_rel_path: "packages/cli/src/lib.rs",
            start_line: 1313,
            end_line: Some(1538),
            anchor_state: "valid",
            anchor_digest: Some("anchor-e2e-core-1"),
            excerpt_artifact_id: None,
        })
        .expect("add code evidence");

    let findings_for_run = store
        .materialized_findings_for_run(&session_id, &review_run_id)
        .expect("findings for run");
    assert_eq!(findings_for_run.len(), 1);
    assert_eq!(findings_for_run[0].id, "finding-e2e-1");

    let findings = run_rr(&["findings", "--session", &session_id, "--robot"], &runtime);
    assert_eq!(findings.exit_code, 0, "{}", findings.stderr);
    let findings_payload = parse_robot_payload(&findings.stdout);
    assert_eq!(findings_payload["outcome"], "complete");
    let has_materialized_finding = findings_payload["data"]["items"]
        .as_array()
        .expect("findings items")
        .iter()
        .any(|item| item["finding_id"] == "finding-e2e-1");
    assert!(has_materialized_finding);

    store
        .create_outbound_draft(CreateOutboundDraft {
            id: "draft-e2e-1",
            session_id: &session_id,
            finding_id: "finding-e2e-1",
            target_locator: "github:owner/repo#42/files#thread-e2e-1",
            payload_digest: "sha256:payload-e2e-1",
            body: "Please re-check the invalidation guard on explicit posting.",
        })
        .expect("create outbound draft");
    store
        .approve_outbound_draft(
            "approval-token-e2e-1",
            "draft-e2e-1",
            "sha256:payload-e2e-1",
            "github:owner/repo#42/files#thread-e2e-1",
        )
        .expect("approve outbound draft");

    let batch = OutboundDraftBatch {
        id: "batch-e2e-1".to_owned(),
        review_session_id: session_id.clone(),
        review_run_id: review_run_id.clone(),
        repo_id: "owner/repo".to_owned(),
        remote_review_target_id: "pr-42".to_owned(),
        payload_digest: "sha256:payload-e2e-1".to_owned(),
        approval_state: ApprovalState::Approved,
        approved_at: Some(1_700_000_100),
        invalidated_at: None,
        invalidation_reason_code: None,
        row_version: 1,
    };
    let draft = OutboundDraft {
        id: "draft-e2e-1".to_owned(),
        review_session_id: session_id.clone(),
        review_run_id: review_run_id.clone(),
        finding_id: Some("finding-e2e-1".to_owned()),
        draft_batch_id: batch.id.clone(),
        repo_id: "owner/repo".to_owned(),
        remote_review_target_id: "pr-42".to_owned(),
        payload_digest: batch.payload_digest.clone(),
        approval_state: ApprovalState::Approved,
        anchor_digest: "anchor-e2e-core-1".to_owned(),
        row_version: 1,
    };
    let approval = OutboundApprovalToken {
        id: "approval-e2e-1".to_owned(),
        draft_batch_id: batch.id.clone(),
        payload_digest: batch.payload_digest.clone(),
        target_tuple_json: outbound_target_tuple_json(&batch),
        approved_at: 1_700_000_120,
        revoked_at: None,
    };

    let posting_double = PostingDouble::new();
    let posting_result = execute_explicit_posting_flow(
        ExplicitPostingInput {
            action_id: "posted-e2e-1",
            provider: "github",
            batch: &batch,
            drafts: std::slice::from_ref(&draft),
            approval: &approval,
            refresh_signals: &[],
            reconfirmed_finding_ids: &HashSet::new(),
        },
        &posting_double,
    );
    assert_eq!(posting_double.calls.get(), 1);
    assert_eq!(posting_result.outcome, ExplicitPostingOutcome::Posted);
    let posted_action = posting_result
        .posted_action
        .expect("posted action must be recorded");
    assert_eq!(posted_action.status, PostedActionStatus::Succeeded);

    store
        .record_posted_action(
            &posted_action.id,
            "draft-e2e-1",
            &posted_action.remote_identifier,
            &posted_action.posted_payload_digest,
            "posted",
        )
        .expect("record posted action");

    let overview = store
        .session_overview(&session_id)
        .expect("session overview");
    assert_eq!(overview.run_count, 1);
    assert_eq!(overview.finding_count, 1);
    assert_eq!(overview.draft_count, 1);
    assert_eq!(overview.approval_count, 1);
    assert_eq!(overview.posted_action_count, 1);

    drop(store);
    let reopened = RogerStore::open(&runtime.store_root).expect("reopen store");
    let reopened_overview = reopened
        .session_overview(&session_id)
        .expect("reopened session overview");
    assert_eq!(reopened_overview.finding_count, 1);
    assert_eq!(reopened_overview.posted_action_count, 1);

    let metadata_dir = workspace_root().join("tests/suites");
    let suites = discover_suite_metadata(&metadata_dir).expect("discover suite metadata");
    let e2e_suite = suites
        .iter()
        .find(|suite| suite.id == "e2e_core_review_happy_path")
        .expect("e2e suite metadata");
    assert_eq!(
        e2e_suite.fixture_families,
        vec!["fixture_repo_compact_review", "fixture_github_draft_batch"]
    );

    let failing_ids = vec!["e2e_core_review_happy_path".to_owned()];
    let failure_paths = failure_artifact_paths(
        &metadata_dir,
        temp.path().join("test-artifacts"),
        &failing_ids,
    )
    .expect("failure artifact paths");
    assert_eq!(failure_paths.len(), 1);
    let failure_path = failure_paths[0].to_string_lossy();
    assert!(
        failure_path.contains("failures/e2e_core_review_happy_path/sample_failure"),
        "failure artifact path must stay in shared harness layout, got: {failure_path}"
    );
}
