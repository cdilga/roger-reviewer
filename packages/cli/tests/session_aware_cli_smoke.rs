#![cfg(unix)]

use roger_app_core::{
    ApprovalState, ContinuityQuality, HarnessAdapter, HarnessCommandBinding, LaunchAction,
    LaunchIntent, OutboundApprovalToken, OutboundDraft, OutboundDraftBatch, PostedAction,
    PostedActionStatus, ResumeBundle, ResumeBundleProfile, ReviewTarget, ReviewTask,
    ReviewTaskKind, RogerCommand, RogerCommandId, RogerCommandInvocationSurface,
    RogerCommandRouteStatus, Surface, WORKER_OPERATION_REQUEST_SCHEMA_V1,
    WORKER_STAGE_RESULT_SCHEMA_V1, WorkerContextPacket, WorkerGitHubPosture, WorkerMutationPosture,
    WorkerOperationResponseStatus, WorkerStageOutcome, WorkerStageResult, WorkerTransportKind,
    outbound_target_tuple_json, route_harness_command,
};
use roger_bridge::{BridgeLaunchIntent, BridgePreflight, BridgeResponse, handle_bridge_intent};
use roger_cli::{CliRuntime, HarnessCommandInvocation, run, run_harness_command};
use roger_session_opencode::OpenCodeAdapter;
use roger_storage::{
    ArtifactBudgetClass, CreateMaterializedFinding, CreateReviewRun, CreateReviewSession,
    CreateSessionLaunchBinding, LaunchAttemptState, LaunchSurface, RogerStore, UpsertMemoryItem,
};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;
use tempfile::{TempDir, tempdir};

fn sample_target(pr_number: u64) -> ReviewTarget {
    ReviewTarget {
        repository: "owner/repo".to_owned(),
        pull_request_number: pr_number,
        base_ref: "main".to_owned(),
        head_ref: format!("feature-{pr_number}"),
        base_commit: "aaa".to_owned(),
        head_commit: "bbb".to_owned(),
    }
}

fn sample_worker_task() -> ReviewTask {
    ReviewTask {
        id: "task-rr-agent-1".to_owned(),
        review_session_id: "session-rr-agent-1".to_owned(),
        review_run_id: "run-rr-agent-1".to_owned(),
        stage: "deep_review".to_owned(),
        task_kind: ReviewTaskKind::DeepReviewPass,
        task_nonce: "nonce-rr-agent-1".to_owned(),
        objective: "Review approval and posting safety regressions.".to_owned(),
        turn_strategy: roger_app_core::WorkerTurnStrategy::SingleTurnReport,
        allowed_scopes: vec!["repo".to_owned()],
        allowed_operations: vec![
            "worker.get_review_context".to_owned(),
            "worker.search_memory".to_owned(),
            "worker.get_status".to_owned(),
            "worker.list_findings".to_owned(),
            "worker.get_finding_detail".to_owned(),
            "worker.get_artifact_excerpt".to_owned(),
            "worker.request_clarification".to_owned(),
            "worker.request_memory_review".to_owned(),
            "worker.propose_follow_up".to_owned(),
            "worker.submit_stage_result".to_owned(),
        ],
        expected_result_schema: WORKER_STAGE_RESULT_SCHEMA_V1.to_owned(),
        prompt_preset_id: Some("preset-deep-review".to_owned()),
        created_at: 100,
    }
}

fn sample_worker_context(task: &ReviewTask) -> WorkerContextPacket {
    WorkerContextPacket {
        review_target: sample_target(42),
        review_session_id: task.review_session_id.clone(),
        review_run_id: task.review_run_id.clone(),
        review_task_id: task.id.clone(),
        task_nonce: task.task_nonce.clone(),
        baseline_snapshot_ref: Some("baseline-rr-agent-1".to_owned()),
        provider: "opencode".to_owned(),
        transport_kind: WorkerTransportKind::AgentCli,
        stage: task.stage.clone(),
        objective: task.objective.clone(),
        allowed_scopes: task.allowed_scopes.clone(),
        allowed_operations: task.allowed_operations.clone(),
        mutation_posture: WorkerMutationPosture::ReviewOnly,
        github_posture: WorkerGitHubPosture::Blocked,
        unresolved_findings: Vec::new(),
        continuity_summary: Some("provider continuity is usable".to_owned()),
        memory_cards: Vec::new(),
        artifact_refs: Vec::new(),
    }
}

fn sample_stage_result(task: &ReviewTask) -> WorkerStageResult {
    WorkerStageResult {
        schema_id: WORKER_STAGE_RESULT_SCHEMA_V1.to_owned(),
        review_session_id: task.review_session_id.clone(),
        review_run_id: task.review_run_id.clone(),
        review_task_id: task.id.clone(),
        worker_invocation_id: None,
        task_nonce: task.task_nonce.clone(),
        stage: task.stage.clone(),
        task_kind: task.task_kind,
        outcome: WorkerStageOutcome::Completed,
        summary: "Found one likely invalidation issue.".to_owned(),
        structured_findings_pack: Some(json!({
            "schema_version": "structured_findings_pack.v1",
            "findings": [
                {
                    "title": "Approval token survives stale refresh",
                    "summary": "The refresh path reports success without invalidating an approval token.",
                    "severity": "high",
                    "confidence": "medium"
                }
            ]
        })),
        clarification_requests: Vec::new(),
        memory_review_requests: Vec::new(),
        follow_up_proposals: Vec::new(),
        memory_citations: Vec::new(),
        artifact_refs: Vec::new(),
        provider_metadata: Some(json!({"provider": "opencode"})),
        warnings: Vec::new(),
    }
}

fn sample_worker_request(task: &ReviewTask, operation: &str, payload: Option<Value>) -> Value {
    json!({
        "schema_id": WORKER_OPERATION_REQUEST_SCHEMA_V1,
        "review_session_id": task.review_session_id,
        "review_run_id": task.review_run_id,
        "review_task_id": task.id,
        "task_nonce": task.task_nonce,
        "operation": operation,
        "requested_scopes": ["repo"],
        "payload": payload,
    })
}

fn write_json_fixture(path: &Path, value: &impl serde::Serialize) {
    fs::write(
        path,
        serde_json::to_vec_pretty(value).expect("serialize fixture json"),
    )
    .expect("write fixture json");
}

