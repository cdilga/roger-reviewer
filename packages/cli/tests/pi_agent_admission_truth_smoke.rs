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
fn pi_agent_admission_stays_deferred_and_non_live() {
    let plan = fs::read_to_string(workspace_root().join("docs/PLAN_FOR_ROGER_REVIEWER.md"))
        .expect("read canonical plan");
    assert_contains_all(
        &plan,
        &[
            "| Pi-Agent | Deferred future review harness candidate | No | No |",
            "Pi-Agent is outside the `0.1.0` support order",
            "later admission spike proves it belongs in the matrix",
        ],
        "canonical plan pi-agent admission posture",
    );

    let release_matrix =
        fs::read_to_string(workspace_root().join("docs/RELEASE_AND_TEST_MATRIX.md"))
            .expect("read release matrix");
    assert_contains_all(
        &release_matrix,
        &[
            "| Pi-Agent | Not in `0.1.0` |",
            "no `rr review --provider pi-agent`",
        ],
        "release matrix pi-agent non-live posture",
    );

    let spike_memo = fs::read_to_string(
        workspace_root().join("docs/operator-stability/rr-x51h.6.6.1-pi-agent-admission-spike.md"),
    )
    .expect("read rr-x51h.6.6.1 spike memo");
    assert_contains_all(
        &spike_memo,
        &[
            "Pi-Agent remains out of the live Roger support matrix.",
            "deferred Tier A candidate only",
            "No live support claim changes in this spike.",
        ],
        "rr-x51h.6.6.1 non-widening recommendation",
    );
}
