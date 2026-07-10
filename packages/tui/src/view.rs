//! Pure textual rendering of the cockpit (no ftui, no TTY).
//!
//! The workspace shape follows the TUI contract: a top status strip, a
//! primary working pane, and a persistent secondary inspector pane whenever
//! the terminal is wide enough. Everything here is a pure function of
//! [`CockpitModel`] state so degraded/empty rendering and layout bounds are
//! unit testable.

use crate::model::{
    CockpitModel, EMPTY_STORE_HINT, ElevationKind, FindingRow, InputMode, Overlay, Screen,
    SearchMode, TimelineEntry,
};

/// Minimum total width before the inspector pane is rendered side-by-side.
const INSPECTOR_MIN_WIDTH: usize = 96;

impl CockpitModel {
    /// Render the cockpit as plain text lines for a terminal of `width` x
    /// `height` cells. Pure so degraded/empty rendering is unit testable.
    pub fn render_lines(&self, width: usize, height: usize) -> Vec<String> {
        let mut lines = Vec::new();
        lines.push(truncate_line(&self.status_line(), width));
        lines.push("─".repeat(width.min(200)));

        let body_budget = height.saturating_sub(4).max(1);
        let body = match &self.overlay {
            Overlay::Help => self.help_lines(),
            Overlay::Elevation { .. } => self.elevation_lines(),
            Overlay::None if self.input_mode == InputMode::Composer => self.composer_lines(),
            Overlay::None => self.body_lines(width, body_budget),
        };
        for line in body.into_iter().take(body_budget) {
            lines.push(truncate_line(&line, width));
        }

        while lines.len() < height.saturating_sub(2) {
            lines.push(String::new());
        }
        if let Some(notice) = &self.notice {
            lines.push(truncate_line(notice, width));
        } else {
            lines.push(String::new());
        }
        lines.push(truncate_line(&self.key_hints(), width));
        lines.truncate(height.max(4));
        lines
    }

    /// Top status strip: target identity, run state, attention counts.
    fn status_line(&self) -> String {
        let screen = match self.screen {
            Screen::Picker => "Session Picker",
            Screen::Home => "Review Home",
            Screen::SessionHome => "Session Overview",
            Screen::Findings => "Findings Queue",
            Screen::Drafts => "Draft Approval Queue",
            Screen::DraftItems => "Draft Batch",
            Screen::Timeline => "Timeline/History",
            Screen::Search => "Search/Recall",
            Screen::Queue => "Review Queue",
        };
        if self.screen == Screen::Picker {
            return format!(
                "Roger Reviewer — {screen} — {} candidate session(s)",
                self.picker_candidates.len()
            );
        }
        let mut line = match &self.active_session {
            Some(session) if self.screen != Screen::Home => {
                let run = self
                    .home
                    .as_ref()
                    .and_then(|home| home.latest_run_id.as_deref())
                    .unwrap_or("none");
                let mut line = format!(
                    "Roger Reviewer — {screen} — {} PR #{} [{}] attention={} run={}",
                    session.repository,
                    session.pull_request,
                    session.provider,
                    session.attention_state,
                    run
                );
                if self.screen == Screen::Findings {
                    line.push_str(&format!(" — {} findings", self.visible_findings().len()));
                    if !self.selected_findings.is_empty() {
                        line.push_str(&format!(" sel={}", self.selected_findings.len()));
                    }
                }
                line
            }
            _ => {
                let mut line = format!(
                    "Roger Reviewer — {screen} — {} session(s)",
                    self.sessions.len()
                );
                let counts = self.attention_counts();
                if !counts.is_empty() {
                    let summary = counts
                        .iter()
                        .map(|(state, count)| format!("{state}={count}"))
                        .collect::<Vec<_>>()
                        .join(" ");
                    line.push_str(&format!(" — attention: {summary}"));
                }
                line
            }
        };
        // Surface the retrieval/memory posture on the Search screen where it
        // materially affects operator judgment of recall results.
        if self.screen == Screen::Search
            && let Some(posture) = &self.memory_posture
        {
            line.push_str(&format!(" — memory: {}", posture.summary()));
        }
        line
    }

    fn key_hints(&self) -> String {
        match &self.overlay {
            Overlay::Help => return "Esc/? close help".to_owned(),
            Overlay::Elevation { kind, .. } => {
                return format!(
                    "type {} + Enter to confirm  Esc cancel",
                    kind.confirm_word()
                );
            }
            Overlay::None => {}
        }
        match self.input_mode {
            InputMode::SessionFilter | InputMode::FindingFilter => {
                return "type to filter  Enter keep  Esc clear  Backspace delete".to_owned();
            }
            InputMode::SearchQuery => {
                return "type query  Enter search  Esc leave input  Backspace delete".to_owned();
            }
            InputMode::Composer => {
                return "type clarification  Enter submit  Esc cancel  Backspace delete".to_owned();
            }
            InputMode::None => {}
        }
        match self.screen {
            Screen::Picker => {
                "j/k move  Enter open session  Esc full finder  ? help  q quit".to_owned()
            }
            Screen::Home => {
                "j/k move  Enter open  o launch  p pr queue  ! doctor  / filter  r reload  ? help  q quit"
                    .to_owned()
            }
            Screen::SessionHome => {
                "f findings  d drafts  t timeline  s search  c clarify  o launch  Esc home  ? help  q quit"
                    .to_owned()
            }
            Screen::Findings => {
                "j/k move  space select  a/i/n/r triage  d draft  s sort  g group  / filter  c clarify  Esc back  ? help  q quit"
                    .to_owned()
            }
            Screen::Drafts => {
                "j/k move  Enter inspect  a approve  p post (GitHub)  Esc home  ? help  q quit"
                    .to_owned()
            }
            Screen::DraftItems => "j/k move  e edit draft  Esc drafts  ? help  q quit".to_owned(),
            Screen::Timeline => "j/k move  Esc home  ? help  q quit".to_owned(),
            Screen::Queue => {
                "j/k move  Enter start/resume review  r reload  Esc home  ? help  q quit".to_owned()
            }
            Screen::Search => match self.search_mode {
                SearchMode::Hits => {
                    "j/k move  / new query  m review-requests  Esc home  ? help  q quit".to_owned()
                }
                SearchMode::Reviews => {
                    "j/k move  a accept  x reject  m back to hits  Esc home  ? help  q quit"
                        .to_owned()
                }
            },
        }
    }

