use roger_app_core::{
    AppError, ContinuityQuality, HarnessAdapter, LaunchAction, LaunchIntent,
    ProviderContinuityCapability, Result, ResumeAttemptOutcome, ResumeBundle, ResumeDecision,
    ResumeSessionState, ResumeStrategy, ReviewTarget, SessionLocator, decide_resume_strategy,
    now_ts,
};
use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodexAdapter {
    binary_path: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CodexInvocationMode {
    Start,
    Reseed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct CodexInvocationContext {
    mode: CodexInvocationMode,
    review_target: ReviewTarget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CodexSessionPath {
    StartedFresh,
    ReseededFromBundle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodexSessionLinkage {
    pub locator: SessionLocator,
    pub path: CodexSessionPath,
    pub continuity_quality: ContinuityQuality,
    pub decision: Option<ResumeDecision>,
}

impl CodexAdapter {
    pub fn new() -> Self {
        Self::with_binary("codex")
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
    ) -> Result<CodexSessionLinkage> {
        link_or_resume_codex_session(self, target, intent, locator, resume_bundle)
    }

    fn build_invocation_context(
        mode: CodexInvocationMode,
        target: &ReviewTarget,
    ) -> Result<String> {
        let context = CodexInvocationContext {
            mode,
            review_target: target.clone(),
        };
        serde_json::to_string(&context).map_err(AppError::SerializationError)
    }
}

impl HarnessAdapter for CodexAdapter {
    fn start_session(
        &self,
        target: &ReviewTarget,
        _intent: &LaunchIntent,
    ) -> Result<SessionLocator> {
        let now = now_ts();
        Ok(SessionLocator {
            provider: "codex".to_owned(),
            session_id: format!("cx-{}", now),
            invocation_context_json: Self::build_invocation_context(
                CodexInvocationMode::Start,
                target,
            )?,
            captured_at: now,
            last_tested_at: Some(now),
        })
    }

    fn seed_from_resume_bundle(&self, bundle: &ResumeBundle) -> Result<SessionLocator> {
        let now = now_ts();
        Ok(SessionLocator {
            provider: "codex".to_owned(),
            session_id: format!("cx-reseed-{}", now),
            invocation_context_json: Self::build_invocation_context(
                CodexInvocationMode::Reseed,
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
        if locator.provider != "codex" {
            return Ok(ContinuityQuality::Unusable);
        }
        Ok(ContinuityQuality::Degraded)
    }

    fn reopen_by_locator(&self, _locator: &SessionLocator) -> Result<()> {
        Err(AppError::HarnessError(
            "codex tier-a adapter does not support direct session reopen; reseed required"
                .to_owned(),
        ))
    }

    fn open_in_bare_harness_mode(
        &self,
        _locator: &SessionLocator,
        _bundle: &ResumeBundle,
    ) -> Result<()> {
        Err(AppError::HarnessError(
            "codex tier-a adapter does not support bare-harness dropout mode".to_owned(),
        ))
    }

    fn return_to_roger_session(&self, _locator: &SessionLocator) -> Result<()> {
        Err(AppError::HarnessError(
            "codex tier-a adapter does not support rr return from bare harness".to_owned(),
        ))
    }
}

pub fn link_or_resume_codex_session(
    adapter: &CodexAdapter,
    target: &ReviewTarget,
    intent: &LaunchIntent,
    locator: Option<&SessionLocator>,
    resume_bundle: Option<&ResumeBundle>,
) -> Result<CodexSessionLinkage> {
    if matches!(intent.action, LaunchAction::StartReview) {
        let new_locator = adapter.start_session(target, intent)?;
        return Ok(CodexSessionLinkage {
            locator: new_locator,
            path: CodexSessionPath::StartedFresh,
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
                    "codex resume selected reseed but no ResumeBundle was provided".to_owned(),
                )
            })?;
            ensure_bundle_target_matches(bundle, target)?;
            let locator = adapter.seed_from_resume_bundle(bundle)?;
            Ok(CodexSessionLinkage {
                locator,
                path: CodexSessionPath::ReseededFromBundle,
                continuity_quality: decision.continuity_quality.clone(),
                decision: Some(decision),
            })
        }
        ResumeStrategy::FailClosed => Err(AppError::HarnessError(format!(
            "codex resume failed closed: {}",
            decision.reason
        ))),
        ResumeStrategy::ReopenExisting => Err(AppError::HarnessError(
            "codex tier-a adapter cannot reopen existing sessions".to_owned(),
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
    use roger_app_core::{ResumeBundleProfile, Surface};
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
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
            objective: Some("codex review".to_owned()),
            launch_profile_id: Some("profile-codex".to_owned()),
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
            provider: "codex".to_owned(),
            continuity_quality: ContinuityQuality::Degraded,
            stage_summary: "follow-up pending".to_owned(),
            unresolved_finding_ids: vec!["finding-1".to_owned()],
            outbound_draft_ids: vec![],
            attention_summary: "awaiting_user_input".to_owned(),
            artifact_refs: vec!["artifact-raw".to_owned()],
        }
    }

    #[test]
    fn start_review_links_fresh_codex_session() {
        let adapter = CodexAdapter::new();
        let target = sample_target(42);
        let linkage = adapter
            .link_session(
                &target,
                &sample_intent(LaunchAction::StartReview),
                None,
                None,
            )
            .expect("link session");

        assert_eq!(linkage.path, CodexSessionPath::StartedFresh);
        assert_eq!(linkage.continuity_quality, ContinuityQuality::Degraded);
        assert_eq!(linkage.locator.provider, "codex");
        assert!(linkage.locator.session_id.starts_with("cx-"));
    }

    #[test]
    fn resume_fails_closed_without_resume_bundle() {
        let adapter = CodexAdapter::new();
        let target = sample_target(42);

        let err = adapter
            .link_session(
                &target,
                &sample_intent(LaunchAction::ResumeReview),
                None,
                None,
            )
            .expect_err("expected fail-closed error");

        assert!(
            err.to_string().contains("failed closed"),
            "unexpected error: {err:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn capture_raw_output_reads_codex_export() {
        let tmp = tempdir().expect("tempdir");
        let bin_path = tmp.path().join("codex-stub");
        fs::write(
            &bin_path,
            r#"#!/bin/sh
if [ "$1" = "export" ]; then
  echo '{"raw":"ok"}'
  exit 0
fi
exit 1
"#,
        )
        .expect("write stub");
        let mut perms = fs::metadata(&bin_path).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&bin_path, perms).expect("chmod stub");

        let adapter = CodexAdapter::with_binary(bin_path.to_string_lossy().to_string());
        let locator = SessionLocator {
            provider: "codex".to_owned(),
            session_id: "cx-42".to_owned(),
            invocation_context_json: "{}".to_owned(),
            captured_at: now_ts(),
            last_tested_at: Some(now_ts()),
        };

        let raw = adapter.capture_raw_output(&locator).expect("raw output");
        assert!(raw.contains("\"raw\":\"ok\""));
    }

    #[test]
    fn resume_reseeds_when_bundle_exists() {
        let adapter = CodexAdapter::new();
        let target = sample_target(42);
        let bundle = sample_bundle(target.clone());

        let linkage = adapter
            .link_session(
                &target,
                &sample_intent(LaunchAction::ResumeReview),
                None,
                Some(&bundle),
            )
            .expect("reseed linkage");

        assert_eq!(linkage.path, CodexSessionPath::ReseededFromBundle);
        assert_eq!(linkage.locator.provider, "codex");
        assert_eq!(linkage.continuity_quality, ContinuityQuality::Degraded);
    }
}
