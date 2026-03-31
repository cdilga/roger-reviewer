use roger_app_core::{
    DraftRefreshSignal, ExplicitPostingInput, ExplicitPostingOutcome, OutboundApprovalToken,
    OutboundDraft, OutboundDraftBatch, OutboundPostGateDecision, OutboundPostGateInput,
    OutboundPostingAdapter, PostedAction, PostedActionStatus, PostingAdapterItemResult,
    PostingAdapterItemStatus, evaluate_outbound_post_gate, execute_explicit_posting_flow,
    outbound_target_tuple_json, validate_outbound_draft_batch_linkage,
};
use serde::Deserialize;
use std::cell::Cell;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
struct OutboundAuditFixture {
    valid_batch: OutboundDraftBatch,
    valid_draft: OutboundDraft,
    invalid_draft: OutboundDraft,
    valid_approval: OutboundApprovalToken,
    invalidate_signal: DraftRefreshSignal,
    reconfirm_signal: DraftRefreshSignal,
    posted_action: PostedAction,
}

fn load_fixture() -> OutboundAuditFixture {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/fixture_github_draft_batch/outbound_audit_case.json");
    let raw = fs::read_to_string(&path).expect("failed to read outbound audit fixture");
    serde_json::from_str(&raw).expect("failed to decode outbound audit fixture")
}

#[derive(Clone, Debug)]
struct StubPostingAdapter {
    result: std::result::Result<Vec<PostingAdapterItemResult>, String>,
    calls: Cell<u32>,
}

impl StubPostingAdapter {
    fn succeeds(items: Vec<PostingAdapterItemResult>) -> Self {
        Self {
            result: Ok(items),
            calls: Cell::new(0),
        }
    }

    fn fails(reason: &str) -> Self {
        Self {
            result: Err(reason.to_owned()),
            calls: Cell::new(0),
        }
    }
}

impl OutboundPostingAdapter for StubPostingAdapter {
    fn post_approved_draft_batch(
        &self,
        _batch: &OutboundDraftBatch,
        _drafts: &[OutboundDraft],
    ) -> std::result::Result<Vec<PostingAdapterItemResult>, String> {
        self.calls.set(self.calls.get() + 1);
        self.result.clone()
    }
}

#[test]
fn linkage_validation_reports_scope_target_payload_and_finding_issues() {
    let fixture = load_fixture();

    let validation =
        validate_outbound_draft_batch_linkage(&fixture.valid_batch, &[fixture.invalid_draft]);
    assert!(!validation.valid);

    let reason_codes: HashSet<&str> = validation
        .issues
        .iter()
        .map(|issue| issue.reason_code.as_str())
        .collect();

    assert!(reason_codes.contains("session_mismatch"));
    assert!(reason_codes.contains("run_mismatch"));
    assert!(reason_codes.contains("batch_mismatch"));
    assert!(reason_codes.contains("target_mismatch"));
    assert!(reason_codes.contains("payload_digest_mismatch"));
    assert!(reason_codes.contains("missing_finding_link"));
}

#[test]
fn post_gate_allows_post_when_approval_target_and_refresh_state_match() {
    let fixture = load_fixture();
    let batch = fixture.valid_batch;
    let drafts = vec![fixture.valid_draft];
    let mut approval = fixture.valid_approval;
    approval.target_tuple_json = outbound_target_tuple_json(&batch);

    let reconfirmed_finding_ids = HashSet::new();
    let decision = evaluate_outbound_post_gate(OutboundPostGateInput {
        batch: &batch,
        drafts: &drafts,
        approval: &approval,
        refresh_signals: &[],
        reconfirmed_finding_ids: &reconfirmed_finding_ids,
    });

    assert_eq!(decision, OutboundPostGateDecision::PostAllowed);
}