    /// Primary working pane + secondary inspector pane composition.
    fn body_lines(&self, width: usize, budget: usize) -> Vec<String> {
        let (primary, cursor_line) = self.primary_lines();
        let primary = window_lines(primary, cursor_line, budget);
        if width < INSPECTOR_MIN_WIDTH {
            return primary;
        }
        let inspector = self.inspector_lines();
        let primary_width = (width * 3) / 5;
        let inspector_width = width.saturating_sub(primary_width + 3);
        compose_columns(primary, inspector, primary_width, inspector_width, budget)
    }

    /// Primary pane lines and the index of the cursor row (for windowing).
    fn primary_lines(&self) -> (Vec<String>, usize) {
        match self.screen {
            Screen::Picker => self.render_picker_primary(),
            Screen::Home => self.render_home_primary(),
            Screen::SessionHome => (self.render_session_overview(), 0),
            Screen::Findings => self.render_findings_primary(),
            Screen::Drafts => self.render_drafts_primary(),
            Screen::DraftItems => self.render_items_primary(),
            Screen::Timeline => self.render_timeline_primary(),
            Screen::Search => self.render_search_primary(),
            Screen::Queue => self.render_queue_primary(),
        }
    }

    // --- Session Picker --------------------------------------------------------

    fn render_picker_primary(&self) -> (Vec<String>, usize) {
        let mut lines = Vec::new();
        // One-line disambiguation header keyed on the shared repo/PR.
        let header = match self.picker_candidates.first() {
            Some(first) => format!(
                "{} sessions exist for {} PR #{} — choose one",
                self.picker_candidates.len(),
                first.repository,
                first.pull_request
            ),
            None => "no candidate sessions to disambiguate".to_owned(),
        };
        lines.push(header);
        lines.push(String::new());
        if self.picker_candidates.is_empty() {
            lines.push("Esc returns to the full session finder".to_owned());
            return (lines, 0);
        }
        let mut cursor_line = 0;
        for (idx, candidate) in self.picker_candidates.iter().enumerate() {
            if idx == self.picker_cursor {
                cursor_line = lines.len();
            }
            let tier = candidate
                .continuity_tier
                .as_deref()
                .map(|tier| format!("  continuity={tier}"))
                .unwrap_or_default();
            lines.push(format!(
                "{} {}  [{}]  attention={}{}  updated_at={}",
                cursor_marker(idx == self.picker_cursor),
                candidate.session_id,
                candidate.provider,
                candidate.attention_state,
                tier,
                candidate.updated_at
            ));
        }
        (lines, cursor_line)
    }

    // --- Review Home -----------------------------------------------------------

    fn render_home_primary(&self) -> (Vec<String>, usize) {
        let visible = self.visible_sessions();
        let mut lines = Vec::new();
        if !self.session_filter.is_empty() || self.input_mode == InputMode::SessionFilter {
            let caret = if self.input_mode == InputMode::SessionFilter {
                "_"
            } else {
                ""
            };
            lines.push(format!("filter: {}{caret}", self.session_filter));
        }
        if self.sessions.is_empty() {
            lines.push(String::new());
            lines.push(EMPTY_STORE_HINT.to_owned());
            return (lines, 0);
        }
        if visible.is_empty() {
            lines.push(String::new());
            lines.push("no sessions match this filter (Esc clears)".to_owned());
            return (lines, 0);
        }
        let mut cursor_line = 0;
        let mut last_group: Option<String> = None;
        for (pos, &idx) in visible.iter().enumerate() {
            let row = &self.sessions[idx];
            let group = row.attention_state.clone();
            if last_group.as_ref() != Some(&group) {
                lines.push(format!("── attention: {group}"));
                last_group = Some(group);
            }
            if pos == self.session_cursor {
                cursor_line = lines.len();
            }
            lines.push(format!(
                "{} {}  {} PR #{}  [{}]  updated_at={}",
                cursor_marker(pos == self.session_cursor),
                row.session_id,
                row.repository,
                row.pull_request,
                row.provider,
                row.updated_at
            ));
        }
        (lines, cursor_line)
    }

    // --- Session Overview --------------------------------------------------------

