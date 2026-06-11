//! Thin ftui adapter for the cockpit.
//!
//! Everything interesting lives in the pure model (`model.rs`) and the
//! storage backend (`backend.rs`). This file only translates ftui events into
//! [`CockpitKey`] values, forwards them to the reducer, and paints the
//! rendered lines. Keep it thin: no domain logic here.

use crate::backend::StoreCockpitBackend;
use crate::model::{CockpitKey, CockpitModel, KeyOutcome};
use ftui::widgets::Widget;
use ftui::widgets::paragraph::Paragraph;
use ftui::{Cmd, Event, Frame, KeyCode, KeyEventKind, Model, Modifiers};

pub(crate) struct CockpitApp {
    model: CockpitModel,
    backend: StoreCockpitBackend,
}

impl CockpitApp {
    pub(crate) fn new(model: CockpitModel, backend: StoreCockpitBackend) -> Self {
        Self { model, backend }
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
        match self.model.handle_key(key, &mut self.backend) {
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
