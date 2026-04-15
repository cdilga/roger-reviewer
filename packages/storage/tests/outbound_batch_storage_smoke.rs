use tempfile::tempdir;

use roger_app_core::{
    ApprovalState, OutboundApprovalToken, OutboundDraft, OutboundDraftBatch, PostedAction,
    PostedActionStatus, ReviewTarget, outbound_target_tuple_json,
};
use roger_storage::{CreateFinding, CreateReviewRun, CreateReviewSession, Result, RogerStore};

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
    store.create_finding(CreateFinding {
        id: "finding-2",
        session_id: "session-1",
        first_run_id: "run-1",
        fingerprint: "fp:two",
        title: "Second outbound finding",
        triage_state: "accepted",
        outbound_state: "drafted",
    })?;
    Ok(())
}

#[test]
fn canonical_outbound_batch_storage_round_trips_and_counts_toward_session_overview() -> Result<()> {
    let temp = tempdir()?;
    let root = temp.path().join("profile");

    let batch = OutboundDraftBatch {
        id: "batch-1".to_owned(),
        review_session_id: "session-1".to_owned(),
        review_run_id: "run-1".to_owned(),
        repo_id: "owner/repo".to_owned(),
        remote_review_target_id: "pr-42".to_owned(),
        payload_digest: "sha256:payload-batch-1".to_owned(),
        approval_state: ApprovalState::Approved,
        approved_at: Some(1_710_001_000),
        invalidated_at: None,
        invalidation_reason_code: None,
        row_version: 1,
    };
    let draft_one = OutboundDraft {
        id: "draft-1".to_owned(),
        review_session_id: "session-1".to_owned(),
        review_run_id: "run-1".to_owned(),
        finding_id: Some("finding-1".to_owned()),
        draft_batch_id: batch.id.clone(),
        repo_id: batch.repo_id.clone(),
        remote_review_target_id: batch.remote_review_target_id.clone(),
        payload_digest: batch.payload_digest.clone(),
        approval_state: ApprovalState::Approved,
        anchor_digest: "anchor:one".to_owned(),
        row_version: 1,
    };
    let draft_two = OutboundDraft {
        id: "draft-2".to_owned(),
        review_session_id: "session-1".to_owned(),
        review_run_id: "run-1".to_owned(),
        finding_id: Some("finding-2".to_owned()),
        draft_batch_id: batch.id.clone(),
        repo_id: batch.repo_id.clone(),
        remote_review_target_id: batch.remote_review_target_id.clone(),
        payload_digest: batch.payload_digest.clone(),
        approval_state: ApprovalState::Approved,
        anchor_digest: "anchor:two".to_owned(),
        row_version: 1,
    };
    let approval = OutboundApprovalToken {
        id: "approval-1".to_owned(),
        draft_batch_id: batch.id.clone(),
        payload_digest: batch.payload_digest.clone(),
        target_tuple_json: outbound_target_tuple_json(&batch),
        approved_at: 1_710_001_010,
        revoked_at: None,
    };
    let posted = PostedAction {
        id: "posted-1".to_owned(),
        draft_batch_id: batch.id.clone(),
        provider: "github".to_owned(),
        remote_identifier: "review-comment-1001".to_owned(),
        status: PostedActionStatus::Succeeded,
        posted_payload_digest: batch.payload_digest.clone(),
        posted_at: 1_710_001_020,
        failure_code: None,
    };

    {
        let store = RogerStore::open(&root)?;
        seed_review(&store)?;
        store.store_outbound_draft_batch(&batch)?;
        store.store_outbound_draft_item(&draft_one)?;
        store.store_outbound_draft_item(&draft_two)?;
        store.store_outbound_approval_token(&approval)?;
        store.store_posted_batch_action(&posted)?;
    }

    let reopened = RogerStore::open(&root)?;
    assert_eq!(
        reopened
            .outbound_draft_batch("batch-1")?
            .expect("batch should exist"),
        batch
    );
    assert_eq!(
        reopened.outbound_draft_items_for_batch("batch-1")?,
        vec![draft_one, draft_two]
    );
    assert_eq!(
        reopened
            .approval_token_for_batch("batch-1")?
            .expect("approval should exist"),
        approval
    );
    assert_eq!(reopened.posted_actions_for_batch("batch-1")?, vec![posted]);

    let overview = reopened.session_overview("session-1")?;
    assert_eq!(overview.draft_count, 2);
    assert_eq!(overview.approval_count, 1);
    assert_eq!(overview.posted_action_count, 1);

    Ok(())
}

