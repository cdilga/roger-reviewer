//! Shared review-domain operations (ports-and-adapters application layer).
//!
//! These functions are the single home for Roger's fail-closed review-domain
//! rules — draft materialization, triage, approval, posting, memory resolution,
//! and clarification. The CLI, the TUI backend, and the browser bridge are all
//! adapters that call these operations; none of them reimplements a fail-closed
//! check. Each operation takes a `&RogerStore` plus an already-resolved session
//! (the caller owns session resolution / picker routing) and returns a typed
//! outcome or a typed rejection. A rejection carries the same `reason_code` and
//! discriminating data every surface needs; each surface renders it in its own
//! idiom (the CLI's robot envelopes, the TUI's elevation prompts, the
//! extension's bounded read/handoff surfaces).

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};

use roger_app_core::{
    ApprovalState, ExplicitPostingInput, ExplicitPostingOutcome, ExplicitPostingResult,
    OutboundApprovalToken, OutboundDraft, OutboundDraftBatch, OutboundDraftBatchValidation,
    OutboundPostingAdapter, ReviewTarget, execute_explicit_posting_flow,
    outbound_target_tuple_json, time, validate_outbound_draft_batch_linkage,
};
use roger_storage::{
    ClarificationRequestRecord, CreateClarificationRequest, MaterializedFindingRecord,
    MemoryReviewDecision, MemoryReviewResolutionOutcome,
    OUTBOUND_APPROVAL_REVOKED_REASON_DRAFT_REVISED, ReviewSessionRecord, RogerStore, StorageError,
};
use serde::Serialize;
use sha2::{Digest, Sha256};

// Re-export the storage clarification input/record types so callers of
// `create_clarification` have a single import surface.
pub use roger_storage::{
    ClarificationRequestQuery, ClarificationSource, MemoryReviewDecision as ReviewMemoryDecision,
};

/// Operator-settable triage states. `new` and `stale` are Roger-derived and are
/// rejected here — the single source of truth every surface validates against.
pub const TRIAGE_OPERATOR_STATES: &[&str] = &["accepted", "ignored", "needs_follow_up", "resolved"];

/// True when `state` is an operator-settable triage state.
pub fn is_supported_triage_state(state: &str) -> bool {
    TRIAGE_OPERATOR_STATES.contains(&state)
}

static ID_SEQ: AtomicU64 = AtomicU64::new(1);

/// Mint a unique, process-local id with the same shape the CLI used before this
/// logic was extracted (`{prefix}-{ts}-{pid}-{seq}`). Ids are intentionally
/// non-deterministic; callers must not assert their exact value.
fn next_id(prefix: &str) -> String {
    let seq = ID_SEQ.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    format!("{prefix}-{}-{pid}-{seq}", time::now_ts())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut text = String::with_capacity(digest.len() * 2);
    for byte in digest {
        text.push_str(&format!("{byte:02x}"));
    }
    text
}

fn sha256_prefixed_json<T: Serialize>(value: &T) -> std::result::Result<String, String> {
    serde_json::to_vec(value)
        .map(|bytes| format!("sha256:{}", sha256_hex(&bytes)))
        .map_err(|err| format!("failed to serialize draft payload: {err}"))
}

/// The exact local outbound draft body Roger materializes for a finding.
pub fn render_local_outbound_draft_body(finding: &MaterializedFindingRecord) -> String {
    format!(
        "Finding: {}\n\nSummary:\n{}\n\nSeverity: {}\nConfidence: {}\nFingerprint: {}",
        finding.title,
        finding.normalized_summary,
        finding.severity,
        finding.confidence,
        finding.fingerprint
    )
}

/// The anchor digest that binds a draft to its finding evidence.
pub fn outbound_draft_anchor_digest(
    finding: &MaterializedFindingRecord,
) -> std::result::Result<String, String> {
    sha256_prefixed_json(&serde_json::json!({
        "finding_id": finding.id,
        "fingerprint": finding.fingerprint,
        "title": finding.title,
        "normalized_summary": finding.normalized_summary,
    }))
}

/// The stable target locator for a finding's outbound draft.
pub fn outbound_draft_target_locator(target: &ReviewTarget, finding_id: &str) -> String {
    let _ = finding_id;
    format!(
        "github:issue-comment:{}#{}",
        target.repository, target.pull_request_number
    )
}

// ---------------------------------------------------------------------------
// Shared session precondition
// ---------------------------------------------------------------------------

/// A fail-closed session precondition that blocks any outbound-mutating review
/// op before it touches local batch state. Every surface renders these; the
/// `reason_code` each maps to is stable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionPreconditionBlock {
    /// Persisted attention state requires explicit reconciliation
    /// (`reason_code = stale_local_state`).
    StaleLocalState,
    /// The session has no concrete review target
    /// (`reason_code = missing_review_target`).
    MissingReviewTarget,
    /// No persisted local review run exists yet
    /// (`reason_code = missing_local_state`).
    MissingLocalStateNoRun,
}

enum ActiveRunError {
    Blocked(SessionPreconditionBlock),
    Failed(String),
}

fn resolve_active_run(
    store: &RogerStore,
    session: &ReviewSessionRecord,
) -> std::result::Result<roger_storage::ReviewRunRecord, ActiveRunError> {
    if session.attention_state == "refresh_recommended" {
        return Err(ActiveRunError::Blocked(
            SessionPreconditionBlock::StaleLocalState,
        ));
    }
    if session.review_target.repository.trim().is_empty()
        || session.review_target.pull_request_number == 0
    {
        return Err(ActiveRunError::Blocked(
            SessionPreconditionBlock::MissingReviewTarget,
        ));
    }
    match store.latest_review_run(&session.id) {
        Ok(Some(run)) => Ok(run),
        Ok(None) => Err(ActiveRunError::Blocked(
            SessionPreconditionBlock::MissingLocalStateNoRun,
        )),
        Err(err) => Err(ActiveRunError::Failed(format!(
            "failed to load latest run: {err}"
        ))),
    }
}

// ---------------------------------------------------------------------------
// 1. materialize_draft_batch
// ---------------------------------------------------------------------------

/// Which findings a draft batch should group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DraftSelection {
    /// Every finding in the latest run.
    AllFindings,
    /// An explicit set of finding ids.
    Explicit(Vec<String>),
}

/// Serializable per-item view of a materialized draft. The field names feed the
/// batch payload digest, so they are part of the domain contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DraftItemView {
    pub finding_id: String,
    pub fingerprint: String,
    pub title: String,
    pub normalized_summary: String,
    pub severity: String,
    pub confidence: String,
    pub anchor_digest: String,
    pub target_locator: String,
    pub body: String,
}

