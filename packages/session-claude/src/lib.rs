use roger_app_core::{
    AppError, ContinuityQuality, HarnessAdapter, LaunchAction, LaunchIntent,
    ProviderContinuityCapability, Result, ResumeAttemptOutcome, ResumeBundle, ResumeDecision,
    ResumeSessionState, ResumeStrategy, ReviewTarget, SessionLocator, decide_resume_strategy,
    now_ts,
};
use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaudeAdapter {
    binary_path: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ClaudeInvocationMode {
    Start,
    Reseed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct ClaudeInvocationContext {
    mode: ClaudeInvocationMode,
    review_target: ReviewTarget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClaudeSessionPath {
    StartedFresh,
    ReseededFromBundle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaudeSessionLinkage {
    pub locator: SessionLocator,
    pub path: ClaudeSessionPath,
    pub continuity_quality: ContinuityQuality,
    pub decision: Option<ResumeDecision>,
}

impl ClaudeAdapter {
    pub fn new() -> Self {
        Self::with_binary("claude")
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
    ) -> Result<ClaudeSessionLinkage> {
        link_or_resume_claude_session(self, target, intent, locator, resume_bundle)
    }

    fn build_invocation_context(
        mode: ClaudeInvocationMode,
        target: &ReviewTarget,
    ) -> Result<String> {
        let context = ClaudeInvocationContext {
            mode,
            review_target: target.clone(),
        };
        serde_json::to_string(&context).map_err(AppError::SerializationError)
    }
}

impl HarnessAdapter for ClaudeAdapter {
    fn start_session(
        &self,
        target: &ReviewTarget,
        _intent: &LaunchIntent,
    ) -> Result<SessionLocator> {
        let now = now_ts();
        Ok(SessionLocator {
            provider: "claude".to_owned(),
            session_id: format!("cl-{}", now),
            invocation_context_json: Self::build_invocation_context(
                ClaudeInvocationMode::Start,
                target,
            )?,
            captured_at: now,
            last_tested_at: Some(now),
        })
    }

    fn seed_from_resume_bundle(&self, bundle: &ResumeBundle) -> Result<SessionLocator> {
        let now = now_ts();
        Ok(SessionLocator {
            provider: "claude".to_owned(),
            session_id: format!("cl-reseed-{}", now),
            invocation_context_json: Self::build_invocation_context(
                ClaudeInvocationMode::Reseed,
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
        if locator.provider != "claude" {
            return Ok(ContinuityQuality::Unusable);
        }
        Ok(ContinuityQuality::Degraded)
    }

    fn reopen_by_locator(&self, _locator: &SessionLocator) -> Result<()> {
        Err(AppError::HarnessError(
            "claude tier-a adapter does not support direct session reopen; reseed required"
                .to_owned(),
        ))
    }

    fn open_in_bare_harness_mode(
        &self,
        _locator: &SessionLocator,
        _bundle: &ResumeBundle,
    ) -> Result<()> {
        Err(AppError::HarnessError(
            "claude tier-a adapter does not support bare-harness dropout mode".to_owned(),
        ))
    }

    fn return_to_roger_session(&self, _locator: &SessionLocator) -> Result<()> {
        Err(AppError::HarnessError(
            "claude tier-a adapter does not support rr return from bare harness".to_owned(),
        ))
    }
}

pub fn link_or_resume_claude_session(
    adapter: &ClaudeAdapter,
    target: &ReviewTarget,
    intent: &LaunchIntent,
    locator: Option<&SessionLocator>,
    resume_bundle: Option<&ResumeBundle>,
) -> Result<ClaudeSessionLinkage> {
    if matches!(intent.action, LaunchAction::StartReview) {
        let new_locator = adapter.start_session(target, intent)?;
        return Ok(ClaudeSessionLinkage {
            locator: new_locator,
            path: ClaudeSessionPath::StartedFresh,
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
                    "claude resume selected reseed but no ResumeBundle was provided".to_owned(),
                )
            })?;
            ensure_bundle_target_matches(bundle, target)?;
            let locator = adapter.seed_from_resume_bundle(bundle)?;
            Ok(ClaudeSessionLinkage {
                locator,
                path: ClaudeSessionPath::ReseededFromBundle,
                continuity_quality: decision.continuity_quality.clone(),
                decision: Some(decision),
            })
        }
        ResumeStrategy::FailClosed => Err(AppError::HarnessError(format!(
            "claude resume failed closed: {}",
            decision.reason
        ))),
        ResumeStrategy::ReopenExisting => Err(AppError::HarnessError(
            "claude tier-a adapter cannot reopen existing sessions".to_owned(),
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
