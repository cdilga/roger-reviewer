//! Cockpit operations boundary (CLI-runtime actions).
//!
//! [`CockpitOps`] is the narrow surface for actions the cockpit cannot perform
//! over storage alone: listing the GitHub PR review queue, launching or
//! resuming a provider review, materializing an outbound draft batch, and
//! posting an approved batch to GitHub. The `rr` CLI implements this trait by
//! dispatching the *same* parsed-command handlers the shell, robot, and
//! browser-extension surfaces use, so every gate (provider support, stale
//! state, approval binding) applies identically no matter which surface drove
//! the action. The reducer never calls these directly — it queues a
//! [`crate::model::ModelEffect`] and the runtime layer drains it here.

/// One PR row in the review-queue lane (projection of `rr queue` items).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueueRow {
    pub pr_number: u64,
    pub title: String,
    pub author: String,
    pub is_draft: bool,
    pub updated_at: String,
    /// Local Roger state for this PR (`not_started`, `in_review`, …).
    pub roger_state: String,
    /// Existing local session covering this PR, when one exists.
    pub session_id: Option<String>,
}

/// The PR review queue for one repository (projection of `rr queue`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QueueView {
    pub repository: String,
    pub rows: Vec<QueueRow>,
}

/// Outcome of a CLI-runtime operation, projected from the command response the
/// equivalent `rr` invocation would have produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpsOutcome {
    /// Whether the command completed (complete/empty/degraded) rather than
    /// blocking or erroring. Blocked outcomes carry their repair actions.
    pub ok: bool,
    pub message: String,
    pub repair_actions: Vec<String>,
    /// Session touched by a launch/resume outcome, when the response names one.
    pub session_id: Option<String>,
}

impl OpsOutcome {
    /// One-line notice for the cockpit status area: the first message line,
    /// plus the first repair action when the operation did not complete.
    pub fn notice(&self) -> String {
        let first_line = self.message.lines().next().unwrap_or("").to_owned();
        match (self.ok, self.repair_actions.first()) {
            (false, Some(repair)) => format!("{first_line} — {repair}"),
            _ => first_line,
        }
    }
}

/// CLI-runtime operations the cockpit can request. Implemented by the `rr`
/// CLI over the exact command handlers the other surfaces use; absent (test
/// models, embedded callers) the cockpit degrades to an honest notice.
pub trait CockpitOps {
    /// Load the open-PR review queue (`rr queue`) for the cockpit's repo scope.
    fn load_queue(&mut self) -> Result<QueueView, String>;
    /// Start (or reuse, unless `fresh`) a review for a PR — `rr review --pr`.
    fn start_review(&mut self, repository: &str, pr: u64, fresh: bool) -> OpsOutcome;
    /// Re-enter an existing review session — `rr resume --session`.
    fn resume_session(&mut self, session_id: &str) -> OpsOutcome;
    /// Materialize an outbound draft batch from findings — `rr send draft`.
    fn create_draft(&mut self, session_id: &str, finding_ids: &[String]) -> OpsOutcome;
    /// Post one approved batch to GitHub — `rr send post`.
    fn post_batch(&mut self, session_id: &str, batch_id: &str) -> OpsOutcome;
}
