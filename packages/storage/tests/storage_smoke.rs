use std::fs;
use std::path::PathBuf;

use tempfile::tempdir;

use roger_app_core::{
    ContinuityQuality, LaunchAction, LaunchIntent, ProviderContinuityCapability,
    ResumeAttemptOutcome, ResumeBundle, ResumeBundleProfile, ResumeSessionState, ResumeStrategy,
    ReviewTarget, SessionLocator, Surface, decide_resume_strategy,
};
use roger_storage::{
    ArtifactBudgetClass, ArtifactStorageKind, CreateFinding, CreateLaunchAttempt,
    CreateLaunchProfile, CreateOutboundDraft, CreateReviewRun, CreateReviewSession,
    CreateSessionLaunchBinding, ExistingSessionLaunchFinalizationError,
    FinalizeExistingSessionLaunchAttempt, FinalizeReviewLaunchAttempt, LaunchAttemptAction,
    LaunchAttemptState, LaunchSurface, ResolveSessionLaunchBinding, ResolveSessionLocalRoot,
    Result, RogerStore, SessionBindingResolution, UpdateIndexState, UpdateLaunchAttempt,
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

fn sample_resume_bundle() -> ResumeBundle {
    ResumeBundle {
        schema_version: 1,
        profile: ResumeBundleProfile::ReseedResume,
        review_target: sample_target(),
        launch_intent: LaunchIntent {
            action: LaunchAction::ResumeReview,
            source_surface: Surface::Cli,
            objective: Some("resume review".to_owned()),
            launch_profile_id: Some("profile-open-pr".to_owned()),
            cwd: Some("/tmp/repo".to_owned()),
            worktree_root: None,
        },
        provider: "opencode".to_owned(),
        continuity_quality: ContinuityQuality::Degraded,
        stage_summary: "follow-up pending".to_owned(),
        unresolved_finding_ids: vec!["finding-1".to_owned()],
        outbound_draft_ids: vec!["draft-1".to_owned()],
        attention_summary: "awaiting_user_input".to_owned(),
        artifact_refs: vec!["artifact-inline".to_owned()],
    }
}

fn expected_schema_version() -> Result<i64> {
    let migrations_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("migrations");
    let count = fs::read_dir(migrations_dir)?
        .map(|entry| entry.map(|item| item.path()))
        .collect::<std::io::Result<Vec<_>>>()?
        .into_iter()
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("sql"))
        .count();
    Ok(count as i64)
}