#[test]
fn post_gate_blocks_on_approval_batch_payload_or_target_mismatch() {
    let fixture = load_fixture();
    let batch = fixture.valid_batch;
    let drafts = vec![fixture.valid_draft];

    let mut approval_batch_mismatch = fixture.valid_approval.clone();
    approval_batch_mismatch.target_tuple_json = outbound_target_tuple_json(&batch);
    approval_batch_mismatch.draft_batch_id = "batch-other".to_owned();
    assert_eq!(
        evaluate_outbound_post_gate(OutboundPostGateInput {
            batch: &batch,
            drafts: &drafts,
            approval: &approval_batch_mismatch,
            refresh_signals: &[],
            reconfirmed_finding_ids: &HashSet::new(),
        }),
        OutboundPostGateDecision::Blocked {
            reason_code: "approval_batch_mismatch".to_owned(),
        }
    );

    let mut approval_payload_mismatch = fixture.valid_approval.clone();
    approval_payload_mismatch.target_tuple_json = outbound_target_tuple_json(&batch);
    approval_payload_mismatch.payload_digest = "payload-other".to_owned();
    assert_eq!(
        evaluate_outbound_post_gate(OutboundPostGateInput {
            batch: &batch,
            drafts: &drafts,
            approval: &approval_payload_mismatch,
            refresh_signals: &[],
            reconfirmed_finding_ids: &HashSet::new(),
        }),
        OutboundPostGateDecision::Blocked {
            reason_code: "approval_payload_digest_mismatch".to_owned(),
        }
    );

    let mut approval_target_mismatch = fixture.valid_approval;
    approval_target_mismatch.target_tuple_json = "{\"review_session_id\":\"other\"}".to_owned();
    assert_eq!(
        evaluate_outbound_post_gate(OutboundPostGateInput {
            batch: &batch,
            drafts: &drafts,
            approval: &approval_target_mismatch,
            refresh_signals: &[],
            reconfirmed_finding_ids: &HashSet::new(),
        }),
        OutboundPostGateDecision::Blocked {
            reason_code: "approval_target_tuple_mismatch".to_owned(),
        }
    );
}

#[test]
fn post_gate_blocks_revoked_or_invalidated_approval_state() {
    let fixture = load_fixture();
    let mut batch = fixture.valid_batch;
    let drafts = vec![fixture.valid_draft];

    let mut revoked_approval = fixture.valid_approval.clone();
    revoked_approval.target_tuple_json = outbound_target_tuple_json(&batch);
    revoked_approval.revoked_at = Some(1_700_000_999);
    assert_eq!(
        evaluate_outbound_post_gate(OutboundPostGateInput {
            batch: &batch,
            drafts: &drafts,
            approval: &revoked_approval,
            refresh_signals: &[],
            reconfirmed_finding_ids: &HashSet::new(),
        }),
        OutboundPostGateDecision::Blocked {
            reason_code: "approval_revoked".to_owned(),
        }
    );

    batch.invalidated_at = Some(1_700_001_111);
    batch.invalidation_reason_code = Some("worktree_changed".to_owned());
    let mut active_approval = fixture.valid_approval;
    active_approval.target_tuple_json = outbound_target_tuple_json(&batch);
    assert_eq!(
        evaluate_outbound_post_gate(OutboundPostGateInput {
            batch: &batch,
            drafts: &drafts,
            approval: &active_approval,
            refresh_signals: &[],
            reconfirmed_finding_ids: &HashSet::new(),
        }),
        OutboundPostGateDecision::Blocked {
            reason_code: "approval_invalidated:worktree_changed".to_owned(),
        }
    );
}

#[test]
fn post_gate_blocks_refresh_invalidation_and_enforces_reconfirmation() {
    let fixture = load_fixture();
    let batch = fixture.valid_batch;
    let drafts = vec![fixture.valid_draft];
    let mut approval = fixture.valid_approval;
    approval.target_tuple_json = outbound_target_tuple_json(&batch);

    let invalidation_signals = vec![fixture.invalidate_signal];
    assert_eq!(
        evaluate_outbound_post_gate(OutboundPostGateInput {
            batch: &batch,
            drafts: &drafts,
            approval: &approval,
            refresh_signals: &invalidation_signals,
            reconfirmed_finding_ids: &HashSet::new(),
        }),
        OutboundPostGateDecision::Blocked {
            reason_code: "refresh_invalidated:target_rebased".to_owned(),
        }
    );

    let reconfirm_signals = vec![fixture.reconfirm_signal];
    assert_eq!(
        evaluate_outbound_post_gate(OutboundPostGateInput {
            batch: &batch,
            drafts: &drafts,
            approval: &approval,
            refresh_signals: &reconfirm_signals,
            reconfirmed_finding_ids: &HashSet::new(),
        }),
        OutboundPostGateDecision::Blocked {
            reason_code: "reconfirmation_required:finding-1".to_owned(),
        }
    );

    let reconfirmed_finding_ids = HashSet::from(["finding-1".to_owned()]);
    assert_eq!(
        evaluate_outbound_post_gate(OutboundPostGateInput {
            batch: &batch,
            drafts: &drafts,
            approval: &approval,
            refresh_signals: &reconfirm_signals,
            reconfirmed_finding_ids: &reconfirmed_finding_ids,
        }),
        OutboundPostGateDecision::PostAllowed
    );
}

