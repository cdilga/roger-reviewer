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
fn semantic_profile_evolution_stays_deferred_until_explicit_proof_gate() {
    let plan = fs::read_to_string(workspace_root().join("docs/PLAN_FOR_ROGER_REVIEWER.md"))
        .expect("read canonical plan");
    assert_contains_all(
        &plan,
        &[
            "after the first Roger-owned default semantic asset profile lands",
            "when should Roger add code-oriented, sparse, or rerank variants",
        ],
        "canonical plan semantic-profile open question",
    );

    let policy = fs::read_to_string(
        workspace_root().join("docs/SEARCH_MEMORY_LIFECYCLE_AND_SEMANTIC_ASSET_POLICY.md"),
    )
    .expect("read search/memory semantic policy");
    assert_contains_all(
        &policy,
        &[
            "rr assets install --asset semantic-default",
            "If semantic assets are missing, invalid, or unverified",
            "continue with lexical-only retrieval",
            "recovery_scan",
        ],
        "search-memory semantic asset policy",
    );

    let spike_memo = fs::read_to_string(
        workspace_root()
            .join("docs/operator-stability/rr-x51h.9.7-semantic-profile-variants-spike.md"),
    )
    .expect("read rr-x51h.9.7 spike memo");
    assert_contains_all(
        &spike_memo,
        &[
            "Roger keeps `semantic-default` as the only supported semantic profile baseline",
            "Admission Gate For New Semantic Profiles",
            "This spike does not change the current live search support matrix.",
        ],
        "rr-x51h.9.7 spike memo non-widening gate",
    );
}
