mod view;

use crate::app::{App, DiffViewMode};
use crate::pane::DetailPane;
use crossterm::event::KeyEvent;
use ratatui::{layout::Rect, Frame};

pub struct DiffViewPane;

impl DetailPane for DiffViewPane {
    fn handle_key(&self, app: &mut App, key: KeyEvent) {
        match app.diff_view_mode {
            DiffViewMode::Scroll => app.handle_diff_scroll_key(key),
            DiffViewMode::Normal => app.handle_diff_normal_key(key),
            DiffViewMode::Visual | DiffViewMode::VisualLine => app.handle_diff_visual_key(key),
        }
    }

    fn render(&self, f: &mut Frame, app: &mut App, area: Rect) {
        view::render(f, app, area);
    }
}