fn seed_rr_agent_session(runtime: &CliRuntime, task: &ReviewTask) {
    let store = RogerStore::open(&runtime.store_root).expect("open store");
    store
        .create_review_session(CreateReviewSession {
            id: &task.review_session_id,
            review_target: &sample_target(42),
            provider: "opencode",
            session_locator: None,
            resume_bundle_artifact_id: None,
            continuity_state: "resume:usable",
            attention_state: "awaiting_user_input",
            launch_profile_id: Some("profile-open-pr"),
        })
        .expect("create review session");
    store
        .create_review_run(CreateReviewRun {
            id: &task.review_run_id,
            session_id: &task.review_session_id,
            run_kind: "deep_review",
            repo_snapshot: "{\"head\":\"bbb\"}",
            continuity_quality: "usable",
            session_locator_artifact_id: None,
        })
        .expect("create review run");
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
            id: "finding-search-1",
            session_id,
            review_run_id,
            stage: "deep_review",
            fingerprint: "fp:approval-refresh",
            title: "Approval token survives stale refresh",
            normalized_summary: "approval token stale refresh should gate posting",
            severity: "high",
            confidence: "high",
            triage_state: "accepted",
            outbound_state: "drafted",
        })
        .expect("seed materialized finding");
    store
        .upsert_memory_item(UpsertMemoryItem {
            id: "memory-promoted-1",
            scope_key: &scope_key,
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
            id: "memory-candidate-1",
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

fn sample_launch_intent(action: LaunchAction) -> LaunchIntent {
    LaunchIntent {
        action,
        source_surface: Surface::Cli,
        objective: Some("cli smoke".to_owned()),
        launch_profile_id: Some("profile-open-pr".to_owned()),
        cwd: Some("/tmp/repo".to_owned()),
        worktree_root: None,
    }
}

fn dropout_bundle(target: ReviewTarget) -> ResumeBundle {
    ResumeBundle {
        schema_version: 1,
        profile: ResumeBundleProfile::DropoutControl,
        review_target: target,
        launch_intent: sample_launch_intent(LaunchAction::ResumeReview),
        provider: "opencode".to_owned(),
        continuity_quality: ContinuityQuality::Usable,
        stage_summary: "awaiting explicit return".to_owned(),
        unresolved_finding_ids: vec!["finding-1".to_owned()],
        outbound_draft_ids: vec![],
        attention_summary: "awaiting_return".to_owned(),
        artifact_refs: vec!["artifact-dropout".to_owned()],
    }
}

fn run_rr(args: &[&str], runtime: &CliRuntime) -> roger_cli::CliRunResult {
    let argv = args
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    run(&argv, runtime)
}

fn run_rr_process(args: &[&str], runtime: &CliRuntime) -> std::process::Output {
    if let Ok(rr_bin) = std::env::var("CARGO_BIN_EXE_rr") {
        return Command::new(rr_bin)
            .args(args)
            .current_dir(&runtime.cwd)
            .env("RR_STORE_ROOT", &runtime.store_root)
            .env("RR_OPENCODE_BIN", &runtime.opencode_bin)
            .output()
            .expect("run rr process via CARGO_BIN_EXE_rr");
    }

    let workspace = workspace_root();
    let local_rr = workspace.join("target/debug/rr");
    if local_rr.exists() {
        return Command::new(local_rr)
            .args(args)
            .current_dir(&runtime.cwd)
            .env("RR_STORE_ROOT", &runtime.store_root)
            .env("RR_OPENCODE_BIN", &runtime.opencode_bin)
            .output()
            .expect("run rr process via target/debug/rr");
    }

    let mut cmd = Command::new("cargo");
    cmd.arg("run")
        .arg("-q")
        .arg("-p")
        .arg("roger-cli")
        .arg("--bin")
        .arg("rr")
        .arg("--");
    cmd.args(args)
        .current_dir(workspace)
        .env("RR_STORE_ROOT", &runtime.store_root)
        .env("RR_OPENCODE_BIN", &runtime.opencode_bin);
    cmd.output().expect("run rr process via cargo run fallback")
}

fn run_rr_process_with_stdin(
    args: &[&str],
    runtime: &CliRuntime,
    stdin_bytes: &[u8],
) -> std::process::Output {
    let mut cmd = if let Ok(rr_bin) = std::env::var("CARGO_BIN_EXE_rr") {
        let mut cmd = Command::new(rr_bin);
        cmd.current_dir(&runtime.cwd)
            .env("RR_STORE_ROOT", &runtime.store_root)
            .env("RR_OPENCODE_BIN", &runtime.opencode_bin);
        cmd
    } else {
        let workspace = workspace_root();
        let local_rr = workspace.join("target/debug/rr");
        if local_rr.exists() {
            let mut cmd = Command::new(local_rr);
            cmd.current_dir(&runtime.cwd)
                .env("RR_STORE_ROOT", &runtime.store_root)
                .env("RR_OPENCODE_BIN", &runtime.opencode_bin);
            cmd
        } else {
            let mut cmd = Command::new("cargo");
            cmd.arg("run")
                .arg("-q")
                .arg("-p")
                .arg("roger-cli")
                .arg("--bin")
                .arg("rr")
                .arg("--")
                .current_dir(workspace)
                .env("RR_STORE_ROOT", &runtime.store_root)
                .env("RR_OPENCODE_BIN", &runtime.opencode_bin);
            cmd
        }
    };

    cmd.args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().expect("spawn rr process");
    {
        let stdin = child.stdin.as_mut().expect("child stdin");
        stdin
            .write_all(stdin_bytes)
            .expect("write native messaging request");
    }
    child
        .wait_with_output()
        .expect("wait for rr process output")
}

fn run_harness(
    command_id: RogerCommandId,
    provider: &str,
    runtime: &CliRuntime,
    pr: Option<u64>,
) -> roger_cli::CliRunResult {
    run_harness_command(
        &HarnessCommandInvocation {
            provider: provider.to_owned(),
            command_id,
            repo: None,
            pr,
            session_id: None,
            robot: true,
        },
        runtime,
    )
}

fn parse_robot_payload(stdout: &str) -> Value {
    serde_json::from_str(stdout).expect("robot payload json")
}

fn parse_toon_payload(stdout: &str) -> Value {
    toon_format::decode_default(stdout).expect("robot payload toon")
}

fn encode_native_intent(intent: &BridgeLaunchIntent) -> Vec<u8> {
    let json = serde_json::to_vec(intent).expect("serialize native intent");
    let len = json.len() as u32;
    let mut wire = Vec::with_capacity(4 + json.len());
    wire.extend_from_slice(&len.to_le_bytes());
    wire.extend_from_slice(&json);
    wire
}

fn decode_native_response(stdout: &[u8]) -> BridgeResponse {
    assert!(
        stdout.len() >= 4,
        "native host output missing length prefix: {} bytes",
        stdout.len()
    );
    let len = u32::from_le_bytes([stdout[0], stdout[1], stdout[2], stdout[3]]) as usize;
    assert_eq!(
        stdout.len(),
        4 + len,
        "native host output length prefix mismatch"
    );
    serde_json::from_slice(&stdout[4..]).expect("decode native host response payload")
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("packages parent")
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn read_workspace_file(path: &str) -> String {
    fs::read_to_string(workspace_root().join(path)).expect("read workspace file")
}

fn normalize_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn assert_normalized_contains(haystack: &str, needle: &str, label: &str) {
    let haystack = normalize_whitespace(haystack);
    let needle = normalize_whitespace(needle);
    assert!(
        haystack.contains(&needle),
        "{label} is missing expected provider-truth snippet: {needle}"
    );
}

fn extension_pack_test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn write_guided_profile_discovery_state(runtime: &CliRuntime, browser: &str, extension_id: &str) {
    let package_dir = workspace_root().join("target/bridge/extension/roger-extension-unpacked");
    let preferences_path = runtime
        .store_root
        .join("bridge/browser-profiles")
        .join(browser)
        .join("Default/Secure Preferences");
    fs::create_dir_all(
        preferences_path
            .parent()
            .expect("profile preferences parent directory"),
    )
    .expect("create profile preferences parent");
    let preferences = serde_json::json!({
        "extensions": {
            "settings": {
                extension_id: {
                    "path": package_dir.to_string_lossy().to_string()
                }
            }
        }
    });
    fs::write(
        &preferences_path,
        serde_json::to_vec_pretty(&preferences).expect("serialize preferences"),
    )
    .expect("write secure preferences");
}

fn register_extension_identity_via_bridge(runtime: &CliRuntime, browser: &str, extension_id: &str) {
    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _env_guard = ENV_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("env lock");

    let previous_store_root = std::env::var_os("RR_STORE_ROOT");
    // SAFETY: tests serialize RR_STORE_ROOT mutation via ENV_LOCK and restore it before return.
    unsafe {
        std::env::set_var("RR_STORE_ROOT", &runtime.store_root);
    }

    let response = handle_bridge_intent(
        &BridgeLaunchIntent {
            action: "register_extension_identity".to_owned(),
            owner: "roger".to_owned(),
            repo: "roger-reviewer".to_owned(),
            pr_number: 0,
            head_ref: None,
            instance: None,
            extension_id: Some(extension_id.to_owned()),
            browser: Some(browser.to_owned()),
        },
        &BridgePreflight {
            roger_binary_found: false,
            roger_data_dir_exists: false,
            gh_available: false,
        },
        Path::new("rr"),
    );

    match previous_store_root {
        Some(value) => {
            // SAFETY: tests serialize RR_STORE_ROOT mutation via ENV_LOCK and restore it before return.
            unsafe {
                std::env::set_var("RR_STORE_ROOT", value);
            }
        }
        None => {
            // SAFETY: tests serialize RR_STORE_ROOT mutation via ENV_LOCK and restore it before return.
            unsafe {
                std::env::remove_var("RR_STORE_ROOT");
            }
        }
    }

    assert!(
        response.ok,
        "bridge registration intent failed: {} / {:?}",
        response.message, response.guidance
    );
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

fn write_stub_binary(reopen_fails: bool) -> (TempDir, PathBuf) {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("opencode-stub");
    let script = if reopen_fails {
        r#"#!/bin/sh
if [ "$1" = "--session" ]; then
  exit 1
fi
if [ "$1" = "export" ]; then
  echo "{}"
  exit 0
fi
exit 0
"#
    } else {
        r#"#!/bin/sh
if [ "$1" = "--session" ]; then
  exit 0
fi
if [ "$1" = "export" ]; then
  echo "{}"
  exit 0
fi
exit 0
"#
    };

    fs::write(&path, script).expect("write stub binary");
    let mut perms = fs::metadata(&path).expect("metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).expect("chmod stub binary");
    (dir, path)
}

fn write_probe_binary() -> (TempDir, PathBuf, PathBuf) {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("opencode-probe");
    let marker = dir.path().join("invoked.log");
    let script = format!(
        r#"#!/bin/sh
echo "$@" >> "{marker}"
if [ "$1" = "export" ]; then
  echo "{{}}"
  exit 0
fi
exit 0
"#,
        marker = marker.to_string_lossy()
    );

    fs::write(&path, script).expect("write probe binary");
    let mut perms = fs::metadata(&path).expect("metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).expect("chmod probe binary");
    (dir, path, marker)
}

#[test]
fn help_forms_exit_cleanly_for_quickstart_probe() {
    let temp = tempdir().expect("tempdir");
    let runtime = CliRuntime {
        cwd: temp.path().to_path_buf(),
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };

    for args in [&["help"][..], &["--help"][..], &["-h"][..]] {
        let result = run_rr(args, &runtime);
        assert_eq!(
            result.exit_code, 0,
            "args={args:?} stderr={}",
            result.stderr
        );
        assert!(
            result.stdout.contains("Usage:"),
            "args={args:?} stdout={}",
            result.stdout
        );
        assert!(
            result.stderr.trim().is_empty(),
            "args={args:?} stderr={}",
            result.stderr
        );
    }
}

#[test]
fn rr_binary_accepts_native_host_registration_intents_via_stdio_envelope() {
    let temp = tempdir().expect("tempdir");
    let runtime = CliRuntime {
        cwd: temp.path().to_path_buf(),
        store_root: temp.path().join(".roger"),
        opencode_bin: "opencode".to_owned(),
    };

    let intent = BridgeLaunchIntent {
        action: "register_extension_identity".to_owned(),
        owner: "roger".to_owned(),
        repo: "roger-reviewer".to_owned(),
        pr_number: 0,
        head_ref: None,
        instance: None,
        extension_id: Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned()),
        browser: Some("chrome".to_owned()),
    };
    let output = run_rr_process_with_stdin(&[], &runtime, &encode_native_intent(&intent));

    assert!(
        output.status.success(),
        "native host registration should exit cleanly: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response = decode_native_response(&output.stdout);
    assert!(response.ok, "response should be ok: {:?}", response);
    assert_eq!(response.action, "register_extension_identity");
}

#[test]
fn rr_binary_native_host_path_returns_bridge_response_for_launch_intents() {
    let temp = tempdir().expect("tempdir");
    let runtime = CliRuntime {
        cwd: temp.path().to_path_buf(),
        store_root: temp.path().join(".roger-missing"),
        opencode_bin: "opencode".to_owned(),
    };

    let intent = BridgeLaunchIntent {
        action: "start_review".to_owned(),
        owner: "owner".to_owned(),
        repo: "repo".to_owned(),
        pr_number: 42,
        head_ref: None,
        instance: None,
        extension_id: None,
        browser: None,
    };
    let output = run_rr_process_with_stdin(&[], &runtime, &encode_native_intent(&intent));

    assert!(
        !output.status.success(),
        "preflight failure should fail closed with non-zero exit"
    );
    let response = decode_native_response(&output.stdout);
    assert!(!response.ok);
    assert_eq!(response.action, "start_review");
    assert!(
        response.message.contains("Roger is not ready"),
        "unexpected response message: {}",
        response.message
    );
}

#[test]
fn rr_binary_native_host_path_handles_all_primary_launch_actions_without_hanging() {
    let temp = tempdir().expect("tempdir");
    let runtime = CliRuntime {
        cwd: temp.path().to_path_buf(),
        store_root: temp.path().join(".roger-missing"),
        opencode_bin: "opencode".to_owned(),
    };

    for action in ["start_review", "resume_review", "show_findings"] {
        let intent = BridgeLaunchIntent {
            action: action.to_owned(),
            owner: "owner".to_owned(),
            repo: "repo".to_owned(),
            pr_number: 42,
            head_ref: None,
            instance: None,
            extension_id: None,
            browser: None,
        };
        let output = run_rr_process_with_stdin(&[], &runtime, &encode_native_intent(&intent));
        assert!(
            !output.stdout.is_empty(),
            "expected Native Messaging envelope output for action={action}"
        );
        let response = decode_native_response(&output.stdout);

        assert!(
            !output.status.success(),
            "preflight should fail closed for action={action}"
        );
        assert!(
            !response.ok,
            "response should fail closed for action={action}"
        );
        assert_eq!(response.action, action);
        assert!(
            response.message.contains("Roger is not ready"),
            "unexpected message for action={action}: {}",
            response.message
        );
        let guidance = response.guidance.as_deref().unwrap_or_default();
        assert!(
            guidance.contains("Run `rr init`") || guidance.contains("Run `rr extension setup`"),
            "expected setup guidance for action={action}: {:?}",
            response.guidance
        );
    }
}

fn seed_session_with_provider(
    runtime: &CliRuntime,
    provider: &str,
    pr_number: u64,
    session_id: &str,
) {
    let target = sample_target(pr_number);
    let store = RogerStore::open(&runtime.store_root).expect("open store");
    store
        .create_review_session(CreateReviewSession {
            id: session_id,
            review_target: &target,
            provider,
            session_locator: None,
            resume_bundle_artifact_id: None,
            continuity_state: "awaiting_user_input",
            attention_state: "awaiting_user_input",
            launch_profile_id: Some("profile-open-pr"),
        })
        .expect("create session");

    let binding_id = format!("binding-{session_id}");
    store
        .put_session_launch_binding(CreateSessionLaunchBinding {
            id: &binding_id,
            session_id,
            repo_locator: &target.repository,
            review_target: Some(&target),
            surface: LaunchSurface::Cli,
            launch_profile_id: Some("profile-open-pr"),
            ui_target: Some("cli"),
            instance_preference: Some("reuse_if_possible"),
            cwd: Some("/tmp/repo"),
            worktree_root: None,
        })
        .expect("create binding");
}

fn seed_session_for_finder(
    runtime: &CliRuntime,
    session_id: &str,
    repository: &str,
    pr_number: u64,
    attention_state: &str,
) {
    let target = ReviewTarget {
        repository: repository.to_owned(),
        pull_request_number: pr_number,
        base_ref: "main".to_owned(),
        head_ref: format!("feature-{pr_number}"),
        base_commit: "aaa".to_owned(),
        head_commit: "bbb".to_owned(),
    };
    let store = RogerStore::open(&runtime.store_root).expect("open store");
    store
        .create_review_session(CreateReviewSession {
            id: session_id,
            review_target: &target,
            provider: "opencode",
            session_locator: None,
            resume_bundle_artifact_id: None,
            continuity_state: "awaiting_resume",
            attention_state,
            launch_profile_id: Some("profile-open-pr"),
        })
        .expect("create session");
}

#[test]
fn shell_commands_work_without_extension_on_blessed_opencode_path() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let review = run_rr(&["review", "--pr", "42", "--robot"], &runtime);
    assert_eq!(review.exit_code, 0, "{}", review.stderr);

    let status = run_rr(&["status", "--robot"], &runtime);
    assert_eq!(status.exit_code, 0, "{}", status.stderr);
    let status_payload = parse_robot_payload(&status.stdout);
    assert_eq!(status_payload["outcome"], "complete");
    assert_eq!(status_payload["data"]["target"]["pull_request"], 42);

    let findings = run_rr(&["findings", "--robot"], &runtime);
    assert_eq!(findings.exit_code, 0, "{}", findings.stderr);
    let findings_payload = parse_robot_payload(&findings.stdout);
    assert!(findings_payload["outcome"] == "empty" || findings_payload["outcome"] == "complete");

    let resume = run_rr(&["resume", "--pr", "42", "--robot"], &runtime);
    assert_eq!(resume.exit_code, 0, "{}", resume.stderr);

    let ret = run_rr(&["return", "--pr", "42", "--robot"], &runtime);
    assert_eq!(ret.exit_code, 0, "{}", ret.stderr);
}

#[test]
fn review_records_verified_launch_attempt_for_robot_launch() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let review = run_rr(&["review", "--pr", "42", "--robot"], &runtime);
    assert_eq!(review.exit_code, 0, "{}", review.stderr);
    let payload = parse_robot_payload(&review.stdout);
    let attempt_id = payload["data"]["launch_attempt_id"]
        .as_str()
        .expect("launch attempt id");
    let session_id = payload["data"]["session_id"].as_str().expect("session id");

    let store = RogerStore::open(&runtime.store_root).expect("open store");
    let attempt = store
        .launch_attempt(attempt_id)
        .expect("read launch attempt")
        .expect("launch attempt record");
    assert_eq!(attempt.state, LaunchAttemptState::VerifiedStarted);
    assert_eq!(attempt.final_session_id.as_deref(), Some(session_id));
    assert_eq!(attempt.review_target.pull_request_number, 42);
    assert_eq!(attempt.provider, "opencode");
    assert_eq!(
        attempt
            .verified_locator
            .as_ref()
            .expect("verified locator")
            .session_id,
        attempt
            .provider_session_id
            .as_deref()
            .expect("provider session id")
    );
}

#[test]
fn repeated_review_reuses_resume_bundle_artifact_for_duplicate_digest() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let first = run_rr(&["review", "--pr", "42", "--robot"], &runtime);
    assert_eq!(first.exit_code, 0, "{}", first.stderr);
    let first_payload = parse_robot_payload(&first.stdout);
    let first_bundle_id = first_payload["data"]["resume_bundle_artifact_id"]
        .as_str()
        .expect("first bundle id");

    let second = run_rr(&["review", "--pr", "42", "--robot"], &runtime);
    assert_eq!(second.exit_code, 0, "{}", second.stderr);
    let second_payload = parse_robot_payload(&second.stdout);
    let second_bundle_id = second_payload["data"]["resume_bundle_artifact_id"]
        .as_str()
        .expect("second bundle id");

    assert_eq!(
        first_bundle_id, second_bundle_id,
        "duplicate resume-bundle payload digest should reuse the existing artifact id"
    );
    assert!(
        !second
            .stderr
            .contains("UNIQUE constraint failed: artifacts.digest"),
        "review path should avoid duplicate artifact digest failures: {}",
        second.stderr
    );
}

#[test]
fn separate_process_review_sequence_avoids_cross_process_artifact_id_collisions() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let review_42 = run_rr_process(
        &["review", "--repo", "owner/repo", "--pr", "42", "--robot"],
        &runtime,
    );
    assert_eq!(
        review_42.status.code(),
        Some(0),
        "review PR 42 failed: {}",
        String::from_utf8_lossy(&review_42.stderr)
    );
    let review_42_payload = parse_robot_payload(std::str::from_utf8(&review_42.stdout).unwrap());
    let bundle_42 = review_42_payload["data"]["resume_bundle_artifact_id"]
        .as_str()
        .expect("bundle id for PR 42")
        .to_owned();

    let review_43 = run_rr_process(
        &["review", "--repo", "owner/repo", "--pr", "43", "--robot"],
        &runtime,
    );
    assert_eq!(
        review_43.status.code(),
        Some(0),
        "review PR 43 failed: {}",
        String::from_utf8_lossy(&review_43.stderr)
    );
    let review_43_payload = parse_robot_payload(std::str::from_utf8(&review_43.stdout).unwrap());
    let bundle_43 = review_43_payload["data"]["resume_bundle_artifact_id"]
        .as_str()
        .expect("bundle id for PR 43")
        .to_owned();
    assert_ne!(
        bundle_42, bundle_43,
        "separate process invocations must not collide on bundle artifact id"
    );

    let review_codex = run_rr_process(
        &[
            "review",
            "--repo",
            "owner/repo",
            "--pr",
            "99",
            "--provider",
            "codex",
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(
        review_codex.status.code(),
        Some(5),
        "codex review should remain degraded tier-a, stderr: {}",
        String::from_utf8_lossy(&review_codex.stderr)
    );
    assert!(
        !String::from_utf8_lossy(&review_codex.stderr).contains("UNIQUE constraint failed"),
        "cross-process sequence must not fail with unique-constraint artifact collisions"
    );
    let codex_payload = parse_robot_payload(std::str::from_utf8(&review_codex.stdout).unwrap());
    assert_eq!(codex_payload["outcome"], "degraded");
    assert_eq!(codex_payload["data"]["provider"], "codex");
}

#[test]
fn status_repo_pr_resolution_matches_live_session_picker_and_explicit_session() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let review = run_rr(
        &["review", "--pr", "11", "--repo", "owner/repo", "--robot"],
        &runtime,
    );
    assert_eq!(review.exit_code, 0, "{}", review.stderr);
    let review_payload = parse_robot_payload(&review.stdout);
    let session_id = review_payload["data"]["session_id"]
        .as_str()
        .expect("review session id")
        .to_owned();

    let sessions = run_rr(
        &["sessions", "--repo", "owner/repo", "--pr", "11", "--robot"],
        &runtime,
    );
    assert_eq!(sessions.exit_code, 0, "{}", sessions.stderr);
    let sessions_payload = parse_robot_payload(&sessions.stdout);
    let listed_session = sessions_payload["data"]["items"]
        .as_array()
        .expect("sessions items")
        .first()
        .expect("single session entry");
    assert_eq!(
        listed_session["session_id"]
            .as_str()
            .expect("listed session id"),
        session_id
    );

    let resume = run_rr(
        &[
            "resume",
            "--repo",
            "owner/repo",
            "--pr",
            "11",
            "--dry-run",
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(resume.exit_code, 0, "{}", resume.stderr);
    let resume_payload = parse_robot_payload(&resume.stdout);
    assert_eq!(
        resume_payload["data"]["session_id"]
            .as_str()
            .expect("resume session id"),
        session_id
    );

    let status = run_rr(
        &["status", "--repo", "owner/repo", "--pr", "11", "--robot"],
        &runtime,
    );
    assert_eq!(status.exit_code, 0, "{}", status.stderr);
    let status_payload = parse_robot_payload(&status.stdout);
    assert_eq!(status_payload["outcome"], "complete");
    assert_eq!(
        status_payload["data"]["session"]["id"]
            .as_str()
            .expect("status session id"),
        session_id
    );

    let status_by_session = run(
        &[
            "status".to_owned(),
            "--session".to_owned(),
            session_id.clone(),
            "--robot".to_owned(),
        ],
        &runtime,
    );
    assert_eq!(
        status_by_session.exit_code, 0,
        "{}",
        status_by_session.stderr
    );
    let status_by_session_payload = parse_robot_payload(&status_by_session.stdout);
    assert_eq!(
        status_by_session_payload["data"]["session"]["id"]
            .as_str()
            .expect("explicit status session id"),
        session_id
    );
}

#[test]
fn resume_blocks_with_picker_when_repo_match_is_ambiguous() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    assert_eq!(
        run_rr(&["review", "--pr", "42", "--robot"], &runtime).exit_code,
        0
    );
    assert_eq!(
        run_rr(&["review", "--pr", "43", "--robot"], &runtime).exit_code,
        0
    );

    let resume = run_rr(&["resume", "--robot"], &runtime);
    assert_eq!(resume.exit_code, 3, "{}", resume.stderr);
    let payload = parse_robot_payload(&resume.stdout);
    assert_eq!(payload["outcome"], "blocked");
    assert!(
        payload["data"]["reason"]
            .as_str()
            .expect("reason")
            .contains("multiple repo-local sessions")
    );
    assert_eq!(
        payload["data"]["candidates"]
            .as_array()
            .expect("candidate list")
            .len(),
        2
    );
}

#[test]
fn return_with_explicit_session_bypasses_repo_ambiguity() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let review_42 = run_rr(&["review", "--pr", "42", "--robot"], &runtime);
    assert_eq!(review_42.exit_code, 0, "{}", review_42.stderr);
    let review_42_payload = parse_robot_payload(&review_42.stdout);
    let session_42 = review_42_payload["data"]["session_id"]
        .as_str()
        .expect("session id for pr 42")
        .to_owned();

    let review_43 = run_rr(&["review", "--pr", "43", "--robot"], &runtime);
    assert_eq!(review_43.exit_code, 0, "{}", review_43.stderr);

    let explicit_return = run(
        &[
            "return".to_owned(),
            "--session".to_owned(),
            session_42.clone(),
            "--robot".to_owned(),
        ],
        &runtime,
    );
    assert_eq!(explicit_return.exit_code, 0, "{}", explicit_return.stderr);
    let explicit_payload = parse_robot_payload(&explicit_return.stdout);
    assert_eq!(explicit_payload["outcome"], "complete");
    assert_eq!(explicit_payload["data"]["session_id"], session_42);
    assert_eq!(
        explicit_payload["data"]["return_path"],
        "rebound_existing_session"
    );

    let ambiguous_return = run_rr(&["return", "--robot"], &runtime);
    assert_eq!(ambiguous_return.exit_code, 3, "{}", ambiguous_return.stderr);
    let ambiguous_payload = parse_robot_payload(&ambiguous_return.stdout);
    assert_eq!(ambiguous_payload["outcome"], "blocked");
    assert!(
        ambiguous_payload["data"]["reason"]
            .as_str()
            .expect("blocked reason")
            .contains("multiple repo-local sessions"),
    );
}

#[test]
fn resume_dry_run_with_explicit_pr_no_match_fails_closed_without_cross_pr_candidates() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    seed_session_with_provider(&runtime, "opencode", 123, "session-opencode-123");

    let resume = run_rr(&["resume", "--pr", "2", "--dry-run", "--robot"], &runtime);
    assert_eq!(resume.exit_code, 3, "{}", resume.stderr);

    let payload = parse_robot_payload(&resume.stdout);
    assert_eq!(payload["outcome"], "blocked");
    assert!(
        payload["data"]["reason"]
            .as_str()
            .expect("blocked reason")
            .contains("no matching repo-local session found for pull request 2")
    );
    assert_eq!(
        payload["data"]["candidates"]
            .as_array()
            .expect("candidate list")
            .len(),
        0,
        "explicit PR no-match should not include cross-PR picker candidates"
    );
    assert!(
        payload["warnings"]
            .as_array()
            .expect("warnings")
            .iter()
            .any(|warning| warning
                .as_str()
                .expect("warning text")
                .contains("no matching session found")),
        "no-match path should emit truthful no-match warning"
    );
}

#[test]
fn resume_robot_mode_suppresses_stale_locator_reopen_attempts() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_ok_stub_dir, ok_bin) = write_stub_binary(false);
    let (_fail_stub_dir, fail_bin) = write_stub_binary(true);

    let stable_runtime = CliRuntime {
        cwd: repo.clone(),
        store_root: temp.path().join("roger-store"),
        opencode_bin: ok_bin.to_string_lossy().to_string(),
    };

    let review = run_rr(&["review", "--pr", "42", "--robot"], &stable_runtime);
    assert_eq!(review.exit_code, 0, "{}", review.stderr);

    let stale_runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: fail_bin.to_string_lossy().to_string(),
    };

    let resume = run_rr(&["resume", "--pr", "42", "--robot"], &stale_runtime);
    assert_eq!(resume.exit_code, 0, "{}", resume.stderr);

    let payload = parse_robot_payload(&resume.stdout);
    assert_eq!(payload["outcome"], "complete");
    assert_eq!(payload["data"]["mode"], "robot_non_interactive");
    assert_eq!(payload["data"]["launch_suppressed"], true);
    assert_eq!(
        payload["data"]["reason_code"],
        "interactive_launch_suppressed_for_robot_mode"
    );
    assert_eq!(payload["data"]["command"], "resume");
}

