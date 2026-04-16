use roger_config::{ResolvedLaunchBaseline, ResolvedProviderCapability};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShellPanelKind {
    SessionOverview,
    RecentRuns,
    FindingsList,
    FindingDetail,
    DraftApprovalQueue,
    ActivityFeed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionChrome {
    pub session_id: String,
    pub repository: String,
    pub pull_request_number: u64,
    pub provider: String,
    pub support_tier: String,
    pub isolation_mode: String,
    pub policy_profile: String,
    pub continuity_state: String,
    pub attention_state: String,
    #[serde(default)]
    pub status_reason: Option<String>,
}

impl SessionChrome {
    pub fn from_resolved_config(
        session_id: impl Into<String>,
        repository: impl Into<String>,
        pull_request_number: u64,
        continuity_state: impl Into<String>,
        attention_state: impl Into<String>,
        launch: &ResolvedLaunchBaseline,
        provider: &ResolvedProviderCapability,
        status_reason: Option<String>,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            repository: repository.into(),
            pull_request_number,
            provider: provider.provider.clone(),
            support_tier: provider.support_tier.clone(),
            isolation_mode: launch.isolation_mode.value.clone(),
            policy_profile: provider.policy_profile.id.clone(),
            continuity_state: continuity_state.into(),
            attention_state: attention_state.into(),
            status_reason: status_reason
                .or_else(|| provider.degraded_reason.clone())
                .or_else(|| provider.fail_closed_reason.clone()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadOnlyPanelState {
    pub kind: ShellPanelKind,
    pub title: String,
    pub lines: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundJobClass {
    Refresh,
    Prompt,
    Index,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundJobStatus {
    Queued,
    Running,
    Completed,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackgroundJobSnapshot {
    pub job_id: String,
    pub class: BackgroundJobClass,
    pub status: BackgroundJobStatus,
    pub summary: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupervisorSnapshot {
    pub queue_depth: usize,
    pub pending_jobs: usize,
    pub wake_requested: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WakeReason {
    Tick,
    JobUpdate,
    UserRefresh,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WakeSignal {
    pub reason: WakeReason,
    pub jobs: Vec<BackgroundJobSnapshot>,
    pub supervisor: Option<SupervisorSnapshot>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindingListRow {
    pub finding_id: String,
    pub title: String,
    pub severity: String,
    pub triage_state: String,
    pub outbound_state: String,
    pub refresh_lineage: Option<String>,
    pub degraded: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceSnippet {
    pub path: String,
    pub start_line: u64,
    pub end_line: Option<u64>,
    pub excerpt: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindingDetail {
    pub finding_id: String,
    pub normalized_summary: String,
    pub refresh_lineage: Option<String>,
    pub degraded_reason: Option<String>,
    pub evidence: Vec<EvidenceSnippet>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindingTriageIntent {
    pub finding_id: String,
    pub from_state: String,
    pub to_state: String,
    pub recorded_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClarificationIntentStatus {
    Queued,
    Completed,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClarificationIntent {
    pub intent_id: String,
    pub finding_id: String,
    pub prompt: String,
    pub status: ClarificationIntentStatus,
    pub created_at: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DraftReviewDecision {
    Pending,
    Reviewed,
    Edited,
    Approved,
    Rejected,
    Invalidated,
}

impl DraftReviewDecision {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Reviewed => "reviewed",
            Self::Edited => "edited",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Invalidated => "invalidated",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalDraftReviewEntry {
    pub draft_id: String,
    pub finding_id: Option<String>,
    pub preview: String,
    pub decision: DraftReviewDecision,
    pub edited_body: Option<String>,
    pub invalidation_reason: Option<String>,
    pub pending_post: bool,
    #[serde(default)]
    pub post_failure_reason: Option<String>,
    #[serde(default)]
    pub recovery_hint: Option<String>,
    pub updated_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveSessionEntry {
    pub session_id: String,
    pub repository: String,
    pub pull_request_number: u64,
    pub provider: String,
    pub support_tier: String,
    pub isolation_mode: String,
    pub policy_profile: String,
    pub continuity_state: String,
    pub attention_state: String,
    pub degraded: bool,
    #[serde(default)]
    pub status_reason: Option<String>,
}

impl ActiveSessionEntry {
    pub fn from_resolved_config(
        session_id: impl Into<String>,
        repository: impl Into<String>,
        pull_request_number: u64,
        continuity_state: impl Into<String>,
        attention_state: impl Into<String>,
        degraded: bool,
        launch: &ResolvedLaunchBaseline,
        provider: &ResolvedProviderCapability,
        status_reason: Option<String>,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            repository: repository.into(),
            pull_request_number,
            provider: provider.provider.clone(),
            support_tier: provider.support_tier.clone(),
            isolation_mode: launch.isolation_mode.value.clone(),
            policy_profile: provider.policy_profile.id.clone(),
            continuity_state: continuity_state.into(),
            attention_state: attention_state.into(),
            degraded,
            status_reason: status_reason
                .or_else(|| provider.degraded_reason.clone())
                .or_else(|| provider.fail_closed_reason.clone()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadOnlySessionSnapshot {
    pub chrome: SessionChrome,
    pub overview_lines: Vec<String>,
    pub recent_run_lines: Vec<String>,
    pub findings_preview_lines: Vec<String>,
    pub activity_lines: Vec<String>,
    pub jobs: Vec<BackgroundJobSnapshot>,
    pub supervisor: SupervisorSnapshot,
    #[serde(default)]
    pub finding_rows: Vec<FindingListRow>,
    #[serde(default)]
    pub finding_details: Vec<FindingDetail>,
    #[serde(default)]
    pub local_draft_queue: Vec<LocalDraftReviewEntry>,
    #[serde(default)]
    pub active_sessions: Vec<ActiveSessionEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MinimalTuiShell {
    pub chrome: SessionChrome,
    pub panels: Vec<ReadOnlyPanelState>,
    pub active_panel_index: usize,
    pub wake_count: u64,
    pub jobs: Vec<BackgroundJobSnapshot>,
    pub supervisor: SupervisorSnapshot,
    pub finding_rows: Vec<FindingListRow>,
    pub finding_details: Vec<FindingDetail>,
    pub selected_finding_id: Option<String>,
    pub triage_intents: Vec<FindingTriageIntent>,
    pub clarification_intents: Vec<ClarificationIntent>,
    pub local_draft_queue: Vec<LocalDraftReviewEntry>,
    pub active_sessions: Vec<ActiveSessionEntry>,
    pub active_session_index: usize,
    pub posting_requested: bool,
    overview_lines: Vec<String>,
    recent_run_lines: Vec<String>,
    findings_preview_lines: Vec<String>,
    activity_lines: Vec<String>,
}

impl MinimalTuiShell {
    pub fn open(snapshot: ReadOnlySessionSnapshot) -> Self {
        let active_sessions = if snapshot.active_sessions.is_empty() {
            vec![ActiveSessionEntry {
                session_id: snapshot.chrome.session_id.clone(),
                repository: snapshot.chrome.repository.clone(),
                pull_request_number: snapshot.chrome.pull_request_number,
                provider: snapshot.chrome.provider.clone(),
                support_tier: snapshot.chrome.support_tier.clone(),
                isolation_mode: snapshot.chrome.isolation_mode.clone(),
                policy_profile: snapshot.chrome.policy_profile.clone(),
                continuity_state: snapshot.chrome.continuity_state.clone(),
                attention_state: snapshot.chrome.attention_state.clone(),
                degraded: false,
                status_reason: snapshot.chrome.status_reason.clone(),
            }]
        } else {
            snapshot.active_sessions
        };
        let active_session_index = active_sessions
            .iter()
            .position(|entry| entry.session_id == snapshot.chrome.session_id)
            .unwrap_or(0);

        let selected_finding_id = snapshot
            .finding_rows
            .first()
            .map(|row| row.finding_id.clone())
            .or_else(|| {
                snapshot
                    .finding_details
                    .first()
                    .map(|detail| detail.finding_id.clone())
            });

        let mut shell = Self {
            chrome: snapshot.chrome,
            panels: Vec::new(),
            active_panel_index: 0,
            wake_count: 0,
            jobs: snapshot.jobs,
            supervisor: snapshot.supervisor,
            finding_rows: snapshot.finding_rows,
            finding_details: snapshot.finding_details,
            selected_finding_id,
            triage_intents: Vec::new(),
            clarification_intents: Vec::new(),
            local_draft_queue: snapshot.local_draft_queue,
            active_sessions,
            active_session_index,
            posting_requested: false,
            overview_lines: snapshot.overview_lines,
            recent_run_lines: snapshot.recent_run_lines,
            findings_preview_lines: snapshot.findings_preview_lines,
            activity_lines: snapshot.activity_lines,
        };
        shell.rebuild_panels();
        shell
    }

    pub fn render_chrome_line(&self) -> String {
        format!(
            "{} · PR #{} · {} · {} · {} · {}",
            self.chrome.repository,
            self.chrome.pull_request_number,
            self.chrome.provider,
            self.chrome.support_tier,
            self.chrome.isolation_mode,
            self.chrome.attention_state
        )
    }

    pub fn active_panel(&self) -> &ReadOnlyPanelState {
        &self.panels[self.active_panel_index]
    }

    pub fn active_session(&self) -> &ActiveSessionEntry {
        &self.active_sessions[self.active_session_index]
    }

    pub fn switch_to_next_session(&mut self) -> bool {
        if self.active_sessions.len() < 2 {
            return false;
        }
        self.active_session_index = (self.active_session_index + 1) % self.active_sessions.len();
        self.apply_active_session_chrome();
        self.rebuild_panels();
        true
    }

    pub fn switch_to_previous_session(&mut self) -> bool {
        if self.active_sessions.len() < 2 {
            return false;
        }
        self.active_session_index = if self.active_session_index == 0 {
            self.active_sessions.len() - 1
        } else {
            self.active_session_index - 1
        };
        self.apply_active_session_chrome();
        self.rebuild_panels();
        true
    }

    pub fn switch_to_session(&mut self, session_id: &str) -> bool {
        let Some(index) = self
            .active_sessions
            .iter()
            .position(|entry| entry.session_id == session_id)
        else {
            return false;
        };
        self.active_session_index = index;
        self.apply_active_session_chrome();
        self.rebuild_panels();
        true
    }

    pub fn navigate_next_panel(&mut self) {
        self.active_panel_index = (self.active_panel_index + 1) % self.panels.len();
    }

    pub fn navigate_previous_panel(&mut self) {
        self.active_panel_index = if self.active_panel_index == 0 {
            self.panels.len() - 1
        } else {
            self.active_panel_index - 1
        };
    }

    pub fn select_finding(&mut self, finding_id: &str) -> bool {
        if !self
            .finding_rows
            .iter()
            .any(|row| row.finding_id == finding_id)
            && !self
                .finding_details
                .iter()
                .any(|detail| detail.finding_id == finding_id)
        {
            return false;
        }

        self.selected_finding_id = Some(finding_id.to_owned());
        self.rebuild_panels();
        true
    }

    pub fn selected_finding_detail(&self) -> Option<&FindingDetail> {
        let selected_id = self.selected_finding_id.as_deref()?;
        self.finding_details
            .iter()
            .find(|detail| detail.finding_id == selected_id)
    }

    pub fn record_triage_intent(
        &mut self,
        finding_id: &str,
        to_state: &str,
        recorded_at: i64,
    ) -> bool {
        let Some(row) = self
            .finding_rows
            .iter_mut()
            .find(|row| row.finding_id == finding_id)
        else {
            return false;
        };

        let from_state = row.triage_state.clone();
        row.triage_state = to_state.to_owned();
        self.triage_intents.push(FindingTriageIntent {
            finding_id: finding_id.to_owned(),
            from_state,
            to_state: to_state.to_owned(),
            recorded_at,
        });
        self.rebuild_panels();
        true
    }

    pub fn queue_clarification_intent(
        &mut self,
        intent_id: &str,
        finding_id: &str,
        prompt: &str,
        created_at: i64,
    ) -> bool {
        if !self
            .finding_rows
            .iter()
            .any(|row| row.finding_id == finding_id)
            && !self
                .finding_details
                .iter()
                .any(|detail| detail.finding_id == finding_id)
        {
            return false;
        }

        self.clarification_intents.push(ClarificationIntent {
            intent_id: intent_id.to_owned(),
            finding_id: finding_id.to_owned(),
            prompt: prompt.to_owned(),
            status: ClarificationIntentStatus::Queued,
            created_at,
        });
        self.rebuild_panels();
        true
    }

    pub fn review_draft(
        &mut self,
        draft_id: &str,
        decision: DraftReviewDecision,
        edited_body: Option<&str>,
        invalidation_reason: Option<&str>,
        updated_at: i64,
    ) -> bool {
        let Some(entry) = self
            .local_draft_queue
            .iter_mut()
            .find(|entry| entry.draft_id == draft_id)
        else {
            return false;
        };

        entry.decision = decision;
        entry.updated_at = updated_at;
        entry.edited_body = if matches!(decision, DraftReviewDecision::Edited) {
            edited_body.map(ToOwned::to_owned)
        } else {
            None
        };
        entry.invalidation_reason = if matches!(decision, DraftReviewDecision::Invalidated) {
            invalidation_reason.map(ToOwned::to_owned)
        } else {
            None
        };
        entry.pending_post = matches!(decision, DraftReviewDecision::Approved);
        entry.post_failure_reason = None;
        entry.recovery_hint = None;

        self.rebuild_panels();
        true
    }

    pub fn mark_draft_post_failed(
        &mut self,
        draft_id: &str,
        failure_reason: &str,
        recovery_hint: Option<&str>,
        updated_at: i64,
    ) -> bool {
        let Some(entry) = self
            .local_draft_queue
            .iter_mut()
            .find(|entry| entry.draft_id == draft_id)
        else {
            return false;
        };

        entry.pending_post = false;
        entry.post_failure_reason = Some(failure_reason.to_owned());
        entry.recovery_hint = recovery_hint.map(ToOwned::to_owned);
        entry.updated_at = updated_at;

        self.rebuild_panels();
        true
    }

    pub fn pending_post_drafts(&self) -> Vec<&LocalDraftReviewEntry> {
        self.local_draft_queue
            .iter()
            .filter(|entry| entry.pending_post)
            .collect()
    }

    pub fn apply_snapshot(&mut self, snapshot: ReadOnlySessionSnapshot) {
        let preferred_session_id = self
            .active_sessions
            .get(self.active_session_index)
            .map(|entry| entry.session_id.clone());
        let selected_finding_id = self.selected_finding_id.clone();
        let active_sessions = if snapshot.active_sessions.is_empty() {
            vec![ActiveSessionEntry {
                session_id: snapshot.chrome.session_id.clone(),
                repository: snapshot.chrome.repository.clone(),
                pull_request_number: snapshot.chrome.pull_request_number,
                provider: snapshot.chrome.provider.clone(),
                support_tier: snapshot.chrome.support_tier.clone(),
                isolation_mode: snapshot.chrome.isolation_mode.clone(),
                policy_profile: snapshot.chrome.policy_profile.clone(),
                continuity_state: snapshot.chrome.continuity_state.clone(),
                attention_state: snapshot.chrome.attention_state.clone(),
                degraded: false,
                status_reason: snapshot.chrome.status_reason.clone(),
            }]
        } else {
            snapshot.active_sessions
        };

        self.chrome = snapshot.chrome;
        self.jobs = snapshot.jobs;
        self.supervisor = snapshot.supervisor;
        self.finding_rows = snapshot.finding_rows;
        self.finding_details = snapshot.finding_details;
        self.local_draft_queue = snapshot.local_draft_queue;
        self.active_sessions = active_sessions;
        self.overview_lines = snapshot.overview_lines;
        self.recent_run_lines = snapshot.recent_run_lines;
        self.findings_preview_lines = snapshot.findings_preview_lines;
        self.activity_lines = snapshot.activity_lines;
        self.posting_requested = false;

        self.active_session_index = preferred_session_id
            .and_then(|session_id| {
                self.active_sessions
                    .iter()
                    .position(|entry| entry.session_id == session_id)
            })
            .or_else(|| {
                self.active_sessions
                    .iter()
                    .position(|entry| entry.session_id == self.chrome.session_id)
            })
            .unwrap_or(0);
        self.apply_active_session_chrome();

        self.selected_finding_id = selected_finding_id
            .filter(|finding_id| self.has_finding(finding_id))
            .or_else(|| self.default_selected_finding_id());

        self.rebuild_panels();
    }

    pub fn apply_wake_signal(&mut self, wake: WakeSignal) {
        self.wake_count += 1;
        self.jobs = wake.jobs;
        if let Some(supervisor) = wake.supervisor {
            self.supervisor = supervisor;
        }
    }

    fn rebuild_panels(&mut self) {
        let previous_kind = self
            .panels
            .get(self.active_panel_index)
            .map(|panel| panel.kind.clone());

        let mut panels = vec![
            ReadOnlyPanelState {
                kind: ShellPanelKind::SessionOverview,
                title: "Session".to_owned(),
                lines: self.render_session_overview_lines(),
            },
            ReadOnlyPanelState {
                kind: ShellPanelKind::RecentRuns,
                title: "Recent Runs".to_owned(),
                lines: self.recent_run_lines.clone(),
            },
            ReadOnlyPanelState {
                kind: ShellPanelKind::FindingsList,
                title: "Findings".to_owned(),
                lines: self.render_findings_lines(),
            },
            ReadOnlyPanelState {
                kind: ShellPanelKind::FindingDetail,
                title: "Finding Detail".to_owned(),
                lines: self.render_active_detail_lines(),
            },
            ReadOnlyPanelState {
                kind: ShellPanelKind::DraftApprovalQueue,
                title: "Draft Queue".to_owned(),
                lines: self.render_draft_queue_lines(),
            },
            ReadOnlyPanelState {
                kind: ShellPanelKind::ActivityFeed,
                title: "Activity".to_owned(),
                lines: self.activity_lines.clone(),
            },
        ];

        panels.retain(|panel| !panel.lines.is_empty());
        if panels.is_empty() {
            panels.push(ReadOnlyPanelState {
                kind: ShellPanelKind::SessionOverview,
                title: "Session".to_owned(),
                lines: vec!["No read-only session data available".to_owned()],
            });
        }

        self.panels = panels;

        if let Some(kind) = previous_kind {
            if let Some(index) = self.panels.iter().position(|panel| panel.kind == kind) {
                self.active_panel_index = index;
                return;
            }
        }

        if self.active_panel_index >= self.panels.len() {
            self.active_panel_index = 0;
        }
    }

    fn render_findings_lines(&self) -> Vec<String> {
        if self.finding_rows.is_empty() {
            return self.findings_preview_lines.clone();
        }

        self.finding_rows
            .iter()
            .map(|row| {
                let mut line = format!(
                    "[{}] {} · triage={} · outbound={}",
                    row.severity, row.title, row.triage_state, row.outbound_state
                );
                if let Some(lineage) = row.refresh_lineage.as_deref() {
                    line.push_str(&format!(" · lineage={lineage}"));
                }
                if row.degraded {
                    line.push_str(" · degraded");
                }
                line
            })
            .collect()
    }

    fn render_session_overview_lines(&self) -> Vec<String> {
        let mut lines = self.overview_lines.clone();
        let active = self.active_session();
        lines.push(format!(
            "active_session={} · {}#{} · {} · tier={} · isolation={}",
            active.session_id,
            active.repository,
            active.pull_request_number,
            active.provider,
            active.support_tier,
            active.isolation_mode
        ));
        lines.push(format!("policy_profile={}", active.policy_profile));
        if active.degraded {
            lines.push("active_session_degraded=true".to_owned());
        }
        if let Some(reason) = active.status_reason.as_deref() {
            lines.push(format!("status_reason={reason}"));
        }
        if self.active_sessions.len() > 1 {
            lines.push("available_sessions:".to_owned());
            for (index, session) in self.active_sessions.iter().enumerate() {
                let marker = if index == self.active_session_index {
                    "*"
                } else {
                    "-"
                };
                lines.push(format!(
                    "{marker} {} {}#{} {} tier={} isolation={}",
                    session.session_id,
                    session.repository,
                    session.pull_request_number,
                    session.attention_state,
                    session.support_tier,
                    session.isolation_mode
                ));
            }
        }
        lines
    }

    fn render_active_detail_lines(&self) -> Vec<String> {
        let Some(selected_id) = self.selected_finding_id.as_deref() else {
            return Vec::new();
        };

        let Some(detail) = self
            .finding_details
            .iter()
            .find(|detail| detail.finding_id == selected_id)
        else {
            return vec![format!("No detail loaded for finding {selected_id}")];
        };

        let row = self
            .finding_rows
            .iter()
            .find(|row| row.finding_id == selected_id);
        let mut lines = vec![
            format!("Finding {}", detail.finding_id),
            detail.normalized_summary.clone(),
        ];

        if let Some(row) = row {
            lines.push(format!(
                "triage={} · outbound={}",
                row.triage_state, row.outbound_state
            ));
        }
        if let Some(lineage) = detail.refresh_lineage.as_deref() {
            lines.push(format!("refresh_lineage={lineage}"));
        }
        if let Some(reason) = detail.degraded_reason.as_deref() {
            lines.push(format!("degraded_reason={reason}"));
        }

        if !detail.evidence.is_empty() {
            lines.push("evidence:".to_owned());
            for snippet in &detail.evidence {
                let end = snippet
                    .end_line
                    .map(|line| format!("-{line}"))
                    .unwrap_or_default();
                lines.push(format!(
                    "{}:{}{} {}",
                    snippet.path, snippet.start_line, end, snippet.excerpt
                ));
            }
        }

        let pending_clarifications = self
            .clarification_intents
            .iter()
            .filter(|intent| {
                intent.finding_id == selected_id
                    && matches!(intent.status, ClarificationIntentStatus::Queued)
            })
            .count();
        if pending_clarifications > 0 {
            lines.push(format!(
                "clarification_intents_pending={pending_clarifications}"
            ));
        }

        lines
    }

    fn render_draft_queue_lines(&self) -> Vec<String> {
        self.local_draft_queue
            .iter()
            .map(|entry| {
                let mut line = format!("{} · {}", entry.draft_id, entry.decision.as_str());
                if entry.pending_post {
                    line.push_str(" · pending_post");
                }
                if let Some(reason) = entry.invalidation_reason.as_deref() {
                    line.push_str(&format!(" · reason={reason}"));
                }
                if let Some(reason) = entry.post_failure_reason.as_deref() {
                    line.push_str(&format!(" · post_failed={reason}"));
                }
                if let Some(hint) = entry.recovery_hint.as_deref() {
                    line.push_str(&format!(" · recovery={hint}"));
                }
                line
            })
            .collect()
    }

    fn apply_active_session_chrome(&mut self) {
        let active = self.active_session().clone();
        self.chrome.session_id = active.session_id;
        self.chrome.repository = active.repository;
        self.chrome.pull_request_number = active.pull_request_number;
        self.chrome.provider = active.provider;
        self.chrome.support_tier = active.support_tier;
        self.chrome.isolation_mode = active.isolation_mode;
        self.chrome.policy_profile = active.policy_profile;
        self.chrome.continuity_state = active.continuity_state;
        self.chrome.attention_state = active.attention_state;
        self.chrome.status_reason = active.status_reason;
    }

    fn has_finding(&self, finding_id: &str) -> bool {
        self.finding_rows
            .iter()
            .any(|row| row.finding_id == finding_id)
            || self
                .finding_details
                .iter()
                .any(|detail| detail.finding_id == finding_id)
    }

    fn default_selected_finding_id(&self) -> Option<String> {
        self.finding_rows
            .first()
            .map(|row| row.finding_id.clone())
            .or_else(|| {
                self.finding_details
                    .first()
                    .map(|detail| detail.finding_id.clone())
            })
    }
}
