//! Cockpit data access boundary.
//!
//! The TUI is an adapter over Roger storage/domain rules. [`CockpitBackend`]
//! is the narrow read/triage/approve surface the cockpit needs;
//! [`StoreCockpitBackend`] implements it directly over [`RogerStore`] so
//! triage and approval semantics stay the storage crate's, not the TUI's.

use crate::model::{
    ApproveOutcome, DecisionEventRow, DraftBatchRow, DraftItemRow, EvidenceRow, FindingDetail,
    FindingRow, MemoryPostureRow, MemoryReviewResolutionRow, MemoryReviewRow, OutboundLinkRow,
    PostedActionRow, SearchHitRow, SearchView, SessionHomeView, SessionRow, StageResultRow,
    TimelineRunRow, TimelineView, TriageAction, bump_count,
};
use roger_storage::{
    OutboundBatchApproval, PriorReviewLookupQuery, PriorReviewRetrievalMode, RogerStore,
    SemanticEmbedderAdapter, SessionFinderQuery, UpdateIndexState, projected_outbound_batch_state,
    semantic_embedder_status,
};

/// Maximum sessions projected into the cockpit session finder screen.
const SESSION_FINDER_LIMIT: usize = 100;

/// Maximum prior-review hits projected into the search screen.
const SEARCH_LIMIT: usize = 50;

pub trait CockpitBackend {
    fn list_sessions(&mut self) -> Result<Vec<SessionRow>, String>;
    fn load_session_home(&mut self, session_id: &str) -> Result<SessionHomeView, String>;
    fn load_findings(&mut self, session_id: &str) -> Result<Vec<FindingRow>, String>;
    /// Read-only inspector detail: code evidence, linked outbound state,
    /// decision-event history.
    fn load_finding_detail(&mut self, finding_id: &str) -> Result<FindingDetail, String>;
    /// Apply the same storage mutation `rr triage` uses
    /// (`RogerStore::update_finding_triage_state`).
    fn set_triage_state(&mut self, finding_id: &str, action: TriageAction) -> Result<(), String>;
    fn load_draft_batches(&mut self, session_id: &str) -> Result<Vec<DraftBatchRow>, String>;
    fn load_batch_items(&mut self, batch_id: &str) -> Result<Vec<DraftItemRow>, String>;
    /// Execute the same fail-closed storage approval path `rr approve` uses
    /// (`RogerStore::approve_outbound_batch_for_session`); payload-digest and
    /// target-tuple binding are never bypassed.
    fn approve_batch(&mut self, session_id: &str, batch_id: &str)
    -> Result<ApproveOutcome, String>;
    fn load_timeline(&mut self, session_id: &str) -> Result<TimelineView, String>;
    /// Read-only scoped lookup over prior findings via the storage
    /// prior-review lookup API (repo scope only, lexical posture).
    fn search_prior_reviews(&mut self, repository: &str, query: &str)
    -> Result<SearchView, String>;
    /// Project the session-level retrieval/memory posture (retrieval mode,
    /// semantic asset/embedder state, degraded reasons) from storage.
    fn memory_posture(&mut self) -> Result<MemoryPostureRow, String>;
    /// Persist an edited outbound-draft body through the storage revision path
    /// (same author/audit semantics a future `rr` draft-edit command uses).
    fn revise_draft_body(&mut self, draft_id: &str, new_body: &str) -> Result<(), String>;
    /// List pending memory review requests in this repo scope (the operator
    /// promotion-review surface). Read-only projection.
    fn list_pending_memory_reviews(
        &mut self,
        repository: &str,
    ) -> Result<Vec<MemoryReviewRow>, String>;
    /// Resolve (accept/reject) a pending memory review request through storage.
    /// Accept is the first production `memory_items` writer; the mutation is
    /// local-only (not GitHub-elevated), so it uses plain keys with a clear
    /// notice rather than the elevation gate.
    fn resolve_memory_review(
        &mut self,
        request_id: &str,
        accept: bool,
    ) -> Result<MemoryReviewResolutionRow, String>;
}

pub struct StoreCockpitBackend {
    store: RogerStore,
    repo_filter: Option<String>,
    pr_filter: Option<u64>,
}

impl StoreCockpitBackend {
    pub fn new(store: RogerStore, repo_filter: Option<String>, pr_filter: Option<u64>) -> Self {
        Self {
            store,
            repo_filter,
            pr_filter,
        }
    }
}

