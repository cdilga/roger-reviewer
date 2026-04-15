use roger_app_core::tui_shell::{
    ActiveSessionEntry, BackgroundJobClass, BackgroundJobSnapshot, BackgroundJobStatus,
    ClarificationIntentStatus, DraftReviewDecision, EvidenceSnippet, FindingDetail, FindingListRow,
    LocalDraftReviewEntry, MinimalTuiShell, ReadOnlySessionSnapshot, SessionChrome,
    SupervisorSnapshot, WakeReason, WakeSignal,
};

fn sample_snapshot() -> ReadOnlySessionSnapshot {
    ReadOnlySessionSnapshot {
        chrome: SessionChrome {
            session_id: "session-42".to_owned(),
            repository: "owner/repo".to_owned(),
            pull_request_number: 42,
            provider: "opencode".to_owned(),
            continuity_state: "review_launched".to_owned(),
            attention_state: "awaiting_user_input".to_owned(),
        },
        overview_lines: vec!["Session launched from CLI".to_owned()],
        recent_run_lines: vec!["explore: completed".to_owned()],
        findings_preview_lines: vec!["FP-1: possible invalidation bug".to_owned()],
        activity_lines: vec!["refresh recommended after new commit".to_owned()],
        jobs: vec![BackgroundJobSnapshot {
            job_id: "job-1".to_owned(),
            class: BackgroundJobClass::Refresh,
            status: BackgroundJobStatus::Queued,
            summary: "refresh queue pending".to_owned(),
        }],
        supervisor: SupervisorSnapshot {
            queue_depth: 1,
            pending_jobs: 1,
            wake_requested: false,
        },
        finding_rows: vec![FindingListRow {
            finding_id: "finding-1".to_owned(),
            title: "Potential approval invalidation bug".to_owned(),
            severity: "high".to_owned(),
            triage_state: "new".to_owned(),
            outbound_state: "drafted".to_owned(),
            refresh_lineage: Some("carried_forward".to_owned()),
            degraded: true,
        }],
        finding_details: vec![FindingDetail {
            finding_id: "finding-1".to_owned(),
            normalized_summary: "Approval token may survive target retargeting".to_owned(),
            refresh_lineage: Some("carried_forward".to_owned()),
            degraded_reason: Some("anchor digest missing on rerun".to_owned()),
            evidence: vec![EvidenceSnippet {
                path: "src/post_gate.rs".to_owned(),
                start_line: 42,
                end_line: Some(49),
                excerpt: "approval token not revoked on target change".to_owned(),
            }],
        }],
        local_draft_queue: vec![LocalDraftReviewEntry {
            draft_id: "draft-1".to_owned(),
            finding_id: Some("finding-1".to_owned()),
            preview: "Please revoke approval when target changes.".to_owned(),
            decision: DraftReviewDecision::Pending,
            edited_body: None,
            invalidation_reason: None,
            pending_post: false,
            post_failure_reason: None,
            recovery_hint: None,
            updated_at: 1_700_000_000,
        }],
        active_sessions: vec![
            ActiveSessionEntry {
                session_id: "session-42".to_owned(),
                repository: "owner/repo".to_owned(),
                pull_request_number: 42,
                provider: "opencode".to_owned(),
                continuity_state: "review_launched".to_owned(),
                attention_state: "awaiting_user_input".to_owned(),
                degraded: false,
            },
            ActiveSessionEntry {
                session_id: "session-43".to_owned(),
                repository: "owner/repo".to_owned(),
                pull_request_number: 43,
                provider: "opencode".to_owned(),
                continuity_state: "awaiting_resume".to_owned(),
                attention_state: "review_launched".to_owned(),
                degraded: true,
            },
        ],
    }
}

#[test]
fn renders_session_chrome_with_read_only_context() {
    let shell = MinimalTuiShell::open(sample_snapshot());
    let line = shell.render_chrome_line();
    assert!(line.contains("owner/repo"));
    assert!(line.contains("PR #42"));
    assert!(line.contains("opencode"));
    assert!(line.contains("awaiting_user_input"));
}

#[test]
fn navigates_findings_and_detail_panels_without_mutation_paths() {
    let mut shell = MinimalTuiShell::open(sample_snapshot());
    assert_eq!(shell.active_panel().title, "Session");

    shell.navigate_next_panel();
    assert_eq!(shell.active_panel().title, "Recent Runs");

    shell.navigate_next_panel();
    assert_eq!(shell.active_panel().title, "Findings");

    shell.navigate_next_panel();
    assert_eq!(shell.active_panel().title, "Finding Detail");
}

