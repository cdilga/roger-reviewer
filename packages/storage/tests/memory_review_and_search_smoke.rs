//! Storage-level coverage for bead rr-memory-reality-slice-o0go: durable memory
//! review requests + the promotion write path, plus the honest-search behaviors
//! (multi-token AND matching, the exact-identifier fast path, and posted/
//! revision bodies entering the evidence lane).

use tempfile::tempdir;

use roger_app_core::{
    ApprovalState, OutboundDraft, OutboundDraftBatch, PostedAction, PostedActionItem,
    PostedActionStatus, PostingAdapterItemStatus, ReviewTarget,
};
use roger_storage::{
    CreateMaterializedFinding, CreateMemoryReviewRequest, CreateReviewRun, CreateReviewSession,
    MemoryReviewDecision, MemoryReviewRequestKind, MemoryReviewSource, PriorReviewLookupQuery,
    Result, RogerStore, normalize_memory_key,
};

fn target(repository: &str, pull_request_number: u64) -> ReviewTarget {
    ReviewTarget {
        repository: repository.to_owned(),
        pull_request_number,
        base_ref: "main".to_owned(),
        head_ref: "feature".to_owned(),
        base_commit: "1111111".to_owned(),
        head_commit: "2222222".to_owned(),
    }
}

fn seed_session(
    store: &RogerStore,
    session_id: &str,
    run_id: &str,
    repository: &str,
) -> Result<()> {
    store.create_review_session(CreateReviewSession {
        id: session_id,
        review_target: &target(repository, 7),
        provider: "opencode",
        session_locator: None,
        resume_bundle_artifact_id: None,
        continuity_state: "usable",
        attention_state: "awaiting_user_input",
        launch_profile_id: None,
    })?;
    store.create_review_run(CreateReviewRun {
        id: run_id,
        session_id,
        run_kind: "deep_review",
        repo_snapshot: "git:2222222",
        continuity_quality: "usable",
        session_locator_artifact_id: None,
    })?;
    Ok(())
}

fn worker_request<'a>(
    session_id: &'a str,
    scope_key: &'a str,
    statement: &'a str,
    normalized_key: &'a str,
    external_ref: &'a str,
) -> CreateMemoryReviewRequest<'a> {
    CreateMemoryReviewRequest {
        review_session_id: session_id,
        review_run_id: None,
        source: MemoryReviewSource::Worker,
        request_kind: MemoryReviewRequestKind::Promote,
        statement,
        normalized_key,
        scope_key,
        memory_class: "semantic",
        rationale: Some("worker proposal"),
        external_ref: Some(external_ref),
    }
}

#[test]
fn memory_review_requests_persist_list_and_count() -> Result<()> {
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path().join("profile"))?;
    seed_session(&store, "session-a", "run-a", "owner/repo")?;
    let scope_key = "repo:owner/repo";

    let worker = store.create_memory_review_request(worker_request(
        "session-a",
        scope_key,
        "Prefer explicit fallback gate",
        "prefer explicit fallback gate",
        "w1",
    ))?;
    assert_eq!(worker.source, "worker");
    assert_eq!(worker.request_kind, "promote");
    assert_eq!(worker.status, "pending_review");

    // Operator-sourced request in the same scope.
    store.create_memory_review_request(CreateMemoryReviewRequest {
        source: MemoryReviewSource::Operator,
        rationale: None,
        external_ref: Some("op1"),
        ..worker_request(
            "session-a",
            scope_key,
            "Operator note about anchors",
            "operator note about anchors",
            "op1",
        )
    })?;

    let pending = store.pending_memory_review_requests(Some(scope_key), 50)?;
    assert_eq!(pending.len(), 2);
    assert_eq!(
        store.count_pending_memory_review_requests(Some(scope_key))?,
        2
    );
    // A different scope sees none.
    assert_eq!(
        store.count_pending_memory_review_requests(Some("repo:other/repo"))?,
        0
    );
    assert_eq!(store.count_pending_memory_review_requests(None)?, 2);
    Ok(())
}