    fn render_session_overview(&self) -> Vec<String> {
        let Some(home) = &self.home else {
            return vec!["session overview not loaded".to_owned()];
        };
        let mut lines = Vec::new();
        if let Some(session) = &self.active_session {
            lines.push(format!("session   {}", session.session_id));
            lines.push(format!(
                "target    {} PR #{}",
                session.repository, session.pull_request
            ));
            lines.push(format!("provider  {}", session.provider));
            lines.push(format!("attention {}", session.attention_state));
        }
        match (&home.latest_run_id, &home.latest_run_kind) {
            (Some(run_id), kind) => lines.push(format!(
                "run       {} ({}) of {} total",
                run_id,
                kind.as_deref().unwrap_or("unknown"),
                home.run_count
            )),
            (None, _) => lines.push("run       none persisted yet".to_owned()),
        }
        lines.push(String::new());
        lines.push(format!("findings  total={}", home.findings_total));
        let triage = home
            .triage_counts
            .iter()
            .map(|(state, count)| format!("{state}={count}"))
            .collect::<Vec<_>>()
            .join("  ");
        lines.push(format!(
            "triage    {}",
            if triage.is_empty() { "none" } else { &triage }
        ));
        let outbound = home
            .outbound_counts
            .iter()
            .map(|(state, count)| format!("{state}={count}"))
            .collect::<Vec<_>>()
            .join("  ");
        lines.push(format!(
            "outbound  {}",
            if outbound.is_empty() {
                "none"
            } else {
                &outbound
            }
        ));
        lines.push(format!(
            "drafts    {}  posted_actions {}",
            home.draft_count, home.posted_action_count
        ));
        lines.push(String::new());
        lines.push("f findings  d drafts  t timeline  s search".to_owned());
        lines
    }

    // --- Findings Queue ----------------------------------------------------------

    fn render_findings_primary(&self) -> (Vec<String>, usize) {
        let visible = self.visible_findings();
        let mut lines = Vec::new();
        lines.push(format!(
            "sort={}  group={}{}",
            self.finding_sort.label(),
            self.finding_group.label(),
            if self.finding_filter.is_empty() && self.input_mode != InputMode::FindingFilter {
                String::new()
            } else {
                let caret = if self.input_mode == InputMode::FindingFilter {
                    "_"
                } else {
                    ""
                };
                format!("  filter: {}{caret}", self.finding_filter)
            }
        ));
        if self.findings.is_empty() {
            lines.push(String::new());
            // Explain *why* the queue is empty and the next step, distinguishing
            // no-run / worker-pending / zero-findings.
            for explanation in self.findings_empty_explanation() {
                lines.push(explanation);
            }
            return (lines, 0);
        }
        if visible.is_empty() {
            lines.push(String::new());
            lines.push("no findings match this filter (Esc clears)".to_owned());
            return (lines, 0);
        }
        let mut cursor_line = 0;
        let mut last_group: Option<String> = None;
        for (pos, &idx) in visible.iter().enumerate() {
            let row = &self.findings[idx];
            if let Some(group) = self.finding_group_label(row)
                && last_group.as_ref() != Some(&group)
            {
                lines.push(format!("── {group}"));
                last_group = Some(group);
            }
            if pos == self.finding_cursor {
                cursor_line = lines.len();
            }
            lines.push(self.finding_row_line(row, pos == self.finding_cursor));
        }
        (lines, cursor_line)
    }

    fn finding_row_line(&self, row: &FindingRow, focused: bool) -> String {
        let selected = if self.selected_findings.contains(&row.finding_id) {
            "*"
        } else {
            " "
        };
        let lineage = match (&row.last_seen_stage, &row.last_seen_run_id) {
            (Some(stage), Some(run)) => format!(
                "{}@{}→{stage}@{run}",
                row.first_seen_stage,
                short_id(&row.first_run_id)
            ),
            _ => format!("{}@{}", row.first_seen_stage, short_id(&row.first_run_id)),
        };
        format!(
            "{}{} {}  sev={}  triage={}  outbound={}  {}  {}  {}",
            cursor_marker(focused),
            selected,
            short_fingerprint(&row.fingerprint),
            row.severity,
            row.triage_state,
            row.outbound_state,
            row.title,
            row.primary_file.as_deref().unwrap_or("(no code evidence)"),
            lineage
        )
    }

    // --- Drafts -------------------------------------------------------------------

    fn render_drafts_primary(&self) -> (Vec<String>, usize) {
        let mut lines = Vec::new();
        let mut cursor_line = 0;
        if self.batches.is_empty() {
            lines.push(String::new());
            lines.push("no outbound draft batches for this session".to_owned());
            lines.push("run rr draft --session <id> --finding <id> to materialize one".to_owned());
        } else {
            for (idx, row) in self.batches.iter().enumerate() {
                let invalidation = row
                    .invalidation_reason_code
                    .as_deref()
                    .map(|code| format!("  reason={code}"))
                    .unwrap_or_default();
                if idx == self.batch_cursor {
                    cursor_line = lines.len();
                }
                lines.push(format!(
                    "{} {}  state={}  items={}{}",
                    cursor_marker(idx == self.batch_cursor),
                    row.batch_id,
                    row.state,
                    row.item_count,
                    invalidation
                ));
            }
        }
        lines.push(String::new());
        lines.push(
            "a opens the elevated approve confirmation for an awaiting_approval batch".to_owned(),
        );
        lines.push(
            "p opens the elevated post confirmation (posts to GitHub) for an approved batch"
                .to_owned(),
        );
        (lines, cursor_line)
    }

    fn render_items_primary(&self) -> (Vec<String>, usize) {
        let mut lines = Vec::new();
        if let Some(batch_id) = &self.open_batch_id {
            lines.push(format!("batch {batch_id} (read-only payload preview)"));
            lines.push(String::new());
        }
        let mut cursor_line = 0;
        if self.batch_items.is_empty() {
            lines.push("no draft items stored for this batch".to_owned());
        } else {
            for (idx, item) in self.batch_items.iter().enumerate() {
                let body_excerpt = item.body.lines().next().unwrap_or("");
                if idx == self.item_cursor {
                    cursor_line = lines.len();
                }
                lines.push(format!(
                    "{} {}  finding={}  target={}  {}",
                    cursor_marker(idx == self.item_cursor),
                    item.draft_id,
                    item.finding_id.as_deref().unwrap_or("-"),
                    item.target_locator,
                    body_excerpt
                ));
            }
        }
        lines.push(String::new());
        lines.push("e edit a draft body in $EDITOR  ·  Esc returns to the batch list".to_owned());
        (lines, cursor_line)
    }