#[test]
fn robot_resume_does_not_launch_interactive_provider_paths() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_probe_dir, probe_bin, marker_path) = write_probe_binary();

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: probe_bin.to_string_lossy().to_string(),
    };
    seed_session_with_provider(&runtime, "opencode", 42, "session-opencode-robot-1");

    let resume = run_rr(
        &["resume", "--session", "session-opencode-robot-1", "--robot"],
        &runtime,
    );
    assert_eq!(resume.exit_code, 0, "{}", resume.stderr);
    let resume_payload = parse_robot_payload(&resume.stdout);
    assert_eq!(resume_payload["outcome"], "complete");
    assert_eq!(resume_payload["data"]["mode"], "robot_non_interactive");
    assert_eq!(resume_payload["data"]["launch_suppressed"], true);
    assert_eq!(
        resume_payload["data"]["reason_code"],
        "interactive_launch_suppressed_for_robot_mode"
    );
    assert_eq!(resume_payload["data"]["command"], "resume");

    assert!(
        !marker_path.exists(),
        "provider binary should not be invoked for robot resume"
    );
}

#[test]
fn review_blocks_truthfully_for_unsupported_provider() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let review = run_rr(
        &["review", "--pr", "42", "--provider", "pi-agent", "--robot"],
        &runtime,
    );
    assert_eq!(review.exit_code, 3, "{}", review.stderr);

    let payload = parse_robot_payload(&review.stdout);
    assert_eq!(payload["outcome"], "blocked");
    assert!(
        payload["data"]["supported_providers"]
            .as_array()
            .expect("supported list")
            .iter()
            .any(|p| p.as_str() == Some("opencode"))
    );
    assert_eq!(
        payload["data"]["planned_not_live_providers"],
        serde_json::json!(["copilot"])
    );
    assert_eq!(
        payload["data"]["not_supported_providers"],
        serde_json::json!(["pi-agent"])
    );
    let live_review_provider_support = payload["data"]["live_review_provider_support"]
        .as_array()
        .expect("live review provider support");
    let opencode = live_review_provider_support
        .iter()
        .find(|item| item["provider"] == "opencode")
        .expect("opencode provider support");
    assert_eq!(opencode["display_name"], "OpenCode");
    assert_eq!(opencode["supports"]["return"], true);
    let gemini = live_review_provider_support
        .iter()
        .find(|item| item["provider"] == "gemini")
        .expect("gemini provider support");
    assert_eq!(
        gemini["notes"],
        "bounded tier-a start/reseed/raw-capture path only; no locator reopen or rr return"
    );
}

#[test]
fn review_succeeds_with_degraded_outcome_for_claude_and_gemini() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    for provider in ["claude", "gemini"] {
        let review = run_rr(
            &["review", "--pr", "42", "--provider", provider, "--robot"],
            &runtime,
        );
        // Exits 5 for Degraded because Tier A providers (Claude/Gemini) are always degraded
        assert_eq!(
            review.exit_code, 5,
            "provider {} failed: {}",
            provider, review.stderr
        );

        let payload = parse_robot_payload(&review.stdout);
        assert_eq!(payload["outcome"], "degraded");
        assert_eq!(payload["data"]["provider"], provider);
        assert!(
            payload["warnings"]
                .as_array()
                .expect("warnings")
                .iter()
                .any(|warning| warning
                    .as_str()
                    .expect("warning string")
                    .contains("start/reseed/raw-capture only")
                    || warning
                        .as_str()
                        .expect("warning string")
                        .contains("start/reseed/raw-capture"))
        );
        assert!(
            payload["warnings"]
                .as_array()
                .expect("warnings")
                .iter()
                .any(|warning| warning
                    .as_str()
                    .expect("warning string")
                    .contains("does not support locator reopen or rr return"))
        );
    }
}

#[test]
fn codex_review_and_resume_are_truthful_tier_a_degraded_paths() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let review = run_rr(
        &["review", "--pr", "42", "--provider", "codex", "--robot"],
        &runtime,
    );
    assert_eq!(review.exit_code, 5, "{}", review.stderr);

    let review_payload = parse_robot_payload(&review.stdout);
    assert_eq!(review_payload["outcome"], "degraded");
    assert_eq!(review_payload["data"]["provider"], "codex");
    assert_eq!(review_payload["data"]["session_path"], "started_fresh");
    assert_eq!(review_payload["data"]["continuity_quality"], "degraded");
    assert!(
        review_payload["warnings"]
            .as_array()
            .expect("warnings")
            .iter()
            .any(|warning| warning.as_str().expect("warning text").contains("tier-a"))
    );

    let resume = run_rr(&["resume", "--pr", "42", "--robot"], &runtime);
    assert_eq!(resume.exit_code, 5, "{}", resume.stderr);

    let resume_payload = parse_robot_payload(&resume.stdout);
    assert_eq!(resume_payload["outcome"], "degraded");
    assert_eq!(resume_payload["data"]["provider"], "codex");
    assert_eq!(
        resume_payload["data"]["resume_path"],
        "reseeded_from_bundle"
    );
    assert_eq!(resume_payload["data"]["continuity_quality"], "degraded");
}

#[test]
fn bounded_provider_outputs_are_truthful_for_claude_and_gemini() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };
    for (provider, pr) in [("claude", "42"), ("gemini", "43")] {
        let session_id = format!("session-{provider}-1");
        seed_session_with_provider(
            &runtime,
            provider,
            pr.parse().expect("pr number"),
            &session_id,
        );

        let status = run_rr(&["status", "--pr", pr, "--robot"], &runtime);
        assert_eq!(status.exit_code, 0, "{provider}: {}", status.stderr);
        let status_payload = parse_robot_payload(&status.stdout);
        assert_eq!(status_payload["outcome"], "complete");
        assert_eq!(status_payload["data"]["session"]["provider"], provider);
        assert_eq!(
            status_payload["data"]["session"]["resume_mode"],
            "bounded_provider"
        );
        assert_eq!(status_payload["data"]["continuity"]["tier"], "tier_a");
        assert_eq!(
            status_payload["data"]["provider_capability"]["provider"],
            provider
        );
        assert_eq!(
            status_payload["data"]["provider_capability"]["status"],
            "bounded_live"
        );
        assert_eq!(
            status_payload["data"]["provider_capability"]["tier"],
            "tier_a"
        );
        assert_eq!(
            status_payload["data"]["provider_capability"]["supports"]["status"],
            serde_json::json!(true)
        );
        assert_eq!(
            status_payload["data"]["provider_capability"]["supports"]["findings"],
            serde_json::json!(true)
        );
        assert_eq!(
            status_payload["data"]["provider_capability"]["supports"]["resume_reopen"],
            serde_json::json!(false)
        );
        assert_eq!(
            status_payload["data"]["provider_capability"]["supports"]["return"],
            serde_json::json!(false)
        );
        assert!(
            status_payload["warnings"]
                .as_array()
                .expect("status warnings")
                .iter()
                .any(|warning| warning
                    .as_str()
                    .expect("warning string")
                    .contains("bounded support"))
        );

        let resume = run_rr(&["resume", "--pr", pr, "--robot"], &runtime);
        assert_eq!(resume.exit_code, 5, "{provider}: {}", resume.stderr);
        let resume_payload = parse_robot_payload(&resume.stdout);
        assert_eq!(resume_payload["outcome"], "degraded");
        assert_eq!(resume_payload["data"]["provider"], provider);
        assert_eq!(
            resume_payload["data"]["resume_path"],
            "reseeded_from_bundle"
        );
        assert_eq!(resume_payload["data"]["continuity_quality"], "degraded");
        assert_eq!(
            resume_payload["data"]["provider_capability"]["provider"],
            provider
        );
        assert_eq!(
            resume_payload["data"]["provider_capability"]["status"],
            "bounded_live"
        );
        assert_eq!(
            resume_payload["data"]["provider_capability"]["tier"],
            "tier_a"
        );
        assert_eq!(
            resume_payload["data"]["provider_capability"]["supports"]["resume_reseed"],
            serde_json::json!(true)
        );
        assert_eq!(
            resume_payload["data"]["provider_capability"]["supports"]["resume_reopen"],
            serde_json::json!(false)
        );
        assert_eq!(
            resume_payload["data"]["provider_capability"]["supports"]["return"],
            serde_json::json!(false)
        );
        assert!(
            resume_payload["warnings"]
                .as_array()
                .expect("resume warnings")
                .iter()
                .any(|warning| warning
                    .as_str()
                    .expect("warning string")
                    .contains("bounded support"))
        );

        let ret = run_rr(&["return", "--pr", pr, "--robot"], &runtime);
        assert_eq!(ret.exit_code, 3, "{provider}: {}", ret.stderr);
        let return_payload = parse_robot_payload(&ret.stdout);
        assert_eq!(return_payload["outcome"], "blocked");
        assert_eq!(return_payload["data"]["provider"], provider);
        assert_eq!(
            return_payload["data"]["provider_capability"]["provider"],
            provider
        );
        assert_eq!(
            return_payload["data"]["provider_capability"]["status"],
            "bounded_live"
        );
        assert_eq!(
            return_payload["data"]["provider_capability"]["tier"],
            "tier_a"
        );
        assert_eq!(
            return_payload["data"]["provider_capability"]["supports"]["return"],
            serde_json::json!(false)
        );
        assert_eq!(
            return_payload["data"]["provider_capability"]["required_tier_for_return"],
            "tier_b"
        );
    }
}