#[test]
fn accept_materializes_memory_item_and_dedups_by_normalized_key() -> Result<()> {
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path().join("profile"))?;
    seed_session(&store, "session-a", "run-a", "owner/repo")?;
    let scope_key = "repo:owner/repo";
    let normalized_key = normalize_memory_key("Prefer explicit fallback gate");

    let first = store.create_memory_review_request(worker_request(
        "session-a",
        scope_key,
        "Prefer explicit fallback gate",
        &normalized_key,
        "w1",
    ))?;
    let second = store.create_memory_review_request(worker_request(
        "session-a",
        scope_key,
        "Prefer explicit fallback gate (restated)",
        &normalized_key,
        "w2",
    ))?;

    // First acceptance materializes a NEW memory item in `established` state.
    let outcome = store.resolve_memory_review_request(
        &first.id,
        MemoryReviewDecision::Accept,
        "operator:test",
    )?;
    assert!(outcome.materialized_new_item);
    assert_eq!(outcome.request.status, "accepted");
    assert!(outcome.request.resolved_at.is_some());
    assert_eq!(
        outcome.request.resolution_actor.as_deref(),
        Some("operator:test")
    );
    let memory_id = outcome
        .resulting_memory_item_id
        .clone()
        .expect("materialized memory id");
    let item = store.memory_item(&memory_id)?.expect("memory item exists");
    assert_eq!(item.state, "established");
    assert_eq!(item.normalized_key, normalized_key);

    // Second acceptance dedups: it UPDATES the existing item (same id), never a
    // duplicate insert.
    let outcome2 = store.resolve_memory_review_request(
        &second.id,
        MemoryReviewDecision::Accept,
        "operator:test",
    )?;
    assert!(!outcome2.materialized_new_item);
    assert_eq!(
        outcome2.resulting_memory_item_id.as_deref(),
        Some(memory_id.as_str())
    );

    // Exactly one memory item exists for this normalized key.
    let lookup = store.prior_review_lookup(PriorReviewLookupQuery {
        scope_key,
        repository: "owner/repo",
        query_text: "explicit fallback gate",
        limit: 10,
        include_tentative_candidates: false,
        allow_project_scope: false,
        allow_org_scope: false,
        semantic_assets_verified: false,
        semantic_candidates: Vec::new(),
    })?;
    let matching: Vec<_> = lookup
        .promoted_memory
        .iter()
        .filter(|hit| hit.normalized_key == normalized_key)
        .collect();
    assert_eq!(matching.len(), 1, "dedup must keep a single memory row");
    Ok(())
}

#[test]
fn reject_resolves_without_materializing_and_double_resolve_conflicts() -> Result<()> {
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path().join("profile"))?;
    seed_session(&store, "session-a", "run-a", "owner/repo")?;
    let scope_key = "repo:owner/repo";

    let request = store.create_memory_review_request(worker_request(
        "session-a",
        scope_key,
        "Reject me",
        "reject me",
        "w1",
    ))?;
    let outcome = store.resolve_memory_review_request(
        &request.id,
        MemoryReviewDecision::Reject,
        "operator:test",
    )?;
    assert_eq!(outcome.request.status, "rejected");
    assert!(outcome.resulting_memory_item_id.is_none());
    assert!(!outcome.materialized_new_item);
    assert_eq!(
        store.count_pending_memory_review_requests(Some(scope_key))?,
        0
    );

    // Re-resolving an already-resolved request fails closed.
    let err = store.resolve_memory_review_request(
        &request.id,
        MemoryReviewDecision::Accept,
        "operator:test",
    );
    assert!(err.is_err(), "double resolve must conflict");
    Ok(())
}

#[test]
fn multi_token_and_matching_requires_every_token() -> Result<()> {
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path().join("profile"))?;
    seed_session(&store, "session-a", "run-a", "owner/repo")?;

    store.upsert_materialized_finding(CreateMaterializedFinding {
        id: "finding-both",
        session_id: "session-a",
        review_run_id: "run-a",
        stage: "deep_review",
        fingerprint: "fp:both",
        title: "Race condition in refresh path",
        normalized_summary: "the refresh path has a race condition on token reload",
        severity: "high",
        confidence: "high",
        triage_state: "new",
        outbound_state: "not_drafted",
    })?;
    store.upsert_materialized_finding(CreateMaterializedFinding {
        id: "finding-one",
        session_id: "session-a",
        review_run_id: "run-a",
        stage: "deep_review",
        fingerprint: "fp:one",
        title: "Refresh cache invalidation",
        normalized_summary: "refresh invalidates the cache promptly and safely",
        severity: "medium",
        confidence: "medium",
        triage_state: "new",
        outbound_state: "not_drafted",
    })?;

    // Both tokens must match: only finding-both mentions "race" AND "refresh".
    let lookup = store.prior_review_lookup(PriorReviewLookupQuery {
        scope_key: "repo:owner/repo",
        repository: "owner/repo",
        query_text: "race refresh",
        limit: 10,
        include_tentative_candidates: false,
        allow_project_scope: false,
        allow_org_scope: false,
        semantic_assets_verified: false,
        semantic_candidates: Vec::new(),
    })?;
    let ids: Vec<_> = lookup
        .evidence_hits
        .iter()
        .map(|hit| hit.finding_id.as_str())
        .collect();
    assert_eq!(ids, vec!["finding-both"]);
    Ok(())
}

