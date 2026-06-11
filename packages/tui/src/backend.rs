//! Cockpit data access boundary.
//!
//! The TUI is an adapter over Roger storage/domain rules. [`CockpitBackend`]
//! is the narrow read/triage/approve surface the cockpit needs;
//! [`StoreCockpitBackend`] implements it directly over [`RogerStore`] so
//! triage and approval semantics stay the storage crate's, not the TUI's.

use crate::model::{
    ApproveOutcome, DecisionEventRow, DraftBatchRow, DraftItemRow, EvidenceRow, FindingDetail,
    FindingRow, OutboundLinkRow, PostedActionRow, SearchHitRow, SearchView, SessionHomeView,
    SessionRow, StageResultRow, TimelineRunRow, TimelineView, TriageAction, bump_count,
};
use roger_storage::{
    OutboundBatchApproval, PriorReviewLookupQuery, PriorReviewRetrievalMode, RogerStore,
    SessionFinderQuery, projected_outbound_batch_state,
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
                semantic_assets_verified: false,
                semantic_candidates: Vec::new(),
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
