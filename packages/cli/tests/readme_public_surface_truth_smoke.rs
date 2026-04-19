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

fn assert_contains_none(text: &str, fragments: &[&str], context: &str) {
    let normalized_text = normalize_whitespace(text);
    for fragment in fragments {
        let normalized_fragment = normalize_whitespace(fragment);
        assert!(
            !normalized_text.contains(&normalized_fragment),
            "{context} unexpectedly contained fragment:\n{fragment}\n\nFull text:\n{text}"
        );
    }
}

#[test]
fn readme_public_surface_truth_matches_install_and_browser_contracts() {
    let readme = fs::read_to_string(workspace_root().join("README.md")).expect("read README");
    assert_contains_all(
        &readme,
        &[
            "https://github.com/cdilga/roger-reviewer/releases/latest/download/rr-install.sh",
            "https://github.com/cdilga/roger-reviewer/releases/latest/download/rr-install.ps1",
            "supported browsers: Chrome, Edge, and Brave",
            "`rr extension setup --browser <edge|chrome|brave>`",
            "`rr extension doctor --browser <edge|chrome|brave>`",
            "Roger Reviewer uses an issue-first contribution path.",
        ],
        "README public packaging contract",
    );
    assert_contains_none(
        &readme,
        &[
            "ngrok",
            "current state as of",
            "sprint snapshot",
            "release-smoke completion",
        ],
        "README public packaging hygiene",
    );

    let release_workflow =
        fs::read_to_string(workspace_root().join(".github/workflows/release.yml"))
            .expect("read release workflow");
    assert_contains_all(
        &release_workflow,
        &[
            "rr-install.sh",
            "rr-install.ps1",
            "Verify release assets from in-workflow artifacts",
        ],
        "release workflow installer asset contract",
    );
}
