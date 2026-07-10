//! Cross-surface parity guard (docs/SURFACE_PARITY_CONTRACT.md).
//!
//! Asserts the parity matrix holds so a surface can't silently drop below
//! parity: every operator operation is reachable on the surfaces the contract
//! requires (by presence of its command / TUI key / bridge action), and the
//! two deliberate asymmetries stay asymmetric:
//!   1. the extension has NO approve/post action (posting hands off), and
//!   2. the worker transport is not an operator action on any surface.
//!
//! This is a source-presence guard, not a behavioral test — the behavior is
//! covered by each surface's own suites. It fails loudly when a rename or a
//! deletion breaks a parity leg.

use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("packages parent")
        .parent()
        .expect("repo root")
        .to_path_buf()
}

fn read(rel: &str) -> String {
    let path = repo_root().join(rel);
    match std::fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(err) => {
            assert!(false, "read {}: {err}", path.display());
            unreachable!()
        }
    }
}

/// The CLI is the reference surface: every operator operation has a command
/// token in the grammar and (for the machine surface) a robot schema id.
#[test]
fn cli_exposes_every_operator_operation() {
    let cli = read("packages/cli/src/lib.rs");
    for token in [
        "\"doctor\"",
        "\"queue\"",
        "\"review\"",
        "\"resume\"",
        "\"return\"",
        "\"findings\"",
        "\"triage\"",
        "\"draft\"",
        "\"approve\"",
        "\"post\"",
        "\"search\"",
        "\"sessions\"",
        "\"status\"",
        "\"memory\"",   // reverse-parity: memory review/accept/reject
        "\"timeline\"", // reverse-parity: timeline view
        "\"clarify\"",  // reverse-parity: durable clarification
    ] {
        assert!(
            cli.contains(token),
            "CLI parity: grammar is missing command token {token}"
        );
    }
    for schema in [
        "rr.robot.memory.v1",
        "rr.robot.timeline.v1",
        "rr.robot.clarify.v1",
    ] {
        assert!(
            cli.contains(schema),
            "CLI parity: robot schema id {schema} is missing"
        );
    }
}

/// The TUI is the full operator cockpit: create-draft, elevated post, clarify
/// composer, evidence excerpt, and launch/return must be wired (backend
/// methods) and the deferred-hint placeholders must be gone.
#[test]
fn tui_reaches_full_operator_parity() {
    let backend = read("packages/tui/src/backend.rs");
    for method in [
        "fn materialize_draft_batch",
        "fn post_batch",
        "fn create_clarification",
        "fn load_evidence_excerpt",
        "fn resolve_memory_review",
        "fn set_triage_state",
        "fn load_queue", // entry leg: the open-PR queue
    ] {
        assert!(
            backend.contains(method),
            "TUI parity: CockpitBackend is missing {method}"
        );
    }
    let model = read("packages/tui/src/model.rs");
    assert!(
        model.contains("LaunchProvider"),
        "TUI parity: launch/return effect (LaunchProvider) missing"
    );
    for stale in [
        "LAUNCH_DEFERRED_HINT",
        "CLARIFY_HINT",
        "ELEVATED_MUTATION_HINT",
    ] {
        assert!(
            !model.contains(stale),
            "TUI parity regression: deferred placeholder {stale} is back \
             (the real action must replace the hint)"
        );
    }
    // Posting is elevated and distinct from approve.
    assert!(
        model.contains("POST_CONFIRMATION_WORD"),
        "TUI parity: elevated post confirm word missing"
    );

    // Entry legs (v2026.07.10): the cockpit can reach review work on its own —
    // an open-PR queue, a fresh start, a reuse-or-fresh decision, and doctor.
    assert!(
        model.contains("Screen::Queue"),
        "TUI parity: Queue screen (entry leg) missing"
    );
    assert!(
        model.contains("fn emit_doctor_dropout"),
        "TUI parity: doctor preflight dropout missing"
    );
    assert!(
        model.contains("fn act_on_queue_row"),
        "TUI parity: reuse-or-fresh queue action missing"
    );
    // The empty cockpit must offer the in-TUI queue, not only a shell command.
    assert!(
        model.contains("press p to pick an open PR"),
        "TUI parity regression: the empty cockpit sends the operator back to the shell"
    );
}