/// Why a selected finding cannot be drafted from the current local state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DraftSelectionIssue {
    /// Finding is not `accepted` (`reason_code = triage_state_not_accepted`).
    TriageStateNotAccepted {
        finding_id: String,
        triage_state: String,
    },
    /// Finding already has outbound state (`reason_code = existing_outbound_state`).
    ExistingOutboundState {
        finding_id: String,
        current_outbound_state: String,
        draft_id: Option<String>,
        draft_batch_id: Option<String>,
        approval_id: Option<String>,
        posted_action_id: Option<String>,
    },
}

/// Successful materialization of a draft batch.
#[derive(Debug, Clone)]
pub struct MaterializeDraftOutcome {
    pub review_run_id: String,
    /// `"all_findings"` or `"explicit_findings"`.
    pub selection_mode: &'static str,
    /// Selected finding ids, sorted/deduped (batch item order).
    pub selected_finding_ids: Vec<String>,
    pub batch: OutboundDraftBatch,
    /// Stored draft rows, parallel to `previews`.
    pub drafts: Vec<OutboundDraft>,
    /// Per-item digest-bearing views, parallel to `drafts`.
    pub previews: Vec<DraftItemView>,
}

/// Fail-closed rejections of `materialize_draft_batch`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaterializeDraftRejection {
    Precondition(SessionPreconditionBlock),
    /// The latest run has no findings (`reason_code = missing_local_state`).
    MissingFindings {
        review_run_id: String,
    },
    /// No selection and not `--all-findings` (`reason_code = finding_selection_required`).
    FindingSelectionRequired {
        review_run_id: String,
        available_finding_ids: Vec<String>,
    },
    /// Explicit selection referenced unknown findings
    /// (`reason_code = missing_local_state`).
    MissingFindingSelection {
        review_run_id: String,
        missing_finding_ids: Vec<String>,
    },
    /// One or more selected findings are not draftable
    /// (`reason_code = stale_local_state`).
    SelectionNotDraftable {
        review_run_id: String,
        issues: Vec<DraftSelectionIssue>,
    },
    /// Internal failure the CLI renders as an error envelope.
    Failed(String),
}

/// Materialize an `OutboundDraftBatch` + `OutboundDraft` rows from a resolved
/// session and a finding selection, applying the same accepted-only / not-drafted
/// / target-binding rules as the CLI's `rr draft`. The batch/item digests, target
/// binding, and stored rows are byte-identical to the legacy CLI handler.
pub fn materialize_draft_batch(
    store: &RogerStore,
    session: &ReviewSessionRecord,
    selection: &DraftSelection,
) -> std::result::Result<MaterializeDraftOutcome, MaterializeDraftRejection> {
    let run = match resolve_active_run(store, session) {
        Ok(run) => run,
        Err(ActiveRunError::Blocked(block)) => {
            return Err(MaterializeDraftRejection::Precondition(block));
        }
        Err(ActiveRunError::Failed(msg)) => return Err(MaterializeDraftRejection::Failed(msg)),
    };

    let findings = store
        .materialized_findings_for_run(&session.id, &run.id)
        .map_err(|err| {
            MaterializeDraftRejection::Failed(format!(
                "failed to load findings for latest run: {err}"
            ))
        })?;

    if findings.is_empty() {
        return Err(MaterializeDraftRejection::MissingFindings {
            review_run_id: run.id.clone(),
        });
    }

    let explicit_ids: Option<&[String]> = match selection {
        DraftSelection::AllFindings => None,
        DraftSelection::Explicit(ids) => Some(ids.as_slice()),
    };

    if let Some(ids) = explicit_ids
        && ids.is_empty()
    {
        return Err(MaterializeDraftRejection::FindingSelectionRequired {
            review_run_id: run.id.clone(),
            available_finding_ids: findings.iter().map(|f| f.id.clone()).collect(),
        });
    }

    let selection_mode = match selection {
        DraftSelection::AllFindings => "all_findings",
        DraftSelection::Explicit(_) => "explicit_findings",
    };

    let mut selected_findings = match selection {
        DraftSelection::AllFindings => findings.clone(),
        DraftSelection::Explicit(ids) => {
            let by_id: std::collections::HashMap<&str, &MaterializedFindingRecord> =
                findings.iter().map(|f| (f.id.as_str(), f)).collect();
            let mut selected = Vec::new();
            let mut missing = Vec::new();
            for finding_id in ids {
                match by_id.get(finding_id.as_str()) {
                    Some(finding) => selected.push((*finding).clone()),
                    None => missing.push(finding_id.clone()),
                }
            }
            if !missing.is_empty() {
                return Err(MaterializeDraftRejection::MissingFindingSelection {
                    review_run_id: run.id.clone(),
                    missing_finding_ids: missing,
                });
            }
            selected
        }
    };
    selected_findings.sort_by(|left, right| left.id.cmp(&right.id));
    selected_findings.dedup_by(|left, right| left.id == right.id);

    let mut issues = Vec::new();
    for finding in &selected_findings {
        let projection = store
            .outbound_surface_projection_for_finding(&finding.id, &finding.outbound_state)
            .map_err(|err| {
                MaterializeDraftRejection::Failed(format!(
                    "failed to inspect outbound state for finding {}: {err}",
                    finding.id
                ))
            })?;
        if !finding.triage_state.eq_ignore_ascii_case("accepted") {
            issues.push(DraftSelectionIssue::TriageStateNotAccepted {
                finding_id: finding.id.clone(),
                triage_state: finding.triage_state.clone(),
            });
        }
        if projection.state != "not_drafted" {
            issues.push(DraftSelectionIssue::ExistingOutboundState {
                finding_id: finding.id.clone(),
                current_outbound_state: projection.state.clone(),
                draft_id: projection.draft_id.clone(),
                draft_batch_id: projection.draft_batch_id.clone(),
                approval_id: projection.approval_id.clone(),
                posted_action_id: projection.posted_action_id.clone(),
            });
        }
    }
    if !issues.is_empty() {
        return Err(MaterializeDraftRejection::SelectionNotDraftable {
            review_run_id: run.id.clone(),
            issues,
        });
    }

    let repo_id = session.review_target.repository.clone();
    let remote_review_target_id = format!("pr-{}", session.review_target.pull_request_number);
    let mut previews = Vec::with_capacity(selected_findings.len());
    for finding in &selected_findings {
        let body = render_local_outbound_draft_body(finding);
        let anchor_digest = outbound_draft_anchor_digest(finding).map_err(|err| {
            MaterializeDraftRejection::Failed(format!(
                "failed to build draft anchor digest for finding {}: {err}",
                finding.id
            ))
        })?;
        previews.push(DraftItemView {
            finding_id: finding.id.clone(),
            fingerprint: finding.fingerprint.clone(),
            title: finding.title.clone(),
            normalized_summary: finding.normalized_summary.clone(),
            severity: finding.severity.clone(),
            confidence: finding.confidence.clone(),
            anchor_digest,
            target_locator: outbound_draft_target_locator(&session.review_target, &finding.id),
            body,
        });
    }

    let payload_digest = sha256_prefixed_json(&serde_json::json!({
        "repo_id": repo_id.clone(),
        "remote_review_target_id": remote_review_target_id.clone(),
        "drafts": previews.clone(),
    }))
    .map_err(|err| {
        MaterializeDraftRejection::Failed(format!("failed to build batch payload digest: {err}"))
    })?;

    let batch = OutboundDraftBatch {
        id: next_id("draft-batch"),
        review_session_id: session.id.clone(),
        review_run_id: run.id.clone(),
        repo_id: repo_id.clone(),
        remote_review_target_id: remote_review_target_id.clone(),
        payload_digest: payload_digest.clone(),
        approval_state: ApprovalState::Drafted,
        approved_at: None,
        invalidated_at: None,
        invalidation_reason_code: None,
        row_version: 0,
    };
    store.store_outbound_draft_batch(&batch).map_err(|err| {
        MaterializeDraftRejection::Failed(format!("failed to store outbound draft batch: {err}"))
    })?;

    let mut drafts = Vec::with_capacity(previews.len());
    for preview in &previews {
        let draft = OutboundDraft {
            id: next_id("draft"),
            review_session_id: session.id.clone(),
            review_run_id: run.id.clone(),
            finding_id: Some(preview.finding_id.clone()),
            draft_batch_id: batch.id.clone(),
            repo_id: repo_id.clone(),
            remote_review_target_id: remote_review_target_id.clone(),
            payload_digest: payload_digest.clone(),
            approval_state: ApprovalState::Drafted,
            anchor_digest: preview.anchor_digest.clone(),
            target_locator: preview.target_locator.clone(),
            body: preview.body.clone(),
            row_version: 0,
        };
        store.store_outbound_draft_item(&draft).map_err(|err| {
            MaterializeDraftRejection::Failed(format!(
                "failed to store outbound draft item for finding {}: {err}",
                preview.finding_id
            ))
        })?;
        drafts.push(draft);
    }

    Ok(MaterializeDraftOutcome {
        review_run_id: run.id,
        selection_mode,
        selected_finding_ids: selected_findings.iter().map(|f| f.id.clone()).collect(),
        batch,
        drafts,
        previews,
    })
}