#[test]
fn finding_detail_surfaces_lineage_degraded_state_and_evidence() {
    let mut shell = MinimalTuiShell::open(sample_snapshot());
    assert!(shell.select_finding("finding-1"));

    let detail_lines = shell
        .panels
        .iter()
        .find(|panel| panel.title == "Finding Detail")
        .expect("finding detail panel")
        .lines
        .join("\n");

    assert!(detail_lines.contains("refresh_lineage=carried_forward"));
    assert!(detail_lines.contains("degraded_reason=anchor digest missing on rerun"));
    assert!(detail_lines.contains("src/post_gate.rs:42-49"));
    assert!(
        shell
            .selected_finding_detail()
            .expect("selected detail")
            .evidence
            .len()
            == 1
    );
}

#[test]
fn triage_and_clarification_actions_are_recorded_locally_only() {
    let mut shell = MinimalTuiShell::open(sample_snapshot());

    assert!(shell.record_triage_intent("finding-1", "accepted", 1_700_000_111));
    assert!(shell.queue_clarification_intent(
        "intent-1",
        "finding-1",
        "Re-run with latest head?",
        1_700_000_112,
    ));

    assert_eq!(shell.triage_intents.len(), 1);
    assert_eq!(shell.triage_intents[0].from_state, "new");
    assert_eq!(shell.triage_intents[0].to_state, "accepted");
    assert_eq!(shell.clarification_intents.len(), 1);
    assert_eq!(
        shell.clarification_intents[0].status,
        ClarificationIntentStatus::Queued
    );
    assert!(!shell.posting_requested);

    let detail_lines = shell
        .panels
        .iter()
        .find(|panel| panel.title == "Finding Detail")
        .expect("finding detail panel")
        .lines
        .join("\n");
    assert!(detail_lines.contains("clarification_intents_pending=1"));
    assert!(!detail_lines.contains("pending_post"));
    assert!(!detail_lines.contains("post_failed="));
}

#[test]
fn queue_and_inspector_keep_outbound_states_and_posting_elevation_visible() {
    let mut snapshot = sample_snapshot();
    snapshot.finding_rows[0].outbound_state = "approved".to_owned();
    snapshot.local_draft_queue[0].decision = DraftReviewDecision::Approved;
    snapshot.local_draft_queue[0].pending_post = true;

    let mut shell = MinimalTuiShell::open(snapshot);
    shell.select_finding("finding-1");

    let findings_lines = shell
        .panels
        .iter()
        .find(|panel| panel.title == "Findings")
        .expect("findings panel")
        .lines
        .join("\n");
    assert!(findings_lines.contains("outbound=approved"));

    let detail_lines = shell
        .panels
        .iter()
        .find(|panel| panel.title == "Finding Detail")
        .expect("finding detail panel")
        .lines
        .join("\n");
    assert!(detail_lines.contains("triage=new · outbound=approved"));

    let draft_queue_lines = shell
        .panels
        .iter()
        .find(|panel| panel.title == "Draft Queue")
        .expect("draft queue panel")
        .lines
        .join("\n");
    assert!(draft_queue_lines.contains("approved"));
    assert!(draft_queue_lines.contains("pending_post"));

    let mut invalidated_snapshot = sample_snapshot();
    invalidated_snapshot.finding_rows[0].outbound_state = "invalidated".to_owned();
    invalidated_snapshot.local_draft_queue[0].decision = DraftReviewDecision::Invalidated;
    invalidated_snapshot.local_draft_queue[0].invalidation_reason =
        Some("target_rebased".to_owned());
    let mut invalidated_shell = MinimalTuiShell::open(invalidated_snapshot);
    invalidated_shell.select_finding("finding-1");

    let invalidated_detail = invalidated_shell
        .panels
        .iter()
        .find(|panel| panel.title == "Finding Detail")
        .expect("finding detail panel")
        .lines
        .join("\n");
    assert!(invalidated_detail.contains("outbound=invalidated"));

    let invalidated_queue = invalidated_shell
        .panels
        .iter()
        .find(|panel| panel.title == "Draft Queue")
        .expect("draft queue panel")
        .lines
        .join("\n");
    assert!(invalidated_queue.contains("reason=target_rebased"));
}

