use tempfile::tempdir;

use roger_app_core::{
    ApprovalState, OutboundApprovalToken, OutboundDraft, OutboundDraftBatch, PostedAction,
    PostedActionStatus, PostingAdapterItemResult, PostingAdapterItemStatus, ReviewTarget,
    outbound_target_tuple_json, posted_action_items_from_item_results,
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
        target_locator: "github:owner/repo#42/files#thread-1".to_owned(),
        body: "Canonical outbound body one".to_owned(),
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
        target_locator: "github:owner/repo#42/files#thread-2".to_owned(),
        body: "Canonical outbound body two".to_owned(),
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
    assert!(
        reopened
            .posted_action_items_for_batch("batch-1")?
            .is_empty()
    );

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
        target_locator: "github:owner/repo#42/files#thread-1".to_owned(),
        body: "Canonical outbound body one".to_owned(),
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

    let mut relocalized_draft = valid_draft.clone();
    relocalized_draft.target_locator = "github:owner/repo#42/files#thread-2".to_owned();
    let locator_identity_err = store
        .store_outbound_draft_item(&relocalized_draft)
        .expect_err("same draft id should not silently rewrite target locator");
    assert!(
        locator_identity_err
            .to_string()
            .contains("target_locator_mismatch"),
        "{locator_identity_err}"
    );

    let mut rewritten_body_draft = valid_draft.clone();
    rewritten_body_draft.body = "Canonical outbound body rewritten".to_owned();
    let body_identity_err = store
        .store_outbound_draft_item(&rewritten_body_draft)
        .expect_err("same draft id should not silently rewrite postable body");
    assert!(
        body_identity_err.to_string().contains("body_mismatch"),
        "{body_identity_err}"
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

    let valid_posted = PostedAction {
        posted_payload_digest: batch.payload_digest.clone(),
        ..wrong_posted.clone()
    };
    store.store_posted_batch_action(&valid_posted)?;

    let missing_remote_identifier = roger_app_core::PostedActionItem {
        id: "posted-1:draft-1".to_owned(),
        posted_action_id: valid_posted.id.clone(),
        draft_id: valid_draft.id.clone(),
        status: PostingAdapterItemStatus::Posted,
        remote_identifier: None,
        failure_code: None,
    };
    let item_err = store
        .store_posted_action_item(&missing_remote_identifier)
        .expect_err("posted item without remote identifier should fail closed");
    assert!(
        item_err.to_string().contains("posted_action_item_binding"),
        "{item_err}"
    );

    let second_batch = OutboundDraftBatch {
        id: "batch-2".to_owned(),
        review_session_id: "session-1".to_owned(),
        review_run_id: "run-1".to_owned(),
        repo_id: "owner/repo".to_owned(),
        remote_review_target_id: "pr-42".to_owned(),
        payload_digest: "sha256:payload-batch-2".to_owned(),
        approval_state: ApprovalState::Drafted,
        approved_at: None,
        invalidated_at: None,
        invalidation_reason_code: None,
        row_version: 0,
    };
    store.store_outbound_draft_batch(&second_batch)?;
    store.store_outbound_draft_item(&OutboundDraft {
        id: "draft-2".to_owned(),
        review_session_id: "session-1".to_owned(),
        review_run_id: "run-1".to_owned(),
        finding_id: Some("finding-2".to_owned()),
        draft_batch_id: second_batch.id.clone(),
        repo_id: second_batch.repo_id.clone(),
        remote_review_target_id: second_batch.remote_review_target_id.clone(),
        payload_digest: second_batch.payload_digest.clone(),
        approval_state: ApprovalState::Drafted,
        anchor_digest: "anchor:two".to_owned(),
        target_locator: "github:owner/repo#42/files#thread-2".to_owned(),
        body: "Canonical outbound body two".to_owned(),
        row_version: 0,
    })?;
    let batch_mismatch_item = roger_app_core::PostedActionItem {
        id: "posted-1:draft-2".to_owned(),
        posted_action_id: valid_posted.id.clone(),
        draft_id: "draft-2".to_owned(),
        status: PostingAdapterItemStatus::Failed,
        remote_identifier: None,
        failure_code: Some("retryable:service_unavailable".to_owned()),
    };
    let batch_mismatch_err = store
        .store_posted_action_item(&batch_mismatch_item)
        .expect_err("posted item bound to the wrong draft batch should fail closed");
    assert!(
        batch_mismatch_err
            .to_string()
            .contains("posted_action_item_binding"),
        "{batch_mismatch_err}"
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
        target_locator: "github:owner/repo#42/files#thread-1".to_owned(),
        body: "Canonical outbound body one".to_owned(),
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

#[test]
fn canonical_drafted_batch_projects_queryable_local_draft_state() -> Result<()> {
    let temp = tempdir()?;
    let root = temp.path().join("profile");
    let store = RogerStore::open(&root)?;
    seed_review(&store)?;

    let batch = OutboundDraftBatch {
        id: "batch-drafted".to_owned(),
        review_session_id: "session-1".to_owned(),
        review_run_id: "run-1".to_owned(),
        repo_id: "owner/repo".to_owned(),
        remote_review_target_id: "pr-42".to_owned(),
        payload_digest: "sha256:payload-drafted".to_owned(),
        approval_state: ApprovalState::Drafted,
        approved_at: None,
        invalidated_at: None,
        invalidation_reason_code: None,
        row_version: 0,
    };
    store.store_outbound_draft_batch(&batch)?;
    store.store_outbound_draft_item(&OutboundDraft {
        id: "draft-drafted".to_owned(),
        review_session_id: "session-1".to_owned(),
        review_run_id: "run-1".to_owned(),
        finding_id: Some("finding-1".to_owned()),
        draft_batch_id: batch.id.clone(),
        repo_id: batch.repo_id.clone(),
        remote_review_target_id: batch.remote_review_target_id.clone(),
        payload_digest: batch.payload_digest.clone(),
        approval_state: ApprovalState::Drafted,
        anchor_digest: "anchor:drafted".to_owned(),
        target_locator: "github:owner/repo#42/files#thread-drafted".to_owned(),
        body: "Canonical outbound body drafted".to_owned(),
        row_version: 0,
    })?;

    let projection = store.outbound_surface_projection_for_finding("finding-1", "drafted")?;
    assert_eq!(projection.state, "awaiting_approval");
    assert_eq!(projection.source, "canonical_batch");
    assert_eq!(projection.draft_id.as_deref(), Some("draft-drafted"));
    assert_eq!(projection.draft_batch_id.as_deref(), Some("batch-drafted"));
    assert!(projection.approval_id.is_none());
    assert!(projection.posted_action_id.is_none());
    assert!(projection.posted_action_status.is_none());
    assert!(projection.invalidation_reason_code.is_none());
    assert!(!projection.mutation_elevated);

    let counts = store.outbound_state_counts_for_run("session-1", "run-1")?;
    assert_eq!(counts.awaiting_approval, 2);
    assert_eq!(counts.approved, 0);
    assert_eq!(counts.invalidated, 0);
    assert_eq!(counts.posted, 0);
    assert_eq!(counts.failed, 0);

    Ok(())
}

#[test]
fn outbound_surface_projection_reconciles_canonical_and_legacy_state_paths() -> Result<()> {
    let temp = tempdir()?;
    let root = temp.path().join("profile");
    let store = RogerStore::open(&root)?;
    seed_review(&store)?;
    store.create_finding(CreateFinding {
        id: "finding-3",
        session_id: "session-1",
        first_run_id: "run-1",
        fingerprint: "fp:three",
        title: "Third outbound finding",
        triage_state: "accepted",
        outbound_state: "drafted",
    })?;
    store.create_finding(CreateFinding {
        id: "finding-4",
        session_id: "session-1",
        first_run_id: "run-1",
        fingerprint: "fp:four",
        title: "Fourth outbound finding",
        triage_state: "accepted",
        outbound_state: "posted",
    })?;
    store.create_finding(CreateFinding {
        id: "finding-5",
        session_id: "session-1",
        first_run_id: "run-1",
        fingerprint: "fp:five",
        title: "Fifth outbound finding",
        triage_state: "accepted",
        outbound_state: "failed",
    })?;

    store.create_outbound_draft(roger_storage::CreateOutboundDraft {
        id: "legacy-awaiting",
        session_id: "session-1",
        finding_id: "finding-1",
        target_locator: "github:owner/repo#42/files#thread-1",
        payload_digest: "sha256:legacy-awaiting",
        body: "Awaiting approval body",
    })?;

    let approved_batch = OutboundDraftBatch {
        id: "batch-approved".to_owned(),
        review_session_id: "session-1".to_owned(),
        review_run_id: "run-1".to_owned(),
        repo_id: "owner/repo".to_owned(),
        remote_review_target_id: "pr-42".to_owned(),
        payload_digest: "sha256:payload-approved".to_owned(),
        approval_state: ApprovalState::Approved,
        approved_at: Some(1_710_010_000),
        invalidated_at: None,
        invalidation_reason_code: None,
        row_version: 1,
    };
    store.store_outbound_draft_batch(&approved_batch)?;
    store.store_outbound_draft_item(&OutboundDraft {
        id: "draft-approved".to_owned(),
        review_session_id: "session-1".to_owned(),
        review_run_id: "run-1".to_owned(),
        finding_id: Some("finding-2".to_owned()),
        draft_batch_id: approved_batch.id.clone(),
        repo_id: approved_batch.repo_id.clone(),
        remote_review_target_id: approved_batch.remote_review_target_id.clone(),
        payload_digest: approved_batch.payload_digest.clone(),
        approval_state: ApprovalState::Approved,
        anchor_digest: "anchor:approved".to_owned(),
        target_locator: "github:owner/repo#42/files#thread-2".to_owned(),
        body: "Approved canonical outbound body".to_owned(),
        row_version: 1,
    })?;
    store.store_outbound_approval_token(&OutboundApprovalToken {
        id: "approval-approved".to_owned(),
        draft_batch_id: approved_batch.id.clone(),
        payload_digest: approved_batch.payload_digest.clone(),
        target_tuple_json: outbound_target_tuple_json(&approved_batch),
        approved_at: 1_710_010_001,
        revoked_at: None,
    })?;

    let invalidated_batch = OutboundDraftBatch {
        id: "batch-invalidated".to_owned(),
        review_session_id: "session-1".to_owned(),
        review_run_id: "run-1".to_owned(),
        repo_id: "owner/repo".to_owned(),
        remote_review_target_id: "pr-42".to_owned(),
        payload_digest: "sha256:payload-invalidated".to_owned(),
        approval_state: ApprovalState::Invalidated,
        approved_at: Some(1_710_010_010),
        invalidated_at: Some(1_710_010_020),
        invalidation_reason_code: Some("target_rebased".to_owned()),
        row_version: 2,
    };
    store.store_outbound_draft_batch(&invalidated_batch)?;
    store.store_outbound_draft_item(&OutboundDraft {
        id: "draft-invalidated".to_owned(),
        review_session_id: "session-1".to_owned(),
        review_run_id: "run-1".to_owned(),
        finding_id: Some("finding-3".to_owned()),
        draft_batch_id: invalidated_batch.id.clone(),
        repo_id: invalidated_batch.repo_id.clone(),
        remote_review_target_id: invalidated_batch.remote_review_target_id.clone(),
        payload_digest: invalidated_batch.payload_digest.clone(),
        approval_state: ApprovalState::Invalidated,
        anchor_digest: "anchor:invalidated".to_owned(),
        target_locator: "github:owner/repo#42/files#thread-3".to_owned(),
        body: "Invalidated canonical outbound body".to_owned(),
        row_version: 2,
    })?;

    store.create_outbound_draft(roger_storage::CreateOutboundDraft {
        id: "legacy-posted",
        session_id: "session-1",
        finding_id: "finding-4",
        target_locator: "github:owner/repo#42/files#thread-4",
        payload_digest: "sha256:legacy-posted",
        body: "Posted body",
    })?;
    store.approve_outbound_draft(
        "legacy-approval-posted",
        "legacy-posted",
        "sha256:legacy-posted",
        "github:owner/repo#42/files#thread-4",
    )?;
    store.record_posted_action(
        "legacy-posted-action",
        "legacy-posted",
        "github:owner/repo#42/files#thread-4",
        "sha256:legacy-posted",
        "posted",
    )?;

    let failed_batch = OutboundDraftBatch {
        id: "batch-failed".to_owned(),
        review_session_id: "session-1".to_owned(),
        review_run_id: "run-1".to_owned(),
        repo_id: "owner/repo".to_owned(),
        remote_review_target_id: "pr-42".to_owned(),
        payload_digest: "sha256:payload-failed".to_owned(),
        approval_state: ApprovalState::Approved,
        approved_at: Some(1_710_010_030),
        invalidated_at: None,
        invalidation_reason_code: None,
        row_version: 1,
    };
    store.store_outbound_draft_batch(&failed_batch)?;
    store.store_outbound_draft_item(&OutboundDraft {
        id: "draft-failed".to_owned(),
        review_session_id: "session-1".to_owned(),
        review_run_id: "run-1".to_owned(),
        finding_id: Some("finding-5".to_owned()),
        draft_batch_id: failed_batch.id.clone(),
        repo_id: failed_batch.repo_id.clone(),
        remote_review_target_id: failed_batch.remote_review_target_id.clone(),
        payload_digest: failed_batch.payload_digest.clone(),
        approval_state: ApprovalState::Approved,
        anchor_digest: "anchor:failed".to_owned(),
        target_locator: "github:owner/repo#42/files#thread-5".to_owned(),
        body: "Failed canonical outbound body".to_owned(),
        row_version: 1,
    })?;
    store.store_posted_batch_action(&PostedAction {
        id: "posted-failed".to_owned(),
        draft_batch_id: failed_batch.id.clone(),
        provider: "github".to_owned(),
        remote_identifier: "review-comment-2005".to_owned(),
        status: PostedActionStatus::Failed,
        posted_payload_digest: failed_batch.payload_digest.clone(),
        posted_at: 1_710_010_040,
        failure_code: Some("github_write_denied".to_owned()),
    })?;

    let counts = store.outbound_state_counts_for_run("session-1", "run-1")?;
    assert_eq!(counts.awaiting_approval, 1);
    assert_eq!(counts.approved, 1);
    assert_eq!(counts.invalidated, 1);
    assert_eq!(counts.posted, 1);
    assert_eq!(counts.failed, 1);

    let awaiting = store.outbound_surface_projection_for_finding("finding-1", "drafted")?;
    assert_eq!(awaiting.state, "awaiting_approval");
    assert_eq!(awaiting.source, "legacy_draft");
    assert!(!awaiting.mutation_elevated);

    let approved = store.outbound_surface_projection_for_finding("finding-2", "approved")?;
    assert_eq!(approved.state, "approved");
    assert_eq!(approved.source, "canonical_batch");
    assert!(approved.mutation_elevated);

    let invalidated = store.outbound_surface_projection_for_finding("finding-3", "drafted")?;
    assert_eq!(invalidated.state, "invalidated");
    assert_eq!(
        invalidated.invalidation_reason_code.as_deref(),
        Some("target_rebased")
    );

    let posted = store.outbound_surface_projection_for_finding("finding-4", "posted")?;
    assert_eq!(posted.state, "posted");
    assert_eq!(posted.source, "legacy_draft");

    let failed = store.outbound_surface_projection_for_finding("finding-5", "failed")?;
    assert_eq!(failed.state, "failed");
    assert_eq!(failed.source, "canonical_batch");
    assert_eq!(failed.posted_action_status.as_deref(), Some("Failed"));

    Ok(())
}

#[test]
fn partial_post_restart_preserves_exact_per_finding_membership_and_retry_lineage() -> Result<()> {
    let temp = tempdir()?;
    let root = temp.path().join("profile");
    let batch = OutboundDraftBatch {
        id: "batch-partial".to_owned(),
        review_session_id: "session-1".to_owned(),
        review_run_id: "run-1".to_owned(),
        repo_id: "owner/repo".to_owned(),
        remote_review_target_id: "pr-42".to_owned(),
        payload_digest: "sha256:payload-partial".to_owned(),
        approval_state: ApprovalState::Approved,
        approved_at: Some(1_710_020_000),
        invalidated_at: None,
        invalidation_reason_code: None,
        row_version: 1,
    };
    let draft_one = OutboundDraft {
        id: "draft-partial-1".to_owned(),
        review_session_id: "session-1".to_owned(),
        review_run_id: "run-1".to_owned(),
        finding_id: Some("finding-1".to_owned()),
        draft_batch_id: batch.id.clone(),
        repo_id: batch.repo_id.clone(),
        remote_review_target_id: batch.remote_review_target_id.clone(),
        payload_digest: batch.payload_digest.clone(),
        approval_state: ApprovalState::Approved,
        anchor_digest: "anchor:partial-1".to_owned(),
        target_locator: "github:owner/repo#42/files#thread-partial-1".to_owned(),
        body: "Canonical outbound body one".to_owned(),
        row_version: 1,
    };
    let draft_two = OutboundDraft {
        id: "draft-partial-2".to_owned(),
        review_session_id: "session-1".to_owned(),
        review_run_id: "run-1".to_owned(),
        finding_id: Some("finding-2".to_owned()),
        draft_batch_id: batch.id.clone(),
        repo_id: batch.repo_id.clone(),
        remote_review_target_id: batch.remote_review_target_id.clone(),
        payload_digest: batch.payload_digest.clone(),
        approval_state: ApprovalState::Approved,
        anchor_digest: "anchor:partial-2".to_owned(),
        target_locator: "github:owner/repo#42/files#thread-partial-2".to_owned(),
        body: "Canonical outbound body two".to_owned(),
        row_version: 1,
    };
    let approval = OutboundApprovalToken {
        id: "approval-partial".to_owned(),
        draft_batch_id: batch.id.clone(),
        payload_digest: batch.payload_digest.clone(),
        target_tuple_json: outbound_target_tuple_json(&batch),
        approved_at: 1_710_020_001,
        revoked_at: None,
    };
    let posted = PostedAction {
        id: "posted-partial".to_owned(),
        draft_batch_id: batch.id.clone(),
        provider: "github".to_owned(),
        remote_identifier: "73".to_owned(),
        status: PostedActionStatus::Partial,
        posted_payload_digest: batch.payload_digest.clone(),
        posted_at: 1_710_020_020,
        failure_code: Some("partial_failure".to_owned()),
    };
    let item_results = vec![
        PostingAdapterItemResult {
            draft_id: draft_one.id.clone(),
            status: PostingAdapterItemStatus::Posted,
            remote_identifier: Some("73".to_owned()),
            failure_code: None,
        },
        PostingAdapterItemResult {
            draft_id: draft_two.id.clone(),
            status: PostingAdapterItemStatus::Failed,
            remote_identifier: None,
            failure_code: Some("retryable:service_unavailable".to_owned()),
        },
    ];
    let posted_items = posted_action_items_from_item_results(&posted.id, &item_results);

    {
        let store = RogerStore::open(&root)?;
        seed_review(&store)?;
        store.store_outbound_draft_batch(&batch)?;
        store.store_outbound_draft_item(&draft_one)?;
        store.store_outbound_draft_item(&draft_two)?;
        store.store_outbound_approval_token(&approval)?;
        store.store_posted_batch_action(&posted)?;
        store.store_posted_action_items(&posted_items)?;
    }

    let reopened = RogerStore::open(&root)?;
    assert_eq!(
        reopened.posted_actions_for_batch(&batch.id)?,
        vec![posted.clone()]
    );
    assert_eq!(
        reopened.posted_action_items_for_batch(&batch.id)?,
        posted_items
    );

    let counts = reopened.outbound_state_counts_for_run("session-1", "run-1")?;
    assert_eq!(counts.awaiting_approval, 0);
    assert_eq!(counts.approved, 0);
    assert_eq!(counts.invalidated, 0);
    assert_eq!(counts.posted, 1);
    assert_eq!(counts.failed, 1);

    let posted_projection =
        reopened.outbound_surface_projection_for_finding("finding-1", "approved")?;
    assert_eq!(posted_projection.state, "posted");
    assert_eq!(posted_projection.source, "canonical_batch");
    assert_eq!(
        posted_projection.posted_action_status.as_deref(),
        Some("Partial")
    );
    assert_eq!(
        posted_projection.posted_action_item_status.as_deref(),
        Some("Posted")
    );
    assert_eq!(posted_projection.remote_identifier.as_deref(), Some("73"));
    assert_eq!(posted_projection.failure_code, None);

    let failed_projection =
        reopened.outbound_surface_projection_for_finding("finding-2", "approved")?;
    assert_eq!(failed_projection.state, "failed");
    assert_eq!(failed_projection.source, "canonical_batch");
    assert_eq!(
        failed_projection.posted_action_status.as_deref(),
        Some("Partial")
    );
    assert_eq!(
        failed_projection.posted_action_item_status.as_deref(),
        Some("Failed")
    );
    assert_eq!(failed_projection.remote_identifier, None);
    assert_eq!(
        failed_projection.failure_code.as_deref(),
        Some("retryable:service_unavailable")
    );

    Ok(())
}
