use std::cell::RefCell;
use std::collections::HashMap;

use roger_app_core::{LaunchAction, LaunchIntent, ReviewTarget, SessionLocator, Surface};
use roger_prompt_engine::stage_execution::{
    ReviewStage, StageExecutionRequest, StageHarness, StageHarnessOutput, StagePrompt,
    execute_review_stage,
};
use roger_storage::{CreateReviewRun, CreateReviewSession, RogerStore};
use tempfile::TempDir;

struct StubStageHarness {
    responses: HashMap<ReviewStage, StageHarnessOutput>,
    calls: RefCell<Vec<ReviewStage>>,
}

impl StubStageHarness {
    fn new(responses: HashMap<ReviewStage, StageHarnessOutput>) -> Self {
        Self {
            responses,
            calls: RefCell::new(Vec::new()),
        }
    }

    #[allow(dead_code)]
    fn calls(&self) -> Vec<ReviewStage> {
        self.calls.borrow().clone()
    }
}

impl StageHarness for StubStageHarness {
    fn execute_stage(
        &self,
        _locator: &SessionLocator,
        stage: ReviewStage,
        _prompt_text: &str,
    ) -> std::result::Result<StageHarnessOutput, String> {
        self.calls.borrow_mut().push(stage);
        self.responses
            .get(&stage)
            .cloned()
            .ok_or_else(|| format!("missing stub output for stage {}", stage.as_str()))
    }
}

fn sample_target() -> ReviewTarget {
    ReviewTarget {
        repository: "owner/repo".to_owned(),
        pull_request_number: 42,
        base_ref: "main".to_owned(),
        head_ref: "feature".to_owned(),
        base_commit: "aaa".to_owned(),
        head_commit: "bbb".to_owned(),
    }
}

fn sample_locator() -> SessionLocator {
    SessionLocator {
        provider: "opencode".to_owned(),
        session_id: "oc-existing".to_owned(),
        invocation_context_json:
            r#"{"mode":"start","review_target":{"repository":"owner/repo","pull_request_number":42}}"#
                .to_owned(),
        captured_at: 10,
        last_tested_at: Some(11),
    }
}

fn setup_store(session_id: &str, run_id: &str) -> (TempDir, RogerStore) {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let store = RogerStore::open(tempdir.path()).expect("open store");
    let target = sample_target();
    let locator = sample_locator();

    store
        .create_review_session(CreateReviewSession {
            id: session_id,
            review_target: &target,
            provider: "opencode",
            session_locator: Some(&locator),
            resume_bundle_artifact_id: None,
            continuity_state: "usable",
            attention_state: "awaiting_review",
            launch_profile_id: Some("profile-open-pr"),
        })
        .expect("create review session");

    store
        .create_review_run(CreateReviewRun {
            id: run_id,
            session_id,
            run_kind: "deep_review",
            repo_snapshot: "head=bbb",
            continuity_quality: "usable",
            session_locator_artifact_id: None,
        })
        .expect("create review run");

    (tempdir, store)
}

fn sample_prompt<'a>(preset_id: &'a str) -> StagePrompt<'a> {
    StagePrompt {
        prompt_preset_id: preset_id,
        resolved_text: "Review this PR for safety and continuity issues.",
        source_surface: "cli",
        explicit_objective: Some("focus on correctness regressions"),
        provider: Some("opencode"),
        model: Some("o4"),
        scope_context_json: Some(r#"{"repo":"owner/repo"}"#),
        config_layer_digest: Some("cfg-materialization"),
        launch_intake_id: Some("launch-materialization"),
    }
}

fn valid_structured_pack() -> String {
    r#"{
  "schema_version": "structured_findings_pack.v1",
  "findings": [
    {
      "fingerprint": "fp-materialized-1",
      "title": "Critical guard is bypassed",
      "summary": "A safety guard does not execute before side effects.",
      "severity": "high",
      "confidence": "high",
      "evidence": [
        {
          "repo_rel_path": "src/lib.rs",
          "start_line": 12,
          "end_line": 16,
          "anchor_state": "exact",
          "evidence_role": "primary"
        }
      ]
    }
  ]
}"#
    .to_owned()
}