#[test]
fn local_draft_queue_transitions_keep_pending_post_visibility_local() {
    let mut shell = MinimalTuiShell::open(sample_snapshot());
    assert_eq!(shell.pending_post_drafts().len(), 0);

    assert!(shell.review_draft(
        "draft-1",
        DraftReviewDecision::Edited,
        Some("Updated draft body"),
        None,
        1_700_000_120,
    ));
    assert_eq!(
        shell.local_draft_queue[0].edited_body.as_deref(),
        Some("Updated draft body")
    );
    assert!(!shell.local_draft_queue[0].pending_post);

    assert!(shell.review_draft(
        "draft-1",
        DraftReviewDecision::Approved,
        None,
        None,
        1_700_000_121,
    ));
    assert_eq!(shell.pending_post_drafts().len(), 1);
    assert!(!shell.posting_requested);

    assert!(shell.mark_draft_post_failed(
        "draft-1",
        "github_unavailable",
        Some("retry after restoring gh auth"),
        1_700_000_121,
    ));
    assert_eq!(shell.pending_post_drafts().len(), 0);
    assert_eq!(
        shell.local_draft_queue[0].post_failure_reason.as_deref(),
        Some("github_unavailable")
    );
    assert_eq!(
        shell.local_draft_queue[0].recovery_hint.as_deref(),
        Some("retry after restoring gh auth")
    );

    assert!(shell.review_draft(
        "draft-1",
        DraftReviewDecision::Invalidated,
        None,
        Some("finding became stale"),
        1_700_000_122,
    ));
    assert_eq!(shell.pending_post_drafts().len(), 0);
    assert_eq!(
        shell.local_draft_queue[0].invalidation_reason.as_deref(),
        Some("finding became stale")
    );
    assert!(shell.local_draft_queue[0].post_failure_reason.is_none());
    assert!(!shell.posting_requested);
}

#[test]
fn session_switching_updates_active_chrome_without_side_effects() {
    let mut shell = MinimalTuiShell::open(sample_snapshot());
    assert_eq!(shell.active_session().session_id, "session-42");
    assert_eq!(shell.chrome.session_id, "session-42");

    assert!(shell.switch_to_next_session());
    assert_eq!(shell.active_session().session_id, "session-43");
    assert_eq!(shell.chrome.session_id, "session-43");
    assert_eq!(shell.chrome.pull_request_number, 43);

    assert!(shell.switch_to_previous_session());
    assert_eq!(shell.active_session().session_id, "session-42");
    assert!(shell.switch_to_session("session-43"));
    assert_eq!(shell.chrome.session_id, "session-43");
    assert!(!shell.posting_requested);

    let session_lines = shell
        .panels
        .iter()
        .find(|panel| panel.title == "Session")
        .expect("session panel")
        .lines
        .join("\n");
    assert!(session_lines.contains("active_session=session-43"));
    assert!(session_lines.contains("available_sessions:"));
}

#[test]
fn wake_signal_surfaces_background_jobs_and_supervisor_snapshot() {
    let mut shell = MinimalTuiShell::open(sample_snapshot());

    shell.apply_wake_signal(WakeSignal {
        reason: WakeReason::JobUpdate,
        jobs: vec![BackgroundJobSnapshot {
            job_id: "job-1".to_owned(),
            class: BackgroundJobClass::Refresh,
            status: BackgroundJobStatus::Running,
            summary: "refresh collecting diffs".to_owned(),
        }],
        supervisor: Some(SupervisorSnapshot {
            queue_depth: 2,
            pending_jobs: 1,
            wake_requested: true,
        }),
    });

    assert_eq!(shell.wake_count, 1);
    assert_eq!(shell.jobs.len(), 1);
    assert_eq!(shell.jobs[0].status, BackgroundJobStatus::Running);
    assert_eq!(shell.supervisor.queue_depth, 2);
    assert!(shell.supervisor.wake_requested);
}