#[test]
fn status_and_findings_surface_outbound_approval_states_truthfully() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

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
    for (finding_id, fingerprint, title, outbound_state) in [
        (
            "finding-awaiting",
            "fp-awaiting",
            "Awaiting approval finding",
            "drafted",
        ),
        (
            "finding-approved",
            "fp-approved",
            "Approved draft finding",
            "approved",
        ),
        (
            "finding-invalidated",
            "fp-invalidated",
            "Invalidated draft finding",
            "drafted",
        ),
        (
            "finding-posted",
            "fp-posted",
            "Posted draft finding",
            "posted",
        ),
        (
            "finding-failed",
            "fp-failed",
            "Failed posting finding",
            "failed",
        ),
    ] {
        store
            .upsert_materialized_finding(CreateMaterializedFinding {
                id: finding_id,
                session_id: &session_id,
                review_run_id: &review_run_id,
                stage: "deep_review",
                fingerprint,
                title,
                normalized_summary: title,
                severity: "medium",
                confidence: "medium",
                triage_state: "accepted",
                outbound_state,
            })
            .expect("upsert materialized finding");
    }

    store
        .create_outbound_draft(roger_storage::CreateOutboundDraft {
            id: "legacy-awaiting",
            session_id: &session_id,
            finding_id: "finding-awaiting",
            target_locator: "github:owner/repo#42/files#thread-awaiting",
            payload_digest: "sha256:legacy-awaiting",
            body: "Awaiting approval body",
        })
        .expect("create legacy awaiting draft");

    let approved_batch = OutboundDraftBatch {
        id: "batch-approved".to_owned(),
        review_session_id: session_id.clone(),
        review_run_id: review_run_id.clone(),
        repo_id: "owner/repo".to_owned(),
        remote_review_target_id: "pr-42".to_owned(),
        payload_digest: "sha256:payload-approved".to_owned(),
        approval_state: ApprovalState::Approved,
        approved_at: Some(1_710_020_000),
        invalidated_at: None,
        invalidation_reason_code: None,
        row_version: 1,
    };
    store
        .store_outbound_draft_batch(&approved_batch)
        .expect("store approved batch");
    store
        .store_outbound_draft_item(&OutboundDraft {
            id: "draft-approved".to_owned(),
            review_session_id: session_id.clone(),
            review_run_id: review_run_id.clone(),
            finding_id: Some("finding-approved".to_owned()),
            draft_batch_id: approved_batch.id.clone(),
            repo_id: approved_batch.repo_id.clone(),
            remote_review_target_id: approved_batch.remote_review_target_id.clone(),
            payload_digest: approved_batch.payload_digest.clone(),
            approval_state: ApprovalState::Approved,
            anchor_digest: "anchor:approved".to_owned(),
            target_locator: "github:owner/repo#42/files#thread-approved".to_owned(),
            body: "Approved canonical outbound body".to_owned(),
            row_version: 1,
        })
        .expect("store approved draft item");
    store
        .store_outbound_approval_token(&OutboundApprovalToken {
            id: "approval-approved".to_owned(),
            draft_batch_id: approved_batch.id.clone(),
            payload_digest: approved_batch.payload_digest.clone(),
            target_tuple_json: outbound_target_tuple_json(&approved_batch),
            approved_at: 1_710_020_001,
            revoked_at: None,
        })
        .expect("store approved token");

    let invalidated_batch = OutboundDraftBatch {
        id: "batch-invalidated".to_owned(),
        review_session_id: session_id.clone(),
        review_run_id: review_run_id.clone(),
        repo_id: "owner/repo".to_owned(),
        remote_review_target_id: "pr-42".to_owned(),
        payload_digest: "sha256:payload-invalidated".to_owned(),
        approval_state: ApprovalState::Invalidated,
        approved_at: Some(1_710_020_010),
        invalidated_at: Some(1_710_020_020),
        invalidation_reason_code: Some("target_rebased".to_owned()),
        row_version: 2,
    };
    store
        .store_outbound_draft_batch(&invalidated_batch)
        .expect("store invalidated batch");
    store
        .store_outbound_draft_item(&OutboundDraft {
            id: "draft-invalidated".to_owned(),
            review_session_id: session_id.clone(),
            review_run_id: review_run_id.clone(),
            finding_id: Some("finding-invalidated".to_owned()),
            draft_batch_id: invalidated_batch.id.clone(),
            repo_id: invalidated_batch.repo_id.clone(),
            remote_review_target_id: invalidated_batch.remote_review_target_id.clone(),
            payload_digest: invalidated_batch.payload_digest.clone(),
            approval_state: ApprovalState::Invalidated,
            anchor_digest: "anchor:invalidated".to_owned(),
            target_locator: "github:owner/repo#42/files#thread-invalidated".to_owned(),
            body: "Invalidated canonical outbound body".to_owned(),
            row_version: 2,
        })
        .expect("store invalidated draft item");

    store
        .create_outbound_draft(roger_storage::CreateOutboundDraft {
            id: "legacy-posted",
            session_id: &session_id,
            finding_id: "finding-posted",
            target_locator: "github:owner/repo#42/files#thread-posted",
            payload_digest: "sha256:legacy-posted",
            body: "Posted body",
        })
        .expect("create legacy posted draft");
    store
        .approve_outbound_draft(
            "legacy-approval-posted",
            "legacy-posted",
            "sha256:legacy-posted",
            "github:owner/repo#42/files#thread-posted",
        )
        .expect("approve legacy posted draft");
    store
        .record_posted_action(
            "legacy-posted-action",
            "legacy-posted",
            "github:owner/repo#42/files#thread-posted",
            "sha256:legacy-posted",
            "posted",
        )
        .expect("record legacy posted action");

    let failed_batch = OutboundDraftBatch {
        id: "batch-failed".to_owned(),
        review_session_id: session_id.clone(),
        review_run_id: review_run_id.clone(),
        repo_id: "owner/repo".to_owned(),
        remote_review_target_id: "pr-42".to_owned(),
        payload_digest: "sha256:payload-failed".to_owned(),
        approval_state: ApprovalState::Approved,
        approved_at: Some(1_710_020_030),
        invalidated_at: None,
        invalidation_reason_code: None,
        row_version: 1,
    };
    store
        .store_outbound_draft_batch(&failed_batch)
        .expect("store failed batch");
    store
        .store_outbound_draft_item(&OutboundDraft {
            id: "draft-failed".to_owned(),
            review_session_id: session_id.clone(),
            review_run_id: review_run_id.clone(),
            finding_id: Some("finding-failed".to_owned()),
            draft_batch_id: failed_batch.id.clone(),
            repo_id: failed_batch.repo_id.clone(),
            remote_review_target_id: failed_batch.remote_review_target_id.clone(),
            payload_digest: failed_batch.payload_digest.clone(),
            approval_state: ApprovalState::Approved,
            anchor_digest: "anchor:failed".to_owned(),
            target_locator: "github:owner/repo#42/files#thread-failed".to_owned(),
            body: "Failed canonical outbound body".to_owned(),
            row_version: 1,
        })
        .expect("store failed draft item");
    store
        .store_posted_batch_action(&PostedAction {
            id: "posted-failed".to_owned(),
            draft_batch_id: failed_batch.id.clone(),
            provider: "github".to_owned(),
            remote_identifier: "review-comment-failed".to_owned(),
            status: PostedActionStatus::Failed,
            posted_payload_digest: failed_batch.payload_digest.clone(),
            posted_at: 1_710_020_040,
            failure_code: Some("github_write_denied".to_owned()),
        })
        .expect("store failed posted action");

    let status = run_rr(&["status", "--session", &session_id, "--robot"], &runtime);
    assert_eq!(status.exit_code, 0, "{}", status.stderr);
    let status_payload = parse_robot_payload(&status.stdout);
    assert_eq!(
        status_payload["data"]["outbound"]["state_counts"]["awaiting_approval"],
        1
    );
    assert_eq!(
        status_payload["data"]["outbound"]["state_counts"]["approved"],
        1
    );
    assert_eq!(
        status_payload["data"]["outbound"]["state_counts"]["invalidated"],
        1
    );
    assert_eq!(
        status_payload["data"]["outbound"]["state_counts"]["posted"],
        1
    );
    assert_eq!(
        status_payload["data"]["outbound"]["state_counts"]["failed"],
        1
    );
    assert_eq!(
        status_payload["data"]["outbound"]["posting_gate"]["ready_count"],
        1
    );
    assert_eq!(
        status_payload["data"]["outbound"]["posting_gate"]["visibly_elevated"],
        serde_json::json!(true)
    );

    let findings = run_rr(&["findings", "--session", &session_id, "--robot"], &runtime);
    assert_eq!(findings.exit_code, 0, "{}", findings.stderr);
    let findings_payload = parse_robot_payload(&findings.stdout);
    let items = findings_payload["data"]["items"]
        .as_array()
        .expect("findings items");
    let indexed = items
        .iter()
        .map(|item| {
            (
                item["finding_id"].as_str().expect("finding id").to_owned(),
                item,
            )
        })
        .collect::<HashMap<_, _>>();

    assert_eq!(
        indexed["finding-awaiting"]["outbound_state"],
        "awaiting_approval"
    );
    assert_eq!(
        indexed["finding-awaiting"]["outbound_detail"]["source"],
        "legacy_draft"
    );
    assert_eq!(indexed["finding-approved"]["outbound_state"], "approved");
    assert_eq!(
        indexed["finding-approved"]["outbound_detail"]["mutation_elevated"],
        serde_json::json!(true)
    );
    assert_eq!(
        indexed["finding-invalidated"]["outbound_state"],
        "invalidated"
    );
    assert_eq!(
        indexed["finding-invalidated"]["outbound_detail"]["invalidation_reason_code"],
        "target_rebased"
    );
    assert_eq!(indexed["finding-posted"]["outbound_state"], "posted");
    assert_eq!(
        indexed["finding-posted"]["outbound_detail"]["source"],
        "legacy_draft"
    );
    assert_eq!(indexed["finding-failed"]["outbound_state"], "failed");
    assert_eq!(
        indexed["finding-failed"]["outbound_detail"]["posted_action_status"],
        "Failed"
    );
}

#[test]
fn return_reports_truthful_rebind_path_after_dropout_style_state() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let target = sample_target(42);
    let adapter = OpenCodeAdapter::with_binary(runtime.opencode_bin.clone());
    let locator = adapter
        .start_session(&target, &sample_launch_intent(LaunchAction::StartReview))
        .expect("start locator");

    let store = RogerStore::open(&runtime.store_root).expect("open store");
    store
        .store_resume_bundle("bundle-dropout-1", &dropout_bundle(target.clone()))
        .expect("store bundle");
    store
        .create_review_session(CreateReviewSession {
            id: "session-dropout-1",
            review_target: &target,
            provider: "opencode",
            session_locator: Some(&locator),
            resume_bundle_artifact_id: Some("bundle-dropout-1"),
            continuity_state: "awaiting_return",
            attention_state: "awaiting_return",
            launch_profile_id: Some("profile-open-pr"),
        })
        .expect("create session");
    store
        .put_session_launch_binding(CreateSessionLaunchBinding {
            id: "binding-dropout-1",
            session_id: "session-dropout-1",
            repo_locator: &target.repository,
            review_target: Some(&target),
            surface: LaunchSurface::Cli,
            launch_profile_id: Some("profile-open-pr"),
            ui_target: Some("cli"),
            instance_preference: Some("reuse_if_possible"),
            cwd: Some("/tmp/repo"),
            worktree_root: None,
        })
        .expect("create binding");

    let ret = run_rr(&["return", "--pr", "42", "--robot"], &runtime);
    assert_eq!(ret.exit_code, 0, "{}", ret.stderr);

    let payload = parse_robot_payload(&ret.stdout);
    assert_eq!(payload["outcome"], "complete");
    assert_eq!(payload["data"]["return_path"], "rebound_existing_session");
}

#[test]
fn harness_status_routes_to_same_core_cli_operation() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let review = run_rr(&["review", "--pr", "42", "--robot"], &runtime);
    assert_eq!(review.exit_code, 0, "{}", review.stderr);

    let harness_status = run_harness(RogerCommandId::RogerStatus, "opencode", &runtime, Some(42));
    assert_eq!(harness_status.exit_code, 0, "{}", harness_status.stderr);

    let payload = parse_robot_payload(&harness_status.stdout);
    assert_eq!(payload["schema_id"], "rr.robot.status.v1");
    assert_eq!(payload["outcome"], "complete");
    assert_eq!(payload["data"]["target"]["pull_request"], 42);
}

#[test]
fn harness_command_falls_back_truthfully_when_provider_binding_is_absent() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let result = run_harness(RogerCommandId::RogerReturn, "gemini", &runtime, Some(42));
    assert_eq!(result.exit_code, 3, "{}", result.stderr);

    let payload = parse_robot_payload(&result.stdout);
    assert_eq!(payload["schema_id"], "rr.robot.harness_command.v1");
    assert_eq!(payload["outcome"], "blocked");
    assert_eq!(payload["data"]["command_id"], "roger-return");
    assert_eq!(payload["data"]["fallback_cli_command"], "rr return");
    assert!(
        payload["repair_actions"]
            .as_array()
            .expect("repair actions")
            .iter()
            .any(|action| action
                .as_str()
                .expect("repair action")
                .contains("rr return"))
    );
}

#[test]
fn harness_return_stale_locator_matches_cli_degraded_semantics() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_ok_stub_dir, ok_bin) = write_stub_binary(false);
    let (_fail_stub_dir, fail_bin) = write_stub_binary(true);

    let stable_runtime = CliRuntime {
        cwd: repo.clone(),
        store_root: temp.path().join("roger-store"),
        opencode_bin: ok_bin.to_string_lossy().to_string(),
    };

    let review = run_rr(&["review", "--pr", "42", "--robot"], &stable_runtime);
    assert_eq!(review.exit_code, 0, "{}", review.stderr);

    let stale_runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: fail_bin.to_string_lossy().to_string(),
    };

    let harness_return = run_harness(
        RogerCommandId::RogerReturn,
        "opencode",
        &stale_runtime,
        Some(42),
    );
    assert_eq!(harness_return.exit_code, 5, "{}", harness_return.stderr);

    let payload = parse_robot_payload(&harness_return.stdout);
    assert_eq!(payload["schema_id"], "rr.robot.return.v1");
    assert_eq!(payload["outcome"], "degraded");
    assert_eq!(payload["data"]["return_path"], "reseeded_session");
}

