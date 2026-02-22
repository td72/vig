pub(crate) mod keys;
pub(crate) mod view;

use crate::core::app::AppContext;
use crate::git::domain::diff::FileDiff;
use crate::git::state::{DiffScroll, DiffViewMode, GitShared, HighlightState, PaneEvent, VimState};
use crossterm::event::KeyEvent;
use ratatui::{layout::Rect, Frame};

pub struct DiffViewPane {
    pub scroll: DiffScroll,
    pub vim: VimState,
    pub highlight: HighlightState,
}

impl DiffViewPane {
    pub fn new() -> Self {
        Self {
            scroll: DiffScroll::default(),
            vim: VimState::default(),
            highlight: HighlightState::new(),
        }
    }

    pub fn handle_key(
        &mut self,
        shared: &GitShared,
        file: Option<&FileDiff>,
        key: KeyEvent,
    ) -> Vec<PaneEvent> {
        match self.vim.mode {
            DiffViewMode::Scroll => keys::handle_diff_scroll_key(self, shared, file, key),
            DiffViewMode::Normal => keys::handle_diff_normal_key(self, shared, file, key),
            DiffViewMode::Visual | DiffViewMode::VisualLine => {
                keys::handle_diff_visual_key(self, shared, file, key)
            }
        }
    }

    pub fn render(
        &mut self,
        f: &mut Frame,
        _ctx: &AppContext,
        shared: &GitShared,
        file: Option<&FileDiff>,
        area: Rect,
    ) {
        view::render(f, self, shared, file, area);
    }

    pub fn scroll_to_cursor(&mut self) {
        let row = self.vim.cursor.row as u16;
        let height = self.scroll.view_height;
        if height == 0 {
            return;
        }
        if row < self.scroll.y {
            self.scroll.y = row;
        } else if row >= self.scroll.y + height {
            self.scroll.y = row - height + 1;
        }
    }
}