    // --- Timeline -------------------------------------------------------------------

    fn render_timeline_primary(&self) -> (Vec<String>, usize) {
        let Some(timeline) = &self.timeline else {
            return (vec!["timeline not loaded".to_owned()], 0);
        };
        let entries = self.timeline_entries();
        if entries.is_empty() {
            return (
                vec![
                    String::new(),
                    "no review runs persisted for this session yet".to_owned(),
                ],
                0,
            );
        }
        let mut lines = Vec::new();
        let mut cursor_line = 0;
        let mut posted_header_emitted = false;
        for (pos, entry) in entries.iter().enumerate() {
            match entry {
                TimelineEntry::Run(run_idx) => {
                    let run = &timeline.runs[*run_idx];
                    if pos == self.timeline_cursor {
                        cursor_line = lines.len();
                    }
                    lines.push(format!(
                        "{} run {}  kind={}  continuity={}  at={}",
                        cursor_marker(pos == self.timeline_cursor),
                        run.run_id,
                        run.run_kind,
                        run.continuity_quality,
                        run.created_at
                    ));
                }
                TimelineEntry::Stage(run_idx, stage_idx) => {
                    let stage = &timeline.runs[*run_idx].stages[*stage_idx];
                    if pos == self.timeline_cursor {
                        cursor_line = lines.len();
                    }
                    lines.push(format!(
                        "{}   stage {}  task={}  outcome={}  {}",
                        cursor_marker(pos == self.timeline_cursor),
                        stage.stage,
                        stage.task_kind,
                        stage.outcome,
                        stage.summary
                    ));
                }
                TimelineEntry::Posted(posted_idx) => {
                    if !posted_header_emitted {
                        lines.push("── posted actions".to_owned());
                        posted_header_emitted = true;
                    }
                    let posted = &timeline.posted_actions[*posted_idx];
                    if pos == self.timeline_cursor {
                        cursor_line = lines.len();
                    }
                    let failure = posted
                        .failure_code
                        .as_deref()
                        .map(|code| format!("  failure={code}"))
                        .unwrap_or_default();
                    lines.push(format!(
                        "{} posted {}  batch={}  remote={}  status={}{}",
                        cursor_marker(pos == self.timeline_cursor),
                        posted.action_id,
                        posted.batch_id,
                        posted.remote_identifier,
                        posted.status,
                        failure
                    ));
                }
            }
        }
        (lines, cursor_line)
    }

    // --- Search ----------------------------------------------------------------------

    /// Queue screen: open PRs joined to local session truth. Each row shows the
    /// state Roger already has, so the operator can see whether Enter will
    /// start a fresh review or resume an existing one (reuse-or-fresh).
    fn render_queue_primary(&self) -> (Vec<String>, usize) {
        let mut lines = Vec::new();
        let Some(view) = &self.queue else {
            return (vec!["queue not loaded".to_owned()], 0);
        };
        lines.push(format!(
            "open pull requests — {}  ({} shown)",
            view.repository,
            view.rows.len()
        ));
        lines.push(String::new());
        if view.rows.is_empty() {
            lines.push("no open pull requests".to_owned());
            return (lines, 0);
        }
        let header_offset = lines.len();
        lines.push(format!(
            "{:<7} {:<14} {:<6} {:<14} {}",
            "PR", "STATE", "DRAFT", "AUTHOR", "TITLE"
        ));
        for (idx, row) in view.rows.iter().enumerate() {
            let marker = if idx == self.queue_cursor { ">" } else { " " };
            lines.push(format!(
                "{marker}{:<6} {:<14} {:<6} {:<14} {}",
                format!("#{}", row.pr_number),
                row.roger_state,
                if row.is_draft { "yes" } else { "no" },
                truncate_cell(&row.author, 14),
                row.title
            ));
        }
        let cursor_line = header_offset + 1 + self.queue_cursor;
        (lines, cursor_line)
    }

    /// Inspector for the focused queue row: the exact command Enter will run,
    /// and why (fresh vs resume).
    fn inspect_queue_row(&self) -> Vec<String> {
        let Some(row) = self
            .queue
            .as_ref()
            .and_then(|view| view.rows.get(self.queue_cursor))
        else {
            return vec!["no pull request focused".to_owned()];
        };
        let action = if row.is_fresh() {
            "Enter starts a FRESH review (no local session for this PR)"
        } else {
            "Enter RESUMES the existing local session for this PR"
        };
        let mut lines = vec![
            "inspector — pull request".to_owned(),
            String::new(),
            format!("pr       #{}", row.pr_number),
            format!("title    {}", row.title),
            format!("author   {}", row.author),
            format!("head     {}", row.head_ref),
            format!("updated  {}", row.updated_at),
            format!("state    {}", row.roger_state),
        ];
        match &row.session_id {
            Some(session_id) => lines.push(format!("session  {session_id}")),
            None => lines.push("session  (none)".to_owned()),
        }
        lines.push(String::new());
        lines.push(action.to_owned());
        lines.push(format!("command  {}", row.next_command));
        lines.push(String::new());
        lines.push(format!("url      {}", row.url));
        lines
    }

