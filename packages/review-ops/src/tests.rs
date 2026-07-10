use super::*;
use roger_app_core::{
    PostingAdapterItemResult, PostingAdapterItemStatus, ReviewTarget as CoreReviewTarget,
};
use roger_storage::{
    CreateMaterializedFinding, CreateReviewRun, CreateReviewSession, MemoryReviewDecision,
    MemoryReviewRequestKind, MemoryReviewSource, RogerStore,
};
use tempfile::tempdir;

fn target(repository: &str, pr: u64) -> CoreReviewTarget {
    CoreReviewTarget {
        repository: repository.to_owned(),
        pull_request_number: pr,
        base_ref: "main".to_owned(),
        head_ref: "feature".to_owned(),
        base_commit: "1111111".to_owned(),
        head_commit: "2222222".to_owned(),
    }
}

fn open_store() -> (tempfile::TempDir, RogerStore) {
    let temp = tempdir().expect("tempdir");
    let store = RogerStore::open(temp.path().join("profile")).expect("open store");
    (temp, store)
}

fn seed_session(store: &RogerStore, session_id: &str, run_id: &str, repository: &str, pr: u64) {
    store
        .create_review_session(CreateReviewSession {
            id: session_id,
            review_target: &target(repository, pr),
            provider: "opencode",
            session_locator: None,
            resume_bundle_artifact_id: None,
            continuity_state: "usable",
            attention_state: "awaiting_user_input",
            launch_profile_id: None,
        })
        .expect("create session");
    store
        .create_review_run(CreateReviewRun {
            id: run_id,
            session_id,
            run_kind: "deep_review",
            repo_snapshot: "git:2222222",
            continuity_quality: "usable",
            session_locator_artifact_id: None,
        })
        .expect("create run");
}

fn seed_finding(
    store: &RogerStore,
    id: &str,
    session_id: &str,
    run_id: &str,
    triage_state: &str,
    outbound_state: &str,
) {
    store
        .upsert_materialized_finding(CreateMaterializedFinding {
            id,
            session_id,
            review_run_id: run_id,
            stage: "deep_review",
            fingerprint: &format!("fp:{id}"),
            title: &format!("Title {id}"),
            normalized_summary: &format!("summary {id}"),
            severity: "high",
            confidence: "high",
            triage_state,
            outbound_state,
        })
        .expect("seed finding");
}

fn resolved_session(store: &RogerStore, session_id: &str) -> roger_storage::ReviewSessionRecord {
    store
        .review_session(session_id)
        .expect("load session")
        .expect("session present")
}

/// Fake posting adapter: reports every draft as posted with a synthetic id.
struct FakePostingAdapter;

impl OutboundPostingAdapter for FakePostingAdapter {
    fn post_approved_draft_batch(
        &self,
        _target: &CoreReviewTarget,
        _batch: &OutboundDraftBatch,
        drafts: &[OutboundDraft],
    ) -> Result<Vec<PostingAdapterItemResult>, String> {
        Ok(drafts
            .iter()
            .map(|draft| PostingAdapterItemResult {
                draft_id: draft.id.clone(),
                status: PostingAdapterItemStatus::Posted,
                remote_identifier: Some(format!("remote-{}", draft.id)),
                failure_code: None,
            })
            .collect())
    }
}

// --- materialize_draft_batch ------------------------------------------------

#[test]
fn materialize_requires_accepted_findings() {
    let (_tmp, store) = open_store();
    seed_session(&store, "s1", "r1", "owner/repo", 7);
    seed_finding(&store, "f1", "s1", "r1", "new", "not-drafted");
    let session = resolved_session(&store, "s1");

    let err = materialize_draft_batch(
        &store,
        &session,
        &DraftSelection::Explicit(vec!["f1".to_owned()]),
    )
    .expect_err("non-accepted finding must be rejected");
    match err {
        MaterializeDraftRejection::SelectionNotDraftable { issues, .. } => {
            assert!(matches!(
                issues.as_slice(),
                [DraftSelectionIssue::TriageStateNotAccepted { .. }]
            ));
        }
        other => unreachable!("unexpected rejection: {other:?}"),
    }
}

#[test]
fn materialize_accepts_and_stores_batch() {
    let (_tmp, store) = open_store();
    seed_session(&store, "s1", "r1", "owner/repo", 7);
    seed_finding(&store, "f1", "s1", "r1", "accepted", "not-drafted");
    seed_finding(&store, "f2", "s1", "r1", "accepted", "not-drafted");
    let session = resolved_session(&store, "s1");

    let outcome = materialize_draft_batch(&store, &session, &DraftSelection::AllFindings)
        .expect("materialize");
    assert_eq!(outcome.selection_mode, "all_findings");
    assert_eq!(outcome.drafts.len(), 2);
    assert_eq!(outcome.previews.len(), 2);
    assert_eq!(outcome.selected_finding_ids, vec!["f1", "f2"]);
    assert!(outcome.batch.payload_digest.starts_with("sha256:"));
    let stored = store
        .outbound_draft_batch(&outcome.batch.id)
        .expect("load")
        .expect("present");
    assert!(matches!(stored.approval_state, ApprovalState::Drafted));
}

