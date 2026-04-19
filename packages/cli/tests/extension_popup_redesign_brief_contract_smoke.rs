#![cfg(unix)]

use std::fs;
use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("packages parent")
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn normalize_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn assert_contains_all(text: &str, fragments: &[&str], context: &str) {
    let normalized_text = normalize_whitespace(text);
    for fragment in fragments {
        let normalized_fragment = normalize_whitespace(fragment);
        assert!(
            normalized_text.contains(&normalized_fragment),
            "{context} missing fragment:\n{fragment}\n\nFull text:\n{text}"
        );
    }
}

#[test]
fn popup_redesign_brief_captures_required_hierarchy_and_identity_rules() {
    let brief_path = workspace_root().join("docs/extension-popup-redesign-brief.md");
    let brief = fs::read_to_string(&brief_path).expect("read extension popup redesign brief");

    assert_contains_all(
        &brief,
        &[
            "## Information Architecture",
            "## Action Hierarchy And Button Variants",
            "## Copy And States",
            "## Info Affordance (Build/Version And Supplemental Guidance)",
            "## Visual Language Split",
            "The inline build/version row is removed from the primary card body.",
            "Tooltip-only delivery is not sufficient for critical metadata",
            "GitHub-native cues (must remain):",
            "Roger-specific metallic cues (allowed and required):",
            "returning to full Signal Beacon shell styling",
        ],
        "extension popup redesign brief",
    );
}
