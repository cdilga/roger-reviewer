use roger_app_core::{
    AppError, ContinuityQuality, HarnessAdapter, LaunchAction, LaunchIntent,
    ProviderContinuityCapability, Result, ResumeAttemptOutcome, ResumeBundle, ResumeBundleProfile,
    ResumeDecision, ResumeSessionState, ResumeStrategy, ReviewTarget, SessionLocator,
    decide_resume_strategy, now_ts,
};
use roger_storage::{
    ResolveSessionLaunchBinding, ResumeLedgerResolution, RogerStore, StorageError,
};
use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenCodeAdapter {
    binary_path: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum OpenCodeInvocationMode {
    Start,
    Reseed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct OpenCodeInvocationContext {
    mode: OpenCodeInvocationMode,
    review_target: ReviewTarget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reseed_from_provider: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenCodeSessionPath {
    StartedFresh,
    ReopenedByLocator,
    ReseededFromBundle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenCodeSessionLinkage {
    pub locator: SessionLocator,
    pub path: OpenCodeSessionPath,
    pub continuity_quality: ContinuityQuality,
    pub decision: Option<ResumeDecision>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenCodeDropoutTransition {
    pub locator: SessionLocator,
    pub control_bundle: ResumeBundle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenCodeReturnPath {
    ReboundExistingSession,
    ReseededSession,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenCodeReturnOutcome {
    pub session_id: String,
    pub locator: SessionLocator,
    pub path: OpenCodeReturnPath,
    pub continuity_quality: ContinuityQuality,
    pub decision: ResumeDecision,
}

impl OpenCodeAdapter {
    pub fn new() -> Self {
        Self::with_binary("opencode")
    }

    pub fn with_binary(binary_path: impl Into<String>) -> Self {
        Self {
            binary_path: binary_path.into(),
        }
    }

    pub fn link_session(
        &self,
        target: &ReviewTarget,
        intent: &LaunchIntent,
        locator: Option<&SessionLocator>,
        resume_bundle: Option<&ResumeBundle>,
    ) -> Result<OpenCodeSessionLinkage> {
        link_or_resume_opencode_session(self, target, intent, locator, resume_bundle)
    }

    pub fn review_target_from_locator(locator: &SessionLocator) -> Option<ReviewTarget> {
        Self::parse_invocation_context(locator)
            .ok()
            .map(|context| context.review_target)
    }

    fn build_invocation_context(
        mode: OpenCodeInvocationMode,
        target: &ReviewTarget,
        reseed_from_provider: Option<&str>,
    ) -> Result<String> {
        let context = OpenCodeInvocationContext {
            mode,
            review_target: target.clone(),
            reseed_from_provider: reseed_from_provider.map(str::to_owned),
        };
        serde_json::to_string(&context).map_err(AppError::SerializationError)
    }

    fn parse_invocation_context(locator: &SessionLocator) -> Result<OpenCodeInvocationContext> {
        serde_json::from_str(&locator.invocation_context_json).map_err(AppError::SerializationError)
    }
}

impl HarnessAdapter for OpenCodeAdapter {
    fn start_session(
        &self,
        target: &ReviewTarget,
        _intent: &LaunchIntent,
    ) -> Result<SessionLocator> {
        let now = now_ts();
        let invocation_context_json =
            Self::build_invocation_context(OpenCodeInvocationMode::Start, target, None)?;
        Ok(SessionLocator {
            provider: "opencode".to_owned(),
            session_id: format!("oc-{}", now),
            invocation_context_json,
            captured_at: now,
            last_tested_at: Some(now),
        })
    }

    fn seed_from_resume_bundle(&self, bundle: &ResumeBundle) -> Result<SessionLocator> {
        let now = now_ts();
        let invocation_context_json = Self::build_invocation_context(
            OpenCodeInvocationMode::Reseed,
            &bundle.review_target,
            Some(bundle.provider.as_str()),
        )?;
        Ok(SessionLocator {
            provider: "opencode".to_owned(),
            session_id: format!("oc-reseed-{}", now),
            invocation_context_json,
            captured_at: now,
            last_tested_at: Some(now),
        })
    }

    fn capture_raw_output(&self, locator: &SessionLocator) -> Result<String> {
        let output = Command::new(&self.binary_path)
            .arg("export")
            .arg(&locator.session_id)
            .output()
            .map_err(|e| {
                AppError::HarnessError(format!("failed to invoke {}: {}", self.binary_path, e))
            })?;

        if !output.status.success() {
            return Err(AppError::HarnessError(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn report_continuity_quality(
        &self,
        locator: &SessionLocator,
        target: &ReviewTarget,
    ) -> Result<ContinuityQuality> {
        if locator.provider != "opencode" {
            return Ok(ContinuityQuality::Unusable);
        }

        match Self::review_target_from_locator(locator) {
            Some(context_target) if context_target == *target => Ok(ContinuityQuality::Usable),
            Some(_) => Ok(ContinuityQuality::Unusable),
            None => Ok(ContinuityQuality::Degraded),
        }
    }

    fn reopen_by_locator(&self, locator: &SessionLocator) -> Result<()> {
        let status = Command::new(&self.binary_path)
            .arg("--session")
            .arg(&locator.session_id)
            .status()
            .map_err(|e| {
                AppError::HarnessError(format!("failed to invoke {}: {}", self.binary_path, e))
            })?;

        if !status.success() {
            return Err(AppError::HarnessError(format!(
                "failed to reopen opencode session {}",
                locator.session_id
            )));
        }

        Ok(())
    }

    fn open_in_bare_harness_mode(
        &self,
        locator: &SessionLocator,
        _bundle: &ResumeBundle,
    ) -> Result<()> {
        self.reopen_by_locator(locator)
    }

    fn return_to_roger_session(&self, _locator: &SessionLocator) -> Result<()> {
        // This would be called from within the harness or after it exits.
        Ok(())
    }
}

pub fn link_or_resume_opencode_session(
    adapter: &impl HarnessAdapter,
    target: &ReviewTarget,
    intent: &LaunchIntent,
    locator: Option<&SessionLocator>,
    resume_bundle: Option<&ResumeBundle>,
) -> Result<OpenCodeSessionLinkage> {
    if matches!(intent.action, LaunchAction::StartReview) {
        let new_locator = adapter.start_session(target, intent)?;
        return Ok(OpenCodeSessionLinkage {
            locator: new_locator,
            path: OpenCodeSessionPath::StartedFresh,
            continuity_quality: ContinuityQuality::Usable,
            decision: None,
        });
    }

    let resume_state = ResumeSessionState {
        locator_present: locator.is_some(),
        resume_bundle_present: resume_bundle.is_some(),
    };
    let attempt_outcome = attempt_reopen(adapter, locator, target);
    let decision = decide_resume_strategy(
        ProviderContinuityCapability::ReopenByLocator,
        &resume_state,
        attempt_outcome,
    );

    match decision.strategy {
        ResumeStrategy::ReopenExisting => {
            let locator = locator.ok_or_else(|| {
                AppError::HarnessError(
                    "resume decision selected reopen but no session locator is present".to_owned(),
                )
            })?;
            Ok(OpenCodeSessionLinkage {
                locator: locator.clone(),
                path: OpenCodeSessionPath::ReopenedByLocator,
                continuity_quality: decision.continuity_quality.clone(),
                decision: Some(decision),
            })
        }
        ResumeStrategy::ReseedFromBundle => {
            let bundle = resume_bundle.ok_or_else(|| {
                AppError::HarnessError(
                    "resume decision selected reseed but no ResumeBundle is present".to_owned(),
                )
            })?;
            ensure_bundle_target_matches(bundle, target)?;
            let seeded_locator = adapter.seed_from_resume_bundle(bundle)?;
            Ok(OpenCodeSessionLinkage {
                locator: seeded_locator,
                path: OpenCodeSessionPath::ReseededFromBundle,
                continuity_quality: decision.continuity_quality.clone(),
                decision: Some(decision),
            })
        }
        ResumeStrategy::FailClosed => Err(AppError::HarnessError(format!(
            "resume failed closed: {}",
            decision.reason
        ))),
    }
}

pub fn dropout_to_plain_opencode(
    adapter: &impl HarnessAdapter,
    locator: &SessionLocator,
    control_bundle: &ResumeBundle,
) -> Result<OpenCodeDropoutTransition> {
    ensure_dropout_control_bundle(control_bundle)?;
    adapter.open_in_bare_harness_mode(locator, control_bundle)?;
    Ok(OpenCodeDropoutTransition {
        locator: locator.clone(),
        control_bundle: control_bundle.clone(),
    })
}

pub fn rr_return_to_roger_session(
    adapter: &impl HarnessAdapter,
    store: &RogerStore,
    query: ResolveSessionLaunchBinding<'_>,
    outcome: ResumeAttemptOutcome,
) -> Result<OpenCodeReturnOutcome> {
    let resolution = store
        .resolve_resume_ledger(
            query,
            ProviderContinuityCapability::ReopenByLocator,
            outcome,
        )
        .map_err(map_storage_error)?;

    match resolution {
        ResumeLedgerResolution::NotFound => Err(AppError::HarnessError(
            "rr return could not locate a bound Roger session".to_owned(),
        )),
        ResumeLedgerResolution::Ambiguous { session_ids } => Err(AppError::HarnessError(format!(
            "rr return found multiple candidate sessions: {}",
            session_ids.join(", ")
        ))),
        ResumeLedgerResolution::Stale { binding_id, reason } => Err(AppError::HarnessError(
            format!("rr return binding is stale ({binding_id}): {reason}"),
        )),
        ResumeLedgerResolution::MissingSession { session_id } => Err(AppError::HarnessError(
            format!("rr return binding points to missing session {session_id}"),
        )),
        ResumeLedgerResolution::Resolved(ledger) => {
            if ledger.session.provider != "opencode" {
                return Err(AppError::HarnessError(format!(
                    "rr return expected provider opencode but found {}",
                    ledger.session.provider
                )));
            }

            let decision = ledger.decision;
            match decision.strategy.clone() {
                ResumeStrategy::ReopenExisting => {
                    let locator = ledger.session.session_locator.ok_or_else(|| {
                        AppError::HarnessError(
                            "rr return selected reopen but no SessionLocator exists".to_owned(),
                        )
                    })?;
                    adapter.return_to_roger_session(&locator)?;
                    Ok(OpenCodeReturnOutcome {
                        session_id: ledger.session.id,
                        locator,
                        path: OpenCodeReturnPath::ReboundExistingSession,
                        continuity_quality: decision.continuity_quality.clone(),
                        decision,
                    })
                }
                ResumeStrategy::ReseedFromBundle => {
                    let bundle = ledger.resume_bundle.ok_or_else(|| {
                        AppError::HarnessError(
                            "rr return selected reseed but ResumeBundle is missing".to_owned(),
                        )
                    })?;
                    ensure_bundle_target_matches(&bundle, &ledger.session.review_target)?;
                    let locator = adapter.seed_from_resume_bundle(&bundle)?;
                    adapter.return_to_roger_session(&locator)?;
                    Ok(OpenCodeReturnOutcome {
                        session_id: ledger.session.id,
                        locator,
                        path: OpenCodeReturnPath::ReseededSession,
                        continuity_quality: decision.continuity_quality.clone(),
                        decision,
                    })
                }
                ResumeStrategy::FailClosed => Err(AppError::HarnessError(format!(
                    "rr return failed closed: {}",
                    decision.reason
                ))),
            }
        }
    }
}

fn attempt_reopen(
    adapter: &impl HarnessAdapter,
    locator: Option<&SessionLocator>,
    target: &ReviewTarget,
) -> ResumeAttemptOutcome {
    let Some(locator) = locator else {
        return ResumeAttemptOutcome::ReopenUnavailable;
    };

    match adapter.reopen_by_locator(locator) {
        Ok(()) => match adapter.report_continuity_quality(locator, target) {
            Ok(ContinuityQuality::Usable) => ResumeAttemptOutcome::ReopenedUsable,
            Ok(ContinuityQuality::Degraded) | Ok(ContinuityQuality::Unusable) => {
                ResumeAttemptOutcome::ReopenedDegraded
            }
            Err(error) => classify_reopen_error(&error),
        },
        Err(error) => classify_reopen_error(&error),
    }
}

fn classify_reopen_error(error: &AppError) -> ResumeAttemptOutcome {
    let message = error.to_string().to_lowercase();
    if message.contains("target mismatch") {
        ResumeAttemptOutcome::TargetMismatch
    } else if message.contains("missing")
        || message.contains("compacted")
        || message.contains("not found")
        || message.contains("stale")
    {
        ResumeAttemptOutcome::MissingHarnessState
    } else {
        ResumeAttemptOutcome::ReopenUnavailable
    }
}

fn ensure_bundle_target_matches(bundle: &ResumeBundle, target: &ReviewTarget) -> Result<()> {
    if &bundle.review_target == target {
        return Ok(());
    }

    Err(AppError::HarnessError(format!(
        "resume bundle target mismatch: expected {}#{}, found {}#{}",
        target.repository,
        target.pull_request_number,
        bundle.review_target.repository,
        bundle.review_target.pull_request_number
    )))
}

fn ensure_dropout_control_bundle(bundle: &ResumeBundle) -> Result<()> {
    if bundle.profile == ResumeBundleProfile::DropoutControl {
        return Ok(());
    }

    Err(AppError::HarnessError(format!(
        "dropout requires ResumeBundleProfile::DropoutControl, found {:?}",
        bundle.profile
    )))
}

fn map_storage_error(error: StorageError) -> AppError {
    AppError::ProviderError(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use roger_app_core::{LaunchAction, ResumeDecisionReason, Surface};
    use roger_storage::{CreateReviewSession, CreateSessionLaunchBinding, LaunchSurface};
    use std::cell::{Cell, RefCell};
    use tempfile::tempdir;

    fn sample_target(pr_number: u64) -> ReviewTarget {
        ReviewTarget {
            repository: "owner/repo".to_owned(),
            pull_request_number: pr_number,
            base_ref: "main".to_owned(),
            head_ref: "feature".to_owned(),
            base_commit: "aaa".to_owned(),
            head_commit: "bbb".to_owned(),
        }
    }

    fn sample_intent(action: LaunchAction) -> LaunchIntent {
        LaunchIntent {
            action,
            source_surface: Surface::Cli,
            objective: Some("review this PR".to_owned()),
            launch_profile_id: Some("profile-open-pr".to_owned()),
            cwd: Some("/tmp/repo".to_owned()),
            worktree_root: None,
        }
    }

    fn sample_locator(session_id: &str) -> SessionLocator {
        SessionLocator {
            provider: "opencode".to_owned(),
            session_id: session_id.to_owned(),
            invocation_context_json: r#"{"repository":"owner/repo"}"#.to_owned(),
            captured_at: 10,
            last_tested_at: Some(11),
        }
    }

    fn sample_bundle(target: ReviewTarget) -> ResumeBundle {
        ResumeBundle {
            schema_version: 1,
            profile: roger_app_core::ResumeBundleProfile::ReseedResume,
            review_target: target,
            launch_intent: sample_intent(LaunchAction::ResumeReview),
            provider: "opencode".to_owned(),
            continuity_quality: ContinuityQuality::Degraded,
            stage_summary: "follow-up pending".to_owned(),
            unresolved_finding_ids: vec!["finding-1".to_owned()],
            outbound_draft_ids: vec![],
            attention_summary: "awaiting_user_input".to_owned(),
            artifact_refs: vec!["artifact-1".to_owned()],
        }
    }

    fn sample_dropout_bundle(target: ReviewTarget) -> ResumeBundle {
        let mut bundle = sample_bundle(target);
        bundle.profile = ResumeBundleProfile::DropoutControl;
        bundle
    }

    struct StubHarness {
        start_locator: SessionLocator,
        reseed_locator: SessionLocator,
        reopen_error: Option<String>,
        continuity_quality: ContinuityQuality,
        start_calls: Cell<u32>,
        reseed_calls: Cell<u32>,
        open_bare_calls: Cell<u32>,
        return_calls: Cell<u32>,
        reseed_targets: RefCell<Vec<ReviewTarget>>,
    }

    impl StubHarness {
        fn new() -> Self {
            Self {
                start_locator: sample_locator("oc-start"),
                reseed_locator: sample_locator("oc-reseed"),
                reopen_error: None,
                continuity_quality: ContinuityQuality::Usable,
                start_calls: Cell::new(0),
                reseed_calls: Cell::new(0),
                open_bare_calls: Cell::new(0),
                return_calls: Cell::new(0),
                reseed_targets: RefCell::new(Vec::new()),
            }
        }
    }

    impl HarnessAdapter for StubHarness {
        fn start_session(
            &self,
            _target: &ReviewTarget,
            _intent: &LaunchIntent,
        ) -> Result<SessionLocator> {
            self.start_calls.set(self.start_calls.get() + 1);
            Ok(self.start_locator.clone())
        }

        fn seed_from_resume_bundle(&self, bundle: &ResumeBundle) -> Result<SessionLocator> {
            self.reseed_calls.set(self.reseed_calls.get() + 1);
            self.reseed_targets
                .borrow_mut()
                .push(bundle.review_target.clone());
            Ok(self.reseed_locator.clone())
        }

        fn capture_raw_output(&self, _locator: &SessionLocator) -> Result<String> {
            Ok("raw output".to_owned())
        }

        fn report_continuity_quality(
            &self,
            _locator: &SessionLocator,
            _target: &ReviewTarget,
        ) -> Result<ContinuityQuality> {
            Ok(self.continuity_quality.clone())
        }

        fn reopen_by_locator(&self, _locator: &SessionLocator) -> Result<()> {
            if let Some(message) = &self.reopen_error {
                return Err(AppError::HarnessError(message.clone()));
            }
            Ok(())
        }

        fn open_in_bare_harness_mode(
            &self,
            _locator: &SessionLocator,
            _bundle: &ResumeBundle,
        ) -> Result<()> {
            self.open_bare_calls.set(self.open_bare_calls.get() + 1);
            Ok(())
        }

        fn return_to_roger_session(&self, _locator: &SessionLocator) -> Result<()> {
            self.return_calls.set(self.return_calls.get() + 1);
            Ok(())
        }
    }

    #[test]
    fn starts_fresh_session_for_start_review() {
        let harness = StubHarness::new();
        let target = sample_target(42);
        let linkage = link_or_resume_opencode_session(
            &harness,
            &target,
            &sample_intent(LaunchAction::StartReview),
            None,
            None,
        )
        .expect("start session should succeed");

        assert_eq!(linkage.path, OpenCodeSessionPath::StartedFresh);
        assert_eq!(linkage.locator.session_id, "oc-start");
        assert_eq!(linkage.continuity_quality, ContinuityQuality::Usable);
        assert_eq!(harness.start_calls.get(), 1);
        assert_eq!(harness.reseed_calls.get(), 0);
    }

    #[test]
    fn reopens_by_locator_for_resume_when_continuity_is_usable() {
        let harness = StubHarness::new();
        let target = sample_target(42);
        let locator = sample_locator("oc-existing");

        let linkage = link_or_resume_opencode_session(
            &harness,
            &target,
            &sample_intent(LaunchAction::ResumeReview),
            Some(&locator),
            None,
        )
        .expect("resume by locator should succeed");

        assert_eq!(linkage.path, OpenCodeSessionPath::ReopenedByLocator);
        assert_eq!(linkage.locator.session_id, "oc-existing");
        assert_eq!(linkage.continuity_quality, ContinuityQuality::Usable);
        assert_eq!(harness.reseed_calls.get(), 0);
    }

    #[test]
    fn reseeds_when_reopen_is_unavailable_and_bundle_matches_target() {
        let mut harness = StubHarness::new();
        harness.reopen_error = Some("session not found".to_owned());

        let target = sample_target(42);
        let linkage = link_or_resume_opencode_session(
            &harness,
            &target,
            &sample_intent(LaunchAction::ResumeReview),
            Some(&sample_locator("oc-stale")),
            Some(&sample_bundle(target.clone())),
        )
        .expect("reseed fallback should succeed");

        assert_eq!(linkage.path, OpenCodeSessionPath::ReseededFromBundle);
        assert_eq!(linkage.locator.session_id, "oc-reseed");
        assert_eq!(linkage.continuity_quality, ContinuityQuality::Degraded);
        assert_eq!(harness.reseed_calls.get(), 1);
        assert_eq!(harness.reseed_targets.borrow().as_slice(), &[target]);
    }

    #[test]
    fn reseed_fails_closed_when_bundle_target_does_not_match() {
        let mut harness = StubHarness::new();
        harness.reopen_error = Some("session not found".to_owned());

        let expected_target = sample_target(42);
        let mismatched_bundle = sample_bundle(sample_target(99));
        let err = link_or_resume_opencode_session(
            &harness,
            &expected_target,
            &sample_intent(LaunchAction::ResumeReview),
            Some(&sample_locator("oc-stale")),
            Some(&mismatched_bundle),
        )
        .expect_err("target mismatch must fail closed");

        let error_text = err.to_string();
        assert!(error_text.contains("target mismatch"));
        assert_eq!(harness.reseed_calls.get(), 0);
    }

    #[test]
    fn dropout_requires_dropout_control_bundle() {
        let harness = StubHarness::new();
        let locator = sample_locator("oc-session");
        let err = dropout_to_plain_opencode(&harness, &locator, &sample_bundle(sample_target(42)))
            .expect_err("dropout should reject non-dropout bundle profiles");

        assert!(err.to_string().contains("DropoutControl"));
        assert_eq!(harness.open_bare_calls.get(), 0);
    }

    #[test]
    fn dropout_opens_bare_harness_and_returns_control_transition() {
        let harness = StubHarness::new();
        let locator = sample_locator("oc-session");
        let bundle = sample_dropout_bundle(sample_target(42));

        let transition =
            dropout_to_plain_opencode(&harness, &locator, &bundle).expect("dropout should succeed");

        assert_eq!(transition.locator.session_id, "oc-session");
        assert_eq!(
            transition.control_bundle.profile,
            ResumeBundleProfile::DropoutControl
        );
        assert_eq!(harness.open_bare_calls.get(), 1);
    }

    #[test]
    fn rr_return_rebinds_existing_session_after_restart() {
        let harness = StubHarness::new();
        let temp = tempdir().expect("tempdir");
        let root = temp.path().join("profile");
        let target = sample_target(42);

        {
            let store = RogerStore::open(&root).expect("open store");
            store
                .store_resume_bundle("bundle-1", &sample_dropout_bundle(target.clone()))
                .expect("store bundle");
            store
                .create_review_session(CreateReviewSession {
                    id: "session-1",
                    review_target: &target,
                    provider: "opencode",
                    session_locator: Some(&sample_locator("oc-existing")),
                    resume_bundle_artifact_id: Some("bundle-1"),
                    continuity_state: "awaiting_return",
                    attention_state: "awaiting_return",
                    launch_profile_id: None,
                })
                .expect("create session");
            store
                .put_session_launch_binding(CreateSessionLaunchBinding {
                    id: "binding-1",
                    session_id: "session-1",
                    repo_locator: &target.repository,
                    review_target: Some(&target),
                    surface: LaunchSurface::Cli,
                    launch_profile_id: None,
                    ui_target: Some("cli"),
                    instance_preference: Some("reuse_if_possible"),
                    cwd: Some("/tmp/repo"),
                    worktree_root: None,
                })
                .expect("put binding");
        }

        let reopened = RogerStore::open(&root).expect("reopen store");
        let outcome = rr_return_to_roger_session(
            &harness,
            &reopened,
            ResolveSessionLaunchBinding {
                surface: LaunchSurface::Cli,
                repo_locator: &target.repository,
                review_target: Some(&target),
                ui_target: Some("cli"),
                instance_preference: Some("reuse_if_possible"),
            },
            ResumeAttemptOutcome::ReopenedUsable,
        )
        .expect("rr return reopen should succeed");

        assert_eq!(outcome.session_id, "session-1");
        assert_eq!(outcome.path, OpenCodeReturnPath::ReboundExistingSession);
        assert_eq!(outcome.locator.session_id, "oc-existing");
        assert_eq!(outcome.decision.strategy, ResumeStrategy::ReopenExisting);
        assert_eq!(
            outcome.decision.reason_code,
            ResumeDecisionReason::LocatorReopenedUsable
        );
        assert_eq!(harness.return_calls.get(), 1);
        assert_eq!(harness.reseed_calls.get(), 0);
    }

    #[test]
    fn rr_return_reseeds_when_reopen_unavailable() {
        let harness = StubHarness::new();
        let temp = tempdir().expect("tempdir");
        let root = temp.path().join("profile");
        let target = sample_target(42);

        {
            let store = RogerStore::open(&root).expect("open store");
            store
                .store_resume_bundle("bundle-1", &sample_dropout_bundle(target.clone()))
                .expect("store bundle");
            store
                .create_review_session(CreateReviewSession {
                    id: "session-1",
                    review_target: &target,
                    provider: "opencode",
                    session_locator: Some(&sample_locator("oc-stale")),
                    resume_bundle_artifact_id: Some("bundle-1"),
                    continuity_state: "awaiting_return",
                    attention_state: "awaiting_return",
                    launch_profile_id: None,
                })
                .expect("create session");
            store
                .put_session_launch_binding(CreateSessionLaunchBinding {
                    id: "binding-1",
                    session_id: "session-1",
                    repo_locator: &target.repository,
                    review_target: Some(&target),
                    surface: LaunchSurface::Cli,
                    launch_profile_id: None,
                    ui_target: Some("cli"),
                    instance_preference: Some("reuse_if_possible"),
                    cwd: Some("/tmp/repo"),
                    worktree_root: None,
                })
                .expect("put binding");
        }

        let reopened = RogerStore::open(&root).expect("reopen store");
        let outcome = rr_return_to_roger_session(
            &harness,
            &reopened,
            ResolveSessionLaunchBinding {
                surface: LaunchSurface::Cli,
                repo_locator: &target.repository,
                review_target: Some(&target),
                ui_target: Some("cli"),
                instance_preference: Some("reuse_if_possible"),
            },
            ResumeAttemptOutcome::ReopenUnavailable,
        )
        .expect("rr return reseed should succeed");

        assert_eq!(outcome.session_id, "session-1");
        assert_eq!(outcome.path, OpenCodeReturnPath::ReseededSession);
        assert_eq!(outcome.decision.strategy, ResumeStrategy::ReseedFromBundle);
        assert_eq!(
            outcome.decision.reason_code,
            ResumeDecisionReason::ReopenUnavailableNeedsReseed
        );
        assert_eq!(outcome.continuity_quality, ContinuityQuality::Degraded);
        assert_eq!(harness.reseed_calls.get(), 1);
        assert_eq!(harness.return_calls.get(), 1);
    }
}