impl CockpitBackend for StoreCockpitBackend {
    fn list_sessions(&mut self) -> Result<Vec<SessionRow>, String> {
        let entries = self
            .store
            .session_finder(SessionFinderQuery {
                repository: self.repo_filter.clone(),
                pull_request_number: self.pr_filter,
                attention_states: Vec::new(),
                limit: SESSION_FINDER_LIMIT,
            })
            .map_err(|err| format!("failed to load sessions: {err}"))?;
        Ok(entries
            .into_iter()
            .map(|entry| SessionRow {
                session_id: entry.session_id,
                repository: entry.repository,
                pull_request: entry.pull_request_number,
                provider: entry.provider,
                attention_state: entry.attention_state,
                updated_at: entry.updated_at,
            })
            .collect())
    }

    fn load_session_home(&mut self, session_id: &str) -> Result<SessionHomeView, String> {
        let findings = self.latest_run_findings(session_id)?;
        let mut triage_counts: Vec<(String, usize)> = Vec::new();
        let mut outbound_counts: Vec<(String, usize)> = Vec::new();
        for finding in &findings {
            bump_count(&mut triage_counts, &finding.triage_state);
            bump_count(&mut outbound_counts, &finding.outbound_state);
        }
        let latest_run = self
            .store
            .latest_review_run(session_id)
            .map_err(|err| format!("failed to load latest run: {err}"))?;
        // Count worker stage results for the latest run so the findings screen
        // can distinguish "worker has not submitted results yet" from "run
        // completed with zero findings".
        let latest_run_stage_count = match &latest_run {
            Some(run) => self
                .store
                .worker_stage_results_for_run(session_id, &run.id)
                .map_err(|err| format!("failed to load stage results for {}: {err}", run.id))?
                .len(),
            None => 0,
        };
        let overview = self
            .store
            .session_overview(session_id)
            .map_err(|err| format!("failed to load session overview: {err}"))?;
        Ok(SessionHomeView {
            findings_total: findings.len(),
            triage_counts,
            outbound_counts,
            latest_run_id: latest_run.as_ref().map(|run| run.id.clone()),
            latest_run_kind: latest_run.as_ref().map(|run| run.run_kind.clone()),
            latest_run_stage_count,
            run_count: overview.run_count.max(0) as usize,
            draft_count: overview.draft_count.max(0) as usize,
            posted_action_count: overview.posted_action_count.max(0) as usize,
        })
    }

    fn load_findings(&mut self, session_id: &str) -> Result<Vec<FindingRow>, String> {
        self.latest_run_findings(session_id)
    }

    fn load_finding_detail(&mut self, finding_id: &str) -> Result<FindingDetail, String> {
        let finding = self
            .store
            .materialized_finding(finding_id)
            .map_err(|err| format!("failed to load finding {finding_id}: {err}"))?
            .ok_or_else(|| format!("finding {finding_id} no longer exists"))?;
        let evidence = self
            .store
            .code_evidence_locations_for_finding(finding_id)
            .map_err(|err| format!("failed to load code evidence for {finding_id}: {err}"))?
            .into_iter()
            .map(|row| EvidenceRow {
                repo_rel_path: row.repo_rel_path,
                start_line: row.start_line,
                end_line: row.end_line,
                evidence_role: row.evidence_role,
                anchor_state: row.anchor_state,
            })
            .collect();
        let projection = self
            .store
            .outbound_surface_projection_for_finding(finding_id, &finding.outbound_state)
            .map_err(|err| format!("failed to project outbound state for {finding_id}: {err}"))?;
        let decision_events = self
            .store
            .finding_decision_events_for_finding(finding_id)
            .map_err(|err| format!("failed to load decision events for {finding_id}: {err}"))?
            .into_iter()
            .map(|event| DecisionEventRow {
                triage_state: event.triage_state,
                outbound_state: event.outbound_state,
                created_at: event.created_at,
            })
            .collect();
        Ok(FindingDetail {
            finding_id: finding_id.to_owned(),
            evidence,
            outbound: OutboundLinkRow {
                state: projection.state,
                draft_id: projection.draft_id,
                draft_batch_id: projection.draft_batch_id,
                posted_action_status: projection.posted_action_status,
                remote_identifier: projection.remote_identifier,
                invalidation_reason_code: projection.invalidation_reason_code,
                failure_code: projection.failure_code,
            },
            decision_events,
        })
    }