// ---------------------------------------------------------------------------
// 2. set_finding_triage
// ---------------------------------------------------------------------------

/// Successful triage application.
#[derive(Debug, Clone)]
pub struct SetTriageOutcome {
    pub triage_state: String,
    pub updated_findings: Vec<MaterializedFindingRecord>,
}

/// Fail-closed rejections of `set_finding_triage`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetTriageRejection {
    /// The requested triage state is not operator-settable
    /// (`reason_code = unsupported_triage_state`).
    UnsupportedState { requested_state: String },
    /// One or more finding ids do not bind to the session
    /// (`reason_code = unknown_finding_ids`).
    UnknownFindingIds { unknown_finding_ids: Vec<String> },
    /// Internal failure the CLI renders as an error envelope.
    Failed(String),
}

/// Apply `triage_state` to `finding_ids` under the resolved session, with the
/// same validation the CLI's `rr triage` performs: reject non-operator states,
/// require every finding to bind to the session, then update each and append a
/// decision event (via the storage layer).
pub fn set_finding_triage(
    store: &RogerStore,
    session: &ReviewSessionRecord,
    finding_ids: &[String],
    triage_state: &str,
) -> std::result::Result<SetTriageOutcome, SetTriageRejection> {
    if !is_supported_triage_state(triage_state) {
        return Err(SetTriageRejection::UnsupportedState {
            requested_state: triage_state.to_owned(),
        });
    }

    let mut ids: Vec<String> = finding_ids.to_vec();
    ids.sort();
    ids.dedup();

    let mut selected = Vec::with_capacity(ids.len());
    let mut unknown = Vec::new();
    for finding_id in &ids {
        match store.materialized_finding(finding_id) {
            Ok(Some(record)) if record.session_id == session.id => selected.push(record),
            Ok(_) => unknown.push(finding_id.clone()),
            Err(err) => {
                return Err(SetTriageRejection::Failed(format!(
                    "failed to load finding {finding_id}: {err}"
                )));
            }
        }
    }
    if !unknown.is_empty() {
        return Err(SetTriageRejection::UnknownFindingIds {
            unknown_finding_ids: unknown,
        });
    }

    let mut updated = Vec::with_capacity(selected.len());
    for finding in &selected {
        match store.update_finding_triage_state(&finding.id, triage_state) {
            Ok(record) => updated.push(record),
            Err(err) => {
                return Err(SetTriageRejection::Failed(format!(
                    "failed to update triage state for finding {}: {err}",
                    finding.id
                )));
            }
        }
    }

    Ok(SetTriageOutcome {
        triage_state: triage_state.to_owned(),
        updated_findings: updated,
    })
}

// ---------------------------------------------------------------------------
// Shared batch binding (approve + post)
// ---------------------------------------------------------------------------

/// The linkage-issue category the CLI derives from a failed batch validation.
pub fn approval_invalidation_reason_for_linkage_issues(
    validation: &OutboundDraftBatchValidation,
) -> &'static str {
    if validation.issues.iter().any(|issue| {
        matches!(
            issue.reason_code.as_str(),
            "target_mismatch" | "payload_digest_mismatch"
        )
    }) {
        "target_or_payload_drift"
    } else if validation.issues.iter().any(|issue| {
        matches!(
            issue.reason_code.as_str(),
            "empty_batch" | "missing_finding_link"
        )
    }) {
        "missing_local_state"
    } else {
        "local_state_drift"
    }
}

