//! Roger-owned TUI cockpit.
//!
//! `roger-tui` keeps the FrankenTUI (`ftui`) dependency isolated behind a
//! small Roger-owned surface: callers (the `rr` CLI) only see
//! [`RogerTuiConfig`], [`run_cockpit`], [`TuiExit`], and [`TuiError`].
//!
//! First-release cockpit scope (see
//! `docs/TUI_WORKSPACE_AND_OPERATOR_FLOW_CONTRACT.md` and the PLAN's TUI
//! requirements): a three-region workspace (status strip, primary pane,
//! inspector pane) over the durable destinations Review Home (with the global
//! session finder), Session Overview, Findings Queue
//! (sort/filter/group/multi-select/batch triage), Draft Approval Queue with
//! an explicit elevated approve confirmation (same storage path as
//! `rr approve`; posting stays CLI-only), Timeline/History, and
//! Search/Recall. Clarify-in-place and the session chat lane are out of this
//! slice and surface as bounded hints.

#[cfg(unix)]
mod app;
mod backend;
mod model;
#[cfg(unix)]
mod tty;
mod view;

pub use backend::{CockpitBackend, StoreCockpitBackend};
pub use model::{
    APPROVE_CONFIRMATION_WORD, ApproveOutcome, CLARIFY_HINT, CockpitEntry, CockpitKey,
    CockpitModel, DecisionEventRow, DraftBatchRow, DraftItemRow, ELEVATED_MUTATION_HINT,
    EMPTY_STORE_HINT, EvidenceRow, FindingDetail, FindingRow, GroupKey, InputMode, KeyOutcome,
    LAUNCH_DEFERRED_HINT, MemoryPostureRow, ModelEffect, OutboundLinkRow, Overlay, PickerCandidate,
    PostedActionRow, Screen, SearchHitRow, SearchView, SessionHomeView, SessionRow, SortKey,
    StageResultRow, TimelineEntry, TimelineRunRow, TimelineView, TriageAction,
};

use roger_storage::{RogerStore, StorageError};
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct RogerTuiConfig {
    /// Root of the canonical Roger store.
    pub store_root: PathBuf,
    /// Optional repository filter for the sessions screen.
    pub repo: Option<String>,
    /// Optional pull-request filter for the sessions screen.
    pub pr: Option<u64>,
    /// Resolved session to open directly on session home. When `None` the
    /// sessions list is the entry screen.
    pub initial_session_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TuiExit {
    /// Operator quit the cockpit normally.
    Quit,
}

#[derive(Debug)]
pub enum TuiError {
    /// The store fails closed under migration posture; same contract as the
    /// CLI's store-open gate.
    StoreMigrationBlocked {
        reason: String,
    },
    Storage(String),
    Terminal(String),
}

impl std::fmt::Display for TuiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StoreMigrationBlocked { reason } => {
                write!(f, "store migration posture blocks the TUI: {reason}")
            }
            Self::Storage(message) => write!(f, "storage error: {message}"),
            Self::Terminal(message) => write!(f, "terminal error: {message}"),
        }
    }
}

impl std::error::Error for TuiError {}

/// Open the operator cockpit and block until the operator quits.
///
/// Interactive-only: callers must verify TTY posture before invoking; this
/// function assumes stdin/stdout are an interactive terminal.
///
/// Thin wrapper over [`run_cockpit_with_entry`] that opens on the config's
/// resolved single session (or the finder). Existing callers keep working
/// unchanged; the session-picker path uses [`run_cockpit_with_entry`].
pub fn run_cockpit(config: RogerTuiConfig) -> Result<TuiExit, TuiError> {
    let entry = CockpitEntry::from_session(config.initial_session_id.clone());
    run_cockpit_with_entry(config, entry)
}

/// Open the operator cockpit for a full [`CockpitEntry`].
///
/// When `entry.picker_candidates` is present and non-empty the cockpit opens on
/// the disambiguation picker; otherwise it behaves like [`run_cockpit`]. The
/// CLI stitches `SessionReentryResolution::PickerRequired` candidates here.
/// `entry.initial_session_id` takes precedence over `config.initial_session_id`.
pub fn run_cockpit_with_entry(
    config: RogerTuiConfig,
    entry: CockpitEntry,
) -> Result<TuiExit, TuiError> {
    let store = match RogerStore::open(&config.store_root) {
        Ok(store) => store,
        Err(StorageError::Conflict { entity, id }) if entity == "store_migration_policy" => {
            return Err(TuiError::StoreMigrationBlocked { reason: id });
        }
        Err(err) => {
            return Err(TuiError::Storage(format!(
                "failed to open Roger store: {err}"
            )));
        }
    };

    let mut backend = StoreCockpitBackend::new(store, config.repo.clone(), config.pr);
    let model =
        CockpitModel::bootstrap_with_entry(&mut backend, &entry).map_err(TuiError::Storage)?;

    #[cfg(unix)]
    {
        use std::sync::Arc;
        use std::sync::atomic::AtomicBool;
        let force_repaint = Arc::new(AtomicBool::new(false));
        run_ftui_program(
            app::CockpitApp::new(model, backend, Arc::clone(&force_repaint)),
            force_repaint,
        )
        .map_err(|err| TuiError::Terminal(err.to_string()))?;
        Ok(TuiExit::Quit)
    }
    // The interactive event source is termios-based; Windows support is a
    // bounded gap, not a degraded imitation. Fail closed with honest guidance.
    #[cfg(not(unix))]
    {
        let _ = model;
        Err(TuiError::Terminal(
            "rr tui interactive mode is not yet supported on Windows; use the rr command surface (or WSL) instead".to_owned(),
        ))
    }
}

/// Drive the cockpit through the ftui runtime with the Roger-owned raw-mode
/// TTY event source (see `tty.rs` for why ftui's own backends are not used).
#[cfg(unix)]
fn run_ftui_program(
    cockpit: app::CockpitApp,
    force_repaint: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> std::io::Result<()> {
    use ftui::runtime::{BackendFeatures, Program, ProgramConfig};
    use ftui::{TerminalCapabilities, TerminalWriter};

    let mut config = ProgramConfig::fullscreen();
    // Keyboard-only slice: no mouse capture, paste, or focus reporting modes.
    config.mouse_capture_policy = ftui::runtime::MouseCapturePolicy::Off;
    config.bracketed_paste = false;
    config.focus_reporting = false;
    config.kitty_keyboard = false;
    // The Roger event source handles terminal lifecycle itself and surfaces
    // Ctrl-C as a key event; no ftui signal interception layer is installed.
    config.intercept_signals = false;

    let events = tty::RogerTtyEventSource::open(force_repaint)?;
    let capabilities = TerminalCapabilities::with_overrides();
    let writer = TerminalWriter::with_diff_config(
        std::io::stdout(),
        config.screen_mode,
        config.ui_anchor,
        capabilities,
        config.diff_config.clone(),
    );
    let mut program =
        Program::with_event_source(cockpit, events, BackendFeatures::default(), writer, config)?;
    program.run()
}

#[cfg(test)]
mod tests;
