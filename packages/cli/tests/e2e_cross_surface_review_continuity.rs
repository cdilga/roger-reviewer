#![cfg(unix)]

use roger_app_core::tui_shell::{
    DraftReviewDecision, FindingDetail, FindingListRow, LocalDraftReviewEntry, MinimalTuiShell,
    ReadOnlySessionSnapshot, SessionChrome, SupervisorSnapshot,
};
use roger_bridge::{handle_bridge_intent, BridgeLaunchIntent, BridgePreflight};
use roger_cli::{run, CliRuntime};
use roger_storage::{CreateMaterializedFinding, RogerStore, UpsertMemoryItem};
use roger_validation::{discover_suite_metadata, failure_artifact_paths};
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output};
use tempfile::{tempdir, TempDir};

const PACKAGED_MANIFEST_KEY_EXTENSION_ID: &str = "djbjigobohmlljboggckmhhnoeldinlp";

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("packages parent")
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn parse_robot_payload(stdout: &str) -> Value {
    serde_json::from_str(stdout).expect("robot payload json")
}

fn run_rr(args: &[&str], runtime: &CliRuntime) -> roger_cli::CliRunResult {
    let argv = args
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    run(&argv, runtime)
}

fn sh_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn rr_binary_hint() -> Option<PathBuf> {
    if let Ok(rr_bin) = std::env::var("CARGO_BIN_EXE_rr") {
        let path = PathBuf::from(rr_bin);
        if path.exists() {
            return Some(path);
        }
    }

    let local = workspace_root().join("target/debug/rr");
    if local.exists() {
        Some(local)
    } else {
        None
    }
}

