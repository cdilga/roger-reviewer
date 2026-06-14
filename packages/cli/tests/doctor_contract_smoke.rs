#![cfg(unix)]

use roger_cli::{CliRuntime, run};
use roger_config::cli_defaults::{ENV_COPILOT_BIN, ENV_OPENCODE_BIN};
use roger_session_copilot::ENV_COPILOT_ADMISSION_GATE;
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

fn assert_sha256_prefixed(value: &Value, label: &str) {
    let digest = value.as_str();
    assert!(digest.is_some(), "{label} must be a digest string");
    let digest = digest.expect("digest presence already asserted");
    assert!(
        digest.starts_with("sha256:"),
        "{label} must start with sha256:, got {digest}"
    );
}

fn check_by_id<'a>(payload: &'a Value, id: &str) -> &'a Value {
    payload["data"]["checks"]
        .as_array()
        .expect("doctor checks")
        .iter()
        .find(|item| item["id"] == id)
        .map_or_else(
            || {
                assert!(false, "missing doctor check {id}");
                unreachable!()
            },
            |item| item,
        )
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
}

fn write_fake_binary(path: &Path) {
    fs::write(path, "#!/bin/sh\nexit 0\n").expect("write fake binary");
    let mut perms = fs::metadata(path).expect("stat fake binary").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).expect("chmod fake binary");
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

fn runtime_for(workspace: PathBuf, store_root: PathBuf) -> CliRuntime {
    CliRuntime {
        cwd: workspace,
        store_root,
        opencode_bin: "opencode".to_owned(),
    }
}

#[test]
fn rr_doctor_reports_missing_store_as_auto_bootstrap_pending() {
    let temp = tempdir().expect("tempdir");
    let workspace = temp.path().join("workspace");
    init_git_repo(&workspace);

    let fake_opencode = temp.path().join("fake-opencode");
    write_fake_binary(&fake_opencode);
    let runtime = runtime_for(workspace, temp.path().join("roger-store"));

    let opencode_bin = fake_opencode.to_string_lossy().to_string();
    let result = run_with_env_overrides(&[(ENV_OPENCODE_BIN, Some(opencode_bin.as_str()))], || {
        run(
            &[
                "doctor".to_owned(),
                "--provider".to_owned(),
                "opencode".to_owned(),
                "--robot".to_owned(),
            ],
            &runtime,
        )
    });

    // A missing store on a fresh machine is the expected auto-bootstrap
    // state, not an error: rr init is optional and every store-backed
    // command bootstraps on first use.
    let payload = parse_robot(&result.stdout);
    assert_eq!(payload["data"]["provider"], "opencode");

    let store_root_check = check_by_id(&payload, "store_root_present");
    assert_eq!(store_root_check["status"], "deferred");
    assert_eq!(
        store_root_check["reason_code"],
        "store_auto_bootstrap_pending"
    );
    let db_check = check_by_id(&payload, "store_db_readable");
    assert_eq!(db_check["status"], "deferred");
    assert_eq!(db_check["reason_code"], "store_auto_bootstrap_pending");
    assert!(
        !payload["repair_actions"]
            .as_array()
            .expect("repair actions")
            .iter()
            .any(|item| {
                item.as_str()
                    .unwrap_or_default()
                    .contains("run rr init to bootstrap")
            }),
        "fresh-machine doctor must not demand rr init: {}",
        payload["repair_actions"]
    );
}