fn awaiting_approval_batch_ids_for_run(
    store: &RogerStore,
    review_session_id: &str,
    review_run_id: &str,
) -> std::result::Result<Vec<String>, String> {
    batch_ids_in_state(store, review_session_id, review_run_id, "awaiting_approval")
}

fn approved_batch_ids_for_run(
    store: &RogerStore,
    review_session_id: &str,
    review_run_id: &str,
) -> std::result::Result<Vec<String>, String> {
    batch_ids_in_state(store, review_session_id, review_run_id, "approved")
}

fn batch_ids_in_state(
    store: &RogerStore,
    review_session_id: &str,
    review_run_id: &str,
    wanted_state: &str,
) -> std::result::Result<Vec<String>, String> {
    let findings = store
        .materialized_findings_for_run(review_session_id, review_run_id)
        .map_err(|err| format!("failed to load findings for latest run: {err}"))?;
    let mut batch_ids = Vec::new();
    for finding in findings {
        let projection = store
            .outbound_surface_projection_for_finding(&finding.id, &finding.outbound_state)
            .map_err(|err| {
                format!(
                    "failed to inspect outbound approval state for finding {}: {err}",
                    finding.id
                )
            })?;
        if projection.state == wanted_state
            && let Some(batch_id) = projection.draft_batch_id
        {
            batch_ids.push(batch_id);
        }
    }
    batch_ids.sort();
    batch_ids.dedup();
    Ok(batch_ids)
}

/// A draft item that is no longer in an approvable/postable state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftStateIssue {
    pub draft_id: String,
    pub finding_id: Option<String>,
    /// Stable domain reason code (e.g. `draft_already_posted`).
    pub reason_code: String,
}

/// A batch-binding rejection shared by approve and post. `reason_code`/data are
/// identical for both verbs; only the surface message differs.
#[derive(Debug, Clone, PartialEq, Eq)]
enum BatchBindingBlock {
    NotFound,
    SessionMismatch {
        batch_review_session_id: String,
    },
    RunMismatch {
        batch_review_run_id: String,
    },
    TargetDrift {
        expected_repo_id: String,
        expected_remote_review_target_id: String,
        stored_repo_id: String,
        stored_remote_review_target_id: String,
    },
    ExistingPostedAction {
        posted_action_id: String,
        posted_action_status: String,
        failure_code: Option<String>,
    },
    MissingDraftItems,
    LinkageInvalid {
        reason_suffix: &'static str,
        issues: Vec<(Option<String>, String)>,
    },
    BatchInvalidated {
        invalidation_reason_code: Option<String>,
        invalidated_at: Option<i64>,
    },
}

enum BatchBindingError {
    Blocked(BatchBindingBlock),
    Failed(String),
}

/// Load a batch by id and enforce every binding check the CLI applies before
/// approve/post touch it (session/run binding, target binding, prior posted
/// action, non-empty drafts, linkage validity, non-invalidated). Returns the
/// batch and its draft items.
fn load_bound_batch(
    store: &RogerStore,
    session: &ReviewSessionRecord,
    run_id: &str,
    batch_id: &str,
) -> std::result::Result<(OutboundDraftBatch, Vec<OutboundDraft>), BatchBindingError> {
    let batch = store
        .outbound_draft_batch(batch_id)
        .map_err(|err| {
            BatchBindingError::Failed(format!("failed to load outbound draft batch: {err}"))
        })?
        .ok_or(BatchBindingError::Blocked(BatchBindingBlock::NotFound))?;

    if batch.review_session_id != session.id {
        return Err(BatchBindingError::Blocked(
            BatchBindingBlock::SessionMismatch {
                batch_review_session_id: batch.review_session_id.clone(),
            },
        ));
    }
    if batch.review_run_id != run_id {
        return Err(BatchBindingError::Blocked(BatchBindingBlock::RunMismatch {
            batch_review_run_id: batch.review_run_id.clone(),
        }));
    }

    let expected_remote_review_target_id =
        format!("pr-{}", session.review_target.pull_request_number);
    if batch.repo_id != session.review_target.repository
        || batch.remote_review_target_id != expected_remote_review_target_id
    {
        return Err(BatchBindingError::Blocked(BatchBindingBlock::TargetDrift {
            expected_repo_id: session.review_target.repository.clone(),
            expected_remote_review_target_id,
            stored_repo_id: batch.repo_id.clone(),
            stored_remote_review_target_id: batch.remote_review_target_id.clone(),
        }));
    }

    let posted_actions = store.posted_actions_for_batch(&batch.id).map_err(|err| {
        BatchBindingError::Failed(format!("failed to inspect prior posted actions: {err}"))
    })?;
    if let Some(latest) = posted_actions.last() {
        return Err(BatchBindingError::Blocked(
            BatchBindingBlock::ExistingPostedAction {
                posted_action_id: latest.id.clone(),
                posted_action_status: format!("{:?}", latest.status),
                failure_code: latest.failure_code.clone(),
            },
        ));
    }

    let drafts = store
        .outbound_draft_items_for_batch(&batch.id)
        .map_err(|err| {
            BatchBindingError::Failed(format!("failed to load outbound draft items: {err}"))
        })?;
    if drafts.is_empty() {
        return Err(BatchBindingError::Blocked(
            BatchBindingBlock::MissingDraftItems,
        ));
    }

    let validation = validate_outbound_draft_batch_linkage(&batch, &drafts);
    if !validation.valid {
        return Err(BatchBindingError::Blocked(
            BatchBindingBlock::LinkageInvalid {
                reason_suffix: approval_invalidation_reason_for_linkage_issues(&validation),
                issues: validation
                    .issues
                    .iter()
                    .map(|issue| (issue.draft_id.clone(), issue.reason_code.clone()))
                    .collect(),
            },
        ));
    }

    if batch.invalidated_at.is_some() || matches!(&batch.approval_state, ApprovalState::Invalidated)
    {
        return Err(BatchBindingError::Blocked(
            BatchBindingBlock::BatchInvalidated {
                invalidation_reason_code: batch.invalidation_reason_code.clone(),
                invalidated_at: batch.invalidated_at,
            },
        ));
    }

    Ok((batch, drafts))
}

// ---------------------------------------------------------------------------
// 3. approve_batch
// ---------------------------------------------------------------------------

/// Successful approval of a batch.
#[derive(Debug, Clone)]
pub struct ApproveOutcome {
    pub review_run_id: String,
    pub batch: OutboundDraftBatch,
    pub drafts: Vec<OutboundDraft>,
    pub approval: OutboundApprovalToken,
    pub expected_target_tuple_json: String,
    /// True when the batch was already Approved before this call.
    pub batch_already_approved: bool,
    /// True when this call recorded a fresh approval (`!batch_already_approved`).
    pub approval_created: bool,
}