#[test]
fn storage_smoke_persists_resume_and_approval_state_across_restart() -> Result<()> {
    let temp = tempdir()?;
    let root = temp.path().join("profile");

    {
        let store = RogerStore::open(&root)?;
        assert_eq!(store.schema_version()?, expected_schema_version()?);

        store.put_launch_profile(CreateLaunchProfile {
            id: "profile-open-pr",
            name: "Open PR",
            source_surface: LaunchSurface::Cli,
            ui_target: "cli",
            terminal_environment: "vscode_integrated_terminal",
            multiplexer_mode: "ntm",
            reuse_policy: "reuse_if_possible",
            repo_root: "/tmp/repo",
            worktree_strategy: "shared-if-clean",
        })?;

        store.store_resume_bundle("resume-bundle-1", &sample_resume_bundle())?;

        let session = store.create_review_session(CreateReviewSession {
            id: "session-1",
            review_target: &sample_target(),
            provider: "opencode",
            session_locator: Some(&SessionLocator {
                provider: "opencode".to_owned(),
                session_id: "abc".to_owned(),
                invocation_context_json: "{\"cwd\":\"/tmp/repo\"}".to_owned(),
                captured_at: 111,
                last_tested_at: Some(112),
            }),
            resume_bundle_artifact_id: Some("resume-bundle-1"),
            continuity_state: "awaiting_resume",
            attention_state: "awaiting_user_input",
            launch_profile_id: Some("profile-open-pr"),
        })?;

        store.create_review_run(CreateReviewRun {
            id: "run-1",
            session_id: &session.id,
            run_kind: "explore",
            repo_snapshot: "git:deadbeef",
            continuity_quality: "degraded",
            session_locator_artifact_id: None,
        })?;

        store.put_session_launch_binding(CreateSessionLaunchBinding {
            id: "binding-cli",
            session_id: &session.id,
            repo_locator: "owner/repo",
            review_target: Some(&sample_target()),
            surface: LaunchSurface::Cli,
            launch_profile_id: Some("profile-open-pr"),
            ui_target: Some("cli"),
            instance_preference: Some("reuse_if_possible"),
            cwd: Some("/tmp/repo"),
            worktree_root: None,
        })?;
        store.put_session_launch_binding(CreateSessionLaunchBinding {
            id: "binding-tui",
            session_id: &session.id,
            repo_locator: "owner/repo",
            review_target: Some(&sample_target()),
            surface: LaunchSurface::Tui,
            launch_profile_id: Some("profile-open-pr"),
            ui_target: Some("tui"),
            instance_preference: Some("always_new"),
            cwd: Some("/tmp/repo"),
            worktree_root: None,
        })?;
        store.put_session_launch_binding(CreateSessionLaunchBinding {
            id: "binding-bridge",
            session_id: &session.id,
            repo_locator: "owner/repo",
            review_target: Some(&sample_target()),
            surface: LaunchSurface::Bridge,
            launch_profile_id: Some("profile-open-pr"),
            ui_target: Some("tui"),
            instance_preference: Some("reuse_if_possible"),
            cwd: Some("/tmp/repo"),
            worktree_root: Some("/tmp/repo/.worktrees/pr-42"),
        })?;

        store.create_finding(CreateFinding {
            id: "finding-1",
            session_id: &session.id,
            first_run_id: "run-1",
            fingerprint: "fp:deadbeef",
            title: "Potential approval invalidation bug",
            triage_state: "new",
            outbound_state: "drafted",
        })?;

        store.create_outbound_draft(CreateOutboundDraft {
            id: "draft-1",
            session_id: &session.id,
            finding_id: "finding-1",
            target_locator: "github:owner/repo#42/files#thread-1",
            payload_digest: "sha256:payload-1",
            body: "Please double-check the invalidation path.",
        })?;

        store.approve_outbound_draft(
            "approval-1",
            "draft-1",
            "sha256:payload-1",
            "github:owner/repo#42/files#thread-1",
        )?;

        store.record_posted_action(
            "posted-1",
            "draft-1",
            "github-review-comment-1001",
            "sha256:payload-1",
            "posted",
        )?;

        store.upsert_index_state(UpdateIndexState {
            scope_key: "repo:owner/repo",
            generation: 2,
            status: "ready",
            artifact_digest: Some("sha256:index-generation"),
        })?;

        let inline_artifact = store.store_artifact(
            "artifact-inline",
            ArtifactBudgetClass::InlineSummary,
            "text/plain",
            b"resume bundle excerpt",
        )?;
        assert_eq!(inline_artifact.storage_kind, ArtifactStorageKind::Inline);

        let cold_artifact = store.store_artifact(
            "artifact-cold",
            ArtifactBudgetClass::ColdArtifact,
            "application/json",
            &vec![b'x'; 32 * 1024],
        )?;
        assert_eq!(
            cold_artifact.storage_kind,
            ArtifactStorageKind::ExternalContentAddressed
        );

        let sidecar_artifact = store.store_artifact(
            "artifact-sidecar",
            ArtifactBudgetClass::DerivedIndexState,
            "application/octet-stream",
            b"tantivy generation bytes",
        )?;
        assert_eq!(
            sidecar_artifact.storage_kind,
            ArtifactStorageKind::DerivedSidecar
        );
    }

    {
        let reopened = RogerStore::open(&root)?;
        let session = reopened.review_session("session-1")?.expect("session");
        assert_eq!(session.provider, "opencode");
        assert_eq!(
            session.launch_profile_id.as_deref(),
            Some("profile-open-pr")
        );
        assert_eq!(session.continuity_state, "awaiting_resume");
        assert_eq!(
            session
                .session_locator
                .as_ref()
                .expect("session locator")
                .session_id,
            "abc"
        );
        assert_eq!(
            session.resume_bundle_artifact_id.as_deref(),
            Some("resume-bundle-1")
        );

        let overview = reopened.session_overview("session-1")?;
        assert_eq!(overview.run_count, 1);
        assert_eq!(overview.finding_count, 1);
        assert_eq!(overview.draft_count, 1);
        assert_eq!(overview.approval_count, 1);
        assert_eq!(overview.posted_action_count, 1);
        assert_eq!(overview.attention_state, "awaiting_user_input");

        let approval = reopened
            .approval_for_draft("draft-1")?
            .expect("approval record");
        assert_eq!(approval.payload_digest, "sha256:payload-1");

        let index_state = reopened
            .index_state("repo:owner/repo")?
            .expect("index state record");
        assert_eq!(index_state.generation, 2);
        assert_eq!(index_state.status, "ready");

        let latest_run = reopened
            .latest_review_run("session-1")?
            .expect("latest run record");
        assert_eq!(latest_run.continuity_quality, "degraded");

        let bindings = reopened.launch_bindings_for_session("session-1")?;
        assert_eq!(bindings.len(), 3);
        assert_eq!(bindings[0].surface, "cli");
        assert_eq!(bindings[0].repo_locator, "owner/repo");
        assert_eq!(bindings[0].ui_target.as_deref(), Some("cli"));
        assert_eq!(
            bindings[0].instance_preference.as_deref(),
            Some("reuse_if_possible")
        );
        assert_eq!(bindings[1].surface, "tui");
        assert_eq!(bindings[1].ui_target.as_deref(), Some("tui"));
        assert_eq!(bindings[2].surface, "bridge");
        assert_eq!(
            bindings[2]
                .review_target
                .as_ref()
                .expect("binding review target"),
            &sample_target()
        );

        let resolved = reopened.resolve_session_launch_binding(ResolveSessionLaunchBinding {
            explicit_session_id: None,
            surface: LaunchSurface::Tui,
            repo_locator: "owner/repo",
            review_target: Some(&sample_target()),
            ui_target: Some("tui"),
            instance_preference: Some("always_new"),
        })?;
        assert!(
            matches!(
                &resolved,
                SessionBindingResolution::Resolved(binding)
                    if binding.id == "binding-tui" && binding.session_id == "session-1"
            ),
            "expected resolved binding, got {resolved:?}"
        );

        assert_eq!(
            reopened.artifact_bytes("artifact-inline")?,
            b"resume bundle excerpt"
        );
        assert_eq!(
            reopened.artifact_bytes("artifact-cold")?,
            vec![b'x'; 32 * 1024]
        );
        assert_eq!(
            reopened.artifact_bytes("artifact-sidecar")?,
            b"tantivy generation bytes"
        );

        let loaded_bundle = reopened.load_resume_bundle("resume-bundle-1")?;
        assert_eq!(loaded_bundle.review_target, sample_target());
        let reseed = decide_resume_strategy(
            ProviderContinuityCapability::ReopenByLocator,
            &ResumeSessionState {
                locator_present: true,
                resume_bundle_present: true,
            },
            ResumeAttemptOutcome::ReopenUnavailable,
        );
        assert_eq!(reseed.strategy, ResumeStrategy::ReseedFromBundle);

        let fail_closed = decide_resume_strategy(
            ProviderContinuityCapability::ReopenByLocator,
            &ResumeSessionState {
                locator_present: true,
                resume_bundle_present: false,
            },
            ResumeAttemptOutcome::MissingHarnessState,
        );
        assert_eq!(fail_closed.strategy, ResumeStrategy::FailClosed);

        let by_target = reopened.find_sessions_by_target("owner/repo", 42)?;
        assert_eq!(by_target.len(), 1);
        assert_eq!(by_target[0].id, "session-1");

        let by_target_none = reopened.find_sessions_by_target("owner/repo", 43)?;
        assert!(by_target_none.is_empty());
    }


    Ok(())
}