#[test]
fn rr_doctor_reports_verified_local_checks_and_deferred_auth_for_live_provider() {
    let temp = tempdir().expect("tempdir");
    let workspace = temp.path().join("workspace");
    init_git_repo(&workspace);

    let fake_opencode = temp.path().join("fake-opencode");
    write_fake_binary(&fake_opencode);
    let runtime = runtime_for(workspace, temp.path().join("roger-store"));

    let opencode_bin = fake_opencode.to_string_lossy().to_string();
    let (init_result, doctor_result) =
        run_with_env_overrides(&[(ENV_OPENCODE_BIN, Some(opencode_bin.as_str()))], || {
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
    assert_eq!(payload["data"]["provider"], "opencode");
    assert_eq!(payload["data"]["summary"]["blocked_count"], 0);
    assert!(
        payload["data"]["summary"]["deferred_count"]
            .as_u64()
            .expect("deferred count")
            >= 1
    );

    assert_eq!(
        check_by_id(&payload, "store_root_present")["status"],
        "verified"
    );
    assert_eq!(
        check_by_id(&payload, "store_db_readable")["status"],
        "verified"
    );
    assert_eq!(
        check_by_id(&payload, "provider_binary_present")["status"],
        "verified"
    );
    assert_eq!(
        payload["data"]["provider_capability"]["hook_profile"]["contract_version"],
        "none"
    );
    assert_eq!(
        payload["data"]["provider_capability"]["custom_instructions"]["contract_version"],
        "prompt_preset_contract.v1"
    );
    assert_eq!(
        payload["data"]["provider_capability"]["mcp_posture"]["posture"],
        "not_declared"
    );
    assert!(
        payload["data"]["provider_capability"]["policy_profile_digest_sha256"]
            .as_str()
            .unwrap_or_default()
            .starts_with("sha256:")
    );
    assert!(payload["data"]["routine_surface"]["worktree_root"].is_string());
    let auth_check = check_by_id(&payload, "provider_auth_preflight");
    assert_eq!(auth_check["status"], "deferred");
    assert_eq!(auth_check["reason_code"], "auth_not_preflight_verified");
}

#[test]
fn rr_doctor_warns_about_legacy_opencode_home_config() {
    let temp = tempdir().expect("tempdir");
    let workspace = temp.path().join("workspace");
    init_git_repo(&workspace);

    let fake_opencode = temp.path().join("opencode");
    write_fake_binary(&fake_opencode);
    let runtime = runtime_for(workspace, temp.path().join("roger-store"));

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

    let opencode_bin = fake_opencode.to_string_lossy().to_string();
    let home = legacy_home.to_string_lossy().to_string();
    let (init_result, doctor_result) = run_with_env_overrides(
        &[
            (ENV_OPENCODE_BIN, Some(opencode_bin.as_str())),
            ("HOME", Some(home.as_str())),
        ],
        || {
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
        },
    );

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
fn rr_doctor_surfaces_planned_copilot_lane_and_missing_admission_assets() {
    let temp = tempdir().expect("tempdir");
    let workspace = temp.path().join("workspace");
    init_git_repo(&workspace);

    let fake_copilot = temp.path().join("fake-copilot");
    write_fake_binary(&fake_copilot);
    let runtime = runtime_for(workspace, temp.path().join("roger-store"));

    let copilot_bin = fake_copilot.to_string_lossy().to_string();
    let (init_result, doctor_result) =
        run_with_env_overrides(&[(ENV_COPILOT_BIN, Some(copilot_bin.as_str()))], || {
            let init = run(&["init".to_owned(), "--robot".to_owned()], &runtime);
            let doctor = run(
                &[
                    "doctor".to_owned(),
                    "--provider".to_owned(),
                    "copilot".to_owned(),
                    "--robot".to_owned(),
                ],
                &runtime,
            );
            (init, doctor)
        });

    assert_eq!(init_result.exit_code, 0, "{}", init_result.stderr);
    assert_eq!(doctor_result.exit_code, 3, "{}", doctor_result.stderr);

    let payload = parse_robot(&doctor_result.stdout);
    assert_eq!(payload["outcome"], "blocked");
    assert_eq!(payload["data"]["provider"], "copilot");
    // With the feature gate OFF, copilot is honestly classified as a documented
    // feature-gated tier-b provider that is disabled-but-enableable, NOT the
    // planned_not_live/tier_a_planned classification reserved for genuinely
    // planned providers.
    assert_eq!(
        payload["data"]["provider_capability"]["status"],
        "feature_gated_disabled"
    );
    assert_eq!(
        payload["data"]["provider_capability"]["support_tier"],
        "tier_b_feature_gated"
    );
    assert_eq!(
        payload["data"]["provider_capability"]["surface_class"],
        "review_bounded"
    );
    assert_eq!(
        payload["data"]["provider_capability"]["policy_profile"]["id"],
        "review_readonly"
    );
    assert_eq!(
        payload["data"]["provider_capability"]["capability_provenance"]["source"],
        "providers.copilot.support_matrix"
    );
    assert_sha256_prefixed(
        &payload["data"]["provider_capability"]["policy_profile_digest_sha256"],
        "doctor policy profile digest",
    );
    assert_eq!(
        payload["data"]["provider_capability"]["hook_profile"]["contract_version"],
        "copilot_review_readonly_hooks.v1"
    );
    assert_sha256_prefixed(
        &payload["data"]["provider_capability"]["hook_profile_digest_sha256"],
        "doctor hook profile digest",
    );
    assert_eq!(
        payload["data"]["provider_capability"]["custom_instructions"]["contract_version"],
        "copilot_review_readonly_instructions.v1"
    );
    assert_sha256_prefixed(
        &payload["data"]["provider_capability"]["custom_instructions_digest_sha256"],
        "doctor custom instructions digest",
    );
    assert_eq!(
        payload["data"]["provider_capability"]["mcp_posture"]["posture"],
        "restricted"
    );
    assert_eq!(
        payload["data"]["provider_capability"]["mcp_posture"]["builtin_github_mcp"]["state"],
        "denied"
    );
    assert_eq!(
        payload["data"]["provider_capability"]["mcp_posture"]["builtin_github_mcp"]["denied"],
        serde_json::json!(true)
    );
    assert_eq!(
        payload["data"]["provider_capability"]["mcp_posture"]["broad_mcp_access"]["denied"],
        serde_json::json!(true)
    );
    assert_eq!(
        payload["data"]["provider_capability"]["audit_artifact_classes"],
        serde_json::json!(["copilot_tool_denial", "copilot_transcript_reference"])
    );
    let surfaced_worktree_root = PathBuf::from(
        payload["data"]["routine_surface"]["worktree_root"]
            .as_str()
            .expect("doctor routine_surface worktree_root"),
    );
    assert_eq!(
        fs::canonicalize(&surfaced_worktree_root).expect("canonical surfaced worktree root"),
        fs::canonicalize(&runtime.cwd).expect("canonical runtime cwd")
    );
    assert_eq!(
        check_by_id(&payload, "provider_admission_state")["reason_code"],
        "provider_feature_gate_disabled"
    );
    assert_eq!(
        check_by_id(&payload, "copilot_instructions_present")["status"],
        "blocked"
    );
    // The repair names the documented enable step instead of steering the
    // operator to a different provider.
    assert!(
        payload["repair_actions"]
            .as_array()
            .expect("repair actions")
            .iter()
            .any(|item| item
                .as_str()
                .unwrap_or_default()
                .contains("RR_ENABLE_COPILOT_PROVIDER=1")),
        "repair must name the documented Copilot enable gate: {:?}",
        payload["repair_actions"]
    );
    assert!(
        !payload["repair_actions"]
            .as_array()
            .expect("repair actions")
            .iter()
            .any(|item| item
                .as_str()
                .unwrap_or_default()
                .contains("rr review --provider opencode|codex|gemini|claude")),
        "gate-off copilot repair must not steer to a different provider: {:?}",
        payload["repair_actions"]
    );
}

#[test]
fn rr_doctor_treats_gate_enabled_copilot_as_bounded_live_when_assets_are_present() {
    let temp = tempdir().expect("tempdir");
    let workspace = temp.path().join("workspace");
    init_git_repo(&workspace);

    fs::create_dir_all(workspace.join(".github/instructions"))
        .expect("create copilot instruction directory");
    fs::create_dir_all(workspace.join(".github/hooks")).expect("create copilot hook directory");
    fs::write(
        workspace.join(".github/copilot-instructions.md"),
        "# Roger Copilot instructions\n",
    )
    .expect("write copilot instructions");
    fs::write(
        workspace.join(".github/instructions/rust.instructions.md"),
        "Roger bounded Tier A instructions\n",
    )
    .expect("write copilot instruction asset");
    fs::write(
        workspace.join(".github/hooks/roger-review.json"),
        "{\n  \"version\": 1\n}\n",
    )
    .expect("write copilot hook asset");

    let fake_copilot = temp.path().join("fake-copilot");
    write_fake_binary(&fake_copilot);
    let runtime = runtime_for(workspace, temp.path().join("roger-store"));

    let gate = "1".to_owned();
    let copilot_bin = fake_copilot.to_string_lossy().to_string();
    let (init_result, doctor_result) = run_with_env_overrides(
        &[
            (ENV_COPILOT_ADMISSION_GATE, Some(gate.as_str())),
            (ENV_COPILOT_BIN, Some(copilot_bin.as_str())),
        ],
        || {
            let init = run(&["init".to_owned(), "--robot".to_owned()], &runtime);
            let doctor = run(
                &[
                    "doctor".to_owned(),
                    "--provider".to_owned(),
                    "copilot".to_owned(),
                    "--robot".to_owned(),
                ],
                &runtime,
            );
            (init, doctor)
        },
    );

    assert_eq!(init_result.exit_code, 0, "{}", init_result.stderr);
    assert_eq!(doctor_result.exit_code, 0, "{}", doctor_result.stderr);

    let payload = parse_robot(&doctor_result.stdout);
    assert_eq!(payload["outcome"], "complete");
    assert_eq!(payload["data"]["provider"], "copilot");
    assert_eq!(
        payload["data"]["provider_capability"]["status"],
        "feature_gated_bounded_live"
    );
    assert_eq!(
        payload["data"]["provider_capability"]["support_tier"],
        "tier_b_feature_gated"
    );
    assert_eq!(
        payload["data"]["provider_capability"]["notes"],
        "feature-gated bounded tier-b continuity path: verified start, locator/session-id reopen, rr return, and honest ResumeBundle reseed fallback; no default public live claim"
    );
    assert_eq!(
        payload["data"]["provider_capability"]["supports"]["review_start"],
        serde_json::json!(true)
    );
    assert_eq!(
        payload["data"]["provider_capability"]["supports"]["resume_reseed"],
        serde_json::json!(true)
    );
    assert_eq!(
        payload["data"]["provider_capability"]["supports"]["resume_reopen"],
        serde_json::json!(true)
    );
    assert_eq!(
        payload["data"]["provider_capability"]["supports"]["return"],
        serde_json::json!(true)
    );
    assert_eq!(
        payload["data"]["routine_surface"]["provider"]["status"],
        "feature_gated_bounded_live"
    );
    assert_eq!(
        check_by_id(&payload, "provider_doctor_support")["status"],
        "verified"
    );
    assert_eq!(
        check_by_id(&payload, "provider_admission_state")["status"],
        "verified"
    );
    assert_eq!(
        check_by_id(&payload, "provider_binary_present")["status"],
        "verified"
    );
    assert_eq!(
        check_by_id(&payload, "copilot_instructions_present")["status"],
        "verified"
    );
    assert_eq!(
        check_by_id(&payload, "copilot_instruction_assets_present")["status"],
        "verified"
    );
    assert_eq!(
        check_by_id(&payload, "copilot_hook_assets_present")["status"],
        "verified"
    );
    assert_eq!(
        check_by_id(&payload, "provider_auth_preflight")["status"],
        "deferred"
    );
    assert!(
        !payload["repair_actions"]
            .as_array()
            .expect("repair actions")
            .iter()
            .any(|item| item
                .as_str()
                .unwrap_or_default()
                .contains("rr review --provider opencode|codex|gemini|claude"))
    );
}