fn sample_intent() -> LaunchIntent {
    LaunchIntent {
        action: LaunchAction::ResumeReview,
        source_surface: Surface::Cli,
        objective: Some("review this PR".to_owned()),
        launch_profile_id: Some("profile-open-pr".to_owned()),
        cwd: Some("/tmp/repo".to_owned()),
        worktree_root: None,
    }
}

#[test]
fn structured_stage_materializes_findings_with_provenance_and_evidence() {
    let (_tempdir, store) = setup_store("session-materialized", "run-materialized");
    let locator = sample_locator();
    let harness = StubStageHarness::new(HashMap::from([(
        ReviewStage::Exploration,
        StageHarnessOutput {
            raw_output: "exploration raw transcript".to_owned(),
            structured_pack_json: Some(valid_structured_pack()),
            degraded_reason: None,
        },
    )]));

    let result = execute_review_stage(
        &store,
        &harness,
        StageExecutionRequest {
            review_session_id: "session-materialized",
            review_run_id: "run-materialized",
            stage: ReviewStage::Exploration,
            session_locator: &locator,
            prompt: sample_prompt("preset-exploration"),
            repair_attempt: 0,
            repair_retry_budget: 1,
            actor_id: Some("copper-brook"),
        },
    )
    .expect("stage execution succeeds");

    assert_eq!(result.materialized_findings.len(), 1);
    assert_eq!(result.outcome_metadata.materialized_finding_ids.len(), 1);
    let finding_id = &result.outcome_metadata.materialized_finding_ids[0];

    let stored = store
        .materialized_finding(finding_id)
        .expect("lookup materialized finding")
        .expect("materialized finding exists");
    assert_eq!(stored.session_id, "session-materialized");
    assert_eq!(stored.last_seen_run_id.as_deref(), Some("run-materialized"));
    assert_eq!(stored.first_seen_stage, "exploration");
    assert_eq!(stored.last_seen_stage.as_deref(), Some("exploration"));
    assert_eq!(stored.severity, "high");
    assert_eq!(stored.confidence, "high");

    let run_findings = store
        .materialized_findings_for_run("session-materialized", "run-materialized")
        .expect("list materialized findings for run");
    assert_eq!(run_findings.len(), 1);
    assert_eq!(run_findings[0].id, *finding_id);

    let evidence = store
        .code_evidence_locations_for_finding(finding_id)
        .expect("list evidence");
    assert_eq!(evidence.len(), 1);
    assert_eq!(evidence[0].repo_rel_path, "src/lib.rs");
    assert_eq!(evidence[0].start_line, 12);
    assert_eq!(evidence[0].end_line, Some(16));

    let _ = sample_intent();
}

#[test]
fn degraded_raw_only_stage_does_not_fabricate_materialized_findings() {
    let (_tempdir, store) = setup_store("session-degraded", "run-degraded");
    let locator = sample_locator();
    let harness = StubStageHarness::new(HashMap::from([(
        ReviewStage::FollowUp,
        StageHarnessOutput {
            raw_output: "follow-up raw transcript".to_owned(),
            structured_pack_json: None,
            degraded_reason: Some("provider continuation degraded".to_owned()),
        },
    )]));

    let result = execute_review_stage(
        &store,
        &harness,
        StageExecutionRequest {
            review_session_id: "session-degraded",
            review_run_id: "run-degraded",
            stage: ReviewStage::FollowUp,
            session_locator: &locator,
            prompt: sample_prompt("preset-follow-up"),
            repair_attempt: 0,
            repair_retry_budget: 1,
            actor_id: Some("copper-brook"),
        },
    )
    .expect("degraded stage execution succeeds");

    assert!(result.materialized_findings.is_empty());
    assert!(result.outcome_metadata.materialized_finding_ids.is_empty());
    assert!(result.outcome_metadata.degraded);
    assert_eq!(
        result.outcome_metadata.degraded_reason.as_deref(),
        Some("provider continuation degraded")
    );

    let run_findings = store
        .materialized_findings_for_run("session-degraded", "run-degraded")
        .expect("list materialized findings for degraded run");
    assert!(run_findings.is_empty());
}
