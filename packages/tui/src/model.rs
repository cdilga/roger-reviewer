//! Pure cockpit view-model and reducer.
//!
//! This module owns the full first-release operator grammar: screen
//! transitions, selection movement, multi-select, filtering/sorting/grouping,
//! the elevated approve confirmation state machine, search, timeline
//! navigation, and the help overlay. It deliberately has no dependency on
//! ftui or on a TTY so the whole grammar is unit testable. The thin ftui
//! adapter in `app.rs` only translates terminal events into [`CockpitKey`]
//! values and paints the rendered lines produced by `view.rs`.

use crate::backend::CockpitBackend;
use std::collections::BTreeSet;

/// Operator-settable triage states. `new` and `stale` are Roger-derived and
/// deliberately absent, matching the `rr triage` vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriageAction {
    Accepted,
    Ignored,
    NeedsFollowUp,
    Resolved,
}

impl TriageAction {
    pub fn as_state_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Ignored => "ignored",
            Self::NeedsFollowUp => "needs_follow_up",
            Self::Resolved => "resolved",
        }
    }
}

/// Terminal-agnostic key input for the cockpit reducer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CockpitKey {
    Up,
    Down,
    Enter,
    Esc,
    Backspace,
    Char(char),
}

/// What the runtime loop should do after a key was applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyOutcome {
    Continue,
    Quit,
}

/// Durable operator destinations (contract: Home, Findings, Drafts,
/// Search/History, Sessions). Home doubles as the global session finder, so
/// the session list is not a second peer workspace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    /// Explicit session-reentry disambiguation. Listed only when the caller
    /// hands the cockpit picker candidates (a `PickerRequired` resolution);
    /// Esc falls back to the full session finder (`Home`).
    Picker,
    Home,
    SessionHome,
    Findings,
    Drafts,
    DraftItems,
    Timeline,
    Search,
}

/// Overlays/drawers per contract: help and the elevated approve confirmation
/// are behavior, not peer workspaces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Overlay {
    None,
    Help,
    /// Elevated mutation gate for in-process batch approval. The operator
    /// must type the confirmation word and press Enter; the displayed
    /// command is the exact CLI equivalent.
    Elevation {
        batch_id: String,
        command: String,
        input: String,
    },
}

/// Which text buffer (if any) currently captures character input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputMode {
    None,
    SessionFilter,
    FindingFilter,
    SearchQuery,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortKey {
    Stored,
    Severity,
    Triage,
    Outbound,
    File,
}

impl SortKey {
    pub fn next(self) -> Self {
        match self {
            Self::Stored => Self::Severity,
            Self::Severity => Self::Triage,
            Self::Triage => Self::Outbound,
            Self::Outbound => Self::File,
            Self::File => Self::Stored,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Stored => "stored",
            Self::Severity => "severity",
            Self::Triage => "triage",
            Self::Outbound => "outbound",
            Self::File => "file",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroupKey {
    None,
    Severity,
    Triage,
    Outbound,
    File,
}

impl GroupKey {
    pub fn next(self) -> Self {
        match self {
            Self::None => Self::Severity,
            Self::Severity => Self::Triage,
            Self::Triage => Self::Outbound,
            Self::Outbound => Self::File,
            Self::File => Self::None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Severity => "severity",
            Self::Triage => "triage",
            Self::Outbound => "outbound",
            Self::File => "file",
        }
    }
}

// --- row/view types projected from canonical storage records ----------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionRow {
    pub session_id: String,
    pub repository: String,
    pub pull_request: u64,
    pub provider: String,
    pub attention_state: String,
    pub updated_at: i64,
}

/// One disambiguation candidate for the session picker. The CLI produces these
/// from `SessionReentryResolution::PickerRequired`; the picker lists only the
/// candidate sessions for a single repo/PR so the operator can choose which one
/// to open. `continuity_tier` is optional because the finder projection does
/// not always carry it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickerCandidate {
    pub session_id: String,
    pub repository: String,
    pub pull_request: u64,
    pub provider: String,
    pub attention_state: String,
    pub continuity_tier: Option<String>,
    pub updated_at: i64,
}

/// Cockpit entry payload. Carries the resolved single session (opens session
/// home directly) and/or disambiguation candidates (opens the picker). Both
/// empty means the full session finder is the entry screen.
#[derive(Clone, Debug, Default)]
pub struct CockpitEntry {
    pub initial_session_id: Option<String>,
    pub picker_candidates: Option<Vec<PickerCandidate>>,
}

impl CockpitEntry {
    /// Entry that opens on a resolved single session (or the finder when
    /// `None`), with no picker candidates.
    pub fn from_session(initial_session_id: Option<String>) -> Self {
        Self {
            initial_session_id,
            picker_candidates: None,
        }
    }
}

/// Side effect the pure reducer cannot perform itself. The runtime layer drains
/// this after each key and performs the terminal-coupled work (currently:
/// suspend the TUI and open `$EDITOR` on a draft body).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelEffect {
    /// Open `$EDITOR` on the draft body, then call
    /// [`CockpitModel::apply_draft_revision`] with the edited text.
    EditDraft { draft_id: String, body: String },
}

/// Session-level retrieval/memory posture for the status strip and Search
/// screen. Projected from the storage semantic-component state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MemoryPostureRow {
    /// Default retrieval posture: `hybrid` when the semantic lane is
    /// operational, otherwise `lexical_only`.
    pub retrieval_mode: String,
    pub semantic_operational: bool,
    pub assets_verified: bool,
    pub embedder_available: bool,
    pub embedder_backend: Option<String>,
    pub degraded_reasons: Vec<String>,
}

