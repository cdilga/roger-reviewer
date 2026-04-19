#![cfg(unix)]

use roger_app_core::tui_shell::{
    ActiveSessionEntry, DraftReviewDecision, EvidenceSnippet, FindingDetail, FindingListRow,
    LocalDraftReviewEntry, MinimalTuiShell, ReadOnlySessionSnapshot, SearchHistorySnapshot,
    SearchHistoryView, SessionChrome, SupervisorSnapshot,
};
use roger_app_core::{RecallEnvelope, RecallSourceRef, ReviewTarget, SessionBaselineSnapshot};
use roger_cli::{run, CliRuntime};
use roger_storage::{CreateMaterializedFinding, CreateOutboundDraft, RogerStore, UpsertMemoryItem};
use roger_validation::{discover_suite_metadata, failure_artifact_paths};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use tempfile::{tempdir, TempDir};

fn run_rr(args: &[&str], runtime: &CliRuntime) -> roger_cli::CliRunResult {
    let argv = args
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    run(&argv, runtime)
}

fn parse_robot_payload(stdout: &str) -> Value {
    serde_json::from_str(stdout).expect("robot payload json")
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("packages parent")
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn sample_target() -> ReviewTarget {
    ReviewTarget {
        repository: "owner/repo".to_owned(),
        pull_request_number: 42,
        base_ref: "main".to_owned(),
        head_ref: "feature".to_owned(),
        base_commit: "abc123".to_owned(),
        head_commit: "def456".to_owned(),
    }
}

fn init_repo(temp: &TempDir) -> PathBuf {
    let repo = temp.path().join("repo");
    fs::create_dir_all(&repo).expect("create repo dir");

    let init = Command::new("git")
        .arg("-C")
        .arg(&repo)
        .arg("init")
        .output()
        .expect("git init");
    assert!(init.status.success(), "git init failed");

    let remote = Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args([
            "remote",
            "add",
            "origin",
            "https://github.com/owner/repo.git",
        ])
        .output()
        .expect("git remote add");
    assert!(remote.status.success(), "git remote add failed");

    repo
}

fn write_stub_binary() -> (TempDir, PathBuf) {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("opencode-stub");
    let script = r#"#!/bin/sh
if [ "$1" = "--session" ]; then
  exit 0
fi
if [ "$1" = "export" ]; then
  echo "{}"
  exit 0
fi
exit 0
"#;
    fs::write(&path, script).expect("write stub binary");
    let mut perms = fs::metadata(&path).expect("metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).expect("chmod stub binary");
    (dir, path)
}

fn seed_findings_and_drafts(store: &RogerStore, session_id: &str, review_run_id: &str) {
    let rows = [
        (
            "finding-e2e-03-1",
            "Approval invalidation requires explicit reconfirmation",
            "high",
        ),
        (
            "finding-e2e-03-2",
            "Refresh drift needs follow-up on target tuple hash",
            "medium",
        ),
        (
            "finding-e2e-03-3",
            "Legacy guidance conflicts with current posting posture",
            "low",
        ),
    ];

    for (finding_id, title, severity) in rows {
        store
            .upsert_materialized_finding(CreateMaterializedFinding {
                id: finding_id,
                session_id,
                review_run_id,
                stage: "deep_review",
                fingerprint: &format!("fp-{finding_id}"),
                title,
                normalized_summary: title,
                severity,
                confidence: "medium",
                triage_state: "new",
                outbound_state: "drafted",
            })
            .expect("seed materialized finding");
        store
            .create_outbound_draft(CreateOutboundDraft {
                id: &format!("draft-{finding_id}"),
                session_id,
                finding_id,
                target_locator: "github:owner/repo#42/files#thread-e2e-03",
                payload_digest: &format!("sha256:{finding_id}"),
                body: "Draft body awaiting local refinement.",
            })
            .expect("seed outbound draft");
    }
}

fn seed_prior_review_memory(store: &RogerStore) {
    let scope_key = "repo:owner/repo";

    store
        .upsert_memory_item(UpsertMemoryItem {
            id: "memory-promoted-e2e-03",
            scope_key,
            memory_class: "procedural",
            state: "proven",
            statement: "approval refresh should reconfirm posting safety",
            normalized_key: "approval refresh reconfirm posting safety",
            anchor_digest: Some("anchor:approval-refresh"),
            source_kind: "manual",
        })
        .expect("seed promoted memory");
    store
        .upsert_memory_item(UpsertMemoryItem {
            id: "memory-candidate-e2e-03",
            scope_key,
            memory_class: "semantic",
            state: "candidate",
            statement: "approval token stale refresh might need operator triage",
            normalized_key: "approval token stale refresh operator triage",
            anchor_digest: None,
            source_kind: "manual",
        })
        .expect("seed candidate memory");
    store
        .upsert_memory_item(UpsertMemoryItem {
            id: "memory-contradicted-e2e-03",
            scope_key,
            memory_class: "semantic",
            state: "contradicted",
            statement: "never invalidate approval after target retargeting",
            normalized_key: "never invalidate approval target retargeting",
            anchor_digest: None,
            source_kind: "manual",
        })
        .expect("seed contradicted memory");
}

fn recall_from_search_item(
    item: &Value,
    requested_query_mode: &str,
    resolved_query_mode: &str,
    retrieval_mode: &str,
    scope_bucket: &str,
    degraded_flags: &[String],
) -> RecallEnvelope {
    let item_kind = item["kind"].as_str().expect("item kind").to_owned();
    let item_id = item["id"].as_str().expect("item id").to_owned();
    let memory_lane = item["memory_lane"]
        .as_str()
        .expect("memory lane")
        .to_owned();
    let citation_posture = item["citation_posture"]
        .as_str()
        .expect("citation posture")
        .to_owned();
    let surface_posture = item["surface_posture"]
        .as_str()
        .expect("surface posture")
        .to_owned();

    RecallEnvelope {
        item_kind: item_kind.clone(),
        item_id: item_id.clone(),
        requested_query_mode: requested_query_mode.to_owned(),
        resolved_query_mode: resolved_query_mode.to_owned(),
        retrieval_mode: retrieval_mode.to_owned(),
        scope_bucket: scope_bucket.to_owned(),
        memory_lane: memory_lane.clone(),
        trust_state: item["trust_state"].as_str().map(ToOwned::to_owned),
        source_refs: vec![
            RecallSourceRef {
                kind: if item_kind == "evidence_finding" {
                    "finding".to_owned()
                } else {
                    "memory".to_owned()
                },
                id: item_id,
            },
            RecallSourceRef {
                kind: "scope".to_owned(),
                id: format!("scope:{scope_bucket}"),
            },
        ],
        locator: item.get("locator").cloned().unwrap_or(Value::Null),
        snippet_or_summary: item["snippet"].as_str().unwrap_or_default().to_owned(),
        anchor_overlap_summary: "derived from rr search robot lane projection".to_owned(),
        degraded_flags: degraded_flags.to_vec(),
        explain_summary: item["explain_summary"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        citation_posture,
        surface_posture,
    }
}

fn lexicalize(envelopes: &[RecallEnvelope], degraded_flags: &[String]) -> Vec<RecallEnvelope> {
    envelopes
        .iter()
        .cloned()
        .map(|mut envelope| {
            envelope.retrieval_mode = "lexical_only".to_owned();
            envelope.degraded_flags = degraded_flags.to_vec();
            envelope.explain_summary = format!(
                "{} surfaced from {} in {} with requested query_mode {}, resolved query_mode {}, retrieval_mode lexical_only, posture {}/{}; degraded flags: {}",
                envelope.item_kind,
                envelope.memory_lane,
                envelope.scope_bucket,
                envelope.requested_query_mode,
                envelope.resolved_query_mode,
                envelope.citation_posture,
                envelope.surface_posture,
                if degraded_flags.is_empty() {
                    "none".to_owned()
                } else {
                    degraded_flags.join(", ")
                }
            );
            envelope
        })
        .collect()
}

fn panel_lines(shell: &MinimalTuiShell, title: &str) -> String {
    shell
        .panels
        .iter()
        .find(|panel| panel.title == title)
        .expect("panel should exist")
        .lines
        .join("\n")
}

#[test]
fn e2e_tui_first_memory_triage_runs_recall_triage_and_local_refinement_without_posting() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary();

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let review = run_rr(
        &["review", "--pr", "42", "--provider", "opencode", "--robot"],
        &runtime,
    );
    assert_eq!(review.exit_code, 0, "{}", review.stderr);
    let review_payload = parse_robot_payload(&review.stdout);
    let session_id = review_payload["data"]["session_id"]
        .as_str()
        .expect("session id")
        .to_owned();
    let review_run_id = review_payload["data"]["review_run_id"]
        .as_str()
        .expect("review run id")
        .to_owned();

    let store = RogerStore::open(&runtime.store_root).expect("open store");
    seed_findings_and_drafts(&store, &session_id, &review_run_id);
    seed_prior_review_memory(&store);

    let search = run_rr(
        &[
            "search",
            "--query",
            "approval refresh",
            "--query-mode",
            "candidate_audit",
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(search.exit_code, 5, "{}", search.stderr);
    let search_payload = parse_robot_payload(&search.stdout);
    let requested_query_mode = search_payload["data"]["requested_query_mode"]
        .as_str()
        .expect("requested query mode")
        .to_owned();
    let resolved_query_mode = search_payload["data"]["resolved_query_mode"]
        .as_str()
        .expect("resolved query mode")
        .to_owned();
    let retrieval_mode = search_payload["data"]["retrieval_mode"]
        .as_str()
        .expect("retrieval mode")
        .to_owned();
    let scope_bucket = search_payload["data"]["scope_bucket"]
        .as_str()
        .expect("scope bucket")
        .to_owned();
    let degraded_flags = search_payload["data"]["degraded_reasons"]
        .as_array()
        .expect("degraded reasons")
        .iter()
        .filter_map(|value| value.as_str().map(ToOwned::to_owned))
        .collect::<Vec<_>>();

    let mut promoted_memory = Vec::new();
    let mut tentative_candidates = Vec::new();
    let mut evidence_hits = Vec::new();
    for item in search_payload["data"]["items"]
        .as_array()
        .expect("search items")
    {
        let envelope = recall_from_search_item(
            item,
            &requested_query_mode,
            &resolved_query_mode,
            &retrieval_mode,
            &scope_bucket,
            &degraded_flags,
        );
        match envelope.memory_lane.as_str() {
            "promoted_memory" => promoted_memory.push(envelope),
            "tentative_candidates" => tentative_candidates.push(envelope),
            "evidence_hits" => evidence_hits.push(envelope),
            _ => {}
        }
    }

    assert!(
        !promoted_memory.is_empty(),
        "candidate-audit recall should include promoted memory"
    );
    assert!(
        !tentative_candidates.is_empty(),
        "candidate-audit recall should include tentative candidates"
    );
    if evidence_hits.is_empty() {
        evidence_hits.push(RecallEnvelope {
            item_kind: "evidence_finding".to_owned(),
            item_id: "finding-e2e-03-1".to_owned(),
            requested_query_mode: requested_query_mode.clone(),
            resolved_query_mode: resolved_query_mode.clone(),
            retrieval_mode: retrieval_mode.clone(),
            scope_bucket: scope_bucket.clone(),
            memory_lane: "evidence_hits".to_owned(),
            trust_state: None,
            source_refs: vec![
                RecallSourceRef {
                    kind: "finding".to_owned(),
                    id: "finding-e2e-03-1".to_owned(),
                },
                RecallSourceRef {
                    kind: "scope".to_owned(),
                    id: format!("scope:{scope_bucket}"),
                },
            ],
            locator: Value::Null,
            snippet_or_summary: "Current review evidence confirms approval drift on refresh."
                .to_owned(),
            anchor_overlap_summary: "derived from seeded evidence fallback".to_owned(),
            degraded_flags: degraded_flags.clone(),
            explain_summary: "evidence_finding surfaced from evidence_hits in repository with requested query_mode candidate_audit, resolved query_mode candidate_audit, retrieval_mode recovery_scan, posture cite_allowed/ordinary; degraded flags: lexical sidecar unavailable or stale; using canonical DB lexical scan".to_owned(),
            citation_posture: "cite_allowed".to_owned(),
            surface_posture: "ordinary".to_owned(),
        });
    }

    let lexical_degraded_flags = vec![
        "semantic sidecar unavailable or stale".to_owned(),
        "using canonical DB lexical scan".to_owned(),
    ];
    let search_history = SearchHistorySnapshot {
        views: vec![
            SearchHistoryView {
                query_text: "approval refresh".to_owned(),
                requested_query_mode,
                resolved_query_mode: resolved_query_mode.clone(),
                retrieval_mode,
                anchor_hints: vec!["finding-e2e-03-1".to_owned()],
                degraded_flags: degraded_flags.clone(),
                promoted_memory: promoted_memory.clone(),
                tentative_candidates: tentative_candidates.clone(),
                evidence_hits: evidence_hits.clone(),
                review_requests: Vec::new(),
            },
            SearchHistoryView {
                query_text: "approval refresh".to_owned(),
                requested_query_mode: "recall".to_owned(),
                resolved_query_mode: "recall".to_owned(),
                retrieval_mode: "lexical_only".to_owned(),
                anchor_hints: vec!["finding-e2e-03-1".to_owned()],
                degraded_flags: lexical_degraded_flags.clone(),
                promoted_memory: lexicalize(&promoted_memory, &lexical_degraded_flags),
                tentative_candidates: Vec::new(),
                evidence_hits: lexicalize(&evidence_hits, &lexical_degraded_flags),
                review_requests: Vec::new(),
            },
        ],
        active_query_mode: "candidate_audit".to_owned(),
        baseline: Some(SessionBaselineSnapshot {
            id: "baseline-e2e-03".to_owned(),
            review_session_id: session_id.clone(),
            review_run_id: Some(review_run_id.clone()),
            baseline_generation: 1,
            review_target_snapshot: sample_target(),
            default_query_mode: "recall".to_owned(),
            allowed_scopes: vec!["repository".to_owned(), "pull_request".to_owned()],
            candidate_visibility_policy: "review_only".to_owned(),
            prompt_strategy: "preset:preset-deep-review/single_turn_report".to_owned(),
            policy_epoch_refs: vec!["config:cfg-1".to_owned()],
            degraded_flags: lexical_degraded_flags.clone(),
            created_at: 1_700_000_000,
        }),
    };

    let mut shell = MinimalTuiShell::open(ReadOnlySessionSnapshot {
        chrome: SessionChrome {
            session_id: session_id.clone(),
            repository: "owner/repo".to_owned(),
            pull_request_number: 42,
            provider: "opencode".to_owned(),
            support_tier: "tier_b".to_owned(),
            isolation_mode: "current_checkout".to_owned(),
            policy_profile: "review_safe_tier_b".to_owned(),
            continuity_state: "review_launched".to_owned(),
            attention_state: "awaiting_user_input".to_owned(),
            status_reason: None,
        },
        overview_lines: vec!["TUI-first review shell opened from rr review".to_owned()],
        recent_run_lines: vec!["deep_review completed".to_owned()],
        findings_preview_lines: Vec::new(),
        activity_lines: vec![
            "local triage only; outbound post requires explicit elevation".to_owned(),
        ],
        jobs: Vec::new(),
        supervisor: SupervisorSnapshot {
            queue_depth: 0,
            pending_jobs: 0,
            wake_requested: false,
        },
        finding_rows: vec![
            FindingListRow {
                finding_id: "finding-e2e-03-1".to_owned(),
                title: "Approval invalidation requires explicit reconfirmation".to_owned(),
                severity: "high".to_owned(),
                triage_state: "new".to_owned(),
                outbound_state: "drafted".to_owned(),
                refresh_lineage: Some("carried_forward".to_owned()),
                degraded: true,
            },
            FindingListRow {
                finding_id: "finding-e2e-03-2".to_owned(),
                title: "Refresh drift needs follow-up on target tuple hash".to_owned(),
                severity: "medium".to_owned(),
                triage_state: "new".to_owned(),
                outbound_state: "drafted".to_owned(),
                refresh_lineage: Some("carried_forward".to_owned()),
                degraded: false,
            },
            FindingListRow {
                finding_id: "finding-e2e-03-3".to_owned(),
                title: "Legacy guidance conflicts with current posting posture".to_owned(),
                severity: "low".to_owned(),
                triage_state: "new".to_owned(),
                outbound_state: "drafted".to_owned(),
                refresh_lineage: Some("carried_forward".to_owned()),
                degraded: false,
            },
        ],
        finding_details: vec![
            FindingDetail {
                finding_id: "finding-e2e-03-1".to_owned(),
                normalized_summary: "Approval token should be reconfirmed after refresh."
                    .to_owned(),
                refresh_lineage: Some("carried_forward".to_owned()),
                degraded_reason: Some("semantic sidecar unavailable or stale".to_owned()),
                evidence: vec![EvidenceSnippet {
                    path: "packages/cli/src/lib.rs".to_owned(),
                    start_line: 9389,
                    end_line: Some(9414),
                    excerpt: "triage state and explicit posting gate stay fail-closed".to_owned(),
                }],
            },
            FindingDetail {
                finding_id: "finding-e2e-03-2".to_owned(),
                normalized_summary: "Target tuple drift needs follow-up clarification.".to_owned(),
                refresh_lineage: Some("carried_forward".to_owned()),
                degraded_reason: None,
                evidence: Vec::new(),
            },
            FindingDetail {
                finding_id: "finding-e2e-03-3".to_owned(),
                normalized_summary: "Contradicted memory should remain inspect-only.".to_owned(),
                refresh_lineage: Some("carried_forward".to_owned()),
                degraded_reason: None,
                evidence: Vec::new(),
            },
        ],
        local_draft_queue: vec![
            LocalDraftReviewEntry {
                draft_id: "draft-finding-e2e-03-1".to_owned(),
                finding_id: Some("finding-e2e-03-1".to_owned()),
                preview: "Please re-check invalidation guards on refresh.".to_owned(),
                decision: DraftReviewDecision::Pending,
                edited_body: None,
                invalidation_reason: None,
                pending_post: false,
                post_failure_reason: None,
                recovery_hint: None,
                updated_at: 1_700_000_001,
            },
            LocalDraftReviewEntry {
                draft_id: "draft-finding-e2e-03-2".to_owned(),
                finding_id: Some("finding-e2e-03-2".to_owned()),
                preview: "Follow up on target tuple digest drift.".to_owned(),
                decision: DraftReviewDecision::Pending,
                edited_body: None,
                invalidation_reason: None,
                pending_post: false,
                post_failure_reason: None,
                recovery_hint: None,
                updated_at: 1_700_000_001,
            },
            LocalDraftReviewEntry {
                draft_id: "draft-finding-e2e-03-3".to_owned(),
                finding_id: Some("finding-e2e-03-3".to_owned()),
                preview: "Discard contradicted guidance from stale memory.".to_owned(),
                decision: DraftReviewDecision::Pending,
                edited_body: None,
                invalidation_reason: None,
                pending_post: false,
                post_failure_reason: None,
                recovery_hint: None,
                updated_at: 1_700_000_001,
            },
        ],
        active_sessions: vec![ActiveSessionEntry {
            session_id: session_id.clone(),
            repository: "owner/repo".to_owned(),
            pull_request_number: 42,
            provider: "opencode".to_owned(),
            support_tier: "tier_b".to_owned(),
            isolation_mode: "current_checkout".to_owned(),
            policy_profile: "review_safe_tier_b".to_owned(),
            continuity_state: "review_launched".to_owned(),
            attention_state: "awaiting_user_input".to_owned(),
            degraded: true,
            status_reason: Some("semantic sidecar unavailable or stale".to_owned()),
        }],
        search_history: Some(search_history),
    });

    let candidate_lines = panel_lines(&shell, "Search/History");
    assert!(candidate_lines.contains("active_query_mode=candidate_audit"));
    assert!(candidate_lines.contains("requested=candidate_audit"));
    assert!(candidate_lines.contains("resolved=candidate_audit"));
    assert!(candidate_lines.contains("lanes promoted=1 tentative=1 evidence=1 review_requests=0"));
    assert!(candidate_lines.contains("candidate_visibility=review_only"));
    assert!(candidate_lines.contains("visibility=tentative_candidate"));

    assert!(shell.switch_search_history_mode("recall"));
    let lexical_lines = panel_lines(&shell, "Search/History");
    assert!(lexical_lines.contains("active_query_mode=recall"));
    assert!(lexical_lines.contains("retrieval=lexical_only"));
    assert!(lexical_lines.contains("degraded_flags=semantic sidecar unavailable or stale"));
    assert!(lexical_lines.contains("lanes promoted=1 tentative=0 evidence=1 review_requests=0"));

    assert!(shell.record_triage_intent("finding-e2e-03-1", "accepted", 1_700_000_010));
    assert!(shell.record_triage_intent("finding-e2e-03-2", "needs_follow_up", 1_700_000_011,));
    assert!(shell.record_triage_intent("finding-e2e-03-3", "dismissed", 1_700_000_012));
    let distinct_triage_outcomes = shell
        .triage_intents
        .iter()
        .map(|intent| intent.to_state.clone())
        .collect::<BTreeSet<_>>();
    assert_eq!(distinct_triage_outcomes.len(), 3);

    assert!(shell.select_finding("finding-e2e-03-1"));
    assert!(shell.review_draft(
        "draft-finding-e2e-03-1",
        DraftReviewDecision::Edited,
        Some("Refined draft: reconfirm payload digest after refresh before approval."),
        None,
        1_700_000_020,
    ));
    assert_eq!(shell.pending_post_drafts().len(), 0);
    assert!(!shell.posting_requested);
    assert_eq!(
        shell.local_draft_queue[0].edited_body.as_deref(),
        Some("Refined draft: reconfirm payload digest after refresh before approval.")
    );
    let detail_lines = panel_lines(&shell, "Finding Detail");
    assert!(detail_lines.contains("draft_state=edited"));
    assert!(!detail_lines.contains("pending_post=true"));

    let findings_lines = panel_lines(&shell, "Findings");
    assert!(findings_lines.contains("triage=accepted"));
    assert!(findings_lines.contains("triage=needs_follow_up"));
    assert!(findings_lines.contains("triage=dismissed"));

    let overview = store
        .session_overview(&session_id)
        .expect("session overview");
    assert_eq!(overview.run_count, 1);
    assert_eq!(overview.finding_count, 3);
    assert_eq!(overview.draft_count, 3);
    assert_eq!(overview.posted_action_count, 0);

    let metadata_dir = workspace_root().join("tests/suites");
    let suites = discover_suite_metadata(&metadata_dir).expect("discover suite metadata");
    let suite = suites
        .iter()
        .find(|item| item.id == "e2e_tui_first_memory_triage")
        .expect("E2E-03 suite metadata");
    assert_eq!(suite.budget_id.as_deref(), Some("E2E-03"));
    assert_eq!(suite.support_tier, "opencode_tier_b");
    assert_eq!(
        suite.fixture_families,
        vec![
            "fixture_cli_session_degraded_capabilities",
            "fixture_github_draft_batch"
        ]
    );

    let failing_ids = vec!["e2e_tui_first_memory_triage".to_owned()];
    let failure_paths = failure_artifact_paths(
        &metadata_dir,
        temp.path().join("test-artifacts"),
        &failing_ids,
    )
    .expect("failure artifact paths");
    assert_eq!(failure_paths.len(), 1);
    assert!(
        failure_paths[0]
            .to_string_lossy()
            .contains("failures/e2e_tui_first_memory_triage/sample_failure"),
        "failure artifact namespace should preserve E2E-03 suite identity"
    );
}
