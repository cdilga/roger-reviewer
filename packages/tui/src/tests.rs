use crate::backend::CockpitBackend;
use crate::model::{
    ApproveOutcome, CLARIFY_HINT, CockpitEntry, CockpitKey, CockpitModel, DecisionEventRow,
    DraftBatchRow, DraftItemRow, ELEVATED_MUTATION_HINT, EMPTY_STORE_HINT, EvidenceRow,
    FindingDetail, FindingRow, GroupKey, InputMode, KeyOutcome, MemoryPostureRow,
    MemoryReviewResolutionRow, MemoryReviewRow, ModelEffect, OutboundLinkRow, Overlay,
    PickerCandidate, PostedActionRow, Screen, SearchHitRow, SearchMode, SearchView,
    SessionHomeView, SessionRow, SortKey, StageResultRow, TimelineEntry, TimelineRunRow,
    TimelineView, TriageAction,
};

#[derive(Default)]
struct FakeBackend {
    sessions: Vec<SessionRow>,
    findings: Vec<FindingRow>,
    batches: Vec<DraftBatchRow>,
    items: Vec<DraftItemRow>,
    timeline: TimelineView,
    search_view: SearchView,
    triage_calls: Vec<(String, &'static str)>,
    approve_calls: Vec<(String, String)>,
    approve_outcome: Option<ApproveOutcome>,
    search_calls: Vec<(String, String)>,
    detail_calls: Vec<String>,
    fail_home: bool,
    fail_detail: bool,
    /// Session-home latest-run projection knobs (drive the explained
    /// empty-findings states).
    home_latest_run_id: Option<String>,
    home_stage_count: usize,
    /// Memory posture returned by `memory_posture`.
    memory_posture: MemoryPostureRow,
    fail_memory_posture: bool,
    /// Recorded `revise_draft_body(draft_id, new_body)` calls.
    revise_calls: Vec<(String, String)>,
    fail_revise: bool,
    /// Pending memory-review requests returned by `list_pending_memory_reviews`.
    memory_reviews: Vec<MemoryReviewRow>,
    fail_memory_reviews: bool,
    /// Recorded `resolve_memory_review(request_id, accept)` calls.
    resolve_review_calls: Vec<(String, bool)>,
    fail_resolve_review: bool,
}

impl CockpitBackend for FakeBackend {
    fn list_sessions(&mut self) -> Result<Vec<SessionRow>, String> {
        Ok(self.sessions.clone())
    }

    fn load_session_home(&mut self, _session_id: &str) -> Result<SessionHomeView, String> {
        if self.fail_home {
            return Err("home load failed".to_owned());
        }
        Ok(SessionHomeView {
            findings_total: self.findings.len(),
            triage_counts: vec![("new".to_owned(), self.findings.len())],
            outbound_counts: vec![("not_drafted".to_owned(), self.findings.len())],
            latest_run_id: self.home_latest_run_id.clone(),
            latest_run_kind: self
                .home_latest_run_id
                .as_ref()
                .map(|_| "initial".to_owned()),
            latest_run_stage_count: self.home_stage_count,
            run_count: 2,
            draft_count: self.batches.len(),
            posted_action_count: self.timeline.posted_actions.len(),
        })
    }

    fn load_findings(&mut self, _session_id: &str) -> Result<Vec<FindingRow>, String> {
        Ok(self.findings.clone())
    }

    fn load_finding_detail(&mut self, finding_id: &str) -> Result<FindingDetail, String> {
        self.detail_calls.push(finding_id.to_owned());
        if self.fail_detail {
            return Err("detail load failed".to_owned());
        }
        Ok(FindingDetail {
            finding_id: finding_id.to_owned(),
            evidence: vec![EvidenceRow {
                repo_rel_path: "src/lib.rs".to_owned(),
                start_line: 10,
                end_line: Some(14),
                evidence_role: "primary".to_owned(),
                anchor_state: "valid".to_owned(),
            }],
            outbound: OutboundLinkRow {
                state: "not_drafted".to_owned(),
                ..OutboundLinkRow::default()
            },
            decision_events: vec![DecisionEventRow {
                triage_state: "new".to_owned(),
                outbound_state: "not_drafted".to_owned(),
                created_at: 50,
            }],
        })
    }

    fn set_triage_state(&mut self, finding_id: &str, action: TriageAction) -> Result<(), String> {
        self.triage_calls
            .push((finding_id.to_owned(), action.as_state_str()));
        if let Some(row) = self
            .findings
            .iter_mut()
            .find(|row| row.finding_id == finding_id)
        {
            row.triage_state = action.as_state_str().to_owned();
        }
        Ok(())
    }

    fn load_draft_batches(&mut self, _session_id: &str) -> Result<Vec<DraftBatchRow>, String> {
        Ok(self.batches.clone())
    }

    fn load_batch_items(&mut self, _batch_id: &str) -> Result<Vec<DraftItemRow>, String> {
        Ok(self.items.clone())
    }

    fn approve_batch(
        &mut self,
        session_id: &str,
        batch_id: &str,
    ) -> Result<ApproveOutcome, String> {
        self.approve_calls
            .push((session_id.to_owned(), batch_id.to_owned()));
        if let Some(outcome) = &self.approve_outcome {
            if let ApproveOutcome::Approved { .. } = outcome
                && let Some(batch) = self
                    .batches
                    .iter_mut()
                    .find(|batch| batch.batch_id == batch_id)
            {
                batch.state = "approved".to_owned();
            }
            return Ok(outcome.clone());
        }
        Err("approve backend unavailable".to_owned())
    }

    fn load_timeline(&mut self, _session_id: &str) -> Result<TimelineView, String> {
        Ok(self.timeline.clone())
    }

    fn search_prior_reviews(
        &mut self,
        repository: &str,
        query: &str,
    ) -> Result<SearchView, String> {
        self.search_calls
            .push((repository.to_owned(), query.to_owned()));
        Ok(self.search_view.clone())
    }

    fn memory_posture(&mut self) -> Result<MemoryPostureRow, String> {
        if self.fail_memory_posture {
            return Err("posture load failed".to_owned());
        }
        Ok(self.memory_posture.clone())
    }

    fn revise_draft_body(&mut self, draft_id: &str, new_body: &str) -> Result<(), String> {
        self.revise_calls
            .push((draft_id.to_owned(), new_body.to_owned()));
        if self.fail_revise {
            return Err("revision rejected by storage".to_owned());
        }
        if let Some(item) = self.items.iter_mut().find(|item| item.draft_id == draft_id) {
            item.body = new_body.to_owned();
        }
        Ok(())
    }

    fn list_pending_memory_reviews(
        &mut self,
        _repository: &str,
    ) -> Result<Vec<MemoryReviewRow>, String> {
        if self.fail_memory_reviews {
            return Err("memory review load failed".to_owned());
        }
        Ok(self.memory_reviews.clone())
    }

