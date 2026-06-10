#![cfg(unix)]

use roger_cli::{CliRuntime, run};
use roger_session_copilot::{ENV_COPILOT_ADMISSION_GATE, ENV_COPILOT_BIN};
use roger_storage::{LaunchAttemptState, RogerStore};
use serde_json::Value;
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use tempfile::tempdir;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("packages parent")
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn parse_robot(stdout: &str) -> Value {
    serde_json::from_str(stdout).expect("robot payload")
}

fn fixture_json(path: &str) -> Value {
    serde_json::from_slice(&fs::read(workspace_root().join(path)).expect("read fixture json"))
        .expect("parse fixture json")
}

fn normalized_path_string(path: &Path) -> String {
    fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

fn fixture_case(path: &str, id: &str, list_key: &str) -> Value {
    let fixture = fixture_json(path);
    fixture[list_key]
        .as_array()
        .expect("fixture cases")
        .iter()
        .find(|case| case["id"] == id)
        .cloned()
        .map_or_else(
            || {
                assert!(false, "missing fixture case {id}");
                unreachable!()
            },
            |case| case,
        )
}

fn init_repo(path: &Path) {
    fs::create_dir_all(path).expect("create repo path");
    let init = Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("init")
        .output()
        .expect("git init");
    assert!(
        init.status.success(),
        "git init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );
    let remote = Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("remote")
        .arg("add")
        .arg("origin")
        .arg("git@github.com:owner/repo.git")
        .output()
        .expect("git remote add origin");
    assert!(
        remote.status.success(),
        "git remote add origin failed: {}",
        String::from_utf8_lossy(&remote.stderr)
    );
}

fn write_executable(path: &Path, contents: &str) {
    fs::write(path, contents).expect("write executable");
    let mut permissions = fs::metadata(path).expect("stat executable").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).expect("chmod executable");
}

fn run_with_env_overrides<T>(
    overrides: &[(&str, Option<&str>)],
    run_block: impl FnOnce() -> T,
) -> T {
    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = ENV_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("env lock");

    let previous: Vec<(String, Option<OsString>)> = overrides
        .iter()
        .map(|(key, _)| ((*key).to_owned(), std::env::var_os(key)))
        .collect();

    for (key, value) in overrides {
        match value {
            Some(value) => {
                // SAFETY: tests serialize environment mutation via ENV_LOCK and restore before return.
                unsafe {
                    std::env::set_var(key, value);
                }
            }
            None => {
                // SAFETY: tests serialize environment mutation via ENV_LOCK and restore before return.
                unsafe {
                    std::env::remove_var(key);
                }
            }
        }
    }

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(run_block));

    for (key, value) in previous {
        match value {
            Some(value) => {
                // SAFETY: tests serialize environment mutation via ENV_LOCK and restore before return.
                unsafe {
                    std::env::set_var(&key, value);
                }
            }
            None => {
                // SAFETY: tests serialize environment mutation via ENV_LOCK and restore before return.
                unsafe {
                    std::env::remove_var(&key);
                }
            }
        }
    }

    match result {
        Ok(value) => value,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

#[test]
fn rr_review_copilot_feature_gate_verifies_hook_and_binds_session() {
    let launch_case = fixture_case(
        "tests/fixtures/fixture_copilot_launch_resume/copilot_launch_resume_cases.json",
        "launch_review_verified_session_id",
        "cases",
    );
    let hook_event = fixture_case(
        "tests/fixtures/fixture_copilot_hook_stream/copilot_hook_events.json",
        "session_start_verified_id",
        "events",
    );
    let transcript_record = fixture_case(
        "tests/fixtures/fixture_copilot_transcript_capture/copilot_transcript_refs.json",
        "transcript_capture_nominal",
        "records",
    );
    let verified_session_id = launch_case["expected"]["verified_session_id"]
        .as_str()
        .expect("verified session id");
    let expected_launch_state = launch_case["expected"]["launch_attempt_state"]
        .as_str()
        .expect("launch attempt state");
    let expected_continuity = launch_case["expected"]["continuity_quality"]
        .as_str()
        .expect("continuity quality");
    let expected_launch_profile = hook_event["payload"]["launch_profile_id"]
        .as_str()
        .expect("launch profile id");
    let transcript_artifact_ref = transcript_record["artifact_refs"][0]
        .as_str()
        .expect("transcript artifact ref");

    let temp = tempdir().expect("tempdir");
    let repo = temp.path().join("repo");
    init_repo(&repo);
    let store_root = temp.path().join("roger-store");
    let cwd_capture = temp.path().join("copilot.cwd");
    let fake_copilot = temp.path().join("fake-copilot");
    let fake_copilot_script = format!(
        "#!/bin/sh\nset -eu\npwd > \"{}\"\nmkdir -p \"$(dirname \"$RR_COPILOT_SESSION_START_ARTIFACT\")\"\ncat > \"$RR_COPILOT_SESSION_START_ARTIFACT\" <<EOF\n{{\"hook\":\"session-start\",\"payload\":{{\"provider\":\"copilot\",\"session_id\":\"{}\",\"worktree_root\":\"$PWD\",\"launch_profile_id\":\"{}\",\"artifact_refs\":[\"{}\"]}}}}\nEOF\nprintf 'Copilot CLI ready\\nsession_id={}\\n'\n",
        cwd_capture.display(),
        verified_session_id,
        expected_launch_profile,
        transcript_artifact_ref,
        verified_session_id,
    );
    write_executable(&fake_copilot, &fake_copilot_script);

    let runtime = CliRuntime {
        cwd: repo.clone(),
        store_root: store_root.clone(),
        opencode_bin: "opencode".to_owned(),
    };

    let gate = "1".to_owned();
    let copilot_bin = fake_copilot.to_string_lossy().to_string();
    let review = run_with_env_overrides(
        &[
            (ENV_COPILOT_ADMISSION_GATE, Some(gate.as_str())),
            (ENV_COPILOT_BIN, Some(copilot_bin.as_str())),
        ],
        || {
            run(
                &[
                    "review".to_owned(),
                    "--pr".to_owned(),
                    "42".to_owned(),
                    "--provider".to_owned(),
                    "copilot".to_owned(),
                    "--robot".to_owned(),
                ],
                &runtime,
            )
        },
    );

    assert_eq!(review.exit_code, 5, "{}", review.stderr);
    let payload = parse_robot(&review.stdout);
    assert_eq!(payload["outcome"], "degraded");
    assert_eq!(payload["data"]["provider"], "copilot");
    assert_eq!(payload["data"]["continuity_quality"], expected_continuity);
    assert_eq!(
        payload["data"]["provider_capability"]["status"],
        "feature_gated_bounded_live"
    );
    assert_eq!(
        payload["data"]["provider_capability"]["support_tier"],
        "tier_b_feature_gated"
    );
    assert_eq!(
        payload["data"]["provider_capability"]["supports"]["review_start"],
        true
    );
    assert_eq!(
        payload["data"]["provider_capability"]["supports"]["resume_reseed"],
        true
    );
    assert_eq!(
        payload["data"]["provider_capability"]["supports"]["resume_reopen"],
        true
    );
    assert_eq!(
        payload["data"]["provider_capability"]["supports"]["return"],
        true
    );
    assert_eq!(
        payload["data"]["provider_capability"]["audit_artifact_classes"],
        serde_json::json!(["copilot_tool_denial", "copilot_transcript_reference"])
    );
    assert_eq!(
        payload["data"]["routine_surface"]["worktree_root"],
        normalized_path_string(&repo)
    );
    assert_eq!(
        fs::read_to_string(&cwd_capture)
            .expect("read cwd capture")
            .trim(),
        normalized_path_string(&repo)
    );

    let session_id = payload["data"]["session_id"]
        .as_str()
        .expect("roger session id");
    let launch_attempt_id = payload["data"]["launch_attempt_id"]
        .as_str()
        .expect("launch attempt id");

    let store = RogerStore::open(&store_root).expect("open store");
    let session = store
        .review_session(session_id)
        .expect("read session")
        .expect("session exists");
    assert_eq!(session.provider, "copilot");
    assert_eq!(
        session
            .session_locator
            .as_ref()
            .expect("session locator")
            .session_id,
        verified_session_id
    );
    assert!(
        session
            .session_locator
            .as_ref()
            .expect("session locator")
            .invocation_context_json
            .contains("\"verification_source\":\"session_start_hook_artifact\"")
    );
    let invocation_context: Value = serde_json::from_str(
        &session
            .session_locator
            .as_ref()
            .expect("session locator")
            .invocation_context_json,
    )
    .expect("parse invocation context");
    let launch_capture_path = invocation_context["launch_capture_path"]
        .as_str()
        .expect("launch capture path");
    assert!(
        Path::new(launch_capture_path).is_file(),
        "missing launch capture file {}",
        launch_capture_path
    );
    assert!(
        invocation_context["artifact_refs"]
            .as_array()
            .expect("artifact refs")
            .iter()
            .any(|value| value.as_str() == Some(transcript_artifact_ref))
    );
    assert_eq!(session.resume_bundle_artifact_id.as_deref().is_some(), true);
    let bundle = store
        .load_resume_bundle(
            session
                .resume_bundle_artifact_id
                .as_deref()
                .expect("resume bundle artifact id"),
        )
        .expect("load resume bundle");
    assert!(
        bundle
            .artifact_refs
            .iter()
            .any(|value| value == transcript_artifact_ref)
    );
    assert!(
        bundle
            .artifact_refs
            .iter()
            .any(|value| value == launch_capture_path)
    );
    let launch_capture: Value =
        serde_json::from_slice(&fs::read(launch_capture_path).expect("read launch capture"))
            .expect("parse launch capture");
    assert_eq!(
        launch_capture["session_start_payload"]["session_id"],
        verified_session_id
    );
    assert!(
        launch_capture["artifact_refs"]
            .as_array()
            .expect("capture artifact refs")
            .iter()
            .any(|value| value.as_str() == Some(transcript_artifact_ref))
    );

    let attempt = store
        .launch_attempt(launch_attempt_id)
        .expect("read attempt")
        .expect("attempt exists");
    assert_eq!(
        expected_launch_state, "verified_started",
        "unexpected fixture launch state {expected_launch_state}"
    );
    assert_eq!(attempt.state, LaunchAttemptState::VerifiedStarted);
    assert_eq!(attempt.final_session_id.as_deref(), Some(session_id));
    assert_eq!(
        attempt.provider_session_id.as_deref(),
        Some(verified_session_id)
    );
}

#[test]
fn rr_review_copilot_feature_gate_fails_closed_when_hook_omits_session_id() {
    let negative_event = fixture_case(
        "tests/fixtures/fixture_copilot_hook_stream/copilot_hook_events.json",
        "missing_session_start",
        "negative_events",
    );
    let temp = tempdir().expect("tempdir");
    let repo = temp.path().join("repo");
    init_repo(&repo);
    let store_root = temp.path().join("roger-store");
    let fake_copilot = temp.path().join("fake-copilot-missing-id");
    let fake_copilot_script = format!(
        "#!/bin/sh\nset -eu\nmkdir -p \"$(dirname \"$RR_COPILOT_SESSION_START_ARTIFACT\")\"\ncat > \"$RR_COPILOT_SESSION_START_ARTIFACT\" <<EOF\n{{\"hook\":\"session-start\",\"payload\":{{\"provider\":\"copilot\",\"session_id\":\"{}\",\"worktree_root\":\"$PWD\",\"launch_profile_id\":\"profile-open-pr\"}}}}\nEOF\nprintf 'Copilot CLI ready\\n'\n",
        negative_event["payload"]["session_id"]
            .as_str()
            .expect("missing session id fixture"),
    );
    write_executable(&fake_copilot, &fake_copilot_script);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: store_root.clone(),
        opencode_bin: "opencode".to_owned(),
    };

    let gate = "1".to_owned();
    let copilot_bin = fake_copilot.to_string_lossy().to_string();
    let review = run_with_env_overrides(
        &[
            (ENV_COPILOT_ADMISSION_GATE, Some(gate.as_str())),
            (ENV_COPILOT_BIN, Some(copilot_bin.as_str())),
        ],
        || {
            run(
                &[
                    "review".to_owned(),
                    "--pr".to_owned(),
                    "42".to_owned(),
                    "--provider".to_owned(),
                    "copilot".to_owned(),
                    "--robot".to_owned(),
                ],
                &runtime,
            )
        },
    );

    assert_eq!(review.exit_code, 3, "{}", review.stderr);
    let payload = parse_robot(&review.stdout);
    assert_eq!(payload["outcome"], "blocked");
    assert_eq!(
        payload["data"]["reason_code"],
        "missing_verified_session_id"
    );
    let launch_attempt_id = payload["data"]["launch_attempt_id"]
        .as_str()
        .expect("launch attempt id");

    let store = RogerStore::open(&store_root).expect("open store");
    let attempt = store
        .launch_attempt(launch_attempt_id)
        .expect("read attempt")
        .expect("attempt exists");
    assert_eq!(
        attempt.state,
        LaunchAttemptState::FailedProviderVerification
    );
    assert!(attempt.final_session_id.is_none());
}

#[test]
fn rr_review_copilot_feature_gate_retains_crash_evidence_before_binding() {
    let crash_case = fixture_case(
        "tests/fixtures/fixture_copilot_crash_recovery/copilot_crash_recovery_cases.json",
        "launch_process_crash_before_session_start",
        "cases",
    );
    let stderr_line = crash_case["stub_output"]["stderr_lines"][0]
        .as_str()
        .expect("crash stderr line");
    let temp = tempdir().expect("tempdir");
    let repo = temp.path().join("repo");
    init_repo(&repo);
    let store_root = temp.path().join("roger-store");
    let fake_copilot = temp.path().join("fake-copilot-crash");
    let fake_copilot_script = format!(
        "#!/bin/sh\nset -eu\nprintf '{}\\n' >&2\nexit 137\n",
        stderr_line
    );
    write_executable(&fake_copilot, &fake_copilot_script);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: store_root.clone(),
        opencode_bin: "opencode".to_owned(),
    };

    let gate = "1".to_owned();
    let copilot_bin = fake_copilot.to_string_lossy().to_string();
    let review = run_with_env_overrides(
        &[
            (ENV_COPILOT_ADMISSION_GATE, Some(gate.as_str())),
            (ENV_COPILOT_BIN, Some(copilot_bin.as_str())),
        ],
        || {
            run(
                &[
                    "review".to_owned(),
                    "--pr".to_owned(),
                    "42".to_owned(),
                    "--provider".to_owned(),
                    "copilot".to_owned(),
                    "--robot".to_owned(),
                ],
                &runtime,
            )
        },
    );

    assert_eq!(review.exit_code, 3, "{}", review.stderr);
    let payload = parse_robot(&review.stdout);
    assert_eq!(payload["outcome"], "blocked");
    assert_eq!(payload["data"]["reason_code"], "provider_process_crash");
    assert!(
        payload["data"]["stderr_preview"]
            .as_array()
            .expect("stderr preview")
            .iter()
            .any(|line| line
                .as_str()
                .is_some_and(|value| value.contains(stderr_line)))
    );
    let launch_attempt_id = payload["data"]["launch_attempt_id"]
        .as_str()
        .expect("launch attempt id");

    let store = RogerStore::open(&store_root).expect("open store");
    let attempt = store
        .launch_attempt(launch_attempt_id)
        .expect("read attempt")
        .expect("attempt exists");
    assert_eq!(attempt.state, LaunchAttemptState::FailedSpawn);
    assert!(attempt.final_session_id.is_none());
    assert!(
        attempt
            .failure_reason
            .as_deref()
            .unwrap_or_default()
            .contains(stderr_line)
    );
}

