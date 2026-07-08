#![cfg(unix)]

//! Executes the review_readonly pre-tool-use allowlist matcher unit-test
//! harness (scripts/copilot-hooks/test_pre_tool_use.sh) so the fail-closed
//! carve-out for the Roger worker transport is guarded in CI.
//!
//! (bead rr-copilot-policy-rr-agent-carveout-6182)

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("packages parent")
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn ensure_executable(path: &Path) {
    let mut permissions = std::fs::metadata(path)
        .expect("stat script")
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions).expect("chmod script");
}

#[test]
fn pre_tool_use_allowlist_matcher_unit_tests_pass() {
    let root = workspace_root();
    let harness = root.join("scripts/copilot-hooks/test_pre_tool_use.sh");
    let hook = root.join("scripts/copilot-hooks/pre-tool-use.sh");
    assert!(harness.is_file(), "missing matcher test harness");
    assert!(hook.is_file(), "missing pre-tool-use hook");
    ensure_executable(&harness);
    ensure_executable(&hook);

    let output = Command::new("sh")
        .arg(&harness)
        .output()
        .expect("run pre-tool-use matcher harness");

    assert!(
        output.status.success(),
        "pre-tool-use matcher harness failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("pre-tool-use matcher tests passed"),
        "matcher harness did not report success: {stdout}"
    );
    assert!(
        !stdout.contains("FAIL -"),
        "matcher harness reported failing cases: {stdout}"
    );
}
