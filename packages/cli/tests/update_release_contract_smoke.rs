#![cfg(unix)]

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
    let digest = Sha256::digest(bytes);
    format!("{digest:x}")
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
        _ => "",
    };
    assert!(
        !target.is_empty(),
        "unsupported host target for update_release_contract_smoke: {:?}",
        (std::env::consts::OS, std::env::consts::ARCH)
    );
    target
}

fn alternate_release_target(exclude: &str) -> &'static str {
    [
        "aarch64-unknown-linux-gnu",
        "x86_64-unknown-linux-gnu",
        "aarch64-apple-darwin",
        "x86_64-apple-darwin",
    ]
    .into_iter()
    .find(|candidate| *candidate != exclude)
    .expect("alternate release target distinct from host")
}

#[derive(Debug)]
struct ReleaseFixture {
    tag: String,
}

fn create_release_fixture(
    download_root: &Path,
    version: &str,
    channel: &str,
    target: &str,
    binary_source: &Path,
    metadata_checksums_name: &str,
    published_checksums_name: &str,
) -> ReleaseFixture {
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

    let archive_path = release_dir.join(&archive_name);
    let archive_sha = sha256_file(&archive_path);
    let checksums_entry_name = if metadata_checksums_name == published_checksums_name {
        archive_name.clone()
    } else {
        format!("core-{target}/{archive_name}")
    };
    fs::write(
        release_dir.join(published_checksums_name),
        format!("{archive_sha}  {checksums_entry_name}\n"),
    )
    .expect("write checksums manifest");

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
            "checksums_name": metadata_checksums_name,
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
                "store_schema_version": 10,
                "min_supported_store_schema": 0,
                "auto_migrate_from": 0,
                "migration_policy": "binary_only",
                "migration_class_max_auto": "none",
                "sidecar_generation": "v1",
                "backup_required": true,
            },
        }),
    );

    ReleaseFixture { tag }
}

fn parse_robot(stdout: &[u8]) -> Value {
    serde_json::from_slice(stdout).expect("valid robot payload")
}