#[test]
fn rr_resume_copilot_feature_gate_reopens_live_locator_and_preserves_session_id() {
    let launch_case = fixture_case(
        "tests/fixtures/fixture_copilot_launch_resume/copilot_launch_resume_cases.json",
        "launch_review_verified_session_id",
        "cases",
    );
    let resume_case = fixture_case(
        "tests/fixtures/fixture_copilot_launch_resume/copilot_launch_resume_cases.json",
        "resume_live_locator_reopen",
        "cases",
    );
    let transcript = fixture_case(
        "tests/fixtures/fixture_copilot_transcript_capture/copilot_transcript_refs.json",
        "transcript_capture_nominal",
        "records",
    );
    let session_id = launch_case["expected"]["verified_session_id"]
        .as_str()
        .expect("launch session id");
    let transcript_artifact_ref = transcript["artifact_refs"][0]
        .as_str()
        .expect("transcript ref");

    let temp = tempdir().expect("tempdir");
    let repo = temp.path().join("repo");
    init_repo(&repo);
    let store_root = temp.path().join("roger-store");

    let start_bin = temp.path().join("fake-copilot-start");
    let start_script = format!(
        "#!/bin/sh\nset -eu\nmkdir -p \"$(dirname \"$RR_COPILOT_SESSION_START_ARTIFACT\")\"\ncat > \"$RR_COPILOT_SESSION_START_ARTIFACT\" <<EOF\n{{\"hook\":\"session-start\",\"payload\":{{\"provider\":\"copilot\",\"session_id\":\"{}\",\"worktree_root\":\"$PWD\",\"launch_profile_id\":\"profile-open-pr\",\"artifact_refs\":[\"{}\"]}}}}\nEOF\nprintf 'Copilot CLI ready\\nsession_id={}\\n'\n",
        session_id, transcript_artifact_ref, session_id,
    );
    write_executable(&start_bin, &start_script);

    let reopen_bin = temp.path().join("fake-copilot-reopen");
    let reopen_script = format!(
        "#!/bin/sh\nset -eu\nmkdir -p \"$(dirname \"$RR_COPILOT_SESSION_START_ARTIFACT\")\"\ncat > \"$RR_COPILOT_SESSION_START_ARTIFACT\" <<EOF\n{{\"hook\":\"session-start\",\"payload\":{{\"provider\":\"copilot\",\"session_id\":\"{}\",\"worktree_root\":\"$PWD\",\"launch_profile_id\":\"profile-open-pr\",\"artifact_refs\":[\"{}\"]}}}}\nEOF\nprintf 'Resumed existing Copilot session\\nsession_id={}\\n'\n",
        resume_case["expected"]["verified_session_id"]
            .as_str()
            .expect("resume session id"),
        transcript_artifact_ref,
        resume_case["expected"]["verified_session_id"]
            .as_str()
            .expect("resume session id"),
    );
    write_executable(&reopen_bin, &reopen_script);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: store_root.clone(),
        opencode_bin: "opencode".to_owned(),
    };

    let gate = "1".to_owned();
    let start_bin_path = start_bin.to_string_lossy().to_string();
    let review = run_with_env_overrides(
        &[
            (ENV_COPILOT_ADMISSION_GATE, Some(gate.as_str())),
            (ENV_COPILOT_BIN, Some(start_bin_path.as_str())),
        ],
        || {
            run(
                &[
                    "review".to_owned(),
                    "--pr".to_owned(),
                    "42".to_owned(),
                    "--provider".to_owned(),
                    "copilot".to_owned(),
                    "--robot".to_owned(),
                ],
                &runtime,
            )
        },
    );
    assert_eq!(review.exit_code, 5, "{}", review.stderr);
    let review_payload = parse_robot(&review.stdout);
    let roger_session_id = review_payload["data"]["session_id"]
        .as_str()
        .expect("roger session id")
        .to_owned();

    let reopen_bin_path = reopen_bin.to_string_lossy().to_string();
    let resume = run_with_env_overrides(
        &[
            (ENV_COPILOT_ADMISSION_GATE, Some(gate.as_str())),
            (ENV_COPILOT_BIN, Some(reopen_bin_path.as_str())),
        ],
        || {
            run(
                &["resume".to_owned(), "--pr".to_owned(), "42".to_owned()],
                &runtime,
            )
        },
    );
    assert_eq!(resume.exit_code, 0, "{}", resume.stderr);
    assert!(
        resume.stderr.contains("bounded tier-b path"),
        "unexpected resume stderr: {}",
        resume.stderr
    );

    let store = RogerStore::open(&store_root).expect("open store");
    let session = store
        .review_session(&roger_session_id)
        .expect("read session")
        .expect("session exists");
    assert_eq!(
        session
            .session_locator
            .as_ref()
            .expect("session locator")
            .session_id,
        session_id
    );
    let invocation_context: Value = serde_json::from_str(
        &session
            .session_locator
            .as_ref()
            .expect("session locator")
            .invocation_context_json,
    )
    .expect("parse invocation context");
    assert_eq!(invocation_context["mode"], "resume");

    let attempts = store
        .launch_attempts_for_session(&roger_session_id)
        .expect("launch attempts");
    let attempt = attempts.last().expect("resume attempt");
    assert_eq!(attempt.state, LaunchAttemptState::VerifiedReopened);
    assert_eq!(attempt.provider_session_id.as_deref(), Some(session_id));
}