#[test]
fn sessions_lists_filters_and_compacts_with_explicit_follow_on_hints() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    seed_session_for_finder(
        &runtime,
        "session-a",
        "owner/repo-a",
        101,
        "awaiting_user_input",
    );
    seed_session_for_finder(
        &runtime,
        "session-b",
        "owner/repo-b",
        202,
        "review_launched",
    );
    seed_session_for_finder(
        &runtime,
        "session-c",
        "owner/repo-c",
        303,
        "awaiting_user_input",
    );

    let sessions = run_rr(&["sessions", "--robot"], &runtime);
    assert_eq!(sessions.exit_code, 0, "{}", sessions.stderr);
    let payload = parse_robot_payload(&sessions.stdout);
    assert_eq!(payload["schema_id"], "rr.robot.sessions.v1");
    assert_eq!(payload["outcome"], "complete");
    assert_eq!(payload["data"]["count"], 3);
    assert_eq!(payload["data"]["truncated"], false);
    let items = payload["data"]["items"].as_array().expect("session items");
    assert_eq!(items.len(), 3);
    for item in items {
        assert!(item.get("session_id").is_some());
        assert!(item.get("repo").is_some());
        assert!(item["target"].get("repository").is_some());
        assert!(item["target"].get("pull_request").is_some());
        assert!(item.get("attention_state").is_some());
        assert_eq!(item["provider_capability"]["provider"], "opencode");
        assert_eq!(item["provider_capability"]["status"], "first_class_live");
        assert_eq!(item["provider_capability"]["tier"], "tier_b");
        assert_eq!(
            item["provider_capability"]["supports"]["sessions"],
            serde_json::json!(true)
        );
        assert_eq!(
            item["provider_capability"]["supports"]["resume_reopen"],
            serde_json::json!(true)
        );
        assert!(item.get("updated_at").is_some());
        assert_eq!(
            item["follow_on"]["requires_explicit_session"].as_bool(),
            Some(true)
        );
        assert!(
            item["follow_on"]["resume_command"]
                .as_str()
                .expect("resume command")
                .contains("--session ")
        );
    }

    let compact_filtered = run_rr(
        &[
            "sessions",
            "--attention",
            "awaiting_user_input",
            "--limit",
            "1",
            "--robot",
            "--robot-format",
            "compact",
        ],
        &runtime,
    );
    assert_eq!(compact_filtered.exit_code, 0, "{}", compact_filtered.stderr);
    let compact_payload = parse_robot_payload(&compact_filtered.stdout);
    assert_eq!(compact_payload["schema_id"], "rr.robot.sessions.v1");
    assert_eq!(compact_payload["robot_format"], "compact");
    assert_eq!(compact_payload["data"]["count"], 1);
    assert_eq!(compact_payload["data"]["truncated"], true);
    assert_eq!(
        compact_payload["data"]["items"]
            .as_array()
            .expect("compact items")
            .len(),
        1
    );
    assert!(
        compact_payload["data"]["items"][0]
            .get("session_id")
            .is_some()
    );
    assert!(compact_payload["data"]["items"][0].get("repo").is_some());
    assert!(
        compact_payload["data"]["items"][0]
            .get("pull_request")
            .is_some()
    );
    assert_eq!(
        compact_payload["data"]["items"][0]["attention_state"],
        "awaiting_user_input"
    );
}

#[test]
fn search_reports_truthful_degraded_mode_and_stable_robot_fields() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let search = run_rr(&["search", "--query", "stale draft", "--robot"], &runtime);
    assert_eq!(search.exit_code, 5, "{}", search.stderr);
    let payload = parse_robot_payload(&search.stdout);
    assert_eq!(payload["schema_id"], "rr.robot.search.v1");
    assert_eq!(payload["outcome"], "degraded");
    assert_eq!(payload["data"]["query"], "stale draft");
    assert_eq!(payload["data"]["requested_query_mode"], "auto");
    assert_eq!(payload["data"]["resolved_query_mode"], "recall");
    assert_eq!(payload["data"]["retrieval_mode"], "recovery_scan");
    assert_eq!(payload["data"]["mode"], "recovery_scan");
    assert_eq!(payload["data"]["candidate_included"], false);
    assert_eq!(
        payload["data"]["search_plan"]["query_plan"]["strategy"]["primary_lane"],
        "lexical_recall"
    );
    assert!(payload["data"]["items"].is_array());
    assert!(payload["data"]["count"].is_number());
    assert!(payload["data"]["truncated"].is_boolean());
    assert_eq!(
        payload["data"]["search_plan"]["scope_keys"],
        json!(["repo:owner/repo"])
    );
    assert_eq!(
        payload["data"]["search_plan"]["retrieval_classes"],
        json!(["promoted_memory", "evidence_hits"])
    );
    assert_eq!(
        payload["data"]["search_plan"]["semantic_runtime_posture"],
        "disabled_pending_verification"
    );
    assert_eq!(
        payload["data"]["search_plan"]["retrieval_strategy"]["semantic"],
        false
    );
    assert!(
        payload["data"]["degraded_reasons"]
            .as_array()
            .expect("degraded reasons")
            .iter()
            .any(|reason| reason
                .as_str()
                .expect("degraded reason")
                .contains("lexical"))
    );

    let compact = run_rr(
        &[
            "search",
            "--query",
            "stale draft",
            "--robot",
            "--robot-format",
            "compact",
        ],
        &runtime,
    );
    assert_eq!(compact.exit_code, 5, "{}", compact.stderr);
    let compact_payload = parse_robot_payload(&compact.stdout);
    assert_eq!(compact_payload["schema_id"], "rr.robot.search.v1");
    assert_eq!(compact_payload["robot_format"], "compact");
    assert_eq!(compact_payload["data"]["requested_query_mode"], "auto");
    assert_eq!(compact_payload["data"]["resolved_query_mode"], "recall");
    assert_eq!(compact_payload["data"]["retrieval_mode"], "recovery_scan");
    assert_eq!(
        compact_payload["data"]["search_plan"]["retrieval_classes"],
        json!(["promoted_memory", "evidence_hits"])
    );
    assert!(compact_payload["data"]["items"].is_array());
}

#[test]
fn search_resolves_auto_to_exact_lookup_and_blocks_anchor_free_related_context() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let exact = run_rr(
        &["search", "--query", "packages/cli/src/lib.rs", "--robot"],
        &runtime,
    );
    assert_eq!(exact.exit_code, 5, "{}", exact.stderr);
    let exact_payload = parse_robot_payload(&exact.stdout);
    assert_eq!(exact_payload["outcome"], "degraded");
    assert_eq!(exact_payload["data"]["requested_query_mode"], "auto");
    assert_eq!(exact_payload["data"]["resolved_query_mode"], "exact_lookup");
    assert_eq!(exact_payload["data"]["retrieval_mode"], "recovery_scan");

    let blocked = run_rr(
        &[
            "search",
            "--query",
            "stale draft",
            "--query-mode",
            "related_context",
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(blocked.exit_code, 3, "{}", blocked.stderr);
    let blocked_payload = parse_robot_payload(&blocked.stdout);
    assert_eq!(blocked_payload["outcome"], "blocked");
    assert_eq!(
        blocked_payload["data"]["reason_code"],
        "query_mode_requires_anchor_hints"
    );
    assert_eq!(
        blocked_payload["data"]["requested_query_mode"],
        "related_context"
    );
}

#[test]
fn search_projects_canonical_recall_truth_for_seeded_hits() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let target = sample_target(42);
    let store = RogerStore::open(&runtime.store_root).expect("open store");
    store
        .create_review_session(CreateReviewSession {
            id: "session-search-1",
            review_target: &target,
            provider: "opencode",
            session_locator: None,
            resume_bundle_artifact_id: None,
            continuity_state: "awaiting_resume",
            attention_state: "awaiting_user_input",
            launch_profile_id: Some("profile-open-pr"),
        })
        .expect("create review session");
    store
        .create_review_run(CreateReviewRun {
            id: "run-search-1",
            session_id: "session-search-1",
            run_kind: "deep_review",
            repo_snapshot: "{\"head\":\"bbb\"}",
            continuity_quality: "usable",
            session_locator_artifact_id: None,
        })
        .expect("create review run");
    seed_prior_review_lookup_records(&store, "session-search-1", "run-search-1", "owner/repo");

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
    let payload = parse_robot_payload(&search.stdout);
    assert_eq!(payload["schema_id"], "rr.robot.search.v1");
    assert_eq!(payload["outcome"], "degraded");
    assert_eq!(payload["data"]["requested_query_mode"], "candidate_audit");
    assert_eq!(payload["data"]["resolved_query_mode"], "candidate_audit");
    assert_eq!(payload["data"]["retrieval_mode"], "recovery_scan");
    assert_eq!(payload["data"]["candidate_included"], true);
    assert_eq!(
        payload["data"]["search_plan"]["retrieval_classes"],
        json!(["promoted_memory", "tentative_candidates", "evidence_hits"])
    );
    assert_eq!(
        payload["data"]["search_plan"]["retrieval_strategy"]["candidate_audit"],
        true
    );
    assert_eq!(
        payload["data"]["search_plan"]["semantic_runtime_posture"],
        "disabled_by_query_mode"
    );

    let items = payload["data"]["items"].as_array().expect("search items");
    let scope_bucket = payload["data"]["scope_bucket"].clone();
    let promoted = items
        .iter()
        .find(|item| item["kind"] == "promoted_memory")
        .expect("promoted memory item");
    assert_eq!(promoted["memory_lane"], "promoted_memory");
    assert_eq!(promoted["scope_bucket"], scope_bucket);
    assert_eq!(promoted["citation_posture"], "cite_allowed");
    assert_eq!(promoted["surface_posture"], "ordinary");
    assert_eq!(promoted["locator"]["state"], "proven");
    assert!(
        promoted["explain_summary"]
            .as_str()
            .expect("promoted explain summary")
            .contains("retrieval_mode recovery_scan")
    );

    let candidate = items
        .iter()
        .find(|item| item["kind"] == "candidate_memory")
        .expect("candidate memory item");
    assert_eq!(candidate["memory_lane"], "tentative_candidates");
    assert_eq!(candidate["scope_bucket"], scope_bucket);
    assert_eq!(candidate["citation_posture"], "inspect_only");
    assert_eq!(candidate["surface_posture"], "candidate_review");
    assert_eq!(candidate["locator"]["state"], "candidate");
    assert!(
        candidate["explain_summary"]
            .as_str()
            .expect("candidate explain summary")
            .contains("candidate_review")
    );

    let evidence = items
        .iter()
        .find(|item| item["kind"] == "evidence_finding")
        .expect("evidence finding item");
    assert_eq!(evidence["memory_lane"], "evidence_hits");
    assert_eq!(evidence["scope_bucket"], scope_bucket);
    assert_eq!(evidence["citation_posture"], "cite_allowed");
    assert_eq!(evidence["surface_posture"], "ordinary");
    assert_eq!(evidence["locator"]["repository"], "owner/repo");
    assert!(payload["data"]["lane_counts"]["tentative_candidates"].as_u64() >= Some(1));
}

#[test]
fn search_help_mentions_explicit_planner_modes() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let help = run_rr(&["--help"], &runtime);
    assert_eq!(help.exit_code, 0, "{}", help.stderr);
    assert!(
        help.stdout.contains(
            "rr search --query <text> [--query-mode auto|exact_lookup|recall|related_context|candidate_audit|promotion_review]"
        ),
        "{}",
        help.stdout
    );
}
#[test]
fn robot_docs_surfaces_schema_inventory_and_blocks_unknown_topics() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let schemas = run_rr(&["robot-docs", "schemas", "--robot"], &runtime);
    assert_eq!(schemas.exit_code, 0, "{}", schemas.stderr);
    let payload = parse_robot_payload(&schemas.stdout);
    assert_eq!(payload["schema_id"], "rr.robot.robot_docs.v1");
    assert_eq!(payload["outcome"], "complete");
    assert_eq!(payload["data"]["topic"], "schemas");
    assert_eq!(payload["data"]["version"], "0.1.0");
    let items = payload["data"]["items"].as_array().expect("schema items");
    assert!(
        items
            .iter()
            .any(|item| item["schema_id"] == "rr.robot.sessions.v1")
    );
    assert!(
        items
            .iter()
            .any(|item| item["schema_id"] == "rr.robot.search.v1")
    );
    assert!(
        items
            .iter()
            .any(|item| item["schema_id"] == "rr.robot.update.v1")
    );

    let compact = run_rr(
        &[
            "robot-docs",
            "commands",
            "--robot",
            "--robot-format",
            "compact",
        ],
        &runtime,
    );
    assert_eq!(compact.exit_code, 0, "{}", compact.stderr);
    let compact_payload = parse_robot_payload(&compact.stdout);
    assert_eq!(compact_payload["schema_id"], "rr.robot.robot_docs.v1");
    assert_eq!(compact_payload["robot_format"], "compact");
    assert_eq!(compact_payload["data"]["topic"], "commands");
    let command_items = compact_payload["data"]["items"]
        .as_array()
        .expect("command items");
    assert!(
        command_items
            .iter()
            .any(|item| item["command"] == "rr update")
    );
    let review_dry_run = command_items
        .iter()
        .find(|item| item["command"] == "rr review --dry-run")
        .expect("review dry-run command item");
    assert_eq!(
        review_dry_run["supported_providers"],
        serde_json::json!(["opencode", "codex", "gemini", "claude"])
    );
    assert_eq!(
        review_dry_run["planned_not_live_providers"],
        serde_json::json!(["copilot"])
    );
    assert_eq!(
        review_dry_run["not_supported_providers"],
        serde_json::json!(["pi-agent"])
    );

    let guide = run_rr(&["robot-docs", "guide", "--robot"], &runtime);
    assert_eq!(guide.exit_code, 0, "{}", guide.stderr);
    let guide_payload = parse_robot_payload(&guide.stdout);
    assert_eq!(guide_payload["data"]["topic"], "guide");
    let guide_items = guide_payload["data"]["items"]
        .as_array()
        .expect("guide items");
    let provider_support = guide_items
        .iter()
        .find(|item| item["kind"] == "provider_support")
        .expect("provider support guide item");
    assert_eq!(
        provider_support["planned_not_live_providers"],
        serde_json::json!(["copilot"])
    );
    assert_eq!(
        provider_support["not_supported_providers"],
        serde_json::json!(["pi-agent"])
    );
    let live_review_providers = provider_support["live_review_providers"]
        .as_array()
        .expect("live review providers");
    let opencode = live_review_providers
        .iter()
        .find(|item| item["provider"] == "opencode")
        .expect("opencode support entry");
    assert_eq!(opencode["display_name"], "OpenCode");
    assert_eq!(opencode["tier"], "tier_b");
    assert_eq!(opencode["supports"]["return"], true);
    let gemini = live_review_providers
        .iter()
        .find(|item| item["provider"] == "gemini")
        .expect("gemini support entry");
    assert_eq!(gemini["display_name"], "Gemini");
    assert_eq!(gemini["tier"], "tier_a");
    assert_eq!(gemini["supports"]["resume_reopen"], false);
    assert_eq!(gemini["supports"]["return"], false);
    assert_eq!(
        gemini["notes"],
        "bounded tier-a start/reseed/raw-capture path only; no locator reopen or rr return"
    );
    let inside_roger = guide_items
        .iter()
        .find(|item| item["context"] == "inside_roger")
        .expect("inside Roger guide item");
    assert_eq!(
        inside_roger["skill_path"],
        ".claude/skills/roger-inside-roger-agent/SKILL.md"
    );
    let example_commands = inside_roger["example"]["commands"]
        .as_array()
        .expect("example commands");
    assert!(
        example_commands
            .iter()
            .any(|command| command == "roger-status")
    );

    let workflows = run_rr(&["robot-docs", "workflows", "--robot"], &runtime);
    assert_eq!(workflows.exit_code, 0, "{}", workflows.stderr);
    let workflows_payload = parse_robot_payload(&workflows.stdout);
    assert_eq!(workflows_payload["data"]["topic"], "workflows");
    let workflow_items = workflows_payload["data"]["items"]
        .as_array()
        .expect("workflow items");
    let inside_roger_workflow = workflow_items
        .iter()
        .find(|item| item["name"] == "inside_roger_safe_subset")
        .expect("inside Roger workflow");
    assert_eq!(
        inside_roger_workflow["skill_path"],
        ".claude/skills/roger-inside-roger-agent/SKILL.md"
    );
    let steps = inside_roger_workflow["steps"]
        .as_array()
        .expect("workflow steps");
    assert!(steps.iter().any(|step| step == "roger-return"));

    let blocked = run_rr(&["robot-docs", "unknown-topic", "--robot"], &runtime);
    assert_eq!(blocked.exit_code, 3, "{}", blocked.stderr);
    let blocked_payload = parse_robot_payload(&blocked.stdout);
    assert_eq!(blocked_payload["schema_id"], "rr.robot.robot_docs.v1");
    assert_eq!(blocked_payload["outcome"], "blocked");
    assert_eq!(
        blocked_payload["data"]["reason_code"],
        "unknown_robot_docs_topic"
    );
}

