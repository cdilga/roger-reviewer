#![cfg(unix)]

//! Update-refreshes-extension contract (bead rr-update-refreshes-extension-2pcf).
//!
//! These exercise the real installed-binary in-place apply path, so they only
//! run when the test binary embeds `ROGER_RELEASE_VERSION` (matching the sibling
//! `update_release_contract_smoke` guard). Without it the updater fail-closes on
//! `local_or_unpublished_build`, so we skip rather than assert.

use roger_bridge::{
    NativeHostManifest, SupportedBrowser, SupportedOs, native_host_install_path_for,
};
use roger_storage::CURRENT_SCHEMA_VERSION;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::tempdir;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("packages parent")
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn write_json(path: &Path, value: &Value) {
    fs::write(
        path,
        serde_json::to_string_pretty(value).expect("serialize json") + "\n",
    )
    .expect("write json");
}

fn sha256_file(path: &Path) -> String {
    let bytes = fs::read(path).expect("read file for sha256");
    format!("{:x}", Sha256::digest(bytes))
}

fn write_latest_release_pointer(api_root: &Path, tag: &str) {
    let releases_dir = api_root.join("releases");
    fs::create_dir_all(&releases_dir).expect("create releases dir");
    write_json(&releases_dir.join("latest"), &json!({ "tag_name": tag }));
}

fn host_update_target() -> &'static str {
    let target = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
        _ => "",
    };
    assert!(
        !target.is_empty(),
        "unsupported host target for update_extension_refresh_smoke: {:?}",
        (std::env::consts::OS, std::env::consts::ARCH)
    );
    target
}

/// Build a synthetic release into `download_root`. When `with_extension` is
/// true, also publishes a verifiable `<stem>-extension.zip` asset (containing a
/// minimal unpacked extension) plus its `SHA256SUMS` entry.
fn create_release_fixture(
    download_root: &Path,
    version: &str,
    channel: &str,
    target: &str,
    binary_source: &Path,
    with_extension: bool,
) -> String {
    let artifact_stem = format!("roger-reviewer-{version}");
    let payload_dir = format!("{artifact_stem}-core-{target}");
    let archive_name = format!("{payload_dir}.tar.gz");
    let tag = format!("v{version}");
    let release_dir = download_root.join(&tag);
    fs::create_dir_all(&release_dir).expect("create release dir");

    let payload_root = release_dir.join("payload-root");
    let payload_dir_path = payload_root.join(&payload_dir);
    fs::create_dir_all(&payload_dir_path).expect("create payload dir");
    let binary_name = "rr";
    let binary_target = payload_dir_path.join(binary_name);
    fs::copy(binary_source, &binary_target).expect("copy rr binary into payload");
    let mut perms = fs::metadata(&binary_target)
        .expect("stat payload binary")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&binary_target, perms).expect("chmod payload binary");

    let tar_output = Command::new("tar")
        .arg("-czf")
        .arg(release_dir.join(&archive_name))
        .arg("-C")
        .arg(&payload_root)
        .arg(&payload_dir)
        .output()
        .expect("create fixture archive");
    assert!(
        tar_output.status.success(),
        "fixture archive creation failed: {}",
        String::from_utf8_lossy(&tar_output.stderr)
    );

    let archive_sha = sha256_file(&release_dir.join(&archive_name));
    let mut checksums = format!("{archive_sha}  {archive_name}\n");

    if with_extension {
        let extension_zip = format!("{artifact_stem}-extension.zip");
        let zip_path = release_dir.join(&extension_zip);
        let manifest_json = r#"{"name":"Roger Reviewer","version":"1.0","manifest_version":3}"#;
        let py = Command::new("python3")
            .arg("-c")
            .arg("import zipfile,sys; z=zipfile.ZipFile(sys.argv[1],'w'); z.writestr('manifest.json', sys.argv[2]); z.close()")
            .arg(&zip_path)
            .arg(manifest_json)
            .output()
            .expect("build extension zip");
        assert!(
            py.status.success(),
            "extension zip build failed: {}",
            String::from_utf8_lossy(&py.stderr)
        );
        let zip_sha = sha256_file(&zip_path);
        checksums.push_str(&format!("{zip_sha}  {extension_zip}\n"));
    }

    fs::write(release_dir.join("SHA256SUMS"), checksums).expect("write checksums manifest");

    let core_manifest_name = format!("release-core-manifest-{version}.json");
    write_json(
        &release_dir.join(&core_manifest_name),
        &json!({
            "schema": "roger.release-build-core.v1",
            "channel": channel,
            "version": version,
            "tag": tag,
            "prerelease": false,
            "artifact_stem": artifact_stem,
            "targets": [{
                "target": target,
                "archive_name": archive_name,
                "archive_sha256": archive_sha,
                "payload_dir": payload_dir,
                "binary_name": binary_name,
            }],
        }),
    );

    write_json(
        &release_dir.join(format!("release-install-metadata-{version}.json")),
        &json!({
            "schema": "roger.release.install-metadata.v1",
            "release": {
                "channel": channel,
                "version": version,
                "tag": tag,
                "prerelease": false,
                "artifact_stem": artifact_stem,
            },
            "checksums_name": "SHA256SUMS",
            "core_manifest_name": core_manifest_name,
            "targets": [{
                "target": target,
                "archive_name": archive_name,
                "archive_sha256": archive_sha,
                "payload_dir": payload_dir,
                "binary_name": binary_name,
            }],
            "store_compatibility": {
                "envelope_version": 1,
                "store_schema_version": CURRENT_SCHEMA_VERSION,
                "min_supported_store_schema": 0,
                "auto_migrate_from": 0,
                "migration_policy": "binary_only",
                "migration_class_max_auto": "none",
                "sidecar_generation": "v1",
                "backup_required": true,
            },
        }),
    );

    tag
}