#[test]
fn explicit_posting_executes_only_after_gate_revalidation() {
    let fixture = load_fixture();
    let batch = fixture.valid_batch;
    let draft = fixture.valid_draft;
    let mut approval = fixture.valid_approval;
    approval.target_tuple_json = outbound_target_tuple_json(&batch);

    let adapter = StubPostingAdapter::succeeds(vec![PostingAdapterItemResult {
        draft_id: draft.id.clone(),
        status: PostingAdapterItemStatus::Posted,
        remote_identifier: Some("https://api.github.com/reviews/123".to_owned()),
        failure_code: None,
    }]);

    let result = execute_explicit_posting_flow(
        ExplicitPostingInput {
            action_id: "posted-action-1",
            provider: "github",
            batch: &batch,
            drafts: std::slice::from_ref(&draft),
            approval: &approval,
            refresh_signals: &[],
            reconfirmed_finding_ids: &HashSet::new(),
        },
        &adapter,
    );

    assert_eq!(adapter.calls.get(), 1);
    assert_eq!(result.outcome, ExplicitPostingOutcome::Posted);
    assert!(result.retry_draft_ids.is_empty());
    let posted = result.posted_action.expect("posted action should exist");
    assert_eq!(posted.status, PostedActionStatus::Succeeded);
    assert_eq!(posted.posted_payload_digest, batch.payload_digest);
    assert!(posted.remote_identifier.contains("/reviews/123"));
}

#[test]
fn explicit_posting_fails_closed_without_adapter_call_when_gate_blocks() {
    let fixture = load_fixture();
    let mut batch = fixture.valid_batch;
    batch.invalidated_at = Some(1_700_111_111);
    batch.invalidation_reason_code = Some("refresh_target_changed".to_owned());
    let draft = fixture.valid_draft;
    let mut approval = fixture.valid_approval;
    approval.target_tuple_json = outbound_target_tuple_json(&batch);

    let adapter = StubPostingAdapter::succeeds(vec![PostingAdapterItemResult {
        draft_id: draft.id.clone(),
        status: PostingAdapterItemStatus::Posted,
        remote_identifier: Some("https://api.github.com/reviews/123".to_owned()),
        failure_code: None,
    }]);

    let result = execute_explicit_posting_flow(
        ExplicitPostingInput {
            action_id: "posted-action-blocked",
            provider: "github",
            batch: &batch,
            drafts: std::slice::from_ref(&draft),
            approval: &approval,
            refresh_signals: &[],
            reconfirmed_finding_ids: &HashSet::new(),
        },
        &adapter,
    );

    assert_eq!(adapter.calls.get(), 0);
    assert_eq!(result.outcome, ExplicitPostingOutcome::Blocked);
    assert!(
        result
            .reason_code
            .as_deref()
            .is_some_and(|reason| reason.contains("approval_invalidated"))
    );
    assert!(result.posted_action.is_none());
}

#[test]
fn explicit_posting_fails_closed_when_local_draft_state_is_missing() {
    let fixture = load_fixture();
    let batch = fixture.valid_batch;
    let mut approval = fixture.valid_approval;
    approval.target_tuple_json = outbound_target_tuple_json(&batch);
    let drafts: Vec<OutboundDraft> = Vec::new();

    let adapter = StubPostingAdapter::succeeds(Vec::new());

    let result = execute_explicit_posting_flow(
        ExplicitPostingInput {
            action_id: "posted-action-missing-local-state",
            provider: "github",
            batch: &batch,
            drafts: &drafts,
            approval: &approval,
            refresh_signals: &[],
            reconfirmed_finding_ids: &HashSet::new(),
        },
        &adapter,
    );

    assert_eq!(adapter.calls.get(), 0);
    assert_eq!(result.outcome, ExplicitPostingOutcome::Blocked);
    assert_eq!(result.reason_code.as_deref(), Some("batch_linkage_invalid"));
    assert!(result.posted_action.is_none());
}