#[test]
fn review_launch_attempt_finalize_is_atomic_and_records_verified_binding() -> Result<()> {
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path())?;
    let target = sample_target();
    let locator = SessionLocator {
        provider: "opencode".to_owned(),
        session_id: "oc-verified-1".to_owned(),
        invocation_context_json: "{\"cwd\":\"/tmp/repo\"}".to_owned(),
        captured_at: 1,
        last_tested_at: Some(1),
    };

    store.create_launch_attempt(CreateLaunchAttempt {
        id: "attempt-1",
        action: LaunchAttemptAction::StartReview,
        provider: "opencode",
        source_surface: LaunchSurface::Cli,
        review_target: &target,
        requested_session_id: None,
        state: LaunchAttemptState::Pending,
    })?;
    store.update_launch_attempt(UpdateLaunchAttempt {
        id: "attempt-1",
        state: LaunchAttemptState::Dispatching,
        final_session_id: None,
        launch_binding_id: None,
        provider_session_id: None,
        verified_locator: None,
        failure_reason: None,
    })?;

    let finalized = store.finalize_review_launch_attempt(FinalizeReviewLaunchAttempt {
        attempt_id: "attempt-1",
        terminal_state: LaunchAttemptState::VerifiedStarted,
        provider_session_id: "oc-verified-1",
        verified_locator: &locator,
        review_session: CreateReviewSession {
            id: "session-1",
            review_target: &target,
            provider: "opencode",
            session_locator: Some(&locator),
            resume_bundle_artifact_id: None,
            continuity_state: "usable",
            attention_state: "review_launched",
            launch_profile_id: Some("profile-open-pr"),
        },
        review_run: CreateReviewRun {
            id: "run-1",
            session_id: "session-1",
            run_kind: "review",
            repo_snapshot: "owner/repo#42",
            continuity_quality: "usable",
            session_locator_artifact_id: None,
        },
        launch_binding: CreateSessionLaunchBinding {
            id: "binding-1",
            session_id: "session-1",
            repo_locator: "owner/repo",
            review_target: Some(&target),
            surface: LaunchSurface::Cli,
            launch_profile_id: Some("profile-open-pr"),
            ui_target: Some("cli"),
            instance_preference: Some("reuse_if_possible"),
            cwd: Some("/tmp/repo"),
            worktree_root: None,
        },
    })?;

    assert_eq!(finalized.state, LaunchAttemptState::VerifiedStarted);
    assert_eq!(finalized.final_session_id.as_deref(), Some("session-1"));
    assert_eq!(finalized.launch_binding_id.as_deref(), Some("binding-1"));
    assert_eq!(
        finalized.provider_session_id.as_deref(),
        Some("oc-verified-1")
    );
    assert_eq!(
        finalized
            .verified_locator
            .as_ref()
            .expect("verified locator")
            .session_id,
        "oc-verified-1"
    );
    assert!(finalized.finalized_at.is_some());

    let session = store.review_session("session-1")?.expect("review session");
    assert_eq!(session.provider, "opencode");
    let run = store.latest_review_run("session-1")?.expect("review run");
    assert_eq!(run.id, "run-1");
    let bindings = store.launch_bindings_for_session("session-1")?;
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].id, "binding-1");

    Ok(())
}

