use tempfile::tempdir;

use roger_app_core::{
    ApprovalState, OutboundApprovalToken, OutboundDraft, OutboundDraftBatch, PostedAction,
    PostedActionStatus, ReviewTarget, outbound_target_tuple_json,
};
use roger_storage::{
    CreateFinding, CreateReviewRun, CreateReviewSession,
    OUTBOUND_APPROVAL_REVOKED_REASON_DRAFT_REVISED, Result, RevisionAuthorKind, RogerStore,
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

fn seed_review(store: &RogerStore) -> Result<()> {
    store.create_review_session(CreateReviewSession {
        id: "session-1",
        review_target: &sample_target(),
        provider: "opencode",
        session_locator: None,
        resume_bundle_artifact_id: None,
        continuity_state: "active",
        attention_state: "ready",
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

fn drafted_batch() -> OutboundDraftBatch {
    OutboundDraftBatch {
        id: "batch-1".to_owned(),
        review_session_id: "session-1".to_owned(),
        review_run_id: "run-1".to_owned(),
        repo_id: "owner/repo".to_owned(),
        remote_review_target_id: "pr-42".to_owned(),
        payload_digest: "sha256:payload-batch-1".to_owned(),
        approval_state: ApprovalState::Drafted,
        approved_at: None,
        invalidated_at: None,
        invalidation_reason_code: None,
        row_version: 0,
    }
}

fn draft_in(batch: &OutboundDraftBatch, state: ApprovalState, body: &str) -> OutboundDraft {
    OutboundDraft {
        id: "draft-1".to_owned(),
        review_session_id: "session-1".to_owned(),
        review_run_id: "run-1".to_owned(),
        finding_id: Some("finding-1".to_owned()),
        draft_batch_id: batch.id.clone(),
        repo_id: batch.repo_id.clone(),
        remote_review_target_id: batch.remote_review_target_id.clone(),
        payload_digest: batch.payload_digest.clone(),
        approval_state: state,
        anchor_digest: "anchor:one".to_owned(),
        target_locator: "github:owner/repo#42/files#thread-1".to_owned(),
        body: body.to_owned(),
        row_version: 0,
    }
}

#[test]
fn revision_lineage_preserves_original_and_orders_zero_to_n() -> Result<()> {
    let temp = tempdir()?;
    let root = temp.path().join("profile");
    let store = RogerStore::open(&root)?;
    seed_review(&store)?;

    let batch = drafted_batch();
    store.store_outbound_draft_batch(&batch)?;
    store.store_outbound_draft_item(&draft_in(
        &batch,
        ApprovalState::Drafted,
        "Original generated body",
    ))?;

    // No revisions until the first edit; current body falls back to the row.
    assert!(store.outbound_draft_revisions("draft-1")?.is_empty());
    assert_eq!(
        store.current_outbound_draft_body("draft-1")?.as_deref(),
        Some("Original generated body")
    );

    let first = store.revise_outbound_draft_body(
        "draft-1",
        "Operator edit one",
        RevisionAuthorKind::Operator,
    )?;
    assert_eq!(first.revision_index, 1);
    assert_eq!(first.author_kind, RevisionAuthorKind::Operator);
    assert_eq!(
        first.supersedes_revision_id.as_deref(),
        Some("draftrev-draft-1-0")
    );

    let second =
        store.revise_outbound_draft_body("draft-1", "Agent edit two", RevisionAuthorKind::Agent)?;
    assert_eq!(second.revision_index, 2);
    assert_eq!(
        second.supersedes_revision_id.as_deref(),
        Some("draftrev-draft-1-1")
    );

    let lineage = store.outbound_draft_revisions("draft-1")?;
    assert_eq!(lineage.len(), 3);
    // Revision 0 preserves the original generated body, authored by the agent.
    assert_eq!(lineage[0].revision_index, 0);
    assert_eq!(lineage[0].body, "Original generated body");
    assert_eq!(lineage[0].author_kind, RevisionAuthorKind::Agent);
    assert_eq!(lineage[0].supersedes_revision_id, None);
    assert_eq!(lineage[1].body, "Operator edit one");
    assert_eq!(lineage[2].body, "Agent edit two");
    // Monotonic and chained.
    assert_eq!(
        lineage[1].supersedes_revision_id.as_deref(),
        Some(lineage[0].id.as_str())
    );
    assert_eq!(
        lineage[2].supersedes_revision_id.as_deref(),
        Some(lineage[1].id.as_str())
    );

    // Latest body is the read-path/posting body, and it survives a reopen.
    assert_eq!(
        store.current_outbound_draft_body("draft-1")?.as_deref(),
        Some("Agent edit two")
    );
    drop(store);
    let reopened = RogerStore::open(&root)?;
    assert_eq!(
        reopened.current_outbound_draft_body("draft-1")?.as_deref(),
        Some("Agent edit two")
    );
    assert_eq!(
        reopened
            .outbound_draft_item("draft-1")?
            .expect("draft")
            .body,
        "Agent edit two"
    );
    // The original is still retrievable after reopen.
    assert_eq!(
        reopened.outbound_draft_revisions("draft-1")?[0].body,
        "Original generated body"
    );
    Ok(())
}

#[test]
fn editing_approved_draft_revokes_token_and_resets_state_fail_closed() -> Result<()> {
    let temp = tempdir()?;
    let root = temp.path().join("profile");
    let store = RogerStore::open(&root)?;
    seed_review(&store)?;

    let mut batch = drafted_batch();
    batch.approval_state = ApprovalState::Approved;
    batch.approved_at = Some(1_710_001_010);
    batch.row_version = 1;
    store.store_outbound_draft_batch(&batch)?;
    store.store_outbound_draft_item(&draft_in(&batch, ApprovalState::Approved, "Approved body"))?;
    store.store_outbound_approval_token(&OutboundApprovalToken {
        id: "approval-1".to_owned(),
        draft_batch_id: batch.id.clone(),
        payload_digest: batch.payload_digest.clone(),
        target_tuple_json: outbound_target_tuple_json(&batch),
        approved_at: 1_710_001_010,
        revoked_at: None,
    })?;

    store.revise_outbound_draft_body(
        "draft-1",
        "Revised after approval",
        RevisionAuthorKind::Operator,
    )?;

    // Token revoked with the typed reason.
    let approval = store
        .approval_token_for_batch("batch-1")?
        .expect("approval record");
    assert!(
        approval.revoked_at.is_some(),
        "approval token must be revoked"
    );
    assert_eq!(
        store
            .outbound_approval_revocation_reason("batch-1")?
            .as_deref(),
        Some(OUTBOUND_APPROVAL_REVOKED_REASON_DRAFT_REVISED)
    );

    // Draft and batch reset from Approved back to Drafted.
    assert_eq!(
        store
            .outbound_draft_item("draft-1")?
            .expect("draft")
            .approval_state,
        ApprovalState::Drafted
    );
    let reset_batch = store.outbound_draft_batch("batch-1")?.expect("batch");
    assert_eq!(reset_batch.approval_state, ApprovalState::Drafted);
    assert_eq!(reset_batch.approved_at, None);

    // The revised body is the current body.
    assert_eq!(
        store.current_outbound_draft_body("draft-1")?.as_deref(),
        Some("Revised after approval")
    );

    // The exact-payload binding is re-derived: batch and item digests change
    // together, so the revoked token's stale digest can never match the
    // revised payload.
    let original_digest = batch.payload_digest.clone();
    assert_ne!(
        reset_batch.payload_digest, original_digest,
        "revision must re-derive the batch payload digest"
    );
    assert_eq!(
        store
            .outbound_draft_item("draft-1")?
            .expect("draft")
            .payload_digest,
        reset_batch.payload_digest,
        "batch and item digests stay shared after revision"
    );

    // Re-approval path: reissuing the token row (same id) bound to the revised
    // digest re-arms it and clears the revocation reason.
    store.store_outbound_approval_token(&OutboundApprovalToken {
        id: approval.id.clone(),
        draft_batch_id: reset_batch.id.clone(),
        payload_digest: reset_batch.payload_digest.clone(),
        target_tuple_json: outbound_target_tuple_json(&reset_batch),
        approved_at: 1_710_001_020,
        revoked_at: None,
    })?;
    let reissued = store
        .approval_token_for_batch("batch-1")?
        .expect("reissued approval");
    assert_eq!(reissued.revoked_at, None, "reissued token must be live");
    assert_eq!(reissued.payload_digest, reset_batch.payload_digest);
    assert_eq!(
        store.outbound_approval_revocation_reason("batch-1")?,
        None,
        "re-arming the token must clear the revocation reason"
    );
    Ok(())
}

#[test]
fn editing_a_posted_batch_is_rejected_outright() -> Result<()> {
    let temp = tempdir()?;
    let root = temp.path().join("profile");
    let store = RogerStore::open(&root)?;
    seed_review(&store)?;

    let mut batch = drafted_batch();
    batch.approval_state = ApprovalState::Approved;
    batch.approved_at = Some(1_710_001_010);
    batch.row_version = 1;
    store.store_outbound_draft_batch(&batch)?;
    store.store_outbound_draft_item(&draft_in(&batch, ApprovalState::Approved, "Posted body"))?;
    store.store_posted_batch_action(&PostedAction {
        id: "posted-1".to_owned(),
        draft_batch_id: batch.id.clone(),
        provider: "github".to_owned(),
        remote_identifier: "review-comment-1001".to_owned(),
        status: PostedActionStatus::Succeeded,
        posted_payload_digest: batch.payload_digest.clone(),
        posted_at: 1_710_001_020,
        failure_code: None,
    })?;

    let err = store
        .revise_outbound_draft_body("draft-1", "Edit after post", RevisionAuthorKind::Operator)
        .expect_err("editing a posted batch must be rejected");
    assert!(
        err.to_string().contains("posted_batch_edit_rejected"),
        "{err}"
    );

    // Nothing was recorded and the body is unchanged.
    assert!(store.outbound_draft_revisions("draft-1")?.is_empty());
    assert_eq!(
        store.current_outbound_draft_body("draft-1")?.as_deref(),
        Some("Posted body")
    );
    Ok(())
}

#[test]
fn revising_unknown_draft_fails_not_found() -> Result<()> {
    let temp = tempdir()?;
    let root = temp.path().join("profile");
    let store = RogerStore::open(&root)?;
    seed_review(&store)?;

    let err = store
        .revise_outbound_draft_body("missing-draft", "body", RevisionAuthorKind::Operator)
        .expect_err("unknown draft must fail closed");
    assert!(
        err.to_string().contains("outbound_draft_item not found"),
        "{err}"
    );
    Ok(())
}
