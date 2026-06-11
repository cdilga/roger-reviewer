//! Storage proof for the TUI cockpit's narrow read APIs and the shared
//! in-process batch-approval path (`approve_outbound_batch_for_session`).
//!
//! The approval path must behave exactly like `rr approve`'s storage writes:
//! payload-digest and target-tuple bound token, item/batch state promotion,
//! and fail-closed blocks on drift, invalidation, and posted-action
//! immutability.

use tempfile::tempdir;

use roger_app_core::{ApprovalState, OutboundDraft, OutboundDraftBatch, ReviewTarget};
use roger_storage::{
    CreateFinding, CreateReviewRun, CreateReviewSession, OutboundBatchApproval, Result, RogerStore,
};

fn sample_target() -> ReviewTarget {
    ReviewTarget {
        repository: "owner/repo".to_owned(),
        pull_request_number: 42,
        base_ref: "main".to_owned(),
        head_ref: "feature".to_owned(),
        base_commit: "deadbeef".to_owned(),
        head_commit: "feedface".to_owned(),
    }
}

fn seed_session(store: &RogerStore, attention_state: &str) -> Result<()> {
    store.create_review_session(CreateReviewSession {
        id: "session-1",
        review_target: &sample_target(),
        provider: "opencode",
        session_locator: None,
        resume_bundle_artifact_id: None,
        continuity_state: "active",
        attention_state,
        launch_profile_id: None,
    })?;
    store.create_review_run(CreateReviewRun {
        id: "run-1",
        session_id: "session-1",
        run_kind: "review",
        repo_snapshot: "git:feedface",
        continuity_quality: "usable",
        session_locator_artifact_id: None,
    })?;
    store.create_finding(CreateFinding {
        id: "finding-1",
        session_id: "session-1",
        first_run_id: "run-1",
        fingerprint: "fp:one",
        title: "First outbound finding",
        triage_state: "accepted",
        outbound_state: "drafted",
    })?;
    Ok(())
}

fn drafted_batch(batch_id: &str, run_id: &str) -> OutboundDraftBatch {
    OutboundDraftBatch {
        id: batch_id.to_owned(),
        review_session_id: "session-1".to_owned(),
        review_run_id: run_id.to_owned(),
        repo_id: "owner/repo".to_owned(),
        remote_review_target_id: "pr-42".to_owned(),
        payload_digest: format!("sha256:payload-{batch_id}"),
        approval_state: ApprovalState::Drafted,
        approved_at: None,
        invalidated_at: None,
        invalidation_reason_code: None,
        row_version: 0,
    }
}

fn drafted_item(draft_id: &str, batch: &OutboundDraftBatch) -> OutboundDraft {
    OutboundDraft {
        id: draft_id.to_owned(),
        review_session_id: batch.review_session_id.clone(),
        review_run_id: batch.review_run_id.clone(),
        finding_id: Some("finding-1".to_owned()),
        draft_batch_id: batch.id.clone(),
        repo_id: batch.repo_id.clone(),
        remote_review_target_id: batch.remote_review_target_id.clone(),
        payload_digest: batch.payload_digest.clone(),
        approval_state: ApprovalState::Drafted,
        anchor_digest: "anchor:one".to_owned(),
        target_locator: "github:owner/repo#42/files#thread-1".to_owned(),
        body: "Canonical outbound body".to_owned(),
        row_version: 0,
    }
}

fn seed_drafted_batch(store: &RogerStore, batch_id: &str, run_id: &str) -> Result<()> {
    let batch = drafted_batch(batch_id, run_id);
    store.store_outbound_draft_batch(&batch)?;
    store.store_outbound_draft_item(&drafted_item(&format!("draft-{batch_id}"), &batch))?;
    Ok(())
}

#[test]
fn finding_decision_events_read_api_returns_history_in_order() -> Result<()> {
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path().join("profile"))?;
    seed_session(&store, "awaiting_user_input")?;

    store.update_finding_triage_state("finding-1", "needs_follow_up")?;
    store.update_finding_triage_state("finding-1", "accepted")?;

    let events = store.finding_decision_events_for_finding("finding-1")?;
    assert_eq!(events.len(), 3, "creation + two triage decisions");
    assert!(events.iter().all(|event| event.finding_id == "finding-1"));
    assert_eq!(events[0].triage_state, "accepted"); // creation state from seed
    assert_eq!(events[1].triage_state, "needs_follow_up");
    assert_eq!(events[2].triage_state, "accepted");
    assert!(
        events
            .windows(2)
            .all(|w| w[0].created_at <= w[1].created_at),
        "events ordered oldest first"
    );

    assert!(
        store
            .finding_decision_events_for_finding("finding-missing")?
            .is_empty()
    );
    Ok(())
}

#[test]
fn review_runs_for_session_returns_newest_first() -> Result<()> {
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path().join("profile"))?;
    seed_session(&store, "awaiting_user_input")?;
    store.create_review_run(CreateReviewRun {
        id: "run-2",
        session_id: "session-1",
        run_kind: "refresh",
        repo_snapshot: "git:feedface2",
        continuity_quality: "usable",
        session_locator_artifact_id: None,
    })?;

    let runs = store.review_runs_for_session("session-1")?;
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].id, "run-2", "newest first");
    assert_eq!(runs[1].id, "run-1");
    assert_eq!(runs[0].run_kind, "refresh");

    assert!(store.review_runs_for_session("session-missing")?.is_empty());
    Ok(())
}

