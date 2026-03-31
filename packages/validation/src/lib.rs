pub mod calver;

use roger_test_harness::{
    BudgetReport, E2eBudgetPolicy, SuiteMetadata, ValidationLane, ValidationPlan, artifact_dir,
    evaluate_e2e_budget, load_suite_metadata, plan_for_lane,
};
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
  "blessed_automated_e2e_budget": 1,
  "current_planned_blessed_automated_e2e_count": 1,
  "warning_mode": "warn_on_unjustified_growth",
  "future_ci_mode": "fail_on_unjustified_growth",
  "blessed_e2e_ids": [
    {
      "id": "E2E-01",
      "name": "Core review happy path",
      "status": "planned",
      "notes": "Blessed path"
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
}