    fn render_search_primary(&self) -> (Vec<String>, usize) {
        let mut lines = Vec::new();
        let caret = if self.input_mode == InputMode::SearchQuery {
            "_"
        } else {
            ""
        };
        let lane = match self.search_mode {
            SearchMode::Hits => "hits",
            SearchMode::Reviews => "memory-review",
        };
        lines.push(format!(
            "query: {}{caret}   [lane: {lane}  (m toggles)]",
            self.search_input
        ));
        // Session-level retrieval/memory posture line (distinct from the
        // per-query retrieval_mode reported by the lookup result below). Includes
        // the count of pending memory-review requests awaiting operator action.
        if let Some(posture) = &self.memory_posture {
            lines.push(format!(
                "memory posture: {}  pending_reviews={}",
                posture.summary(),
                self.memory_reviews.len()
            ));
            if let Some(backend) = &posture.embedder_backend {
                lines.push(format!("  embedder backend: {backend}"));
            }
            for reason in &posture.degraded_reasons {
                lines.push(format!("  posture degraded: {reason}"));
            }
        } else {
            lines.push(format!(
                "memory posture: unknown (projection unavailable)  pending_reviews={}",
                self.memory_reviews.len()
            ));
        }

        if self.search_mode == SearchMode::Reviews {
            return self.render_memory_reviews(lines);
        }

        let Some(view) = &self.search else {
            lines.push(String::new());
            lines.push(
                "scoped lookup over prior findings (repo scope of the active session)".to_owned(),
            );
            lines.push("type a query and press Enter".to_owned());
            return (lines, 0);
        };
        lines.push(format!(
            "scope={}  retrieval_mode={}{}",
            view.scope_bucket,
            view.retrieval_mode,
            if view.degraded_reasons.is_empty() {
                String::new()
            } else {
                format!("  degraded({})", view.degraded_reasons.len())
            }
        ));
        for reason in &view.degraded_reasons {
            lines.push(format!("  degraded: {reason}"));
        }
        if view.hits.is_empty() {
            lines.push(String::new());
            lines.push("no prior findings matched".to_owned());
            return (lines, 0);
        }
        let mut cursor_line = 0;
        for (idx, hit) in view.hits.iter().enumerate() {
            if idx == self.search_cursor {
                cursor_line = lines.len();
            }
            lines.push(format!(
                "{} {}  {} PR #{}  sev={}  triage={}  outbound={}  score={}  {}",
                cursor_marker(idx == self.search_cursor),
                hit.finding_id,
                hit.repository,
                hit.pull_request,
                hit.severity,
                hit.triage_state,
                hit.outbound_state,
                hit.fused_score,
                hit.title
            ));
        }
        (lines, cursor_line)
    }

    /// Render the pending memory-review operator surface.
    fn render_memory_reviews(&self, mut lines: Vec<String>) -> (Vec<String>, usize) {
        lines.push(String::new());
        lines.push("pending memory-review requests (a accept  x reject)".to_owned());
        if self.memory_reviews.is_empty() {
            lines.push(String::new());
            lines.push("no pending memory-review requests in this repo scope".to_owned());
            return (lines, 0);
        }
        let mut cursor_line = 0;
        for (idx, row) in self.memory_reviews.iter().enumerate() {
            if idx == self.memory_review_cursor {
                cursor_line = lines.len();
            }
            lines.push(format!(
                "{} {}  {}/{}  {}",
                cursor_marker(idx == self.memory_review_cursor),
                row.id,
                row.source,
                row.request_kind,
                row.statement
            ));
        }
        (lines, cursor_line)
    }

    // --- inspector pane -----------------------------------------------------------

    /// Secondary inspector pane: high-signal detail for the current focus.
    pub fn inspector_lines(&self) -> Vec<String> {
        match self.screen {
            Screen::Picker => self.inspect_picker_candidate(),
            Screen::Home => self.inspect_session(),
            Screen::SessionHome => self.inspect_active_session_entrypoints(),
            Screen::Findings => self.inspect_finding(),
            Screen::Drafts => self.inspect_batch(),
            Screen::DraftItems => self.inspect_draft_item(),
            Screen::Timeline => self.inspect_timeline_entry(),
            Screen::Search => self.inspect_search_hit(),
            Screen::Queue => self.inspect_queue_row(),
        }
    }

    fn inspect_picker_candidate(&self) -> Vec<String> {
        let Some(candidate) = self.picker_candidates.get(self.picker_cursor) else {
            return vec!["no candidate focused".to_owned()];
        };
        let mut lines = vec![
            "inspector — candidate session".to_owned(),
            String::new(),
            format!("session   {}", candidate.session_id),
            format!(
                "target    {} PR #{}",
                candidate.repository, candidate.pull_request
            ),
            format!("provider  {}", candidate.provider),
            format!("attention {}", candidate.attention_state),
        ];
        if let Some(tier) = &candidate.continuity_tier {
            lines.push(format!("continuity {tier}"));
        }
        lines.push(format!("updated   {}", candidate.updated_at));
        lines.push(String::new());
        lines.push("Enter opens this session; Esc returns to the full finder".to_owned());
        lines
    }

    fn inspect_session(&self) -> Vec<String> {
        let Some(session) = self.selected_session() else {
            return vec!["no session focused".to_owned()];
        };
        vec![
            "inspector — session".to_owned(),
            String::new(),
            format!("session   {}", session.session_id),
            format!(
                "target    {} PR #{}",
                session.repository, session.pull_request
            ),
            format!("provider  {}", session.provider),
            format!("attention {}", session.attention_state),
            format!("updated   {}", session.updated_at),
            String::new(),
            format!("resume:   rr resume --session {}", session.session_id),
            format!(
                "fresh:    rr review --repo {} --pr {}",
                session.repository, session.pull_request
            ),
            String::new(),
            "o launches this session's provider review here (suspends the TUI)".to_owned(),
        ]
    }