#[test]
fn rr_resume_copilot_feature_gate_reseeds_from_bundle_and_updates_locator() {
    let launch_case = fixture_case(
        "tests/fixtures/fixture_copilot_launch_resume/copilot_launch_resume_cases.json",
        "launch_review_verified_session_id",
        "cases",
    );
    let resume_case = fixture_case(
        "tests/fixtures/fixture_copilot_launch_resume/copilot_launch_resume_cases.json",
        "resume_stale_locator_reseed",
        "cases",
    );
    let nominal_transcript = fixture_case(
        "tests/fixtures/fixture_copilot_transcript_capture/copilot_transcript_refs.json",
        "transcript_capture_nominal",
        "records",
    );
    let degraded_transcript = fixture_case(
        "tests/fixtures/fixture_copilot_transcript_capture/copilot_transcript_refs.json",
        "transcript_capture_truncated_payload",
        "records",
    );
    let start_session_id = launch_case["expected"]["verified_session_id"]
        .as_str()
        .expect("start session id");
    let reseeded_session_id = resume_case["expected"]["verified_session_id"]
        .as_str()
        .expect("reseeded session id");
    let start_artifact_ref = nominal_transcript["artifact_refs"][0]
        .as_str()
        .expect("start transcript ref");
    let reseed_artifact_ref = degraded_transcript["artifact_refs"][0]
        .as_str()
        .expect("reseed transcript ref");

    let temp = tempdir().expect("tempdir");
    let repo = temp.path().join("repo");
    init_repo(&repo);
    let store_root = temp.path().join("roger-store");
    let start_bin = temp.path().join("fake-copilot-start");
    let start_script = format!(
        "#!/bin/sh\nset -eu\nmkdir -p \"$(dirname \"$RR_COPILOT_SESSION_START_ARTIFACT\")\"\ncat > \"$RR_COPILOT_SESSION_START_ARTIFACT\" <<EOF\n{{\"hook\":\"session-start\",\"payload\":{{\"provider\":\"copilot\",\"session_id\":\"{}\",\"worktree_root\":\"$PWD\",\"launch_profile_id\":\"profile-open-pr\",\"artifact_refs\":[\"{}\"]}}}}\nEOF\nprintf 'Copilot CLI ready\\nsession_id={}\\n'\n",
        start_session_id, start_artifact_ref, start_session_id,
    );
    write_executable(&start_bin, &start_script);

    let reseed_bin = temp.path().join("fake-copilot-reseed");
    let reseed_script = format!(
        "#!/bin/sh\nset -eu\nif [ \"${{1-}}\" = \"--resume\" ]; then\n  printf 'session cp-live-001 not found\\n' >&2\n  exit 2\nfi\nmkdir -p \"$(dirname \"$RR_COPILOT_SESSION_START_ARTIFACT\")\"\ncat > \"$RR_COPILOT_SESSION_START_ARTIFACT\" <<EOF\n{{\"hook\":\"session-start\",\"payload\":{{\"provider\":\"copilot\",\"session_id\":\"{}\",\"worktree_root\":\"$PWD\",\"launch_profile_id\":\"profile-open-pr\",\"artifact_refs\":[\"{}\"]}}}}\nEOF\nprintf 'Reseeded from Roger bundle\\nsession_id={}\\n'\n",
        reseeded_session_id, reseed_artifact_ref, reseeded_session_id,
    );
    write_executable(&reseed_bin, &reseed_script);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: store_root.clone(),
        opencode_bin: "opencode".to_owned(),
    };

    let gate = "1".to_owned();
    let start_bin_path = start_bin.to_string_lossy().to_string();
    let review = run_with_env_overrides(
        &[
            (ENV_COPILOT_ADMISSION_GATE, Some(gate.as_str())),
            (ENV_COPILOT_BIN, Some(start_bin_path.as_str())),
        ],
        || {
            run(
                &[
                    "review".to_owned(),
                    "--pr".to_owned(),
                    "42".to_owned(),
                    "--provider".to_owned(),
                    "copilot".to_owned(),
                    "--robot".to_owned(),
                ],
                &runtime,
            )
        },
    );
    assert_eq!(review.exit_code, 5, "{}", review.stderr);
    let review_payload = parse_robot(&review.stdout);
    let session_id = review_payload["data"]["session_id"]
        .as_str()
        .expect("roger session id")
        .to_owned();

    let reseed_bin_path = reseed_bin.to_string_lossy().to_string();
    let resume = run_with_env_overrides(
        &[
            (ENV_COPILOT_ADMISSION_GATE, Some(gate.as_str())),
            (ENV_COPILOT_BIN, Some(reseed_bin_path.as_str())),
        ],
        || {
            run(
                &["resume".to_owned(), "--pr".to_owned(), "42".to_owned()],
                &runtime,
            )
        },
    );
    assert_eq!(resume.exit_code, 0, "{}", resume.stderr);
    assert!(
        resume.stdout.contains("resume completed"),
        "unexpected resume stdout: {}",
        resume.stdout
    );
    assert!(
        resume.stderr.contains("bounded tier-b path"),
        "unexpected resume stderr: {}",
        resume.stderr
    );
    let status =
        run_with_env_overrides(&[(ENV_COPILOT_ADMISSION_GATE, Some(gate.as_str()))], || {
            run(
                &[
                    "status".to_owned(),
                    "--session".to_owned(),
                    session_id.clone(),
                    "--robot".to_owned(),
                ],
                &runtime,
            )
        });
    assert_eq!(status.exit_code, 0, "{}", status.stderr);
    let status_payload = parse_robot(&status.stdout);
    assert_eq!(
        status_payload["data"]["provider_capability"]["audit_artifact_classes"],
        serde_json::json!(["copilot_tool_denial", "copilot_transcript_reference"])
    );

    let store = RogerStore::open(&store_root).expect("open store");
    let session = store
        .review_session(&session_id)
        .expect("read session")
        .expect("session exists");
    assert_eq!(
        session
            .session_locator
            .as_ref()
            .expect("session locator")
            .session_id,
        reseeded_session_id
    );
    let invocation_context: Value = serde_json::from_str(
        &session
            .session_locator
            .as_ref()
            .expect("session locator")
            .invocation_context_json,
    )
    .expect("parse invocation context");
    assert_eq!(invocation_context["mode"], "resume_reseed");
    assert!(
        invocation_context["artifact_refs"]
            .as_array()
            .expect("artifact refs")
            .iter()
            .any(|value| value.as_str() == Some(reseed_artifact_ref))
    );

    let attempts = store
        .launch_attempts_for_session(&session_id)
        .expect("launch attempts");
    let attempt = attempts.last().expect("resume attempt");
    assert_eq!(attempt.state, LaunchAttemptState::VerifiedReseeded);
    assert_eq!(
        attempt.provider_session_id.as_deref(),
        Some(reseeded_session_id)
    );
}

