use roger_app_core::{
    AppError, ContinuityQuality, HarnessAdapter, LaunchAction, LaunchIntent,
    ProviderContinuityCapability, Result, ResumeAttemptOutcome, ResumeBundle, ResumeDecision,
    ResumeSessionState, ResumeStrategy, ReviewTarget, SessionLocator, decide_resume_strategy,
    now_ts,
};
use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeminiAdapter {
    binary_path: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum GeminiInvocationMode {
    Start,
    Reseed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct GeminiInvocationContext {
    mode: GeminiInvocationMode,
    review_target: ReviewTarget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GeminiSessionPath {
    StartedFresh,
    ReseededFromBundle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeminiSessionLinkage {
    pub locator: SessionLocator,
    pub path: GeminiSessionPath,
    pub continuity_quality: ContinuityQuality,
    pub decision: Option<ResumeDecision>,
}

impl GeminiAdapter {
    pub fn new() -> Self {
        Self::with_binary("gemini")
    }

    pub fn with_binary(binary_path: impl Into<String>) -> Self {
        Self {
            binary_path: binary_path.into(),
        }
    }

    pub fn continuity_capability(&self) -> ProviderContinuityCapability {
        ProviderContinuityCapability::ReseedOnly
    }

    pub fn link_session(
        &self,
        target: &ReviewTarget,
        intent: &LaunchIntent,
        locator: Option<&SessionLocator>,
        resume_bundle: Option<&ResumeBundle>,
    ) -> Result<GeminiSessionLinkage> {
        link_or_resume_gemini_session(self, target, intent, locator, resume_bundle)
    }

    fn build_invocation_context(
        mode: GeminiInvocationMode,
        target: &ReviewTarget,
    ) -> Result<String> {
        let context = GeminiInvocationContext {
            mode,
            review_target: target.clone(),
        };
        serde_json::to_string(&context).map_err(AppError::SerializationError)
    }
}

impl HarnessAdapter for GeminiAdapter {
    fn start_session(
        &self,
        target: &ReviewTarget,
        _intent: &LaunchIntent,
    ) -> Result<SessionLocator> {
        let now = now_ts();
        Ok(SessionLocator {
            provider: "gemini".to_owned(),
            session_id: format!("gm-{}", now),
            invocation_context_json: Self::build_invocation_context(
                GeminiInvocationMode::Start,
                target,
            )?,
            captured_at: now,
            last_tested_at: Some(now),
        })
    }

    fn seed_from_resume_bundle(&self, bundle: &ResumeBundle) -> Result<SessionLocator> {
        let now = now_ts();
        Ok(SessionLocator {
            provider: "gemini".to_owned(),
            session_id: format!("gm-reseed-{}", now),
            invocation_context_json: Self::build_invocation_context(
                GeminiInvocationMode::Reseed,
                &bundle.review_target,
            )?,
            captured_at: now,
            last_tested_at: Some(now),
        })
    }

    fn capture_raw_output(&self, locator: &SessionLocator) -> Result<String> {
        let output = Command::new(&self.binary_path)
            .arg("export")
            .arg(&locator.session_id)
            .output()
            .map_err(|error| {
                AppError::HarnessError(format!("failed to invoke {}: {}", self.binary_path, error))
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
        _target: &ReviewTarget,
    ) -> Result<ContinuityQuality> {
        if locator.provider != "gemini" {
            return Ok(ContinuityQuality::Unusable);
        }
        Ok(ContinuityQuality::Degraded)
    }

    fn reopen_by_locator(&self, _locator: &SessionLocator) -> Result<()> {
        Err(AppError::HarnessError(
            "gemini tier-a adapter does not support direct session reopen; reseed required"
                .to_owned(),
        ))
    }

    fn open_in_bare_harness_mode(
        &self,
        _locator: &SessionLocator,
        _bundle: &ResumeBundle,
    ) -> Result<()> {
        Err(AppError::HarnessError(
            "gemini tier-a adapter does not support bare-harness dropout mode".to_owned(),
        ))
    }

    fn return_to_roger_session(&self, _locator: &SessionLocator) -> Result<()> {
        Err(AppError::HarnessError(
            "gemini tier-a adapter does not support rr return from bare harness".to_owned(),
        ))
    }
}

pub fn link_or_resume_gemini_session(
    adapter: &GeminiAdapter,
    target: &ReviewTarget,
    intent: &LaunchIntent,
    locator: Option<&SessionLocator>,
    resume_bundle: Option<&ResumeBundle>,
) -> Result<GeminiSessionLinkage> {
    if matches!(intent.action, LaunchAction::StartReview) {
        let new_locator = adapter.start_session(target, intent)?;
        return Ok(GeminiSessionLinkage {
            locator: new_locator,
            path: GeminiSessionPath::StartedFresh,
            continuity_quality: ContinuityQuality::Degraded,
            decision: None,
        });
    }

    let session_state = ResumeSessionState {
        locator_present: locator.is_some(),
        resume_bundle_present: resume_bundle.is_some(),
    };
    let decision = decide_resume_strategy(
        adapter.continuity_capability(),
        &session_state,
        ResumeAttemptOutcome::ReopenUnavailable,
    );

    match decision.strategy {
        ResumeStrategy::ReseedFromBundle => {
            let bundle = resume_bundle.ok_or_else(|| {
                AppError::HarnessError(
                    "gemini resume selected reseed but no ResumeBundle was provided".to_owned(),
                )
            })?;
            ensure_bundle_target_matches(bundle, target)?;
            let locator = adapter.seed_from_resume_bundle(bundle)?;
            Ok(GeminiSessionLinkage {
                locator,
                path: GeminiSessionPath::ReseededFromBundle,
                continuity_quality: decision.continuity_quality.clone(),
                decision: Some(decision),
            })
        }
        ResumeStrategy::FailClosed => Err(AppError::HarnessError(format!(
            "gemini resume failed closed: {}",
            decision.reason
        ))),
        ResumeStrategy::ReopenExisting => Err(AppError::HarnessError(
            "gemini tier-a adapter cannot reopen existing sessions".to_owned(),
        )),
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

#[cfg(test)]
mod tests {
    use super::*;
    use roger_app_core::{
        FindingsBoundaryInput, FindingsBoundaryState, ResumeBundleProfile, Surface,
        validate_structured_findings_boundary,
    };
    use roger_storage::{ArtifactBudgetClass, CreateReviewRun, CreateReviewSession, RogerStore};
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
            launch_profile_id: Some("profile-gemini".to_owned()),
            cwd: Some("/tmp/repo".to_owned()),
            worktree_root: None,
        }
    }

    fn sample_bundle(target: ReviewTarget) -> ResumeBundle {
        ResumeBundle {
            schema_version: 1,
            profile: ResumeBundleProfile::ReseedResume,
            review_target: target,
            launch_intent: sample_intent(LaunchAction::ResumeReview),
            provider: "gemini".to_owned(),
            continuity_quality: ContinuityQuality::Degraded,
            stage_summary: "follow-up pending".to_owned(),
            unresolved_finding_ids: vec!["finding-1".to_owned()],
            outbound_draft_ids: vec![],
            attention_summary: "awaiting_user_input".to_owned(),
            artifact_refs: vec!["artifact-raw".to_owned()],
        }
    }

    fn sample_locator(session_id: &str) -> SessionLocator {
        SessionLocator {
            provider: "gemini".to_owned(),
            session_id: session_id.to_owned(),
            invocation_context_json: r#"{"mode":"start","review_target":{"repository":"owner/repo","pull_request_number":42,"base_ref":"main","head_ref":"feature","base_commit":"aaa","head_commit":"bbb"}}"#.to_owned(),
            captured_at: 100,
            last_tested_at: Some(100),
        }
    }

    #[test]
    fn starts_fresh_session_for_start_review() {
        let adapter = GeminiAdapter::new();
        let linkage = link_or_resume_gemini_session(
            &adapter,
            &sample_target(42),
            &sample_intent(LaunchAction::StartReview),
            None,
            None,
        )
        .expect("start session succeeds");

        assert_eq!(linkage.path, GeminiSessionPath::StartedFresh);
        assert_eq!(linkage.locator.provider, "gemini");
        assert!(linkage.locator.session_id.starts_with("gm-"));
    }

    #[test]
    fn reseeds_when_resume_bundle_is_available() {
        let adapter = GeminiAdapter::new();
        let bundle = sample_bundle(sample_target(42));
        let linkage = link_or_resume_gemini_session(
            &adapter,
            &sample_target(42),
            &sample_intent(LaunchAction::ResumeReview),
            Some(&sample_locator("gm-stale")),
            Some(&bundle),
        )
        .expect("reseed succeeds");

        assert_eq!(linkage.path, GeminiSessionPath::ReseededFromBundle);
        assert_eq!(
            linkage.decision.expect("resume decision").reason_code,
            roger_app_core::ResumeDecisionReason::ProviderLimitedNeedsReseed
        );
        assert!(linkage.locator.session_id.starts_with("gm-reseed-"));
    }

    #[test]
    fn fails_closed_without_resume_bundle() {
        let adapter = GeminiAdapter::new();
        let error = link_or_resume_gemini_session(
            &adapter,
            &sample_target(42),
            &sample_intent(LaunchAction::ResumeReview),
            Some(&sample_locator("gm-stale")),
            None,
        )
        .expect_err("resume should fail closed without bundle");

        assert!(
            error.to_string().contains(
                "provider does not support locator reopen and no ResumeBundle is available"
            )
        );
    }

    #[test]
    fn rejects_deeper_continuity_capabilities() {
        let adapter = GeminiAdapter::new();
        let locator = sample_locator("gm-123");
        let bundle = sample_bundle(sample_target(42));

        assert!(
            adapter
                .reopen_by_locator(&locator)
                .expect_err("reopen unsupported")
                .to_string()
                .contains("tier-a")
        );
        assert!(
            adapter
                .open_in_bare_harness_mode(&locator, &bundle)
                .expect_err("dropout unsupported")
                .to_string()
                .contains("tier-a")
        );
        assert!(
            adapter
                .return_to_roger_session(&locator)
                .expect_err("return unsupported")
                .to_string()
                .contains("tier-a")
        );
    }

    #[test]
    fn persists_raw_and_structured_inputs_in_same_ledger()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempdir()?;
        let store = RogerStore::open(temp.path())?;
        let target = sample_target(42);
        let adapter = GeminiAdapter::new();
        let locator = adapter.start_session(&target, &sample_intent(LaunchAction::StartReview))?;

        store.create_review_session(CreateReviewSession {
            id: "session-gemini-1",
            review_target: &target,
            provider: "gemini",
            session_locator: Some(&locator),
            resume_bundle_artifact_id: Some("resume-bundle-gemini-1"),
            continuity_state: "review_launched",
            attention_state: "awaiting_user_input",
            launch_profile_id: Some("profile-gemini"),
        })?;
        store.create_review_run(CreateReviewRun {
            id: "run-gemini-1",
            session_id: "session-gemini-1",
            run_kind: "explore",
            repo_snapshot: "git:feedface",
            continuity_quality: "degraded",
            session_locator_artifact_id: None,
        })?;

        let raw_bytes = b"{\"provider\":\"gemini\",\"raw\":\"output\"}";
        store.store_artifact(
            "artifact-raw",
            ArtifactBudgetClass::ColdArtifact,
            "application/json",
            raw_bytes,
        )?;
        let bundle = sample_bundle(target.clone());
        store.store_resume_bundle("resume-bundle-gemini-1", &bundle)?;

        let structured_pack = r#"{
            "schema_version": "structured_findings_pack/v1",
            "stage": "explore",
            "findings": [
                {
                    "fingerprint": "fp-gemini-1",
                    "title": "Potential null-check gap",
                    "normalized_summary": "Gemini observed a guard that may not hold.",
                    "severity": "medium",
                    "confidence": "medium",
                    "code_evidence": []
                }
            ]
        }"#;

        let boundary = validate_structured_findings_boundary(FindingsBoundaryInput {
            raw_output_artifact_id: Some("artifact-raw"),
            pack_json: Some(structured_pack),
            repair_attempt: 0,
            retry_budget: 1,
        });
        assert_eq!(boundary.state, FindingsBoundaryState::Structured);
        assert_eq!(
            boundary
                .refresh_candidates()
                .expect("structured findings should produce refresh candidates")
                .len(),
            1
        );

        let reopened = RogerStore::open(temp.path())?;
        assert_eq!(reopened.artifact_bytes("artifact-raw")?, raw_bytes);
        let loaded_bundle = reopened.load_resume_bundle("resume-bundle-gemini-1")?;
        assert_eq!(loaded_bundle.provider, "gemini");
        assert_eq!(loaded_bundle.review_target, target);

        Ok(())
    }
}
