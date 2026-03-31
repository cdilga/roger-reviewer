use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use roger_app_core::{LaunchAction, LaunchIntent, ReviewTarget, SessionLocator, Surface};
use roger_prompt_engine::stage_execution::{
    PROMPT_INVOKED_EVENT_TYPE, ReviewStage, StageExecutionRequest, StageHarness,
    StageHarnessOutput, StageOutcomeMetadata, StagePrompt, execute_review_stage,
};
use roger_prompt_engine::{RepairAction, StageState};
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

fn valid_structured_pack(title: &str, summary: &str) -> String {
    format!(
        r#"{{
  "schema_version": "structured_findings_pack.v1",
  "findings": [
    {{
      "title": "{title}",
      "summary": "{summary}",
      "severity": "high",
      "confidence": "medium",
      "evidence": [
        {{ "repo_rel_path": "src/lib.rs", "start_line": 10 }}
      ]
    }}
  ]
}}"#
    )
}

#[test]
fn stage_passes_run_independently_and_capture_raw_output_even_when_degraded() {
    let (_tempdir, store) = setup_store("session-1", "run-1");
    let locator = sample_locator();
    let harness = StubStageHarness::new(HashMap::from([
        (
            ReviewStage::Exploration,
            StageHarnessOutput {
                raw_output: "exploration raw transcript".to_owned(),
                structured_pack_json: Some(valid_structured_pack(
                    "Exploration finding",
                    "Something needs attention.",
                )),
                degraded_reason: None,
            },
        ),
        (
            ReviewStage::DeepReview,
            StageHarnessOutput {
                raw_output: "deep review raw transcript".to_owned(),
                structured_pack_json: None,
                degraded_reason: None,
            },
        ),
        (
            ReviewStage::FollowUp,
            StageHarnessOutput {
                raw_output: "follow-up raw transcript".to_owned(),
                structured_pack_json: None,
                degraded_reason: Some("provider flagged degraded continuation".to_owned()),
            },
        ),
    ]));

    let exploration = execute_review_stage(
        &store,
        &harness,
        StageExecutionRequest {
            review_session_id: "session-1",
            review_run_id: "run-1",
            stage: ReviewStage::Exploration,
            session_locator: &locator,
            prompt: StagePrompt {
                prompt_preset_id: "preset-exploration",
                resolved_text: "Explore key review risks.",
                source_surface: "cli",
                explicit_objective: Some("focus on regressions"),
                provider: Some("opencode"),
                model: Some("o4"),
                scope_context_json: Some(r#"{"repo":"owner/repo"}"#),
                config_layer_digest: Some("cfg-1"),
                launch_intake_id: Some("launch-1"),
            },
            repair_attempt: 1,
            repair_retry_budget: 1,
            actor_id: Some("azure-river"),
        },
    )
    .expect("exploration execution");

    let deep_review = execute_review_stage(
        &store,
        &harness,
        StageExecutionRequest {
            review_session_id: "session-1",
            review_run_id: "run-1",
            stage: ReviewStage::DeepReview,
            session_locator: &locator,
            prompt: StagePrompt {
                prompt_preset_id: "preset-deep-review",
                resolved_text: "Perform deep review for safety and approval gates.",
                source_surface: "cli",
                explicit_objective: None,
                provider: Some("opencode"),
                model: Some("o4"),
                scope_context_json: None,
                config_layer_digest: Some("cfg-1"),
                launch_intake_id: Some("launch-1"),
            },
            repair_attempt: 1,
            repair_retry_budget: 1,
            actor_id: Some("azure-river"),
        },
    )
    .expect("deep review execution");

    let follow_up = execute_review_stage(
        &store,
        &harness,
        StageExecutionRequest {
            review_session_id: "session-1",
            review_run_id: "run-1",
            stage: ReviewStage::FollowUp,
            session_locator: &locator,
            prompt: StagePrompt {
                prompt_preset_id: "preset-follow-up",
                resolved_text: "Follow up on unresolved high-risk findings.",
                source_surface: "cli",
                explicit_objective: None,
                provider: Some("opencode"),
                model: Some("o4"),
                scope_context_json: None,
                config_layer_digest: Some("cfg-1"),
                launch_intake_id: Some("launch-1"),
            },
            repair_attempt: 1,
            repair_retry_budget: 1,
            actor_id: Some("azure-river"),
        },
    )
    .expect("follow-up execution");

    assert_eq!(
        exploration.validation_outcome.stage_state,
        StageState::Structured
    );
    assert_eq!(
        deep_review.validation_outcome.stage_state,
        StageState::RawOnly
    );
    assert_eq!(
        follow_up.validation_outcome.stage_state,
        StageState::RawOnly
    );
    assert!(follow_up.outcome_metadata.degraded);
    assert_eq!(
        follow_up.outcome_metadata.degraded_reason.as_deref(),
        Some("provider flagged degraded continuation")
    );

    assert_eq!(
        store
            .artifact_bytes(&exploration.raw_output_artifact_id)
            .expect("exploration raw bytes"),
        b"exploration raw transcript"
    );
    assert_eq!(
        store
            .artifact_bytes(&deep_review.raw_output_artifact_id)
            .expect("deep raw bytes"),
        b"deep review raw transcript"
    );
    assert_eq!(
        store
            .artifact_bytes(&follow_up.raw_output_artifact_id)
            .expect("follow-up raw bytes"),
        b"follow-up raw transcript"
    );

    let exploration_invocation = store
        .prompt_invocation(&exploration.prompt_invocation_id)
        .expect("lookup invocation")
        .expect("invocation exists");
    assert_eq!(exploration_invocation.stage, "exploration");
    assert_eq!(
        exploration_invocation.prompt_preset_id,
        "preset-exploration"
    );

    let events = store
        .outcome_events_for_run("session-1", "run-1")
        .expect("list events");
    assert_eq!(events.len(), 3);
    assert!(
        events
            .iter()
            .all(|event| event.event_type == PROMPT_INVOKED_EVENT_TYPE)
    );

    let stage_set: HashSet<String> = events
        .iter()
        .map(|event| {
            let payload: StageOutcomeMetadata =
                serde_json::from_str(&event.payload_json).expect("decode payload");
            payload.stage
        })
        .collect();
    assert_eq!(
        stage_set,
        HashSet::from([
            "exploration".to_owned(),
            "deep_review".to_owned(),
            "follow_up".to_owned()
        ])
    );

    assert_eq!(
        harness.calls(),
        vec![
            ReviewStage::Exploration,
            ReviewStage::DeepReview,
            ReviewStage::FollowUp
        ]
    );
}

#[test]
fn stage_outcome_metadata_retains_repair_state_for_audit_and_refresh() {
    let (_tempdir, store) = setup_store("session-2", "run-2");
    let locator = sample_locator();
    let harness = StubStageHarness::new(HashMap::from([(
        ReviewStage::DeepReview,
        StageHarnessOutput {
            raw_output: "raw deep review transcript".to_owned(),
            structured_pack_json: Some("{bad-json".to_owned()),
            degraded_reason: None,
        },
    )]));

    let result = execute_review_stage(
        &store,
        &harness,
        StageExecutionRequest {
            review_session_id: "session-2",
            review_run_id: "run-2",
            stage: ReviewStage::DeepReview,
            session_locator: &locator,
            prompt: StagePrompt {
                prompt_preset_id: "preset-deep-review",
                resolved_text: "Deep review this patch for regressions.",
                source_surface: "cli",
                explicit_objective: Some("recover structured findings if malformed"),
                provider: Some("opencode"),
                model: Some("o4"),
                scope_context_json: None,
                config_layer_digest: Some("cfg-2"),
                launch_intake_id: Some("launch-2"),
            },
            repair_attempt: 0,
            repair_retry_budget: 1,
            actor_id: Some("azure-river"),
        },
    )
    .expect("execution should succeed with repair-needed state");

    assert_eq!(
        result.validation_outcome.stage_state,
        StageState::RepairNeeded
    );
    assert_eq!(
        result.validation_outcome.repair_action,
        RepairAction::RetryRepair
    );
    assert!(!result.validation_outcome.issues.is_empty());

    let events = store
        .outcome_events_for_run("session-2", "run-2")
        .expect("list outcome events");
    let event = events
        .iter()
        .find(|event| event.id == result.outcome_event_id)
        .expect("result event exists");

    let payload: StageOutcomeMetadata =
        serde_json::from_str(&event.payload_json).expect("decode payload");
    assert_eq!(payload.stage, "deep_review");
    assert_eq!(payload.stage_state, StageState::RepairNeeded);
    assert_eq!(payload.repair_action, RepairAction::RetryRepair);
    assert!(payload.structured_pack_present);
    assert_eq!(payload.finding_count, 0);
    assert!(payload.issue_count > 0);
    assert!(!payload.degraded);

    let invocation = store
        .prompt_invocation(&result.prompt_invocation_id)
        .expect("lookup invocation")
        .expect("invocation exists");
    assert_eq!(invocation.stage, "deep_review");
    assert_eq!(invocation.prompt_preset_id, "preset-deep-review");
    assert_eq!(invocation.source_surface, "cli");

    let _ = sample_intent();
}
