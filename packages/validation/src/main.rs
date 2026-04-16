use roger_test_harness::{BudgetStatus, ValidationLane};
use roger_validation::calver::{CalVerDerivationInput, derive_calver_release};
use roger_validation::{
    budget_report, build_plan, failure_artifact_paths, persona_recovery_report,
};
use std::env;
use std::process::ExitCode;

fn print_usage() {
    eprintln!(
        "usage:\n  roger-validation plan <fast-local|pr|gated|nightly|release> [metadata_dir] [artifact_root]\n  roger-validation guard-e2e-budget [metadata_dir] [budget_json]\n  roger-validation guard-persona-recovery [metadata_dir] [issues_jsonl]\n  roger-validation failure-paths <suite_id>... [--metadata-dir DIR] [--artifact-root DIR]\n  roger-validation derive-calver [--git-ref REF] [--sha SHA] [--run-number N] [--run-attempt N] [--date-utc YYYY-MM-DD]"
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
        "guard-persona-recovery" => {
            let metadata_dir = args.get(2).map(String::as_str).unwrap_or("tests/suites");
            let issues_jsonl = args
                .get(3)
                .map(String::as_str)
                .unwrap_or(".beads/issues.jsonl");

            match persona_recovery_report(metadata_dir, issues_jsonl) {
                Ok(report) => {
                    println!("scenarios_checked={}", report.scenarios.len());
                    for scenario in &report.scenarios {
                        if scenario.ok() {
                            println!("scenario={} status=ok", scenario.scenario_id);
                            continue;
                        }
                        println!("scenario={} status=missing", scenario.scenario_id);
                        for suite_id in &scenario.missing_suite_ids {
                            println!("missing_suite={}:{}", scenario.scenario_id, suite_id);
                        }
                        for suite_id in &scenario.missing_persona_suite_ids {
                            println!("missing_persona_link={}:{}", scenario.scenario_id, suite_id);
                        }
                        for invariant_id in &scenario.missing_invariant_ids {
                            println!(
                                "missing_invariant={}:{}",
                                scenario.scenario_id, invariant_id
                            );
                        }
                        for bead_id in &scenario.missing_bead_ids {
                            println!("missing_bead={}:{}", scenario.scenario_id, bead_id);
                        }
                    }
                    if report.ok() {
                        ExitCode::SUCCESS
                    } else {
                        ExitCode::FAILURE
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
        "derive-calver" => {
            let mut git_ref = env::var("GITHUB_REF").ok();
            let mut sha = env::var("GITHUB_SHA").ok();
            let mut run_number = env::var("GITHUB_RUN_NUMBER")
                .ok()
                .and_then(|raw| raw.parse::<u64>().ok());
            let mut run_attempt = env::var("GITHUB_RUN_ATTEMPT")
                .ok()
                .and_then(|raw| raw.parse::<u32>().ok());
            let mut date_utc = env::var("RR_CALVER_DATE_UTC").ok();

            let mut index = 2;
            while index < args.len() {
                match args[index].as_str() {
                    "--git-ref" if index + 1 < args.len() => {
                        git_ref = Some(args[index + 1].clone());
                        index += 2;
                    }
                    "--sha" if index + 1 < args.len() => {
                        sha = Some(args[index + 1].clone());
                        index += 2;
                    }
                    "--run-number" if index + 1 < args.len() => {
                        match args[index + 1].parse::<u64>() {
                            Ok(parsed) => run_number = Some(parsed),
                            Err(_) => {
                                eprintln!("invalid --run-number '{}'", args[index + 1]);
                                return ExitCode::FAILURE;
                            }
                        }
                        index += 2;
                    }
                    "--run-attempt" if index + 1 < args.len() => {
                        match args[index + 1].parse::<u32>() {
                            Ok(parsed) => run_attempt = Some(parsed),
                            Err(_) => {
                                eprintln!("invalid --run-attempt '{}'", args[index + 1]);
                                return ExitCode::FAILURE;
                            }
                        }
                        index += 2;
                    }
                    "--date-utc" if index + 1 < args.len() => {
                        date_utc = Some(args[index + 1].clone());
                        index += 2;
                    }
                    unknown => {
                        eprintln!("unknown derive-calver option '{unknown}'");
                        print_usage();
                        return ExitCode::FAILURE;
                    }
                }
            }

            let Some(git_ref) = git_ref else {
                eprintln!("derive-calver requires --git-ref or GITHUB_REF");
                return ExitCode::FAILURE;
            };
            let Some(sha) = sha else {
                eprintln!("derive-calver requires --sha or GITHUB_SHA");
                return ExitCode::FAILURE;
            };
            let Some(run_number) = run_number else {
                eprintln!("derive-calver requires --run-number or GITHUB_RUN_NUMBER");
                return ExitCode::FAILURE;
            };
            let Some(run_attempt) = run_attempt else {
                eprintln!("derive-calver requires --run-attempt or GITHUB_RUN_ATTEMPT");
                return ExitCode::FAILURE;
            };
            let Some(date_utc) = date_utc else {
                eprintln!("derive-calver requires --date-utc or RR_CALVER_DATE_UTC");
                return ExitCode::FAILURE;
            };

            let input = CalVerDerivationInput {
                git_ref: &git_ref,
                sha: &sha,
                run_number,
                run_attempt,
                date_utc: &date_utc,
            };

            match derive_calver_release(&input) {
                Ok(derived) => {
                    println!("channel={}", derived.channel.as_str());
                    println!("canonical_version={}", derived.canonical_version);
                    println!("artifact_version={}", derived.artifact_version);
                    println!("tag={}", derived.tag);
                    println!("release_name={}", derived.release_name);
                    println!("artifact_prefix={}", derived.artifact_prefix);
                    println!("release_prerelease={}", derived.release_prerelease);
                    println!("provenance={}", derived.provenance);
                    println!("source_ref={git_ref}");
                    println!("source_sha={sha}");
                    println!("source_run_number={run_number}");
                    println!("source_run_attempt={run_attempt}");
                    println!("source_date_utc={date_utc}");
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