    fn resolve_memory_review(
        &mut self,
        request_id: &str,
        accept: bool,
    ) -> Result<MemoryReviewResolutionRow, String> {
        self.resolve_review_calls
            .push((request_id.to_owned(), accept));
        if self.fail_resolve_review {
            return Err("resolution rejected by storage".to_owned());
        }
        // Drop the resolved request from the pending list, mirroring storage.
        self.memory_reviews.retain(|row| row.id != request_id);
        Ok(MemoryReviewResolutionRow {
            id: request_id.to_owned(),
            status: if accept {
                "accepted".to_owned()
            } else {
                "rejected".to_owned()
            },
            resulting_memory_item_id: if accept {
                Some(format!("mem-{request_id}"))
            } else {
                None
            },
            materialized_new_item: accept,
        })
    }
}

fn session_row(id: &str, pr: u64) -> SessionRow {
    SessionRow {
        session_id: id.to_owned(),
        repository: "owner/repo".to_owned(),
        pull_request: pr,
        provider: "opencode".to_owned(),
        attention_state: "awaiting_user_input".to_owned(),
        updated_at: 100,
    }
}

fn finding_row(id: &str, title: &str) -> FindingRow {
    FindingRow {
        finding_id: id.to_owned(),
        fingerprint: format!("fp-{id}-0123456789abcdef"),
        title: title.to_owned(),
        normalized_summary: format!("summary of {title}"),
        severity: "high".to_owned(),
        confidence: "medium".to_owned(),
        triage_state: "new".to_owned(),
        outbound_state: "not_drafted".to_owned(),
        first_seen_stage: "initial".to_owned(),
        first_run_id: "run-1".to_owned(),
        last_seen_stage: Some("refresh".to_owned()),
        last_seen_run_id: Some("run-2".to_owned()),
        primary_file: Some(format!("src/{id}.rs")),
    }
}

fn populated_backend() -> FakeBackend {
    FakeBackend {
        sessions: vec![session_row("session-1", 7), session_row("session-2", 8)],
        findings: vec![
            finding_row("finding-1", "Null result ignored"),
            finding_row("finding-2", "Race in refresh path"),
            finding_row("finding-3", "Missing fail-closed gate"),
        ],
        batches: vec![DraftBatchRow {
            batch_id: "batch-1".to_owned(),
            state: "awaiting_approval".to_owned(),
            item_count: 2,
            payload_digest: "digest-1".to_owned(),
            invalidation_reason_code: None,
        }],
        items: vec![DraftItemRow {
            draft_id: "draft-1".to_owned(),
            finding_id: Some("finding-1".to_owned()),
            target_locator: "github:owner/repo#7".to_owned(),
            body: "Finding: Null result ignored\nmore detail".to_owned(),
        }],
        timeline: TimelineView {
            runs: vec![TimelineRunRow {
                run_id: "run-2".to_owned(),
                run_kind: "refresh".to_owned(),
                continuity_quality: "tier_b".to_owned(),
                created_at: 200,
                stages: vec![StageResultRow {
                    stage: "initial".to_owned(),
                    task_kind: "InitialReview".to_owned(),
                    outcome: "Succeeded".to_owned(),
                    summary: "3 findings".to_owned(),
                }],
            }],
            posted_actions: vec![PostedActionRow {
                action_id: "posted-1".to_owned(),
                batch_id: "batch-0".to_owned(),
                remote_identifier: "github-comment-99".to_owned(),
                status: "Succeeded".to_owned(),
                failure_code: None,
                posted_at: 300,
            }],
        },
        search_view: SearchView {
            scope_bucket: "repo".to_owned(),
            retrieval_mode: "lexical_only".to_owned(),
            degraded_reasons: vec!["semantic assets are missing".to_owned()],
            hits: vec![SearchHitRow {
                finding_id: "finding-old".to_owned(),
                session_id: "session-0".to_owned(),
                repository: "owner/repo".to_owned(),
                pull_request: 3,
                title: "Stale lock handling".to_owned(),
                normalized_summary: "prior pass flagged lock handling".to_owned(),
                severity: "medium".to_owned(),
                triage_state: "accepted".to_owned(),
                outbound_state: "posted".to_owned(),
                fused_score: 41,
            }],
        },
        approve_outcome: Some(ApproveOutcome::Approved {
            draft_count: 2,
            already_recorded: false,
        }),
        // A run exists with worker stage results (drives the "zero findings"
        // explained empty-state when findings are cleared).
        home_latest_run_id: Some("run-2".to_owned()),
        home_stage_count: 1,
        memory_posture: MemoryPostureRow {
            retrieval_mode: "lexical_only".to_owned(),
            semantic_operational: false,
            assets_verified: false,
            embedder_available: true,
            embedder_backend: Some("fastembed".to_owned()),
            degraded_reasons: vec!["semantic model not installed".to_owned()],
        },
        ..FakeBackend::default()
    }
}

fn bootstrap(backend: &mut FakeBackend) -> CockpitModel {
    CockpitModel::bootstrap(backend, None).expect("bootstrap cockpit model")
}

fn keys(model: &mut CockpitModel, backend: &mut FakeBackend, sequence: &[CockpitKey]) {
    for key in sequence {
        model.handle_key(*key, backend);
    }
}

fn type_chars(model: &mut CockpitModel, backend: &mut FakeBackend, text: &str) {
    for c in text.chars() {
        model.handle_key(CockpitKey::Char(c), backend);
    }
}

fn open_findings(model: &mut CockpitModel, backend: &mut FakeBackend) {
    keys(model, backend, &[CockpitKey::Enter, CockpitKey::Char('f')]);
    assert_eq!(model.screen, Screen::Findings);
}

// --- entry / bootstrap ------------------------------------------------------

#[test]
fn bootstrap_without_session_opens_review_home() {
    let mut backend = populated_backend();
    let model = bootstrap(&mut backend);
    assert_eq!(model.screen, Screen::Home);
    assert_eq!(model.sessions.len(), 2);
    assert_eq!(model.session_cursor, 0);
}

#[test]
fn bootstrap_with_resolved_session_opens_session_home() {
    let mut backend = populated_backend();
    let model = CockpitModel::bootstrap(&mut backend, Some("session-2")).expect("bootstrap");
    assert_eq!(model.screen, Screen::SessionHome);
    assert_eq!(
        model.active_session.as_ref().map(|s| s.session_id.as_str()),
        Some("session-2")
    );
    assert!(model.home.is_some());
}

#[test]
fn bootstrap_with_unknown_session_falls_back_to_home_with_notice() {
    let mut backend = populated_backend();
    let model = CockpitModel::bootstrap(&mut backend, Some("session-missing")).expect("bootstrap");
    assert_eq!(model.screen, Screen::Home);
    assert!(
        model
            .notice
            .as_deref()
            .is_some_and(|notice| notice.contains("session-missing")),
        "{:?}",
        model.notice
    );
}

// --- review home: selection, attention grouping, session finder ---------------

#[test]
fn session_selection_moves_and_clamps() {
    let mut backend = populated_backend();
    let mut model = bootstrap(&mut backend);
    model.handle_key(CockpitKey::Down, &mut backend);
    assert_eq!(model.session_cursor, 1);
    model.handle_key(CockpitKey::Char('j'), &mut backend);
    assert_eq!(model.session_cursor, 1, "cursor clamps at end");
    model.handle_key(CockpitKey::Char('k'), &mut backend);
    assert_eq!(model.session_cursor, 0);
    model.handle_key(CockpitKey::Up, &mut backend);
    assert_eq!(model.session_cursor, 0, "cursor clamps at start");
}

#[test]
fn empty_list_selection_is_safe() {
    let mut backend = FakeBackend::default();
    let mut model = bootstrap(&mut backend);
    model.handle_key(CockpitKey::Down, &mut backend);
    model.handle_key(CockpitKey::Enter, &mut backend);
    assert_eq!(model.screen, Screen::Home);
    assert_eq!(model.session_cursor, 0);
}

#[test]
fn sessions_order_by_attention_priority_then_recency() {
    let mut backend = populated_backend();
    backend.sessions = vec![
        SessionRow {
            attention_state: "review_launched".to_owned(),
            updated_at: 500,
            ..session_row("session-running", 1)
        },
        SessionRow {
            attention_state: "outbound_approval_required".to_owned(),
            updated_at: 10,
            ..session_row("session-approval", 2)
        },
        SessionRow {
            attention_state: "awaiting_user_input".to_owned(),
            updated_at: 100,
            ..session_row("session-input", 3)
        },
    ];
    let model = bootstrap(&mut backend);
    let visible = model.visible_sessions();
    let ordered: Vec<&str> = visible
        .iter()
        .map(|&idx| model.sessions[idx].session_id.as_str())
        .collect();
    assert_eq!(
        ordered,
        vec!["session-approval", "session-input", "session-running"],
        "attention queue puts approval-required and input-needed sessions first"
    );
}

#[test]
fn home_renders_attention_group_headers_and_resume_command() {
    let mut backend = populated_backend();
    let model = bootstrap(&mut backend);
    let rendered = model.render_lines(160, 30).join("\n");
    assert!(
        rendered.contains("── attention: awaiting_user_input"),
        "{rendered}"
    );
    assert!(
        rendered.contains("rr resume --session session-1"),
        "inspector shows the launch/resume entrypoint command: {rendered}"
    );
}

#[test]
fn session_finder_filters_across_repo_and_pr() {
    let mut backend = populated_backend();
    backend.sessions.push(SessionRow {
        repository: "other/repo".to_owned(),
        ..session_row("session-other", 42)
    });
    let mut model = bootstrap(&mut backend);
    model.handle_key(CockpitKey::Char('/'), &mut backend);
    assert_eq!(model.input_mode, InputMode::SessionFilter);
    type_chars(&mut model, &mut backend, "other");
    assert_eq!(model.visible_sessions().len(), 1);
    assert_eq!(
        model.selected_session().map(|s| s.session_id.as_str()),
        Some("session-other")
    );
    // Enter keeps the filter and leaves input mode.
    model.handle_key(CockpitKey::Enter, &mut backend);
    assert_eq!(model.input_mode, InputMode::None);
    assert_eq!(model.session_filter, "other");
    // Esc on the list clears the filter.
    model.handle_key(CockpitKey::Esc, &mut backend);
    assert!(model.session_filter.is_empty());
    assert_eq!(model.visible_sessions().len(), 3);
}

#[test]
fn typing_q_in_filter_input_does_not_quit() {
    let mut backend = populated_backend();
    let mut model = bootstrap(&mut backend);
    model.handle_key(CockpitKey::Char('/'), &mut backend);
    assert_eq!(
        model.handle_key(CockpitKey::Char('q'), &mut backend),
        KeyOutcome::Continue
    );
    assert_eq!(model.session_filter, "q");
    model.handle_key(CockpitKey::Backspace, &mut backend);
    assert!(model.session_filter.is_empty());
}

// --- navigation grammar -----------------------------------------------------

#[test]
fn enter_opens_overview_and_keys_route_to_all_destinations() {
    let mut backend = populated_backend();
    let mut model = bootstrap(&mut backend);

    model.handle_key(CockpitKey::Enter, &mut backend);
    assert_eq!(model.screen, Screen::SessionHome);

    model.handle_key(CockpitKey::Char('f'), &mut backend);
    assert_eq!(model.screen, Screen::Findings);
    assert_eq!(model.findings.len(), 3);
    model.handle_key(CockpitKey::Esc, &mut backend);
    assert_eq!(model.screen, Screen::SessionHome);

    model.handle_key(CockpitKey::Char('d'), &mut backend);
    assert_eq!(model.screen, Screen::Drafts);
    model.handle_key(CockpitKey::Enter, &mut backend);
    assert_eq!(model.screen, Screen::DraftItems);
    assert_eq!(model.open_batch_id.as_deref(), Some("batch-1"));
    model.handle_key(CockpitKey::Esc, &mut backend);
    assert_eq!(model.screen, Screen::Drafts);
    model.handle_key(CockpitKey::Esc, &mut backend);
    assert_eq!(model.screen, Screen::SessionHome);

    model.handle_key(CockpitKey::Char('t'), &mut backend);
    assert_eq!(model.screen, Screen::Timeline);
    model.handle_key(CockpitKey::Esc, &mut backend);
    assert_eq!(model.screen, Screen::SessionHome);

    model.handle_key(CockpitKey::Char('s'), &mut backend);
    assert_eq!(model.screen, Screen::Search);
    assert_eq!(model.input_mode, InputMode::SearchQuery);
    model.handle_key(CockpitKey::Esc, &mut backend);
    assert_eq!(model.screen, Screen::SessionHome);

    model.handle_key(CockpitKey::Esc, &mut backend);
    assert_eq!(model.screen, Screen::Home);
}

#[test]
fn quit_key_quits_from_browse_screens() {
    let mut backend = populated_backend();
    let mut model = bootstrap(&mut backend);
    assert_eq!(
        model.handle_key(CockpitKey::Char('q'), &mut backend),
        KeyOutcome::Quit
    );
    model.handle_key(CockpitKey::Enter, &mut backend);
    assert_eq!(
        model.handle_key(CockpitKey::Char('q'), &mut backend),
        KeyOutcome::Quit
    );
}

#[test]
fn uppercase_navigation_keys_are_normalized() {
    let mut backend = populated_backend();
    let mut model = bootstrap(&mut backend);
    model.handle_key(CockpitKey::Char('J'), &mut backend);
    assert_eq!(model.session_cursor, 1);
}

// --- triage keymap ----------------------------------------------------------

#[test]
fn triage_keymap_maps_to_state_transitions() {
    for (key, expected_state) in [
        ('a', "accepted"),
        ('i', "ignored"),
        ('n', "needs_follow_up"),
        ('r', "resolved"),
    ] {
        let mut backend = populated_backend();
        let mut model = bootstrap(&mut backend);
        open_findings(&mut model, &mut backend);
        model.handle_key(CockpitKey::Down, &mut backend);
        model.handle_key(CockpitKey::Char(key), &mut backend);

        assert_eq!(
            backend.triage_calls,
            vec![("finding-2".to_owned(), expected_state)],
            "key {key} must call the rr-triage storage mutation"
        );
        // Truthful re-render: the row shows the reloaded stored state.
        assert_eq!(model.findings[1].triage_state, expected_state);
        assert!(
            model
                .notice
                .as_deref()
                .is_some_and(|notice| notice.contains(expected_state)),
            "{:?}",
            model.notice
        );
    }
}

#[test]
fn triage_selection_stays_bound_to_finding_id_after_reload() {
    let mut backend = populated_backend();
    let mut model = bootstrap(&mut backend);
    open_findings(&mut model, &mut backend);
    keys(
        &mut model,
        &mut backend,
        &[CockpitKey::Down, CockpitKey::Down],
    );
    assert_eq!(model.finding_cursor, 2);
    model.handle_key(CockpitKey::Char('a'), &mut backend);
    assert_eq!(
        model.current_finding().map(|f| f.finding_id.as_str()),
        Some("finding-3"),
        "selection is id-stable across the post-triage reload"
    );
}

#[test]
fn triage_keys_are_inert_on_empty_findings() {
    let mut backend = populated_backend();
    backend.findings.clear();
    let mut model = bootstrap(&mut backend);
    open_findings(&mut model, &mut backend);
    model.handle_key(CockpitKey::Char('a'), &mut backend);
    assert!(backend.triage_calls.is_empty());
}

// --- multi-select + batch triage ---------------------------------------------

#[test]
fn space_toggles_multi_select_bound_to_finding_ids() {
    let mut backend = populated_backend();
    let mut model = bootstrap(&mut backend);
    open_findings(&mut model, &mut backend);
    model.handle_key(CockpitKey::Char(' '), &mut backend);
    model.handle_key(CockpitKey::Down, &mut backend);
    model.handle_key(CockpitKey::Char(' '), &mut backend);
    assert!(model.selected_findings.contains("finding-1"));
    assert!(model.selected_findings.contains("finding-2"));
    // Toggle off.
    model.handle_key(CockpitKey::Char(' '), &mut backend);
    assert!(!model.selected_findings.contains("finding-2"));
    assert_eq!(model.selected_findings.len(), 1);
}

#[test]
fn batch_triage_applies_to_the_whole_selection() {
    let mut backend = populated_backend();
    let mut model = bootstrap(&mut backend);
    open_findings(&mut model, &mut backend);
    model.handle_key(CockpitKey::Char(' '), &mut backend);
    model.handle_key(CockpitKey::Down, &mut backend);
    model.handle_key(CockpitKey::Down, &mut backend);
    model.handle_key(CockpitKey::Char(' '), &mut backend);
    model.handle_key(CockpitKey::Char('i'), &mut backend);

    let mut triaged: Vec<&str> = backend
        .triage_calls
        .iter()
        .map(|(id, _)| id.as_str())
        .collect();
    triaged.sort();
    assert_eq!(triaged, vec!["finding-1", "finding-3"]);
    assert!(
        backend
            .triage_calls
            .iter()
            .all(|(_, state)| *state == "ignored")
    );
    assert!(
        model
            .notice
            .as_deref()
            .is_some_and(|notice| notice.contains("2 findings")),
        "{:?}",
        model.notice
    );
    // Selection survives the reload because the ids still exist.
    assert_eq!(model.selected_findings.len(), 2);
}

#[test]
fn esc_clears_selection_then_filter_then_navigates_back() {
    let mut backend = populated_backend();
    let mut model = bootstrap(&mut backend);
    open_findings(&mut model, &mut backend);
    model.handle_key(CockpitKey::Char('/'), &mut backend);
    type_chars(&mut model, &mut backend, "race");
    model.handle_key(CockpitKey::Enter, &mut backend);
    model.handle_key(CockpitKey::Char(' '), &mut backend);
    assert_eq!(model.selected_findings.len(), 1);

    model.handle_key(CockpitKey::Esc, &mut backend);
    assert!(
        model.selected_findings.is_empty(),
        "first Esc clears selection"
    );
    assert_eq!(model.screen, Screen::Findings);

    model.handle_key(CockpitKey::Esc, &mut backend);
    assert!(model.finding_filter.is_empty(), "second Esc clears filter");
    assert_eq!(model.screen, Screen::Findings);

    model.handle_key(CockpitKey::Esc, &mut backend);
    assert_eq!(
        model.screen,
        Screen::SessionHome,
        "third Esc navigates back"
    );
}

// --- filtering / sorting / grouping -------------------------------------------

#[test]
fn finding_filter_narrows_queue_by_title_and_file() {
    let mut backend = populated_backend();
    let mut model = bootstrap(&mut backend);
    open_findings(&mut model, &mut backend);
    model.handle_key(CockpitKey::Char('/'), &mut backend);
    type_chars(&mut model, &mut backend, "refresh");
    assert_eq!(model.visible_findings().len(), 1);
    assert_eq!(
        model.current_finding().map(|f| f.finding_id.as_str()),
        Some("finding-2")
    );
    // Filter by file path too.
    model.handle_key(CockpitKey::Esc, &mut backend);
    model.handle_key(CockpitKey::Char('/'), &mut backend);
    type_chars(&mut model, &mut backend, "src/finding-3");
    assert_eq!(model.visible_findings().len(), 1);
    assert_eq!(
        model.current_finding().map(|f| f.finding_id.as_str()),
        Some("finding-3")
    );
}

#[test]
fn sort_cycle_orders_by_severity() {
    let mut backend = populated_backend();
    backend.findings[0].severity = "low".to_owned();
    backend.findings[1].severity = "critical".to_owned();
    backend.findings[2].severity = "medium".to_owned();
    let mut model = bootstrap(&mut backend);
    open_findings(&mut model, &mut backend);
    assert_eq!(model.finding_sort, SortKey::Stored);
    model.handle_key(CockpitKey::Char('s'), &mut backend);
    assert_eq!(model.finding_sort, SortKey::Severity);
    let visible = model.visible_findings();
    let ordered: Vec<&str> = visible
        .iter()
        .map(|&idx| model.findings[idx].severity.as_str())
        .collect();
    assert_eq!(ordered, vec!["critical", "medium", "low"]);
}

#[test]
fn group_cycle_emits_group_headers_in_render() {
    let mut backend = populated_backend();
    backend.findings[1].triage_state = "accepted".to_owned();
    let mut model = bootstrap(&mut backend);
    open_findings(&mut model, &mut backend);
    model.handle_key(CockpitKey::Char('g'), &mut backend);
    model.handle_key(CockpitKey::Char('g'), &mut backend);
    assert_eq!(model.finding_group, GroupKey::Triage);
    let rendered = model.render_lines(200, 40).join("\n");
    assert!(rendered.contains("── triage: accepted"), "{rendered}");
    assert!(rendered.contains("── triage: new"), "{rendered}");
}

#[test]
fn grouping_orders_rows_by_group_value() {
    let mut backend = populated_backend();
    backend.findings[0].outbound_state = "posted".to_owned();
    backend.findings[2].outbound_state = "awaiting_approval".to_owned();
    let mut model = bootstrap(&mut backend);
    open_findings(&mut model, &mut backend);
    model.finding_group = GroupKey::Outbound;
    let visible = model.visible_findings();
    let ordered: Vec<&str> = visible
        .iter()
        .map(|&idx| model.findings[idx].outbound_state.as_str())
        .collect();
    assert_eq!(ordered, vec!["awaiting_approval", "not_drafted", "posted"]);
}

// --- finding inspector ----------------------------------------------------------

#[test]
fn inspector_loads_detail_for_focused_finding() {
    let mut backend = populated_backend();
    let mut model = bootstrap(&mut backend);
    open_findings(&mut model, &mut backend);
    assert_eq!(backend.detail_calls, vec!["finding-1".to_owned()]);
    model.handle_key(CockpitKey::Down, &mut backend);
    assert_eq!(
        backend.detail_calls,
        vec!["finding-1".to_owned(), "finding-2".to_owned()]
    );
    let inspector = model.inspector_lines().join("\n");
    assert!(inspector.contains("finding-2"), "{inspector}");
    assert!(inspector.contains("src/lib.rs:10-14"), "{inspector}");
    assert!(inspector.contains("decision history"), "{inspector}");
    assert!(
        inspector.contains("outbound  state=not_drafted"),
        "{inspector}"
    );
    assert!(inspector.contains(CLARIFY_HINT), "{inspector}");
}

#[test]
fn inspector_shows_lineage_columns_from_storage() {
    let mut backend = populated_backend();
    let mut model = bootstrap(&mut backend);
    open_findings(&mut model, &mut backend);
    let rendered = model.render_lines(220, 30).join("\n");
    assert!(
        rendered.contains("initial@run-1→refresh@run-2"),
        "{rendered}"
    );
}

#[test]
fn inspector_detail_failure_degrades_honestly() {
    let mut backend = populated_backend();
    backend.fail_detail = true;
    let mut model = bootstrap(&mut backend);
    open_findings(&mut model, &mut backend);
    assert!(model.finding_detail.is_none());
    let inspector = model.inspector_lines().join("\n");
    assert!(inspector.contains("detail unavailable"), "{inspector}");
    assert!(
        model
            .notice
            .as_deref()
            .is_some_and(|notice| notice.contains("detail load failed")),
        "{:?}",
        model.notice
    );
}

// --- elevated approval flow -------------------------------------------------------

#[test]
fn approve_key_opens_elevation_prompt_with_exact_cli_command() {
    let mut backend = populated_backend();
    let mut model = bootstrap(&mut backend);
    keys(
        &mut model,
        &mut backend,
        &[CockpitKey::Enter, CockpitKey::Char('d')],
    );
    model.handle_key(CockpitKey::Char('a'), &mut backend);
    let Overlay::Elevation {
        batch_id, command, ..
    } = &model.overlay
    else {
        assert!(false, "expected elevation overlay, got {:?}", model.overlay);
        unreachable!()
    };
    assert_eq!(batch_id, "batch-1");
    assert_eq!(command, "rr approve --batch batch-1");
    let rendered = model.render_lines(120, 30).join("\n");
    assert!(rendered.contains("ELEVATED ACTION"), "{rendered}");
    assert!(
        rendered.contains("rr approve --batch batch-1"),
        "{rendered}"
    );
    assert!(rendered.contains(ELEVATED_MUTATION_HINT), "{rendered}");
}

#[test]
fn elevation_requires_typing_approve_exactly() {
    let mut backend = populated_backend();
    let mut model = bootstrap(&mut backend);
    keys(
        &mut model,
        &mut backend,
        &[CockpitKey::Enter, CockpitKey::Char('d')],
    );
    model.handle_key(CockpitKey::Char('a'), &mut backend);

    type_chars(&mut model, &mut backend, "yes");
    model.handle_key(CockpitKey::Enter, &mut backend);
    assert!(
        backend.approve_calls.is_empty(),
        "wrong word must not approve"
    );
    assert!(matches!(model.overlay, Overlay::Elevation { .. }));
    assert!(
        model
            .notice
            .as_deref()
            .is_some_and(|notice| notice.contains("type approve exactly")),
        "{:?}",
        model.notice
    );

    // Clear the wrong input, then confirm correctly.
    keys(
        &mut model,
        &mut backend,
        &[
            CockpitKey::Backspace,
            CockpitKey::Backspace,
            CockpitKey::Backspace,
        ],
    );
    type_chars(&mut model, &mut backend, "approve");
    model.handle_key(CockpitKey::Enter, &mut backend);
    assert_eq!(
        backend.approve_calls,
        vec![("session-1".to_owned(), "batch-1".to_owned())],
        "confirmation executes the same storage approval path as rr approve"
    );
    assert_eq!(model.overlay, Overlay::None);
    assert!(
        model
            .notice
            .as_deref()
            .is_some_and(|notice| notice.contains("recorded local approval")),
        "{:?}",
        model.notice
    );
    // Truthful re-render after reload.
    assert_eq!(model.batches[0].state, "approved");
}

#[test]
fn elevation_escape_cancels_without_mutation() {
    let mut backend = populated_backend();
    let mut model = bootstrap(&mut backend);
    keys(
        &mut model,
        &mut backend,
        &[CockpitKey::Enter, CockpitKey::Char('d')],
    );
    model.handle_key(CockpitKey::Char('a'), &mut backend);
    type_chars(&mut model, &mut backend, "appr");
    model.handle_key(CockpitKey::Esc, &mut backend);
    assert_eq!(model.overlay, Overlay::None);
    assert!(backend.approve_calls.is_empty());
}

#[test]
fn typing_q_in_elevation_prompt_does_not_quit() {
    let mut backend = populated_backend();
    let mut model = bootstrap(&mut backend);
    keys(
        &mut model,
        &mut backend,
        &[CockpitKey::Enter, CockpitKey::Char('d')],
    );
    model.handle_key(CockpitKey::Char('a'), &mut backend);
    assert_eq!(
        model.handle_key(CockpitKey::Char('q'), &mut backend),
        KeyOutcome::Continue
    );
    let Overlay::Elevation { input, .. } = &model.overlay else {
        assert!(false, "expected elevation overlay");
        unreachable!()
    };
    assert_eq!(input, "q");
}

#[test]
fn elevation_blocked_outcome_surfaces_reason_code() {
    let mut backend = populated_backend();
    backend.approve_outcome = Some(ApproveOutcome::Blocked {
        reason_code: "approval_invalidated:target_drift".to_owned(),
    });
    let mut model = bootstrap(&mut backend);
    keys(
        &mut model,
        &mut backend,
        &[CockpitKey::Enter, CockpitKey::Char('d')],
    );
    model.handle_key(CockpitKey::Char('a'), &mut backend);
    type_chars(&mut model, &mut backend, "approve");
    model.handle_key(CockpitKey::Enter, &mut backend);
    assert!(
        model
            .notice
            .as_deref()
            .is_some_and(|notice| notice.contains("approval_invalidated:target_drift")),
        "{:?}",
        model.notice
    );
}

#[test]
fn approve_key_on_already_approved_batch_points_at_cli_only_posting() {
    let mut backend = populated_backend();
    backend.batches[0].state = "approved".to_owned();
    let mut model = bootstrap(&mut backend);
    keys(
        &mut model,
        &mut backend,
        &[CockpitKey::Enter, CockpitKey::Char('d')],
    );
    model.handle_key(CockpitKey::Char('a'), &mut backend);
    assert_eq!(model.overlay, Overlay::None);
    assert!(
        model
            .notice
            .as_deref()
            .is_some_and(|notice| notice.contains("rr post --batch batch-1")),
        "{:?}",
        model.notice
    );
    assert!(backend.approve_calls.is_empty());
}

#[test]
fn approve_key_on_invalidated_batch_is_blocked_locally() {
    let mut backend = populated_backend();
    backend.batches[0].state = "invalidated".to_owned();
    backend.batches[0].invalidation_reason_code = Some("target_drift".to_owned());
    let mut model = bootstrap(&mut backend);
    keys(
        &mut model,
        &mut backend,
        &[CockpitKey::Enter, CockpitKey::Char('d')],
    );
    model.handle_key(CockpitKey::Char('a'), &mut backend);
    assert_eq!(model.overlay, Overlay::None);
    assert!(
        model
            .notice
            .as_deref()
            .is_some_and(|notice| notice.contains("not approvable")),
        "{:?}",
        model.notice
    );
}

// --- drafts rendering ---------------------------------------------------------

#[test]
fn drafts_view_shows_explicit_elevated_mutation_hint() {
    let mut backend = populated_backend();
    let mut model = bootstrap(&mut backend);
    keys(
        &mut model,
        &mut backend,
        &[CockpitKey::Enter, CockpitKey::Char('d')],
    );
    let rendered = model.render_lines(120, 24).join("\n");
    assert!(rendered.contains(ELEVATED_MUTATION_HINT), "{rendered}");
    assert!(rendered.contains("awaiting_approval"), "{rendered}");

    model.handle_key(CockpitKey::Enter, &mut backend);
    let items_rendered = model.render_lines(120, 24).join("\n");
    assert!(
        items_rendered.contains("read-only payload preview"),
        "{items_rendered}"
    );
    assert!(
        items_rendered.contains(ELEVATED_MUTATION_HINT),
        "{items_rendered}"
    );
}

#[test]
fn draft_item_inspector_previews_payload_body() {
    let mut backend = populated_backend();
    let mut model = bootstrap(&mut backend);
    keys(
        &mut model,
        &mut backend,
        &[CockpitKey::Enter, CockpitKey::Char('d'), CockpitKey::Enter],
    );
    let inspector = model.inspector_lines().join("\n");
    assert!(inspector.contains("payload preview"), "{inspector}");
    assert!(
        inspector.contains("Finding: Null result ignored"),
        "{inspector}"
    );
    assert!(inspector.contains("more detail"), "{inspector}");
    assert!(inspector.contains("github:owner/repo#7"), "{inspector}");
}

// --- timeline ------------------------------------------------------------------

#[test]
fn timeline_flattens_runs_stages_and_posted_actions() {
    let mut backend = populated_backend();
    let mut model = bootstrap(&mut backend);
    keys(
        &mut model,
        &mut backend,
        &[CockpitKey::Enter, CockpitKey::Char('t')],
    );
    let entries = model.timeline_entries();
    assert_eq!(
        entries,
        vec![
            TimelineEntry::Run(0),
            TimelineEntry::Stage(0, 0),
            TimelineEntry::Posted(0),
        ]
    );
    let rendered = model.render_lines(160, 30).join("\n");
    assert!(rendered.contains("run run-2"), "{rendered}");
    assert!(rendered.contains("stage initial"), "{rendered}");
    assert!(rendered.contains("github-comment-99"), "{rendered}");
    assert!(rendered.contains("── posted actions"), "{rendered}");
}

#[test]
fn timeline_navigation_drives_inspector_detail() {
    let mut backend = populated_backend();
    let mut model = bootstrap(&mut backend);
    keys(
        &mut model,
        &mut backend,
        &[CockpitKey::Enter, CockpitKey::Char('t')],
    );
    assert!(model.inspector_lines().join("\n").contains("review run"));
    model.handle_key(CockpitKey::Down, &mut backend);
    assert!(
        model
            .inspector_lines()
            .join("\n")
            .contains("worker stage result")
    );
    model.handle_key(CockpitKey::Down, &mut backend);
    let inspector = model.inspector_lines().join("\n");
    assert!(inspector.contains("posted action"), "{inspector}");
    assert!(inspector.contains("github-comment-99"), "{inspector}");
    // Clamp at the end.
    model.handle_key(CockpitKey::Down, &mut backend);
    assert_eq!(model.timeline_cursor, 2);
}

#[test]
fn empty_timeline_renders_honest_state() {
    let mut backend = populated_backend();
    backend.timeline = TimelineView::default();
    let mut model = bootstrap(&mut backend);
    keys(
        &mut model,
        &mut backend,
        &[CockpitKey::Enter, CockpitKey::Char('t')],
    );
    let rendered = model.render_lines(120, 24).join("\n");
    assert!(
        rendered.contains("no review runs persisted for this session yet"),
        "{rendered}"
    );
}

// --- search / recall ----------------------------------------------------------

#[test]
fn search_executes_repo_scoped_query_and_navigates_results() {
    let mut backend = populated_backend();
    let mut model = bootstrap(&mut backend);
    keys(
        &mut model,
        &mut backend,
        &[CockpitKey::Enter, CockpitKey::Char('s')],
    );
    assert_eq!(model.input_mode, InputMode::SearchQuery);
    type_chars(&mut model, &mut backend, "lock");
    model.handle_key(CockpitKey::Enter, &mut backend);
    assert_eq!(
        backend.search_calls,
        vec![("owner/repo".to_owned(), "lock".to_owned())]
    );
    assert_eq!(model.input_mode, InputMode::None);

    let rendered = model.render_lines(200, 30).join("\n");
    assert!(rendered.contains("scope=repo"), "{rendered}");
    assert!(
        rendered.contains("retrieval_mode=lexical_only"),
        "{rendered}"
    );
    assert!(
        rendered.contains("degraded: semantic assets are missing"),
        "{rendered}"
    );
    assert!(rendered.contains("Stale lock handling"), "{rendered}");

    let inspector = model.inspector_lines().join("\n");
    assert!(inspector.contains("prior finding"), "{inspector}");
    assert!(inspector.contains("finding-old"), "{inspector}");
    assert!(inspector.contains("session-0"), "{inspector}");
}

#[test]
fn empty_search_results_surface_honest_notice() {
    let mut backend = populated_backend();
    backend.search_view.hits.clear();
    let mut model = bootstrap(&mut backend);
    keys(
        &mut model,
        &mut backend,
        &[CockpitKey::Enter, CockpitKey::Char('s')],
    );
    type_chars(&mut model, &mut backend, "nothing");
    model.handle_key(CockpitKey::Enter, &mut backend);
    assert!(
        model
            .notice
            .as_deref()
            .is_some_and(|notice| notice.contains("no prior findings matched")),
        "{:?}",
        model.notice
    );
}

#[test]
fn empty_search_query_is_rejected() {
    let mut backend = populated_backend();
    let mut model = bootstrap(&mut backend);
    keys(
        &mut model,
        &mut backend,
        &[CockpitKey::Enter, CockpitKey::Char('s')],
    );
    model.handle_key(CockpitKey::Enter, &mut backend);
    assert!(backend.search_calls.is_empty());
    assert!(
        model
            .notice
            .as_deref()
            .is_some_and(|notice| notice.contains("enter a search query")),
        "{:?}",
        model.notice
    );
}

// --- help overlay ----------------------------------------------------------------

#[test]
fn help_overlay_opens_renders_and_closes() {
    let mut backend = populated_backend();
    let mut model = bootstrap(&mut backend);
    model.handle_key(CockpitKey::Char('?'), &mut backend);
    assert_eq!(model.overlay, Overlay::Help);
    let rendered = model.render_lines(140, 30).join("\n");
    assert!(rendered.contains("keyboard reference"), "{rendered}");
    assert!(rendered.contains("a/i/n/r triage"), "{rendered}");
    assert!(rendered.contains(CLARIFY_HINT), "{rendered}");
    model.handle_key(CockpitKey::Esc, &mut backend);
    assert_eq!(model.overlay, Overlay::None);
}

// --- degraded / empty rendering models --------------------------------------

#[test]
fn empty_store_renders_friendly_cockpit_with_quickstart_hint() {
    let mut backend = FakeBackend::default();
    let model = bootstrap(&mut backend);
    let rendered = model.render_lines(120, 24).join("\n");
    assert!(rendered.contains("Review Home"), "{rendered}");
    assert!(rendered.contains(EMPTY_STORE_HINT), "{rendered}");
}

#[test]
fn backend_errors_surface_as_notice_instead_of_navigation() {
    let mut backend = populated_backend();
    backend.fail_home = true;
    let mut model = bootstrap(&mut backend);
    model.handle_key(CockpitKey::Enter, &mut backend);
    assert_eq!(model.screen, Screen::Home, "failed load must not navigate");
    let rendered = model.render_lines(120, 24).join("\n");
    assert!(rendered.contains("error: home load failed"), "{rendered}");
}

#[test]
fn findings_render_shows_selection_marker_and_states() {
    let mut backend = populated_backend();
    let mut model = bootstrap(&mut backend);
    open_findings(&mut model, &mut backend);
    let rendered = model.render_lines(220, 24);
    let selected_line = rendered
        .iter()
        .find(|line| line.starts_with("> "))
        .expect("selected row marker");
    assert!(
        selected_line.contains("Null result ignored"),
        "{selected_line}"
    );
    assert!(selected_line.contains("triage=new"), "{selected_line}");
    assert!(
        selected_line.contains("outbound=not_drafted"),
        "{selected_line}"
    );
    assert!(
        selected_line.contains("src/finding-1.rs"),
        "{selected_line}"
    );
}

// --- workspace layout --------------------------------------------------------

#[test]
fn render_respects_width_and_height_bounds() {
    let mut backend = populated_backend();
    let model = bootstrap(&mut backend);
    let lines = model.render_lines(20, 8);
    assert!(lines.len() <= 8);
    assert!(lines.iter().all(|line| line.chars().count() <= 20));
}

#[test]
fn wide_terminal_shows_inspector_pane_next_to_primary_pane() {
    let mut backend = populated_backend();
    let mut model = bootstrap(&mut backend);
    open_findings(&mut model, &mut backend);
    let wide = model.render_lines(200, 30).join("\n");
    assert!(
        wide.contains(" │ "),
        "wide layout composes two panes: {wide}"
    );
    assert!(wide.contains("inspector — finding"), "{wide}");

    let narrow = model.render_lines(80, 30).join("\n");
    assert!(
        !narrow.contains("inspector — finding"),
        "narrow layout drops the inspector pane: {narrow}"
    );
}

#[test]
fn status_strip_shows_target_identity_and_attention() {
    let mut backend = populated_backend();
    let mut model = bootstrap(&mut backend);
    let home_status = &model.render_lines(160, 24)[0];
    assert!(home_status.contains("2 session(s)"), "{home_status}");
    assert!(
        home_status.contains("awaiting_user_input=2"),
        "{home_status}"
    );

    open_findings(&mut model, &mut backend);
    let status = &model.render_lines(160, 24)[0];
    assert!(status.contains("owner/repo PR #7"), "{status}");
    assert!(status.contains("attention=awaiting_user_input"), "{status}");
    assert!(status.contains("run=run-2"), "{status}");
    assert!(status.contains("3 findings"), "{status}");
}

#[test]
fn long_queue_windows_around_cursor_with_position_marker() {
    let mut backend = populated_backend();
    backend.findings = (0..50)
        .map(|idx| finding_row(&format!("finding-{idx}"), &format!("Issue {idx}")))
        .collect();
    let mut model = bootstrap(&mut backend);
    open_findings(&mut model, &mut backend);
    for _ in 0..40 {
        model.handle_key(CockpitKey::Down, &mut backend);
    }
    let rendered = model.render_lines(220, 20);
    let joined = rendered.join("\n");
    assert!(
        rendered.iter().any(|line| line.starts_with("> ")),
        "cursor row stays visible in the window: {joined}"
    );
    assert!(
        joined.contains("… rows"),
        "position marker rendered: {joined}"
    );
}

#[test]
fn session_overview_renders_run_state_and_counts() {
    let mut backend = populated_backend();
    let mut model = bootstrap(&mut backend);
    model.handle_key(CockpitKey::Enter, &mut backend);
    let rendered = model.render_lines(200, 30).join("\n");
    assert!(
        rendered.contains("run-2 (initial) of 2 total"),
        "{rendered}"
    );
    assert!(rendered.contains("findings  total=3"), "{rendered}");
    assert!(rendered.contains("posted_actions 1"), "{rendered}");
}

#[test]
fn clarify_key_shows_bounded_hint_instead_of_fake_chat() {
    let mut backend = populated_backend();
    let mut model = bootstrap(&mut backend);
    model.handle_key(CockpitKey::Enter, &mut backend);
    model.handle_key(CockpitKey::Char('c'), &mut backend);
    assert_eq!(model.notice.as_deref(), Some(CLARIFY_HINT));

    open_findings(&mut model, &mut backend);
    model.notice = None;
    model.handle_key(CockpitKey::Char('c'), &mut backend);
    assert_eq!(model.notice.as_deref(), Some(CLARIFY_HINT));
}

// --- session picker ---------------------------------------------------------

fn picker_candidate(id: &str, pr: u64, attention: &str) -> PickerCandidate {
    PickerCandidate {
        session_id: id.to_owned(),
        repository: "owner/repo".to_owned(),
        pull_request: pr,
        provider: "opencode".to_owned(),
        attention_state: attention.to_owned(),
        continuity_tier: Some("tier_b".to_owned()),
        updated_at: 100,
    }
}

fn entry_with_candidates(candidates: Vec<PickerCandidate>) -> CockpitEntry {
    CockpitEntry {
        initial_session_id: None,
        picker_candidates: Some(candidates),
    }
}

#[test]
fn bootstrap_with_picker_candidates_opens_picker_with_explanation() {
    let mut backend = populated_backend();
    let entry = entry_with_candidates(vec![
        picker_candidate("session-1", 7, "awaiting_user_input"),
        picker_candidate("session-2", 7, "outbound_approval_required"),
    ]);
    let model = CockpitModel::bootstrap_with_entry(&mut backend, &entry).expect("bootstrap");
    assert_eq!(model.screen, Screen::Picker);
    assert_eq!(model.picker_candidates.len(), 2);

    let rendered = model.render_lines(160, 30).join("\n");
    assert!(
        rendered.contains("2 sessions exist for owner/repo PR #7 — choose one"),
        "{rendered}"
    );
    // Candidate rows project provider, attention, continuity tier, updated_at.
    assert!(rendered.contains("session-1"), "{rendered}");
    assert!(rendered.contains("[opencode]"), "{rendered}");
    assert!(
        rendered.contains("attention=outbound_approval_required"),
        "{rendered}"
    );
    assert!(rendered.contains("continuity=tier_b"), "{rendered}");
    assert!(
        rendered.contains("Session Picker"),
        "status strip: {rendered}"
    );
}

#[test]
fn picker_enter_opens_selected_candidate_session_home() {
    let mut backend = populated_backend();
    let entry = entry_with_candidates(vec![
        picker_candidate("session-1", 7, "awaiting_user_input"),
        picker_candidate("session-2", 8, "outbound_approval_required"),
    ]);
    let mut model = CockpitModel::bootstrap_with_entry(&mut backend, &entry).expect("bootstrap");
    model.handle_key(CockpitKey::Down, &mut backend);
    assert_eq!(model.picker_cursor, 1);
    model.handle_key(CockpitKey::Enter, &mut backend);
    assert_eq!(model.screen, Screen::SessionHome);
    assert_eq!(
        model.active_session.as_ref().map(|s| s.session_id.as_str()),
        Some("session-2"),
        "Enter opens the highlighted candidate"
    );
    assert!(model.home.is_some());
}

#[test]
fn picker_esc_falls_back_to_full_session_finder() {
    let mut backend = populated_backend();
    let entry = entry_with_candidates(vec![picker_candidate(
        "session-1",
        7,
        "awaiting_user_input",
    )]);
    let mut model = CockpitModel::bootstrap_with_entry(&mut backend, &entry).expect("bootstrap");
    assert_eq!(model.screen, Screen::Picker);
    model.handle_key(CockpitKey::Esc, &mut backend);
    assert_eq!(
        model.screen,
        Screen::Home,
        "Esc falls back to the full finder"
    );
    // The full finder lists every session, not just the candidate.
    assert_eq!(model.sessions.len(), 2);
    let rendered = model.render_lines(160, 30).join("\n");
    assert!(rendered.contains("Review Home"), "{rendered}");
}

#[test]
fn empty_picker_candidates_open_review_home_not_picker() {
    let mut backend = populated_backend();
    let entry = entry_with_candidates(Vec::new());
    let model = CockpitModel::bootstrap_with_entry(&mut backend, &entry).expect("bootstrap");
    assert_eq!(
        model.screen,
        Screen::Home,
        "an empty candidate list is not a disambiguation state"
    );
}

// --- memory / search posture -----------------------------------------------

#[test]
fn search_screen_and_status_strip_render_memory_posture() {
    let mut backend = populated_backend();
    let mut model = bootstrap(&mut backend);
    keys(
        &mut model,
        &mut backend,
        &[CockpitKey::Enter, CockpitKey::Char('s')],
    );
    assert_eq!(model.screen, Screen::Search);
    let rendered = model.render_lines(200, 30).join("\n");
    // Posture line on the Search screen body.
    assert!(
        rendered.contains("memory posture: retrieval_mode=lexical_only semantic=degraded(1)"),
        "{rendered}"
    );
    assert!(
        rendered.contains("posture degraded: semantic model not installed"),
        "{rendered}"
    );
    assert!(
        rendered.contains("embedder backend: fastembed"),
        "{rendered}"
    );
    // Status strip carries a compact posture summary on the Search screen.
    let status = &model.render_lines(200, 30)[0];
    assert!(
        status.contains("memory: retrieval_mode=lexical_only"),
        "{status}"
    );
}

#[test]
fn memory_posture_operational_reports_hybrid_without_degraded_reasons() {
    let mut backend = populated_backend();
    backend.memory_posture = MemoryPostureRow {
        retrieval_mode: "hybrid".to_owned(),
        semantic_operational: true,
        assets_verified: true,
        embedder_available: true,
        embedder_backend: Some("fastembed".to_owned()),
        degraded_reasons: Vec::new(),
    };
    let mut model = bootstrap(&mut backend);
    keys(
        &mut model,
        &mut backend,
        &[CockpitKey::Enter, CockpitKey::Char('s')],
    );
    let rendered = model.render_lines(200, 30).join("\n");
    assert!(
        rendered.contains("memory posture: retrieval_mode=hybrid semantic=operational"),
        "{rendered}"
    );
    assert!(!rendered.contains("posture degraded:"), "{rendered}");
}

#[test]
fn memory_posture_failure_degrades_to_unknown() {
    let mut backend = populated_backend();
    backend.fail_memory_posture = true;
    let mut model = bootstrap(&mut backend);
    assert!(
        model.memory_posture.is_none(),
        "posture load failure leaves posture unknown"
    );
    keys(
        &mut model,
        &mut backend,
        &[CockpitKey::Enter, CockpitKey::Char('s')],
    );
    let rendered = model.render_lines(200, 30).join("\n");
    assert!(rendered.contains("memory posture: unknown"), "{rendered}");
}

fn seed_pending_reviews(backend: &mut FakeBackend) {
    backend.memory_reviews = vec![
        MemoryReviewRow {
            id: "mrr-1".to_owned(),
            source: "worker".to_owned(),
            request_kind: "promote".to_owned(),
            statement: "Prefer explicit fallback gate".to_owned(),
            normalized_key: "prefer explicit fallback gate".to_owned(),
            status: "pending_review".to_owned(),
            created_at: 1,
        },
        MemoryReviewRow {
            id: "mrr-2".to_owned(),
            source: "operator".to_owned(),
            request_kind: "promote".to_owned(),
            statement: "Anchor drift needs a recheck".to_owned(),
            normalized_key: "anchor drift needs a recheck".to_owned(),
            status: "pending_review".to_owned(),
            created_at: 2,
        },
    ];
}

#[test]
fn search_posture_line_surfaces_pending_review_count() {
    let mut backend = populated_backend();
    seed_pending_reviews(&mut backend);
    let mut model = bootstrap(&mut backend);
    keys(
        &mut model,
        &mut backend,
        &[CockpitKey::Enter, CockpitKey::Char('s')],
    );
    let rendered = model.render_lines(200, 30).join("\n");
    assert!(rendered.contains("pending_reviews=2"), "{rendered}");
}

#[test]
fn search_screen_memory_review_lane_accepts_and_rejects() {
    let mut backend = populated_backend();
    seed_pending_reviews(&mut backend);
    let mut model = bootstrap(&mut backend);
    // Open Search, leave the query input (empty Enter), then toggle to the
    // memory-review lane with 'm'.
    keys(
        &mut model,
        &mut backend,
        &[
            CockpitKey::Enter,
            CockpitKey::Char('s'),
            CockpitKey::Enter,
            CockpitKey::Char('m'),
        ],
    );
    assert_eq!(model.search_mode, SearchMode::Reviews);
    let rendered = model.render_lines(200, 30).join("\n");
    assert!(
        rendered.contains("pending memory-review requests"),
        "{rendered}"
    );
    assert!(rendered.contains("mrr-1"), "{rendered}");

    // Accept the focused request: local-only mutation via plain 'a'.
    keys(&mut model, &mut backend, &[CockpitKey::Char('a')]);
    assert_eq!(
        backend.resolve_review_calls,
        vec![("mrr-1".to_owned(), true)]
    );
    let notice = model.notice.clone().unwrap_or_default();
    assert!(notice.contains("accepted"), "{notice}");
    assert!(notice.contains("created"), "{notice}");
    // The resolved request drops out of the pending list.
    assert_eq!(model.memory_reviews.len(), 1);
    assert_eq!(model.memory_reviews[0].id, "mrr-2");

    // Reject the remaining request with 'x'.
    keys(&mut model, &mut backend, &[CockpitKey::Char('x')]);
    assert_eq!(
        backend.resolve_review_calls,
        vec![("mrr-1".to_owned(), true), ("mrr-2".to_owned(), false)]
    );
    assert!(model.memory_reviews.is_empty());
}

// --- explained empty findings states ----------------------------------------

#[test]
fn empty_findings_explains_no_review_run() {
    let mut backend = populated_backend();
    backend.findings.clear();
    backend.home_latest_run_id = None;
    backend.home_stage_count = 0;
    let mut model = bootstrap(&mut backend);
    open_findings(&mut model, &mut backend);
    let rendered = model.render_lines(160, 30).join("\n");
    assert!(
        rendered.contains("no review run yet for this session"),
        "{rendered}"
    );
    assert!(rendered.contains("run rr review --pr 7"), "{rendered}");
}

#[test]
fn empty_findings_explains_worker_has_not_submitted_results() {
    let mut backend = populated_backend();
    backend.findings.clear();
    backend.home_latest_run_id = Some("run-9".to_owned());
    backend.home_stage_count = 0;
    let mut model = bootstrap(&mut backend);
    open_findings(&mut model, &mut backend);
    let rendered = model.render_lines(160, 30).join("\n");
    assert!(
        rendered.contains("the worker has not submitted results yet"),
        "{rendered}"
    );
    assert!(rendered.contains("check rr status"), "{rendered}");
}

#[test]
fn empty_findings_explains_run_completed_with_zero_findings() {
    let mut backend = populated_backend();
    backend.findings.clear();
    backend.home_latest_run_id = Some("run-9".to_owned());
    backend.home_stage_count = 3;
    let mut model = bootstrap(&mut backend);
    open_findings(&mut model, &mut backend);
    let rendered = model.render_lines(160, 30).join("\n");
    assert!(
        rendered.contains("the latest review run completed with zero findings"),
        "{rendered}"
    );
}

// --- draft edit affordance (groundwork) -------------------------------------

fn open_draft_items(model: &mut CockpitModel, backend: &mut FakeBackend) {
    keys(
        model,
        backend,
        &[CockpitKey::Enter, CockpitKey::Char('d'), CockpitKey::Enter],
    );
    assert_eq!(model.screen, Screen::DraftItems);
}

#[test]
fn edit_key_emits_edit_draft_effect_for_focused_item() {
    let mut backend = populated_backend();
    let mut model = bootstrap(&mut backend);
    open_draft_items(&mut model, &mut backend);
    model.handle_key(CockpitKey::Char('e'), &mut backend);
    let effect = model.take_effect().expect("edit effect emitted");
    assert_eq!(
        effect,
        ModelEffect::EditDraft {
            draft_id: "draft-1".to_owned(),
            body: "Finding: Null result ignored\nmore detail".to_owned(),
        }
    );
    // The effect is drained exactly once.
    assert!(model.take_effect().is_none());
}

#[test]
fn apply_draft_revision_persists_and_reloads_preview() {
    let mut backend = populated_backend();
    let mut model = bootstrap(&mut backend);
    open_draft_items(&mut model, &mut backend);
    model.handle_key(CockpitKey::Char('e'), &mut backend);
    let _ = model.take_effect();

    model.apply_draft_revision("draft-1", "Rewritten body line", &mut backend);
    assert_eq!(
        backend.revise_calls,
        vec![("draft-1".to_owned(), "Rewritten body line".to_owned())],
        "revision goes through the storage revise path"
    );
    assert!(
        model
            .notice
            .as_deref()
            .is_some_and(|notice| notice.contains("revised draft draft-1")),
        "{:?}",
        model.notice
    );
    // The read-only preview reflects the reloaded, revised body.
    let inspector = model.inspector_lines().join("\n");
    assert!(inspector.contains("Rewritten body line"), "{inspector}");
    assert!(
        !inspector.contains("Null result ignored"),
        "stale body must not linger: {inspector}"
    );
}

#[test]
fn apply_draft_revision_surfaces_storage_error() {
    let mut backend = populated_backend();
    backend.fail_revise = true;
    let mut model = bootstrap(&mut backend);
    open_draft_items(&mut model, &mut backend);

    model.apply_draft_revision("draft-1", "attempted body", &mut backend);
    assert_eq!(
        backend.revise_calls,
        vec![("draft-1".to_owned(), "attempted body".to_owned())]
    );
    assert!(
        model
            .notice
            .as_deref()
            .is_some_and(|notice| notice.contains("draft edit failed")),
        "{:?}",
        model.notice
    );
}

#[test]
fn draft_items_hint_and_help_document_edit_key() {
    let mut backend = populated_backend();
    let mut model = bootstrap(&mut backend);
    open_draft_items(&mut model, &mut backend);
    let rendered = model.render_lines(120, 24).join("\n");
    assert!(rendered.contains("e edit draft"), "key hint: {rendered}");

    model.handle_key(CockpitKey::Char('?'), &mut backend);
    let help = model.render_lines(140, 40).join("\n");
    assert!(
        help.contains("e edit draft body in $EDITOR"),
        "help overlay: {help}"
    );
}
