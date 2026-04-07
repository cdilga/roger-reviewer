use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuiteFamily {
    Unit,
    Prop,
    IntStorage,
    IntHarness,
    IntCli,
    IntTui,
    IntBridge,
    IntGithub,
    IntSearch,
    AcceptOpencode,
    AcceptCodex,
    AcceptGemini,
    E2e,
    Smoke,
}

impl SuiteFamily {
    pub fn prefix(self) -> &'static str {
        match self {
            Self::Unit => "unit_",
            Self::Prop => "prop_",
            Self::IntStorage => "int_storage_",
            Self::IntHarness => "int_harness_",
            Self::IntCli => "int_cli_",
            Self::IntTui => "int_tui_",
            Self::IntBridge => "int_bridge_",
            Self::IntGithub => "int_github_",
            Self::IntSearch => "int_search_",
            Self::AcceptOpencode => "accept_opencode_",
            Self::AcceptCodex => "accept_codex_",
            Self::AcceptGemini => "accept_gemini_",
            Self::E2e => "e2e_",
            Self::Smoke => "smoke_",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuiteTier {
    Unit,
    Property,
    Integration,
    Acceptance,
    E2e,
    Smoke,
}

impl SuiteTier {
    pub fn artifact_subdir(self) -> &'static str {
        match self {
            Self::Unit => "unit",
            Self::Property => "property",
            Self::Integration => "integration",
            Self::Acceptance => "acceptance",
            Self::E2e => "e2e",
            Self::Smoke => "release-smoke",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ValidationLane {
    FastLocal,
    Pr,
    Gated,
    Nightly,
    Release,
}

impl Display for ValidationLane {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::FastLocal => "fast-local",
            Self::Pr => "pr",
            Self::Gated => "gated",
            Self::Nightly => "nightly",
            Self::Release => "release",
        };
        f.write_str(name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportStatus {
    Blessed,
    Bounded,
    Degraded,
    ManualOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactRetention {
    Never,
    OnFailure,
    Always,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuiteMetadata {
    pub id: String,
    #[serde(default)]
    pub budget_id: Option<String>,
    pub family: SuiteFamily,
    pub flow_ids: Vec<String>,
    pub fixture_families: Vec<String>,
    pub support_tier: String,
    pub support_status: SupportStatus,
    #[serde(default)]
    pub degraded: bool,
    #[serde(default)]
    pub bounded: bool,
    pub tier: SuiteTier,
    pub preserve_failure_artifacts: bool,
    pub artifact_retention: ArtifactRetention,
}

impl SuiteMetadata {
    pub fn validate(&self) -> Result<(), String> {
        if !self.id.starts_with(self.family.prefix()) {
            return Err(format!(
                "suite id '{}' must start with family prefix '{}'",
                self.id,
                self.family.prefix()
            ));
        }
        if self.flow_ids.is_empty() {
            return Err(format!(
                "suite '{}' must declare at least one flow id",
                self.id
            ));
        }
        if self.support_tier.trim().is_empty() {
            return Err(format!("suite '{}' must declare support_tier", self.id));
        }
        if matches!(
            self.family,
            SuiteFamily::AcceptOpencode
                | SuiteFamily::AcceptCodex
                | SuiteFamily::AcceptGemini
                | SuiteFamily::IntBridge
                | SuiteFamily::IntHarness
        ) && !self.preserve_failure_artifacts
        {
            return Err(format!(
                "suite '{}' must preserve failure artifacts for {:?}",
                self.id, self.family
            ));
        }
        if matches!(self.family, SuiteFamily::E2e) && !self.preserve_failure_artifacts {
            return Err(format!(
                "suite '{}' must preserve failure artifacts for the blessed e2e lane",
                self.id
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationPlan {
    pub lane: ValidationLane,
    pub allowed_tiers: Vec<SuiteTier>,
    pub failure_artifact_root: PathBuf,
    pub suites: Vec<SuiteMetadata>,
}

impl ValidationPlan {
    pub fn suite_ids(&self) -> Vec<&str> {
        self.suites.iter().map(|suite| suite.id.as_str()).collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetedE2e {
    pub id: String,
    pub name: String,
    pub status: String,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct E2eBudgetPolicy {
    pub policy_version: u32,
    pub release_line: String,
    pub blessed_automated_e2e_budget: usize,
    pub current_planned_blessed_automated_e2e_count: usize,
    pub warning_mode: String,
    pub future_ci_mode: String,
    pub blessed_e2e_ids: Vec<BudgetedE2e>,
    pub required_justification_fields_for_growth: Vec<String>,
}

impl E2eBudgetPolicy {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let raw = fs::read_to_string(path.as_ref()).map_err(|err| err.to_string())?;
        serde_json::from_str(&raw).map_err(|err| err.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetReport {
    pub status: BudgetStatus,
    pub observed_heavyweight_e2e_count: usize,
    pub blessed_ids: BTreeSet<String>,
    pub unexpected_ids: Vec<String>,
    pub over_budget_by: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetStatus {
    Ok,
    Warn,
    Fail,
}

pub fn plan_for_lane(
    lane: ValidationLane,
    suites: &[SuiteMetadata],
    artifact_root: impl AsRef<Path>,
) -> ValidationPlan {
    let allowed_tiers = match lane {
        ValidationLane::FastLocal => {
            vec![SuiteTier::Unit, SuiteTier::Property, SuiteTier::Integration]
        }
        ValidationLane::Pr => vec![SuiteTier::Unit, SuiteTier::Property, SuiteTier::Integration],
        ValidationLane::Gated => vec![
            SuiteTier::Unit,
            SuiteTier::Property,
            SuiteTier::Integration,
            SuiteTier::Acceptance,
            SuiteTier::E2e,
        ],
        ValidationLane::Nightly => vec![
            SuiteTier::Unit,
            SuiteTier::Property,
            SuiteTier::Integration,
            SuiteTier::Acceptance,
            SuiteTier::E2e,
            SuiteTier::Smoke,
        ],
        ValidationLane::Release => vec![
            SuiteTier::Unit,
            SuiteTier::Property,
            SuiteTier::Integration,
            SuiteTier::Acceptance,
            SuiteTier::E2e,
            SuiteTier::Smoke,
        ],
    };

    let planned_suites = suites
        .iter()
        .filter(|suite| allowed_tiers.contains(&suite.tier))
        .cloned()
        .collect();

    ValidationPlan {
        lane,
        allowed_tiers,
        failure_artifact_root: artifact_root.as_ref().join("failures"),
        suites: planned_suites,
    }
}

pub fn artifact_dir(
    root: impl AsRef<Path>,
    suite: &SuiteMetadata,
    failed: bool,
    test_name: &str,
) -> PathBuf {
    if failed && suite.preserve_failure_artifacts {
        return root
            .as_ref()
            .join("failures")
            .join(&suite.id)
            .join(test_name);
    }

    root.as_ref()
        .join(suite.tier.artifact_subdir())
        .join(&suite.id)
}

pub fn evaluate_e2e_budget(policy: &E2eBudgetPolicy, suites: &[SuiteMetadata]) -> BudgetReport {
    let observed_ids: BTreeSet<String> = suites
        .iter()
        .filter(|suite| suite.tier == SuiteTier::E2e)
        .map(|suite| suite.budget_id.clone().unwrap_or_else(|| suite.id.clone()))
        .collect();

    let blessed_ids: BTreeSet<String> = policy
        .blessed_e2e_ids
        .iter()
        .map(|entry| entry.id.clone())
        .collect();

    let unexpected_ids: Vec<String> = observed_ids
        .iter()
        .filter(|id| !blessed_ids.contains(*id))
        .cloned()
        .collect();

    let observed_count = observed_ids.len();
    let over_budget_by = observed_count.saturating_sub(policy.blessed_automated_e2e_budget);

    let status = if over_budget_by > 0 || !unexpected_ids.is_empty() {
        if policy.future_ci_mode.contains("fail") {
            BudgetStatus::Fail
        } else {
            BudgetStatus::Warn
        }
    } else {
        BudgetStatus::Ok
    };

    BudgetReport {
        status,
        observed_heavyweight_e2e_count: observed_count,
        blessed_ids,
        unexpected_ids,
        over_budget_by,
    }
}

pub fn load_suite_metadata(path: impl AsRef<Path>) -> Result<SuiteMetadata, String> {
    let raw = fs::read_to_string(path.as_ref()).map_err(|err| err.to_string())?;
    let suite: SuiteMetadata = toml::from_str(&raw).map_err(|err| err.to_string())?;
    suite.validate()?;
    Ok(suite)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_suite(id: &str, family: SuiteFamily, tier: SuiteTier) -> SuiteMetadata {
        SuiteMetadata {
            id: id.to_string(),
            budget_id: None,
            family,
            flow_ids: vec!["F01".into()],
            fixture_families: vec!["fixture_repo_compact_review".into()],
            support_tier: "opencode_tier_b".into(),
            support_status: SupportStatus::Blessed,
            degraded: false,
            bounded: false,
            tier,
            preserve_failure_artifacts: !matches!(tier, SuiteTier::Unit | SuiteTier::Property),
            artifact_retention: ArtifactRetention::OnFailure,
        }
    }

    #[test]
    fn validates_prefixes() {
        let suite = sample_suite(
            "wrong_name",
            SuiteFamily::IntHarness,
            SuiteTier::Integration,
        );
        assert!(suite.validate().is_err());
    }

    #[test]
    fn codex_acceptance_family_has_expected_prefix_and_validation() {
        let suite = sample_suite(
            "accept_codex_reseed",
            SuiteFamily::AcceptCodex,
            SuiteTier::Acceptance,
        );
        assert_eq!(suite.family.prefix(), "accept_codex_");
        assert!(suite.validate().is_ok());
    }

    #[test]
    fn codex_acceptance_family_requires_failure_artifacts() {
        let mut suite = sample_suite(
            "accept_codex_reseed",
            SuiteFamily::AcceptCodex,
            SuiteTier::Acceptance,
        );
        suite.preserve_failure_artifacts = false;
        let err = suite
            .validate()
            .expect_err("codex acceptance must fail closed");
        assert!(
            err.contains("must preserve failure artifacts"),
            "expected explicit failure-artifact error, got: {err}"
        );
    }

    #[test]
    fn gated_lane_includes_acceptance_and_e2e() {
        let suites = vec![
            sample_suite("unit_domain_rules", SuiteFamily::Unit, SuiteTier::Unit),
            sample_suite(
                "accept_opencode_resume",
                SuiteFamily::AcceptOpencode,
                SuiteTier::Acceptance,
            ),
            sample_suite(
                "e2e_core_review_happy_path",
                SuiteFamily::E2e,
                SuiteTier::E2e,
            ),
        ];

        let plan = plan_for_lane(ValidationLane::Gated, &suites, "target/test-artifacts");
        assert_eq!(
            plan.suite_ids(),
            vec![
                "unit_domain_rules",
                "accept_opencode_resume",
                "e2e_core_review_happy_path"
            ]
        );
    }

    #[test]
    fn budget_flags_unplanned_e2e_growth() {
        let policy = E2eBudgetPolicy {
            policy_version: 1,
            release_line: "0.1.x".into(),
            blessed_automated_e2e_budget: 1,
            current_planned_blessed_automated_e2e_count: 1,
            warning_mode: "warn_on_unjustified_growth".into(),
            future_ci_mode: "fail_on_unjustified_growth".into(),
            blessed_e2e_ids: vec![BudgetedE2e {
                id: "e2e_core_review_happy_path".into(),
                name: "Core review happy path".into(),
                status: "planned".into(),
                notes: String::new(),
            }],
            required_justification_fields_for_growth: vec![],
        };

        let suites = vec![
            sample_suite(
                "e2e_core_review_happy_path",
                SuiteFamily::E2e,
                SuiteTier::E2e,
            ),
            sample_suite("e2e_second_path", SuiteFamily::E2e, SuiteTier::E2e),
        ];

        let report = evaluate_e2e_budget(&policy, &suites);
        assert_eq!(report.status, BudgetStatus::Fail);
        assert_eq!(report.over_budget_by, 1);
        assert_eq!(report.unexpected_ids, vec!["e2e_second_path"]);
    }
}
