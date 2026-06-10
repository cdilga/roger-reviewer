#![cfg(unix)]

use roger_cli::{CliRuntime, run};
use roger_storage::{RogerStore, StorageLayout};
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use tempfile::tempdir;

fn parse_robot(stdout: &str) -> Value {
    serde_json::from_str(stdout).expect("robot payload")
}

fn path_labels(entries: &Value) -> HashSet<String> {
    entries
        .as_array()
        .expect("path list")
        .iter()
        .filter_map(|entry| entry.get("label").and_then(Value::as_str))
        .map(ToOwned::to_owned)
        .collect()
}

#[test]
fn rr_init_bootstraps_store_and_reports_truthful_follow_up() {
    let temp = tempdir().expect("tempdir");
    let workspace = temp.path().join("workspace");
    fs::create_dir_all(&workspace).expect("create workspace");
    let store_root = temp.path().join("roger-store");
    let runtime = CliRuntime {
        cwd: workspace,
        store_root: store_root.clone(),
        opencode_bin: "opencode".to_owned(),
    };

    let init = run(&["init".to_owned(), "--robot".to_owned()], &runtime);
    assert_eq!(init.exit_code, 0, "{}", init.stderr);
    let payload = parse_robot(&init.stdout);

    assert_eq!(payload["schema_id"], "rr.robot.init.v1");
    assert_eq!(payload["command"], "rr init");
    assert_eq!(payload["outcome"], "complete");

    let data = &payload["data"];
    assert_eq!(data["already_initialized"], false);
    assert_eq!(
        data["provider_follow_up"]["auth_or_install_verified"],
        false
    );
    assert_eq!(
        data["provider_follow_up"]["doctor_surface_status"],
        "available"
    );
    let guidance = data["provider_follow_up"]["guidance"]
        .as_array()
        .expect("provider guidance");
    assert!(
        guidance.iter().any(|line| {
            line.as_str()
                .unwrap_or_default()
                .contains("provider auth/install is not verified")
        }),
        "provider guidance should explicitly call out non-verified provider auth/install"
    );

    let created = path_labels(&data["created_paths"]);
    for expected in [
        "store_root",
        "db_path",
        "artifact_root",
        "sidecar_root",
        "bootstrap_root",
        "metadata_marker",
    ] {
        assert!(
            created.contains(expected),
            "expected created_paths to include {}",
            expected
        );
    }

    let marker_path = StorageLayout::under(&store_root)
        .root
        .join("bootstrap/init-marker.v1.json");
    assert!(marker_path.exists(), "init marker should be written");
    let marker: Value = serde_json::from_slice(&fs::read(&marker_path).expect("read init marker"))
        .expect("parse init marker");
    assert_eq!(marker["schema"], "rr.init.marker.v1");

    let store = RogerStore::open(&store_root).expect("open initialized store");
    let schema_version = store.schema_version().expect("read schema version");
    assert_eq!(data["schema_version"], Value::from(schema_version));
}

#[test]
fn rr_init_is_idempotent_for_existing_store() {
    let temp = tempdir().expect("tempdir");
    let workspace = temp.path().join("workspace");
    fs::create_dir_all(&workspace).expect("create workspace");
    let runtime = CliRuntime {
        cwd: workspace,
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };

    let first = run(&["init".to_owned(), "--robot".to_owned()], &runtime);
    assert_eq!(first.exit_code, 0, "{}", first.stderr);
    let first_payload = parse_robot(&first.stdout);

    let second = run(&["init".to_owned(), "--robot".to_owned()], &runtime);
    assert_eq!(second.exit_code, 0, "{}", second.stderr);
    let second_payload = parse_robot(&second.stdout);

    let first_data = &first_payload["data"];
    let second_data = &second_payload["data"];
    assert_eq!(second_data["already_initialized"], true);
    assert_eq!(
        second_data["metadata_marker"]["first_initialized_at"],
        first_data["metadata_marker"]["first_initialized_at"]
    );
    assert!(
        second_data["metadata_marker"]["last_initialized_at"]
            .as_i64()
            .expect("second last_initialized_at")
            >= first_data["metadata_marker"]["last_initialized_at"]
                .as_i64()
                .expect("first last_initialized_at")
    );

    let second_created = path_labels(&second_data["created_paths"]);
    assert!(
        second_created.is_empty(),
        "idempotent init should not report new created paths on rerun"
    );

    let second_existing = path_labels(&second_data["existing_paths"]);
    for expected in [
        "store_root",
        "db_path",
        "artifact_root",
        "sidecar_root",
        "bootstrap_root",
        "metadata_marker",
    ] {
        assert!(
            second_existing.contains(expected),
            "expected existing_paths to include {}",
            expected
        );
    }
}

#[test]
fn rr_init_rejects_non_init_flags() {
    let temp = tempdir().expect("tempdir");
    let runtime = CliRuntime {
        cwd: temp.path().join("workspace"),
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };

    let result = run(
        &[
            "init".to_owned(),
            "--pr".to_owned(),
            "7".to_owned(),
            "--robot".to_owned(),
        ],
        &runtime,
    );
    assert_eq!(result.exit_code, 2);
    assert!(
        result.stderr.contains("rr init only supports --robot"),
        "{}",
        result.stderr
    );
}