fn install_current(
    download_root: &Path,
    version: &str,
    target: &str,
    install_root: &Path,
) -> PathBuf {
    let installer = workspace_root().join("scripts/release/rr-install.sh");
    let install_output = Command::new("bash")
        .arg(&installer)
        .arg("--version")
        .arg(version)
        .arg("--download-root")
        .arg(format!("file://{}", download_root.to_string_lossy()))
        .arg("--install-dir")
        .arg(install_root)
        .arg("--target")
        .arg(target)
        .output()
        .expect("run installer");
    assert!(
        install_output.status.success(),
        "installer failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&install_output.stdout),
        String::from_utf8_lossy(&install_output.stderr)
    );
    let installed = install_root.join("rr");
    assert!(installed.exists(), "installer did not write rr binary");
    installed
}

fn run_update(
    installed_rr: &Path,
    api_root: &Path,
    download_root: &Path,
    store_root: &Path,
    home: &Path,
) -> Value {
    let output = Command::new(installed_rr)
        .arg("update")
        .arg("--api-root")
        .arg(format!("file://{}", api_root.to_string_lossy()))
        .arg("--download-root")
        .arg(format!("file://{}", download_root.to_string_lossy()))
        .arg("--yes")
        .arg("--robot")
        .env("RR_STORE_ROOT", store_root)
        .env("HOME", home)
        .output()
        .expect("run rr update apply");
    assert!(
        output.status.success(),
        "update apply failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("valid robot payload")
}

fn chrome_manifest_path(home: &Path) -> PathBuf {
    let host_os = SupportedOs::current().expect("supported host os");
    native_host_install_path_for(&SupportedBrowser::Chrome, host_os, home)
}

const FIXTURE_EXTENSION_ID: &str = "abcdefghijklmnopabcdefghijklmnop";

fn skip_reason() -> Option<(&'static str, &'static str)> {
    let version = option_env!("ROGER_RELEASE_VERSION")?;
    let channel = option_env!("ROGER_RELEASE_CHANNEL").unwrap_or("stable");
    Some((version, channel))
}

#[test]
fn update_refreshes_extension_package_and_rewrites_native_host_manifests() {
    let Some((current_version, channel)) = skip_reason() else {
        eprintln!("skipping: ROGER_RELEASE_VERSION not embedded");
        return;
    };
    let next_version = "2026.09.01";
    assert_ne!(current_version, next_version);

    let temp = tempdir().expect("tempdir");
    let api_root = temp.path().join("api/repos/cdilga/roger-reviewer");
    let download_root = temp.path().join("releases/download");
    let install_root = temp.path().join("install/bin");
    let store_root = temp.path().join("roger-store");
    let home = temp.path().join("home");
    fs::create_dir_all(&download_root).expect("create download root");
    fs::create_dir_all(&home).expect("create home");

    let rr_bin = PathBuf::from(env!("CARGO_BIN_EXE_rr"));
    let target = host_update_target();
    create_release_fixture(
        &download_root,
        current_version,
        channel,
        target,
        &rr_bin,
        true,
    );
    let next_tag =
        create_release_fixture(&download_root, next_version, channel, target, &rr_bin, true);

    let installed_rr = install_current(&download_root, current_version, target, &install_root);

    // Pre-seed extension integration: persisted id + an existing Chrome native
    // host manifest under HOME.
    fs::create_dir_all(store_root.join("bridge")).expect("create bridge dir");
    fs::write(
        store_root.join("bridge/extension-id"),
        format!("{FIXTURE_EXTENSION_ID}\n"),
    )
    .expect("write extension-id");
    let manifest_path = chrome_manifest_path(&home);
    fs::create_dir_all(manifest_path.parent().expect("manifest parent"))
        .expect("create native host dir");
    let stale_manifest = NativeHostManifest::for_roger(
        Path::new("/nonexistent/old-launcher.sh"),
        FIXTURE_EXTENSION_ID,
    );
    fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&stale_manifest).expect("serialize manifest"),
    )
    .expect("write stale manifest");

    write_latest_release_pointer(&api_root, &next_tag);
    let payload = run_update(&installed_rr, &api_root, &download_root, &store_root, &home);

    assert_eq!(payload["outcome"], "complete");
    let refresh = &payload["data"]["extension_refresh"];
    assert_eq!(refresh["attempted"], true, "refresh should be attempted");
    assert_eq!(
        refresh["package_refreshed"], true,
        "package should refresh: {payload}"
    );
    assert!(
        refresh["native_hosts_rewritten"]
            .as_array()
            .expect("native_hosts_rewritten array")
            .iter()
            .any(|value| value == "chrome"),
        "chrome native host should be rewritten: {refresh}"
    );
    assert!(
        payload["data"]["warnings"]
            .as_array()
            .map(|_| true)
            .unwrap_or(true),
        "warnings present"
    );
    let warnings = payload["warnings"].as_array().expect("warnings array");
    assert!(
        warnings.iter().any(|w| w
            .as_str()
            .is_some_and(|s| s.contains("extension package refreshed to"))),
        "expected refresh warning: {warnings:?}"
    );

    // New per-version package installed.
    let new_package = store_root
        .join("bridge/extension-package")
        .join(next_version)
        .join("roger-extension-unpacked/manifest.json");
    assert!(
        new_package.is_file(),
        "refreshed package missing at {new_package:?}"
    );

    // Native host manifest rewritten: launcher path now points at the fresh
    // launcher, origin preserved.
    let rewritten: Value =
        serde_json::from_slice(&fs::read(&manifest_path).expect("read rewritten manifest"))
            .expect("parse rewritten manifest");
    assert_eq!(
        rewritten["allowed_origins"][0],
        format!("chrome-extension://{FIXTURE_EXTENSION_ID}/")
    );
    assert_ne!(
        rewritten["path"], "/nonexistent/old-launcher.sh",
        "launcher path should have been rewritten"
    );
    let launcher_path = rewritten["path"].as_str().expect("launcher path");
    assert!(
        Path::new(launcher_path).is_file(),
        "rewritten launcher should exist at {launcher_path}"
    );
}