#[test]
fn materialize_blocks_when_no_selection() {
    let (_tmp, store) = open_store();
    seed_session(&store, "s1", "r1", "owner/repo", 7);
    seed_finding(&store, "f1", "s1", "r1", "accepted", "not-drafted");
    let session = resolved_session(&store, "s1");
    let err = materialize_draft_batch(&store, &session, &DraftSelection::Explicit(vec![]))
        .expect_err("empty explicit selection blocks");
    assert!(matches!(
        err,
        MaterializeDraftRejection::FindingSelectionRequired { .. }
    ));
}

// --- set_finding_triage -----------------------------------------------------

#[test]
fn triage_rejects_unsupported_state() {
    let (_tmp, store) = open_store();
    seed_session(&store, "s1", "r1", "owner/repo", 7);
    seed_finding(&store, "f1", "s1", "r1", "new", "not-drafted");
    let session = resolved_session(&store, "s1");
    let err =
        set_finding_triage(&store, &session, &["f1".to_owned()], "bogus").expect_err("bad state");
    assert!(matches!(err, SetTriageRejection::UnsupportedState { .. }));
}

#[test]
fn triage_rejects_unknown_finding() {
    let (_tmp, store) = open_store();
    seed_session(&store, "s1", "r1", "owner/repo", 7);
    let session = resolved_session(&store, "s1");
    let err = set_finding_triage(&store, &session, &["nope".to_owned()], "accepted")
        .expect_err("unknown finding");
    match err {
        SetTriageRejection::UnknownFindingIds {
            unknown_finding_ids,
        } => {
            assert_eq!(unknown_finding_ids, vec!["nope"]);
        }
        other => unreachable!("unexpected: {other:?}"),
    }
}

#[test]
fn triage_applies_state() {
    let (_tmp, store) = open_store();
    seed_session(&store, "s1", "r1", "owner/repo", 7);
    seed_finding(&store, "f1", "s1", "r1", "new", "not-drafted");
    let session = resolved_session(&store, "s1");
    let outcome =
        set_finding_triage(&store, &session, &["f1".to_owned()], "accepted").expect("triage");
    assert_eq!(outcome.triage_state, "accepted");
    assert_eq!(outcome.updated_findings.len(), 1);
    assert_eq!(outcome.updated_findings[0].triage_state, "accepted");
}

// --- approve_batch ----------------------------------------------------------

fn materialize_ready_batch(store: &RogerStore) -> (roger_storage::ReviewSessionRecord, String) {
    seed_session(store, "s1", "r1", "owner/repo", 7);
    seed_finding(store, "f1", "s1", "r1", "accepted", "not-drafted");
    let session = resolved_session(store, "s1");
    let outcome = materialize_draft_batch(store, &session, &DraftSelection::AllFindings)
        .expect("materialize");
    (session, outcome.batch.id)
}

#[test]
fn approve_requires_batch_selection() {
    let (_tmp, store) = open_store();
    let (session, _batch) = materialize_ready_batch(&store);
    let err = approve_batch(&store, &session, None).expect_err("no batch id");
    assert!(matches!(
        err,
        ApproveRejection::BatchSelectionRequired { .. }
    ));
}

#[test]
fn approve_records_token_and_marks_approved() {
    let (_tmp, store) = open_store();
    let (session, batch_id) = materialize_ready_batch(&store);
    let outcome = approve_batch(&store, &session, Some(&batch_id)).expect("approve");
    assert!(outcome.approval_created);
    assert!(!outcome.batch_already_approved);
    let stored = store
        .outbound_draft_batch(&batch_id)
        .expect("load")
        .expect("present");
    assert!(matches!(stored.approval_state, ApprovalState::Approved));
    let approval_record = store
        .approval_token_for_batch(&batch_id)
        .expect("approval query")
        .expect("approval present");
    assert!(approval_record.revoked_at.is_none());
}

#[test]
fn approve_blocks_unknown_batch() {
    let (_tmp, store) = open_store();
    let (session, _batch) = materialize_ready_batch(&store);
    let err =
        approve_batch(&store, &session, Some("draft-batch-missing")).expect_err("unknown batch");
    assert!(matches!(err, ApproveRejection::BatchNotFound { .. }));
}

// --- post_batch -------------------------------------------------------------

