use roger_app_core::{
    FindingsBoundaryInput, FindingsBoundaryState, RepairIssueCode,
    validate_structured_findings_boundary,
};

#[test]
fn valid_pack_normalizes_and_enables_refresh_and_drafts() {
    let result = validate_structured_findings_boundary(FindingsBoundaryInput {
        raw_output_artifact_id: Some("artifact-raw-1"),
        pack_json: Some(
            r#"{
                "schema_version": "structured_findings_pack/v1",
                "stage": "deep_review",
                "findings": [
                    {
                        "fingerprint": "f-1",
                        "title": "Potential nil dereference",
                        "normalized_summary": "call path can dereference nil response",
                        "severity": "high",
                        "confidence": "medium",
                        "code_evidence": [
                            {
                                "evidence_role": "Primary",
                                "repo_rel_path": "src/lib.rs",
                                "start_line": 42,
                                "start_column": 9,
                                "end_line": 42,
                                "end_column": 24,
                                "anchor_digest": "abc123"
                            }
                        ]
                    }
                ]
            }"#,
        ),
        repair_attempt: 0,
        retry_budget: 2,
    });

    assert_eq!(result.state, FindingsBoundaryState::Structured);
    assert!(result.issues.is_empty());
    assert_eq!(result.refresh_candidates().unwrap().len(), 1);
    assert_eq!(result.draft_candidates().unwrap().len(), 1);
}

#[test]
fn partial_pack_salvages_valid_findings_and_degrades_invalid_anchors() {
    let result = validate_structured_findings_boundary(FindingsBoundaryInput {
        raw_output_artifact_id: Some("artifact-raw-2"),
        pack_json: Some(
            r#"{
                "schema_version": "structured_findings_pack/v1",
                "stage": "deep_review",
                "findings": [
                    {
                        "fingerprint": "f-valid",
                        "title": "Off-by-one read",
                        "normalized_summary": "loop reads one byte past buffer end",
                        "severity": "medium",
                        "confidence": "high",
                        "code_evidence": [
                            {
                                "evidence_role": "Primary",
                                "repo_rel_path": "",
                                "start_line": 0,
                                "anchor_digest": null
                            },
                            {
                                "evidence_role": "Supporting",
                                "repo_rel_path": "src/buffer.rs",
                                "start_line": 18,
                                "end_line": 19,
                                "anchor_digest": "digest-1"
                            }
                        ]
                    },
                    {
                        "fingerprint": "f-bad",
                        "title": "",
                        "normalized_summary": "",
                        "severity": "low",
                        "confidence": "low",
                        "code_evidence": []
                    }
                ]
            }"#,
        ),
        repair_attempt: 0,
        retry_budget: 2,
    });

    assert_eq!(result.state, FindingsBoundaryState::Partial);
    assert_eq!(
        result.raw_output_artifact_id.as_deref(),
        Some("artifact-raw-2")
    );
    assert_eq!(result.refresh_candidates().unwrap().len(), 1);
    assert_eq!(result.draft_candidates().unwrap().len(), 1);
    assert_eq!(result.issues.len(), 2);
    assert!(
        result
            .issues
            .iter()
            .any(|issue| issue.code == RepairIssueCode::InvalidAnchor)
    );
    assert!(
        result
            .issues
            .iter()
            .any(|issue| issue.code == RepairIssueCode::InvalidFieldValue)
    );
}

#[test]
fn raw_only_result_preserves_raw_artifact_without_fake_findings_or_drafts() {
    let result = validate_structured_findings_boundary(FindingsBoundaryInput {
        raw_output_artifact_id: Some("artifact-raw-only"),
        pack_json: None,
        repair_attempt: 0,
        retry_budget: 2,
    });

    assert_eq!(result.state, FindingsBoundaryState::RawOnly);
    assert_eq!(
        result.raw_output_artifact_id.as_deref(),
        Some("artifact-raw-only")
    );
    assert!(result.validated_pack.is_none());
    assert!(result.refresh_candidates().is_err());
    assert!(result.draft_candidates().is_err());
}

#[test]
fn malformed_pack_enters_repair_needed_until_retry_budget_is_exhausted() {
    let first_attempt = validate_structured_findings_boundary(FindingsBoundaryInput {
        raw_output_artifact_id: Some("artifact-raw-3"),
        pack_json: Some("{\"schema_version\":\"structured_findings_pack/v1\","),
        repair_attempt: 0,
        retry_budget: 1,
    });
    assert_eq!(first_attempt.state, FindingsBoundaryState::RepairNeeded);
    assert_eq!(
        first_attempt.raw_output_artifact_id.as_deref(),
        Some("artifact-raw-3")
    );
    assert!(first_attempt.should_retry);
    assert!(first_attempt.refresh_candidates().is_err());
    assert_eq!(
        first_attempt.issues[0].code,
        RepairIssueCode::MalformedSyntax
    );

    let exhausted = validate_structured_findings_boundary(FindingsBoundaryInput {
        raw_output_artifact_id: Some("artifact-raw-3"),
        pack_json: Some("{\"schema_version\":\"structured_findings_pack/v1\","),
        repair_attempt: 1,
        retry_budget: 1,
    });
    assert_eq!(exhausted.state, FindingsBoundaryState::Failed);
    assert_eq!(
        exhausted.raw_output_artifact_id.as_deref(),
        Some("artifact-raw-3")
    );
    assert!(!exhausted.should_retry);
}