#[test]
fn exact_identifier_fast_path_reports_reason() -> Result<()> {
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path().join("profile"))?;
    seed_session(&store, "session-a", "run-a", "owner/repo")?;

    store.upsert_materialized_finding(CreateMaterializedFinding {
        id: "finding-exact",
        session_id: "session-a",
        review_run_id: "run-a",
        stage: "deep_review",
        fingerprint: "fp:unique-anchor",
        title: "Unrelated title words",
        normalized_summary: "nothing lexically overlapping here",
        severity: "low",
        confidence: "low",
        triage_state: "new",
        outbound_state: "not_drafted",
    })?;

    // Querying the exact fingerprint resolves via the identifier fast path even
    // though the fingerprint appears in no title/summary text.
    let lookup = store.prior_review_lookup(PriorReviewLookupQuery {
        scope_key: "repo:owner/repo",
        repository: "owner/repo",
        query_text: "fp:unique-anchor",
        limit: 10,
        include_tentative_candidates: false,
        allow_project_scope: false,
        allow_org_scope: false,
        semantic_assets_verified: false,
        semantic_candidates: Vec::new(),
    })?;
    let hit = lookup
        .evidence_hits
        .iter()
        .find(|hit| hit.finding_id == "finding-exact")
        .expect("exact-identifier hit");
    assert_eq!(hit.retrieval_reason, "exact_identifier");
    Ok(())
}

#[test]
fn posted_and_revision_bodies_enter_the_evidence_lane() -> Result<()> {
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path().join("profile"))?;
    seed_session(&store, "session-a", "run-a", "owner/repo")?;
    store.upsert_materialized_finding(CreateMaterializedFinding {
        id: "finding-draft",
        session_id: "session-a",
        review_run_id: "run-a",
        stage: "deep_review",
        fingerprint: "fp:draft",
        title: "Sidecar hydration mismatch",
        normalized_summary: "sidecar hydration mismatch needs quarantine",
        severity: "high",
        confidence: "high",
        triage_state: "accepted",
        outbound_state: "drafted",
    })?;

    let batch = OutboundDraftBatch {
        id: "batch-1".to_owned(),
        review_session_id: "session-a".to_owned(),
        review_run_id: "run-a".to_owned(),
        repo_id: "owner/repo".to_owned(),
        remote_review_target_id: "owner/repo#7".to_owned(),
        payload_digest: "digest-1".to_owned(),
        approval_state: ApprovalState::Approved,
        approved_at: Some(1),
        invalidated_at: None,
        invalidation_reason_code: None,
        row_version: 0,
    };
    store.store_outbound_draft_batch(&batch)?;

    let draft = OutboundDraft {
        id: "draft-1".to_owned(),
        review_session_id: "session-a".to_owned(),
        review_run_id: "run-a".to_owned(),
        finding_id: Some("finding-draft".to_owned()),
        draft_batch_id: "batch-1".to_owned(),
        repo_id: "owner/repo".to_owned(),
        remote_review_target_id: "owner/repo#7".to_owned(),
        payload_digest: "digest-1".to_owned(),
        approval_state: ApprovalState::Approved,
        anchor_digest: "anchor-1".to_owned(),
        target_locator: "src/lib.rs:10".to_owned(),
        body: "original draft body about hydration".to_owned(),
        row_version: 0,
    };
    store.store_outbound_draft_item(&draft)?;

    // Edit the draft body, creating a revision lineage (rev 0 seeded + rev 1).
    store.revise_outbound_draft_body(
        "draft-1",
        "revised body: quarantine the sidecar on hydration mismatch",
        roger_storage::RevisionAuthorKind::Operator,
    )?;

    // Post the batch and record a posted action item. Editing the body may
    // rewrite the batch payload digest, so post against the batch's current
    // digest.
    let current_digest = store
        .outbound_draft_batch("batch-1")?
        .expect("batch exists")
        .payload_digest;
    store.store_posted_batch_action(&PostedAction {
        id: "posted-action-1".to_owned(),
        draft_batch_id: "batch-1".to_owned(),
        provider: "github".to_owned(),
        remote_identifier: "https://github.com/owner/repo/pull/7#comment-42".to_owned(),
        status: PostedActionStatus::Succeeded,
        posted_payload_digest: current_digest,
        posted_at: 2,
        failure_code: None,
    })?;
    store.store_posted_action_item(&PostedActionItem {
        id: "posted-item-1".to_owned(),
        posted_action_id: "posted-action-1".to_owned(),
        draft_id: "draft-1".to_owned(),
        status: PostingAdapterItemStatus::Posted,
        remote_identifier: Some("comment-42".to_owned()),
        failure_code: None,
    })?;

    // "hydration" matches both the posted body and the latest revision body.
    let lookup = store.prior_review_lookup(PriorReviewLookupQuery {
        scope_key: "repo:owner/repo",
        repository: "owner/repo",
        query_text: "hydration",
        limit: 20,
        include_tentative_candidates: false,
        allow_project_scope: false,
        allow_org_scope: false,
        semantic_assets_verified: false,
        semantic_candidates: Vec::new(),
    })?;
    let source_kinds: Vec<&str> = lookup
        .evidence_hits
        .iter()
        .map(|hit| hit.source_kind.as_str())
        .collect();
    assert!(
        source_kinds.contains(&"posted_action_item"),
        "posted outcomes must be searchable: {source_kinds:?}"
    );
    assert!(
        source_kinds.contains(&"outbound_draft_revision"),
        "draft revision bodies must be searchable: {source_kinds:?}"
    );
    Ok(())
}
