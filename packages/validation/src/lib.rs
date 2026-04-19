pub mod calver;

use roger_test_harness::{
    artifact_dir, evaluate_e2e_budget, load_suite_metadata, plan_for_lane, BudgetReport,
    E2eBudgetPolicy, SuiteMetadata, ValidationLane, ValidationPlan,
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
struct PersonaGuardExpectation {
    id: &'static str,
    invariant_ids: &'static [&'static str],
    suite_ids: &'static [&'static str],
    bead_ids: &'static [&'static str],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonaGuardScenarioReport {
    pub scenario_id: String,
    pub missing_suite_ids: Vec<String>,
    pub missing_persona_suite_ids: Vec<String>,
    pub missing_invariant_ids: Vec<String>,
    pub missing_bead_ids: Vec<String>,
}

impl PersonaGuardScenarioReport {
    pub fn ok(&self) -> bool {
        self.missing_suite_ids.is_empty()
            && self.missing_persona_suite_ids.is_empty()
            && self.missing_invariant_ids.is_empty()
            && self.missing_bead_ids.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonaGuardReport {
    pub scenarios: Vec<PersonaGuardScenarioReport>,
}

impl PersonaGuardReport {
    pub fn ok(&self) -> bool {
        self.scenarios.iter().all(PersonaGuardScenarioReport::ok)
    }
}

const PERSONA_OWNERSHIP_EXPECTATIONS: &[PersonaGuardExpectation] = &[
    PersonaGuardExpectation {
        id: "PJ-01A",
        invariant_ids: &["INV-BRIDGE-001", "INV-BRIDGE-002"],
        suite_ids: &[
            "int_bridge_launch_only_no_status",
            "smoke_browser_launch_chrome",
            "smoke_browser_launch_brave",
            "smoke_browser_launch_edge",
        ],
        bead_ids: &[
            "rr-021",
            "rr-022",
            "rr-8isd.5.1",
            "rr-8isd.5.2",
            "rr-6iah.4",
            "rr-8isd.5.3",
            "rr-b58q.4.4",
        ],
    },
    PersonaGuardExpectation {
        id: "PJ-01B",
        invariant_ids: &["INV-BRIDGE-001", "INV-BRIDGE-002"],
        suite_ids: &[
            "int_bridge_launch_only_no_status",
            "smoke_browser_launch_chrome",
            "smoke_browser_launch_brave",
            "smoke_browser_launch_edge",
        ],
        bead_ids: &[
            "rr-021",
            "rr-8isd.5.1",
            "rr-8isd.5.2",
            "rr-6iah.4",
            "rr-8isd.5.3",
            "rr-b58q.4.4",
        ],
    },
    PersonaGuardExpectation {
        id: "PJ-01C",
        invariant_ids: &["INV-BRIDGE-001", "INV-BRIDGE-002"],
        suite_ids: &[
            "int_bridge_launch_only_no_status",
            "smoke_browser_launch_chrome",
            "smoke_browser_launch_brave",
            "smoke_browser_launch_edge",
        ],
        bead_ids: &[
            "rr-021",
            "rr-022",
            "rr-8isd.5.2",
            "rr-6iah.4",
            "rr-b58q.4.4",
        ],
    },
    PersonaGuardExpectation {
        id: "PJ-01D",
        invariant_ids: &["INV-BRIDGE-001", "INV-BRIDGE-002"],
        suite_ids: &["int_bridge_launch_only_no_status"],
        bead_ids: &["rr-021", "rr-8isd.5.2", "rr-8isd.5.3", "rr-b58q.4.4"],
    },
    PersonaGuardExpectation {
        id: "PJ-02A",
        invariant_ids: &["INV-SESSION-001", "INV-SESSION-002"],
        suite_ids: &[
            "int_cli_session_aware",
            "int_cli_session_reentry_same_pr_routing",
            "accept_opencode_resume",
        ],
        bead_ids: &["rr-005.2", "rr-018", "rr-022", "rr-6iah.1"],
    },
    PersonaGuardExpectation {
        id: "PJ-02B",
        invariant_ids: &["INV-SESSION-001"],
        suite_ids: &[
            "int_cli_sessions_global_finder",
            "int_cli_session_reentry_same_pr_routing",
        ],
        bead_ids: &["rr-005.2", "rr-005.2.1", "rr-009.1", "rr-6iah.1"],
    },
    PersonaGuardExpectation {
        id: "PJ-02C",
        invariant_ids: &["INV-BRIDGE-001", "INV-BRIDGE-002", "INV-SESSION-002"],
        suite_ids: &[
            "int_bridge_launch_only_no_status",
            "int_cli_session_aware",
            "accept_opencode_resume",
        ],
        bead_ids: &["rr-021", "rr-018", "rr-022", "rr-6iah.1"],
    },
    PersonaGuardExpectation {
        id: "PJ-02D",
        invariant_ids: &[
            "INV-SESSION-002",
            "INV-POST-002",
            "INV-POST-003",
            "INV-HARNESS-003",
        ],
        suite_ids: &[
            "prop_refresh_identity_lifecycle",
            "int_cli_session_aware",
            "int_github_posting_safety_recovery",
        ],
        bead_ids: &["rr-005.2", "rr-x51h.3.2.1", "rr-6iah.1", "rr-6iah.3"],
    },
    PersonaGuardExpectation {
        id: "PJ-03A",
        invariant_ids: &["INV-HARNESS-002", "INV-POST-001"],
        suite_ids: &["e2e_core_review_happy_path"],
        bead_ids: &["rr-018", "rr-019", "rr-6iah.2"],
    },
    PersonaGuardExpectation {
        id: "PJ-03B",
        invariant_ids: &["INV-SESSION-001", "INV-SESSION-002"],
        suite_ids: &[
            "int_cli_session_aware",
            "int_cli_session_reentry_same_pr_routing",
        ],
        bead_ids: &["rr-005.2", "rr-018"],
    },
    PersonaGuardExpectation {
        id: "PJ-03C",
        invariant_ids: &["INV-SESSION-002"],
        suite_ids: &[
            "accept_opencode_resume",
            "int_harness_opencode_resume",
            "int_cli_session_aware",
            "smoke_opencode_continuity",
        ],
        bead_ids: &["rr-003.3", "rr-018", "rr-6iah.2", "rr-6iah.5"],
    },
    PersonaGuardExpectation {
        id: "PJ-03D",
        invariant_ids: &["INV-SESSION-001", "INV-SESSION-002"],
        suite_ids: &[
            "int_cli_sessions_global_finder",
            "int_cli_session_reentry_same_pr_routing",
            "int_cli_session_aware",
        ],
        bead_ids: &["rr-005.2", "rr-005.2.1", "rr-018", "rr-6iah.1"],
    },
];

const PERSONA_RECOVERY_EXPECTATIONS: &[PersonaGuardExpectation] = &[
    PersonaGuardExpectation {
        id: "PJ-04A",
        invariant_ids: &["INV-SESSION-002"],
        suite_ids: &[
            "int_cli_session_aware",
            "accept_opencode_resume",
            "smoke_opencode_continuity",
        ],
        bead_ids: &["rr-003.3", "rr-x51h.3.2", "rr-6iah.1", "rr-6iah.5"],
    },
    PersonaGuardExpectation {
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
    PersonaGuardExpectation {
        id: "PJ-04C",
        invariant_ids: &["INV-HARNESS-002"],
        suite_ids: &["int_tui_findings_degraded_modes"],
        bead_ids: &["rr-011.3", "rr-x51h.8.2"],
    },
    PersonaGuardExpectation {
        id: "PJ-04D",
        invariant_ids: &["INV-POST-002", "INV-HARNESS-003"],
        suite_ids: &[
            "prop_refresh_identity_lifecycle",
            "int_github_posting_safety_recovery",
        ],
        bead_ids: &["rr-011.2", "rr-x51h.3.2.1", "rr-6iah.3"],
    },
    PersonaGuardExpectation {
        id: "PJ-05B",
        invariant_ids: &["INV-POST-002"],
        suite_ids: &[
            "prop_refresh_identity_lifecycle",
            "int_github_posting_safety_recovery",
        ],
        bead_ids: &["rr-ph77.1", "rr-x51h.5.2", "rr-x51h.8.4", "rr-6iah.3"],
    },
    PersonaGuardExpectation {
        id: "PJ-05C",
        invariant_ids: &["INV-POST-003"],
        suite_ids: &[
            "int_github_posting_safety_recovery",
            "int_github_outbound_audit",
        ],
        bead_ids: &["rr-ph77.5", "rr-x51h.5.2", "rr-x51h.8.4"],
    },
    PersonaGuardExpectation {
        id: "PJ-05D",
        invariant_ids: &["INV-POST-003"],
        suite_ids: &[
            "int_github_posting_safety_recovery",
            "int_github_outbound_audit",
        ],
        bead_ids: &["rr-ph77.5", "rr-x51h.8.4"],
    },
    PersonaGuardExpectation {
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
    PersonaGuardExpectation {
        id: "PJ-06B",
        invariant_ids: &["INV-SESSION-001"],
        suite_ids: &[
            "int_cli_sessions_global_finder",
            "int_cli_session_reentry_same_pr_routing",
        ],
        bead_ids: &["rr-005.2.1", "rr-011.6"],
    },
    PersonaGuardExpectation {
        id: "PJ-06C",
        invariant_ids: &["INV-STORE-001"],
        suite_ids: &["int_storage_release_migration_gate"],
        bead_ids: &["rr-1xhg.5", "rr-8isd.5.2"],
    },
    PersonaGuardExpectation {
        id: "PJ-06D",
        invariant_ids: &["INV-STORE-001"],
        suite_ids: &["int_storage_release_migration_gate"],
        bead_ids: &["rr-1xhg.5", "rr-8isd.5.3", "rr-g7j6.8"],
    },
];

fn persona_expected_bead_ids(expectations: &[PersonaGuardExpectation]) -> BTreeSet<String> {
    expectations
        .iter()
        .flat_map(|expectation| expectation.bead_ids.iter().copied())
        .map(str::to_owned)
        .collect()
}

pub fn persona_ownership_expected_bead_ids() -> BTreeSet<String> {
    persona_expected_bead_ids(PERSONA_OWNERSHIP_EXPECTATIONS)
}

pub fn persona_recovery_expected_bead_ids() -> BTreeSet<String> {
    persona_expected_bead_ids(PERSONA_RECOVERY_EXPECTATIONS)
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

fn persona_guard_report(
    expectations: &[PersonaGuardExpectation],
    metadata_dir: impl AsRef<Path>,
    issues_jsonl: impl AsRef<Path>,
) -> Result<PersonaGuardReport, String> {
    let suites = discover_suite_metadata(metadata_dir)?;
    let suite_by_id: BTreeMap<&str, &SuiteMetadata> = suites
        .iter()
        .map(|suite| (suite.id.as_str(), suite))
        .collect();
    let bead_ids = load_bead_ids(issues_jsonl)?;

    let scenarios = expectations
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

            PersonaGuardScenarioReport {
                scenario_id: expectation.id.to_owned(),
                missing_suite_ids,
                missing_persona_suite_ids,
                missing_invariant_ids,
                missing_bead_ids,
            }
        })
        .collect();

    Ok(PersonaGuardReport { scenarios })
}

pub fn persona_ownership_report(
    metadata_dir: impl AsRef<Path>,
    issues_jsonl: impl AsRef<Path>,
) -> Result<PersonaGuardReport, String> {
    persona_guard_report(PERSONA_OWNERSHIP_EXPECTATIONS, metadata_dir, issues_jsonl)
}

pub fn persona_recovery_report(
    metadata_dir: impl AsRef<Path>,
    issues_jsonl: impl AsRef<Path>,
) -> Result<PersonaGuardReport, String> {
    persona_guard_report(PERSONA_RECOVERY_EXPECTATIONS, metadata_dir, issues_jsonl)
}

#[cfg(test)]
mod tests {
    use super::*;
    use roger_test_harness::BudgetStatus;
    use roger_test_harness::SupportStatus;
    use serde_json::Value;
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
      "status": "implemented",
      "notes": "Budget-approved scenario slot",
      "persona_ids": ["PJ-01A", "PJ-01B", "PJ-01C"],
      "flow_ids": ["F02", "F02.1", "F10", "F14"],
      "invariant_ids": [
        "INV-BRIDGE-001",
        "INV-BRIDGE-002",
        "INV-SESSION-001"
      ],
      "executable_suite_ids": ["e2e_browser_setup_first_launch"],
      "current_cheaper_suite_ids": [
        "smoke_browser_launch_chrome",
        "smoke_browser_launch_brave",
        "smoke_browser_launch_edge",
        "int_bridge_launch_only_no_status"
      ],
      "follow_on_bead_ids": []
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
    fn full_repo_suite_directory_supports_provider_acceptance_metadata() {
        let metadata_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/suites");
        let plan = build_plan(
            ValidationLane::Gated,
            &metadata_dir,
            "target/test-artifacts",
        )
        .expect("real suite metadata directory should be plannable");
        let suite_ids = plan.suite_ids();
        for suite_id in [
            "accept_opencode_resume",
            "accept_opencode_dropout_return",
            "accept_codex_reseed",
            "accept_gemini_reseed",
            "accept_claude_reseed",
            "accept_copilot_tier_b",
        ] {
            assert!(
                suite_ids.contains(&suite_id),
                "gated lane plan should include provider acceptance metadata for {suite_id}"
            );
        }
    }

    #[test]
    fn full_repo_suite_directory_preserves_provider_surface_truth_metadata() {
        let metadata_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/suites");
        let suites = discover_suite_metadata(&metadata_dir)
            .expect("real suite metadata directory should be discoverable");

        let provider_surface_truth = suites
            .iter()
            .find(|suite| suite.id == "int_cli_provider_surface_truth")
            .expect("provider surface truth suite metadata should exist");
        assert!(provider_surface_truth
            .invariant_ids
            .iter()
            .any(|id| id == "INV-PROVIDER-001"));
        assert_eq!(
            provider_surface_truth.support_tier,
            "provider_surface_truth"
        );
        assert_eq!(
            provider_surface_truth.support_status,
            roger_test_harness::SupportStatus::Bounded
        );
        assert!(provider_surface_truth.degraded);
        assert!(provider_surface_truth.bounded);
        assert!(provider_surface_truth.preserve_failure_artifacts);
        assert_eq!(
            provider_surface_truth.artifact_retention,
            roger_test_harness::ArtifactRetention::OnFailure
        );

        for (suite_id, support_tier, expected_fixtures) in [
            (
                "accept_codex_reseed",
                "codex_tier_a",
                vec!["fixture_resumebundle_stale_locator"],
            ),
            (
                "accept_gemini_reseed",
                "gemini_tier_a",
                vec!["fixture_resumebundle_stale_locator"],
            ),
            (
                "accept_claude_reseed",
                "claude_tier_a",
                vec!["fixture_resumebundle_stale_locator"],
            ),
            (
                "accept_copilot_tier_b",
                "copilot_tier_b_feature_gated",
                vec![
                    "fixture_copilot_launch_resume",
                    "fixture_copilot_hook_stream",
                    "fixture_copilot_policy_failure",
                    "fixture_copilot_crash_recovery",
                ],
            ),
        ] {
            let suite = suites
                .iter()
                .find(|suite| suite.id == suite_id)
                .expect("suite metadata should exist");
            assert!(
                suite.invariant_ids.iter().any(|id| id == "INV-SESSION-002"),
                "{suite_id} should preserve INV-SESSION-002 wiring"
            );
            assert!(
                suite
                    .invariant_ids
                    .iter()
                    .any(|id| id == "INV-PROVIDER-001"),
                "{suite_id} should preserve INV-PROVIDER-001 wiring"
            );
            assert_eq!(suite.support_tier, support_tier);
            assert_eq!(
                suite.support_status,
                roger_test_harness::SupportStatus::Bounded
            );
            assert!(suite.degraded, "{suite_id} should remain degraded");
            assert!(suite.bounded, "{suite_id} should remain bounded");
            for expected_fixture in expected_fixtures {
                assert!(
                    suite
                        .fixture_families
                        .iter()
                        .any(|fixture| fixture == expected_fixture),
                    "{suite_id} should preserve fixture family {expected_fixture}",
                );
            }
            assert!(suite.preserve_failure_artifacts);
            assert_eq!(
                suite.artifact_retention,
                roger_test_harness::ArtifactRetention::OnFailure
            );
        }
    }

    #[test]
    fn full_repo_release_plan_keeps_smoke_scoped_to_first_class_claims() {
        let metadata_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/suites");
        let plan = build_plan(
            ValidationLane::Release,
            &metadata_dir,
            "target/test-artifacts",
        )
        .expect("real suite metadata directory should be plannable for release");

        let smoke_suites: Vec<_> = plan
            .suites
            .iter()
            .filter(|suite| suite.tier == roger_test_harness::SuiteTier::Smoke)
            .collect();
        let smoke_ids: Vec<_> = smoke_suites.iter().map(|suite| suite.id.as_str()).collect();
        assert_eq!(
            smoke_ids,
            vec![
                "smoke_browser_launch_brave",
                "smoke_browser_launch_chrome",
                "smoke_browser_launch_edge",
                "smoke_opencode_continuity",
            ]
        );

        let opencode_smoke = smoke_suites
            .iter()
            .find(|suite| suite.id == "smoke_opencode_continuity")
            .expect("OpenCode smoke suite metadata should exist");
        assert_eq!(opencode_smoke.support_status, SupportStatus::Blessed);
        assert_eq!(opencode_smoke.support_tier, "opencode_tier_b");
        for flow_id in ["F01.1", "F05", "F17", "F17.1"] {
            assert!(
                opencode_smoke.flow_ids.iter().any(|id| id == flow_id),
                "OpenCode smoke should preserve flow {flow_id}",
            );
        }
        for persona_id in ["PJ-03C", "PJ-04A", "PJ-04B"] {
            assert!(
                opencode_smoke.persona_ids.iter().any(|id| id == persona_id),
                "OpenCode smoke should preserve persona {persona_id}",
            );
        }
        for fixture_family in [
            "fixture_resumebundle_stale_locator",
            "fixture_opencode_dropout_return",
        ] {
            assert!(
                opencode_smoke
                    .fixture_families
                    .iter()
                    .any(|fixture| fixture == fixture_family),
                "OpenCode smoke should preserve fixture family {fixture_family}",
            );
        }

        for browser_smoke_id in [
            "smoke_browser_launch_brave",
            "smoke_browser_launch_chrome",
            "smoke_browser_launch_edge",
        ] {
            let suite = smoke_suites
                .iter()
                .find(|suite| suite.id == browser_smoke_id)
                .expect("browser smoke metadata should exist");
            assert_eq!(suite.support_status, SupportStatus::Bounded);
            assert_eq!(suite.support_tier, "native_messaging_v1");
        }
    }

    #[test]
    fn full_repo_budget_guard_matches_blessed_e2e_policy() {
        let metadata_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/suites");
        let budget_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/AUTOMATED_E2E_BUDGET.json");
        let suites = discover_suite_metadata(&metadata_dir)
            .expect("real suite metadata directory should be discoverable");
        let policy = E2eBudgetPolicy::load(&budget_path)
            .expect("budget policy should load from the real repo fixture");
        let report = evaluate_e2e_budget(&policy, &suites);

        assert_eq!(report.status, BudgetStatus::Ok);
        assert_eq!(
            report.observed_heavyweight_e2e_count,
            policy.blessed_automated_e2e_budget
        );
        assert_eq!(
            report.observed_heavyweight_e2e_count,
            policy.current_planned_blessed_automated_e2e_count
        );
        assert!(report.unexpected_ids.is_empty());
        assert!(report.blessed_ids.contains("E2E-01"));
        assert!(report.blessed_ids.contains("E2E-02"));
        assert!(report.blessed_ids.contains("E2E-03"));
        assert!(report.blessed_ids.contains("E2E-04"));
        assert!(report.blessed_ids.contains("E2E-05"));
        assert!(report.blessed_ids.contains("E2E-06"));
    }

    #[test]
    fn full_repo_suite_directory_preserves_outbound_recovery_metadata() {
        let metadata_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/suites");
        let suites = discover_suite_metadata(&metadata_dir)
            .expect("real suite metadata directory should be discoverable");

        let outbound_audit = suites
            .iter()
            .find(|suite| suite.id == "int_github_outbound_audit")
            .expect("outbound audit suite metadata should exist");
        assert!(outbound_audit
            .invariant_ids
            .iter()
            .any(|id| id == "INV-POST-002"));
        assert!(outbound_audit
            .invariant_ids
            .iter()
            .any(|id| id == "INV-POST-003"));
        assert!(outbound_audit.persona_ids.iter().any(|id| id == "PJ-05B"));
        assert!(outbound_audit
            .fixture_families
            .iter()
            .any(|family| family == "fixture_partial_post_recovery"));
        assert!(outbound_audit.preserve_failure_artifacts);
        assert_eq!(
            outbound_audit.artifact_retention,
            roger_test_harness::ArtifactRetention::OnFailure
        );

        let posting_recovery = suites
            .iter()
            .find(|suite| suite.id == "int_github_posting_safety_recovery")
            .expect("posting safety recovery suite metadata should exist");
        assert!(posting_recovery
            .invariant_ids
            .iter()
            .any(|id| id == "INV-POST-002"));
        assert!(posting_recovery
            .invariant_ids
            .iter()
            .any(|id| id == "INV-POST-003"));
        for persona_id in ["PJ-05B", "PJ-05C", "PJ-05D"] {
            assert!(
                posting_recovery
                    .persona_ids
                    .iter()
                    .any(|id| id == persona_id),
                "posting safety recovery suite should preserve persona owner {persona_id}",
            );
        }
        for family in [
            "fixture_refresh_rebase_target_drift",
            "fixture_github_draft_batch",
            "fixture_partial_post_recovery",
        ] {
            assert!(
                posting_recovery
                    .fixture_families
                    .iter()
                    .any(|fixture| fixture == family),
                "posting safety recovery suite should preserve fixture family {family}",
            );
        }
        assert!(posting_recovery.preserve_failure_artifacts);
        assert_eq!(
            posting_recovery.artifact_retention,
            roger_test_harness::ArtifactRetention::OnFailure
        );
    }

    #[test]
    fn copilot_fixture_families_exist_with_expected_case_ids() {
        let fixtures_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
        let expected_fixtures = [
            (
                "fixture_copilot_launch_resume",
                "copilot_launch_resume_cases.json",
                &[
                    "launch_review_verified_session_id",
                    "resume_stale_locator_reseed",
                ][..],
            ),
            (
                "fixture_copilot_hook_stream",
                "copilot_hook_events.json",
                &["session_start_verified_id", "missing_session_start"][..],
            ),
            (
                "fixture_copilot_transcript_capture",
                "copilot_transcript_refs.json",
                &[
                    "transcript_capture_nominal",
                    "transcript_capture_missing_file",
                ][..],
            ),
            (
                "fixture_copilot_policy_failure",
                "copilot_policy_failure_cases.json",
                &["policy_violation_blocked_tool_use"][..],
            ),
            (
                "fixture_copilot_crash_recovery",
                "copilot_crash_recovery_cases.json",
                &["launch_process_crash_before_session_start"][..],
            ),
        ];

        for (family, case_file, required_case_ids) in expected_fixtures {
            let fixture_dir = fixtures_root.join(family);
            assert!(fixture_dir.is_dir(), "missing fixture directory: {family}");

            let manifest_path = fixture_dir.join("MANIFEST.toml");
            let manifest_raw =
                fs::read_to_string(&manifest_path).expect("failed to read fixture manifest");
            assert!(
                manifest_raw.contains(&format!("family = \"{family}\"")),
                "manifest family mismatch for {family}"
            );

            let case_path = fixture_dir.join(case_file);
            let case_raw = fs::read_to_string(&case_path).expect("failed to read fixture case");
            let case_json: Value =
                serde_json::from_str(&case_raw).expect("parse fixture case json");

            let case_ids: Vec<&str> = ["cases", "events", "records"]
                .into_iter()
                .flat_map(|key| {
                    case_json[key]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|entry| entry["id"].as_str())
                })
                .collect();
            assert!(
                !case_ids.is_empty(),
                "fixture {family} case json must contain one of: cases/events/records"
            );
            let negative_ids: Vec<&str> = case_json["negative_events"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|entry| entry["id"].as_str())
                .collect();

            for required_case_id in required_case_ids {
                assert!(
                    case_ids.contains(required_case_id) || negative_ids.contains(required_case_id),
                    "fixture {family} is missing required case id {required_case_id}"
                );
            }
        }
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