#[test]
fn canonical_outbound_storage_rejects_binding_drift_fail_closed() -> Result<()> {
    let temp = tempdir()?;
    let root = temp.path().join("profile");
    let store = RogerStore::open(&root)?;
    seed_review(&store)?;

    let batch = OutboundDraftBatch {
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
    };
    store.store_outbound_draft_batch(&batch)?;

    let mut wrong_batch = batch.clone();
    wrong_batch.remote_review_target_id = "pr-99".to_owned();
    let batch_err = store
        .store_outbound_draft_batch(&wrong_batch)
        .expect_err("batch retarget should fail closed");
    assert!(
        batch_err
            .to_string()
            .contains("outbound_draft_batch_identity"),
        "{batch_err}"
    );

    let mut wrong_draft = OutboundDraft {
        id: "draft-1".to_owned(),
        review_session_id: "session-1".to_owned(),
        review_run_id: "run-1".to_owned(),
        finding_id: Some("finding-1".to_owned()),
        draft_batch_id: batch.id.clone(),
        repo_id: batch.repo_id.clone(),
        remote_review_target_id: batch.remote_review_target_id.clone(),
        payload_digest: batch.payload_digest.clone(),
        approval_state: ApprovalState::Drafted,
        anchor_digest: "anchor:one".to_owned(),
        row_version: 0,
    };
    wrong_draft.remote_review_target_id = "pr-99".to_owned();
    let draft_err = store
        .store_outbound_draft_item(&wrong_draft)
        .expect_err("target drift should fail closed");
    assert!(
        draft_err
            .to_string()
            .contains("outbound_draft_batch_binding"),
        "{draft_err}"
    );

    let valid_draft = OutboundDraft {
        remote_review_target_id: batch.remote_review_target_id.clone(),
        ..wrong_draft.clone()
    };
    store.store_outbound_draft_item(&valid_draft)?;

    let mut retargeted_draft = valid_draft.clone();
    retargeted_draft.anchor_digest = "anchor:other".to_owned();
    let draft_identity_err = store
        .store_outbound_draft_item(&retargeted_draft)
        .expect_err("same draft id should not silently rewrite anchor lineage");
    assert!(
        draft_identity_err
            .to_string()
            .contains("outbound_draft_item_identity"),
        "{draft_identity_err}"
    );

    let valid_approval = OutboundApprovalToken {
        id: "approval-1".to_owned(),
        draft_batch_id: batch.id.clone(),
        payload_digest: batch.payload_digest.clone(),
        target_tuple_json: outbound_target_tuple_json(&batch),
        approved_at: 1_710_001_010,
        revoked_at: None,
    };
    store.store_outbound_approval_token(&valid_approval)?;

    let wrong_approval = OutboundApprovalToken {
        id: "approval-1".to_owned(),
        draft_batch_id: batch.id.clone(),
        payload_digest: batch.payload_digest.clone(),
        target_tuple_json: "{\"review_session_id\":\"session-1\",\"repo_id\":\"owner/repo\",\"remote_review_target_id\":\"pr-99\"}".to_owned(),
        approved_at: 1_710_001_010,
        revoked_at: None,
    };
    let approval_err = store
        .store_outbound_approval_token(&wrong_approval)
        .expect_err("approval target drift should fail closed");
    assert!(
        approval_err
            .to_string()
            .contains("outbound_approval_token_binding"),
        "{approval_err}"
    );

    let rekeyed_approval = OutboundApprovalToken {
        id: "approval-2".to_owned(),
        ..valid_approval
    };
    let approval_identity_err = store
        .store_outbound_approval_token(&rekeyed_approval)
        .expect_err("same batch should not accept a second approval id");
    assert!(
        approval_identity_err
            .to_string()
            .contains("approval_id_mismatch"),
        "{approval_identity_err}"
    );

    let wrong_posted = PostedAction {
        id: "posted-1".to_owned(),
        draft_batch_id: batch.id.clone(),
        provider: "github".to_owned(),
        remote_identifier: "review-comment-1001".to_owned(),
        status: PostedActionStatus::Succeeded,
        posted_payload_digest: "sha256:payload-other".to_owned(),
        posted_at: 1_710_001_020,
        failure_code: None,
    };
    let posted_err = store
        .store_posted_batch_action(&wrong_posted)
        .expect_err("posted payload drift should fail closed");
    assert!(
        posted_err
            .to_string()
            .contains("posted_batch_action_binding"),
        "{posted_err}"
    );

    Ok(())
}

#[test]
fn canonical_outbound_storage_upserts_state_transitions_for_invalidation() -> Result<()> {
    let temp = tempdir()?;
    let root = temp.path().join("profile");
    let store = RogerStore::open(&root)?;
    seed_review(&store)?;

    let mut batch = OutboundDraftBatch {
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
    };
    let mut draft = OutboundDraft {
        id: "draft-1".to_owned(),
        review_session_id: "session-1".to_owned(),
        review_run_id: "run-1".to_owned(),
        finding_id: Some("finding-1".to_owned()),
        draft_batch_id: batch.id.clone(),
        repo_id: batch.repo_id.clone(),
        remote_review_target_id: batch.remote_review_target_id.clone(),
        payload_digest: batch.payload_digest.clone(),
        approval_state: ApprovalState::Drafted,
        anchor_digest: "anchor:one".to_owned(),
        row_version: 0,
    };

    store.store_outbound_draft_batch(&batch)?;
    store.store_outbound_draft_item(&draft)?;

    batch.approval_state = ApprovalState::Approved;
    batch.approved_at = Some(1_710_001_010);
    batch.row_version = 1;
    draft.approval_state = ApprovalState::Approved;
    draft.row_version = 1;
    store.store_outbound_draft_batch(&batch)?;
    store.store_outbound_draft_item(&draft)?;

    let mut approval = OutboundApprovalToken {
        id: "approval-1".to_owned(),
        draft_batch_id: batch.id.clone(),
        payload_digest: batch.payload_digest.clone(),
        target_tuple_json: outbound_target_tuple_json(&batch),
        approved_at: 1_710_001_010,
        revoked_at: None,
    };
    store.store_outbound_approval_token(&approval)?;

    batch.approval_state = ApprovalState::Invalidated;
    batch.invalidated_at = Some(1_710_001_020);
    batch.invalidation_reason_code = Some("target_rebased".to_owned());
    batch.row_version = 2;
    approval.revoked_at = Some(1_710_001_020);
    store.store_outbound_draft_batch(&batch)?;
    store.store_outbound_approval_token(&approval)?;

    assert_eq!(
        store
            .outbound_draft_batch("batch-1")?
            .expect("batch should exist"),
        batch
    );
    assert_eq!(
        store
            .outbound_draft_item("draft-1")?
            .expect("draft should exist"),
        draft
    );
    assert_eq!(
        store
            .approval_token_for_batch("batch-1")?
            .expect("approval should exist"),
        approval
    );

    Ok(())
}