#[test]
fn rr_return_copilot_feature_gate_reopens_bound_session_into_roger() {
    let launch_case = fixture_case(
        "tests/fixtures/fixture_copilot_launch_resume/copilot_launch_resume_cases.json",
        "launch_review_verified_session_id",
        "cases",
    );
    let transcript = fixture_case(
        "tests/fixtures/fixture_copilot_transcript_capture/copilot_transcript_refs.json",
        "transcript_capture_nominal",
        "records",
    );
    let session_id = launch_case["expected"]["verified_session_id"]
        .as_str()
        .expect("launch session id");
    let transcript_artifact_ref = transcript["artifact_refs"][0]
        .as_str()
        .expect("transcript ref");

    let temp = tempdir().expect("tempdir");
    let repo = temp.path().join("repo");
    init_repo(&repo);
    let store_root = temp.path().join("roger-store");

    let start_bin = temp.path().join("fake-copilot-start");
    let start_script = format!(
        "#!/bin/sh\nset -eu\nmkdir -p \"$(dirname \"$RR_COPILOT_SESSION_START_ARTIFACT\")\"\ncat > \"$RR_COPILOT_SESSION_START_ARTIFACT\" <<EOF\n{{\"hook\":\"session-start\",\"payload\":{{\"provider\":\"copilot\",\"session_id\":\"{}\",\"worktree_root\":\"$PWD\",\"launch_profile_id\":\"profile-open-pr\",\"artifact_refs\":[\"{}\"]}}}}\nEOF\nprintf 'Copilot CLI ready\\nsession_id={}\\n'\n",
        session_id, transcript_artifact_ref, session_id,
    );
    write_executable(&start_bin, &start_script);

    let reopen_bin = temp.path().join("fake-copilot-return");
    let reopen_script = format!(
        "#!/bin/sh\nset -eu\nmkdir -p \"$(dirname \"$RR_COPILOT_SESSION_START_ARTIFACT\")\"\ncat > \"$RR_COPILOT_SESSION_START_ARTIFACT\" <<EOF\n{{\"hook\":\"session-start\",\"payload\":{{\"provider\":\"copilot\",\"session_id\":\"{}\",\"worktree_root\":\"$PWD\",\"launch_profile_id\":\"profile-open-pr\",\"artifact_refs\":[\"{}\"]}}}}\nEOF\nprintf 'Returned Copilot session to Roger\\nsession_id={}\\n'\n",
        session_id, transcript_artifact_ref, session_id,
    );
    write_executable(&reopen_bin, &reopen_script);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: store_root.clone(),
        opencode_bin: "opencode".to_owned(),
    };

    let gate = "1".to_owned();
    let start_bin_path = start_bin.to_string_lossy().to_string();
    let review = run_with_env_overrides(
        &[
            (ENV_COPILOT_ADMISSION_GATE, Some(gate.as_str())),
            (ENV_COPILOT_BIN, Some(start_bin_path.as_str())),
        ],
        || {
            run(
                &[
                    "review".to_owned(),
                    "--pr".to_owned(),
                    "42".to_owned(),
                    "--provider".to_owned(),
                    "copilot".to_owned(),
                    "--robot".to_owned(),
                ],
                &runtime,
            )
        },
    );
    assert_eq!(review.exit_code, 5, "{}", review.stderr);
    let review_payload = parse_robot(&review.stdout);
    let roger_session_id = review_payload["data"]["session_id"]
        .as_str()
        .expect("roger session id")
        .to_owned();

    let reopen_bin_path = reopen_bin.to_string_lossy().to_string();
    let ret = run_with_env_overrides(
        &[
            (ENV_COPILOT_ADMISSION_GATE, Some(gate.as_str())),
            (ENV_COPILOT_BIN, Some(reopen_bin_path.as_str())),
        ],
        || {
            run(
                &[
                    "return".to_owned(),
                    "--pr".to_owned(),
                    "42".to_owned(),
                    "--robot".to_owned(),
                ],
                &runtime,
            )
        },
    );
    assert_eq!(
        ret.exit_code, 0,
        "stdout:\n{}\n\nstderr:\n{}",
        ret.stdout, ret.stderr
    );
    let payload = parse_robot(&ret.stdout);
    assert_eq!(payload["outcome"], "complete");
    assert_eq!(payload["data"]["return_path"], "reopened_by_locator");
    assert_eq!(payload["data"]["continuity_quality"], "usable");
    assert_eq!(
        payload["data"]["decision_reason"],
        "copilot_reopened_by_locator"
    );
    assert_eq!(
        payload["data"]["provider_capability"]["support_tier"],
        "tier_b_feature_gated"
    );
    assert_eq!(
        payload["data"]["provider_capability"]["supports"]["return"],
        true
    );

    let store = RogerStore::open(&store_root).expect("open store");
    let session = store
        .review_session(&roger_session_id)
        .expect("read session")
        .expect("session exists");
    assert_eq!(session.attention_state, "returned_to_roger");
    assert_eq!(session.continuity_state, "return:usable");
    assert_eq!(
        session
            .session_locator
            .as_ref()
            .expect("session locator")
            .session_id,
        session_id
    );

    let attempts = store
        .launch_attempts_for_session(&roger_session_id)
        .expect("launch attempts");
    let attempt = attempts.last().expect("return attempt");
    assert_eq!(attempt.state, LaunchAttemptState::VerifiedReopened);
    assert_eq!(attempt.provider_session_id.as_deref(), Some(session_id));
}