    fn inspect_active_session_entrypoints(&self) -> Vec<String> {
        let Some(session) = &self.active_session else {
            return vec!["no active session".to_owned()];
        };
        let mut lines = vec![
            "inspector — entrypoints".to_owned(),
            String::new(),
            format!("resume:   rr resume --session {}", session.session_id),
            format!("status:   rr status --session {}", session.session_id),
            String::new(),
            "c opens the clarification composer for this session".to_owned(),
            "o launches this session's provider review here (suspends the TUI)".to_owned(),
        ];
        if let Some(clarification) = &self.last_clarification {
            lines.push(String::new());
            lines.push("pending clarification".to_owned());
            lines.push(format!("  id      {}", clarification.id));
            if let Some(finding_id) = &clarification.finding_id {
                lines.push(format!("  finding {finding_id}"));
            }
            for body_line in clarification.body.lines().take(4) {
                lines.push(format!("  {body_line}"));
            }
        }
        lines
    }

    fn inspect_finding(&self) -> Vec<String> {
        let Some(finding) = self.current_finding() else {
            return vec!["no finding focused".to_owned()];
        };
        let mut lines = vec![
            "inspector — finding".to_owned(),
            String::new(),
            format!("id        {}", finding.finding_id),
            format!("title     {}", finding.title),
            format!(
                "severity  {}  confidence {}",
                finding.severity, finding.confidence
            ),
            format!(
                "states    triage={} outbound={}",
                finding.triage_state, finding.outbound_state
            ),
            format!(
                "lineage   first {}@{}  last {}@{}",
                finding.first_seen_stage,
                finding.first_run_id,
                finding.last_seen_stage.as_deref().unwrap_or("-"),
                finding.last_seen_run_id.as_deref().unwrap_or("-")
            ),
            String::new(),
            "summary".to_owned(),
        ];
        if finding.normalized_summary.is_empty() {
            lines.push("  (no normalized summary stored)".to_owned());
        } else {
            for summary_line in finding.normalized_summary.lines().take(6) {
                lines.push(format!("  {summary_line}"));
            }
        }
        let Some(detail) = self
            .finding_detail
            .as_ref()
            .filter(|detail| detail.finding_id == finding.finding_id)
        else {
            lines.push(String::new());
            lines.push("detail unavailable (load failed or pending)".to_owned());
            return lines;
        };
        lines.push(String::new());
        lines.push("code evidence".to_owned());
        if detail.evidence.is_empty() {
            lines.push("  none stored".to_owned());
        } else {
            for evidence in &detail.evidence {
                lines.push(format!(
                    "  {}  role={}  anchor={}",
                    evidence.locator(),
                    evidence.evidence_role,
                    evidence.anchor_state
                ));
            }
        }
        // Bounded code excerpt read from the session's repo binding at the
        // finding's evidence anchor (honest "unavailable" when unresolved).
        if let Some(excerpt) = self
            .finding_excerpt
            .as_ref()
            .filter(|_| self.finding_detail.is_some())
        {
            lines.push(String::new());
            lines.push(format!("excerpt   {}", excerpt.locator));
            if let Some(reason) = &excerpt.unavailable {
                lines.push(format!("  {reason}"));
            } else {
                for line in &excerpt.lines {
                    lines.push(format!("  {:>5}  {}", line.number, line.text));
                }
            }
        }
        lines.push(String::new());
        lines.push(format!("outbound  state={}", detail.outbound.state));
        if let Some(batch_id) = &detail.outbound.draft_batch_id {
            lines.push(format!("  batch={batch_id}"));
        }
        if let Some(remote) = &detail.outbound.remote_identifier {
            lines.push(format!("  remote={remote}"));
        }
        if let Some(reason) = &detail.outbound.invalidation_reason_code {
            lines.push(format!("  invalidation={reason}"));
        }
        if let Some(failure) = &detail.outbound.failure_code {
            lines.push(format!("  failure={failure}"));
        }
        lines.push(String::new());
        lines.push("decision history".to_owned());
        if detail.decision_events.is_empty() {
            lines.push("  none recorded".to_owned());
        } else {
            for event in detail.decision_events.iter().rev().take(6) {
                lines.push(format!(
                    "  {}  triage={} outbound={}",
                    event.created_at, event.triage_state, event.outbound_state
                ));
            }
        }
        lines.push(String::new());
        lines.push("d drafts this finding  ·  c raises a clarification linked to it".to_owned());
        if let Some(clarification) = self.last_clarification.as_ref().filter(|clarification| {
            clarification.finding_id.as_deref() == Some(&finding.finding_id)
        }) {
            lines.push(format!("pending clarification {}", clarification.id));
        }
        lines
    }

    fn inspect_batch(&self) -> Vec<String> {
        let Some(batch) = self.batches.get(self.batch_cursor) else {
            return vec!["no draft batch focused".to_owned()];
        };
        let mut lines = vec![
            "inspector — draft batch".to_owned(),
            String::new(),
            format!("batch     {}", batch.batch_id),
            format!("state     {}", batch.state),
            format!("items     {}", batch.item_count),
            format!("digest    {}", short_id(&batch.payload_digest)),
        ];
        if let Some(reason) = &batch.invalidation_reason_code {
            lines.push(format!("invalid   {reason}"));
        }
        lines.push(String::new());
        match batch.state.as_str() {
            "awaiting_approval" => {
                lines.push("a opens the elevated approve confirmation".to_owned());
                lines.push(format!("cli:      rr approve --batch {}", batch.batch_id));
            }
            "approved" => {
                lines.push("approved — p opens the elevated post confirmation:".to_owned());
                lines.push("  posting sends this batch to GitHub (visibly elevated)".to_owned());
                lines.push(format!("cli:      rr post --batch {}", batch.batch_id));
            }
            _ => lines.push("not approvable in its current state".to_owned()),
        }
        lines
    }

