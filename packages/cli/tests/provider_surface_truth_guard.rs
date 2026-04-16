#![cfg(unix)]

use roger_cli::{CliRuntime, run};
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

const LIVE_PROVIDERS: &[&str] = &["opencode", "codex", "gemini", "claude"];
const PLANNED_NOT_LIVE_PROVIDERS: &[&str] = &["copilot"];
const NOT_SUPPORTED_PROVIDERS: &[&str] = &["pi-agent"];
const OPENCODE_NOTE: &str = "first-class tier-b continuity path with locator reopen and rr return";
const BOUNDED_PROVIDER_NOTE: &str =
    "bounded tier-a start/reseed/raw-capture path only; no locator reopen or rr return";

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("packages parent")
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn parse_robot_payload(stdout: &str) -> Value {
    serde_json::from_str(stdout).expect("valid robot JSON payload")
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

fn assert_provider_entry(
    entries: &[Value],
    provider: &str,
    display_name: &str,
    tier: &str,
    status: &str,
    resume_reopen: bool,
    rr_return: bool,
    notes: &str,
) {
    let entry = entries.iter().find(|item| item["provider"] == provider);
    assert!(entry.is_some(), "missing provider entry for {}", provider);
    let entry = entry.expect("provider entry present after assertion");
    assert_eq!(entry["display_name"], display_name);
    assert_eq!(entry["tier"], tier);
    assert_eq!(entry["status"], status);
    assert_eq!(entry["supports"]["review_start"], true);
    assert_eq!(entry["supports"]["resume_reseed"], true);
    assert_eq!(entry["supports"]["resume_reopen"], resume_reopen);
    assert_eq!(entry["supports"]["return"], rr_return);
    assert_eq!(entry["notes"], notes);
}

#[test]
fn provider_support_truth_guard_matches_live_cli_help_and_docs() {
    let temp = tempdir().expect("tempdir");
    let runtime = CliRuntime {
        cwd: workspace_root(),
        store_root: temp.path().join("roger-store"),
        opencode_bin: "opencode".to_owned(),
    };

    let help = run(&["--help".to_owned()], &runtime);
    assert_eq!(help.exit_code, 0, "{}", help.stderr);
    assert_contains_all(
        &help.stdout,
        &[
            "rr agent <operation> --task-file <path>",
            "rr agent is the dedicated in-session worker transport; it is separate from --robot",
            "current live rr agent operations cover context/status/search/finding/artifact reads, advisory clarification or follow-up proposals, and worker.submit_stage_result",
            "rr agent emits rr.agent.response.v1 envelopes over the canonical worker operation response payload instead of reusing the --robot surface",
            "opencode is the first-class tier-b continuity path; rr resume can reopen and rr return is supported",
            "codex, gemini, and claude are bounded tier-a providers; start/reseed/raw-capture only, no locator reopen or rr return",
            "copilot is planned but not yet a live --provider value",
            "pi-agent is not part of the 0.1.0 live CLI surface",
        ],
        "rr --help output",
    );

    let blocked = run(
        &[
            "review".to_owned(),
            "--pr".to_owned(),
            "42".to_owned(),
            "--provider".to_owned(),
            "pi-agent".to_owned(),
            "--robot".to_owned(),
        ],
        &runtime,
    );
    assert_eq!(blocked.exit_code, 3, "{}", blocked.stderr);
    let blocked_payload = parse_robot_payload(&blocked.stdout);
    assert_eq!(blocked_payload["outcome"], "blocked");
    assert_eq!(
        blocked_payload["data"]["supported_providers"],
        json!(LIVE_PROVIDERS)
    );
    assert_eq!(
        blocked_payload["data"]["planned_not_live_providers"],
        json!(PLANNED_NOT_LIVE_PROVIDERS)
    );
    assert_eq!(
        blocked_payload["data"]["not_supported_providers"],
        json!(NOT_SUPPORTED_PROVIDERS)
    );
    let blocked_matrix = blocked_payload["data"]["live_review_provider_support"]
        .as_array()
        .expect("blocked provider matrix");
    assert_provider_entry(
        blocked_matrix,
        "opencode",
        "OpenCode",
        "tier_b",
        "first_class_live",
        true,
        true,
        OPENCODE_NOTE,
    );
    for provider in ["codex", "gemini", "claude"] {
        let display_name = match provider {
            "codex" => "Codex",
            "gemini" => "Gemini",
            "claude" => "Claude Code",
            _ => unreachable!(),
        };
        assert_provider_entry(
            blocked_matrix,
            provider,
            display_name,
            "tier_a",
            "bounded_live",
            false,
            false,
            BOUNDED_PROVIDER_NOTE,
        );
    }

    let guide = run(
        &[
            "robot-docs".to_owned(),
            "guide".to_owned(),
            "--robot".to_owned(),
        ],
        &runtime,
    );
    assert_eq!(guide.exit_code, 0, "{}", guide.stderr);
    let guide_payload = parse_robot_payload(&guide.stdout);
    let guide_items = guide_payload["data"]["items"]
        .as_array()
        .expect("guide items");
    let provider_support = guide_items
        .iter()
        .find(|item| item["kind"] == "provider_support")
        .expect("provider support guide item");
    assert_eq!(
        provider_support["planned_not_live_providers"],
        json!(PLANNED_NOT_LIVE_PROVIDERS)
    );
    assert_eq!(
        provider_support["not_supported_providers"],
        json!(NOT_SUPPORTED_PROVIDERS)
    );
    let live_review_providers = provider_support["live_review_providers"]
        .as_array()
        .expect("guide live review providers");
    assert_provider_entry(
        live_review_providers,
        "opencode",
        "OpenCode",
        "tier_b",
        "first_class_live",
        true,
        true,
        OPENCODE_NOTE,
    );
    for provider in ["codex", "gemini", "claude"] {
        let display_name = match provider {
            "codex" => "Codex",
            "gemini" => "Gemini",
            "claude" => "Claude Code",
            _ => unreachable!(),
        };
        assert_provider_entry(
            live_review_providers,
            provider,
            display_name,
            "tier_a",
            "bounded_live",
            false,
            false,
            BOUNDED_PROVIDER_NOTE,
        );
    }
    let inside_roger = guide_items
        .iter()
        .find(|item| item["context"] == "inside_roger")
        .expect("inside Roger guide item");
    assert_eq!(
        inside_roger["finding_return_contract"]["canonical_transport"],
        "rr agent worker.submit_stage_result"
    );
    assert_eq!(
        inside_roger["finding_return_contract"]["availability"],
        "canonical worker contract; separate from the --robot command shortlist and not implied to be shipped by this discovery item alone"
    );
    assert_eq!(
        inside_roger["finding_return_contract"]["binding_fields"],
        json!([
            "review_session_id",
            "review_run_id",
            "review_task_id",
            "task_nonce"
        ])
    );
    assert_eq!(
        inside_roger["finding_return_contract"]["result_fields"],
        json!([
            "schema_id",
            "stage",
            "task_kind",
            "outcome",
            "summary",
            "structured_findings_pack"
        ])
    );
    assert_eq!(
        inside_roger["finding_return_contract"]["finding_pack"],
        json!({
            "schema_version": "structured_findings_pack/v1",
            "finding_fields": [
                "fingerprint",
                "title",
                "normalized_summary",
                "severity",
                "confidence",
                "code_evidence"
            ]
        })
    );

    let commands = run(
        &[
            "robot-docs".to_owned(),
            "commands".to_owned(),
            "--robot".to_owned(),
        ],
        &runtime,
    );
    assert_eq!(commands.exit_code, 0, "{}", commands.stderr);
    let commands_payload = parse_robot_payload(&commands.stdout);
    let command_items = commands_payload["data"]["items"]
        .as_array()
        .expect("command items");
    let review_dry_run = command_items
        .iter()
        .find(|item| item["command"] == "rr review --dry-run")
        .expect("rr review --dry-run docs item");
    assert_eq!(review_dry_run["supported_providers"], json!(LIVE_PROVIDERS));
    assert_eq!(
        review_dry_run["planned_not_live_providers"],
        json!(PLANNED_NOT_LIVE_PROVIDERS)
    );
    assert_eq!(
        review_dry_run["not_supported_providers"],
        json!(NOT_SUPPORTED_PROVIDERS)
    );
    assert!(
        command_items
            .iter()
            .all(|item| item["command"] != "rr agent" && item["command"] != "rr agent <operation>"),
        "rr agent should remain outside the --robot command inventory"
    );

    let schemas = run(
        &[
            "robot-docs".to_owned(),
            "schemas".to_owned(),
            "--robot".to_owned(),
        ],
        &runtime,
    );
    assert_eq!(schemas.exit_code, 0, "{}", schemas.stderr);
    let schemas_payload = parse_robot_payload(&schemas.stdout);
    let schema_items = schemas_payload["data"]["items"]
        .as_array()
        .expect("schema items");
    let agent_schema = schema_items
        .iter()
        .find(|item| item["command"] == "rr agent")
        .expect("rr agent schema item");
    assert_eq!(agent_schema["schema_id"], "rr.agent.response.v1");
    assert_eq!(agent_schema["surface"], "dedicated_worker_transport");

    let readme = fs::read_to_string(workspace_root().join("README.md")).expect("read README");
    assert_contains_all(
        &readme,
        &[
            "`rr review --provider` currently supports `opencode`, `codex`, `gemini`, and",
            "`claude`.",
            "OpenCode remains the strongest continuity path.",
            "Codex, Gemini, and Claude Code are live only as bounded Tier A paths:",
            "it does not claim locator reopen or `rr return` for those providers.",
            "GitHub Copilot CLI is still planned rather than live",
        ],
        "README provider support snapshot",
    );

    let release_matrix =
        fs::read_to_string(workspace_root().join("docs/RELEASE_AND_TEST_MATRIX.md"))
            .expect("read release and test matrix");
    assert_contains_all(
        &release_matrix,
        &[
            "| GitHub Copilot CLI | Golden-path first-class provider target, not yet live |",
            "| OpenCode | First-class fallback and current strongest landed path |",
            "| Codex | Secondary, bounded | Exposed via `rr review --provider codex`; truthful Tier A reseed/raw-capture path, no locator reopen or `rr return` claim |",
            "| Gemini | Secondary, bounded | Exposed via `rr review --provider gemini`; truthful Tier A reseed/raw-capture path, no locator reopen or `rr return` claim |",
            "| Claude Code | Secondary, bounded | Exposed via `rr review --provider claude`; truthful Tier A reseed/raw-capture path, no locator reopen or `rr return` claim |",
            "| Pi-Agent | Not in `0.1.0` | Planning-only future harness candidate; no live support claim, no `rr review --provider pi-agent`, and no Tier A/Tier B language until a later admission spike proves direct-CLI launch, Roger-safe policy control, audit capture, and truthful continuity behavior |",
            "the authoritative provider support order is GitHub Copilot CLI, OpenCode,",
        ],
        "release matrix provider contract",
    );

    let robot_contract = fs::read_to_string(workspace_root().join("docs/ROBOT_CLI_CONTRACT.md"))
        .expect("read robot CLI contract");
    assert_contains_all(
        &robot_contract,
        &[
            "`rr agent ...` worker transport",
            "`rr agent` is not part of the `--robot` contract and must remain a separate",
            "`rr robot-docs guide` should also expose the current `rr review` provider-support",
            "`supported_providers`",
            "`planned_not_live_providers`",
            "`not_supported_providers`",
        ],
        "robot CLI contract provider inventory",
    );

    let inside_roger_skill =
        fs::read_to_string(workspace_root().join("docs/skills/ROGER_INSIDE_ROGER_AGENT.md"))
            .expect("read inside Roger skill doc");
    assert_contains_all(
        &inside_roger_skill,
        &[
            "it remains narrower than the dedicated `rr agent` worker transport",
            "present an unsupported in-harness affordance, or parity with `rr agent`, as if it were shipped",
        ],
        "inside Roger skill truth snapshot",
    );
}
