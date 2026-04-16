use roger_app_core::{
    ReviewTarget, ReviewTaskKind, SessionLocator, WORKER_STAGE_RESULT_SCHEMA_V1, WorkerArtifactRef,
    WorkerClarificationRequest, WorkerFollowUpProposal, WorkerInvocation,
    WorkerInvocationOutcomeState, WorkerMemoryCitation, WorkerMemoryReviewRequest,
    WorkerStageOutcome, WorkerStageResult, WorkerToolCallEvent, WorkerToolCallOutcomeState,
    WorkerTransportKind,
};
use roger_storage::{
    ArtifactBudgetClass, CreateReviewRun, CreateReviewSession, CreateWorkerStageResult, Result,
    RogerStore,
};
use serde_json::json;
use tempfile::tempdir;

fn sample_target() -> ReviewTarget {
    ReviewTarget {
        repository: "owner/repo".to_owned(),
        pull_request_number: 42,
        base_ref: "main".to_owned(),
        head_ref: "feature".to_owned(),
        base_commit: "deadbeef".to_owned(),
        head_commit: "feedface".to_owned(),
    }
}

#[test]
fn worker_audit_records_survive_restart_and_remain_queryable() -> Result<()> {
    let temp = tempdir()?;
    let root = temp.path().join("profile");

    {
        let store = RogerStore::open(&root)?;
        let target = sample_target();
        store.create_review_session(CreateReviewSession {
            id: "session-1",
            review_target: &target,
            provider: "opencode",
            session_locator: Some(&SessionLocator {
                provider: "opencode".to_owned(),
                session_id: "oc-123".to_owned(),
                invocation_context_json: "{\"cwd\":\"/tmp/repo\"}".to_owned(),
                captured_at: 100,
                last_tested_at: Some(101),
            }),
            resume_bundle_artifact_id: None,
            continuity_state: "review_launched",
            attention_state: "awaiting_user_input",
            launch_profile_id: Some("profile-open-pr"),
        })?;
        store.create_review_run(CreateReviewRun {
            id: "run-1",
            session_id: "session-1",
            run_kind: "deep_review",
            repo_snapshot: "git:feedface",
            continuity_quality: "degraded",
            session_locator_artifact_id: None,
        })?;

        let raw_artifact = store.store_artifact(
            "artifact-raw-output",
            ArtifactBudgetClass::ColdArtifact,
            "text/plain",
            b"worker raw output",
        )?;
        let result_artifact = store.store_artifact(
            "artifact-stage-result",
            ArtifactBudgetClass::ColdArtifact,
            "application/json",
            br#"{"summary":"worker stage result"}"#,
        )?;
        let pack_artifact = store.store_artifact(
            "artifact-structured-pack",
            ArtifactBudgetClass::ColdArtifact,
            "application/json",
            br#"{"schema_version":"structured_findings_pack.v1","findings":[]}"#,
        )?;

        let invocation = WorkerInvocation {
            id: "worker-1".to_owned(),
            review_session_id: "session-1".to_owned(),
            review_run_id: "run-1".to_owned(),
            review_task_id: "task-1".to_owned(),
            provider: "opencode".to_owned(),
            provider_session_id: Some("oc-123".to_owned()),
            transport_kind: WorkerTransportKind::AgentCli,
            started_at: 1_746_000_000,
            completed_at: Some(1_746_000_010),
            outcome_state: WorkerInvocationOutcomeState::CompletedPartial,
            prompt_invocation_id: None,
            raw_output_artifact_id: Some(raw_artifact.id.clone()),
            result_artifact_id: Some(result_artifact.id.clone()),
        };
        store.record_worker_invocation(&invocation)?;

        let tool_event = WorkerToolCallEvent {
            id: "tool-call-1".to_owned(),
            review_task_id: "task-1".to_owned(),
            worker_invocation_id: "worker-1".to_owned(),
            operation: "worker.get_review_context".to_owned(),
            request_digest: "sha256:req".to_owned(),
            response_digest: Some("sha256:resp".to_owned()),
            outcome_state: WorkerToolCallOutcomeState::Succeeded,
            occurred_at: 1_746_000_005,
        };
        store.record_worker_tool_call_event(&tool_event)?;

        let result = WorkerStageResult {
            schema_id: WORKER_STAGE_RESULT_SCHEMA_V1.to_owned(),
            review_session_id: "session-1".to_owned(),
            review_run_id: "run-1".to_owned(),
            review_task_id: "task-1".to_owned(),
            worker_invocation_id: Some("worker-1".to_owned()),
            task_nonce: "nonce-1".to_owned(),
            stage: "deep_review".to_owned(),
            task_kind: ReviewTaskKind::DeepReviewPass,
            outcome: WorkerStageOutcome::CompletedPartial,
            summary: "Need a follow-up pass for one remaining risky area.".to_owned(),
            structured_findings_pack: Some(json!({
                "schema_version": "structured_findings_pack.v1",
                "findings": []
            })),
            clarification_requests: vec![WorkerClarificationRequest {
                id: "clarify-1".to_owned(),
                question: "Need the generated config artifact.".to_owned(),
                reason: Some("evidence missing".to_owned()),
                blocking: true,
            }],
            memory_review_requests: vec![WorkerMemoryReviewRequest {
                id: "memory-1".to_owned(),
                query: "outbound invalidation".to_owned(),
                requested_scopes: vec!["repo".to_owned()],
                rationale: Some("compare against prior regressions".to_owned()),
            }],
            follow_up_proposals: vec![WorkerFollowUpProposal {
                id: "follow-up-1".to_owned(),
                title: "Retry config-focused pass".to_owned(),
                objective: "Inspect generated config drift.".to_owned(),
                proposed_task_kind: ReviewTaskKind::FollowUpPass,
                suggested_scopes: vec!["repo".to_owned(), "generated".to_owned()],
            }],
            memory_citations: vec![WorkerMemoryCitation {
                citation_id: "citation-1".to_owned(),
                source_kind: "memory_item".to_owned(),
                source_id: "memory-hit-1".to_owned(),
                summary: "Prior config drift regression".to_owned(),
                scope: "repo".to_owned(),
                trust_tier: Some("proven".to_owned()),
            }],
            artifact_refs: vec![WorkerArtifactRef {
                artifact_id: "artifact-generated-config".to_owned(),
                role: "generated_config".to_owned(),
                media_type: Some("application/json".to_owned()),
                summary: Some("config snapshot".to_owned()),
            }],
            provider_metadata: Some(json!({
                "model": "o4",
                "attempt": 1
            })),
            warnings: vec!["partial findings pack".to_owned()],
        };
        store.record_worker_stage_result(CreateWorkerStageResult {
            result: &result,
            submitted_result_artifact_id: Some(&result_artifact.id),
            structured_findings_pack_artifact_id: Some(&pack_artifact.id),
        })?;
    }

    {
        let reopened = RogerStore::open(&root)?;

        let invocations = reopened.worker_invocations_for_run("session-1", "run-1")?;
        assert_eq!(invocations.len(), 1);
        assert_eq!(invocations[0].id, "worker-1");
        assert_eq!(
            invocations[0].outcome_state,
            WorkerInvocationOutcomeState::CompletedPartial
        );
        assert_eq!(invocations[0].transport_kind, WorkerTransportKind::AgentCli);
        assert_eq!(
            reopened.artifact_bytes("artifact-raw-output")?,
            b"worker raw output"
        );

        let tool_events = reopened.worker_tool_call_events_for_invocation("worker-1")?;
        assert_eq!(tool_events.len(), 1);
        assert_eq!(tool_events[0].operation, "worker.get_review_context");
        assert_eq!(
            tool_events[0].outcome_state,
            WorkerToolCallOutcomeState::Succeeded
        );

        let stage_results = reopened.worker_stage_results_for_run("session-1", "run-1")?;
        assert_eq!(stage_results.len(), 1);
        let stage_result = &stage_results[0];
        assert_eq!(stage_result.review_task_id, "task-1");
        assert_eq!(
            stage_result.worker_invocation_id.as_deref(),
            Some("worker-1")
        );
        assert_eq!(stage_result.task_kind, ReviewTaskKind::DeepReviewPass);
        assert_eq!(stage_result.outcome, WorkerStageOutcome::CompletedPartial);
        assert_eq!(
            stage_result.submitted_result_artifact_id.as_deref(),
            Some("artifact-stage-result")
        );
        assert_eq!(
            stage_result.structured_findings_pack_artifact_id.as_deref(),
            Some("artifact-structured-pack")
        );
        assert!(
            stage_result
                .clarification_requests_json
                .as_deref()
                .expect("clarification json")
                .contains("generated config artifact")
        );
        assert!(
            stage_result
                .provider_metadata_json
                .as_deref()
                .expect("provider metadata json")
                .contains("\"model\":\"o4\"")
        );
        assert_eq!(
            reopened.artifact_bytes("artifact-structured-pack")?,
            br#"{"schema_version":"structured_findings_pack.v1","findings":[]}"#
        );
    }

    Ok(())
}