#[test]
fn failed_review_launch_attempt_finalize_rolls_back_partial_session_writes() -> Result<()> {
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path())?;
    let target = sample_target();
    let locator = SessionLocator {
        provider: "opencode".to_owned(),
        session_id: "oc-rollback-1".to_owned(),
        invocation_context_json: "{\"cwd\":\"/tmp/repo\"}".to_owned(),
        captured_at: 1,
        last_tested_at: Some(1),
    };

    store.create_launch_attempt(CreateLaunchAttempt {
        id: "attempt-rollback",
        action: LaunchAttemptAction::StartReview,
        provider: "opencode",
        source_surface: LaunchSurface::Cli,
        review_target: &target,
        requested_session_id: None,
        state: LaunchAttemptState::Dispatching,
    })?;

    let err = store
        .finalize_review_launch_attempt(FinalizeReviewLaunchAttempt {
            attempt_id: "attempt-rollback",
            terminal_state: LaunchAttemptState::VerifiedStarted,
            provider_session_id: "oc-rollback-1",
            verified_locator: &locator,
            review_session: CreateReviewSession {
                id: "session-rollback",
                review_target: &target,
                provider: "opencode",
                session_locator: Some(&locator),
                resume_bundle_artifact_id: None,
                continuity_state: "usable",
                attention_state: "review_launched",
                launch_profile_id: None,
            },
            review_run: CreateReviewRun {
                id: "run-rollback",
                session_id: "missing-session",
                run_kind: "review",
                repo_snapshot: "owner/repo#42",
                continuity_quality: "usable",
                session_locator_artifact_id: None,
            },
            launch_binding: CreateSessionLaunchBinding {
                id: "binding-rollback",
                session_id: "session-rollback",
                repo_locator: "owner/repo",
                review_target: Some(&target),
                surface: LaunchSurface::Cli,
                launch_profile_id: None,
                ui_target: Some("cli"),
                instance_preference: Some("reuse_if_possible"),
                cwd: Some("/tmp/repo"),
                worktree_root: None,
            },
        })
        .expect_err("finalize should fail closed when the run points at a missing session");
    assert!(err.to_string().contains("FOREIGN KEY"));

    store.update_launch_attempt(UpdateLaunchAttempt {
        id: "attempt-rollback",
        state: LaunchAttemptState::FailedCommit,
        final_session_id: None,
        launch_binding_id: None,
        provider_session_id: Some("oc-rollback-1"),
        verified_locator: Some(&locator),
        failure_reason: Some("failed to finalize review launch"),
    })?;

    assert!(store.review_session("session-rollback")?.is_none());
    assert!(store.latest_review_run("session-rollback")?.is_none());
    let attempt = store
        .launch_attempt("attempt-rollback")?
        .expect("attempt record remains durable");
    assert_eq!(attempt.state, LaunchAttemptState::FailedCommit);
    assert!(attempt.final_session_id.is_none());
    assert!(attempt.finalized_at.is_some());
    assert_eq!(
        attempt.provider_session_id.as_deref(),
        Some("oc-rollback-1")
    );
    assert!(attempt.verified_locator.is_some());
    assert_eq!(
        attempt.failure_reason.as_deref(),
        Some("failed to finalize review launch")
    );

    Ok(())
}

