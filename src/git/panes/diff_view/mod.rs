pub(crate) mod keys;
mod view;

use crate::core::app::AppContext;
use crate::git::state::{DiffViewMode, GitState};
use crate::core::pane::DetailPane;
use crossterm::event::KeyEvent;
use ratatui::{layout::Rect, Frame};

pub struct DiffViewPane;

impl DetailPane<GitState> for DiffViewPane {
    fn handle_key(&self, ctx: &mut AppContext, state: &mut GitState, key: KeyEvent) {
        match state.diff_view_mode {
            DiffViewMode::Scroll => keys::handle_diff_scroll_key(ctx, state, key),
            DiffViewMode::Normal => keys::handle_diff_normal_key(ctx, state, key),
            DiffViewMode::Visual | DiffViewMode::VisualLine => keys::handle_diff_visual_key(ctx, state, key),
        }
    }

    fn render(&self, f: &mut Frame, _ctx: &AppContext, state: &mut GitState, area: Rect) {
        view::render(f, state, area);
    }
}