#[test]
fn provider_support_claim_guard_keeps_help_robot_and_docs_in_lockstep() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let help = run_rr(&["--help"], &runtime);
    assert_eq!(help.exit_code, 0, "{}", help.stderr);

    let blocked_review = run_rr(
        &["review", "--pr", "42", "--provider", "pi-agent", "--robot"],
        &runtime,
    );
    assert_eq!(blocked_review.exit_code, 3, "{}", blocked_review.stderr);
    let blocked_payload = parse_robot_payload(&blocked_review.stdout);

    assert_eq!(
        blocked_payload["data"]["supported_providers"],
        serde_json::json!(["opencode", "codex", "gemini", "claude"])
    );
    assert_eq!(
        blocked_payload["data"]["planned_not_live_providers"],
        serde_json::json!(["copilot"])
    );
    assert_eq!(
        blocked_payload["data"]["not_supported_providers"],
        serde_json::json!(["pi-agent"])
    );

    let live_provider_support = blocked_payload["data"]["live_review_provider_support"]
        .as_array()
        .expect("live provider support");
    let opencode = live_provider_support
        .iter()
        .find(|item| item["provider"] == "opencode")
        .expect("opencode support entry");
    assert_eq!(opencode["display_name"], "OpenCode");
    assert_eq!(opencode["tier"], "tier_b");
    assert_eq!(opencode["status"], "first_class_live");
    assert_eq!(opencode["supports"]["resume_reopen"], true);
    assert_eq!(
        opencode["notes"],
        "first-class tier-b continuity path with locator reopen and rr return"
    );

    let codex = live_provider_support
        .iter()
        .find(|item| item["provider"] == "codex")
        .expect("codex support entry");
    assert_eq!(codex["tier"], "tier_a");
    assert_eq!(codex["status"], "bounded_live");
    assert_eq!(codex["supports"]["resume_reopen"], false);
    assert_eq!(codex["supports"]["return"], false);
    assert_eq!(
        codex["notes"],
        "bounded tier-a start/reseed/raw-capture path only; no locator reopen or rr return"
    );

    let commands = run_rr(&["robot-docs", "commands", "--robot"], &runtime);
    assert_eq!(commands.exit_code, 0, "{}", commands.stderr);
    let commands_payload = parse_robot_payload(&commands.stdout);
    let command_items = commands_payload["data"]["items"]
        .as_array()
        .expect("command items");
    let review_dry_run = command_items
        .iter()
        .find(|item| item["command"] == "rr review --dry-run")
        .expect("review dry-run command item");
    assert_eq!(
        review_dry_run["supported_providers"],
        blocked_payload["data"]["supported_providers"]
    );
    assert_eq!(
        review_dry_run["planned_not_live_providers"],
        blocked_payload["data"]["planned_not_live_providers"]
    );
    assert_eq!(
        review_dry_run["not_supported_providers"],
        blocked_payload["data"]["not_supported_providers"]
    );

    let guide = run_rr(&["robot-docs", "guide", "--robot"], &runtime);
    assert_eq!(guide.exit_code, 0, "{}", guide.stderr);
    let guide_payload = parse_robot_payload(&guide.stdout);
    let guide_items = guide_payload["data"]["items"]
        .as_array()
        .expect("guide items");
    let provider_support = guide_items
        .iter()
        .find(|item| item["kind"] == "provider_support")
        .expect("provider support guide item");
    assert_eq!(
        provider_support["live_review_providers"],
        blocked_payload["data"]["live_review_provider_support"]
    );
    assert_eq!(
        provider_support["planned_not_live_providers"],
        blocked_payload["data"]["planned_not_live_providers"]
    );
    assert_eq!(
        provider_support["not_supported_providers"],
        blocked_payload["data"]["not_supported_providers"]
    );

    assert_normalized_contains(
        &help.stdout,
        "opencode is the first-class tier-b continuity path; rr resume can reopen and rr return is supported",
        "rr --help",
    );
    assert_normalized_contains(
        &help.stdout,
        "codex, gemini, and claude are bounded tier-a providers; start/reseed/raw-capture only, no locator reopen or rr return",
        "rr --help",
    );
    assert_normalized_contains(
        &help.stdout,
        "copilot is planned but not yet a live --provider value",
        "rr --help",
    );
    assert_normalized_contains(
        &help.stdout,
        "pi-agent is not part of the 0.1.0 live CLI surface",
        "rr --help",
    );

    let readme = read_workspace_file("README.md");
    assert_normalized_contains(
        &readme,
        "`rr review --provider` currently supports `opencode`, `codex`, `gemini`, and `claude`.",
        "README.md",
    );
    assert_normalized_contains(
        &readme,
        "Codex, Gemini, and Claude Code are live only as bounded Tier A paths: Roger can start a review, reseed from a `ResumeBundle`, and preserve raw capture, but it does not claim locator reopen or `rr return` for those providers.",
        "README.md",
    );
    assert_normalized_contains(
        &readme,
        "GitHub Copilot CLI is still planned rather than live",
        "README.md",
    );

    let release_matrix = read_workspace_file("docs/RELEASE_AND_TEST_MATRIX.md");
    assert_normalized_contains(
        &release_matrix,
        "| GitHub Copilot CLI | Golden-path first-class provider target, not yet live | Do not claim live support until verified launch, policy, worker boundary, and continuity coverage are real |",
        "docs/RELEASE_AND_TEST_MATRIX.md",
    );
    assert_normalized_contains(
        &release_matrix,
        "| Codex | Secondary, bounded | Exposed via `rr review --provider codex`; truthful Tier A reseed/raw-capture path, no locator reopen or `rr return` claim |",
        "docs/RELEASE_AND_TEST_MATRIX.md",
    );
    assert_normalized_contains(
        &release_matrix,
        "| Gemini | Secondary, bounded | Exposed via `rr review --provider gemini`; truthful Tier A reseed/raw-capture path, no locator reopen or `rr return` claim |",
        "docs/RELEASE_AND_TEST_MATRIX.md",
    );
    assert_normalized_contains(
        &release_matrix,
        "| Claude Code | Secondary, bounded | Exposed via `rr review --provider claude`; truthful Tier A reseed/raw-capture path, no locator reopen or `rr return` claim |",
        "docs/RELEASE_AND_TEST_MATRIX.md",
    );
    assert_normalized_contains(
        &release_matrix,
        "| Pi-Agent | Not in `0.1.0` | Planning-only future harness candidate; no live support claim, no `rr review --provider pi-agent`, and no Tier A/Tier B language until a later admission spike proves direct-CLI launch, Roger-safe policy control, audit capture, and truthful continuity behavior |",
        "docs/RELEASE_AND_TEST_MATRIX.md",
    );

    let canonical_plan = read_workspace_file("docs/PLAN_FOR_ROGER_REVIEWER.md");
    assert_normalized_contains(
        &canonical_plan,
        "the authoritative provider support order is GitHub Copilot CLI, OpenCode, Codex, Gemini, then Claude Code",
        "docs/PLAN_FOR_ROGER_REVIEWER.md",
    );
    assert_normalized_contains(
        &canonical_plan,
        "Codex, Gemini, and Claude Code currently expose bounded Tier A paths in the live CLI",
        "docs/PLAN_FOR_ROGER_REVIEWER.md",
    );
    assert_normalized_contains(
        &canonical_plan,
        "CLI help, status output, and docs must describe only the provider surfaces and command paths that actually exist in the product",
        "docs/PLAN_FOR_ROGER_REVIEWER.md",
    );
}

#[test]
fn rr_agent_reads_bound_context_and_rejects_robot_flag() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let task = sample_worker_task();
    let context = sample_worker_context(&task);
    let task_path = temp.path().join("worker-task.json");
    let context_path = temp.path().join("worker-context.json");
    let request_path = temp.path().join("worker-context-request.json");
    write_json_fixture(&task_path, &task);
    write_json_fixture(&context_path, &context);
    seed_rr_agent_session(&runtime, &task);
    write_json_fixture(
        &request_path,
        &sample_worker_request(&task, "worker.get_review_context", None),
    );

    let result = run_rr(
        &[
            "agent",
            "worker.get_review_context",
            "--task-file",
            task_path.to_str().expect("task path"),
            "--context-file",
            context_path.to_str().expect("context path"),
            "--request-file",
            request_path.to_str().expect("request path"),
        ],
        &runtime,
    );
    assert_eq!(result.exit_code, 0, "{}", result.stderr);
    let payload = parse_robot_payload(&result.stdout);
    assert_eq!(payload["schema_id"], "rr.agent.response.v1");
    assert_eq!(payload["status"], "succeeded");
    assert_eq!(payload["transport_kind"], "agent_cli");
    let operation_response = &payload["operation_response"];
    assert_eq!(
        operation_response["schema_id"],
        "worker_operation_response.v1"
    );
    assert_eq!(
        operation_response["status"],
        json!(WorkerOperationResponseStatus::Succeeded)
    );
    assert_eq!(operation_response["operation"], "worker.get_review_context");
    assert_eq!(operation_response["authorization"]["lane"], "read");
    assert_eq!(operation_response["authorization"]["advisory_only"], false);
    assert_eq!(
        operation_response["payload"]["review_session_id"],
        task.review_session_id
    );
    assert_eq!(
        operation_response["payload"]["review_run_id"],
        task.review_run_id
    );
    assert_eq!(operation_response["payload"]["review_task_id"], task.id);
    assert_eq!(operation_response["payload"]["task_nonce"], task.task_nonce);
    assert!(result.stderr.trim().is_empty(), "{}", result.stderr);

    let blocked = run_rr(
        &[
            "agent",
            "worker.get_review_context",
            "--task-file",
            task_path.to_str().expect("task path"),
            "--context-file",
            context_path.to_str().expect("context path"),
            "--request-file",
            request_path.to_str().expect("request path"),
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(blocked.exit_code, 2);
    assert!(
        blocked
            .stderr
            .contains("rr agent is a separate transport from --robot; omit --robot"),
        "{}",
        blocked.stderr
    );
}

#[test]
fn rr_agent_surfaces_bound_status_findings_and_finding_detail() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let task = sample_worker_task();
    let task_path = temp.path().join("worker-task.json");
    write_json_fixture(&task_path, &task);
    seed_rr_agent_session(&runtime, &task);

    let store = RogerStore::open(&runtime.store_root).expect("open store");
    store
        .upsert_materialized_finding(CreateMaterializedFinding {
            id: "finding-rr-agent-1",
            session_id: &task.review_session_id,
            review_run_id: &task.review_run_id,
            stage: &task.stage,
            fingerprint: "fp-rr-agent-1",
            title: "Approval token survives stale refresh",
            normalized_summary: "Approval token survives stale refresh",
            severity: "high",
            confidence: "medium",
            triage_state: "accepted",
            outbound_state: "awaiting_approval",
        })
        .expect("seed materialized finding");

    let status_request_path = temp.path().join("worker-status-request.json");
    write_json_fixture(
        &status_request_path,
        &sample_worker_request(&task, "worker.get_status", None),
    );
    let status = run_rr(
        &[
            "agent",
            "worker.get_status",
            "--task-file",
            task_path.to_str().expect("task path"),
            "--request-file",
            status_request_path.to_str().expect("request path"),
        ],
        &runtime,
    );
    assert_eq!(status.exit_code, 0, "{}", status.stderr);
    let status_payload = parse_robot_payload(&status.stdout);
    assert_eq!(status_payload["schema_id"], "rr.agent.response.v1");
    assert_eq!(status_payload["status"], "succeeded");
    let status_operation = &status_payload["operation_response"];
    assert_eq!(
        status_operation["status"],
        json!(WorkerOperationResponseStatus::Succeeded)
    );
    assert_eq!(
        status_operation["payload"]["review_session_id"],
        task.review_session_id
    );
    assert_eq!(
        status_operation["payload"]["review_run_id"],
        task.review_run_id
    );
    assert_eq!(
        status_operation["payload"]["attention_state"],
        "awaiting_user_input"
    );
    assert_eq!(
        status_operation["payload"]["continuity_summary"],
        "resume:usable"
    );
    assert_eq!(status_operation["payload"]["unresolved_finding_count"], 1);
    assert_eq!(status_operation["payload"]["draft_count"], 0);

    let list_request_path = temp.path().join("worker-findings-request.json");
    write_json_fixture(
        &list_request_path,
        &sample_worker_request(&task, "worker.list_findings", None),
    );
    let list = run_rr(
        &[
            "agent",
            "worker.list_findings",
            "--task-file",
            task_path.to_str().expect("task path"),
            "--request-file",
            list_request_path.to_str().expect("request path"),
        ],
        &runtime,
    );
    assert_eq!(list.exit_code, 0, "{}", list.stderr);
    let list_payload = parse_robot_payload(&list.stdout);
    assert_eq!(list_payload["schema_id"], "rr.agent.response.v1");
    assert_eq!(list_payload["status"], "succeeded");
    let list_operation = &list_payload["operation_response"];
    assert_eq!(
        list_operation["status"],
        json!(WorkerOperationResponseStatus::Succeeded)
    );
    let items = list_operation["payload"]["items"]
        .as_array()
        .expect("finding items");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["finding_id"], "finding-rr-agent-1");
    assert_eq!(items[0]["fingerprint"], "fp-rr-agent-1");
    assert_eq!(items[0]["outbound_state"], "awaiting_approval");

    let detail_request_path = temp.path().join("worker-finding-detail-request.json");
    write_json_fixture(
        &detail_request_path,
        &sample_worker_request(
            &task,
            "worker.get_finding_detail",
            Some(json!({ "finding_id": "finding-rr-agent-1" })),
        ),
    );
    let detail = run_rr(
        &[
            "agent",
            "worker.get_finding_detail",
            "--task-file",
            task_path.to_str().expect("task path"),
            "--request-file",
            detail_request_path.to_str().expect("request path"),
        ],
        &runtime,
    );
    assert_eq!(detail.exit_code, 0, "{}", detail.stderr);
    let detail_payload = parse_robot_payload(&detail.stdout);
    assert_eq!(detail_payload["schema_id"], "rr.agent.response.v1");
    assert_eq!(detail_payload["status"], "succeeded");
    let detail_operation = &detail_payload["operation_response"];
    assert_eq!(
        detail_operation["status"],
        json!(WorkerOperationResponseStatus::Succeeded)
    );
    assert_eq!(
        detail_operation["payload"]["finding"]["finding_id"],
        "finding-rr-agent-1"
    );
    assert_eq!(
        detail_operation["payload"]["finding"]["summary"],
        "Approval token survives stale refresh"
    );
    assert_eq!(detail_operation["payload"]["evidence_locations"], json!([]));
    assert!(detail.stderr.trim().is_empty(), "{}", detail.stderr);
}

