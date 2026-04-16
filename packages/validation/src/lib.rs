pub mod calver;

use roger_test_harness::{
    BudgetReport, E2eBudgetPolicy, SuiteMetadata, ValidationLane, ValidationPlan, artifact_dir,
    evaluate_e2e_budget, load_suite_metadata, plan_for_lane,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub fn discover_suite_metadata(dir: impl AsRef<Path>) -> Result<Vec<SuiteMetadata>, String> {
    let root = dir.as_ref();
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut suites = Vec::new();
    for entry in fs::read_dir(root).map_err(|err| err.to_string())? {
        let path = entry.map_err(|err| err.to_string())?.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("toml") {
            suites.push(load_suite_metadata(path)?);
        }
    }
    suites.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(suites)
}

pub fn build_plan(
    lane: ValidationLane,
    metadata_dir: impl AsRef<Path>,
    artifact_root: impl AsRef<Path>,
) -> Result<ValidationPlan, String> {
    let suites = discover_suite_metadata(metadata_dir)?;
    Ok(plan_for_lane(lane, &suites, artifact_root))
}

pub fn budget_report(
    metadata_dir: impl AsRef<Path>,
    budget_path: impl AsRef<Path>,
) -> Result<BudgetReport, String> {
    let suites = discover_suite_metadata(metadata_dir)?;
    let policy = E2eBudgetPolicy::load(budget_path)?;
    Ok(evaluate_e2e_budget(&policy, &suites))
}

pub fn failure_artifact_paths(
    metadata_dir: impl AsRef<Path>,
    artifact_root: impl AsRef<Path>,
    failing_suite_ids: &[String],
) -> Result<Vec<PathBuf>, String> {
    let suites = discover_suite_metadata(metadata_dir)?;
    Ok(suites
        .iter()
        .filter(|suite| failing_suite_ids.iter().any(|id| id == &suite.id))
        .map(|suite| artifact_dir(artifact_root.as_ref(), suite, true, "sample_failure"))
        .collect())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PersonaRecoveryExpectation {
    id: &'static str,
    invariant_ids: &'static [&'static str],
    suite_ids: &'static [&'static str],
    bead_ids: &'static [&'static str],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonaRecoveryScenarioReport {
    pub scenario_id: String,
    pub missing_suite_ids: Vec<String>,
    pub missing_persona_suite_ids: Vec<String>,
    pub missing_invariant_ids: Vec<String>,
    pub missing_bead_ids: Vec<String>,
}

