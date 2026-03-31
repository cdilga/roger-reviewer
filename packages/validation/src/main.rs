use roger_test_harness::{BudgetStatus, ValidationLane};
use roger_validation::{budget_report, build_plan, failure_artifact_paths};
use std::env;
use std::process::ExitCode;

fn print_usage() {
    eprintln!(
        "usage:\n  rr-validation plan <fast-local|pr|gated|nightly|release> [metadata_dir] [artifact_root]\n  rr-validation guard-e2e-budget [metadata_dir] [budget_json]\n  rr-validation failure-paths <suite_id>... [--metadata-dir DIR] [--artifact-root DIR]"
    );
}

fn parse_lane(raw: &str) -> Option<ValidationLane> {
    match raw {
        "fast-local" => Some(ValidationLane::FastLocal),
        "pr" => Some(ValidationLane::Pr),
        "gated" => Some(ValidationLane::Gated),
        "nightly" => Some(ValidationLane::Nightly),
        "release" => Some(ValidationLane::Release),
        _ => None,
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage();
        return ExitCode::FAILURE;
    }

    match args[1].as_str() {
        "plan" => {
            if args.len() < 3 {
                print_usage();
                return ExitCode::FAILURE;
            }
            let Some(lane) = parse_lane(&args[2]) else {
                eprintln!("unknown lane '{}'", args[2]);
                return ExitCode::FAILURE;
            };
            let metadata_dir = args.get(3).map(String::as_str).unwrap_or("tests/suites");
            let artifact_root = args
                .get(4)
                .map(String::as_str)
                .unwrap_or("target/test-artifacts");

            match build_plan(lane, metadata_dir, artifact_root) {
                Ok(plan) => {
                    println!("lane={}", plan.lane);
                    println!(
                        "allowed_tiers={}",
                        plan.allowed_tiers
                            .iter()
                            .map(|tier| format!("{tier:?}"))
                            .collect::<Vec<_>>()
                            .join(",")
                    );
                    println!(
                        "failure_artifact_root={}",
                        plan.failure_artifact_root.display()
                    );
                    for suite in plan.suites {
                        println!("suite={}", suite.id);
                    }
                    ExitCode::SUCCESS
                }
                Err(err) => {
                    eprintln!("{err}");
                    ExitCode::FAILURE
                }
            }
        }
        "guard-e2e-budget" => {
            let metadata_dir = args.get(2).map(String::as_str).unwrap_or("tests/suites");
            let budget_path = args
                .get(3)
                .map(String::as_str)
                .unwrap_or("docs/AUTOMATED_E2E_BUDGET.json");

            match budget_report(metadata_dir, budget_path) {
                Ok(report) => {
                    println!(
                        "observed_heavyweight_e2e_count={}",
                        report.observed_heavyweight_e2e_count
                    );
                    for unexpected in report.unexpected_ids {
                        println!("unexpected_e2e_id={unexpected}");
                    }
                    match report.status {
                        BudgetStatus::Ok => ExitCode::SUCCESS,
                        BudgetStatus::Warn => ExitCode::from(2),
                        BudgetStatus::Fail => ExitCode::FAILURE,
                    }
                }
                Err(err) => {
                    eprintln!("{err}");
                    ExitCode::FAILURE
                }
            }
        }
        "failure-paths" => {
            let mut metadata_dir = "tests/suites".to_string();
            let mut artifact_root = "target/test-artifacts".to_string();
            let mut suite_ids = Vec::new();
            let mut index = 2;
            while index < args.len() {
                match args[index].as_str() {
                    "--metadata-dir" if index + 1 < args.len() => {
                        metadata_dir = args[index + 1].clone();
                        index += 2;
                    }
                    "--artifact-root" if index + 1 < args.len() => {
                        artifact_root = args[index + 1].clone();
                        index += 2;
                    }
                    value => {
                        suite_ids.push(value.to_string());
                        index += 1;
                    }
                }
            }
            if suite_ids.is_empty() {
                print_usage();
                return ExitCode::FAILURE;
            }
            match failure_artifact_paths(metadata_dir, artifact_root, &suite_ids) {
                Ok(paths) => {
                    for path in paths {
                        println!("{}", path.display());
                    }
                    ExitCode::SUCCESS
                }
                Err(err) => {
                    eprintln!("{err}");
                    ExitCode::FAILURE
                }
            }
        }
        _ => {
            print_usage();
            ExitCode::FAILURE
        }
    }
}