    fn inspect_draft_item(&self) -> Vec<String> {
        let Some(item) = self.batch_items.get(self.item_cursor) else {
            return vec!["no draft item focused".to_owned()];
        };
        let mut lines = vec![
            "inspector — draft payload".to_owned(),
            String::new(),
            format!("draft     {}", item.draft_id),
            format!("finding   {}", item.finding_id.as_deref().unwrap_or("-")),
            format!("target    {}", item.target_locator),
            String::new(),
            "payload preview".to_owned(),
        ];
        for body_line in item.body.lines().take(14) {
            lines.push(format!("  {body_line}"));
        }
        lines
    }

    fn inspect_timeline_entry(&self) -> Vec<String> {
        let entries = self.timeline_entries();
        let (Some(entry), Some(timeline)) =
            (entries.get(self.timeline_cursor), self.timeline.as_ref())
        else {
            return vec!["no timeline entry focused".to_owned()];
        };
        match entry {
            TimelineEntry::Run(run_idx) => {
                let run = &timeline.runs[*run_idx];
                vec![
                    "inspector — review run".to_owned(),
                    String::new(),
                    format!("run        {}", run.run_id),
                    format!("kind       {}", run.run_kind),
                    format!("continuity {}", run.continuity_quality),
                    format!("created    {}", run.created_at),
                    format!("stages     {}", run.stages.len()),
                ]
            }
            TimelineEntry::Stage(run_idx, stage_idx) => {
                let run = &timeline.runs[*run_idx];
                let stage = &run.stages[*stage_idx];
                let mut lines = vec![
                    "inspector — worker stage result".to_owned(),
                    String::new(),
                    format!("run      {}", run.run_id),
                    format!("stage    {}", stage.stage),
                    format!("task     {}", stage.task_kind),
                    format!("outcome  {}", stage.outcome),
                    String::new(),
                    "summary".to_owned(),
                ];
                for summary_line in stage.summary.lines().take(10) {
                    lines.push(format!("  {summary_line}"));
                }
                lines
            }
            TimelineEntry::Posted(posted_idx) => {
                let posted = &timeline.posted_actions[*posted_idx];
                vec![
                    "inspector — posted action".to_owned(),
                    String::new(),
                    format!("action   {}", posted.action_id),
                    format!("batch    {}", posted.batch_id),
                    format!("remote   {}", posted.remote_identifier),
                    format!("status   {}", posted.status),
                    format!("failure  {}", posted.failure_code.as_deref().unwrap_or("-")),
                    format!("posted   {}", posted.posted_at),
                ]
            }
        }
    }

    fn inspect_search_hit(&self) -> Vec<String> {
        if self.search_mode == SearchMode::Reviews {
            let Some(row) = self.memory_reviews.get(self.memory_review_cursor) else {
                return vec!["no pending memory-review request focused".to_owned()];
            };
            let mut lines = vec![
                "inspector — memory review request".to_owned(),
                String::new(),
                format!("id       {}", row.id),
                format!("source   {}", row.source),
                format!("kind     {}", row.request_kind),
                format!("status   {}", row.status),
                format!("norm_key {}", row.normalized_key),
                String::new(),
                "statement".to_owned(),
            ];
            if row.statement.is_empty() {
                lines.push("  (no statement)".to_owned());
            } else {
                for statement_line in row.statement.lines().take(8) {
                    lines.push(format!("  {statement_line}"));
                }
            }
            return lines;
        }
        let Some(view) = &self.search else {
            return vec!["no search executed yet".to_owned()];
        };
        let Some(hit) = view.hits.get(self.search_cursor) else {
            return vec!["no search hit focused".to_owned()];
        };
        let mut lines = vec![
            "inspector — prior finding".to_owned(),
            String::new(),
            format!("finding  {}", hit.finding_id),
            format!("session  {}", hit.session_id),
            format!("target   {} PR #{}", hit.repository, hit.pull_request),
            format!("severity {}", hit.severity),
            format!(
                "states   triage={} outbound={}",
                hit.triage_state, hit.outbound_state
            ),
            format!("score    {}", hit.fused_score),
            String::new(),
            "summary".to_owned(),
        ];
        if hit.normalized_summary.is_empty() {
            lines.push("  (no normalized summary stored)".to_owned());
        } else {
            for summary_line in hit.normalized_summary.lines().take(8) {
                lines.push(format!("  {summary_line}"));
            }
        }
        lines
    }

    // --- overlays -----------------------------------------------------------------

    fn elevation_lines(&self) -> Vec<String> {
        let Overlay::Elevation {
            batch_id,
            kind,
            command,
            input,
        } = &self.overlay
        else {
            return Vec::new();
        };
        let item_count = self
            .batches
            .iter()
            .find(|row| &row.batch_id == batch_id)
            .map(|row| row.item_count)
            .unwrap_or(0);
        let word = kind.confirm_word();
        let (header, effect) = match kind {
            ElevationKind::Approve => (
                "╔══ ELEVATED ACTION — local batch approval ══".to_owned(),
                vec![
                    "this records a local approval token bound to the exact stored".to_owned(),
                    "batch payload digest and target tuple — nothing is posted to GitHub"
                        .to_owned(),
                ],
            ),
            ElevationKind::Post => (
                "╔══ ELEVATED ACTION — POST TO GITHUB ══".to_owned(),
                vec![
                    "this POSTS the approved batch to GitHub through the real posting".to_owned(),
                    "adapter — this action publishes review comments to the remote PR".to_owned(),
                ],
            ),
        };
        let mut lines = vec![
            header,
            String::new(),
            format!("batch    {batch_id}  ({item_count} draft item(s))"),
            format!("command  {command}"),
            String::new(),
        ];
        lines.extend(effect);
        lines.push(String::new());
        lines.push(format!(
            "type {word} and press Enter to execute in-process; Esc cancels"
        ));
        lines.push(String::new());
        lines.push(format!("> {input}_"));
        lines
    }