/// The queue is one shared op, not a CLI-only projection reimplemented by the
/// TUI. `roger-review-ops::queue_rows` is the single path; a surface computing
/// `roger_state` on its own would let CLI and TUI disagree about a PR.
#[test]
fn queue_state_is_derived_by_exactly_one_shared_op() {
    let ops = read("packages/review-ops/src/lib.rs");
    for symbol in ["pub fn queue_rows", "pub fn derive_queue_state"] {
        assert!(ops.contains(symbol), "shared queue op missing {symbol}");
    }
    for surface in [
        "packages/cli/src/lib.rs",
        "packages/tui/src/backend.rs",
        "packages/tui/src/model.rs",
    ] {
        let source = read(surface);
        assert!(
            !source.contains("fn derive_queue_state")
                && !source.contains("fn derive_prs_queue_state"),
            "{surface} reimplements queue-state derivation — it must call \
             roger_review_ops::derive_queue_state"
        );
    }
}

/// The extension reaches bounded local parity: the local-mutation and read
/// actions dispatch to the shared-op-backed rr commands.
#[test]
fn extension_reaches_bounded_parity() {
    let bridge = read("packages/bridge/src/lib.rs");
    for action in [
        "\"triage_finding\"",
        "\"show_drafts\"",
        "\"revise_draft\"",
        "\"request_clarification\"",
        "\"search\"",
        "\"timeline\"",
    ] {
        assert!(
            bridge.contains(action),
            "Extension parity: bridge action {action} missing"
        );
    }
    let background = read("apps/extension/src/background/main.js");
    for action in [
        "triage_finding",
        "revise_draft",
        "request_clarification",
        "timeline",
    ] {
        assert!(
            background.contains(action),
            "Extension parity: background SUPPORTED_ACTIONS missing {action}"
        );
    }
}

/// Deliberate asymmetry 1: the extension must NOT approve or post — those are
/// Roger-mediated and visibly elevated through the CLI/TUI. A bridge dispatch
/// arm for approve/post would be a security-boundary regression.
#[test]
fn extension_never_approves_or_posts() {
    let bridge = read("packages/bridge/src/lib.rs");
    // No dispatch arm routing an approve/post action to a handler.
    for forbidden in [
        "\"approve\" => Some(",
        "\"post\" => Some(",
        "\"approve_batch\" => Some(",
        "\"post_batch\" => Some(",
    ] {
        assert!(
            !bridge.contains(forbidden),
            "Extension security regression: bridge routes {forbidden} — \
             approval/posting must stay in CLI/TUI (asymmetry 1)"
        );
    }
    // The content surface renders the handoff command, not a post button.
    let content = read("apps/extension/src/content/main.js");
    assert!(
        content.contains("rr send approve") || content.contains("rr send post"),
        "Extension parity: approve/post handoff command block missing"
    );
}

/// Deliberate asymmetry 2: the worker transport (rr agent worker.*) is not an
/// operator surface. It must not appear as a TUI key action or a bridge action.
#[test]
fn worker_transport_is_not_an_operator_surface() {
    let bridge = read("packages/bridge/src/lib.rs");
    assert!(
        !bridge.contains("worker.get_review_context")
            && !bridge.contains("worker.submit_stage_result"),
        "Parity boundary: worker transport leaked into the bridge (asymmetry 2)"
    );
    let background = read("apps/extension/src/background/main.js");
    assert!(
        !background.contains("worker.submit_stage_result"),
        "Parity boundary: worker transport leaked into the extension (asymmetry 2)"
    );
}
