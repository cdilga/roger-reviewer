use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub mod stage_execution;

pub const STRUCTURED_FINDINGS_PACK_V1: &str = "structured_findings_pack.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageState {
    Structured,
    Partial,
    RawOnly,
    RepairNeeded,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairFailureKind {
    MissingPack,
    MalformedSyntax,
    SchemaDrift,
    MissingField,
    InvalidFieldValue,
    InvalidAnchor,
    ContradictoryState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairAction {
    None,
    RetryRepair,
    SurfaceRawOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepairIssue {
    pub kind: RepairFailureKind,
    pub path: String,
    pub message: String,
    pub repairable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationContext<'a> {
    pub review_session_id: &'a str,
    pub review_run_id: &'a str,
    pub repair_attempt: u8,
    pub repair_retry_budget: u8,
}

impl<'a> ValidationContext<'a> {
    fn retries_remaining(&self) -> bool {
        self.repair_attempt < self.repair_retry_budget
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructuredFindingsPackV1 {
    pub schema_version: String,
    #[serde(default)]
    pub findings: Vec<FindingInput>,
    #[serde(default)]
    pub raw_summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindingInput {
    pub title: Option<String>,
    pub summary: Option<String>,
    pub severity: Option<String>,
    pub confidence: Option<String>,
    #[serde(default)]
    pub evidence: Vec<CodeEvidenceLocationInput>,
    #[serde(default)]
    pub suggested_draft: Option<String>,
    #[serde(default)]
    pub triage_state: Option<String>,
    #[serde(default)]
    pub outbound_state: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeEvidenceLocationInput {
    pub repo_rel_path: Option<String>,
    pub start_line: Option<u32>,
    #[serde(default)]
    pub end_line: Option<u32>,
    #[serde(default)]
    pub evidence_role: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedCodeEvidenceLocation {
    pub repo_rel_path: String,
    pub start_line: u32,
    pub end_line: Option<u32>,
    pub evidence_role: String,
    pub anchor_state: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedFinding {
    pub fingerprint: String,
    pub title: String,
    pub normalized_summary: String,
    pub severity: String,
    pub confidence: String,
    pub evidence: Vec<NormalizedCodeEvidenceLocation>,
    pub suggested_draft: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RogerFindingRow {
    pub review_session_id: String,
    pub review_run_id: String,
    pub fingerprint: String,
    pub title: String,
    pub normalized_summary: String,
    pub severity: String,
    pub confidence: String,
    pub triage_state: String,
    pub outbound_state: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationOutcome {
    pub stage_state: StageState,
    pub findings: Vec<NormalizedFinding>,
    pub finding_rows: Vec<RogerFindingRow>,
    pub issues: Vec<RepairIssue>,
    pub repair_action: RepairAction,
    pub raw_summary: Option<String>,
}

impl ValidationOutcome {
    pub fn refresh_candidates(&self) -> &[NormalizedFinding] {
        if matches!(
            self.stage_state,
            StageState::Structured | StageState::Partial
        ) {
            &self.findings
        } else {
            &[]
        }
    }

    pub fn draft_candidates(&self) -> Vec<&NormalizedFinding> {
        if matches!(
            self.stage_state,
            StageState::Structured | StageState::Partial
        ) {
            self.findings
                .iter()
                .filter(|finding| finding.suggested_draft.is_some())
                .collect()
        } else {
            Vec::new()
        }
    }
}

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("json parse error: {0}")]
    Json(#[from] serde_json::Error),
}

pub fn validate_structured_findings_pack(
    ctx: &ValidationContext<'_>,
    structured_pack_json: Option<&str>,
    raw_output: &str,
) -> Result<ValidationOutcome, ValidationError> {
    let Some(structured_pack_json) = structured_pack_json.filter(|json| !json.trim().is_empty())
    else {
        return Ok(raw_only_outcome(
            ctx,
            raw_output,
            RepairFailureKind::MissingPack,
            "$".to_string(),
            "structured findings pack missing".to_string(),
        ));
    };

    let pack: StructuredFindingsPackV1 = match serde_json::from_str(structured_pack_json) {
        Ok(pack) => pack,
        Err(err) => {
            return Ok(if ctx.retries_remaining() {
                ValidationOutcome {
                    stage_state: StageState::RepairNeeded,
                    findings: Vec::new(),
                    finding_rows: Vec::new(),
                    issues: vec![RepairIssue {
                        kind: RepairFailureKind::MalformedSyntax,
                        path: "$".to_string(),
                        message: err.to_string(),
                        repairable: true,
                    }],
                    repair_action: RepairAction::RetryRepair,
                    raw_summary: Some(raw_output.to_string()),
                }
            } else {
                raw_only_outcome(
                    ctx,
                    raw_output,
                    RepairFailureKind::MalformedSyntax,
                    "$".to_string(),
                    err.to_string(),
                )
            });
        }
    };

    if pack.schema_version != STRUCTURED_FINDINGS_PACK_V1 {
        return Ok(if ctx.retries_remaining() {
            ValidationOutcome {
                stage_state: StageState::RepairNeeded,
                findings: Vec::new(),
                finding_rows: Vec::new(),
                issues: vec![RepairIssue {
                    kind: RepairFailureKind::SchemaDrift,
                    path: "$.schema_version".to_string(),
                    message: format!(
                        "expected {}, got {}",
                        STRUCTURED_FINDINGS_PACK_V1, pack.schema_version
                    ),
                    repairable: true,
                }],
                repair_action: RepairAction::RetryRepair,
                raw_summary: pack.raw_summary.or_else(|| Some(raw_output.to_string())),
            }
        } else {
            raw_only_outcome(
                ctx,
                raw_output,
                RepairFailureKind::SchemaDrift,
                "$.schema_version".to_string(),
                format!(
                    "expected {}, got {}",
                    STRUCTURED_FINDINGS_PACK_V1, pack.schema_version
                ),
            )
        });
    }

    let mut normalized_findings = Vec::new();
    let mut finding_rows = Vec::new();
    let mut issues = Vec::new();

    for (index, finding) in pack.findings.iter().enumerate() {
        let path_prefix = format!("$.findings[{index}]");
        let Some(title) = finding
            .title
            .clone()
            .filter(|value| !value.trim().is_empty())
        else {
            issues.push(issue(
                RepairFailureKind::MissingField,
                format!("{path_prefix}.title"),
                "missing title",
                true,
            ));
            continue;
        };
        let Some(summary) = finding
            .summary
            .clone()
            .filter(|value| !value.trim().is_empty())
        else {
            issues.push(issue(
                RepairFailureKind::MissingField,
                format!("{path_prefix}.summary"),
                "missing summary",
                true,
            ));
            continue;
        };
        let severity = normalize_level(
            finding.severity.as_deref(),
            &["low", "medium", "high", "critical"],
            "medium",
        )?;
        if severity.1 {
            issues.push(issue(
                RepairFailureKind::InvalidFieldValue,
                format!("{path_prefix}.severity"),
                "invalid severity; defaulted to medium",
                true,
            ));
        }
        let confidence = normalize_level(
            finding.confidence.as_deref(),
            &["low", "medium", "high"],
            "medium",
        )?;
        if confidence.1 {
            issues.push(issue(
                RepairFailureKind::InvalidFieldValue,
                format!("{path_prefix}.confidence"),
                "invalid confidence; defaulted to medium",
                true,
            ));
        }

        if finding
            .triage_state
            .as_deref()
            .is_some_and(|state| state != "new")
        {
            issues.push(issue(
                RepairFailureKind::ContradictoryState,
                format!("{path_prefix}.triage_state"),
                "provider pack may not set persisted triage state",
                false,
            ));
        }
        if finding
            .outbound_state
            .as_deref()
            .is_some_and(|state| state != "not_drafted")
        {
            issues.push(issue(
                RepairFailureKind::ContradictoryState,
                format!("{path_prefix}.outbound_state"),
                "provider pack may not set persisted outbound state",
                false,
            ));
        }

        let mut normalized_evidence = Vec::new();
        for (evidence_index, evidence) in finding.evidence.iter().enumerate() {
            let evidence_path = format!("{path_prefix}.evidence[{evidence_index}]");
            let Some(repo_rel_path) = evidence
                .repo_rel_path
                .clone()
                .filter(|value| !value.trim().is_empty())
            else {
                issues.push(issue(
                    RepairFailureKind::InvalidAnchor,
                    format!("{evidence_path}.repo_rel_path"),
                    "missing repo-relative path",
                    true,
                ));
                continue;
            };
            let Some(start_line) = evidence.start_line.filter(|line| *line > 0) else {
                issues.push(issue(
                    RepairFailureKind::InvalidAnchor,
                    format!("{evidence_path}.start_line"),
                    "start_line must be >= 1",
                    true,
                ));
                continue;
            };
            normalized_evidence.push(NormalizedCodeEvidenceLocation {
                repo_rel_path,
                start_line,
                end_line: evidence.end_line,
                evidence_role: evidence
                    .evidence_role
                    .clone()
                    .unwrap_or_else(|| "primary".to_string()),
                anchor_state: "valid".to_string(),
            });
        }

        let fingerprint = fingerprint_for(&title, &summary, &normalized_evidence);
        let normalized = NormalizedFinding {
            fingerprint: fingerprint.clone(),
            title: title.clone(),
            normalized_summary: summary.clone(),
            severity: severity.0,
            confidence: confidence.0,
            evidence: normalized_evidence,
            suggested_draft: finding
                .suggested_draft
                .clone()
                .filter(|value| !value.trim().is_empty()),
        };
        finding_rows.push(RogerFindingRow {
            review_session_id: ctx.review_session_id.to_string(),
            review_run_id: ctx.review_run_id.to_string(),
            fingerprint: fingerprint.clone(),
            title,
            normalized_summary: summary,
            severity: normalized.severity.clone(),
            confidence: normalized.confidence.clone(),
            triage_state: "new".to_string(),
            outbound_state: "not_drafted".to_string(),
        });
        normalized_findings.push(normalized);
    }

    let stage_state = if normalized_findings.is_empty() {
        if issues.is_empty() {
            StageState::RawOnly
        } else if ctx.retries_remaining() {
            StageState::RepairNeeded
        } else {
            StageState::RawOnly
        }
    } else if issues.is_empty() {
        StageState::Structured
    } else {
        StageState::Partial
    };

    let repair_action = match stage_state {
        StageState::RepairNeeded => RepairAction::RetryRepair,
        StageState::RawOnly => RepairAction::SurfaceRawOnly,
        _ => RepairAction::None,
    };

    Ok(ValidationOutcome {
        stage_state,
        findings: normalized_findings,
        finding_rows,
        issues,
        repair_action,
        raw_summary: pack.raw_summary.or_else(|| Some(raw_output.to_string())),
    })
}

fn raw_only_outcome(
    ctx: &ValidationContext<'_>,
    raw_output: &str,
    kind: RepairFailureKind,
    path: String,
    message: String,
) -> ValidationOutcome {
    ValidationOutcome {
        stage_state: if ctx.retries_remaining() {
            StageState::RepairNeeded
        } else {
            StageState::RawOnly
        },
        findings: Vec::new(),
        finding_rows: Vec::new(),
        issues: vec![issue(kind, path, message, true)],
        repair_action: if ctx.retries_remaining() {
            RepairAction::RetryRepair
        } else {
            RepairAction::SurfaceRawOnly
        },
        raw_summary: Some(raw_output.to_string()),
    }
}

fn issue(
    kind: RepairFailureKind,
    path: String,
    message: impl Into<String>,
    repairable: bool,
) -> RepairIssue {
    RepairIssue {
        kind,
        path,
        message: message.into(),
        repairable,
    }
}

fn normalize_level(
    value: Option<&str>,
    allowed: &[&str],
    default: &str,
) -> Result<(String, bool), serde_json::Error> {
    match value {
        Some(candidate) if allowed.contains(&candidate) => Ok((candidate.to_string(), false)),
        Some(_) => Ok((default.to_string(), true)),
        None => Ok((default.to_string(), false)),
    }
}

fn fingerprint_for(
    title: &str,
    summary: &str,
    evidence: &[NormalizedCodeEvidenceLocation],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(title.as_bytes());
    hasher.update(b"\n");
    hasher.update(summary.as_bytes());
    for location in evidence {
        hasher.update(b"\n");
        hasher.update(location.repo_rel_path.as_bytes());
        hasher.update(location.start_line.to_be_bytes());
    }
    format!("fp:{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> ValidationContext<'static> {
        ValidationContext {
            review_session_id: "session-1",
            review_run_id: "run-1",
            repair_attempt: 0,
            repair_retry_budget: 1,
        }
    }

    #[test]
    fn valid_pack_normalizes_into_roger_finding_rows() {
        let raw = "raw provider output";
        let json = r#"{
          "schema_version":"structured_findings_pack.v1",
          "findings":[
            {
              "title":"Potential stale approval token",
              "summary":"Approval token may survive refresh after head moves.",
              "severity":"high",
              "confidence":"high",
              "suggested_draft":"Please invalidate the batch on head drift.",
              "evidence":[
                {"repo_rel_path":"src/review.rs","start_line":42,"evidence_role":"primary"}
              ]
            }
          ]
        }"#;

        let outcome = validate_structured_findings_pack(&ctx(), Some(json), raw).unwrap();
        assert_eq!(outcome.stage_state, StageState::Structured);
        assert_eq!(outcome.findings.len(), 1);
        assert_eq!(outcome.finding_rows.len(), 1);
        assert_eq!(outcome.finding_rows[0].triage_state, "new");
        assert_eq!(outcome.finding_rows[0].outbound_state, "not_drafted");
        assert_eq!(outcome.draft_candidates().len(), 1);
    }

    #[test]
    fn partial_pack_salvages_valid_findings_and_invalid_anchors() {
        let json = r#"{
          "schema_version":"structured_findings_pack.v1",
          "findings":[
            {
              "title":"Good finding",
              "summary":"One invalid anchor should not drop the finding.",
              "severity":"critical",
              "confidence":"high",
              "evidence":[
                {"repo_rel_path":"src/lib.rs","start_line":10,"evidence_role":"primary"},
                {"repo_rel_path":"","start_line":0,"evidence_role":"supporting"}
              ]
            },
            {
              "title":"Broken finding",
              "severity":"bogus"
            }
          ]
        }"#;

        let outcome =
            validate_structured_findings_pack(&ctx(), Some(json), "raw fallback").unwrap();
        assert_eq!(outcome.stage_state, StageState::Partial);
        assert_eq!(outcome.findings.len(), 1);
        assert_eq!(outcome.findings[0].evidence.len(), 1);
        assert!(
            outcome
                .issues
                .iter()
                .any(|issue| issue.kind == RepairFailureKind::InvalidAnchor)
        );
        assert!(
            outcome
                .issues
                .iter()
                .any(|issue| issue.kind == RepairFailureKind::MissingField)
        );
        assert_eq!(outcome.refresh_candidates().len(), 1);
    }

    #[test]
    fn malformed_pack_uses_bounded_repair_before_raw_only() {
        let retry_outcome =
            validate_structured_findings_pack(&ctx(), Some("{not json"), "raw text").unwrap();
        assert_eq!(retry_outcome.stage_state, StageState::RepairNeeded);
        assert_eq!(retry_outcome.repair_action, RepairAction::RetryRepair);
        assert!(retry_outcome.draft_candidates().is_empty());

        let exhausted_ctx = ValidationContext {
            repair_attempt: 1,
            ..ctx()
        };
        let raw_only =
            validate_structured_findings_pack(&exhausted_ctx, Some("{not json"), "raw text")
                .unwrap();
        assert_eq!(raw_only.stage_state, StageState::RawOnly);
        assert_eq!(raw_only.repair_action, RepairAction::SurfaceRawOnly);
        assert!(raw_only.refresh_candidates().is_empty());
    }

    #[test]
    fn missing_pack_preserves_raw_only_without_fake_drafts() {
        let outcome = validate_structured_findings_pack(&ctx(), None, "raw transcript").unwrap();
        assert_eq!(outcome.stage_state, StageState::RepairNeeded);
        assert!(outcome.findings.is_empty());
        assert!(outcome.finding_rows.is_empty());
        assert!(outcome.draft_candidates().is_empty());
        assert_eq!(outcome.raw_summary.as_deref(), Some("raw transcript"));
    }

    #[test]
    fn contradictory_persisted_state_is_flagged_and_not_materialized() {
        let json = r#"{
          "schema_version":"structured_findings_pack.v1",
          "findings":[
            {
              "title":"Stateful finding",
              "summary":"Provider tried to set persisted state.",
              "triage_state":"accepted",
              "outbound_state":"posted"
            }
          ]
        }"#;

        let outcome =
            validate_structured_findings_pack(&ctx(), Some(json), "raw state output").unwrap();
        assert_eq!(outcome.stage_state, StageState::Partial);
        assert_eq!(outcome.finding_rows.len(), 1);
        assert_eq!(outcome.finding_rows[0].triage_state, "new");
        assert_eq!(outcome.finding_rows[0].outbound_state, "not_drafted");
        assert!(
            outcome
                .issues
                .iter()
                .any(|issue| issue.kind == RepairFailureKind::ContradictoryState)
        );
    }
}