#[test]
fn launch_binding_resolution_fails_closed_for_ambiguous_and_mismatched_state() -> Result<()> {
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path())?;

    let target = sample_target();
    let other_target = ReviewTarget {
        repository: "owner/repo".to_owned(),
        pull_request_number: 99,
        base_ref: "main".to_owned(),
        head_ref: "other".to_owned(),
        base_commit: "111".to_owned(),
        head_commit: "222".to_owned(),
    };

    for (session_id, binding_id, review_target, instance_preference) in [
        ("session-1", "binding-1", &target, "reuse_if_possible"),
        ("session-2", "binding-2", &other_target, "always_new"),
    ] {
        let (cwd, worktree_root) = match binding_id {
            "binding-1" => (Some("/tmp/worktree-a"), Some("/tmp/worktree-a")),
            "binding-2" => (Some("/tmp/worktree-a/subdir"), Some("/tmp/worktree-a")),
            _ => unreachable!("unexpected binding id"),
        };
        store.create_review_session(CreateReviewSession {
            id: session_id,
            review_target,
            provider: "opencode",
            session_locator: None,
            resume_bundle_artifact_id: None,
            continuity_state: "review_launched",
            attention_state: "review_launched",
            launch_profile_id: None,
        })?;
        store.put_session_launch_binding(CreateSessionLaunchBinding {
            id: binding_id,
            session_id,
            repo_locator: "owner/repo",
            review_target: Some(review_target),
            surface: LaunchSurface::Cli,
            launch_profile_id: None,
            ui_target: Some("cli"),
            instance_preference: Some(instance_preference),
            cwd,
            worktree_root,
        })?;
    }

    let ambiguous = store.resolve_session_launch_binding(ResolveSessionLaunchBinding {
        explicit_session_id: None,
        surface: LaunchSurface::Cli,
        repo_locator: "owner/repo",
        review_target: None,
        ui_target: Some("cli"),
        instance_preference: None,
    })?;
    assert!(
        matches!(
            &ambiguous,
            SessionBindingResolution::Ambiguous { session_ids }
                if session_ids == &vec!["session-1".to_owned(), "session-2".to_owned()]
        ),
        "expected ambiguous resolution, got {ambiguous:?}"
    );

    let stale = store.resolve_session_launch_binding(ResolveSessionLaunchBinding {
        explicit_session_id: None,
        surface: LaunchSurface::Cli,
        repo_locator: "owner/repo",
        review_target: Some(&ReviewTarget {
            repository: "owner/repo".to_owned(),
            pull_request_number: 7,
            base_ref: "main".to_owned(),
            head_ref: "feature".to_owned(),
            base_commit: "aaa".to_owned(),
            head_commit: "bbb".to_owned(),
        }),
        ui_target: Some("cli"),
        instance_preference: Some("reuse_if_possible"),
    })?;
    assert!(
        matches!(
            &stale,
            SessionBindingResolution::Stale { binding_id, reason }
                if binding_id == "binding-1" && reason.contains("binding target mismatch")
        ),
        "expected stale resolution, got {stale:?}"
    );

    let stale_local_root = store.resolve_session_launch_binding_with_context(
        ResolveSessionLaunchBinding {
            explicit_session_id: None,
            surface: LaunchSurface::Cli,
            repo_locator: "owner/repo",
            review_target: Some(&other_target),
            ui_target: Some("cli"),
            instance_preference: Some("always_new"),
        },
        ResolveSessionLocalRoot {
            cwd: Some("/tmp/worktree-b"),
            worktree_root: Some("/tmp/worktree-b"),
        },
    )?;
    assert!(
        matches!(
            &stale_local_root,
            SessionBindingResolution::Stale { binding_id, reason }
                if binding_id == "binding-2" && reason.contains("worktree root mismatch")
        ),
        "expected stale local-root resolution, got {stale_local_root:?}"
    );

    Ok(())
}