#[test]
fn apply_snapshot_clears_stale_pending_post_and_surfaces_canonical_invalidation() {
    let mut shell = MinimalTuiShell::open(sample_snapshot());
    assert!(shell.review_draft(
        "draft-1",
        DraftReviewDecision::Approved,
        None,
        None,
        1_700_000_121,
    ));
    assert_eq!(shell.pending_post_drafts().len(), 1);

    let mut refreshed = sample_snapshot();
    refreshed.finding_rows[0].outbound_state = "invalidated".to_owned();
    refreshed.local_draft_queue[0].decision = DraftReviewDecision::Invalidated;
    refreshed.local_draft_queue[0].pending_post = false;
    refreshed.local_draft_queue[0].invalidation_reason = Some("target_rebased".to_owned());
    refreshed.activity_lines = vec!["posting blocked until draft is regenerated".to_owned()];

    shell.apply_snapshot(refreshed);

    assert_eq!(shell.pending_post_drafts().len(), 0);
    assert!(!shell.posting_requested);
    assert_eq!(
        shell.local_draft_queue[0].decision,
        DraftReviewDecision::Invalidated
    );
    assert_eq!(
        shell.local_draft_queue[0].invalidation_reason.as_deref(),
        Some("target_rebased")
    );

    let findings_lines = shell
        .panels
        .iter()
        .find(|panel| panel.title == "Findings")
        .expect("findings panel")
        .lines
        .join("\n");
    assert!(findings_lines.contains("outbound=invalidated"));

    let draft_queue_lines = shell
        .panels
        .iter()
        .find(|panel| panel.title == "Draft Queue")
        .expect("draft queue panel")
        .lines
        .join("\n");
    assert!(draft_queue_lines.contains("invalidated"));
    assert!(draft_queue_lines.contains("reason=target_rebased"));
    assert!(!draft_queue_lines.contains("pending_post"));
}

#[test]
fn apply_snapshot_preserves_selected_finding_and_panel_when_identity_survives() {
    let mut shell = MinimalTuiShell::open(sample_snapshot());
    shell.navigate_next_panel();
    shell.navigate_next_panel();
    shell.navigate_next_panel();
    assert_eq!(shell.active_panel().title, "Finding Detail");
    assert!(shell.select_finding("finding-1"));

    let mut refreshed = sample_snapshot();
    refreshed.chrome.attention_state = "outbound_approval_required".to_owned();
    refreshed.finding_rows[0].outbound_state = "approved".to_owned();
    refreshed.finding_details[0].normalized_summary =
        "Approval token was revoked and re-approved after refresh".to_owned();
    refreshed.local_draft_queue[0].decision = DraftReviewDecision::Approved;
    refreshed.local_draft_queue[0].pending_post = true;
    refreshed.active_sessions[0].attention_state = "outbound_approval_required".to_owned();

    shell.apply_snapshot(refreshed);

    assert_eq!(shell.active_panel().title, "Finding Detail");
    assert_eq!(
        shell
            .selected_finding_detail()
            .map(|detail| detail.finding_id.as_str()),
        Some("finding-1")
    );
    assert_eq!(shell.chrome.attention_state, "outbound_approval_required");
    assert_eq!(shell.pending_post_drafts().len(), 1);

    let detail_lines = shell
        .panels
        .iter()
        .find(|panel| panel.title == "Finding Detail")
        .expect("finding detail panel")
        .lines
        .join("\n");
    assert!(detail_lines.contains("outbound=approved"));
    assert!(detail_lines.contains("re-approved after refresh"));
}

#[test]
fn apply_snapshot_falls_back_to_first_available_finding_when_selection_is_gone() {
    let mut shell = MinimalTuiShell::open(sample_snapshot());
    shell.navigate_next_panel();
    shell.navigate_next_panel();
    shell.navigate_next_panel();
    assert!(shell.select_finding("finding-1"));

    let mut refreshed = sample_snapshot();
    refreshed.finding_rows = vec![FindingListRow {
        finding_id: "finding-2".to_owned(),
        title: "Posting retry blocked until approval is renewed".to_owned(),
        severity: "medium".to_owned(),
        triage_state: "accepted".to_owned(),
        outbound_state: "awaiting_approval".to_owned(),
        refresh_lineage: Some("revalidated".to_owned()),
        degraded: false,
    }];
    refreshed.finding_details = vec![FindingDetail {
        finding_id: "finding-2".to_owned(),
        normalized_summary: "Refreshed snapshot replaced the prior finding selection".to_owned(),
        refresh_lineage: Some("revalidated".to_owned()),
        degraded_reason: None,
        evidence: Vec::new(),
    }];
    refreshed.local_draft_queue[0].finding_id = Some("finding-2".to_owned());

    shell.apply_snapshot(refreshed);

    assert_eq!(shell.active_panel().title, "Finding Detail");
    assert_eq!(
        shell
            .selected_finding_detail()
            .map(|detail| detail.finding_id.as_str()),
        Some("finding-2")
    );

    let detail_lines = shell
        .panels
        .iter()
        .find(|panel| panel.title == "Finding Detail")
        .expect("finding detail panel")
        .lines
        .join("\n");
    assert!(detail_lines.contains("Finding finding-2"));
    assert!(detail_lines.contains("outbound=awaiting_approval"));
}