    /// The bounded clarification composer body (rendered while
    /// `input_mode == Composer`).
    fn composer_lines(&self) -> Vec<String> {
        let target = match &self.composer_finding_id {
            Some(finding_id) => format!("linked to finding {finding_id}"),
            None => "linked to the active session".to_owned(),
        };
        let mut lines = vec![
            "── clarification / follow-up composer".to_owned(),
            String::new(),
            format!("scope     {target}"),
            "this materializes a durable clarification request (Roger-owned).".to_owned(),
            String::new(),
            format!("> {}_", self.composer_input),
            String::new(),
            "Enter submits · Esc cancels".to_owned(),
        ];
        if let Some(clarification) = &self.last_clarification {
            lines.push(String::new());
            lines.push(format!("last clarification: {}", clarification.id));
        }
        lines
    }

    fn help_lines(&self) -> Vec<String> {
        vec![
            "help — keyboard reference (Esc closes)".to_owned(),
            String::new(),
            "global      q quit   ? help   Esc back/clear".to_owned(),
            "picker      j/k move   Enter open candidate   Esc full session finder".to_owned(),
            "home        j/k move   Enter open session   o launch/resume   / filter   r reload"
                .to_owned(),
            "            p open PR queue   ! run rr doctor preflight (dropout)".to_owned(),
            "queue       j/k move   Enter start (fresh) or resume (existing session)   r reload"
                .to_owned(),
            "overview    f findings   d drafts   t timeline   s search   c clarify   o launch"
                .to_owned(),
            "findings    j/k move   space multi-select   a/i/n/r triage (batch on selection)"
                .to_owned(),
            "            d create draft from selection/accepted   c clarify (linked to finding)"
                .to_owned(),
            "            s cycle sort   g cycle group   / filter   Esc clear sel/filter/back"
                .to_owned(),
            "drafts      Enter inspect items   a elevated approve   p elevated post to GitHub"
                .to_owned(),
            "draft items j/k move   e edit draft body in $EDITOR   Esc drafts".to_owned(),
            "timeline    j/k move across runs, stage results, posted actions".to_owned(),
            "search      / edit query   Enter run   j/k move hits".to_owned(),
            "            posture line shows retrieval_mode + semantic asset state".to_owned(),
            String::new(),
            "mutation    triage writes the same storage state as rr triage".to_owned(),
            "            approval requires typing approve in the elevated prompt".to_owned(),
            "            posting to GitHub requires typing post in a distinct elevated prompt"
                .to_owned(),
            String::new(),
            "clarify     c opens the bounded composer; submit creates a durable request".to_owned(),
            "launch      o suspends the TUI and runs rr review --resume for the session".to_owned(),
            "queue       Enter is reuse-or-fresh: the row's roger_state decides which".to_owned(),
        ]
    }
}

// --- layout helpers ------------------------------------------------------------

fn cursor_marker(selected: bool) -> &'static str {
    if selected { ">" } else { " " }
}

fn short_fingerprint(fingerprint: &str) -> &str {
    let end = fingerprint
        .char_indices()
        .nth(12)
        .map(|(idx, _)| idx)
        .unwrap_or(fingerprint.len());
    &fingerprint[..end]
}

fn short_id(id: &str) -> &str {
    let end = id
        .char_indices()
        .nth(16)
        .map(|(idx, _)| idx)
        .unwrap_or(id.len());
    &id[..end]
}

/// Clamp a table cell to `width` chars (char-wise, not byte-wise).
fn truncate_cell(value: &str, width: usize) -> String {
    if value.chars().count() <= width {
        return value.to_owned();
    }
    value
        .chars()
        .take(width.saturating_sub(1))
        .collect::<String>()
        + "…"
}

fn truncate_line(line: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    line.chars().take(width).collect()
}

/// Window long primary panes so the cursor row stays visible, with an
/// explicit position marker (contract: visible paging/position state).
fn window_lines(lines: Vec<String>, cursor_line: usize, budget: usize) -> Vec<String> {
    if lines.len() <= budget {
        return lines;
    }
    let visible_budget = budget.saturating_sub(1).max(1);
    let start = cursor_line
        .saturating_sub(visible_budget / 2)
        .min(lines.len().saturating_sub(visible_budget));
    let end = (start + visible_budget).min(lines.len());
    let mut windowed: Vec<String> = lines[start..end].to_vec();
    windowed.push(format!("… rows {}-{} of {}", start + 1, end, lines.len()));
    windowed
}

/// Compose the primary pane and inspector pane side-by-side.
fn compose_columns(
    primary: Vec<String>,
    inspector: Vec<String>,
    primary_width: usize,
    inspector_width: usize,
    budget: usize,
) -> Vec<String> {
    let rows = primary.len().max(inspector.len()).min(budget);
    let mut lines = Vec::with_capacity(rows);
    for idx in 0..rows {
        let left = primary.get(idx).map(String::as_str).unwrap_or("");
        let right = inspector.get(idx).map(String::as_str).unwrap_or("");
        let mut left_cell: String = left.chars().take(primary_width).collect();
        let pad = primary_width.saturating_sub(left_cell.chars().count());
        left_cell.extend(std::iter::repeat_n(' ', pad));
        let right_cell: String = right.chars().take(inspector_width).collect();
        lines.push(format!("{left_cell} │ {right_cell}"));
    }
    lines
}