/// Fail-closed rejections of `approve_batch`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApproveRejection {
    Precondition(SessionPreconditionBlock),
    /// No batch id supplied (`reason_code = draft_batch_selection_required`).
    BatchSelectionRequired {
        review_run_id: String,
        available_batch_ids: Vec<String>,
    },
    /// Batch id not found (`reason_code = missing_local_state`).
    BatchNotFound {
        review_run_id: String,
        draft_batch_id: String,
        available_batch_ids: Vec<String>,
    },
    /// Batch belongs to another session (`reason_code = approval_invalidated:local_state_drift`).
    SessionMismatch {
        review_run_id: String,
        draft_batch_id: String,
        batch_review_session_id: String,
    },
    /// Batch belongs to an older run (`reason_code = approval_invalidated:local_state_drift`).
    RunMismatch {
        latest_review_run_id: String,
        draft_batch_id: String,
        batch_review_run_id: String,
    },
    /// Stored target no longer matches (`reason_code = approval_invalidated:target_drift`).
    TargetDrift {
        draft_batch_id: String,
        expected_repo_id: String,
        expected_remote_review_target_id: String,
        stored_repo_id: String,
        stored_remote_review_target_id: String,
    },
    /// A post attempt was already recorded (`reason_code = existing_posted_action`).
    ExistingPostedAction {
        draft_batch_id: String,
        posted_action_id: String,
        posted_action_status: String,
        failure_code: Option<String>,
    },
    /// No draft items for the batch (`reason_code = missing_local_state`).
    MissingDraftItems {
        draft_batch_id: String,
    },
    /// Batch linkage no longer valid (`reason_code = approval_invalidated:{suffix}`).
    LinkageInvalid {
        draft_batch_id: String,
        reason_suffix: &'static str,
        issues: Vec<(Option<String>, String)>,
    },
    /// Batch already invalidated (`reason_code = approval_invalidated:{reason}`).
    BatchInvalidated {
        draft_batch_id: String,
        invalidation_reason_code: Option<String>,
        invalidated_at: Option<i64>,
    },
    /// Draft items not all approvable (`reason_code = stale_local_state`).
    DraftStateNotApprovable {
        draft_batch_id: String,
        issues: Vec<DraftStateIssue>,
    },
    /// Stored approval token already revoked (`reason_code = approval_revoked`).
    ApprovalRevoked {
        draft_batch_id: String,
        approval_id: String,
        revoked_at: Option<i64>,
    },
    /// Stored token payload digest drift (`reason_code = approval_payload_digest_mismatch`).
    ApprovalPayloadDigestMismatch {
        draft_batch_id: String,
        approval_id: String,
        expected_payload_digest: String,
        stored_payload_digest: String,
    },
    /// Stored token target tuple drift (`reason_code = approval_target_tuple_mismatch`).
    ApprovalTargetTupleMismatch {
        draft_batch_id: String,
        approval_id: String,
        expected_target_tuple_json: String,
        stored_target_tuple_json: String,
    },
    /// Batch not in an approvable state (`reason_code = stale_local_state`).
    BatchNotApprovable {
        draft_batch_id: String,
        approval_state: String,
    },
    /// Internal failure the CLI renders as an error envelope.
    Failed(String),
}

fn approve_binding_block_to_rejection(
    block: BatchBindingBlock,
    review_run_id: &str,
    batch_id: &str,
) -> ApproveRejection {
    match block {
        BatchBindingBlock::NotFound => unreachable!("NotFound handled by caller with candidates"),
        BatchBindingBlock::SessionMismatch {
            batch_review_session_id,
        } => ApproveRejection::SessionMismatch {
            review_run_id: review_run_id.to_owned(),
            draft_batch_id: batch_id.to_owned(),
            batch_review_session_id,
        },
        BatchBindingBlock::RunMismatch {
            batch_review_run_id,
        } => ApproveRejection::RunMismatch {
            latest_review_run_id: review_run_id.to_owned(),
            draft_batch_id: batch_id.to_owned(),
            batch_review_run_id,
        },
        BatchBindingBlock::TargetDrift {
            expected_repo_id,
            expected_remote_review_target_id,
            stored_repo_id,
            stored_remote_review_target_id,
        } => ApproveRejection::TargetDrift {
            draft_batch_id: batch_id.to_owned(),
            expected_repo_id,
            expected_remote_review_target_id,
            stored_repo_id,
            stored_remote_review_target_id,
        },
        BatchBindingBlock::ExistingPostedAction {
            posted_action_id,
            posted_action_status,
            failure_code,
        } => ApproveRejection::ExistingPostedAction {
            draft_batch_id: batch_id.to_owned(),
            posted_action_id,
            posted_action_status,
            failure_code,
        },
        BatchBindingBlock::MissingDraftItems => ApproveRejection::MissingDraftItems {
            draft_batch_id: batch_id.to_owned(),
        },
        BatchBindingBlock::LinkageInvalid {
            reason_suffix,
            issues,
        } => ApproveRejection::LinkageInvalid {
            draft_batch_id: batch_id.to_owned(),
            reason_suffix,
            issues,
        },
        BatchBindingBlock::BatchInvalidated {
            invalidation_reason_code,
            invalidated_at,
        } => ApproveRejection::BatchInvalidated {
            draft_batch_id: batch_id.to_owned(),
            invalidation_reason_code,
            invalidated_at,
        },
    }
}