impl MemoryPostureRow {
    /// One-line summary for the status strip (`retrieval_mode` + semantic
    /// posture + degraded count).
    pub fn summary(&self) -> String {
        let semantic = if self.semantic_operational {
            "operational".to_owned()
        } else if self.degraded_reasons.is_empty() {
            "degraded".to_owned()
        } else {
            format!("degraded({})", self.degraded_reasons.len())
        };
        format!("retrieval_mode={} semantic={semantic}", self.retrieval_mode)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionHomeView {
    pub findings_total: usize,
    /// (triage_state, count) pairs in stable display order.
    pub triage_counts: Vec<(String, usize)>,
    /// (outbound_state, count) pairs in stable display order.
    pub outbound_counts: Vec<(String, usize)>,
    pub latest_run_id: Option<String>,
    pub latest_run_kind: Option<String>,
    /// Count of worker stage results submitted for the latest run. Zero with a
    /// present `latest_run_id` means the run exists but the worker has not
    /// submitted stage results yet (used by the explained empty-findings
    /// state).
    pub latest_run_stage_count: usize,
    pub run_count: usize,
    pub draft_count: usize,
    pub posted_action_count: usize,
}

/// One findings-queue row. Lineage columns come straight from the
/// materialized finding record (first/last seen stage + run).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindingRow {
    pub finding_id: String,
    pub fingerprint: String,
    pub title: String,
    pub normalized_summary: String,
    pub severity: String,
    pub confidence: String,
    pub triage_state: String,
    pub outbound_state: String,
    pub first_seen_stage: String,
    pub first_run_id: String,
    pub last_seen_stage: Option<String>,
    pub last_seen_run_id: Option<String>,
    /// Strongest code-evidence path, when code evidence exists.
    pub primary_file: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvidenceRow {
    pub repo_rel_path: String,
    pub start_line: i64,
    pub end_line: Option<i64>,
    pub evidence_role: String,
    pub anchor_state: String,
}

impl EvidenceRow {
    pub fn locator(&self) -> String {
        match self.end_line {
            Some(end) if end != self.start_line => {
                format!("{}:{}-{}", self.repo_rel_path, self.start_line, end)
            }
            _ => format!("{}:{}", self.repo_rel_path, self.start_line),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecisionEventRow {
    pub triage_state: String,
    pub outbound_state: String,
    pub created_at: i64,
}

/// Linked outbound-draft state for a finding (canonical projection from
/// storage; canonical outbound vocabulary only).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OutboundLinkRow {
    pub state: String,
    pub draft_id: Option<String>,
    pub draft_batch_id: Option<String>,
    pub posted_action_status: Option<String>,
    pub remote_identifier: Option<String>,
    pub invalidation_reason_code: Option<String>,
    pub failure_code: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindingDetail {
    pub finding_id: String,
    pub evidence: Vec<EvidenceRow>,
    pub outbound: OutboundLinkRow,
    pub decision_events: Vec<DecisionEventRow>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DraftBatchRow {
    pub batch_id: String,
    /// Canonical outbound-state vocabulary projected by storage.
    pub state: String,
    pub item_count: usize,
    pub payload_digest: String,
    pub invalidation_reason_code: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DraftItemRow {
    pub draft_id: String,
    pub finding_id: Option<String>,
    pub target_locator: String,
    pub body: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageResultRow {
    pub stage: String,
    pub task_kind: String,
    pub outcome: String,
    pub summary: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimelineRunRow {
    pub run_id: String,
    pub run_kind: String,
    pub continuity_quality: String,
    pub created_at: i64,
    pub stages: Vec<StageResultRow>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PostedActionRow {
    pub action_id: String,
    pub batch_id: String,
    pub remote_identifier: String,
    pub status: String,
    pub failure_code: Option<String>,
    pub posted_at: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TimelineView {
    /// Newest run first.
    pub runs: Vec<TimelineRunRow>,
    pub posted_actions: Vec<PostedActionRow>,
}

/// Flattened timeline cursor target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimelineEntry {
    Run(usize),
    Stage(usize, usize),
    Posted(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchHitRow {
    pub finding_id: String,
    pub session_id: String,
    pub repository: String,
    pub pull_request: u64,
    pub title: String,
    pub normalized_summary: String,
    pub severity: String,
    pub triage_state: String,
    pub outbound_state: String,
    pub fused_score: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchView {
    pub scope_bucket: String,
    pub retrieval_mode: String,
    pub degraded_reasons: Vec<String>,
    pub hits: Vec<SearchHitRow>,
}

/// One pending memory-review request row in the operator promotion-review view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryReviewRow {
    pub id: String,
    pub source: String,
    pub request_kind: String,
    pub statement: String,
    pub normalized_key: String,
    pub status: String,
    pub created_at: i64,
}

/// Outcome of resolving a memory-review request through the cockpit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryReviewResolutionRow {
    pub id: String,
    pub status: String,
    pub resulting_memory_item_id: Option<String>,
    pub materialized_new_item: bool,
}

/// Which lane the Search screen is showing: prior-review hits or the pending
/// memory-review requests operator surface.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SearchMode {
    #[default]
    Hits,
    Reviews,
}

/// Result of the in-process elevated approval (same storage path as
/// `rr approve`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApproveOutcome {
    Approved {
        draft_count: usize,
        already_recorded: bool,
    },
    Blocked {
        reason_code: String,
    },
}

// --- bounded hints (honest about what this slice does and does not do) ------

/// Posting stays CLI-only and visually elevated.
pub const ELEVATED_MUTATION_HINT: &str = "POSTING STAYS CLI-ONLY: rr post --batch <id>";

/// Quickstart hint shown by the empty cockpit.
pub const EMPTY_STORE_HINT: &str = "no review sessions yet — run rr review --pr <number> to start";

/// Clarify-in-place and the session chat lane need a live worker transport.
pub const CLARIFY_HINT: &str =
    "clarification requires an active worker session — run rr review/resume";

/// Provider launch from inside the TUI is deferred in this slice.
pub const LAUNCH_DEFERRED_HINT: &str =
    "launching providers from inside the TUI is deferred — run the shown rr command in a shell";

/// Exact word the operator must type in the elevation prompt.
pub const APPROVE_CONFIRMATION_WORD: &str = "approve";

// --- model -------------------------------------------------------------------

#[derive(Debug)]
pub struct CockpitModel {
    pub screen: Screen,
    pub overlay: Overlay,
    pub input_mode: InputMode,

    // Session picker (disambiguation state)
    pub picker_candidates: Vec<PickerCandidate>,
    pub picker_cursor: usize,

    // Home / global session finder
    pub sessions: Vec<SessionRow>,
    pub session_filter: String,
    pub session_cursor: usize,

    pub active_session: Option<SessionRow>,
    pub home: Option<SessionHomeView>,

    // Findings queue
    pub findings: Vec<FindingRow>,
    pub finding_cursor: usize,
    pub finding_filter: String,
    pub finding_sort: SortKey,
    pub finding_group: GroupKey,
    pub selected_findings: BTreeSet<String>,
    pub finding_detail: Option<FindingDetail>,

    // Drafts
    pub batches: Vec<DraftBatchRow>,
    pub batch_cursor: usize,
    pub batch_items: Vec<DraftItemRow>,
    pub item_cursor: usize,
    pub open_batch_id: Option<String>,

    // Timeline / history
    pub timeline: Option<TimelineView>,
    pub timeline_cursor: usize,

    // Search / recall
    pub search_input: String,
    pub search: Option<SearchView>,
    pub search_cursor: usize,

    /// Whether the Search screen is showing prior-review hits or the pending
    /// memory-review operator surface.
    pub search_mode: SearchMode,
    /// Pending memory-review requests for the active repo scope.
    pub memory_reviews: Vec<MemoryReviewRow>,
    pub memory_review_cursor: usize,

    /// Session-level retrieval/memory posture (status strip + Search screen).
    pub memory_posture: Option<MemoryPostureRow>,

    /// Side effect the runtime layer must perform (e.g. suspend + open
    /// `$EDITOR`). Drained by the runtime after each key via [`Self::take_effect`].
    pub pending_effect: Option<ModelEffect>,

    /// One-line status notice (last action result or degraded-state note).
    pub notice: Option<String>,
}

impl CockpitModel {
    /// Build the entry model from a resolved single session id (or the finder
    /// when `None`). Thin wrapper over [`Self::bootstrap_with_entry`] so the
    /// original call site keeps working unchanged.
    pub fn bootstrap(
        backend: &mut dyn CockpitBackend,
        initial_session_id: Option<&str>,
    ) -> Result<Self, String> {
        Self::bootstrap_with_entry(
            backend,
            &CockpitEntry::from_session(initial_session_id.map(str::to_owned)),
        )
    }

    /// Build the entry model from a full [`CockpitEntry`].
    ///
    /// Priority: picker candidates (disambiguation) win — the cockpit opens on
    /// [`Screen::Picker`]. Otherwise a resolved single session opens session
    /// home; otherwise (including ambiguity with no candidates) Review Home is
    /// the entry screen.
    pub fn bootstrap_with_entry(
        backend: &mut dyn CockpitBackend,
        entry: &CockpitEntry,
    ) -> Result<Self, String> {
        let mut model = Self {
            screen: Screen::Home,
            overlay: Overlay::None,
            input_mode: InputMode::None,
            picker_candidates: Vec::new(),
            picker_cursor: 0,
            sessions: Vec::new(),
            session_filter: String::new(),
            session_cursor: 0,
            active_session: None,
            home: None,
            findings: Vec::new(),
            finding_cursor: 0,
            finding_filter: String::new(),
            finding_sort: SortKey::Stored,
            finding_group: GroupKey::None,
            selected_findings: BTreeSet::new(),
            finding_detail: None,
            batches: Vec::new(),
            batch_cursor: 0,
            batch_items: Vec::new(),
            item_cursor: 0,
            open_batch_id: None,
            timeline: None,
            timeline_cursor: 0,
            search_input: String::new(),
            search: None,
            search_cursor: 0,
            search_mode: SearchMode::Hits,
            memory_reviews: Vec::new(),
            memory_review_cursor: 0,
            memory_posture: None,
            pending_effect: None,
            notice: None,
        };
        model.sessions = backend.list_sessions()?;
        // Best-effort posture load so the status strip is truthful from entry;
        // a failure degrades to "unknown" rather than blocking the cockpit.
        model.memory_posture = backend.memory_posture().ok();

        let candidates = entry
            .picker_candidates
            .as_ref()
            .filter(|candidates| !candidates.is_empty());
        if let Some(candidates) = candidates {
            model.picker_candidates = candidates.clone();
            model.picker_cursor = 0;
            model.screen = Screen::Picker;
            return Ok(model);
        }

        if let Some(session_id) = entry.initial_session_id.as_deref() {
            if model
                .sessions
                .iter()
                .any(|row| row.session_id == session_id)
            {
                model.focus_session(session_id);
                model.open_session_home(backend);
            } else {
                model.notice = Some(format!(
                    "session {session_id} not found in the session finder; showing Review Home"
                ));
            }
        }
        Ok(model)
    }

    /// Drain the pending runtime effect (editor suspension, etc.).
    pub fn take_effect(&mut self) -> Option<ModelEffect> {
        self.pending_effect.take()
    }

    // --- derived views (pure) -------------------------------------------------

    /// Visible session indices on Review Home: filtered, then ordered by
    /// attention priority (what needs the operator first), then recency.
    pub fn visible_sessions(&self) -> Vec<usize> {
        let needle = self.session_filter.trim().to_ascii_lowercase();
        let mut indices: Vec<usize> = self
            .sessions
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                if needle.is_empty() {
                    return true;
                }
                let haystack = format!(
                    "{} {} #{} {} {}",
                    row.session_id,
                    row.repository,
                    row.pull_request,
                    row.provider,
                    row.attention_state
                )
                .to_ascii_lowercase();
                haystack.contains(&needle)
            })
            .map(|(idx, _)| idx)
            .collect();
        indices.sort_by(|&a, &b| {
            let left = &self.sessions[a];
            let right = &self.sessions[b];
            attention_rank(&left.attention_state)
                .cmp(&attention_rank(&right.attention_state))
                .then_with(|| right.updated_at.cmp(&left.updated_at))
                .then_with(|| a.cmp(&b))
        });
        indices
    }

    pub fn selected_session(&self) -> Option<&SessionRow> {
        let visible = self.visible_sessions();
        visible
            .get(self.session_cursor)
            .map(|&idx| &self.sessions[idx])
    }

    /// Visible finding indices: filtered, then sorted by group key (when
    /// grouping) and the active sort key. Stable against stored order.
    pub fn visible_findings(&self) -> Vec<usize> {
        let needle = self.finding_filter.trim().to_ascii_lowercase();
        let mut indices: Vec<usize> = self
            .findings
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                if needle.is_empty() {
                    return true;
                }
                let haystack = format!(
                    "{} {} {} {} {} {}",
                    row.finding_id,
                    row.title,
                    row.severity,
                    row.triage_state,
                    row.outbound_state,
                    row.primary_file.as_deref().unwrap_or("")
                )
                .to_ascii_lowercase();
                haystack.contains(&needle)
            })
            .map(|(idx, _)| idx)
            .collect();
        indices.sort_by(|&a, &b| {
            let left = &self.findings[a];
            let right = &self.findings[b];
            let group_cmp = match self.finding_group {
                GroupKey::None => std::cmp::Ordering::Equal,
                GroupKey::Severity => {
                    severity_rank(&left.severity).cmp(&severity_rank(&right.severity))
                }
                GroupKey::Triage => left.triage_state.cmp(&right.triage_state),
                GroupKey::Outbound => left.outbound_state.cmp(&right.outbound_state),
                GroupKey::File => group_file(left).cmp(group_file(right)),
            };
            let sort_cmp = match self.finding_sort {
                SortKey::Stored => std::cmp::Ordering::Equal,
                SortKey::Severity => {
                    severity_rank(&left.severity).cmp(&severity_rank(&right.severity))
                }
                SortKey::Triage => left.triage_state.cmp(&right.triage_state),
                SortKey::Outbound => left.outbound_state.cmp(&right.outbound_state),
                SortKey::File => group_file(left).cmp(group_file(right)),
            };
            group_cmp.then(sort_cmp).then(a.cmp(&b))
        });
        indices
    }

    pub fn current_finding(&self) -> Option<&FindingRow> {
        let visible = self.visible_findings();
        visible
            .get(self.finding_cursor)
            .map(|&idx| &self.findings[idx])
    }

    /// Group label for a visible finding under the active group key, or
    /// `None` when grouping is off.
    pub fn finding_group_label(&self, finding: &FindingRow) -> Option<String> {
        match self.finding_group {
            GroupKey::None => None,
            GroupKey::Severity => Some(format!("severity: {}", finding.severity)),
            GroupKey::Triage => Some(format!("triage: {}", finding.triage_state)),
            GroupKey::Outbound => Some(format!("outbound: {}", finding.outbound_state)),
            GroupKey::File => Some(format!("file: {}", group_file(finding))),
        }
    }

    /// Explain why the findings queue is empty and what to do next, using the
    /// loaded session home (latest run + worker stage results). Distinguishes:
    /// (a) no review run yet, (b) a run exists but the worker has not submitted
    /// stage results yet, (c) a run completed with zero findings.
    pub fn findings_empty_explanation(&self) -> Vec<String> {
        let pr_hint = self
            .active_session
            .as_ref()
            .map(|session| format!("--pr {}", session.pull_request))
            .unwrap_or_else(|| "--pr <number>".to_owned());
        match &self.home {
            // (a) No run has been persisted for this session yet.
            Some(home) if home.latest_run_id.is_none() => vec![
                "no review run yet for this session".to_owned(),
                format!("run rr review {pr_hint} to start a review"),
            ],
            // (b) A run exists but the worker has submitted no stage results.
            Some(home) if home.latest_run_stage_count == 0 => vec![
                "a review run exists but the worker has not submitted results yet".to_owned(),
                "check rr status for the worker/session state".to_owned(),
            ],
            // (c) A run completed with worker results but produced no findings.
            Some(_) => vec![
                "the latest review run completed with zero findings".to_owned(),
                "nothing to triage for this run".to_owned(),
            ],
            // No session home loaded (should not happen on the findings screen).
            None => vec!["no findings materialized for this session yet".to_owned()],
        }
    }

    pub fn timeline_entries(&self) -> Vec<TimelineEntry> {
        let Some(timeline) = &self.timeline else {
            return Vec::new();
        };
        let mut entries = Vec::new();
        for (run_idx, run) in timeline.runs.iter().enumerate() {
            entries.push(TimelineEntry::Run(run_idx));
            for stage_idx in 0..run.stages.len() {
                entries.push(TimelineEntry::Stage(run_idx, stage_idx));
            }
        }
        for posted_idx in 0..timeline.posted_actions.len() {
            entries.push(TimelineEntry::Posted(posted_idx));
        }
        entries
    }

    /// Attention counts across all loaded sessions (for the status strip).
    pub fn attention_counts(&self) -> Vec<(String, usize)> {
        let mut counts: Vec<(String, usize)> = Vec::new();
        let mut indices: Vec<usize> = (0..self.sessions.len()).collect();
        indices.sort_by_key(|&idx| attention_rank(&self.sessions[idx].attention_state));
        for idx in indices {
            bump_count(&mut counts, &self.sessions[idx].attention_state);
        }
        counts
    }

    // --- reducer ---------------------------------------------------------------

    pub fn handle_key(&mut self, key: CockpitKey, backend: &mut dyn CockpitBackend) -> KeyOutcome {
        // Overlays capture input before anything else.
        match &self.overlay {
            Overlay::Help => {
                if matches!(
                    key,
                    CockpitKey::Esc | CockpitKey::Char('?') | CockpitKey::Char('q')
                ) {
                    self.overlay = Overlay::None;
                }
                return KeyOutcome::Continue;
            }
            Overlay::Elevation { .. } => {
                self.handle_elevation_key(key, backend);
                return KeyOutcome::Continue;
            }
            Overlay::None => {}
        }

        // Active text-input modes capture raw characters next.
        if self.input_mode != InputMode::None {
            self.handle_input_mode_key(key, backend);
            return KeyOutcome::Continue;
        }

        // Navigation keymaps are case-insensitive.
        let key = match key {
            CockpitKey::Char(c) => CockpitKey::Char(c.to_ascii_lowercase()),
            other => other,
        };

        if matches!(key, CockpitKey::Char('q')) {
            return KeyOutcome::Quit;
        }
        if matches!(key, CockpitKey::Char('?')) {
            self.overlay = Overlay::Help;
            return KeyOutcome::Continue;
        }

        match self.screen {
            Screen::Picker => self.handle_picker_key(key, backend),
            Screen::Home => self.handle_home_screen_key(key, backend),
            Screen::SessionHome => self.handle_session_home_key(key, backend),
            Screen::Findings => self.handle_findings_key(key, backend),
            Screen::Drafts => self.handle_drafts_key(key, backend),
            Screen::DraftItems => self.handle_items_key(key),
            Screen::Timeline => self.handle_timeline_key(key, backend),
            Screen::Search => self.handle_search_key(key, backend),
        }
        KeyOutcome::Continue
    }

    fn handle_input_mode_key(&mut self, key: CockpitKey, backend: &mut dyn CockpitBackend) {
        match self.input_mode {
            InputMode::SessionFilter => match key {
                CockpitKey::Char(c) => {
                    self.session_filter.push(c);
                    self.session_cursor = 0;
                }
                CockpitKey::Backspace => {
                    self.session_filter.pop();
                    self.session_cursor = 0;
                }
                CockpitKey::Enter => self.input_mode = InputMode::None,
                CockpitKey::Esc => {
                    self.session_filter.clear();
                    self.session_cursor = 0;
                    self.input_mode = InputMode::None;
                }
                _ => {}
            },
            InputMode::FindingFilter => match key {
                CockpitKey::Char(c) => {
                    self.finding_filter.push(c);
                    self.finding_cursor = 0;
                    self.refresh_finding_detail(backend);
                }
                CockpitKey::Backspace => {
                    self.finding_filter.pop();
                    self.finding_cursor = 0;
                    self.refresh_finding_detail(backend);
                }
                CockpitKey::Enter => self.input_mode = InputMode::None,
                CockpitKey::Esc => {
                    self.finding_filter.clear();
                    self.finding_cursor = 0;
                    self.input_mode = InputMode::None;
                    self.refresh_finding_detail(backend);
                }
                _ => {}
            },
            InputMode::SearchQuery => match key {
                CockpitKey::Char(c) => self.search_input.push(c),
                CockpitKey::Backspace => {
                    self.search_input.pop();
                }
                CockpitKey::Enter => {
                    self.run_search(backend);
                    self.input_mode = InputMode::None;
                }
                CockpitKey::Esc => {
                    self.input_mode = InputMode::None;
                    if self.search.is_none() {
                        self.open_session_home_for_active(backend);
                    }
                }
                _ => {}
            },
            InputMode::None => {}
        }
    }

    fn handle_elevation_key(&mut self, key: CockpitKey, backend: &mut dyn CockpitBackend) {
        let Overlay::Elevation {
            batch_id, input, ..
        } = &mut self.overlay
        else {
            return;
        };
        match key {
            CockpitKey::Char(c) => input.push(c),
            CockpitKey::Backspace => {
                input.pop();
            }
            CockpitKey::Esc => {
                self.notice = Some("elevated approval cancelled".to_owned());
                self.overlay = Overlay::None;
            }
            CockpitKey::Enter => {
                if input.trim() != APPROVE_CONFIRMATION_WORD {
                    self.notice = Some(format!(
                        "type {APPROVE_CONFIRMATION_WORD} exactly to confirm (Esc cancels)"
                    ));
                    return;
                }
                let batch_id = batch_id.clone();
                self.overlay = Overlay::None;
                self.execute_elevated_approval(&batch_id, backend);
            }
            _ => {}
        }
    }

    fn execute_elevated_approval(&mut self, batch_id: &str, backend: &mut dyn CockpitBackend) {
        let Some(session_id) = self.active_session_id() else {
            self.notice = Some("no active session for approval".to_owned());
            return;
        };
        match backend.approve_batch(&session_id, batch_id) {
            Ok(ApproveOutcome::Approved {
                draft_count,
                already_recorded,
            }) => {
                self.notice = Some(if already_recorded {
                    format!("approval already recorded for batch {batch_id}")
                } else {
                    format!(
                        "recorded local approval for batch {batch_id} ({draft_count} draft(s)); posting stays CLI-only"
                    )
                });
                self.reload_batches(backend);
            }
            Ok(ApproveOutcome::Blocked { reason_code }) => {
                self.notice = Some(format!("approve blocked: {reason_code}"));
                self.reload_batches(backend);
            }
            Err(err) => self.notice = Some(format!("error: {err}")),
        }
    }

    fn handle_picker_key(&mut self, key: CockpitKey, backend: &mut dyn CockpitBackend) {
        match key {
            CockpitKey::Up | CockpitKey::Char('k') => move_cursor_up(&mut self.picker_cursor),
            CockpitKey::Down | CockpitKey::Char('j') => {
                move_cursor_down(&mut self.picker_cursor, self.picker_candidates.len());
            }
            CockpitKey::Enter => self.open_picker_selection(backend),
            CockpitKey::Esc => {
                // Fall back to the full session finder.
                self.screen = Screen::Home;
                self.notice = Some("picker dismissed — showing the full session finder".to_owned());
                self.reload_sessions(backend);
            }
            _ => {}
        }
    }

    /// Open the session home for the highlighted picker candidate. Prefers the
    /// canonical [`SessionRow`] from the finder when present, otherwise
    /// synthesizes one from the candidate so the picker still works when the
    /// finder projection is filtered differently.
    fn open_picker_selection(&mut self, backend: &mut dyn CockpitBackend) {
        let Some(candidate) = self.picker_candidates.get(self.picker_cursor).cloned() else {
            return;
        };
        let session = self
            .sessions
            .iter()
            .find(|row| row.session_id == candidate.session_id)
            .cloned()
            .unwrap_or_else(|| SessionRow {
                session_id: candidate.session_id.clone(),
                repository: candidate.repository.clone(),
                pull_request: candidate.pull_request,
                provider: candidate.provider.clone(),
                attention_state: candidate.attention_state.clone(),
                updated_at: candidate.updated_at,
            });
        match backend.load_session_home(&session.session_id) {
            Ok(home) => {
                self.active_session = Some(session);
                self.home = Some(home);
                self.screen = Screen::SessionHome;
                self.notice = None;
                self.reset_per_session_state();
            }
            Err(err) => self.notice = Some(format!("error: {err}")),
        }
    }

    fn handle_home_screen_key(&mut self, key: CockpitKey, backend: &mut dyn CockpitBackend) {
        match key {
            CockpitKey::Up | CockpitKey::Char('k') => move_cursor_up(&mut self.session_cursor),
            CockpitKey::Down | CockpitKey::Char('j') => {
                let len = self.visible_sessions().len();
                move_cursor_down(&mut self.session_cursor, len);
            }
            CockpitKey::Enter => self.open_session_home(backend),
            CockpitKey::Char('/') => self.input_mode = InputMode::SessionFilter,
            CockpitKey::Char('r') => self.reload_sessions(backend),
            CockpitKey::Esc if !self.session_filter.is_empty() => {
                self.session_filter.clear();
                self.session_cursor = 0;
            }
            _ => {}
        }
    }

    fn handle_session_home_key(&mut self, key: CockpitKey, backend: &mut dyn CockpitBackend) {
        match key {
            CockpitKey::Char('f') => self.open_findings(backend),
            CockpitKey::Char('d') => self.open_drafts(backend),
            CockpitKey::Char('t') => self.open_timeline(backend),
            CockpitKey::Char('s') => self.open_search(backend),
            CockpitKey::Char('c') => self.notice = Some(CLARIFY_HINT.to_owned()),
            CockpitKey::Esc => {
                self.screen = Screen::Home;
                self.reload_sessions(backend);
            }
            _ => {}
        }
    }

    fn handle_findings_key(&mut self, key: CockpitKey, backend: &mut dyn CockpitBackend) {
        match key {
            CockpitKey::Up | CockpitKey::Char('k') => {
                move_cursor_up(&mut self.finding_cursor);
                self.refresh_finding_detail(backend);
            }
            CockpitKey::Down | CockpitKey::Char('j') => {
                let len = self.visible_findings().len();
                move_cursor_down(&mut self.finding_cursor, len);
                self.refresh_finding_detail(backend);
            }
            CockpitKey::Char(' ') => self.toggle_selection(),
            CockpitKey::Char('a') => self.triage_targets(TriageAction::Accepted, backend),
            CockpitKey::Char('i') => self.triage_targets(TriageAction::Ignored, backend),
            CockpitKey::Char('n') => self.triage_targets(TriageAction::NeedsFollowUp, backend),
            CockpitKey::Char('r') => self.triage_targets(TriageAction::Resolved, backend),
            CockpitKey::Char('s') => {
                self.finding_sort = self.finding_sort.next();
                self.finding_cursor = 0;
                self.notice = Some(format!("sort: {}", self.finding_sort.label()));
                self.refresh_finding_detail(backend);
            }
            CockpitKey::Char('g') => {
                self.finding_group = self.finding_group.next();
                self.finding_cursor = 0;
                self.notice = Some(format!("group: {}", self.finding_group.label()));
                self.refresh_finding_detail(backend);
            }
            CockpitKey::Char('/') => self.input_mode = InputMode::FindingFilter,
            CockpitKey::Char('c') => self.notice = Some(CLARIFY_HINT.to_owned()),
            CockpitKey::Esc => {
                if !self.selected_findings.is_empty() {
                    self.selected_findings.clear();
                    self.notice = Some("selection cleared".to_owned());
                } else if !self.finding_filter.is_empty() {
                    self.finding_filter.clear();
                    self.finding_cursor = 0;
                    self.refresh_finding_detail(backend);
                } else {
                    self.open_session_home_for_active(backend);
                }
            }
            _ => {}
        }
    }

    fn handle_drafts_key(&mut self, key: CockpitKey, backend: &mut dyn CockpitBackend) {
        match key {
            CockpitKey::Up | CockpitKey::Char('k') => move_cursor_up(&mut self.batch_cursor),
            CockpitKey::Down | CockpitKey::Char('j') => {
                move_cursor_down(&mut self.batch_cursor, self.batches.len());
            }
            CockpitKey::Enter => {
                let Some(batch) = self.batches.get(self.batch_cursor) else {
                    return;
                };
                let batch_id = batch.batch_id.clone();
                match backend.load_batch_items(&batch_id) {
                    Ok(items) => {
                        self.batch_items = items;
                        self.item_cursor = 0;
                        self.open_batch_id = Some(batch_id);
                        self.screen = Screen::DraftItems;
                        self.notice = None;
                    }
                    Err(err) => self.notice = Some(format!("error: {err}")),
                }
            }
            CockpitKey::Char('a') => self.open_elevation_prompt(),
            CockpitKey::Esc => self.open_session_home_for_active(backend),
            _ => {}
        }
    }

    fn open_elevation_prompt(&mut self) {
        let Some(batch) = self.batches.get(self.batch_cursor) else {
            return;
        };
        match batch.state.as_str() {
            "awaiting_approval" => {
                self.overlay = Overlay::Elevation {
                    batch_id: batch.batch_id.clone(),
                    command: format!("rr approve --batch {}", batch.batch_id),
                    input: String::new(),
                };
                self.notice = None;
            }
            "approved" => {
                self.notice = Some(format!(
                    "batch {} is already approved — posting stays CLI-only: rr post --batch {}",
                    batch.batch_id, batch.batch_id
                ));
            }
            other => {
                self.notice = Some(format!(
                    "batch {} is not approvable (state={other})",
                    batch.batch_id
                ));
            }
        }
    }

    fn handle_items_key(&mut self, key: CockpitKey) {
        match key {
            CockpitKey::Up | CockpitKey::Char('k') => move_cursor_up(&mut self.item_cursor),
            CockpitKey::Down | CockpitKey::Char('j') => {
                move_cursor_down(&mut self.item_cursor, self.batch_items.len());
            }
            CockpitKey::Char('e') => self.request_draft_edit(),
            CockpitKey::Esc => {
                self.screen = Screen::Drafts;
                self.open_batch_id = None;
            }
            _ => {}
        }
    }

    /// Emit the editor-suspension effect for the focused draft item. The runtime
    /// layer opens `$EDITOR` on the body and calls back into
    /// [`Self::apply_draft_revision`] with the edited text.
    fn request_draft_edit(&mut self) {
        let Some(item) = self.batch_items.get(self.item_cursor) else {
            return;
        };
        self.pending_effect = Some(ModelEffect::EditDraft {
            draft_id: item.draft_id.clone(),
            body: item.body.clone(),
        });
    }

    /// Persist an edited draft body through the same storage revision path a
    /// future `rr` draft-edit command will use, then reload the batch items so
    /// the read-only preview reflects the revision truthfully. Called by the
    /// runtime after the operator's editor exits.
    pub fn apply_draft_revision(
        &mut self,
        draft_id: &str,
        new_body: &str,
        backend: &mut dyn CockpitBackend,
    ) {
        match backend.revise_draft_body(draft_id, new_body) {
            Ok(()) => {
                if let Some(batch_id) = self.open_batch_id.clone() {
                    match backend.load_batch_items(&batch_id) {
                        Ok(items) => {
                            self.batch_items = items;
                            self.item_cursor = self
                                .item_cursor
                                .min(self.batch_items.len().saturating_sub(1));
                        }
                        Err(err) => {
                            self.notice = Some(format!("error: {err}"));
                            return;
                        }
                    }
                }
                self.notice = Some(format!("revised draft {draft_id} body"));
            }
            Err(err) => self.notice = Some(format!("draft edit failed: {err}")),
        }
    }

    fn handle_timeline_key(&mut self, key: CockpitKey, backend: &mut dyn CockpitBackend) {
        match key {
            CockpitKey::Up | CockpitKey::Char('k') => move_cursor_up(&mut self.timeline_cursor),
            CockpitKey::Down | CockpitKey::Char('j') => {
                let len = self.timeline_entries().len();
                move_cursor_down(&mut self.timeline_cursor, len);
            }
            CockpitKey::Esc => self.open_session_home_for_active(backend),
            _ => {}
        }
    }

    fn handle_search_key(&mut self, key: CockpitKey, backend: &mut dyn CockpitBackend) {
        match self.search_mode {
            SearchMode::Hits => match key {
                CockpitKey::Up | CockpitKey::Char('k') => move_cursor_up(&mut self.search_cursor),
                CockpitKey::Down | CockpitKey::Char('j') => {
                    let len = self.search.as_ref().map_or(0, |view| view.hits.len());
                    move_cursor_down(&mut self.search_cursor, len);
                }
                CockpitKey::Char('/') => self.input_mode = InputMode::SearchQuery,
                CockpitKey::Char('m') => self.toggle_search_mode(backend),
                CockpitKey::Esc => self.open_session_home_for_active(backend),
                _ => {}
            },
            SearchMode::Reviews => match key {
                CockpitKey::Up | CockpitKey::Char('k') => {
                    move_cursor_up(&mut self.memory_review_cursor)
                }
                CockpitKey::Down | CockpitKey::Char('j') => {
                    move_cursor_down(&mut self.memory_review_cursor, self.memory_reviews.len());
                }
                CockpitKey::Char('a') => self.resolve_current_review(true, backend),
                CockpitKey::Char('x') => self.resolve_current_review(false, backend),
                CockpitKey::Char('m') => self.toggle_search_mode(backend),
                CockpitKey::Esc => self.open_session_home_for_active(backend),
                _ => {}
            },
        }
    }

    /// Toggle the Search screen between prior-review hits and the pending
    /// memory-review operator surface, refreshing the review list on entry.
    fn toggle_search_mode(&mut self, backend: &mut dyn CockpitBackend) {
        self.search_mode = match self.search_mode {
            SearchMode::Hits => SearchMode::Reviews,
            SearchMode::Reviews => SearchMode::Hits,
        };
        if self.search_mode == SearchMode::Reviews {
            self.reload_memory_reviews(backend);
            self.input_mode = InputMode::None;
        }
        self.notice = None;
    }

    fn reload_memory_reviews(&mut self, backend: &mut dyn CockpitBackend) {
        let Some(session) = self.active_session.clone() else {
            return;
        };
        match backend.list_pending_memory_reviews(&session.repository) {
            Ok(reviews) => {
                self.memory_reviews = reviews;
                if self.memory_review_cursor >= self.memory_reviews.len() {
                    self.memory_review_cursor = self.memory_reviews.len().saturating_sub(1);
                }
            }
            Err(err) => self.notice = Some(format!("error: {err}")),
        }
    }

    /// Accept or reject the focused pending memory-review request. Acceptance is
    /// the first production memory-item writer; this is a local-only mutation, so
    /// plain keys are fine but the result notice makes the effect explicit.
    fn resolve_current_review(&mut self, accept: bool, backend: &mut dyn CockpitBackend) {
        let Some(row) = self.memory_reviews.get(self.memory_review_cursor).cloned() else {
            self.notice = Some("no pending memory-review request focused".to_owned());
            return;
        };
        match backend.resolve_memory_review(&row.id, accept) {
            Ok(resolution) => {
                let verb = if accept { "accepted" } else { "rejected" };
                self.notice = Some(if accept {
                    match &resolution.resulting_memory_item_id {
                        Some(memory_id) => format!(
                            "review {verb}: memory item {memory_id} {}",
                            if resolution.materialized_new_item {
                                "created"
                            } else {
                                "updated"
                            }
                        ),
                        None => format!("review {verb}"),
                    }
                } else {
                    format!("review {verb}")
                });
                self.reload_memory_reviews(backend);
            }
            Err(err) => self.notice = Some(format!("error: {err}")),
        }
    }

    // --- navigation/loading helpers --------------------------------------------

    fn active_session_id(&self) -> Option<String> {
        self.active_session
            .as_ref()
            .map(|session| session.session_id.clone())
    }

    fn focus_session(&mut self, session_id: &str) {
        let visible = self.visible_sessions();
        if let Some(pos) = visible
            .iter()
            .position(|&idx| self.sessions[idx].session_id == session_id)
        {
            self.session_cursor = pos;
        }
    }

    fn open_session_home(&mut self, backend: &mut dyn CockpitBackend) {
        let Some(session) = self.selected_session().cloned() else {
            return;
        };
        match backend.load_session_home(&session.session_id) {
            Ok(home) => {
                self.active_session = Some(session);
                self.home = Some(home);
                self.screen = Screen::SessionHome;
                self.notice = None;
                self.reset_per_session_state();
            }
            Err(err) => self.notice = Some(format!("error: {err}")),
        }
    }

    /// Reset per-session working state on a session switch.
    fn reset_per_session_state(&mut self) {
        self.selected_findings.clear();
        self.finding_filter.clear();
        self.finding_cursor = 0;
        self.finding_detail = None;
        self.search = None;
        self.search_input.clear();
        self.search_cursor = 0;
        self.search_mode = SearchMode::Hits;
        self.memory_reviews.clear();
        self.memory_review_cursor = 0;
        self.timeline = None;
        self.timeline_cursor = 0;
    }

    fn open_session_home_for_active(&mut self, backend: &mut dyn CockpitBackend) {
        let Some(session_id) = self.active_session_id() else {
            self.screen = Screen::Home;
            return;
        };
        match backend.load_session_home(&session_id) {
            Ok(home) => {
                self.home = Some(home);
                self.screen = Screen::SessionHome;
            }
            Err(err) => self.notice = Some(format!("error: {err}")),
        }
    }

    fn open_findings(&mut self, backend: &mut dyn CockpitBackend) {
        let Some(session_id) = self.active_session_id() else {
            return;
        };
        match backend.load_findings(&session_id) {
            Ok(findings) => {
                self.findings = findings;
                self.finding_cursor = 0;
                self.screen = Screen::Findings;
                self.notice = None;
                // Selection survives refresh only for ids that still exist.
                let existing: BTreeSet<String> = self
                    .findings
                    .iter()
                    .map(|row| row.finding_id.clone())
                    .collect();
                self.selected_findings = self
                    .selected_findings
                    .intersection(&existing)
                    .cloned()
                    .collect();
                self.refresh_finding_detail(backend);
            }
            Err(err) => self.notice = Some(format!("error: {err}")),
        }
    }

    fn open_drafts(&mut self, backend: &mut dyn CockpitBackend) {
        let Some(session_id) = self.active_session_id() else {
            return;
        };
        match backend.load_draft_batches(&session_id) {
            Ok(batches) => {
                self.batches = batches;
                self.batch_cursor = 0;
                self.screen = Screen::Drafts;
                self.notice = None;
            }
            Err(err) => self.notice = Some(format!("error: {err}")),
        }
    }

    fn reload_batches(&mut self, backend: &mut dyn CockpitBackend) {
        let Some(session_id) = self.active_session_id() else {
            return;
        };
        let selected_id = self
            .batches
            .get(self.batch_cursor)
            .map(|row| row.batch_id.clone());
        match backend.load_draft_batches(&session_id) {
            Ok(batches) => {
                self.batches = batches;
                self.batch_cursor = selected_id
                    .and_then(|id| self.batches.iter().position(|row| row.batch_id == id))
                    .unwrap_or(0);
            }
            Err(err) => self.notice = Some(format!("error: {err}")),
        }
    }

    fn open_timeline(&mut self, backend: &mut dyn CockpitBackend) {
        let Some(session_id) = self.active_session_id() else {
            return;
        };
        match backend.load_timeline(&session_id) {
            Ok(timeline) => {
                self.timeline = Some(timeline);
                self.timeline_cursor = 0;
                self.screen = Screen::Timeline;
                self.notice = None;
            }
            Err(err) => self.notice = Some(format!("error: {err}")),
        }
    }

    fn open_search(&mut self, backend: &mut dyn CockpitBackend) {
        if self.active_session.is_none() {
            self.notice = Some("open a session before searching its repo scope".to_owned());
            return;
        }
        // Refresh the memory posture so the Search screen and status strip show
        // the current retrieval mode and any degraded reasons.
        if let Ok(posture) = backend.memory_posture() {
            self.memory_posture = Some(posture);
        }
        // Load pending memory-review requests so the posture line can surface the
        // count even before the operator toggles into the review lane.
        if let Some(session) = self.active_session.clone() {
            self.memory_reviews = backend
                .list_pending_memory_reviews(&session.repository)
                .unwrap_or_default();
        }
        self.search_mode = SearchMode::Hits;
        self.memory_review_cursor = 0;
        self.screen = Screen::Search;
        self.input_mode = InputMode::SearchQuery;
        self.notice = None;
    }

    fn run_search(&mut self, backend: &mut dyn CockpitBackend) {
        let Some(session) = self.active_session.clone() else {
            self.notice = Some("open a session before searching its repo scope".to_owned());
            return;
        };
        if self.search_input.trim().is_empty() {
            self.notice = Some("enter a search query first".to_owned());
            return;
        }
        match backend.search_prior_reviews(&session.repository, self.search_input.trim()) {
            Ok(view) => {
                self.search_cursor = 0;
                self.notice = if view.hits.is_empty() {
                    Some("no prior findings matched in this repo scope".to_owned())
                } else {
                    None
                };
                self.search = Some(view);
            }
            Err(err) => self.notice = Some(format!("error: {err}")),
        }
    }

    fn reload_sessions(&mut self, backend: &mut dyn CockpitBackend) {
        match backend.list_sessions() {
            Ok(sessions) => {
                // Stable selection: keep the previously selected session id
                // focused when it still exists after reload.
                let selected_id = self.selected_session().map(|row| row.session_id.clone());
                self.sessions = sessions;
                self.session_cursor = 0;
                if let Some(id) = selected_id {
                    self.focus_session(&id);
                }
            }
            Err(err) => self.notice = Some(format!("error: {err}")),
        }
    }

    fn toggle_selection(&mut self) {
        let Some(finding) = self.current_finding() else {
            return;
        };
        let id = finding.finding_id.clone();
        if !self.selected_findings.remove(&id) {
            self.selected_findings.insert(id);
        }
    }

    /// Apply a triage action to the current working set (multi-select batch
    /// when a selection exists, otherwise the cursor row) through the same
    /// storage mutation `rr triage` uses, then reload findings and re-render
    /// truthfully. Selection stays bound to finding ids, not row indices.
    fn triage_targets(&mut self, action: TriageAction, backend: &mut dyn CockpitBackend) {
        let targets: Vec<String> = if self.selected_findings.is_empty() {
            match self.current_finding() {
                Some(finding) => vec![finding.finding_id.clone()],
                None => return,
            }
        } else {
            self.selected_findings.iter().cloned().collect()
        };
        let Some(session_id) = self.active_session_id() else {
            return;
        };
        let anchor_id = self
            .current_finding()
            .map(|finding| finding.finding_id.clone());

        for finding_id in &targets {
            if let Err(err) = backend.set_triage_state(finding_id, action) {
                self.notice = Some(format!("error: {err}"));
                return;
            }
        }
        match backend.load_findings(&session_id) {
            Ok(findings) => {
                self.findings = findings;
                let existing: BTreeSet<String> = self
                    .findings
                    .iter()
                    .map(|row| row.finding_id.clone())
                    .collect();
                self.selected_findings = self
                    .selected_findings
                    .intersection(&existing)
                    .cloned()
                    .collect();
                let visible = self.visible_findings();
                self.finding_cursor = anchor_id
                    .and_then(|id| {
                        visible
                            .iter()
                            .position(|&idx| self.findings[idx].finding_id == id)
                    })
                    .unwrap_or_else(|| self.finding_cursor.min(visible.len().saturating_sub(1)));
                self.notice = Some(if targets.len() == 1 {
                    format!("{} -> triage_state={}", targets[0], action.as_state_str())
                } else {
                    format!(
                        "{} findings -> triage_state={}",
                        targets.len(),
                        action.as_state_str()
                    )
                });
                self.refresh_finding_detail(backend);
            }
            Err(err) => self.notice = Some(format!("error: {err}")),
        }
    }

    /// Keep the inspector pane truthful for the focused finding.
    fn refresh_finding_detail(&mut self, backend: &mut dyn CockpitBackend) {
        let Some(finding_id) = self
            .current_finding()
            .map(|finding| finding.finding_id.clone())
        else {
            self.finding_detail = None;
            return;
        };
        if self
            .finding_detail
            .as_ref()
            .is_some_and(|detail| detail.finding_id == finding_id)
        {
            return;
        }
        match backend.load_finding_detail(&finding_id) {
            Ok(detail) => self.finding_detail = Some(detail),
            Err(err) => {
                self.finding_detail = None;
                self.notice = Some(format!("error: {err}"));
            }
        }
    }
}

// --- ordering helpers ---------------------------------------------------------

/// Attention priority: what needs the operator first sorts first.
pub fn attention_rank(state: &str) -> u8 {
    match state {
        "outbound_approval_required" => 0,
        "awaiting_user_input" => 1,
        "refresh_recommended" => 2,
        "review_failed" => 3,
        "awaiting_return" => 4,
        "review_launched" => 5,
        "review_resumed" => 6,
        "returned_to_roger" => 7,
        _ => 8,
    }
}

pub fn severity_rank(severity: &str) -> u8 {
    match severity {
        "blocker" => 0,
        "critical" => 1,
        "high" => 2,
        "medium" => 3,
        "low" => 4,
        "info" => 5,
        _ => 6,
    }
}

fn group_file(finding: &FindingRow) -> &str {
    finding
        .primary_file
        .as_deref()
        .unwrap_or("(no code evidence)")
}

pub(crate) fn bump_count(counts: &mut Vec<(String, usize)>, key: &str) {
    if let Some(entry) = counts.iter_mut().find(|(state, _)| state == key) {
        entry.1 += 1;
    } else {
        counts.push((key.to_owned(), 1));
    }
}

fn move_cursor_up(cursor: &mut usize) {
    *cursor = cursor.saturating_sub(1);
}

fn move_cursor_down(cursor: &mut usize, len: usize) {
    if len == 0 {
        *cursor = 0;
        return;
    }
    *cursor = (*cursor + 1).min(len - 1);
}