#[test]
fn rr_agent_accepts_stage_results_and_denies_unimplemented_memory_search() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let task = sample_worker_task();
    let task_path = temp.path().join("worker-task.json");
    write_json_fixture(&task_path, &task);
    seed_rr_agent_session(&runtime, &task);

    let stage_result_request = sample_worker_request(
        &task,
        "worker.submit_stage_result",
        Some(serde_json::to_value(sample_stage_result(&task)).expect("stage result value")),
    );
    let stage_output = run_rr_process_with_stdin(
        &[
            "agent",
            "worker.submit_stage_result",
            "--task-file",
            task_path.to_str().expect("task path"),
        ],
        &runtime,
        &serde_json::to_vec_pretty(&stage_result_request).expect("stage result request"),
    );
    assert_eq!(
        stage_output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&stage_output.stderr)
    );
    let stage_payload =
        parse_robot_payload(std::str::from_utf8(&stage_output.stdout).expect("stage stdout utf8"));
    assert_eq!(stage_payload["schema_id"], "rr.agent.response.v1");
    assert_eq!(stage_payload["status"], "succeeded");
    let stage_operation = &stage_payload["operation_response"];
    assert_eq!(
        stage_operation["status"],
        json!(WorkerOperationResponseStatus::Succeeded)
    );
    assert_eq!(stage_operation["authorization"]["lane"], "proposal");
    assert_eq!(stage_operation["authorization"]["advisory_only"], true);
    assert_eq!(stage_operation["payload"]["review_task_id"], task.id);
    assert_eq!(
        stage_operation["payload"]["result_schema_id"],
        WORKER_STAGE_RESULT_SCHEMA_V1
    );
    assert_eq!(
        stage_operation["payload"]["structured_findings_pack_present"],
        true
    );

    let capability_path = temp.path().join("rr-agent-capability.json");
    write_json_fixture(
        &capability_path,
        &json!({
            "transport_kind": "agent_cli",
            "supports_context_reads": true,
            "supports_memory_search": false,
            "supports_finding_reads": true,
            "supports_artifact_reads": true,
            "supports_stage_result_submission": true,
            "supports_clarification_requests": true,
            "supports_follow_up_hints": true,
            "supports_fix_mode": false
        }),
    );
    let denied_request_path = temp.path().join("worker-search-memory-request.json");
    write_json_fixture(
        &denied_request_path,
        &sample_worker_request(
            &task,
            "worker.search_memory",
            Some(json!({
                "query_text": "stale approval token",
                "query_mode": "recall"
            })),
        ),
    );
    let denied = run_rr(
        &[
            "agent",
            "worker.search_memory",
            "--task-file",
            task_path.to_str().expect("task path"),
            "--capability-file",
            capability_path.to_str().expect("capability path"),
            "--request-file",
            denied_request_path.to_str().expect("request path"),
        ],
        &runtime,
    );
    assert_eq!(denied.exit_code, 3, "{}", denied.stderr);
    let denied_payload = parse_robot_payload(&denied.stdout);
    assert_eq!(denied_payload["schema_id"], "rr.agent.response.v1");
    assert_eq!(denied_payload["status"], "denied");
    assert_eq!(denied_payload["transport_kind"], "agent_cli");
    let denied_operation = &denied_payload["operation_response"];
    assert_eq!(
        denied_operation["status"],
        json!(WorkerOperationResponseStatus::Denied)
    );
    assert_eq!(denied_operation["authorization"], Value::Null);
    assert_eq!(denied_operation["denial"]["code"], "capability_denied");
    assert_eq!(denied_operation["denial"]["denied_scopes"], json!(["repo"]));
    assert_eq!(denied_operation["operation"], "worker.search_memory");
    assert!(
        denied_operation["denial"]["message"]
            .as_str()
            .expect("denial message")
            .contains("requires capability 'supports_memory_search'"),
        "{}",
        denied.stdout
    );
}

#[test]
fn rr_agent_supports_search_artifact_and_advisory_operations() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let task = sample_worker_task();
    let task_path = temp.path().join("worker-task.json");
    write_json_fixture(&task_path, &task);
    seed_rr_agent_session(&runtime, &task);

    let store = RogerStore::open(&runtime.store_root).expect("open store");
    seed_prior_review_lookup_records(
        &store,
        &task.review_session_id,
        &task.review_run_id,
        "owner/repo",
    );
    let artifact = store
        .store_artifact(
            "artifact-rr-agent-1",
            ArtifactBudgetClass::EvidenceExcerpt,
            "text/plain",
            b"approval excerpt line one\nline two\n",
        )
        .expect("store artifact");

    let search_request_path = temp.path().join("worker-search-memory-success.json");
    write_json_fixture(
        &search_request_path,
        &sample_worker_request(
            &task,
            "worker.search_memory",
            Some(json!({
                "query_text": "approval token stale refresh",
                "query_mode": "candidate_audit"
            })),
        ),
    );
    let search = run_rr(
        &[
            "agent",
            "worker.search_memory",
            "--task-file",
            task_path.to_str().expect("task path"),
            "--request-file",
            search_request_path.to_str().expect("request path"),
        ],
        &runtime,
    );
    assert_eq!(search.exit_code, 0, "{}", search.stderr);
    let search_payload = parse_robot_payload(&search.stdout);
    assert_eq!(search_payload["schema_id"], "rr.agent.response.v1");
    assert_eq!(search_payload["status"], "succeeded");
    let search_operation = &search_payload["operation_response"];
    assert_eq!(
        search_operation["status"],
        json!(WorkerOperationResponseStatus::Succeeded)
    );
    assert_eq!(
        search_operation["payload"]["requested_query_mode"],
        "candidate_audit"
    );
    assert_eq!(
        search_operation["payload"]["resolved_query_mode"],
        "candidate_audit"
    );
    assert_eq!(
        search_operation["payload"]["search_plan"]["query_plan"]["candidate_visibility"],
        "candidate_audit_only"
    );
    assert_eq!(
        search_operation["payload"]["search_plan"]["retrieval_classes"],
        json!(["promoted_memory", "tentative_candidates", "evidence_hits"])
    );
    assert_eq!(
        search_operation["payload"]["search_plan"]["retrieval_strategy"]["semantic"],
        false
    );
    assert!(search_operation["payload"]["promoted_memory"].is_array());
    assert!(search_operation["payload"]["tentative_candidates"].is_array());
    assert!(search_operation["payload"]["evidence_hits"].is_array());
    assert_eq!(
        search_operation["payload"]["promoted_memory"][0]["citation_posture"],
        "cite_allowed"
    );
    assert_eq!(
        search_operation["payload"]["tentative_candidates"][0]["citation_posture"],
        "inspect_only"
    );
    assert_eq!(
        search_operation["payload"]["tentative_candidates"][0]["surface_posture"],
        "candidate_review"
    );
    assert_eq!(
        search_operation["payload"]["evidence_hits"][0]["memory_lane"],
        "evidence_hits"
    );

    let artifact_request_path = temp.path().join("worker-artifact-request.json");
    write_json_fixture(
        &artifact_request_path,
        &sample_worker_request(
            &task,
            "worker.get_artifact_excerpt",
            Some(json!({ "artifact_id": artifact.id })),
        ),
    );
    let artifact_result = run_rr(
        &[
            "agent",
            "worker.get_artifact_excerpt",
            "--task-file",
            task_path.to_str().expect("task path"),
            "--request-file",
            artifact_request_path.to_str().expect("request path"),
        ],
        &runtime,
    );
    assert_eq!(artifact_result.exit_code, 0, "{}", artifact_result.stderr);
    let artifact_payload = parse_robot_payload(&artifact_result.stdout);
    let artifact_operation = &artifact_payload["operation_response"];
    assert_eq!(
        artifact_operation["status"],
        json!(WorkerOperationResponseStatus::Succeeded)
    );
    assert_eq!(artifact_operation["payload"]["artifact_id"], artifact.id);
    assert!(
        artifact_operation["payload"]["excerpt"]
            .as_str()
            .expect("artifact excerpt")
            .contains("approval excerpt line one"),
        "{}",
        artifact_result.stdout
    );

    let clarification_request_path = temp.path().join("worker-clarification-request.json");
    write_json_fixture(
        &clarification_request_path,
        &sample_worker_request(
            &task,
            "worker.request_clarification",
            Some(json!({
                "id": "clarify-1",
                "question": "Should Roger preserve this approval token after refresh?",
                "reason": "Need operator intent",
                "blocking": true
            })),
        ),
    );
    let clarification = run_rr(
        &[
            "agent",
            "worker.request_clarification",
            "--task-file",
            task_path.to_str().expect("task path"),
            "--request-file",
            clarification_request_path.to_str().expect("request path"),
        ],
        &runtime,
    );
    assert_eq!(clarification.exit_code, 0, "{}", clarification.stderr);
    let clarification_payload = parse_robot_payload(&clarification.stdout);
    let clarification_operation = &clarification_payload["operation_response"];
    assert_eq!(
        clarification_operation["status"],
        json!(WorkerOperationResponseStatus::Succeeded)
    );
    assert_eq!(
        clarification_operation["payload"]["question"],
        "Should Roger preserve this approval token after refresh?"
    );
    assert_eq!(clarification_operation["payload"]["blocking"], true);

    let memory_review_request_path = temp.path().join("worker-memory-review-request.json");
    write_json_fixture(
        &memory_review_request_path,
        &sample_worker_request(
            &task,
            "worker.request_memory_review",
            Some(json!({
                "id": "memory-review-1",
                "query": "approval token invalidation",
                "requested_scopes": ["repo"],
                "rationale": "Need prior-review context"
            })),
        ),
    );
    let memory_review = run_rr(
        &[
            "agent",
            "worker.request_memory_review",
            "--task-file",
            task_path.to_str().expect("task path"),
            "--request-file",
            memory_review_request_path
                .to_str()
                .expect("memory review request path"),
        ],
        &runtime,
    );
    assert_eq!(memory_review.exit_code, 0, "{}", memory_review.stderr);
    let memory_review_payload = parse_robot_payload(&memory_review.stdout);
    let memory_review_operation = &memory_review_payload["operation_response"];
    assert_eq!(
        memory_review_operation["status"],
        json!(WorkerOperationResponseStatus::Succeeded)
    );
    assert_eq!(
        memory_review_operation["payload"]["query"],
        "approval token invalidation"
    );
    assert_eq!(
        memory_review_operation["payload"]["requested_scopes"],
        json!(["repo"])
    );

    let follow_up_request_path = temp.path().join("worker-follow-up-request.json");
    write_json_fixture(
        &follow_up_request_path,
        &sample_worker_request(
            &task,
            "worker.propose_follow_up",
            Some(json!({
                "id": "follow-up-1",
                "title": "Audit approval invalidation after refresh",
                "objective": "Verify refresh invalidates stale approvals before posting",
                "proposed_task_kind": "deep_review_pass",
                "suggested_scopes": ["repo"]
            })),
        ),
    );
    let follow_up = run_rr(
        &[
            "agent",
            "worker.propose_follow_up",
            "--task-file",
            task_path.to_str().expect("task path"),
            "--request-file",
            follow_up_request_path
                .to_str()
                .expect("follow-up request path"),
        ],
        &runtime,
    );
    assert_eq!(follow_up.exit_code, 0, "{}", follow_up.stderr);
    let follow_up_payload = parse_robot_payload(&follow_up.stdout);
    let follow_up_operation = &follow_up_payload["operation_response"];
    assert_eq!(
        follow_up_operation["status"],
        json!(WorkerOperationResponseStatus::Succeeded)
    );
    assert_eq!(
        follow_up_operation["payload"]["title"],
        "Audit approval invalidation after refresh"
    );
    assert_eq!(
        follow_up_operation["payload"]["suggested_scopes"],
        json!(["repo"])
    );
}

#[test]
fn bridge_pack_extension_emits_checksum_artifacts_in_smoke() {
    let _lock = extension_pack_test_lock()
        .lock()
        .expect("acquire extension pack smoke lock");
    let temp = tempdir().expect("tempdir");
    let runtime = CliRuntime {
        cwd: workspace_root(),
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };
    let output_dir = temp.path().join("pack-output");
    let result = run(
        &[
            "bridge".to_owned(),
            "pack-extension".to_owned(),
            "--output-dir".to_owned(),
            output_dir.to_string_lossy().to_string(),
            "--robot".to_owned(),
        ],
        &runtime,
    );
    assert_eq!(result.exit_code, 0, "{}", result.stderr);
    let payload = parse_robot_payload(&result.stdout);
    assert_eq!(payload["outcome"], "complete");
    assert_eq!(payload["data"]["subcommand"], "pack-extension");
    assert_eq!(payload["data"]["installs_browser_extension"], false);
    assert!(payload["data"]["version"].as_str().is_some());
    assert!(payload["data"]["version_name"].as_str().is_some());
    let package_dir = PathBuf::from(
        payload["data"]["package_dir"]
            .as_str()
            .expect("package_dir should be present"),
    );
    assert!(package_dir.join("SHA256SUMS").exists());
    assert!(package_dir.join("asset-manifest.json").exists());
    let manifest = fs::read_to_string(package_dir.join("manifest.json")).expect("read manifest");
    let manifest_json: serde_json::Value = serde_json::from_str(&manifest).expect("parse manifest");
    assert_eq!(manifest_json["version"], payload["data"]["version"]);
    assert_eq!(
        manifest_json["version_name"],
        payload["data"]["version_name"]
    );
}

