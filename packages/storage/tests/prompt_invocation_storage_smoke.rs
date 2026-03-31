use tempfile::tempdir;

use roger_app_core::{ReviewTarget, SessionLocator};
use roger_storage::{
    ArtifactBudgetClass, CreateOutcomeEvent, CreatePromptInvocation, CreateReviewRun,
    CreateReviewSession, Result, RogerStore,
};

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
fn prompt_invocation_snapshots_and_outcome_events_survive_restart() -> Result<()> {
    let temp = tempdir()?;
    let root = temp.path().join("profile");
    let prompt_text = "Review this PR for invalidation regressions and stale draft state.";

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

        let prompt_artifact = store.store_artifact(
            "artifact-prompt-text",
            ArtifactBudgetClass::EvidenceExcerpt,
            "text/plain",
            prompt_text.as_bytes(),
        )?;

        let invocation = store.record_prompt_invocation(CreatePromptInvocation {
            id: "invocation-1",
            review_session_id: "session-1",
            review_run_id: "run-1",
            stage: "deep_review",
            prompt_preset_id: "security-deep-review",
            source_surface: "cli",
            resolved_text_digest: &prompt_artifact.digest,
            resolved_text_artifact_id: Some("artifact-prompt-text"),
            resolved_text_inline_preview: Some("Review this PR for invalidation regressions"),
            explicit_objective: Some("focus on refresh invalidation and stale drafts"),
            provider: Some("opencode"),
            model: Some("gpt-5.4"),
            scope_context_json: Some("{\"scope\":\"repo\",\"repository\":\"owner/repo\"}"),
            config_layer_digest: Some("sha256:cfg-1"),
            launch_intake_id: Some("intake-1"),
            used_at: 1_746_000_000,
        })?;

        assert_eq!(invocation.prompt_preset_id, "security-deep-review");
        assert_eq!(
            invocation.resolved_text_artifact_id.as_deref(),
            Some("artifact-prompt-text")
        );

        store.record_outcome_event(CreateOutcomeEvent {
            id: "event-1",
            event_type: "finding_emitted",
            review_session_id: "session-1",
            review_run_id: Some("run-1"),
            prompt_invocation_id: Some("invocation-1"),
            actor_kind: "agent",
            actor_id: Some("azure-river"),
            source_surface: "cli",
            payload_json: "{\"finding_id\":\"finding-1\",\"finding_fingerprint\":\"fp-1\",\"severity\":\"high\",\"confidence\":\"medium\",\"stage\":\"deep_review\"}",
            occurred_at: 1_746_000_010,
        })?;

        store.record_outcome_event(CreateOutcomeEvent {
            id: "event-2",
            event_type: "finding_state_changed",
            review_session_id: "session-1",
            review_run_id: Some("run-1"),
            prompt_invocation_id: Some("invocation-1"),
            actor_kind: "human",
            actor_id: Some("reviewer"),
            source_surface: "tui",
            payload_json: "{\"finding_id\":\"finding-1\",\"from_triage_state\":\"new\",\"to_triage_state\":\"accepted\",\"from_outbound_state\":\"not_drafted\",\"to_outbound_state\":\"drafted\"}",
            occurred_at: 1_746_000_020,
        })?;
    }

    {
        let reopened = RogerStore::open(&root)?;

        let invocation = reopened
            .prompt_invocation("invocation-1")?
            .expect("prompt invocation");
        assert_eq!(invocation.review_session_id, "session-1");
        assert_eq!(invocation.review_run_id, "run-1");
        assert_eq!(
            invocation.explicit_objective.as_deref(),
            Some("focus on refresh invalidation and stale drafts")
        );
        assert_eq!(
            invocation.scope_context_json.as_deref(),
            Some("{\"scope\":\"repo\",\"repository\":\"owner/repo\"}")
        );

        assert_eq!(
            reopened.artifact_bytes("artifact-prompt-text")?,
            prompt_text.as_bytes()
        );

        let events = reopened.outcome_events_for_run("session-1", "run-1")?;
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_type, "finding_emitted");
        assert_eq!(events[1].event_type, "finding_state_changed");
        assert_eq!(
            events[0].prompt_invocation_id.as_deref(),
            Some("invocation-1")
        );
        assert_eq!(events[1].source_surface, "tui");
    }

    Ok(())
}
