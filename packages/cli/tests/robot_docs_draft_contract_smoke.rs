#![cfg(unix)]

use roger_cli::{CliRuntime, run};
use serde_json::Value;
use std::path::PathBuf;

fn run_rr(args: &[&str]) -> roger_cli::CliRunResult {
    let argv = args
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<Vec<_>>();
    let runtime = CliRuntime {
        cwd: PathBuf::from("."),
        store_root: PathBuf::from(".roger-test"),
        opencode_bin: "opencode".to_owned(),
    };
    run(&argv, &runtime)
}

fn parse_robot_payload(stdout: &str) -> Value {
    serde_json::from_str(stdout).expect("robot payload json")
}

#[test]
fn robot_docs_schemas_and_commands_advertise_rr_draft() {
    let schemas = run_rr(&["robot-docs", "schemas", "--robot"]);
    assert_eq!(schemas.exit_code, 0, "{}", schemas.stderr);
    let schemas_payload = parse_robot_payload(&schemas.stdout);
    let schema_items = schemas_payload["data"]["items"]
        .as_array()
        .expect("schema items");
    assert!(schema_items.iter().any(|item| {
        item["command"] == "rr draft" && item["schema_id"] == "rr.robot.draft.v1"
    }));

    let commands = run_rr(&["robot-docs", "commands", "--robot"]);
    assert_eq!(commands.exit_code, 0, "{}", commands.stderr);
    let commands_payload = parse_robot_payload(&commands.stdout);
    let command_items = commands_payload["data"]["items"]
        .as_array()
        .expect("command items");
    assert!(command_items.iter().any(|item| item["command"] == "rr draft"));
}

#[test]
fn robot_docs_workflows_advertise_local_outbound_draft_loop() {
    let workflows = run_rr(&["robot-docs", "workflows", "--robot"]);
    assert_eq!(workflows.exit_code, 0, "{}", workflows.stderr);
    let payload = parse_robot_payload(&workflows.stdout);
    let workflow_items = payload["data"]["items"]
        .as_array()
        .expect("workflow items");
    let draft_workflow = workflow_items
        .iter()
        .find(|item| item["name"] == "local_outbound_draft")
        .expect("local outbound draft workflow");
    let steps = draft_workflow["steps"].as_array().expect("workflow steps");
    assert!(steps.iter().any(|step| step == "rr draft --session <id> --finding <finding-id> [--finding <finding-id>] --robot"));
}