#[test]
fn extension_setup_blocks_without_discovered_identity_in_smoke() {
    let _lock = extension_pack_test_lock()
        .lock()
        .expect("acquire extension pack smoke lock");
    let temp = tempdir().expect("tempdir");
    let runtime = CliRuntime {
        cwd: workspace_root(),
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };
    let install_root = temp.path().join("install-root");

    let setup = run(
        &[
            "extension".to_owned(),
            "setup".to_owned(),
            "--browser".to_owned(),
            "chrome".to_owned(),
            "--install-root".to_owned(),
            install_root.to_string_lossy().to_string(),
            "--robot".to_owned(),
        ],
        &runtime,
    );
    assert_eq!(setup.exit_code, 3, "{}", setup.stderr);
    let payload = parse_robot_payload(&setup.stdout);
    assert_eq!(payload["schema_id"], "rr.robot.extension.v1");
    assert_eq!(payload["outcome"], "blocked");
    assert_eq!(payload["data"]["subcommand"], "setup");
    assert_eq!(
        payload["data"]["reason_code"],
        "extension_registration_missing"
    );
    assert_eq!(payload["data"]["browser"], "chrome");
    assert!(
        payload["data"]["manual_browser_step"]
            .as_str()
            .expect("manual browser step")
            .contains("chrome://extensions")
    );
}

#[test]
fn extension_setup_and_doctor_emit_complete_envelopes_in_smoke() {
    let _lock = extension_pack_test_lock()
        .lock()
        .expect("acquire extension pack smoke lock");
    let temp = tempdir().expect("tempdir");
    let runtime = CliRuntime {
        cwd: workspace_root(),
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };
    let install_root = temp.path().join("install-root");
    let extension_id = "abcdefghijklmnopabcdefghijklmnop";
    write_guided_profile_discovery_state(&runtime, "edge", extension_id);

    let setup = run(
        &[
            "extension".to_owned(),
            "setup".to_owned(),
            "--browser".to_owned(),
            "edge".to_owned(),
            "--install-root".to_owned(),
            install_root.to_string_lossy().to_string(),
            "--robot".to_owned(),
        ],
        &runtime,
    );
    assert_eq!(setup.exit_code, 0, "{}", setup.stderr);
    let setup_payload = parse_robot_payload(&setup.stdout);
    assert_eq!(setup_payload["schema_id"], "rr.robot.extension.v1");
    assert_eq!(setup_payload["outcome"], "complete");
    assert_eq!(setup_payload["data"]["subcommand"], "setup");
    assert_eq!(setup_payload["data"]["browser"], "edge");
    assert_eq!(setup_payload["data"]["extension_id"], extension_id);
    assert_eq!(
        setup_payload["data"]["extension_id_source"],
        "browser_profile_preferences"
    );
    assert_eq!(
        setup_payload["data"]["doctor"]["subcommand"], "doctor",
        "setup should embed doctor result envelope"
    );
    assert!(
        setup_payload["data"]["doctor"]["checks"]
            .as_array()
            .expect("setup doctor checks")
            .iter()
            .all(|entry| entry["ok"] == true)
    );

    let doctor = run(
        &[
            "extension".to_owned(),
            "doctor".to_owned(),
            "--browser".to_owned(),
            "edge".to_owned(),
            "--install-root".to_owned(),
            install_root.to_string_lossy().to_string(),
            "--robot".to_owned(),
        ],
        &runtime,
    );
    assert_eq!(doctor.exit_code, 0, "{}", doctor.stderr);
    let doctor_payload = parse_robot_payload(&doctor.stdout);
    assert_eq!(doctor_payload["schema_id"], "rr.robot.extension.v1");
    assert_eq!(doctor_payload["outcome"], "complete");
    assert_eq!(doctor_payload["data"]["subcommand"], "doctor");
    assert_eq!(doctor_payload["data"]["browser"], "edge");
    assert!(
        doctor_payload["data"]["checks"]
            .as_array()
            .expect("doctor checks")
            .iter()
            .all(|entry| entry["ok"] == true)
    );
}

#[test]
fn extension_setup_and_doctor_succeed_after_bridge_registration_event_in_smoke() {
    let _lock = extension_pack_test_lock()
        .lock()
        .expect("acquire extension pack smoke lock");
    let temp = tempdir().expect("tempdir");
    let runtime = CliRuntime {
        cwd: workspace_root(),
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };
    let install_root = temp.path().join("install-root");
    let extension_id = "abcdefghijklmnopabcdefghijklmnop";

    let blocked = run(
        &[
            "extension".to_owned(),
            "setup".to_owned(),
            "--browser".to_owned(),
            "brave".to_owned(),
            "--install-root".to_owned(),
            install_root.to_string_lossy().to_string(),
            "--robot".to_owned(),
        ],
        &runtime,
    );
    assert_eq!(blocked.exit_code, 3, "{}", blocked.stderr);
    let blocked_payload = parse_robot_payload(&blocked.stdout);
    assert_eq!(blocked_payload["outcome"], "blocked");
    assert_eq!(
        blocked_payload["data"]["reason_code"],
        "extension_registration_missing"
    );

    register_extension_identity_via_bridge(&runtime, "brave", extension_id);

    let setup = run(
        &[
            "extension".to_owned(),
            "setup".to_owned(),
            "--browser".to_owned(),
            "brave".to_owned(),
            "--install-root".to_owned(),
            install_root.to_string_lossy().to_string(),
            "--robot".to_owned(),
        ],
        &runtime,
    );
    assert_eq!(setup.exit_code, 0, "{}", setup.stderr);
    let setup_payload = parse_robot_payload(&setup.stdout);
    assert_eq!(setup_payload["outcome"], "complete");
    assert_eq!(setup_payload["data"]["browser"], "brave");
    assert_eq!(setup_payload["data"]["extension_id"], extension_id);
    assert_eq!(
        setup_payload["data"]["extension_id_source"],
        "store_registry"
    );

    let doctor = run(
        &[
            "extension".to_owned(),
            "doctor".to_owned(),
            "--browser".to_owned(),
            "brave".to_owned(),
            "--install-root".to_owned(),
            install_root.to_string_lossy().to_string(),
            "--robot".to_owned(),
        ],
        &runtime,
    );
    assert_eq!(doctor.exit_code, 0, "{}", doctor.stderr);
    let doctor_payload = parse_robot_payload(&doctor.stdout);
    assert_eq!(doctor_payload["outcome"], "complete");
    assert!(
        doctor_payload["data"]["checks"]
            .as_array()
            .expect("doctor checks")
            .iter()
            .all(|entry| entry["ok"] == true)
    );
}

#[test]
fn extension_setup_auto_completes_when_identity_is_observed_during_wait_in_smoke() {
    let _lock = extension_pack_test_lock()
        .lock()
        .expect("acquire extension pack smoke lock");
    let temp = tempdir().expect("tempdir");
    let runtime = CliRuntime {
        cwd: workspace_root(),
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };
    let install_root = temp.path().join("install-root");
    let extension_id = "abcdefghijklmnopabcdefghijklmnop";
    let runtime_for_observer = runtime.clone();
    let extension_id_for_observer = extension_id.to_owned();
    let observer = thread::spawn(move || {
        thread::sleep(Duration::from_millis(900));
        write_guided_profile_discovery_state(
            &runtime_for_observer,
            "chrome",
            &extension_id_for_observer,
        );
    });

    let setup = run(
        &[
            "extension".to_owned(),
            "setup".to_owned(),
            "--browser".to_owned(),
            "chrome".to_owned(),
            "--install-root".to_owned(),
            install_root.to_string_lossy().to_string(),
            "--robot".to_owned(),
        ],
        &runtime,
    );
    observer
        .join()
        .expect("join guided-profile registration observer");
    assert_eq!(setup.exit_code, 0, "{}", setup.stderr);
    let setup_payload = parse_robot_payload(&setup.stdout);
    assert_eq!(setup_payload["outcome"], "complete");
    assert_eq!(setup_payload["data"]["subcommand"], "setup");
    assert_eq!(setup_payload["data"]["browser"], "chrome");
    assert_eq!(setup_payload["data"]["extension_id"], extension_id);
    assert_eq!(
        setup_payload["data"]["extension_id_source"],
        "browser_profile_preferences"
    );
    assert_eq!(
        setup_payload["data"]["registration_observed_during_setup_wait"],
        true
    );
    assert!(
        setup_payload["data"]["doctor"]["checks"]
            .as_array()
            .expect("doctor checks")
            .iter()
            .all(|entry| entry["ok"] == true)
    );
}

#[test]
fn extension_setup_discovers_identity_from_guided_profile_preferences_in_smoke() {
    let _lock = extension_pack_test_lock()
        .lock()
        .expect("acquire extension pack smoke lock");
    let temp = tempdir().expect("tempdir");
    let runtime = CliRuntime {
        cwd: workspace_root(),
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };
    let install_root = temp.path().join("install-root");
    let extension_id = "abcdefghijklmnopabcdefghijklmnop";
    write_guided_profile_discovery_state(&runtime, "chrome", extension_id);

    let setup = run(
        &[
            "extension".to_owned(),
            "setup".to_owned(),
            "--browser".to_owned(),
            "chrome".to_owned(),
            "--install-root".to_owned(),
            install_root.to_string_lossy().to_string(),
            "--robot".to_owned(),
        ],
        &runtime,
    );
    assert_eq!(setup.exit_code, 0, "{}", setup.stderr);
    let setup_payload = parse_robot_payload(&setup.stdout);
    assert_eq!(setup_payload["outcome"], "complete");
    assert_eq!(
        setup_payload["data"]["extension_id_source"],
        "browser_profile_preferences"
    );
    assert_eq!(setup_payload["data"]["extension_id"], extension_id);
    let registry_path = runtime.store_root.join("bridge/extension-id");
    let persisted = fs::read_to_string(registry_path).expect("persisted extension id");
    assert_eq!(persisted.trim(), extension_id);
}

#[test]
fn bridge_install_uninstall_is_failure_closed_and_reports_asset_checksums_in_smoke() {
    let _lock = extension_pack_test_lock()
        .lock()
        .expect("acquire extension pack smoke lock");
    let temp = tempdir().expect("tempdir");
    let runtime = CliRuntime {
        cwd: workspace_root(),
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };
    let install_root = temp.path().join("install-root");

    let blocked = run(
        &[
            "bridge".to_owned(),
            "install".to_owned(),
            "--install-root".to_owned(),
            install_root.to_string_lossy().to_string(),
            "--robot".to_owned(),
        ],
        &runtime,
    );
    assert_eq!(blocked.exit_code, 3, "{}", blocked.stderr);
    let blocked_payload = parse_robot_payload(&blocked.stdout);
    assert_eq!(blocked_payload["outcome"], "blocked");
    assert_eq!(
        blocked_payload["data"]["reason_code"],
        "extension_id_discovery_failed"
    );

    let extension_registry = runtime.store_root.join("bridge/extension-id");
    fs::create_dir_all(
        extension_registry
            .parent()
            .expect("extension registry parent"),
    )
    .expect("create extension registry parent");
    fs::write(&extension_registry, "abcdefghijklmnopabcdefghijklmnop\n")
        .expect("write extension identity registry");

    let install = run(
        &[
            "bridge".to_owned(),
            "install".to_owned(),
            "--install-root".to_owned(),
            install_root.to_string_lossy().to_string(),
            "--robot".to_owned(),
        ],
        &runtime,
    );
    assert_eq!(install.exit_code, 0, "{}", install.stderr);
    let install_payload = parse_robot_payload(&install.stdout);
    assert_eq!(install_payload["outcome"], "complete");
    assert_eq!(
        install_payload["data"]["extension_id_source"],
        "store_registry"
    );
    assert_eq!(
        install_payload["data"]["bridge_binary_source"],
        "installed_rr_current_exe"
    );
    let assets = install_payload["data"]["assets"]
        .as_array()
        .expect("assets array");
    assert!(!assets.is_empty());
    assert!(assets.iter().all(|asset| {
        asset["sha256"]
            .as_str()
            .is_some_and(|checksum| checksum.len() == 64)
    }));

    let uninstall = run(
        &[
            "bridge".to_owned(),
            "uninstall".to_owned(),
            "--install-root".to_owned(),
            install_root.to_string_lossy().to_string(),
            "--robot".to_owned(),
        ],
        &runtime,
    );
    assert_eq!(uninstall.exit_code, 0, "{}", uninstall.stderr);
    let uninstall_payload = parse_robot_payload(&uninstall.stdout);
    assert_eq!(uninstall_payload["outcome"], "complete");
    assert!(
        uninstall_payload["data"]["removed"]
            .as_array()
            .expect("removed list")
            .len()
            >= 1
    );
}

#[test]
fn partial_harness_binding_fails_closed_with_rr_fallback() {
    let bindings = vec![HarnessCommandBinding {
        provider: "opencode".to_owned(),
        command_id: RogerCommandId::RogerStatus,
        provider_command_syntax: "/roger-status".to_owned(),
        capability_requirements: vec!["supports_roger_commands".to_owned()],
    }];

    let command = RogerCommand {
        command_id: RogerCommandId::RogerReturn,
        review_session_id: Some("session-1".to_owned()),
        review_run_id: None,
        args: HashMap::new(),
        invocation_surface: RogerCommandInvocationSurface::HarnessCommand,
        provider: "opencode".to_owned(),
    };

    let routed = route_harness_command(&command, &bindings);
    assert_eq!(routed.status, RogerCommandRouteStatus::FallbackRequired);
    assert_eq!(routed.next_action.fallback_cli_command, "rr return");
    assert!(
        routed
            .next_action
            .session_finder_hint
            .expect("session finder hint")
            .contains("--session <id>")
    );
}

#[test]
fn status_and_findings_support_toon_robot_format() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let review = run_rr(&["review", "--pr", "42", "--robot"], &runtime);
    assert_eq!(review.exit_code, 0, "{}", review.stderr);

    let status = run_rr(
        &["status", "--pr", "42", "--robot", "--robot-format", "toon"],
        &runtime,
    );
    assert_eq!(status.exit_code, 0, "{}", status.stderr);
    let status_payload = parse_toon_payload(&status.stdout);
    assert_eq!(status_payload["schema_id"], "rr.robot.status.v1");
    assert_eq!(status_payload["robot_format"], "toon");
    assert_eq!(status_payload["outcome"], "complete");

    let findings = run_rr(
        &[
            "findings",
            "--pr",
            "42",
            "--robot",
            "--robot-format",
            "toon",
        ],
        &runtime,
    );
    assert_eq!(findings.exit_code, 0, "{}", findings.stderr);
    let findings_payload = parse_toon_payload(&findings.stdout);
    assert_eq!(findings_payload["schema_id"], "rr.robot.findings.v1");
    assert_eq!(findings_payload["robot_format"], "toon");
}

#[test]
fn toon_is_rejected_outside_status_and_findings_in_this_slice() {
    let temp = tempdir().expect("tempdir");
    let repo = init_repo(&temp);
    let (_stub_dir, opencode_bin) = write_stub_binary(false);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    };

    let review = run_rr(
        &["review", "--pr", "42", "--robot", "--robot-format", "toon"],
        &runtime,
    );
    assert_eq!(review.exit_code, 2);
    assert!(
        review
            .stderr
            .contains("toon format is only supported for status/findings in this slice"),
        "{}",
        review.stderr
    );
}