#[test]
fn robot_docs_and_dry_run_surface_copilot_as_gated_tier_b_when_enabled() {
    let temp = tempdir().expect("tempdir");
    let repo = temp.path().join("repo");
    init_repo(&repo);
    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };

    let gate = "1".to_owned();
    let guide =
        run_with_env_overrides(&[(ENV_COPILOT_ADMISSION_GATE, Some(gate.as_str()))], || {
            run(
                &[
                    "robot-docs".to_owned(),
                    "guide".to_owned(),
                    "--robot".to_owned(),
                ],
                &runtime,
            )
        });
    assert_eq!(guide.exit_code, 0, "{}", guide.stderr);
    let guide_payload = parse_robot(&guide.stdout);
    let guide_items = guide_payload["data"]["items"]
        .as_array()
        .expect("guide items");
    let provider_support = guide_items
        .iter()
        .find(|item| item["kind"] == "provider_support")
        .expect("provider support item");
    assert!(
        provider_support["summary"]
            .as_str()
            .expect("summary")
            .contains("GitHub Copilot CLI is feature-gated as a bounded tier-b continuity path")
    );
    let live_review_providers = provider_support["live_review_providers"]
        .as_array()
        .expect("live review providers");
    let copilot_entry = live_review_providers
        .iter()
        .find(|entry| entry["provider"] == "copilot")
        .expect("copilot live entry");
    assert_eq!(copilot_entry["status"], "feature_gated_bounded_live");
    assert_eq!(copilot_entry["tier"], "tier_b_feature_gated");
    assert_eq!(copilot_entry["supports"]["resume_reseed"], true);
    assert_eq!(copilot_entry["supports"]["resume_reopen"], true);
    assert_eq!(copilot_entry["supports"]["return"], true);
    assert!(
        provider_support["planned_not_live_providers"]
            .as_array()
            .expect("planned not live providers")
            .is_empty()
    );

    let commands =
        run_with_env_overrides(&[(ENV_COPILOT_ADMISSION_GATE, Some(gate.as_str()))], || {
            run(
                &[
                    "robot-docs".to_owned(),
                    "commands".to_owned(),
                    "--robot".to_owned(),
                ],
                &runtime,
            )
        });
    assert_eq!(commands.exit_code, 0, "{}", commands.stderr);
    let commands_payload = parse_robot(&commands.stdout);
    let commands_items = commands_payload["data"]["items"]
        .as_array()
        .expect("commands items");
    let review_command = commands_items
        .iter()
        .find(|item| item["command"] == "rr review --dry-run")
        .expect("review command");
    assert!(
        review_command["supported_providers"]
            .as_array()
            .expect("supported providers")
            .iter()
            .any(|value| value.as_str() == Some("copilot"))
    );
    assert!(
        review_command["planned_not_live_providers"]
            .as_array()
            .expect("planned providers")
            .is_empty()
    );
}