#[test]
fn installed_binary_update_dry_run_handles_legacy_checksums_and_emits_release_backed_guidance() {
    let Some(current_version) = option_env!("ROGER_RELEASE_VERSION") else {
        eprintln!(
            "skipping update_release_contract_smoke because ROGER_RELEASE_VERSION was not embedded"
        );
        return;
    };
    let current_channel = option_env!("ROGER_RELEASE_CHANNEL").unwrap_or("stable");
    let next_version = "2026.04.09";
    assert_ne!(
        current_version, next_version,
        "test requires a next release distinct from the embedded current version"
    );

    let temp = tempdir().expect("tempdir");
    let api_root = temp.path().join("api/repos/cdilga/roger-reviewer");
    let download_root_fs = temp.path().join("releases/download");
    let install_root = temp.path().join("install/bin");
    fs::create_dir_all(&download_root_fs).expect("create download root");

    let rr_bin = PathBuf::from(env!("CARGO_BIN_EXE_rr"));
    let target = host_update_target();
    let legacy_checksums_name = format!("roger-reviewer-{current_version}-checksums.txt");
    let current_release = create_release_fixture(
        &download_root_fs,
        current_version,
        current_channel,
        target,
        &rr_bin,
        &legacy_checksums_name,
        "SHA256SUMS",
    );
    let next_release = create_release_fixture(
        &download_root_fs,
        next_version,
        current_channel,
        target,
        &rr_bin,
        &format!("roger-reviewer-{next_version}-checksums.txt"),
        "SHA256SUMS",
    );

    let installer = workspace_root().join("scripts/release/rr-install.sh");
    let install_output = Command::new("bash")
        .arg(&installer)
        .arg("--version")
        .arg(current_version)
        .arg("--download-root")
        .arg(format!("file://{}", download_root_fs.to_string_lossy()))
        .arg("--install-dir")
        .arg(&install_root)
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

    let installed_rr = install_root.join("rr");
    assert!(installed_rr.exists(), "installer did not write rr binary");

    write_latest_release_pointer(&api_root, &current_release.tag);
    let same_version = Command::new(&installed_rr)
        .arg("update")
        .arg("--api-root")
        .arg(format!("file://{}", api_root.to_string_lossy()))
        .arg("--download-root")
        .arg(format!("file://{}", download_root_fs.to_string_lossy()))
        .arg("--dry-run")
        .arg("--robot")
        .env("RR_STORE_ROOT", temp.path().join("roger-store-noop"))
        .output()
        .expect("run same-version rr update");
    assert!(
        same_version.status.success(),
        "same-version dry-run failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&same_version.stdout),
        String::from_utf8_lossy(&same_version.stderr)
    );
    let same_payload = parse_robot(&same_version.stdout);
    assert_eq!(same_payload["outcome"], "empty");
    assert_eq!(same_payload["data"]["up_to_date"], true);
    assert_eq!(same_payload["data"]["current_channel"], current_channel);
    assert_eq!(
        same_payload["data"]["target_release"]["version"],
        Value::Null
    );

    write_latest_release_pointer(&api_root, &next_release.tag);
    let update_dry_run = Command::new(&installed_rr)
        .arg("update")
        .arg("--api-root")
        .arg(format!("file://{}", api_root.to_string_lossy()))
        .arg("--download-root")
        .arg(format!("file://{}", download_root_fs.to_string_lossy()))
        .arg("--dry-run")
        .arg("--robot")
        .env("RR_STORE_ROOT", temp.path().join("roger-store-upgrade"))
        .output()
        .expect("run cross-version rr update");
    assert!(
        update_dry_run.status.success(),
        "cross-version dry-run failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&update_dry_run.stdout),
        String::from_utf8_lossy(&update_dry_run.stderr)
    );
    let update_payload = parse_robot(&update_dry_run.stdout);
    assert_eq!(update_payload["outcome"], "complete");
    assert_eq!(
        update_payload["data"]["current_release"]["channel"],
        current_channel
    );
    assert_eq!(
        update_payload["data"]["target_release"]["channel"],
        current_channel
    );
    assert_eq!(
        update_payload["data"]["target_release"]["version"],
        next_version
    );
    assert_eq!(update_payload["data"]["checksums_legacy_fallback"], true);

    let recommended = update_payload["data"]["recommended_install_command"]
        .as_str()
        .expect("recommended install command");
    assert!(
        recommended.contains(
            "https://github.com/cdilga/roger-reviewer/releases/download/v2026.04.09/rr-install.sh"
        ),
        "expected release-hosted reinstall guidance, got: {recommended}"
    );
    assert!(
        !recommended.contains("scripts/release/"),
        "recommended install command should not use repo-relative scripts: {recommended}"
    );
}