#[test]
fn approve_outbound_batch_records_digest_bound_token_and_is_idempotent() -> Result<()> {
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path().join("profile"))?;
    seed_session(&store, "awaiting_user_input")?;
    seed_drafted_batch(&store, "batch-1", "run-1")?;

    let outcome = store.approve_outbound_batch_for_session("session-1", "batch-1")?;
    let OutboundBatchApproval::Approved {
        approval,
        draft_count,
        already_recorded,
    } = outcome
    else {
        assert!(false, "expected approval, got {outcome:?}");
        unreachable!()
    };
    assert_eq!(draft_count, 1);
    assert!(!already_recorded);
    assert_eq!(approval.payload_digest, "sha256:payload-batch-1");
    assert!(approval.revoked_at.is_none());

    // Storage truth: token, item state, and batch state all promoted.
    // Not a credential: "token" here is Roger's local approval-token row.
    let stored_approval = store
        .approval_token_for_batch("batch-1")?
        .expect("approval token persisted");
    assert_eq!(stored_approval.payload_digest, "sha256:payload-batch-1");
    let batch = store
        .outbound_draft_batch("batch-1")?
        .expect("batch persisted");
    assert!(matches!(batch.approval_state, ApprovalState::Approved));
    assert_eq!(batch.approved_at, Some(approval.approved_at));
    let items = store.outbound_draft_items_for_batch("batch-1")?;
    assert!(
        items
            .iter()
            .all(|item| matches!(item.approval_state, ApprovalState::Approved))
    );

    // Second call is a truthful no-op.
    let second = store.approve_outbound_batch_for_session("session-1", "batch-1")?;
    let OutboundBatchApproval::Approved {
        already_recorded, ..
    } = second
    else {
        assert!(false, "expected approval, got {second:?}");
        unreachable!()
    };
    assert!(already_recorded);
    Ok(())
}

#[test]
fn approve_outbound_batch_blocks_on_run_drift() -> Result<()> {
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path().join("profile"))?;
    seed_session(&store, "awaiting_user_input")?;
    seed_drafted_batch(&store, "batch-1", "run-1")?;
    // A newer persisted run supersedes the batch's run binding.
    store.create_review_run(CreateReviewRun {
        id: "run-2",
        session_id: "session-1",
        run_kind: "refresh",
        repo_snapshot: "git:feedface2",
        continuity_quality: "usable",
        session_locator_artifact_id: None,
    })?;

    let outcome = store.approve_outbound_batch_for_session("session-1", "batch-1")?;
    assert_eq!(
        outcome,
        OutboundBatchApproval::Blocked {
            reason_code: "approval_invalidated:local_state_drift".to_owned()
        }
    );
    assert!(
        store.approval_token_for_batch("batch-1")?.is_none(),
        "blocked approval must not write a token"
    );
    Ok(())
}

#[test]
fn approve_outbound_batch_blocks_after_posted_action() -> Result<()> {
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path().join("profile"))?;
    seed_session(&store, "awaiting_user_input")?;
    seed_drafted_batch(&store, "batch-1", "run-1")?;
    store.store_posted_batch_action(&roger_app_core::PostedAction {
        id: "posted-1".to_owned(),
        draft_batch_id: "batch-1".to_owned(),
        provider: "github".to_owned(),
        remote_identifier: "review-comment-1".to_owned(),
        status: roger_app_core::PostedActionStatus::Succeeded,
        posted_payload_digest: "sha256:payload-batch-1".to_owned(),
        posted_at: 1_710_001_020,
        failure_code: None,
    })?;

    let outcome = store.approve_outbound_batch_for_session("session-1", "batch-1")?;
    assert_eq!(
        outcome,
        OutboundBatchApproval::Blocked {
            reason_code: "existing_posted_action".to_owned()
        }
    );
    Ok(())
}

#[test]
fn approve_outbound_batch_blocks_on_invalidated_batch() -> Result<()> {
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path().join("profile"))?;
    seed_session(&store, "awaiting_user_input")?;
    let mut batch = drafted_batch("batch-1", "run-1");
    batch.approval_state = ApprovalState::Invalidated;
    batch.invalidated_at = Some(1_710_001_000);
    batch.invalidation_reason_code = Some("target_drift".to_owned());
    store.store_outbound_draft_batch(&batch)?;
    store.store_outbound_draft_item(&drafted_item("draft-1", &batch))?;

    let outcome = store.approve_outbound_batch_for_session("session-1", "batch-1")?;
    assert_eq!(
        outcome,
        OutboundBatchApproval::Blocked {
            reason_code: "approval_invalidated:target_drift".to_owned()
        }
    );
    Ok(())
}

#[test]
fn approve_outbound_batch_blocks_on_missing_state_and_stale_attention() -> Result<()> {
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path().join("profile"))?;
    seed_session(&store, "awaiting_user_input")?;

    // Missing batch fails closed.
    let outcome = store.approve_outbound_batch_for_session("session-1", "batch-missing")?;
    assert_eq!(
        outcome,
        OutboundBatchApproval::Blocked {
            reason_code: "missing_local_state".to_owned()
        }
    );

    // Missing session fails closed.
    let outcome = store.approve_outbound_batch_for_session("session-missing", "batch-1")?;
    assert_eq!(
        outcome,
        OutboundBatchApproval::Blocked {
            reason_code: "missing_local_state".to_owned()
        }
    );
    Ok(())
}

#[test]
fn approve_outbound_batch_blocks_when_attention_requires_reconciliation() -> Result<()> {
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path().join("profile"))?;
    seed_session(&store, "refresh_recommended")?;
    seed_drafted_batch(&store, "batch-1", "run-1")?;

    let outcome = store.approve_outbound_batch_for_session("session-1", "batch-1")?;
    assert_eq!(
        outcome,
        OutboundBatchApproval::Blocked {
            reason_code: "stale_local_state".to_owned()
        }
    );
    Ok(())
}