/// Fail-closed approval of a draft batch: the same stale-state, target-binding,
/// digest, revoked-token, and draft-state checks as the CLI's `rr approve`, then
/// records/reissues the approval token and marks the batch and its items
/// approved. A token revoked because a draft body was revised is re-approvable
/// (re-running approve is the explicit approval of the revised payload); any
/// other revocation stays terminal.
pub fn approve_batch(
    store: &RogerStore,
    session: &ReviewSessionRecord,
    requested_batch_id: Option<&str>,
) -> std::result::Result<ApproveOutcome, ApproveRejection> {
    let run = match resolve_active_run(store, session) {
        Ok(run) => run,
        Err(ActiveRunError::Blocked(block)) => return Err(ApproveRejection::Precondition(block)),
        Err(ActiveRunError::Failed(msg)) => return Err(ApproveRejection::Failed(msg)),
    };

    let candidate_batch_ids = awaiting_approval_batch_ids_for_run(store, &session.id, &run.id)
        .map_err(ApproveRejection::Failed)?;

    let Some(batch_id) = requested_batch_id else {
        return Err(ApproveRejection::BatchSelectionRequired {
            review_run_id: run.id.clone(),
            available_batch_ids: candidate_batch_ids,
        });
    };

    let (batch, drafts) = match load_bound_batch(store, session, &run.id, batch_id) {
        Ok(bound) => bound,
        Err(BatchBindingError::Failed(msg)) => return Err(ApproveRejection::Failed(msg)),
        Err(BatchBindingError::Blocked(BatchBindingBlock::NotFound)) => {
            return Err(ApproveRejection::BatchNotFound {
                review_run_id: run.id.clone(),
                draft_batch_id: batch_id.to_owned(),
                available_batch_ids: candidate_batch_ids,
            });
        }
        Err(BatchBindingError::Blocked(block)) => {
            return Err(approve_binding_block_to_rejection(block, &run.id, batch_id));
        }
    };

    let draft_state_issues = drafts
        .iter()
        .filter_map(|draft| match &draft.approval_state {
            ApprovalState::Drafted | ApprovalState::Approved => None,
            ApprovalState::Invalidated => Some(DraftStateIssue {
                draft_id: draft.id.clone(),
                finding_id: draft.finding_id.clone(),
                reason_code: "draft_invalidated".to_owned(),
            }),
            ApprovalState::Posted => Some(DraftStateIssue {
                draft_id: draft.id.clone(),
                finding_id: draft.finding_id.clone(),
                reason_code: "draft_already_posted".to_owned(),
            }),
            ApprovalState::Failed => Some(DraftStateIssue {
                draft_id: draft.id.clone(),
                finding_id: draft.finding_id.clone(),
                reason_code: "draft_already_failed".to_owned(),
            }),
            ApprovalState::NotDrafted => Some(DraftStateIssue {
                draft_id: draft.id.clone(),
                finding_id: draft.finding_id.clone(),
                reason_code: "draft_state_not_materialized".to_owned(),
            }),
        })
        .collect::<Vec<_>>();
    if !draft_state_issues.is_empty() {
        return Err(ApproveRejection::DraftStateNotApprovable {
            draft_batch_id: batch.id.clone(),
            issues: draft_state_issues,
        });
    }

    let expected_target_tuple_json = outbound_target_tuple_json(&batch);
    let existing_approval = store.approval_token_for_batch(&batch.id).map_err(|err| {
        ApproveRejection::Failed(format!("failed to inspect existing approval token: {err}"))
    })?;
    let reapproval_after_revision = existing_approval
        .as_ref()
        .is_some_and(|approval| approval.revoked_at.is_some())
        && matches!(
            store.outbound_approval_revocation_reason(&batch.id),
            Ok(Some(ref reason)) if reason == OUTBOUND_APPROVAL_REVOKED_REASON_DRAFT_REVISED
        );
    if let Some(approval) = &existing_approval {
        if approval.revoked_at.is_some() && !reapproval_after_revision {
            return Err(ApproveRejection::ApprovalRevoked {
                draft_batch_id: batch.id.clone(),
                approval_id: approval.id.clone(),
                revoked_at: approval.revoked_at,
            });
        }
        if reapproval_after_revision {
            // Reissued token binds the revised payload; skip stale comparisons.
        } else {
            if approval.payload_digest != batch.payload_digest {
                return Err(ApproveRejection::ApprovalPayloadDigestMismatch {
                    draft_batch_id: batch.id.clone(),
                    approval_id: approval.id.clone(),
                    expected_payload_digest: batch.payload_digest.clone(),
                    stored_payload_digest: approval.payload_digest.clone(),
                });
            }
            if approval.target_tuple_json != expected_target_tuple_json {
                return Err(ApproveRejection::ApprovalTargetTupleMismatch {
                    draft_batch_id: batch.id.clone(),
                    approval_id: approval.id.clone(),
                    expected_target_tuple_json: expected_target_tuple_json.clone(),
                    stored_target_tuple_json: approval.target_tuple_json.clone(),
                });
            }
        }
    }

    let batch_already_approved = matches!(&batch.approval_state, ApprovalState::Approved);
    if !matches!(
        &batch.approval_state,
        ApprovalState::Drafted | ApprovalState::Approved
    ) {
        return Err(ApproveRejection::BatchNotApprovable {
            draft_batch_id: batch.id.clone(),
            approval_state: format!("{:?}", batch.approval_state),
        });
    }

    let approval_needs_insert = existing_approval.is_none() || reapproval_after_revision;
    let approval = match existing_approval {
        Some(existing) if reapproval_after_revision => OutboundApprovalToken {
            id: existing.id,
            draft_batch_id: batch.id.clone(),
            payload_digest: batch.payload_digest.clone(),
            target_tuple_json: expected_target_tuple_json.clone(),
            approved_at: time::now_ts(),
            revoked_at: None,
        },
        Some(existing) => existing,
        None => OutboundApprovalToken {
            id: next_id("approval"),
            draft_batch_id: batch.id.clone(),
            payload_digest: batch.payload_digest.clone(),
            target_tuple_json: expected_target_tuple_json.clone(),
            approved_at: time::now_ts(),
            revoked_at: None,
        },
    };
    let approval_created = !batch_already_approved;

    for draft in &drafts {
        if matches!(&draft.approval_state, ApprovalState::Approved) {
            continue;
        }
        let mut approved_draft = draft.clone();
        approved_draft.approval_state = ApprovalState::Approved;
        approved_draft.row_version += 1;
        store
            .store_outbound_draft_item(&approved_draft)
            .map_err(|err| {
                ApproveRejection::Failed(format!(
                    "failed to store approved outbound draft item for finding {}: {err}",
                    draft.finding_id.as_deref().unwrap_or("<unknown>")
                ))
            })?;
    }

    if approval_needs_insert {
        store
            .store_outbound_approval_token(&approval)
            .map_err(|err| {
                ApproveRejection::Failed(format!("failed to store outbound approval token: {err}"))
            })?;
    }

    if !batch_already_approved || batch.approved_at != Some(approval.approved_at) {
        let mut approved_batch = batch.clone();
        approved_batch.approval_state = ApprovalState::Approved;
        approved_batch.approved_at = Some(approval.approved_at);
        approved_batch.invalidated_at = None;
        approved_batch.invalidation_reason_code = None;
        approved_batch.row_version += 1;
        store
            .store_outbound_draft_batch(&approved_batch)
            .map_err(|err| {
                ApproveRejection::Failed(format!(
                    "failed to store approved outbound draft batch: {err}"
                ))
            })?;
    }

    Ok(ApproveOutcome {
        review_run_id: run.id,
        batch,
        drafts,
        approval,
        expected_target_tuple_json,
        batch_already_approved,
        approval_created,
    })
}