#[test]
fn same_session_writes_fail_closed_on_row_version_mismatch() -> Result<()> {
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path())?;

    let created = store.create_review_session(CreateReviewSession {
        id: "session-1",
        review_target: &ReviewTarget {
            repository: "owner/repo".to_owned(),
            pull_request_number: 7,
            base_ref: "main".to_owned(),
            head_ref: "feature".to_owned(),
            base_commit: "aaa".to_owned(),
            head_commit: "bbb".to_owned(),
        },
        provider: "opencode",
        session_locator: Some(&SessionLocator {
            provider: "opencode".to_owned(),
            session_id: "123".to_owned(),
            invocation_context_json: "{}".to_owned(),
            captured_at: 1,
            last_tested_at: None,
        }),
        resume_bundle_artifact_id: None,
        continuity_state: "review_launched",
        attention_state: "review_launched",
        launch_profile_id: None,
    })?;
    assert_eq!(created.row_version, 0);

    let updated = store.update_review_session_attention("session-1", 0, "approval_required")?;
    assert_eq!(updated.row_version, 1);
    assert_eq!(updated.attention_state, "approval_required");

    let conflict = store
        .update_review_session_attention("session-1", 0, "completed")
        .expect_err("stale write should fail");
    let message = conflict.to_string();
    assert!(message.contains("stale write conflict"));
    assert!(message.contains("session-1"));

    Ok(())
}
