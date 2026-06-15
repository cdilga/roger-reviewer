#![cfg(unix)]

use roger_cli::{CliRuntime, run};
use roger_storage::{RogerStore, SemanticAssetManifest};
use serde_json::Value;
use sha2::Digest;
use std::fs;
use tempfile::tempdir;

fn parse_robot(stdout: &str) -> Value {
    serde_json::from_str(stdout).expect("robot payload")
}

fn runtime_with_store(store_root: std::path::PathBuf, cwd: std::path::PathBuf) -> CliRuntime {
    CliRuntime {
        cwd,
        store_root,
        opencode_bin: "opencode".to_owned(),
    }
}

fn run_rr(args: &[&str], runtime: &CliRuntime) -> roger_cli::CliRunResult {
    let argv = args.iter().map(|value| value.to_string()).collect::<Vec<_>>();
    run(&argv, runtime)
}

#[test]
fn rr_assets_status_reports_disabled_posture_without_manifest() {
    let temp = tempdir().expect("tempdir");
    let runtime = runtime_with_store(temp.path().join("store"), temp.path().to_path_buf());

    let status = run_rr(&["assets", "status", "--robot"], &runtime);
    // No manifest yet: status is a healthy empty outcome, never degraded.
    assert_eq!(status.exit_code, 0, "{}", status.stderr);
    let payload = parse_robot(&status.stdout);
    assert_eq!(payload["schema_id"], "rr.robot.assets.v1");
    assert_eq!(payload["command"], "rr assets");
    assert_eq!(payload["outcome"], "empty");
    assert_eq!(payload["data"]["manifest_present"], false);
    assert_eq!(payload["data"]["operational"], false);
    assert_eq!(payload["data"]["posture"], "disabled_pending_install");
}

#[test]
fn rr_assets_verify_is_empty_without_manifest() {
    let temp = tempdir().expect("tempdir");
    let runtime = runtime_with_store(temp.path().join("store"), temp.path().to_path_buf());

    let verify = run_rr(&["assets", "verify", "--robot"], &runtime);
    assert_eq!(verify.exit_code, 0, "{}", verify.stderr);
    let payload = parse_robot(&verify.stdout);
    assert_eq!(payload["outcome"], "empty");
    assert_eq!(payload["data"]["verified"], false);
}

#[test]
fn rr_assets_verify_succeeds_for_matching_manifest() {
    let temp = tempdir().expect("tempdir");
    let store_root = temp.path().join("store");
    let runtime = runtime_with_store(store_root.clone(), temp.path().to_path_buf());

    let store = RogerStore::open(&store_root).expect("open store");
    let payload = b"semantic-model-bytes";
    let digest = format!("sha256:{:x}", sha2::Sha256::digest(payload));
    let rel = "semantic-default.onnx";
    let artifact = store.layout().semantic_asset_root().join(rel);
    fs::create_dir_all(artifact.parent().expect("asset parent")).expect("mk asset dir");
    fs::write(&artifact, payload).expect("write artifact");
    store
        .install_semantic_asset_manifest(&SemanticAssetManifest {
            schema_version: 1,
            package_id: "semantic-default".to_owned(),
            revision: "2026.06.14".to_owned(),
            artifact_rel_path: rel.to_owned(),
            artifact_digest: digest,
            installed_at: 1_750_000_000,
        })
        .expect("install manifest");
    drop(store);

    let verify = run_rr(&["assets", "verify", "--robot"], &runtime);
    assert_eq!(verify.exit_code, 0, "{}", verify.stderr);
    let value = parse_robot(&verify.stdout);
    assert_eq!(value["outcome"], "complete");
    assert_eq!(value["data"]["verified"], true);
}

#[test]
fn rr_assets_verify_reports_repair_needed_on_digest_mismatch() {
    let temp = tempdir().expect("tempdir");
    let store_root = temp.path().join("store");
    let runtime = runtime_with_store(store_root.clone(), temp.path().to_path_buf());

    let store = RogerStore::open(&store_root).expect("open store");
    let rel = "semantic-default.onnx";
    let artifact = store.layout().semantic_asset_root().join(rel);
    fs::create_dir_all(artifact.parent().expect("asset parent")).expect("mk asset dir");
    fs::write(&artifact, b"actual-bytes").expect("write artifact");
    store
        .install_semantic_asset_manifest(&SemanticAssetManifest {
            schema_version: 1,
            package_id: "semantic-default".to_owned(),
            revision: "2026.06.14".to_owned(),
            artifact_rel_path: rel.to_owned(),
            // Deliberately wrong digest -> fail-closed repair_needed.
            artifact_digest: "sha256:deadbeef".to_owned(),
            installed_at: 1_750_000_000,
        })
        .expect("install manifest");
    drop(store);

    let verify = run_rr(&["assets", "verify", "--robot"], &runtime);
    assert_eq!(verify.exit_code, 4, "{}", verify.stderr);
    let value = parse_robot(&verify.stdout);
    assert_eq!(value["outcome"], "repair_needed");
    assert_eq!(value["data"]["reason_code"], "semantic_asset_digest_mismatch");
}

#[test]
fn rr_assets_install_blocks_on_local_unpublished_build() {
    let temp = tempdir().expect("tempdir");
    let runtime = runtime_with_store(temp.path().join("store"), temp.path().to_path_buf());

    // No --version and no embedded release metadata in the test binary: install
    // must fail-closed (blocked), never silently write an unverified manifest.
    let install = run_rr(
        &["assets", "install", "--asset", "semantic-default", "--robot"],
        &runtime,
    );
    assert_eq!(install.exit_code, 3, "{}", install.stderr);
    let value = parse_robot(&install.stdout);
    assert_eq!(value["outcome"], "blocked");
    assert_eq!(
        value["data"]["reason_code"],
        "local_or_unpublished_build"
    );
}

#[test]
fn rr_assets_install_rejects_unknown_asset() {
    let temp = tempdir().expect("tempdir");
    let runtime = runtime_with_store(temp.path().join("store"), temp.path().to_path_buf());

    let install = run_rr(
        &["assets", "install", "--asset", "totally-bogus", "--robot"],
        &runtime,
    );
    assert_eq!(install.exit_code, 3, "{}", install.stderr);
    let value = parse_robot(&install.stdout);
    assert_eq!(value["outcome"], "blocked");
    assert_eq!(value["data"]["reason_code"], "unknown_asset_package");
}

#[test]
fn rr_assets_requires_subcommand() {
    let temp = tempdir().expect("tempdir");
    let runtime = runtime_with_store(temp.path().join("store"), temp.path().to_path_buf());

    let bare = run_rr(&["assets"], &runtime);
    assert_ne!(bare.exit_code, 0);
}