// ---------------------------------------------------------------------------
// 4. post_batch
// ---------------------------------------------------------------------------

/// Successful (or partially/failed but non-blocked) posting of a batch.
#[derive(Debug, Clone)]
pub struct PostOutcome {
    pub review_run_id: String,
    pub batch: OutboundDraftBatch,
    pub drafts: Vec<OutboundDraft>,
    pub approval: OutboundApprovalToken,
    pub posting_result: ExplicitPostingResult,
}

/// Fail-closed rejections of `post_batch`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PostRejection {
    Precondition(SessionPreconditionBlock),
    /// No batch id supplied (`reason_code = approved_batch_selection_required`).
    BatchSelectionRequired {
        review_run_id: String,
        available_batch_ids: Vec<String>,
    },
    /// Batch id not found (`reason_code = missing_local_state`).
    BatchNotFound {
        review_run_id: String,
        draft_batch_id: String,
        available_batch_ids: Vec<String>,
    },
    SessionMismatch {
        review_run_id: String,
        draft_batch_id: String,
        batch_review_session_id: String,
    },
    RunMismatch {
        latest_review_run_id: String,
        draft_batch_id: String,
        batch_review_run_id: String,
    },
    TargetDrift {
        draft_batch_id: String,
        expected_repo_id: String,
        expected_remote_review_target_id: String,
        stored_repo_id: String,
        stored_remote_review_target_id: String,
    },
    ExistingPostedAction {
        draft_batch_id: String,
        posted_action_id: String,
        posted_action_status: String,
        failure_code: Option<String>,
    },
    MissingDraftItems {
        draft_batch_id: String,
    },
    LinkageInvalid {
        draft_batch_id: String,
        reason_suffix: &'static str,
        issues: Vec<(Option<String>, String)>,
    },
    BatchInvalidated {
        draft_batch_id: String,
        invalidation_reason_code: Option<String>,
        invalidated_at: Option<i64>,
    },
    /// Draft items not all approved/postable (`reason_code = stale_local_state`).
    DraftStateNotPostable {
        draft_batch_id: String,
        issues: Vec<DraftStateIssue>,
    },
    /// Batch not in Approved state (`reason_code = approval_required`).
    ApprovalRequiredBatchState {
        draft_batch_id: String,
        approval_state: String,
    },
    /// No approval token present (`reason_code = approval_required`).
    ApprovalRequiredNoToken {
        draft_batch_id: String,
    },
    /// Stored approval token revoked (`reason_code = approval_revoked`).
    ApprovalRevoked {
        draft_batch_id: String,
        approval_id: String,
        revoked_at: Option<i64>,
    },
    /// The posting flow itself refused exact-payload verification
    /// (`reason_code = posting_result.reason_code`).
    PostingBlocked {
        review_run_id: String,
        draft_batch_id: String,
        approval_id: String,
        reason_code: Option<String>,
        item_results: Vec<roger_app_core::PostingAdapterItemResult>,
        retry_draft_ids: Vec<String>,
    },
    /// Internal failure the CLI renders as an error envelope.
    Failed(String),
}

fn post_binding_block_to_rejection(
    block: BatchBindingBlock,
    review_run_id: &str,
    batch_id: &str,
) -> PostRejection {
    match block {
        BatchBindingBlock::NotFound => unreachable!("NotFound handled by caller with candidates"),
        BatchBindingBlock::SessionMismatch {
            batch_review_session_id,
        } => PostRejection::SessionMismatch {
            review_run_id: review_run_id.to_owned(),
            draft_batch_id: batch_id.to_owned(),
            batch_review_session_id,
        },
        BatchBindingBlock::RunMismatch {
            batch_review_run_id,
        } => PostRejection::RunMismatch {
            latest_review_run_id: review_run_id.to_owned(),
            draft_batch_id: batch_id.to_owned(),
            batch_review_run_id,
        },
        BatchBindingBlock::TargetDrift {
            expected_repo_id,
            expected_remote_review_target_id,
            stored_repo_id,
            stored_remote_review_target_id,
        } => PostRejection::TargetDrift {
            draft_batch_id: batch_id.to_owned(),
            expected_repo_id,
            expected_remote_review_target_id,
            stored_repo_id,
            stored_remote_review_target_id,
        },
        BatchBindingBlock::ExistingPostedAction {
            posted_action_id,
            posted_action_status,
            failure_code,
        } => PostRejection::ExistingPostedAction {
            draft_batch_id: batch_id.to_owned(),
            posted_action_id,
            posted_action_status,
            failure_code,
        },
        BatchBindingBlock::MissingDraftItems => PostRejection::MissingDraftItems {
            draft_batch_id: batch_id.to_owned(),
        },
        BatchBindingBlock::LinkageInvalid {
            reason_suffix,
            issues,
        } => PostRejection::LinkageInvalid {
            draft_batch_id: batch_id.to_owned(),
            reason_suffix,
            issues,
        },
        BatchBindingBlock::BatchInvalidated {
            invalidation_reason_code,
            invalidated_at,
        } => PostRejection::BatchInvalidated {
            draft_batch_id: batch_id.to_owned(),
            invalidation_reason_code,
            invalidated_at,
        },
    }
}