#[test]
fn post_requires_approval_first() {
    let (_tmp, store) = open_store();
    let (session, batch_id) = materialize_ready_batch(&store);
    let err = post_batch(&store, &session, Some(&batch_id), &FakePostingAdapter)
        .expect_err("post before approve");
    assert!(matches!(err, PostRejection::DraftStateNotPostable { .. }));
}

#[test]
fn post_succeeds_after_approval() {
    let (_tmp, store) = open_store();
    let (session, batch_id) = materialize_ready_batch(&store);
    approve_batch(&store, &session, Some(&batch_id)).expect("approve");
    let outcome = post_batch(&store, &session, Some(&batch_id), &FakePostingAdapter).expect("post");
    assert!(matches!(
        outcome.posting_result.outcome,
        ExplicitPostingOutcome::Posted
    ));
    let posted = store
        .posted_actions_for_batch(&batch_id)
        .expect("posted actions");
    assert_eq!(posted.len(), 1);
}

#[test]
fn post_blocks_duplicate_after_first_post() {
    let (_tmp, store) = open_store();
    let (session, batch_id) = materialize_ready_batch(&store);
    approve_batch(&store, &session, Some(&batch_id)).expect("approve");
    post_batch(&store, &session, Some(&batch_id), &FakePostingAdapter).expect("first post");
    let err = post_batch(&store, &session, Some(&batch_id), &FakePostingAdapter)
        .expect_err("second post blocked");
    assert!(matches!(err, PostRejection::ExistingPostedAction { .. }));
}

#[test]
fn post_requires_batch_selection() {
    let (_tmp, store) = open_store();
    let (session, batch_id) = materialize_ready_batch(&store);
    approve_batch(&store, &session, Some(&batch_id)).expect("approve");
    let err = post_batch(&store, &session, None, &FakePostingAdapter).expect_err("no batch id");
    assert!(matches!(err, PostRejection::BatchSelectionRequired { .. }));
}

// --- resolve_memory_review --------------------------------------------------

#[test]
fn memory_review_accept_materializes_item() {
    let (_tmp, store) = open_store();
    seed_session(&store, "s1", "r1", "owner/repo", 7);
    let request = store
        .create_memory_review_request(roger_storage::CreateMemoryReviewRequest {
            review_session_id: "s1",
            review_run_id: Some("r1"),
            source: MemoryReviewSource::Worker,
            request_kind: MemoryReviewRequestKind::Promote,
            statement: "Always reconfirm approved drafts after refresh",
            normalized_key: "reconfirm approved drafts",
            scope_key: "repo:owner/repo",
            memory_class: "procedural",
            rationale: None,
            external_ref: None,
        })
        .expect("create request");

    let outcome = resolve_memory_review(
        &store,
        &request.id,
        MemoryReviewDecision::Accept,
        "operator",
    )
    .expect("resolve accept");
    assert!(outcome.resulting_memory_item_id.is_some());
    assert!(outcome.materialized_new_item);
}

#[test]
fn memory_review_reject_resolves_without_item() {
    let (_tmp, store) = open_store();
    seed_session(&store, "s1", "r1", "owner/repo", 7);
    let request = store
        .create_memory_review_request(roger_storage::CreateMemoryReviewRequest {
            review_session_id: "s1",
            review_run_id: Some("r1"),
            source: MemoryReviewSource::Worker,
            request_kind: MemoryReviewRequestKind::Promote,
            statement: "Reject me",
            normalized_key: "reject me",
            scope_key: "repo:owner/repo",
            memory_class: "procedural",
            rationale: None,
            external_ref: None,
        })
        .expect("create request");
    let outcome = resolve_memory_review(
        &store,
        &request.id,
        MemoryReviewDecision::Reject,
        "operator",
    )
    .expect("resolve reject");
    assert!(outcome.resulting_memory_item_id.is_none());
}

// --- create_clarification ---------------------------------------------------

#[test]
fn clarification_create_list_resolve_roundtrip() {
    let (_tmp, store) = open_store();
    seed_session(&store, "s1", "r1", "owner/repo", 7);
    seed_finding(&store, "f1", "s1", "r1", "new", "not-drafted");

    let record = create_clarification(
        &store,
        CreateClarificationRequest {
            review_session_id: "s1",
            review_run_id: Some("r1"),
            finding_id: Some("f1"),
            source: ClarificationSource::Worker,
            body: "Why is this a bug?",
            external_ref: Some("wcr-1"),
        },
    )
    .expect("create clarification");
    assert_eq!(record.status, "open");
    assert_eq!(record.finding_id.as_deref(), Some("f1"));

    // Idempotent re-create returns the same row.
    let again = create_clarification(
        &store,
        CreateClarificationRequest {
            review_session_id: "s1",
            review_run_id: Some("r1"),
            finding_id: Some("f1"),
            source: ClarificationSource::Worker,
            body: "Why is this a bug?",
            external_ref: Some("wcr-1"),
        },
    )
    .expect("re-create clarification");
    assert_eq!(again.id, record.id);

    let listed = store
        .list_clarification_requests(ClarificationRequestQuery {
            review_session_id: Some("s1"),
            status: Some("open"),
            limit: 50,
        })
        .expect("list");
    assert_eq!(listed.len(), 1);

    let resolved = store
        .resolve_clarification_request(&record.id, "operator")
        .expect("resolve");
    assert_eq!(resolved.status, "resolved");
    assert_eq!(resolved.resolution_actor.as_deref(), Some("operator"));

    let open_after = store
        .list_clarification_requests(ClarificationRequestQuery {
            review_session_id: Some("s1"),
            status: Some("open"),
            limit: 50,
        })
        .expect("list open");
    assert!(open_after.is_empty());
}

