//! Thin ftui adapter for the cockpit.
//!
//! Everything interesting lives in the pure model (`model.rs`) and the
//! storage backend (`backend.rs`). This file only translates ftui events into
//! [`CockpitKey`] values, forwards them to the reducer, and paints the
//! rendered lines. Keep it thin: no domain logic here.

use crate::backend::StoreCockpitBackend;
use crate::model::{CockpitKey, CockpitModel, KeyOutcome, ModelEffect};
use crate::tty;
use ftui::widgets::Widget;
use ftui::widgets::paragraph::Paragraph;
use ftui::{Cmd, Event, Frame, KeyCode, KeyEventKind, Model, Modifiers};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) struct CockpitApp {
    model: CockpitModel,
    backend: StoreCockpitBackend,
    /// Canonical store root, forwarded (as `RR_STORE_ROOT`) to any `rr`
    /// subprocess launched via a dropout so the child reads the same truth.
    store_root: PathBuf,
    /// Shared with the TTY event source: set true after an editor suspension so
    /// the next poll injects a synthetic resize and forces a full repaint.
    force_repaint: Arc<AtomicBool>,
}

impl CockpitApp {
    pub(crate) fn new(
        model: CockpitModel,
        backend: StoreCockpitBackend,
        store_root: PathBuf,
        force_repaint: Arc<AtomicBool>,
    ) -> Self {
        Self {
            model,
            backend,
            store_root,
            force_repaint,
        }
    }

    /// Perform any side effect the pure reducer queued (currently: suspend the
    /// TUI and open `$EDITOR` on a draft body, then persist the revision).
    fn drain_effect(&mut self) {
        let Some(effect) = self.model.take_effect() else {
            return;
        };
        match effect {
            ModelEffect::EditDraft { draft_id, body } => {
                match tty::suspend_and_edit(&body) {
                    Ok(Some(new_body)) if new_body != body => {
                        self.model
                            .apply_draft_revision(&draft_id, &new_body, &mut self.backend);
                    }
                    Ok(Some(_)) => {
                        self.model.notice =
                            Some(format!("draft {draft_id} unchanged — no revision written"));
                    }
                    Ok(None) => {
                        self.model.notice = Some(format!("draft {draft_id} edit cancelled"));
                    }
                    Err(err) => {
                        self.model.notice = Some(format!("editor failed: {err}"));
                    }
                }
                // The alternate screen was clobbered by the editor; force a full
                // repaint on the next poll.
                self.force_repaint.store(true, Ordering::SeqCst);
            }
            ModelEffect::LaunchProvider { label, args } => {
                // Deliberate dropout: suspend the cockpit, run `rr <args>` on
                // the inherited tty (same suspend-spawn-resume path as $EDITOR),
                // then restore and reload. The child is this same `rr` binary.
                let program = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("rr"));
                match tty::suspend_and_run(&program, &args, &self.store_root) {
                    Ok(status) if status.success() => {
                        self.model.notice = Some(format!("returned from {label}"));
                    }
                    Ok(status) => {
                        let code = status
                            .code()
                            .map(|c| c.to_string())
                            .unwrap_or_else(|| "signal".to_owned());
                        self.model.notice = Some(format!("returned from {label} (exit {code})"));
                    }
                    Err(err) => {
                        self.model.notice = Some(format!("launch failed: {err}"));
                    }
                }
                // The alternate screen was clobbered by the child process;
                // reload cockpit truth and force a full repaint.
                self.model.reload_after_return(&mut self.backend);
                self.force_repaint.store(true, Ordering::SeqCst);
            }
        }
    }
}

fn cockpit_key(event: &Event) -> Option<CockpitKey> {
    let Event::Key(key) = event else {
        return None;
    };
    if key.kind == KeyEventKind::Release {
        return None;
    }
    match key.code {
        KeyCode::Up => Some(CockpitKey::Up),
        KeyCode::Down => Some(CockpitKey::Down),
        KeyCode::Enter => Some(CockpitKey::Enter),
        KeyCode::Escape => Some(CockpitKey::Esc),
        KeyCode::Backspace => Some(CockpitKey::Backspace),
        // Pass characters through unchanged: text-input modes (filters,
        // search query, the elevation confirmation) need the raw character;
        // navigation keymaps normalize case in the reducer.
        KeyCode::Char(c) => Some(CockpitKey::Char(c)),
        _ => None,
    }
}

impl Model for CockpitApp {
    type Message = Event;

    fn update(&mut self, msg: Event) -> Cmd<Event> {
        // Raw mode disables ISIG, so Ctrl-C arrives as a key event; honor it
        // as an explicit quit rather than swallowing it.
        if let Event::Key(key) = &msg
            && key.modifiers.contains(Modifiers::CTRL)
            && matches!(key.code, KeyCode::Char('c'))
        {
            return Cmd::Quit;
        }
        let Some(key) = cockpit_key(&msg) else {
            return Cmd::None;
        };
        let outcome = self.model.handle_key(key, &mut self.backend);
        // Perform any queued side effect (e.g. editor suspension) before the
        // next render so the model and screen stay consistent.
        self.drain_effect();
        match outcome {
            KeyOutcome::Quit => Cmd::Quit,
            KeyOutcome::Continue => Cmd::None,
        }
    }

    fn view(&self, frame: &mut Frame) {
        let width = frame.buffer.width();
        let height = frame.buffer.height();
        let lines = self
            .model
            .render_lines(width as usize, height as usize)
            .join("\n");
        let area = ftui::core::geometry::Rect::new(0, 0, width, height);
        Paragraph::new(ftui::text::Text::raw(lines)).render(area, frame);
    }
}
