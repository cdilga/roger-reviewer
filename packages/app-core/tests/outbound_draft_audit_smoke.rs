use roger_app_core::{
    DraftRefreshSignal, OutboundApprovalToken, OutboundDraft, OutboundDraftBatch,
    OutboundPostGateDecision, OutboundPostGateInput, PostedAction, evaluate_outbound_post_gate,
    outbound_target_tuple_json, validate_outbound_draft_batch_linkage,
};
use serde::Deserialize;
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
    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read fixture {}: {err}", path.display()));
    serde_json::from_str(&raw)
        .unwrap_or_else(|err| panic!("failed to decode fixture {}: {err}", path.display()))
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