fn write_rr_host(runtime: &CliRuntime) -> (TempDir, PathBuf) {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("rr-host");

    let launcher = if let Some(rr_binary) = rr_binary_hint() {
        format!("exec {} \"$@\"", sh_quote(&rr_binary.to_string_lossy()))
    } else {
        format!(
            "cd {} || exit 1\nexec cargo run -q -p roger-cli --bin rr -- \"$@\"",
            sh_quote(&workspace_root().to_string_lossy())
        )
    };

    let script = format!(
        "#!/bin/sh\ncd {} || exit 1\nexport RR_STORE_ROOT={}\nexport RR_OPENCODE_BIN={}\n{}\n",
        sh_quote(&runtime.cwd.to_string_lossy()),
        sh_quote(&runtime.store_root.to_string_lossy()),
        sh_quote(&runtime.opencode_bin),
        launcher
    );

    fs::write(&path, script).expect("write rr host script");
    let mut perms = fs::metadata(&path).expect("metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).expect("chmod rr host script");
    (dir, path)
}

fn run_rr_process(args: &[&str], runtime: &CliRuntime) -> Output {
    let (_host_dir, host_path) = write_rr_host(runtime);
    Command::new(host_path)
        .args(args)
        .output()
        .expect("run rr process")
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

fn seed_prior_review_lookup_records(
    store: &RogerStore,
    session_id: &str,
    review_run_id: &str,
    repository: &str,
) {
    let scope_key = format!("repo:{repository}");
    store
        .upsert_materialized_finding(CreateMaterializedFinding {
            id: "finding-e2e-02-1",
            session_id,
            review_run_id,
            stage: "deep_review",
            fingerprint: "fp:e2e-02:approval-refresh",
            title: "Approval token survives stale refresh",
            normalized_summary: "approval token stale refresh should gate posting",
            severity: "high",
            confidence: "high",
            triage_state: "accepted",
            outbound_state: "not_drafted",
        })
        .expect("seed materialized finding");
    store
        .upsert_memory_item(UpsertMemoryItem {
            id: "memory-e2e-02-promoted",
            scope_key: &scope_key,
            memory_class: "procedural",
            state: "proven",
            statement: "approval refresh should reconfirm posting safety",
            normalized_key: "approval refresh reconfirm posting safety",
            anchor_digest: Some("anchor:e2e-02:approval-refresh"),
            source_kind: "manual",
        })
        .expect("seed promoted memory");
    store
        .upsert_memory_item(UpsertMemoryItem {
            id: "memory-e2e-02-candidate",
            scope_key: &scope_key,
            memory_class: "semantic",
            state: "candidate",
            statement: "approval token stale refresh might need operator triage",
            normalized_key: "approval token stale refresh operator triage",
            anchor_digest: None,
            source_kind: "manual",
        })
        .expect("seed candidate memory");
}

fn parse_robot_output(output: &Output) -> Value {
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_robot_payload(&stdout)
}

#[test]
fn e2e_cross_surface_review_continuity_proves_bridge_resume_tui_and_recall_truth() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary();

    let runtime_primary = CliRuntime {
        cwd: repo.clone(),
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };
    let runtime_secondary = CliRuntime {
        cwd: repo,
        store_root: runtime_primary.store_root.clone(),
        opencode_bin: runtime_primary.opencode_bin.clone(),
    };

    let (_host_dir, rr_host) = write_rr_host(&runtime_primary);
    let bridge_response = handle_bridge_intent(
        &BridgeLaunchIntent {
            action: "start_review".to_owned(),
            owner: "owner".to_owned(),
            repo: "repo".to_owned(),
            pr_number: 42,
            head_ref: None,
            instance: None,
            extension_id: Some(PACKAGED_MANIFEST_KEY_EXTENSION_ID.to_owned()),
            browser: Some("chrome".to_owned()),
        },
        &BridgePreflight {
            roger_binary_found: true,
            roger_data_dir_exists: true,
            gh_available: true,
        },
        &rr_host,
    );

    assert!(bridge_response.ok, "{bridge_response:?}");
    assert_eq!(bridge_response.action, "start_review");
    assert!(
        bridge_response
            .message
            .contains("rr review completed for owner/repo#42"),
        "bridge should surface canonical launch detail: {:?}",
        bridge_response
    );
    let session_id = bridge_response
        .session_id
        .clone()
        .expect("bridge should return launched session id");

    let store = RogerStore::open(&runtime_primary.store_root).expect("open store");
    let run = store
        .latest_review_run(&session_id)
        .expect("load latest run")
        .expect("bridge launch should create a review run");
    seed_prior_review_lookup_records(&store, &session_id, &run.id, "owner/repo");

    let draft = run_rr(
        &[
            "draft",
            "--session",
            &session_id,
            "--all-findings",
            "--robot",
        ],
        &runtime_primary,
    );
    assert_eq!(draft.exit_code, 0, "{}", draft.stderr);
    let draft_payload = parse_robot_payload(&draft.stdout);
    let draft_batch_id = draft_payload["data"]["draft_batch"]["id"]
        .as_str()
        .expect("draft batch id")
        .to_owned();
    let draft_id = draft_payload["data"]["drafts"][0]["id"]
        .as_str()
        .expect("draft id")
        .to_owned();

    let status_primary = run_rr(
        &["status", "--session", &session_id, "--robot"],
        &runtime_primary,
    );
    assert_eq!(status_primary.exit_code, 0, "{}", status_primary.stderr);
    let status_primary_payload = parse_robot_payload(&status_primary.stdout);

    let resume_secondary = run_rr_process(
        &["resume", "--session", &session_id, "--robot"],
        &runtime_secondary,
    );
    assert_eq!(
        resume_secondary.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&resume_secondary.stderr)
    );
    let resume_secondary_payload = parse_robot_output(&resume_secondary);
    assert!(
        matches!(
            resume_secondary_payload["outcome"].as_str(),
            Some("complete") | Some("degraded")
        ),
        "unexpected resume outcome: {}",
        resume_secondary_payload
    );
    assert_eq!(resume_secondary_payload["data"]["session_id"], session_id);

    let status_secondary = run_rr_process(
        &["status", "--session", &session_id, "--robot"],
        &runtime_secondary,
    );
    assert_eq!(
        status_secondary.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&status_secondary.stderr)
    );
    let status_secondary_payload = parse_robot_output(&status_secondary);
    assert_eq!(
        status_secondary_payload["data"]["session"]["id"],
        session_id
    );
    assert_eq!(
        status_primary_payload["data"]["continuity"]["state"],
        status_secondary_payload["data"]["continuity"]["state"]
    );
    assert_eq!(
        status_secondary_payload["data"]["drafts"]["awaiting_approval"],
        json!(1)
    );

    let findings_secondary = run_rr_process(
        &["findings", "--session", &session_id, "--robot"],
        &runtime_secondary,
    );
    assert_eq!(
        findings_secondary.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&findings_secondary.stderr)
    );
    let findings_payload = parse_robot_output(&findings_secondary);
    assert_eq!(findings_payload["data"]["count"], json!(1));
    assert_eq!(
        findings_payload["data"]["items"][0]["outbound_state"],
        "awaiting_approval"
    );
    assert_eq!(
        findings_payload["data"]["items"][0]["outbound_detail"]["draft_batch_id"],
        draft_batch_id
    );

    let search_secondary = run_rr_process(
        &[
            "search",
            "--repo",
            "owner/repo",
            "--query",
            "approval refresh",
            "--query-mode",
            "candidate_audit",
            "--robot",
        ],
        &runtime_secondary,
    );
    assert!(
        matches!(search_secondary.status.code(), Some(0) | Some(5)),
        "{}",
        String::from_utf8_lossy(&search_secondary.stderr)
    );
    let search_payload = parse_robot_output(&search_secondary);
    assert_eq!(search_payload["outcome"], "degraded");
    assert_eq!(
        search_payload["data"]["requested_query_mode"],
        "candidate_audit"
    );
    assert_eq!(
        search_payload["data"]["resolved_query_mode"],
        "candidate_audit"
    );
    let scope_bucket = search_payload["data"]["scope_bucket"]
        .as_str()
        .unwrap_or_default();
    assert!(
        matches!(scope_bucket, "repo" | "repo_memory"),
        "scope bucket should stay within repo-only provenance for this slice: {search_payload}"
    );
    assert_eq!(
        search_payload["data"]["search_plan"]["scope_keys"],
        json!(["repo:owner/repo"])
    );
    assert_eq!(
        search_payload["data"]["search_plan"]["retrieval_strategy"]["semantic"],
        json!(false)
    );
    assert_eq!(search_payload["data"]["candidate_included"], json!(true));
    assert!(
        matches!(
            search_payload["data"]["retrieval_mode"].as_str(),
            Some("recovery_scan") | Some("lexical_only")
        ),
        "retrieval mode must stay truthful for semantic-disabled recall: {}",
        search_payload["data"]["retrieval_mode"]
    );

    let lane_counts = search_payload["data"]["lane_counts"]
        .as_object()
        .expect("lane counts object");
    assert!(
        lane_counts["evidence_hits"].as_u64().unwrap_or_default() >= 1,
        "expected evidence lane coverage: {search_payload}"
    );
    assert!(
        lane_counts["promoted_memory"].as_u64().unwrap_or_default() >= 1,
        "expected promoted memory lane coverage: {search_payload}"
    );
    assert!(
        lane_counts["tentative_candidates"]
            .as_u64()
            .unwrap_or_default()
            >= 1,
        "expected candidate lane coverage: {search_payload}"
    );

    let items = search_payload["data"]["items"]
        .as_array()
        .expect("search items array");
    assert!(
        items.iter().any(|item| {
            item["memory_lane"] == "tentative_candidates"
                && item["surface_posture"] == "candidate_review"
        }),
        "candidate memory must remain visibly tentative: {search_payload}"
    );
    assert!(
        items.iter().any(|item| {
            item["memory_lane"] == "promoted_memory" && item["citation_posture"] == "cite_allowed"
        }),
        "promoted memory must preserve citation posture: {search_payload}"
    );
    assert!(
        items.iter().any(|item| {
            let item_scope = item["scope_bucket"].as_str().unwrap_or_default();
            item["memory_lane"] == "evidence_hits" && matches!(item_scope, "repo" | "repo_memory")
        }),
        "evidence hits must carry repo scope provenance buckets: {search_payload}"
    );

    let finding_id = findings_payload["data"]["items"][0]["finding_id"]
        .as_str()
        .expect("finding id")
        .to_owned();
    let finding_title = findings_payload["data"]["items"][0]["title"]
        .as_str()
        .expect("finding title")
        .to_owned();

    let mut shell = MinimalTuiShell::open(ReadOnlySessionSnapshot {
        chrome: SessionChrome {
            session_id: session_id.clone(),
            repository: "owner/repo".to_owned(),
            pull_request_number: 42,
            provider: status_secondary_payload["data"]["session"]["provider"]
                .as_str()
                .unwrap_or("opencode")
                .to_owned(),
            support_tier: status_secondary_payload["data"]["continuity"]["tier"]
                .as_str()
                .unwrap_or("tier_b_first_class")
                .to_owned(),
            isolation_mode: "workspace_local".to_owned(),
            policy_profile: "review_read_only".to_owned(),
            continuity_state: status_secondary_payload["data"]["continuity"]["state"]
                .as_str()
                .unwrap_or("resume:usable")
                .to_owned(),
            attention_state: status_secondary_payload["data"]["attention"]["state"]
                .as_str()
                .unwrap_or("awaiting_user_input")
                .to_owned(),
            status_reason: None,
        },
        overview_lines: vec!["cross-surface continuity verified".to_owned()],
        recent_run_lines: vec![format!("latest run: {}", run.id)],
        findings_preview_lines: vec![finding_title.clone()],
        activity_lines: vec!["resumed on second terminal".to_owned()],
        jobs: Vec::new(),
        supervisor: SupervisorSnapshot {
            queue_depth: 0,
            pending_jobs: 0,
            wake_requested: false,
        },
        finding_rows: vec![FindingListRow {
            finding_id: finding_id.clone(),
            title: finding_title,
            severity: "high".to_owned(),
            triage_state: "accepted".to_owned(),
            outbound_state: "awaiting_approval".to_owned(),
            refresh_lineage: None,
            degraded: false,
        }],
        finding_details: vec![FindingDetail {
            finding_id: finding_id.clone(),
            normalized_summary: "approval token stale refresh should gate posting".to_owned(),
            refresh_lineage: None,
            degraded_reason: None,
            evidence: Vec::new(),
        }],
        local_draft_queue: vec![LocalDraftReviewEntry {
            draft_id: draft_id.clone(),
            finding_id: Some(finding_id.clone()),
            preview: "Please reconfirm approval after refresh.".to_owned(),
            decision: DraftReviewDecision::Pending,
            edited_body: None,
            invalidation_reason: None,
            pending_post: false,
            post_failure_reason: None,
            recovery_hint: None,
            updated_at: 1_700_000_200,
        }],
        active_sessions: Vec::new(),
        search_history: None,
    });

    assert!(shell.select_finding(&finding_id));
    assert!(
        shell.record_triage_intent(&finding_id, "needs_follow_up", 1_700_000_201),
        "TUI triage should accept the resumed finding"
    );
    assert!(
        shell.review_draft(
            &draft_id,
            DraftReviewDecision::Reviewed,
            None,
            None,
            1_700_000_202
        ),
        "TUI draft queue should preserve local draft continuity state"
    );

    let metadata_dir = workspace_root().join("tests/suites");
    let suites = discover_suite_metadata(&metadata_dir).expect("discover suite metadata");
    let suite = suites
        .iter()
        .find(|item| item.id == "e2e_cross_surface_review_continuity")
        .expect("E2E-02 suite metadata");
    assert_eq!(suite.budget_id.as_deref(), Some("E2E-02"));
    assert_eq!(suite.support_tier, "deterministic_chromium_harness");

    let failing_ids = vec!["e2e_cross_surface_review_continuity".to_owned()];
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
            .contains("failures/e2e_cross_surface_review_continuity/sample_failure"),
        "failure artifact namespace should preserve deterministic e2e suite identity"
    );
}