#[test]
fn explicit_posting_records_partial_outcome_and_retry_candidates() {
    let fixture = load_fixture();
    let batch = fixture.valid_batch;
    let draft_one = fixture.valid_draft;
    let mut draft_two = draft_one.clone();
    draft_two.id = "draft-2".to_owned();
    draft_two.finding_id = Some("finding-2".to_owned());
    draft_two.anchor_digest = "anchor-2".to_owned();
    let drafts = vec![draft_one.clone(), draft_two.clone()];
    let mut approval = fixture.valid_approval;
    approval.target_tuple_json = outbound_target_tuple_json(&batch);

    let adapter = StubPostingAdapter::succeeds(vec![
        PostingAdapterItemResult {
            draft_id: draft_one.id.clone(),
            status: PostingAdapterItemStatus::Posted,
            remote_identifier: Some("https://api.github.com/reviews/123".to_owned()),
            failure_code: None,
        },
        PostingAdapterItemResult {
            draft_id: draft_two.id.clone(),
            status: PostingAdapterItemStatus::Failed,
            remote_identifier: None,
            failure_code: Some("thread_not_found".to_owned()),
        },
    ]);

    let result = execute_explicit_posting_flow(
        ExplicitPostingInput {
            action_id: "posted-action-partial",
            provider: "github",
            batch: &batch,
            drafts: &drafts,
            approval: &approval,
            refresh_signals: &[],
            reconfirmed_finding_ids: &HashSet::new(),
        },
        &adapter,
    );

    assert_eq!(result.outcome, ExplicitPostingOutcome::Partial);
    assert_eq!(result.retry_draft_ids, vec!["draft-2".to_owned()]);
    let posted = result.posted_action.expect("posted action");
    assert_eq!(posted.status, PostedActionStatus::Partial);
    assert_eq!(posted.failure_code.as_deref(), Some("partial_failure"));
    assert!(posted.remote_identifier.contains("/reviews/123"));
}

#[test]
fn explicit_posting_records_failed_attempt_for_adapter_errors() {
    let fixture = load_fixture();
    let batch = fixture.valid_batch;
    let draft = fixture.valid_draft;
    let mut approval = fixture.valid_approval;
    approval.target_tuple_json = outbound_target_tuple_json(&batch);

    let adapter = StubPostingAdapter::fails("network_timeout");

    let result = execute_explicit_posting_flow(
        ExplicitPostingInput {
            action_id: "posted-action-error",
            provider: "github",
            batch: &batch,
            drafts: std::slice::from_ref(&draft),
            approval: &approval,
            refresh_signals: &[],
            reconfirmed_finding_ids: &HashSet::new(),
        },
        &adapter,
    );

    assert_eq!(result.outcome, ExplicitPostingOutcome::Failed);
    assert!(
        result
            .reason_code
            .as_deref()
            .is_some_and(|reason| reason.contains("adapter_error:network_timeout"))
    );
    assert_eq!(result.retry_draft_ids, vec![draft.id.clone()]);
    let posted = result.posted_action.expect("failed posted action");
    assert_eq!(posted.status, PostedActionStatus::Failed);
}

#[test]
fn explicit_posting_fails_closed_on_invalid_adapter_result_shape() {
    let fixture = load_fixture();
    let batch = fixture.valid_batch;
    let draft_one = fixture.valid_draft;
    let mut draft_two = draft_one.clone();
    draft_two.id = "draft-2".to_owned();
    draft_two.finding_id = Some("finding-2".to_owned());
    draft_two.anchor_digest = "anchor-2".to_owned();
    let drafts = vec![draft_one.clone(), draft_two.clone()];
    let mut approval = fixture.valid_approval;
    approval.target_tuple_json = outbound_target_tuple_json(&batch);

    // Missing result for draft-2 should fail closed.
    let adapter = StubPostingAdapter::succeeds(vec![PostingAdapterItemResult {
        draft_id: draft_one.id.clone(),
        status: PostingAdapterItemStatus::Posted,
        remote_identifier: Some("https://api.github.com/reviews/123".to_owned()),
        failure_code: None,
    }]);

    let result = execute_explicit_posting_flow(
        ExplicitPostingInput {
            action_id: "posted-action-invalid",
            provider: "github",
            batch: &batch,
            drafts: &drafts,
            approval: &approval,
            refresh_signals: &[],
            reconfirmed_finding_ids: &HashSet::new(),
        },
        &adapter,
    );

    assert_eq!(result.outcome, ExplicitPostingOutcome::Failed);
    assert!(
        result
            .reason_code
            .as_deref()
            .is_some_and(|reason| reason.contains("adapter_result_invalid"))
    );
    assert_eq!(result.retry_draft_ids, vec![draft_one.id, draft_two.id]);
    let posted = result.posted_action.expect("failed posted action");
    assert_eq!(posted.status, PostedActionStatus::Failed);
}

#[test]
fn posted_action_snapshot_round_trips_for_audit_lineage() {
    let fixture = load_fixture();

    let encoded = serde_json::to_string(&fixture.posted_action).expect("serialize posted action");
    let decoded: PostedAction = serde_json::from_str(&encoded).expect("deserialize posted action");

    assert_eq!(decoded.id, fixture.posted_action.id);
    assert_eq!(decoded.draft_batch_id, fixture.posted_action.draft_batch_id);
    assert_eq!(decoded.provider, "github");
    assert!(decoded.remote_identifier.contains("/pulls/comments/123"));
    assert_eq!(decoded.posted_payload_digest, "payload-aaa");
    assert_eq!(decoded.failure_code.as_deref(), Some("partial_failure"));
}
