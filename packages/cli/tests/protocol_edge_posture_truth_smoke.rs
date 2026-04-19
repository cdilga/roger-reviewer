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
fn protocol_edge_posture_stays_direct_cli_first_and_non_widening() {
    let plan = fs::read_to_string(workspace_root().join("docs/PLAN_FOR_ROGER_REVIEWER.md"))
        .expect("read canonical plan");
    assert_contains_all(
        &plan,
        &[
            "The baseline assumption is direct CLI integration first, not ACP-first",
            "which later integrations justify ACP as a harness-control edge",
            "which justify MCP as a tool/context edge",
        ],
        "canonical plan protocol posture",
    );

    let worker_contract = fs::read_to_string(
        workspace_root().join("docs/REVIEW_WORKER_RUNTIME_AND_BOUNDARY_CONTRACT.md"),
    )
    .expect("read worker/runtime contract");
    assert_contains_all(
        &worker_contract,
        &[
            "MCP is a valid future transport",
            "Do not make MCP the required first implementation.",
        ],
        "worker/runtime MCP posture",
    );

    let spike_memo = fs::read_to_string(
        workspace_root().join("docs/operator-stability/rr-x51h.7.7-acp-mcp-protocol-spike.md"),
    )
    .expect("read rr-x51h.7.7 spike memo");
    assert_contains_all(
        &spike_memo,
        &[
            "Roger remains **direct-CLI-first** for `0.1.x`.",
            "This spike does not widen any live provider support claim.",
            "No live support matrix row changes because of this spike.",
        ],
        "rr-x51h.7.7 spike memo non-widening posture",
    );
}