#[test]
fn clarification_rejects_empty_body() {
    let (_tmp, store) = open_store();
    seed_session(&store, "s1", "r1", "owner/repo", 7);
    let err = create_clarification(
        &store,
        CreateClarificationRequest {
            review_session_id: "s1",
            review_run_id: None,
            finding_id: None,
            source: ClarificationSource::Operator,
            body: "   ",
            external_ref: None,
        },
    )
    .expect_err("empty body");
    assert!(matches!(err, ReviewOpError::Invalid(_)));
}

// --- queue_rows -------------------------------------------------------------

/// In-test lister: keeps `queue_rows` off `gh` and off the network.
struct FakeLister {
    prs: Vec<roger_app_core::OpenPullRequestSummary>,
    error: Option<roger_app_core::OpenPullRequestListError>,
}

impl FakeLister {
    fn with(prs: Vec<u64>) -> Self {
        Self {
            prs: prs
                .into_iter()
                .map(|number| roger_app_core::OpenPullRequestSummary {
                    number,
                    title: format!("PR {number}"),
                    author: "octocat".to_owned(),
                    is_draft: false,
                    head_ref: "feature".to_owned(),
                    updated_at: "2026-07-10T00:00:00Z".to_owned(),
                    url: format!("https://github.com/example/repo/pull/{number}"),
                })
                .collect(),
            error: None,
        }
    }
}

impl roger_app_core::OpenPullRequestLister for FakeLister {
    fn list_open_pull_requests(
        &self,
        _owner: &str,
        _repo: &str,
        limit: usize,
    ) -> std::result::Result<
        Vec<roger_app_core::OpenPullRequestSummary>,
        roger_app_core::OpenPullRequestListError,
    > {
        if let Some(err) = &self.error {
            return Err(err.clone());
        }
        let mut prs = self.prs.clone();
        prs.truncate(limit);
        Ok(prs)
    }
}

#[test]
fn queue_rows_marks_pr_without_session_as_fresh_and_one_with_session_as_reuse() {
    let (_temp, store) = open_store();
    seed_session(&store, "sess-9", "run-9", "example/repo", 9);
    let lister = FakeLister::with(vec![7, 9]);

    let rows = queue_rows(&store, &lister, "example/repo", 25).expect("queue rows");

    assert_eq!(rows.len(), 2);
    let fresh = rows.iter().find(|r| r.pr_number == 7).expect("pr 7");
    assert!(fresh.is_fresh(), "PR with no local session must be fresh");
    assert_eq!(fresh.roger_state, "not_started");
    assert!(fresh.next_command.contains("rr review --pr 7"));

    let existing = rows.iter().find(|r| r.pr_number == 9).expect("pr 9");
    assert!(
        !existing.is_fresh(),
        "PR with a local session must reuse it, not start fresh"
    );
    assert_eq!(existing.session_id.as_deref(), Some("sess-9"));
    assert_eq!(existing.next_command, "rr resume --pr 9");
}

#[test]
fn queue_rows_rejects_a_non_owner_repo_slug_rather_than_guessing() {
    let (_temp, store) = open_store();
    let lister = FakeLister::with(vec![1]);

    let err = queue_rows(&store, &lister, "not-a-slug", 25).expect_err("must reject");

    assert_eq!(
        err,
        QueueRejection::RepositorySlugInvalid("not-a-slug".to_owned())
    );
}

#[test]
fn queue_rows_surfaces_gh_unavailability_as_a_distinct_rejection() {
    let (_temp, store) = open_store();
    let mut lister = FakeLister::with(vec![1]);
    lister.error = Some(roger_app_core::OpenPullRequestListError::GhUnavailable(
        "gh CLI not found".to_owned(),
    ));

    let err = queue_rows(&store, &lister, "example/repo", 25).expect_err("must reject");

    assert!(matches!(err, QueueRejection::GhUnavailable(_)));
}
