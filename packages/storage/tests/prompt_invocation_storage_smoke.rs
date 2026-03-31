use tempfile::tempdir;

use roger_app_core::{ReviewTarget, SessionLocator};
use roger_storage::{
    ArtifactBudgetClass, CreateMergedResolutionLink, CreateOutcomeEvent, CreatePromptInvocation,
    CreateReviewRun, CreateReviewSession, CreateUsageEventDerivationJob, Result, RogerStore,
    UpdateUsageEventDerivationJobStatus,
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

        store.record_merged_resolution_link(CreateMergedResolutionLink {
            id: "merged-link-1",
            prompt_invocation_id: "invocation-1",
            review_session_id: "session-1",
            review_run_id: Some("run-1"),
            source_outcome_event_id: Some("event-2"),
            resolution_kind: "merged",
            source_kind: "posted_action",
            source_id: "posted-action-42",
            remote_identifier: Some("https://github.com/owner/repo/pull/42/files#r1"),
            resolved_at: 1_746_000_030,
        })?;

        let job = store.create_usage_event_derivation_job(CreateUsageEventDerivationJob {
            id: "usage-job-1",
            prompt_invocation_id: "invocation-1",
            review_session_id: "session-1",
            review_run_id: Some("run-1"),
            seed_outcome_event_id: Some("event-1"),
            job_kind: "prompt_usefulness_derivation",
            status: "queued",
            payload_json: "{\"usage_event_types\":[\"surfaced\",\"approved\",\"merged\"]}",
            started_at: None,
            completed_at: None,
            failure_reason: None,
        })?;
        assert_eq!(job.status, "queued");
        assert_eq!(job.row_version, 0);

        let updated = store.update_usage_event_derivation_job_status(
            UpdateUsageEventDerivationJobStatus {
                id: "usage-job-1",
                expected_row_version: 0,
                status: "completed",
                started_at: Some(1_746_000_040),
                completed_at: Some(1_746_000_050),
                failure_reason: None,
            },
        )?;
        assert_eq!(updated.status, "completed");
        assert_eq!(updated.row_version, 1);
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

        let merged_links =
            reopened.merged_resolution_links_for_prompt_invocation("invocation-1")?;
        assert_eq!(merged_links.len(), 1);
        assert_eq!(merged_links[0].resolution_kind, "merged");
        assert_eq!(merged_links[0].source_kind, "posted_action");
        assert_eq!(
            merged_links[0].source_outcome_event_id.as_deref(),
            Some("event-2")
        );

        let jobs = reopened.usage_event_derivation_jobs_for_prompt_invocation("invocation-1")?;
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].job_kind, "prompt_usefulness_derivation");
        assert_eq!(jobs[0].status, "completed");
        assert_eq!(jobs[0].row_version, 1);
        assert_eq!(jobs[0].seed_outcome_event_id.as_deref(), Some("event-1"));
    }

    Ok(())
}

#[test]
fn prompt_usefulness_links_fail_closed_on_cross_session_or_unbound_event_links() -> Result<()> {
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path())?;
    let target = sample_target();

    store.create_review_session(CreateReviewSession {
        id: "session-1",
        review_target: &target,
        provider: "opencode",
        session_locator: None,
        resume_bundle_artifact_id: None,
        continuity_state: "review_launched",
        attention_state: "awaiting_user_input",
        launch_profile_id: None,
    })?;
    store.create_review_run(CreateReviewRun {
        id: "run-1",
        session_id: "session-1",
        run_kind: "deep_review",
        repo_snapshot: "git:111",
        continuity_quality: "degraded",
        session_locator_artifact_id: None,
    })?;
    store.record_prompt_invocation(CreatePromptInvocation {
        id: "invocation-1",
        review_session_id: "session-1",
        review_run_id: "run-1",
        stage: "deep_review",
        prompt_preset_id: "security-deep-review",
        source_surface: "cli",
        resolved_text_digest: "sha256:prompt",
        resolved_text_artifact_id: None,
        resolved_text_inline_preview: None,
        explicit_objective: None,
        provider: Some("opencode"),
        model: Some("gpt-5.4"),
        scope_context_json: None,
        config_layer_digest: None,
        launch_intake_id: None,
        used_at: 1_746_000_000,
    })?;
    store.record_outcome_event(CreateOutcomeEvent {
        id: "event-1",
        event_type: "finding_emitted",
        review_session_id: "session-1",
        review_run_id: Some("run-1"),
        prompt_invocation_id: Some("invocation-1"),
        actor_kind: "agent",
        actor_id: Some("violet-sky"),
        source_surface: "cli",
        payload_json: "{\"finding_id\":\"f-1\"}",
        occurred_at: 1_746_000_010,
    })?;
    store.record_outcome_event(CreateOutcomeEvent {
        id: "event-unbound",
        event_type: "review_note",
        review_session_id: "session-1",
        review_run_id: Some("run-1"),
        prompt_invocation_id: None,
        actor_kind: "human",
        actor_id: Some("reviewer"),
        source_surface: "tui",
        payload_json: "{\"note\":\"manual follow-up\"}",
        occurred_at: 1_746_000_020,
    })?;

    let conflict = store
        .record_merged_resolution_link(CreateMergedResolutionLink {
            id: "merged-link-conflict",
            prompt_invocation_id: "invocation-1",
            review_session_id: "session-2",
            review_run_id: Some("run-1"),
            source_outcome_event_id: Some("event-1"),
            resolution_kind: "merged",
            source_kind: "posted_action",
            source_id: "posted-action-x",
            remote_identifier: None,
            resolved_at: 1_746_000_030,
        })
        .expect_err("cross-session link should fail closed");
    assert!(conflict.to_string().contains("prompt_invocation"));

    let event_conflict = store
        .create_usage_event_derivation_job(CreateUsageEventDerivationJob {
            id: "usage-job-conflict",
            prompt_invocation_id: "invocation-1",
            review_session_id: "session-1",
            review_run_id: Some("run-1"),
            seed_outcome_event_id: Some("event-unbound"),
            job_kind: "prompt_usefulness_derivation",
            status: "queued",
            payload_json: "{\"usage_event_types\":[\"surfaced\"]}",
            started_at: None,
            completed_at: None,
            failure_reason: None,
        })
        .expect_err("unbound outcome-event link should fail closed");
    assert!(event_conflict.to_string().contains("outcome_event"));

    Ok(())
}