#[test]
fn workspace_copilot_assets_exist_and_session_start_hook_emits_roger_artifact() {
    let workspace = workspace_root();
    let required_paths = [
        ".github/copilot-instructions.md",
        ".github/instructions/rust.instructions.md",
        ".github/instructions/extension.instructions.md",
        ".github/hooks/roger-review.json",
        "scripts/copilot-hooks/common.sh",
        "scripts/copilot-hooks/session-start.sh",
        "scripts/copilot-hooks/session-start.ps1",
        "scripts/copilot-hooks/user-prompt.sh",
        "scripts/copilot-hooks/pre-tool-use.sh",
        "scripts/copilot-hooks/post-tool-use.sh",
        "scripts/copilot-hooks/agent-stop.sh",
        "scripts/copilot-hooks/session-end.sh",
    ];
    for relative in required_paths {
        assert!(
            workspace.join(relative).is_file(),
            "missing required Copilot repo asset {relative}"
        );
    }

    let hook_config: Value = serde_json::from_slice(
        &fs::read(workspace.join(".github/hooks/roger-review.json")).expect("read hook config"),
    )
    .expect("parse hook config");
    assert_eq!(hook_config["version"], 1);

    let expected_hooks = [
        "sessionStart",
        "userPromptSubmitted",
        "preToolUse",
        "postToolUse",
        "agentStop",
        "sessionEnd",
    ];
    for hook_name in expected_hooks {
        let entries = hook_config["hooks"][hook_name].as_array();
        assert!(entries.is_some(), "missing hook entry {hook_name}");
        let entries = entries.expect("hook entry presence already asserted");
        assert!(
            !entries.is_empty(),
            "hook entry {hook_name} should declare at least one command"
        );
        let bash_command = entries[0]["bash"].as_str();
        assert!(
            bash_command.is_some(),
            "hook {hook_name} is missing a bash command"
        );
        let bash_command = bash_command.expect("bash command presence already asserted");
        let relative_path = bash_command
            .strip_prefix("./")
            .unwrap_or(bash_command)
            .split_whitespace()
            .next()
            .expect("hook bash command token");
        assert!(
            workspace.join(relative_path).is_file(),
            "hook {hook_name} references missing asset {relative_path}"
        );
    }

    let temp = tempdir().expect("tempdir");
    let worktree_root = normalized_path_string(temp.path());
    let artifact_path = temp
        .path()
        .join("provider/copilot/launch-attempts/attempt-123/session-start.json");
    let script_path = workspace.join("scripts/copilot-hooks/session-start.sh");
    let payload = serde_json::json!({
        "sessionId": "cp-live-001",
        "timestamp": 1704614400000_u64,
        "cwd": worktree_root,
        "source": "new",
        "initialPrompt": "Start a Roger review"
    });

    let mut child = Command::new("sh")
        .arg(script_path)
        .env("RR_COPILOT_SESSION_START_ARTIFACT", &artifact_path)
        .env("RR_COPILOT_ATTEMPT_ID", "attempt-123")
        .env(
            "RR_COPILOT_WORKTREE_ROOT",
            normalized_path_string(temp.path()),
        )
        .env("RR_COPILOT_POLICY_PROFILE_DIGEST", "sha256:policy")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn session-start hook");
    child
        .stdin
        .as_mut()
        .expect("session-start stdin")
        .write_all(
            serde_json::to_string(&payload)
                .expect("serialize hook payload")
                .as_bytes(),
        )
        .expect("write hook payload");
    let status = child.wait().expect("wait for hook");
    assert!(status.success(), "session-start hook failed: {status}");

    let emitted: Value =
        serde_json::from_slice(&fs::read(&artifact_path).expect("read emitted artifact"))
            .expect("parse emitted artifact");
    assert_eq!(emitted["hook"], "session-start");
    assert_eq!(emitted["payload"]["provider"], "copilot");
    assert_eq!(emitted["payload"]["session_id"], "cp-live-001");
    assert_eq!(
        emitted["payload"]["worktree_root"],
        normalized_path_string(temp.path())
    );
    assert_eq!(emitted["payload"]["launch_profile_id"], "profile-open-pr");
    assert_eq!(emitted["payload"]["attempt_nonce"], "attempt-123");
    assert_eq!(emitted["payload"]["policy_digest"], "sha256:policy");
}