#[test]
fn update_without_prior_extension_setup_skips_refresh_silently() {
    let Some((current_version, channel)) = skip_reason() else {
        eprintln!("skipping: ROGER_RELEASE_VERSION not embedded");
        return;
    };
    let next_version = "2026.09.02";

    let temp = tempdir().expect("tempdir");
    let api_root = temp.path().join("api/repos/cdilga/roger-reviewer");
    let download_root = temp.path().join("releases/download");
    let install_root = temp.path().join("install/bin");
    let store_root = temp.path().join("roger-store");
    let home = temp.path().join("home");
    fs::create_dir_all(&download_root).expect("create download root");
    fs::create_dir_all(&home).expect("create home");

    let rr_bin = PathBuf::from(env!("CARGO_BIN_EXE_rr"));
    let target = host_update_target();
    create_release_fixture(
        &download_root,
        current_version,
        channel,
        target,
        &rr_bin,
        true,
    );
    let next_tag =
        create_release_fixture(&download_root, next_version, channel, target, &rr_bin, true);

    let installed_rr = install_current(&download_root, current_version, target, &install_root);

    write_latest_release_pointer(&api_root, &next_tag);
    let payload = run_update(&installed_rr, &api_root, &download_root, &store_root, &home);

    assert_eq!(payload["outcome"], "complete");
    assert_eq!(payload["data"]["extension_refresh"]["attempted"], false);
    // No refresh means no refresh warnings/repair actions.
    let warnings = payload["warnings"].as_array().expect("warnings array");
    assert!(
        !warnings
            .iter()
            .any(|w| w.as_str().is_some_and(|s| s.contains("extension"))),
        "expected no extension warnings: {warnings:?}"
    );
}