impl PersonaRecoveryScenarioReport {
    pub fn ok(&self) -> bool {
        self.missing_suite_ids.is_empty()
            && self.missing_persona_suite_ids.is_empty()
            && self.missing_invariant_ids.is_empty()
            && self.missing_bead_ids.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonaRecoveryReport {
    pub scenarios: Vec<PersonaRecoveryScenarioReport>,
}

impl PersonaRecoveryReport {
    pub fn ok(&self) -> bool {
        self.scenarios.iter().all(PersonaRecoveryScenarioReport::ok)
    }
}

const PERSONA_RECOVERY_EXPECTATIONS: &[PersonaRecoveryExpectation] = &[
    PersonaRecoveryExpectation {
        id: "PJ-04A",
        invariant_ids: &["INV-SESSION-002"],
        suite_ids: &["int_cli_session_aware", "accept_opencode_resume"],
        bead_ids: &["rr-003.3", "rr-x51h.3.2", "rr-6iah.1", "rr-6iah.5"],
    },
    PersonaRecoveryExpectation {
        id: "PJ-04B",
        invariant_ids: &["INV-SESSION-002", "INV-CONTEXT-001"],
        suite_ids: &[
            "int_harness_opencode_resume",
            "accept_opencode_dropout_return",
            "int_storage_opencode_dropout_return",
            "smoke_opencode_continuity",
        ],
        bead_ids: &["rr-003.4", "rr-x51h.3.2", "rr-6iah.1", "rr-6iah.5"],
    },
    PersonaRecoveryExpectation {
        id: "PJ-04C",
        invariant_ids: &["INV-HARNESS-002"],
        suite_ids: &["int_tui_findings_degraded_modes"],
        bead_ids: &["rr-011.3", "rr-x51h.8.2"],
    },
    PersonaRecoveryExpectation {
        id: "PJ-04D",
        invariant_ids: &["INV-POST-002", "INV-HARNESS-003"],
        suite_ids: &[
            "prop_refresh_identity_lifecycle",
            "int_github_posting_safety_recovery",
        ],
        bead_ids: &["rr-011.2", "rr-x51h.3.2.1", "rr-6iah.3"],
    },
    PersonaRecoveryExpectation {
        id: "PJ-05B",
        invariant_ids: &["INV-POST-002"],
        suite_ids: &[
            "prop_refresh_identity_lifecycle",
            "int_github_posting_safety_recovery",
        ],
        bead_ids: &["rr-ph77.1", "rr-x51h.5.2", "rr-6iah.3"],
    },
    PersonaRecoveryExpectation {
        id: "PJ-05C",
        invariant_ids: &["INV-POST-003"],
        suite_ids: &[
            "int_github_posting_safety_recovery",
            "int_github_outbound_audit",
        ],
        bead_ids: &["rr-ph77.5", "rr-x51h.5.2", "rr-x51h.8.4"],
    },
    PersonaRecoveryExpectation {
        id: "PJ-05D",
        invariant_ids: &["INV-POST-003"],
        suite_ids: &[
            "int_github_posting_safety_recovery",
            "int_github_outbound_audit",
        ],
        bead_ids: &["rr-ph77.5", "rr-x51h.8.4"],
    },
    PersonaRecoveryExpectation {
        id: "PJ-06A",
        invariant_ids: &["INV-BRIDGE-001", "INV-BRIDGE-002"],
        suite_ids: &[
            "int_bridge_launch_only_no_status",
            "smoke_browser_launch_chrome",
            "smoke_browser_launch_brave",
            "smoke_browser_launch_edge",
        ],
        bead_ids: &["rr-8isd.5.1", "rr-8isd.5.2", "rr-6iah.4"],
    },
    PersonaRecoveryExpectation {
        id: "PJ-06B",
        invariant_ids: &["INV-SESSION-001"],
        suite_ids: &[
            "int_cli_sessions_global_finder",
            "int_cli_session_reentry_same_pr_routing",
        ],
        bead_ids: &["rr-005.2.1", "rr-011.6"],
    },
    PersonaRecoveryExpectation {
        id: "PJ-06C",
        invariant_ids: &["INV-STORE-001"],
        suite_ids: &["int_storage_release_migration_gate"],
        bead_ids: &["rr-1xhg.5", "rr-8isd.5.2"],
    },
    PersonaRecoveryExpectation {
        id: "PJ-06D",
        invariant_ids: &["INV-STORE-001"],
        suite_ids: &["int_storage_release_migration_gate"],
        bead_ids: &["rr-1xhg.5", "rr-8isd.5.3", "rr-g7j6.8"],
    },
];

pub fn persona_recovery_expected_bead_ids() -> BTreeSet<String> {
    PERSONA_RECOVERY_EXPECTATIONS
        .iter()
        .flat_map(|expectation| expectation.bead_ids.iter().copied())
        .map(str::to_owned)
        .collect()
}

fn load_bead_ids(path: impl AsRef<Path>) -> Result<BTreeSet<String>, String> {
    let raw = fs::read_to_string(path.as_ref()).map_err(|err| err.to_string())?;
    let mut ids = BTreeSet::new();
    for (line_no, line) in raw.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let value: serde_json::Value = serde_json::from_str(line)
            .map_err(|err| format!("invalid JSONL record at line {}: {err}", line_no + 1))?;
        let id = value
            .get("id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("missing bead id at line {}", line_no + 1))?;
        ids.insert(id.to_owned());
    }
    Ok(ids)
}

pub fn persona_recovery_report(
    metadata_dir: impl AsRef<Path>,
    issues_jsonl: impl AsRef<Path>,
) -> Result<PersonaRecoveryReport, String> {
    let suites = discover_suite_metadata(metadata_dir)?;
    let suite_by_id: BTreeMap<&str, &SuiteMetadata> = suites
        .iter()
        .map(|suite| (suite.id.as_str(), suite))
        .collect();
    let bead_ids = load_bead_ids(issues_jsonl)?;

    let scenarios = PERSONA_RECOVERY_EXPECTATIONS
        .iter()
        .map(|expectation| {
            let mut present_invariants = BTreeSet::new();
            let mut missing_suite_ids = Vec::new();
            let mut missing_persona_suite_ids = Vec::new();

            for suite_id in expectation.suite_ids {
                match suite_by_id.get(suite_id) {
                    Some(suite) => {
                        if !suite.persona_ids.iter().any(|id| id == expectation.id) {
                            missing_persona_suite_ids.push((*suite_id).to_owned());
                        }
                        present_invariants.extend(suite.invariant_ids.iter().cloned());
                    }
                    None => missing_suite_ids.push((*suite_id).to_owned()),
                }
            }

            let missing_invariant_ids = expectation
                .invariant_ids
                .iter()
                .filter(|id| !present_invariants.contains(**id))
                .map(|id| (*id).to_owned())
                .collect();

            let missing_bead_ids = expectation
                .bead_ids
                .iter()
                .filter(|id| !bead_ids.contains(**id))
                .map(|id| (*id).to_owned())
                .collect();

            PersonaRecoveryScenarioReport {
                scenario_id: expectation.id.to_owned(),
                missing_suite_ids,
                missing_persona_suite_ids,
                missing_invariant_ids,
                missing_bead_ids,
            }
        })
        .collect();

    Ok(PersonaRecoveryReport { scenarios })
}

#[cfg(test)]
mod tests {
    use super::*;
    use roger_test_harness::BudgetStatus;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("roger-validation-{name}-{nonce}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn discovers_and_plans_pr_lane_suites() {
        let metadata_dir = temp_dir("metadata");
        fs::write(
            metadata_dir.join("int_harness_opencode_resume.toml"),
            r#"
id = "int_harness_opencode_resume"
budget_id = "INT-HARNESS-01"
family = "int_harness"
flow_ids = ["F01", "F05"]
invariant_ids = ["INV-SESSION-002"]
persona_ids = ["PJ-03C"]
fixture_families = ["fixture_resumebundle_stale_locator"]
support_tier = "opencode_tier_b"
support_status = "blessed"
degraded = false
bounded = false
tier = "integration"
preserve_failure_artifacts = true
artifact_retention = "on_failure"
"#,
        )
        .unwrap();

        let plan = build_plan(ValidationLane::Pr, &metadata_dir, "target/test-artifacts").unwrap();
        assert_eq!(plan.suite_ids(), vec!["int_harness_opencode_resume"]);
    }

    #[test]
    fn computes_budget_report_from_metadata_files() {
        let metadata_dir = temp_dir("budget");
        fs::write(
            metadata_dir.join("e2e_core_review_happy_path.toml"),
            r#"
id = "e2e_core_review_happy_path"
budget_id = "E2E-01"
family = "e2e"
flow_ids = ["F01", "F07"]
invariant_ids = ["INV-POST-001"]
persona_ids = ["PJ-03A", "PJ-05A"]
fixture_families = ["fixture_repo_compact_review", "fixture_github_draft_batch"]
support_tier = "opencode_tier_b"
support_status = "blessed"
degraded = false
bounded = false
tier = "e2e"
preserve_failure_artifacts = true
artifact_retention = "always"
"#,
        )
        .unwrap();

        let budget = temp_dir("budget-file").join("budget.json");
        fs::write(
            &budget,
            r#"
{
  "policy_version": 1,
  "release_line": "0.1.x",
  "blessed_automated_e2e_budget": 6,
  "current_planned_blessed_automated_e2e_count": 6,
  "warning_mode": "warn_on_unjustified_growth",
  "future_ci_mode": "fail_on_unjustified_growth",
  "blessed_e2e_ids": [
    {
      "id": "E2E-01",
      "name": "Core review happy path",
      "status": "implemented",
      "notes": "Blessed path",
      "persona_ids": ["PJ-03A", "PJ-05A"],
      "flow_ids": ["F01", "F03", "F07"],
      "invariant_ids": ["INV-HARNESS-002", "INV-POST-001"],
      "executable_suite_ids": ["e2e_core_review_happy_path"],
      "current_cheaper_suite_ids": [
        "int_harness_opencode_resume",
        "int_github_outbound_audit"
      ],
      "follow_on_bead_ids": []
    },
    {
      "id": "E2E-02",
      "name": "Cross-surface review continuity with recall",
      "status": "budgeted_not_yet_implemented",
      "notes": "Budget-approved scenario slot",
      "persona_ids": ["PJ-02A", "PJ-02D", "PJ-04A", "PJ-04B"],
      "flow_ids": ["F02", "F01.1", "F05", "F07", "F09"],
      "invariant_ids": [
        "INV-SESSION-002",
        "INV-CONTEXT-001",
        "INV-SEARCH-003",
        "INV-SEARCH-004"
      ],
      "executable_suite_ids": [],
      "current_cheaper_suite_ids": [
        "int_cli_session_aware",
        "accept_opencode_resume",
        "int_search_prior_review_lookup"
      ],
      "follow_on_bead_ids": ["rr-6iah.1"]
    },
    {
      "id": "E2E-03",
      "name": "TUI-first review with memory-assisted triage",
      "status": "budgeted_not_yet_implemented",
      "notes": "Budget-approved scenario slot",
      "persona_ids": ["PJ-03A", "PJ-03C", "PJ-04A"],
      "flow_ids": ["F01", "F04", "F05", "F08", "F09"],
      "invariant_ids": [
        "INV-TUI-001",
        "INV-TUI-002",
        "INV-SEARCH-003",
        "INV-SEARCH-004"
      ],
      "executable_suite_ids": [],
      "current_cheaper_suite_ids": [
        "int_search_prior_review_lookup",
        "int_cli_session_aware"
      ],
      "follow_on_bead_ids": ["rr-6iah.2"]
    },
    {
      "id": "E2E-04",
      "name": "Refresh and draft reconciliation after new commits",
      "status": "budgeted_not_yet_implemented",
      "notes": "Budget-approved scenario slot",
      "persona_ids": ["PJ-02D", "PJ-04D", "PJ-05B"],
      "flow_ids": ["F06", "F07", "F08", "F13"],
      "invariant_ids": [
        "INV-POST-002",
        "INV-POST-003",
        "INV-HARNESS-003"
      ],
      "executable_suite_ids": [],
      "current_cheaper_suite_ids": [
        "prop_refresh_identity_lifecycle",
        "int_github_posting_safety_recovery"
      ],
      "follow_on_bead_ids": ["rr-6iah.3"]
    },
    {
      "id": "E2E-05",
      "name": "Browser setup and first PR-page launch",
      "status": "budgeted_not_yet_implemented",
      "notes": "Budget-approved scenario slot",
      "persona_ids": ["PJ-01A", "PJ-01B", "PJ-01C"],
      "flow_ids": ["F02", "F02.1", "F10", "F14"],
      "invariant_ids": [
        "INV-BRIDGE-001",
        "INV-BRIDGE-002",
        "INV-SESSION-001"
      ],
      "executable_suite_ids": [],
      "current_cheaper_suite_ids": [
        "smoke_browser_launch_chrome",
        "smoke_browser_launch_brave",
        "smoke_browser_launch_edge",
        "int_bridge_launch_only_no_status"
      ],
      "follow_on_bead_ids": ["rr-6iah.4"]
    },
    {
      "id": "E2E-06",
      "name": "Bare-harness dropout and return continuity",
      "status": "budgeted_not_yet_implemented",
      "notes": "Budget-approved scenario slot",
      "persona_ids": ["PJ-03C", "PJ-04A", "PJ-04B"],
      "flow_ids": ["F01.1", "F05", "F17", "F17.1"],
      "invariant_ids": [
        "INV-SESSION-002",
        "INV-CONTEXT-001"
      ],
      "executable_suite_ids": [],
      "current_cheaper_suite_ids": [
        "accept_opencode_dropout_return",
        "accept_opencode_resume",
        "smoke_opencode_continuity",
        "int_storage_opencode_dropout_return"
      ],
      "follow_on_bead_ids": ["rr-6iah.5"]
    }
  ],
  "required_justification_fields_for_growth": []
}
"#,
        )
        .unwrap();

        let report = budget_report(&metadata_dir, &budget).unwrap();
        assert_eq!(report.status, BudgetStatus::Ok);
    }

    #[test]
    fn release_lane_smoke_suite_preserves_failure_artifact_paths() {
        let metadata_dir = temp_dir("smoke");
        fs::write(
            metadata_dir.join("smoke_opencode_continuity.toml"),
            r#"
id = "smoke_opencode_continuity"
budget_id = "SMOKE-OPENCODE-01"
family = "smoke"
flow_ids = ["F01", "F05"]
invariant_ids = ["INV-SESSION-002"]
persona_ids = ["PJ-04B"]
fixture_families = ["fixture_resumebundle_stale_locator"]
support_tier = "opencode_tier_b"
support_status = "blessed"
degraded = false
bounded = false
tier = "smoke"
preserve_failure_artifacts = true
artifact_retention = "on_failure"
"#,
        )
        .unwrap();

        let plan = build_plan(
            ValidationLane::Release,
            &metadata_dir,
            "target/test-artifacts",
        )
        .unwrap();
        assert_eq!(plan.suite_ids(), vec!["smoke_opencode_continuity"]);

        let failing_ids = vec!["smoke_opencode_continuity".to_owned()];
        let paths =
            failure_artifact_paths(&metadata_dir, "target/test-artifacts", &failing_ids).unwrap();
        assert_eq!(paths.len(), 1);
        assert!(
            paths[0]
                .to_string_lossy()
                .contains("failures/smoke_opencode_continuity/sample_failure"),
            "smoke failure artifacts must route through the failure artifact namespace"
        );
    }

    #[test]
    fn full_repo_suite_directory_supports_codex_acceptance_metadata() {
        let metadata_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/suites");
        let plan = build_plan(
            ValidationLane::Gated,
            &metadata_dir,
            "target/test-artifacts",
        )
        .expect("real suite metadata directory should be plannable");
        let suite_ids = plan.suite_ids();
        assert!(
            suite_ids.contains(&"accept_codex_reseed"),
            "gated lane plan should include codex acceptance metadata"
        );
    }

    #[test]
    fn unsupported_suite_family_fails_with_explicit_error() {
        let metadata_dir = temp_dir("unsupported-family");
        fs::write(
            metadata_dir.join("accept_unknown_reseed.toml"),
            r#"
id = "accept_unknown_reseed"
budget_id = "ACCEPT-UNKNOWN-01"
family = "accept_unknown"
flow_ids = ["F01"]
fixture_families = ["fixture_repo_compact_review"]
support_tier = "unknown_tier"
support_status = "bounded"
degraded = true
bounded = true
tier = "acceptance"
preserve_failure_artifacts = true
artifact_retention = "on_failure"
"#,
        )
        .unwrap();

        let err = build_plan(
            ValidationLane::Gated,
            &metadata_dir,
            "target/test-artifacts",
        )
        .expect_err("unsupported family should fail loudly");
        assert!(
            err.contains("unknown variant `accept_unknown`"),
            "unexpected unsupported-family error: {err}"
        );
    }
}