#[test]
fn installed_binary_update_dry_run_preserves_explicit_target_override_truth() {
    let Some(current_version) = option_env!("ROGER_RELEASE_VERSION") else {
        eprintln!(
            "skipping update_release_contract_smoke because ROGER_RELEASE_VERSION was not embedded"
        );
        return;
    };
    let current_channel = option_env!("ROGER_RELEASE_CHANNEL").unwrap_or("stable");
    let next_version = "2026.04.10";

    let temp = tempdir().expect("tempdir");
    let api_root = temp.path().join("api/repos/cdilga/roger-reviewer");
    let download_root_fs = temp.path().join("releases/download");
    let install_root = temp.path().join("install/bin");
    fs::create_dir_all(&download_root_fs).expect("create download root");

    let rr_bin = PathBuf::from(env!("CARGO_BIN_EXE_rr"));
    let install_target = host_update_target();
    let explicit_target = alternate_release_target(install_target);
    let current_release = create_release_fixture(
        &download_root_fs,
        current_version,
        current_channel,
        install_target,
        &rr_bin,
        &format!("roger-reviewer-{current_version}-checksums.txt"),
        "SHA256SUMS",
    );
    let next_release = create_release_fixture(
        &download_root_fs,
        next_version,
        current_channel,
        explicit_target,
        &rr_bin,
        &format!("roger-reviewer-{next_version}-checksums.txt"),
        "SHA256SUMS",
    );

    let installer = workspace_root().join("scripts/release/rr-install.sh");
    let install_output = Command::new("bash")
        .arg(&installer)
        .arg("--version")
        .arg(current_version)
        .arg("--download-root")
        .arg(format!("file://{}", download_root_fs.to_string_lossy()))
        .arg("--install-dir")
        .arg(&install_root)
        .arg("--target")
        .arg(install_target)
        .output()
        .expect("run installer");
    assert!(
        install_output.status.success(),
        "installer failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&install_output.stdout),
        String::from_utf8_lossy(&install_output.stderr)
    );

    let installed_rr = install_root.join("rr");
    assert!(installed_rr.exists(), "installer did not write rr binary");

    write_latest_release_pointer(&api_root, &current_release.tag);
    write_latest_release_pointer(&api_root, &next_release.tag);
    let update_dry_run = Command::new(&installed_rr)
        .arg("update")
        .arg("--api-root")
        .arg(format!("file://{}", api_root.to_string_lossy()))
        .arg("--download-root")
        .arg(format!("file://{}", download_root_fs.to_string_lossy()))
        .arg("--target")
        .arg(explicit_target)
        .arg("--dry-run")
        .arg("--robot")
        .env("RR_STORE_ROOT", temp.path().join("roger-store-aarch64"))
        .output()
        .expect("run explicit-target rr update");
    assert!(
        update_dry_run.status.success(),
        "explicit-target dry-run failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&update_dry_run.stdout),
        String::from_utf8_lossy(&update_dry_run.stderr)
    );
    let update_payload = parse_robot(&update_dry_run.stdout);
    assert_eq!(update_payload["outcome"], "complete");
    assert_eq!(update_payload["data"]["target"], explicit_target);
    assert_eq!(
        update_payload["data"]["current_release"]["channel"],
        current_channel
    );
    assert_eq!(
        update_payload["data"]["target_release"]["channel"],
        current_channel
    );
    assert_eq!(
        update_payload["data"]["target_release"]["version"],
        next_version
    );
}

#[test]
fn installed_binary_update_dry_run_honors_explicit_rc_channel_request() {
    let Some(current_version) = option_env!("ROGER_RELEASE_VERSION") else {
        eprintln!(
            "skipping update_release_contract_smoke because ROGER_RELEASE_VERSION was not embedded"
        );
        return;
    };
    let current_channel = option_env!("ROGER_RELEASE_CHANNEL").unwrap_or("stable");
    let rc_version = "2026.04.12-rc.1";

    let temp = tempdir().expect("tempdir");
    let api_root = temp.path().join("api/repos/cdilga/roger-reviewer");
    let download_root_fs = temp.path().join("releases/download");
    let install_root = temp.path().join("install/bin");
    fs::create_dir_all(&download_root_fs).expect("create download root");

    let rr_bin = PathBuf::from(env!("CARGO_BIN_EXE_rr"));
    let target = host_update_target();
    create_release_fixture(
        &download_root_fs,
        current_version,
        current_channel,
        target,
        &rr_bin,
        &format!("roger-reviewer-{current_version}-checksums.txt"),
        "SHA256SUMS",
    );
    let rc_release = create_release_fixture(
        &download_root_fs,
        rc_version,
        "rc",
        target,
        &rr_bin,
        &format!("roger-reviewer-{rc_version}-checksums.txt"),
        "SHA256SUMS",
    );

    let installer = workspace_root().join("scripts/release/rr-install.sh");
    let install_output = Command::new("bash")
        .arg(&installer)
        .arg("--version")
        .arg(current_version)
        .arg("--download-root")
        .arg(format!("file://{}", download_root_fs.to_string_lossy()))
        .arg("--install-dir")
        .arg(&install_root)
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

    let installed_rr = install_root.join("rr");
    assert!(installed_rr.exists(), "installer did not write rr binary");

    write_latest_release_pointer(&api_root, &rc_release.tag);
    let update_dry_run = Command::new(&installed_rr)
        .arg("update")
        .arg("--api-root")
        .arg(format!("file://{}", api_root.to_string_lossy()))
        .arg("--download-root")
        .arg(format!("file://{}", download_root_fs.to_string_lossy()))
        .arg("--channel")
        .arg("rc")
        .arg("--version")
        .arg(rc_version)
        .arg("--dry-run")
        .arg("--robot")
        .env("RR_STORE_ROOT", temp.path().join("roger-store-rc"))
        .output()
        .expect("run explicit-rc rr update");
    assert!(
        update_dry_run.status.success(),
        "explicit-rc dry-run failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&update_dry_run.stdout),
        String::from_utf8_lossy(&update_dry_run.stderr)
    );
    let update_payload = parse_robot(&update_dry_run.stdout);
    assert_eq!(update_payload["outcome"], "complete");
    assert_eq!(
        update_payload["data"]["current_release"]["channel"],
        current_channel
    );
    assert_eq!(update_payload["data"]["target_release"]["channel"], "rc");
    assert_eq!(
        update_payload["data"]["target_release"]["version"],
        rc_version
    );

    let recommended = update_payload["data"]["recommended_install_command"]
        .as_str()
        .expect("recommended install command");
    assert!(
        recommended.contains("--channel rc"),
        "expected explicit rc channel in reinstall guidance, got: {recommended}"
    );
}