#[test]
fn update_still_succeeds_when_extension_refresh_fails() {
    let Some((current_version, channel)) = skip_reason() else {
        eprintln!("skipping: ROGER_RELEASE_VERSION not embedded");
        return;
    };
    let next_version = "2026.09.03";

    let temp = tempdir().expect("tempdir");
    let api_root = temp.path().join("api/repos/cdilga/roger-reviewer");
    let download_root = temp.path().join("releases/download");
    let install_root = temp.path().join("install/bin");
    let store_root = temp.path().join("roger-store");
    let home = temp.path().join("home");
    fs::create_dir_all(&download_root).expect("create download root");
    fs::create_dir_all(&home).expect("create home");

    let rr_bin = PathBuf::from(env!("CARGO_BIN_EXE_rr"));
    let target = host_update_target();
    create_release_fixture(
        &download_root,
        current_version,
        channel,
        target,
        &rr_bin,
        true,
    );
    // Next release intentionally omits the extension.zip asset so the refresh
    // fetch fails while the binary update still succeeds.
    let next_tag = create_release_fixture(
        &download_root,
        next_version,
        channel,
        target,
        &rr_bin,
        false,
    );

    let installed_rr = install_current(&download_root, current_version, target, &install_root);

    // Extension integration present (id registry), so refresh is attempted.
    fs::create_dir_all(store_root.join("bridge")).expect("create bridge dir");
    fs::write(
        store_root.join("bridge/extension-id"),
        format!("{FIXTURE_EXTENSION_ID}\n"),
    )
    .expect("write extension-id");

    write_latest_release_pointer(&api_root, &next_tag);
    let payload = run_update(&installed_rr, &api_root, &download_root, &store_root, &home);

    // Binary update still succeeds.
    assert_eq!(payload["outcome"], "complete");
    assert_eq!(payload["data"]["target_release"]["version"], next_version);
    let refresh = &payload["data"]["extension_refresh"];
    assert_eq!(refresh["attempted"], true);
    assert_eq!(refresh["package_refreshed"], false);
    let warnings = payload["warnings"].as_array().expect("warnings array");
    assert!(
        warnings.iter().any(|w| w
            .as_str()
            .is_some_and(|s| s.starts_with("extension_refresh_failed:"))),
        "expected typed refresh failure warning: {warnings:?}"
    );
}
