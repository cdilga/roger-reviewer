//! Live proof that `rr assets install` + `rr search` flip to real hybrid
//! retrieval when compiled with `--features semantic-fastembed`.
//!
//! This test downloads the FastEmbed model from HuggingFace on first run, so it
//! is gated behind both the `semantic-fastembed` feature AND an explicit
//! `RR_SEMANTIC_LIVE=1` opt-in. The default (offline) test lane never runs it.
#![cfg(all(unix, feature = "semantic-fastembed"))]

use roger_cli::{CliRuntime, run};
use roger_storage::{
    CreateMaterializedFinding, CreateReviewRun, CreateReviewSession, RogerStore, UpdateIndexState,
    UpsertMemoryItem,
};
use serde_json::Value;
use std::process::Command;
use tempfile::tempdir;

fn run_rr(args: &[&str], runtime: &CliRuntime) -> roger_cli::CliRunResult {
    let argv = args.iter().map(|value| value.to_string()).collect::<Vec<_>>();
    run(&argv, runtime)
}

fn parse_robot(stdout: &str) -> Value {
    serde_json::from_str(stdout).expect("robot payload json")
}

fn init_repo(dir: &std::path::Path) {
    let run_git = |args: &[&str]| {
        let status = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .expect("git invocation");
        assert!(status.status.success(), "git {args:?} failed");
    };
    run_git(&["init"]);
    run_git(&[
        "remote",
        "add",
        "origin",
        "https://github.com/owner/repo.git",
    ]);
}

#[test]
fn rr_search_flips_to_real_hybrid_after_live_model_install() {
    if std::env::var("RR_SEMANTIC_LIVE").ok().as_deref() != Some("1") {
        eprintln!("skipping live semantic test; set RR_SEMANTIC_LIVE=1 to run (downloads model)");
        return;
    }

    let temp = tempdir().expect("tempdir");
    let repo = temp.path().join("repo");
    std::fs::create_dir_all(&repo).expect("create repo dir");
    init_repo(&repo);

    let runtime = CliRuntime {
        cwd: repo,
        store_root: temp.path().join("store"),
        opencode_bin: "opencode".to_owned(),
    };

    // Seed a tiny corpus: a review session with one finding plus a memory item,
    // all in repo scope owner/repo.
    {
        let store = RogerStore::open(&runtime.store_root).expect("open store");
        store
            .create_review_session(CreateReviewSession {
                id: "session-live-1",
                review_target: &roger_app_core::ReviewTarget {
                    repository: "owner/repo".to_owned(),
                    pull_request_number: 7,
                    base_ref: "main".to_owned(),
                    head_ref: "feature-7".to_owned(),
                    base_commit: "aaa".to_owned(),
                    head_commit: "bbb".to_owned(),
                },
                provider: "opencode",
                session_locator: None,
                resume_bundle_artifact_id: None,
                continuity_state: "resume:usable",
                attention_state: "awaiting_user_input",
                launch_profile_id: None,
            })
            .expect("create session");
        store
            .create_review_run(CreateReviewRun {
                id: "run-live-1",
                session_id: "session-live-1",
                run_kind: "deep_review",
                repo_snapshot: "{\"head\":\"bbb\"}",
                continuity_quality: "usable",
                session_locator_artifact_id: None,
            })
            .expect("create run");
        store
            .upsert_materialized_finding(CreateMaterializedFinding {
                id: "finding-live-1",
                session_id: "session-live-1",
                review_run_id: "run-live-1",
                stage: "deep_review",
                fingerprint: "fp:auth-token-leak",
                title: "Authentication token logged in plaintext",
                normalized_summary:
                    "the auth bearer token is written to the request log without redaction",
                severity: "high",
                confidence: "high",
                triage_state: "accepted",
                outbound_state: "drafted",
            })
            .expect("seed finding");
        store
            .upsert_memory_item(UpsertMemoryItem {
                id: "memory-live-1",
                scope_key: "repo:owner/repo",
                memory_class: "procedural",
                state: "proven",
                statement: "secrets and credentials must never be written to logs",
                normalized_key: "secrets credentials never logged",
                anchor_digest: Some("anchor:secret-logging"),
                source_kind: "manual",
            })
            .expect("seed memory");

        // A fully-indexed store: the lexical sidecar is ready, so the lookup
        // resolves to hybrid (not a recovery scan) once the semantic lane is
        // operational. `rr search` marks the semantic index ready itself when
        // the verified+probed model is present.
        store
            .upsert_index_state(UpdateIndexState {
                scope_key: "lexical:repo:owner/repo",
                generation: 1,
                status: "ready",
                artifact_digest: Some("sha256:lexical-live"),
            })
            .expect("seed lexical index state");
    }

    // Install the real model (downloads from HuggingFace, probes inference).
    let install = run_rr(
        &["assets", "install", "--asset", "semantic-default", "--robot"],
        &runtime,
    );
    assert_eq!(install.exit_code, 0, "install stderr: {}", install.stderr);
    let install_payload = parse_robot(&install.stdout);
    assert_eq!(install_payload["outcome"], "complete");
    assert_eq!(install_payload["data"]["backend"], "fastembed");
    assert!(
        install_payload["data"]["embedding_dimension"]
            .as_u64()
            .is_some_and(|dim| dim > 0),
        "probe must report a real embedding dimension"
    );

    // Status + verify must now report operational.
    let status = run_rr(&["assets", "status", "--robot"], &runtime);
    let status_payload = parse_robot(&status.stdout);
    assert_eq!(status_payload["data"]["operational"], true);
    assert_eq!(status_payload["data"]["posture"], "operational");

    let verify = run_rr(&["assets", "verify", "--robot"], &runtime);
    assert_eq!(verify.exit_code, 0, "verify stderr: {}", verify.stderr);
    assert_eq!(parse_robot(&verify.stdout)["data"]["verified"], true);

    // Search must flip to hybrid with semantic_available true.
    let search = run_rr(
        &[
            "search",
            "--query",
            "leaking credentials into logs",
            "--robot",
        ],
        &runtime,
    );
    assert_eq!(search.exit_code, 0, "search stderr: {}", search.stderr);
    let search_payload = parse_robot(&search.stdout);
    let data = &search_payload["data"];
    assert_eq!(
        data["fallback"]["lane"], "hybrid",
        "expected hybrid lane, got payload: {data}"
    );
    assert_eq!(data["fallback"]["semantic_available"], true);
    assert_eq!(data["retrieval_mode"], "hybrid");
}