    fn set_triage_state(&mut self, finding_id: &str, action: TriageAction) -> Result<(), String> {
        self.store
            .update_finding_triage_state(finding_id, action.as_state_str())
            .map(|_| ())
            .map_err(|err| format!("failed to update triage state for {finding_id}: {err}"))
    }

    fn load_draft_batches(&mut self, session_id: &str) -> Result<Vec<DraftBatchRow>, String> {
        let batches = self
            .store
            .outbound_draft_batches_for_session(session_id)
            .map_err(|err| format!("failed to load draft batches: {err}"))?;
        let mut rows = Vec::with_capacity(batches.len());
        for batch in batches {
            let items = self
                .store
                .outbound_draft_items_for_batch(&batch.id)
                .map_err(|err| format!("failed to load items for batch {}: {err}", batch.id))?;
            rows.push(DraftBatchRow {
                state: projected_outbound_batch_state(&batch).to_owned(),
                item_count: items.len(),
                payload_digest: batch.payload_digest.clone(),
                invalidation_reason_code: batch.invalidation_reason_code.clone(),
                batch_id: batch.id,
            });
        }
        Ok(rows)
    }

    fn load_batch_items(&mut self, batch_id: &str) -> Result<Vec<DraftItemRow>, String> {
        let items = self
            .store
            .outbound_draft_items_for_batch(batch_id)
            .map_err(|err| format!("failed to load items for batch {batch_id}: {err}"))?;
        Ok(items
            .into_iter()
            .map(|item| DraftItemRow {
                draft_id: item.id,
                finding_id: item.finding_id,
                target_locator: item.target_locator,
                body: item.body,
            })
            .collect())
    }

    fn approve_batch(
        &mut self,
        session_id: &str,
        batch_id: &str,
    ) -> Result<ApproveOutcome, String> {
        match self
            .store
            .approve_outbound_batch_for_session(session_id, batch_id)
        {
            Ok(OutboundBatchApproval::Approved {
                draft_count,
                already_recorded,
                ..
            }) => Ok(ApproveOutcome::Approved {
                draft_count,
                already_recorded,
            }),
            Ok(OutboundBatchApproval::Blocked { reason_code }) => {
                Ok(ApproveOutcome::Blocked { reason_code })
            }
            Err(err) => Err(format!("failed to approve batch {batch_id}: {err}")),
        }
    }

    fn load_timeline(&mut self, session_id: &str) -> Result<TimelineView, String> {
        let runs = self
            .store
            .review_runs_for_session(session_id)
            .map_err(|err| format!("failed to load review runs: {err}"))?;
        let mut run_rows = Vec::with_capacity(runs.len());
        for run in runs {
            let stages = self
                .store
                .worker_stage_results_for_run(session_id, &run.id)
                .map_err(|err| format!("failed to load stage results for {}: {err}", run.id))?
                .into_iter()
                .map(|stage| StageResultRow {
                    stage: stage.stage,
                    task_kind: format!("{:?}", stage.task_kind),
                    outcome: format!("{:?}", stage.outcome),
                    summary: stage.summary,
                })
                .collect();
            run_rows.push(TimelineRunRow {
                run_id: run.id,
                run_kind: run.run_kind,
                continuity_quality: run.continuity_quality,
                created_at: run.created_at,
                stages,
            });
        }

        let mut posted_actions = Vec::new();
        let batches = self
            .store
            .outbound_draft_batches_for_session(session_id)
            .map_err(|err| format!("failed to load draft batches: {err}"))?;
        for batch in batches {
            let actions = self
                .store
                .posted_actions_for_batch(&batch.id)
                .map_err(|err| {
                    format!(
                        "failed to load posted actions for batch {}: {err}",
                        batch.id
                    )
                })?;
            for action in actions {
                posted_actions.push(PostedActionRow {
                    action_id: action.id,
                    batch_id: action.draft_batch_id,
                    remote_identifier: action.remote_identifier,
                    status: format!("{:?}", action.status),
                    failure_code: action.failure_code,
                    posted_at: action.posted_at,
                });
            }
        }
        posted_actions.sort_by(|left, right| {
            left.posted_at
                .cmp(&right.posted_at)
                .then_with(|| left.action_id.cmp(&right.action_id))
        });

        Ok(TimelineView {
            runs: run_rows,
            posted_actions,
        })
    }