/// Fully-gated posting of an approved batch: every `rr post` precondition
/// (session non-stale, batch belongs to session/run, no prior posted action,
/// linkage valid, not invalidated, all drafts Approved, batch Approved, live
/// approval token) then execute through the injected GitHub posting adapter and
/// persist the posted action. The adapter is a generic port so tests can drive a
/// fake and the real `GhCliAdapter` stays behind the same gate.
pub fn post_batch<A: OutboundPostingAdapter>(
    store: &RogerStore,
    session: &ReviewSessionRecord,
    requested_batch_id: Option<&str>,
    adapter: &A,
) -> std::result::Result<PostOutcome, PostRejection> {
    let run = match resolve_active_run(store, session) {
        Ok(run) => run,
        Err(ActiveRunError::Blocked(block)) => return Err(PostRejection::Precondition(block)),
        Err(ActiveRunError::Failed(msg)) => return Err(PostRejection::Failed(msg)),
    };

    let candidate_batch_ids =
        approved_batch_ids_for_run(store, &session.id, &run.id).map_err(PostRejection::Failed)?;

    let Some(batch_id) = requested_batch_id else {
        return Err(PostRejection::BatchSelectionRequired {
            review_run_id: run.id.clone(),
            available_batch_ids: candidate_batch_ids,
        });
    };

    let (batch, drafts) = match load_bound_batch(store, session, &run.id, batch_id) {
        Ok(bound) => bound,
        Err(BatchBindingError::Failed(msg)) => return Err(PostRejection::Failed(msg)),
        Err(BatchBindingError::Blocked(BatchBindingBlock::NotFound)) => {
            return Err(PostRejection::BatchNotFound {
                review_run_id: run.id.clone(),
                draft_batch_id: batch_id.to_owned(),
                available_batch_ids: candidate_batch_ids,
            });
        }
        Err(BatchBindingError::Blocked(block)) => {
            return Err(post_binding_block_to_rejection(block, &run.id, batch_id));
        }
    };

    let draft_state_issues = drafts
        .iter()
        .filter_map(|draft| match &draft.approval_state {
            ApprovalState::Approved => None,
            ApprovalState::Drafted => Some(DraftStateIssue {
                draft_id: draft.id.clone(),
                finding_id: draft.finding_id.clone(),
                reason_code: "draft_not_yet_approved".to_owned(),
            }),
            ApprovalState::Invalidated => Some(DraftStateIssue {
                draft_id: draft.id.clone(),
                finding_id: draft.finding_id.clone(),
                reason_code: "draft_invalidated".to_owned(),
            }),
            ApprovalState::Posted => Some(DraftStateIssue {
                draft_id: draft.id.clone(),
                finding_id: draft.finding_id.clone(),
                reason_code: "draft_already_posted".to_owned(),
            }),
            ApprovalState::Failed => Some(DraftStateIssue {
                draft_id: draft.id.clone(),
                finding_id: draft.finding_id.clone(),
                reason_code: "draft_already_failed".to_owned(),
            }),
            ApprovalState::NotDrafted => Some(DraftStateIssue {
                draft_id: draft.id.clone(),
                finding_id: draft.finding_id.clone(),
                reason_code: "draft_state_not_materialized".to_owned(),
            }),
        })
        .collect::<Vec<_>>();
    if !draft_state_issues.is_empty() {
        return Err(PostRejection::DraftStateNotPostable {
            draft_batch_id: batch.id.clone(),
            issues: draft_state_issues,
        });
    }

    if !matches!(&batch.approval_state, ApprovalState::Approved) {
        return Err(PostRejection::ApprovalRequiredBatchState {
            draft_batch_id: batch.id.clone(),
            approval_state: format!("{:?}", batch.approval_state),
        });
    }

    let approval = store.approval_token_for_batch(&batch.id).map_err(|err| {
        PostRejection::Failed(format!("failed to inspect outbound approval token: {err}"))
    })?;
    let Some(approval) = approval else {
        return Err(PostRejection::ApprovalRequiredNoToken {
            draft_batch_id: batch.id.clone(),
        });
    };
    if approval.revoked_at.is_some() {
        return Err(PostRejection::ApprovalRevoked {
            draft_batch_id: batch.id.clone(),
            approval_id: approval.id.clone(),
            revoked_at: approval.revoked_at,
        });
    }

    let action_id = next_id("posted-batch");
    let reconfirmed_finding_ids: HashSet<String> = HashSet::new();
    let posting_result = execute_explicit_posting_flow(
        ExplicitPostingInput {
            action_id: &action_id,
            provider: "github",
            target: &session.review_target,
            batch: &batch,
            drafts: &drafts,
            approval: &approval,
            refresh_signals: &[],
            reconfirmed_finding_ids: &reconfirmed_finding_ids,
        },
        adapter,
    );

    if matches!(&posting_result.outcome, ExplicitPostingOutcome::Blocked) {
        return Err(PostRejection::PostingBlocked {
            review_run_id: run.id.clone(),
            draft_batch_id: batch.id.clone(),
            approval_id: approval.id.clone(),
            reason_code: posting_result.reason_code.clone(),
            item_results: posting_result.item_results.clone(),
            retry_draft_ids: posting_result.retry_draft_ids.clone(),
        });
    }

    if let Some(posted_action) = posting_result.posted_action.as_ref() {
        store
            .store_posted_batch_action(posted_action)
            .map_err(|err| {
                PostRejection::Failed(format!("failed to store posted batch action: {err}"))
            })?;
    }

    Ok(PostOutcome {
        review_run_id: run.id,
        batch,
        drafts,
        approval,
        posting_result,
    })
}

// ---------------------------------------------------------------------------
// 5. resolve_memory_review
// ---------------------------------------------------------------------------

/// Generic internal failure for the thin storage-wrapping ops.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewOpError {
    /// A caller-facing validation failure (e.g. empty clarification body).
    Invalid(String),
    /// A storage-layer failure.
    Storage(String),
}

impl From<StorageError> for ReviewOpError {
    fn from(err: StorageError) -> Self {
        ReviewOpError::Storage(err.to_string())
    }
}

/// Resolve a durable memory review request (accept -> materialize/update the
/// memory item; reject -> just resolve). The single shared path CLI + TUI both
/// call so memory acceptance stays one write path.
pub fn resolve_memory_review(
    store: &RogerStore,
    request_id: &str,
    decision: MemoryReviewDecision,
    actor: &str,
) -> std::result::Result<MemoryReviewResolutionOutcome, ReviewOpError> {
    store
        .resolve_memory_review_request(request_id, decision, actor)
        .map_err(ReviewOpError::from)
}

// ---------------------------------------------------------------------------
// 6. create_clarification
// ---------------------------------------------------------------------------

/// Create a durable clarification request (the shared op behind the worker
/// `worker.request_clarification` transport, a future operator `rr clarify`, and
/// the TUI/extension composers). Rejects an empty body; otherwise persists an
/// open clarification linked to the session (and optional run/finding lineage).
pub fn create_clarification(
    store: &RogerStore,
    input: CreateClarificationRequest<'_>,
) -> std::result::Result<ClarificationRequestRecord, ReviewOpError> {
    if input.body.trim().is_empty() {
        return Err(ReviewOpError::Invalid(
            "clarification body must be non-empty".to_owned(),
        ));
    }
    store
        .create_clarification_request(input)
        .map_err(ReviewOpError::from)
}

#[cfg(test)]
mod tests;
