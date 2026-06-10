#![cfg(unix)]

use roger_app_core::{
    ContinuityQuality, HarnessAdapter, LaunchAction, LaunchIntent, ResumeBundle,
    ResumeBundleProfile, ReviewTarget, Surface,
};
use roger_cli::{CliRuntime, run};
use roger_session_opencode::OpenCodeAdapter;
use roger_storage::{CreateReviewSession, CreateSessionLaunchBinding, LaunchSurface, RogerStore};
use serde_json::Value;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use tempfile::tempdir;

fn parse_robot(stdout: &str) -> Value {
    serde_json::from_str(stdout).expect("robot payload")
}

fn init_git_repo(path: &Path) {
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

    // The return-flow contract infers the review target repository from the
    // git remote so `rr return --pr <n>` can resolve a unique session without
    // an explicit --repo/--session. Without this remote, session inference is
    // ambiguous and the picker blocks the command.
    let remote = Command::new("git")
        .arg("-C")
        .arg(path)
        .args([
            "remote",
            "add",
            "origin",
            "https://github.com/owner/repo.git",
        ])
        .output()
        .expect("git remote add");
    assert!(
        remote.status.success(),
        "git remote add failed: {}",
        String::from_utf8_lossy(&remote.stderr)
    );
}

fn runtime_for(workspace: PathBuf, store_root: PathBuf, opencode_bin: PathBuf) -> CliRuntime {
    CliRuntime {
        cwd: workspace,
        store_root,
        opencode_bin: opencode_bin.to_string_lossy().to_string(),
    }
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

#[test]
fn doctor_reports_legacy_opencode_home_config_guidance() {
    let temp = tempdir().expect("tempdir");
    let workspace = temp.path().join("workspace");
    init_git_repo(&workspace);

    let opencode_bin = temp.path().join("opencode");
    fs::write(&opencode_bin, "#!/bin/sh\nexit 0\n").expect("write fake binary");
    let mut perms = fs::metadata(&opencode_bin)
        .expect("stat fake binary")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&opencode_bin, perms).expect("chmod fake binary");

    let runtime = runtime_for(workspace, temp.path().join("roger-store"), opencode_bin);

    let legacy_home = temp.path().join("home");
    let legacy_dir = legacy_home.join(".opencode");
    fs::create_dir_all(&legacy_dir).expect("create legacy opencode dir");
    fs::write(
        legacy_dir.join("opencode.json"),
        r#"{
  "mcpServers": {
    "mcp-agent-mail": {
      "type": "remote",
      "url": "http://127.0.0.1:8765/mcp/"
    }
  }
}
"#,
    )
    .expect("write legacy opencode config");

    let home = legacy_home.to_string_lossy().to_string();
    let (init_result, doctor_result) =
        run_with_env_overrides(&[("HOME", Some(home.as_str()))], || {
            let init = run(&["init".to_owned(), "--robot".to_owned()], &runtime);
            let doctor = run(
                &[
                    "doctor".to_owned(),
                    "--provider".to_owned(),
                    "opencode".to_owned(),
                    "--robot".to_owned(),
                ],
                &runtime,
            );
            (init, doctor)
        });

    assert_eq!(init_result.exit_code, 0, "{}", init_result.stderr);
    assert_eq!(doctor_result.exit_code, 0, "{}", doctor_result.stderr);
    let payload = parse_robot(&doctor_result.stdout);
    assert_eq!(payload["outcome"], "complete");
    assert!(
        doctor_result
            .stderr
            .contains("legacy OpenCode config detected"),
        "{}",
        doctor_result.stderr
    );
    assert!(
        payload["repair_actions"]
            .as_array()
            .expect("repair actions")
            .iter()
            .any(|item| item
                .as_str()
                .unwrap_or_default()
                .contains("migrate its top-level 'mcpServers' entries"))
    );
}

#[test]
fn return_filters_raw_legacy_warning_and_surfaces_roger_guidance() {
    let temp = tempdir().expect("tempdir");
    let repo = temp.path().join("workspace");
    init_git_repo(&repo);

    let opencode_bin = temp.path().join("opencode");
    fs::write(
        &opencode_bin,
        r#"#!/bin/sh
if [ "$1" = "--session" ]; then
  echo "Unrecognized key: mcpServers" >&2
  exit 0
fi
if [ "$1" = "export" ]; then
  echo "{}"
  exit 0
fi
exit 0
"#,
    )
    .expect("write stub binary");
    let mut perms = fs::metadata(&opencode_bin).expect("metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&opencode_bin, perms).expect("chmod stub binary");

    let runtime = runtime_for(repo, temp.path().join("roger-store"), opencode_bin);
    let target = sample_target(42);
    let adapter = OpenCodeAdapter::with_binary(runtime.opencode_bin.clone());
    let locator = adapter
        .start_session(&target, &sample_launch_intent(LaunchAction::StartReview))
        .expect("start locator");
    let binding_cwd = runtime.cwd.to_string_lossy().to_string();

    let store = RogerStore::open(&runtime.store_root).expect("open store");
    store
        .store_resume_bundle("bundle-dropout-legacy", &dropout_bundle(target.clone()))
        .expect("store bundle");
    store
        .create_review_session(CreateReviewSession {
            id: "session-dropout-legacy",
            review_target: &target,
            provider: "opencode",
            session_locator: Some(&locator),
            resume_bundle_artifact_id: Some("bundle-dropout-legacy"),
            continuity_state: "awaiting_return",
            attention_state: "awaiting_return",
            launch_profile_id: Some("profile-open-pr"),
        })
        .expect("create session");
    store
        .put_session_launch_binding(CreateSessionLaunchBinding {
            id: "binding-dropout-legacy",
            session_id: "session-dropout-legacy",
            repo_locator: &target.repository,
            review_target: Some(&target),
            surface: LaunchSurface::Cli,
            launch_profile_id: Some("profile-open-pr"),
            ui_target: Some("cli"),
            instance_preference: Some("reuse_if_possible"),
            cwd: Some(&binding_cwd),
            worktree_root: None,
        })
        .expect("create binding");

    let legacy_home = temp.path().join("home");
    let legacy_dir = legacy_home.join(".opencode");
    fs::create_dir_all(&legacy_dir).expect("create legacy opencode dir");
    fs::write(
        legacy_dir.join("opencode.json"),
        r#"{
  "mcpServers": {
    "mcp-agent-mail": {
      "type": "remote",
      "url": "http://127.0.0.1:8765/mcp/"
    }
  }
}
"#,
    )
    .expect("write legacy opencode config");
    let home = legacy_home.to_string_lossy().to_string();

    let ret = run_with_env_overrides(&[("HOME", Some(home.as_str()))], || {
        run(
            &[
                "return".to_owned(),
                "--pr".to_owned(),
                "42".to_owned(),
                "--robot".to_owned(),
            ],
            &runtime,
        )
    });

    assert_eq!(ret.exit_code, 0, "{}", ret.stderr);
    let payload = parse_robot(&ret.stdout);
    assert_eq!(payload["outcome"], "complete");
    assert_eq!(payload["data"]["return_path"], "rebound_existing_session");
    assert!(
        ret.stderr.contains("legacy OpenCode config detected"),
        "{}",
        ret.stderr
    );
    assert!(
        ret.stderr
            .lines()
            .all(|line| line != "Unrecognized key: mcpServers"),
        "{}",
        ret.stderr
    );
}