    fn search_prior_reviews(
        &mut self,
        repository: &str,
        query: &str,
    ) -> Result<SearchView, String> {
        let scope_key = format!("repo:{repository}");

        // Real semantic posture (mirrors `rr search`): the semantic lane is
        // eligible only when assets verify AND the embedder is available. Never
        // hardcode "off" — that contradicted this backend's own posture line.
        let component_state = self.store.semantic_component_state().ok();
        let assets_verified = component_state
            .as_ref()
            .map(|state| state.assets_verified)
            .unwrap_or(false);
        let embedder_available = component_state
            .as_ref()
            .map(|state| state.embedder_available)
            .unwrap_or(false);
        let semantic_assets_verified = assets_verified && embedder_available;

        // When the lane is operational, mark this repo scope's semantic index
        // ready so the lookup's readiness gate can flip to hybrid (same design as
        // `rr search`: candidates are embedded live from the canonical corpus).
        if semantic_assets_verified {
            let scope_index_key = format!("semantic:repo:{repository}");
            let digest = self
                .store
                .semantic_asset_manifest()
                .ok()
                .flatten()
                .map(|manifest| manifest.artifact_digest);
            let _ = self.store.upsert_index_state(UpdateIndexState {
                scope_key: &scope_index_key,
                generation: roger_app_core::time::now_ts(),
                status: "ready",
                artifact_digest: digest.as_deref(),
            });
        }

        let semantic_candidates = if semantic_assets_verified {
            let mut embedder = build_cockpit_semantic_embedder(&self.store);
            self.store
                .generate_semantic_candidates(
                    &scope_key,
                    repository,
                    query,
                    &mut embedder,
                    SEARCH_LIMIT + 1,
                )
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        let lookup = self
            .store
            .prior_review_lookup(PriorReviewLookupQuery {
                scope_key: &scope_key,
                repository,
                query_text: query,
                limit: SEARCH_LIMIT,
                include_tentative_candidates: false,
                allow_project_scope: false,
                allow_org_scope: false,
                semantic_assets_verified,
                semantic_candidates,
            })
            .map_err(|err| format!("failed to run prior-review lookup: {err}"))?;
        Ok(SearchView {
            scope_bucket: lookup.scope_bucket,
            retrieval_mode: retrieval_mode_label(&lookup.mode).to_owned(),
            degraded_reasons: lookup.degraded_reasons,
            hits: lookup
                .evidence_hits
                .into_iter()
                .map(|hit| SearchHitRow {
                    finding_id: hit.finding_id,
                    session_id: hit.session_id,
                    repository: hit.repository,
                    pull_request: hit.pull_request_number,
                    title: hit.title,
                    normalized_summary: hit.normalized_summary,
                    severity: hit.severity,
                    triage_state: hit.triage_state,
                    outbound_state: hit.outbound_state,
                    fused_score: hit.fused_score,
                })
                .collect(),
        })
    }

    fn memory_posture(&mut self) -> Result<MemoryPostureRow, String> {
        let state = self
            .store
            .semantic_component_state()
            .map_err(|err| format!("failed to load semantic component state: {err}"))?;
        // The session-level default posture: hybrid only when the semantic lane
        // is actually operational (compiled embedder + verified assets),
        // otherwise lexical-only. Per-query recovery-scan escalation is a
        // property of an individual lookup, not the default posture.
        let retrieval_mode = if state.operational {
            "hybrid"
        } else {
            "lexical_only"
        }
        .to_owned();
        Ok(MemoryPostureRow {
            retrieval_mode,
            semantic_operational: state.operational,
            assets_verified: state.assets_verified,
            embedder_available: state.embedder_available,
            embedder_backend: state.embedder_backend,
            degraded_reasons: state.degraded_reasons,
        })
    }

    fn revise_draft_body(&mut self, draft_id: &str, new_body: &str) -> Result<(), String> {
        self.store
            .revise_outbound_draft_body(
                draft_id,
                new_body,
                roger_storage::RevisionAuthorKind::Operator,
            )
            .map(|_| ())
            .map_err(|err| format!("draft revision failed: {err}"))
    }

    fn list_pending_memory_reviews(
        &mut self,
        repository: &str,
    ) -> Result<Vec<MemoryReviewRow>, String> {
        let scope_key = format!("repo:{repository}");
        let records = self
            .store
            .pending_memory_review_requests(Some(&scope_key), SEARCH_LIMIT)
            .map_err(|err| format!("failed to load pending memory reviews: {err}"))?;
        Ok(records
            .into_iter()
            .map(|record| MemoryReviewRow {
                id: record.id,
                source: record.source,
                request_kind: record.request_kind,
                statement: record.statement,
                normalized_key: record.normalized_key,
                status: record.status,
                created_at: record.created_at,
            })
            .collect())
    }

    fn resolve_memory_review(
        &mut self,
        request_id: &str,
        accept: bool,
    ) -> Result<MemoryReviewResolutionRow, String> {
        let decision = if accept {
            roger_storage::MemoryReviewDecision::Accept
        } else {
            roger_storage::MemoryReviewDecision::Reject
        };
        let outcome = self
            .store
            .resolve_memory_review_request(request_id, decision, "operator:tui")
            .map_err(|err| format!("failed to resolve memory review: {err}"))?;
        Ok(MemoryReviewResolutionRow {
            id: outcome.request.id,
            status: outcome.request.status,
            resulting_memory_item_id: outcome.resulting_memory_item_id,
            materialized_new_item: outcome.materialized_new_item,
        })
    }
}

/// Build the cockpit's semantic embedder the same way `rr search` does: a live
/// FastEmbed adapter when the feature is compiled and the model is installed,
/// otherwise a non-ready stub so `generate_semantic_candidates` returns empty
/// and search stays honestly lexical-only.
fn build_cockpit_semantic_embedder(store: &RogerStore) -> SemanticEmbedderAdapter {
    let status = semantic_embedder_status();
    if !status.available {
        return SemanticEmbedderAdapter::default_for_runtime();
    }
    let cache_dir = store.layout().semantic_asset_root();
    let config = roger_storage::FastEmbedAdapterConfig {
        model: roger_storage::FastEmbedModel::default(),
        cache_dir,
        show_download_progress: false,
    };
    match SemanticEmbedderAdapter::try_fastembed(config) {
        Ok(adapter) => adapter,
        Err(err) => SemanticEmbedderAdapter::unavailable(format!(
            "FastEmbed adapter could not load installed semantic model: {err}"
        )),
    }
}

fn retrieval_mode_label(mode: &PriorReviewRetrievalMode) -> &'static str {
    match mode {
        PriorReviewRetrievalMode::Hybrid => "hybrid",
        PriorReviewRetrievalMode::LexicalOnly => "lexical_only",
        PriorReviewRetrievalMode::RecoveryScan => "recovery_scan",
    }
}

