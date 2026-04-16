use roger_validation::{
    persona_ownership_expected_bead_ids, persona_ownership_report,
    persona_recovery_expected_bead_ids, persona_recovery_report,
};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("packages parent")
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn temp_dir(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("roger-validation-{name}-{nonce}"));
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

#[test]
fn repo_foundation_persona_mapping_is_machine_derivable_from_suite_metadata() {
    let root = workspace_root();
    let temp = temp_dir("persona-ownership");
    let issues_jsonl = temp.join("issues.jsonl");
    let issues = persona_ownership_expected_bead_ids()
        .into_iter()
        .map(|id| format!(r#"{{"id":"{id}"}}"#))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&issues_jsonl, format!("{issues}\n")).expect("issues jsonl");

    let report = persona_ownership_report(root.join("tests/suites"), issues_jsonl)
        .expect("persona ownership report");

    assert!(
        report.ok(),
        "persona ownership mapping drifted: {report:#?}"
    );
}

#[test]
fn repo_recovery_persona_mapping_is_machine_derivable_from_suite_metadata() {
    let root = workspace_root();
    let temp = temp_dir("persona-recovery");
    let issues_jsonl = temp.join("issues.jsonl");
    let issues = persona_recovery_expected_bead_ids()
        .into_iter()
        .map(|id| format!(r#"{{"id":"{id}"}}"#))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&issues_jsonl, format!("{issues}\n")).expect("issues jsonl");

    let report = persona_recovery_report(root.join("tests/suites"), issues_jsonl)
        .expect("persona recovery report");

    assert!(report.ok(), "persona recovery mapping drifted: {report:#?}");
}