#[test]
fn installed_binary_update_blocks_renamed_binary_with_release_backed_guidance() {
    let Some(current_version) = option_env!("ROGER_RELEASE_VERSION") else {
        eprintln!(
            "skipping update_release_contract_smoke because ROGER_RELEASE_VERSION was not embedded"
        );
        return;
    };
    let current_channel = option_env!("ROGER_RELEASE_CHANNEL").unwrap_or("stable");
    let next_version = "2026.04.11";

    let temp = tempdir().expect("tempdir");
    let api_root = temp.path().join("api/repos/cdilga/roger-reviewer");
    let download_root_fs = temp.path().join("releases/download");
    let install_root = temp.path().join("install/bin");
    fs::create_dir_all(&download_root_fs).expect("create download root");

    let rr_bin = PathBuf::from(env!("CARGO_BIN_EXE_rr"));
    let target = host_update_target();
    create_release_fixture(
        &download_root_fs,
        current_version,
        current_channel,
        target,
        &rr_bin,
        &format!("roger-reviewer-{current_version}-checksums.txt"),
        "SHA256SUMS",
    );
    let next_release = create_release_fixture(
        &download_root_fs,
        next_version,
        current_channel,
        target,
        &rr_bin,
        &format!("roger-reviewer-{next_version}-checksums.txt"),
        "SHA256SUMS",
    );

    let installer = workspace_root().join("scripts/release/rr-install.sh");
    let install_output = Command::new("bash")
        .arg(&installer)
        .arg("--version")
        .arg(current_version)
        .arg("--download-root")
        .arg(format!("file://{}", download_root_fs.to_string_lossy()))
        .arg("--install-dir")
        .arg(&install_root)
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

    let installed_rr = install_root.join("rr");
    assert!(installed_rr.exists(), "installer did not write rr binary");
    let renamed_rr = install_root.join("rr-wrapper");
    fs::rename(&installed_rr, &renamed_rr).expect("rename installed binary");

    write_latest_release_pointer(&api_root, &next_release.tag);
    let blocked_update = Command::new(&renamed_rr)
        .arg("update")
        .arg("--api-root")
        .arg(format!("file://{}", api_root.to_string_lossy()))
        .arg("--download-root")
        .arg(format!("file://{}", download_root_fs.to_string_lossy()))
        .arg("--yes")
        .arg("--robot")
        .env("RR_STORE_ROOT", temp.path().join("roger-store-renamed"))
        .output()
        .expect("run renamed-binary rr update");
    assert_eq!(
        blocked_update.status.code(),
        Some(3),
        "renamed-binary update should fail closed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&blocked_update.stdout),
        String::from_utf8_lossy(&blocked_update.stderr)
    );
    let blocked_payload = parse_robot(&blocked_update.stdout);
    assert_eq!(blocked_payload["outcome"], "blocked");
    assert_eq!(
        blocked_payload["data"]["reason_code"],
        "unsupported_install_layout"
    );

    let recommended = blocked_payload["data"]["recommended_reinstall_command"]
        .as_str()
        .expect("recommended install command");
    assert!(
        recommended.contains(
            "https://github.com/cdilga/roger-reviewer/releases/download/v2026.04.11/rr-install.sh"
        ),
        "expected release-hosted reinstall guidance, got: {recommended}"
    );
    assert!(
        !recommended.contains("scripts/release/"),
        "renamed-binary guidance should not point at repo-relative scripts: {recommended}"
    );
}