impl StoreCockpitBackend {
    fn latest_run_findings(&mut self, session_id: &str) -> Result<Vec<FindingRow>, String> {
        let Some(run) = self
            .store
            .latest_review_run(session_id)
            .map_err(|err| format!("failed to load latest run: {err}"))?
        else {
            return Ok(Vec::new());
        };
        let findings = self
            .store
            .materialized_findings_for_run(session_id, &run.id)
            .map_err(|err| format!("failed to load findings: {err}"))?;
        let mut rows = Vec::with_capacity(findings.len());
        for finding in findings {
            let projection = self
                .store
                .outbound_surface_projection_for_finding(&finding.id, &finding.outbound_state)
                .map_err(|err| {
                    format!("failed to project outbound state for {}: {err}", finding.id)
                })?;
            let primary_file = self
                .store
                .code_evidence_locations_for_finding(&finding.id)
                .map_err(|err| format!("failed to load code evidence for {}: {err}", finding.id))?
                .into_iter()
                .next()
                .map(|location| location.repo_rel_path);
            rows.push(FindingRow {
                finding_id: finding.id,
                fingerprint: finding.fingerprint,
                title: finding.title,
                normalized_summary: finding.normalized_summary,
                severity: finding.severity,
                confidence: finding.confidence,
                triage_state: finding.triage_state,
                outbound_state: projection.state,
                first_seen_stage: finding.first_seen_stage,
                first_run_id: finding.first_run_id,
                last_seen_stage: finding.last_seen_stage,
                last_seen_run_id: finding.last_seen_run_id,
                primary_file,
            });
        }
        Ok(rows)
    }
}
