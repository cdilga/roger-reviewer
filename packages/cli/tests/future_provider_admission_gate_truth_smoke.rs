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
fn future_provider_admission_stays_gate_driven_and_non_widening() {
    let plan = fs::read_to_string(workspace_root().join("docs/PLAN_FOR_ROGER_REVIEWER.md"))
        .expect("read canonical plan");
    assert_contains_all(
        &plan,
        &[
            "future providers should be admitted by capability tier, not by one-off",
            "Pi-Agent is outside the `0.1.0` support order",
            "later admission spike proves it belongs in the matrix",
        ],
        "canonical plan future-provider admission posture",
    );

    let release_matrix =
        fs::read_to_string(workspace_root().join("docs/RELEASE_AND_TEST_MATRIX.md"))
            .expect("read release matrix");
    assert_contains_all(
        &release_matrix,
        &[
            "| Pi-Agent | Not in `0.1.0` |",
            "no live support claim, no `rr review --provider pi-agent`",
        ],
        "release matrix pi-agent non-live posture",
    );

    let spike_memo = fs::read_to_string(
        workspace_root().join("docs/operator-stability/rr-x51h.6.6-future-provider-admission-spike.md"),
    )
    .expect("read rr-x51h.6.6 spike memo");
    assert_contains_all(
        &spike_memo,
        &[
            "No provider beyond the current matrix is admitted by this spike.",
            "Minimum Proof Packet",
            "This spike defines future admission gates only. It does not change live provider",
        ],
        "rr-x51h.6.6 spike memo non-widening gate",
    );
}
