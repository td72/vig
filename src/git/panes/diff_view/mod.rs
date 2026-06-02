pub mod keys;
pub(crate) mod view;

use crate::core::app::AppContext;
use crate::core::highlight::HighlightState;
use crate::core::keymap::Keymap;
use crate::core::pane::{Pane, PaneShared};
use crate::core::search::SearchMatch;
use crate::git::domain::diff::FileDiff;
use crate::git::state::PaneEvent;

pub use crate::core::search::DiffSide;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffViewMode {
    Scroll,
    Normal,
    Visual,
    VisualLine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorPos {
    pub row: usize,
    pub col: usize,
    pub side: DiffSide,
}

#[derive(Default)]
pub struct DiffScroll {
    pub y: u16,
    pub x: u16,
    pub total_lines: u16,
    pub view_height: u16,
}

pub struct VimState {
    pub mode: DiffViewMode,
    pub cursor: CursorPos,
    pub visual_anchor: Option<CursorPos>,
    pub pending_key: Option<char>,
    pub count: Option<usize>,
}

impl Default for VimState {
    fn default() -> Self {
        Self {
            mode: DiffViewMode::Scroll,
            cursor: CursorPos {
                row: 0,
                col: 0,
                side: DiffSide::Left,
            },
            visual_anchor: None,
            pending_key: None,
            count: None,
        }
    }
}
use crossterm::event::KeyEvent;
use ratatui::{layout::Rect, Frame};
use std::rc::Rc;

pub struct DiffViewPane {
    pub scroll: DiffScroll,
    pub vim: VimState,
    pub highlight: HighlightState,
    pub(crate) content_lines_cache: Option<(String, DiffSide, Vec<String>)>,
    pub current_file_idx: Option<usize>,
    pub files: Rc<Vec<FileDiff>>,
    pub(crate) scroll_keymap: Keymap<keys::DiffScrollAction>,
}

impl DiffViewPane {
    pub fn new(files: Rc<Vec<FileDiff>>) -> Self {
        Self {
            scroll: DiffScroll::default(),
            vim: VimState::default(),
            highlight: HighlightState::new(),
            content_lines_cache: None,
            current_file_idx: None,
            files,
            scroll_keymap: keys::default_scroll_keymap(),
        }
    }

    /// Returns true when the pane is in a mode that intercepts all keys
    /// (Normal/Visual), meaning view-level keybindings should not apply.
    pub fn intercepts_keys(&self) -> bool {
        self.vim.mode != DiffViewMode::Scroll
    }

    pub fn set_file(&mut self, idx: Option<usize>) {
        self.current_file_idx = idx;
    }

    pub fn set_files(&mut self, files: Rc<Vec<FileDiff>>) {
        self.files = files;
    }

    pub fn set_scroll_keymap(&mut self, km: Keymap<keys::DiffScrollAction>) {
        self.scroll_keymap = km;
    }

    pub fn reset_scroll(&mut self) {
        self.scroll.y = 0;
        self.scroll.x = 0;
    }

    /// Set the current file and reset scroll/highlight state for a fresh view.
    pub fn reset_to_file(&mut self, idx: Option<usize>) {
        self.current_file_idx = idx;
        self.scroll.y = 0;
        self.scroll.x = 0;
        self.highlight.reset();
    }

    /// Spawn background syntax highlighting for the given file data.
    #[allow(clippy::type_complexity)]
    pub fn spawn_highlight(
        &mut self,
        file_data: Vec<(String, Vec<String>, Vec<String>, Vec<usize>)>,
    ) {
        self.highlight.spawn_bg_highlight(file_data);
    }

    pub fn current_file(&self) -> Option<&FileDiff> {
        self.current_file_idx.and_then(|i| self.files.get(i))
    }

    fn collect_search_matches_impl(&self, query: &str) -> Vec<SearchMatch> {
        let file = match self.current_file() {
            Some(f) => f,
            None => return vec![],
        };
        let query_lower = query.to_lowercase();
        let mut matches = Vec::new();
        let mut row_idx: usize = 0;
        for hunk in &file.hunks {
            for (col_start, _) in hunk.header.to_lowercase().match_indices(&query_lower) {
                let col_end = col_start + query.len();
                matches.push(SearchMatch::DiffLine {
                    row: row_idx,
                    col_start,
                    col_end,
                    side: DiffSide::Left,
                });
            }
            row_idx += 1;
            for row in &hunk.rows {
                if let Some(ref side_line) = row.left {
                    for (col_start, _) in
                        side_line.content.to_lowercase().match_indices(&query_lower)
                    {
                        let col_end = col_start + query.len();
                        matches.push(SearchMatch::DiffLine {
                            row: row_idx,
                            col_start,
                            col_end,
                            side: DiffSide::Left,
                        });
                    }
                }
                if let Some(ref side_line) = row.right {
                    for (col_start, _) in
                        side_line.content.to_lowercase().match_indices(&query_lower)
                    {
                        let col_end = col_start + query.len();
                        matches.push(SearchMatch::DiffLine {
                            row: row_idx,
                            col_start,
                            col_end,
                            side: DiffSide::Right,
                        });
                    }
                }
                row_idx += 1;
            }
        }
        matches
    }

    pub fn navigate_to_search_match(&mut self, row: usize, col_start: usize, side: DiffSide) {
        if self.vim.mode == DiffViewMode::Scroll {
            self.scroll.y = row.saturating_sub((self.scroll.view_height / 3) as usize) as u16;
        } else {
            self.vim.cursor.row = row;
            self.vim.cursor.col = col_start;
            self.vim.cursor.side = side;
            self.content_lines_cache = None;
            self.scroll_to_cursor();
        }
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

impl Pane<PaneEvent> for DiffViewPane {
    fn handle_key(&mut self, shared: &PaneShared, key: KeyEvent) -> Vec<PaneEvent> {
        match self.vim.mode {
            DiffViewMode::Scroll => keys::handle_diff_scroll_key(self, shared, key),
            DiffViewMode::Normal => keys::handle_diff_normal_key(self, shared, key),
            DiffViewMode::Visual | DiffViewMode::VisualLine => {
                keys::handle_diff_visual_key(self, shared, key)
            }
        }
    }

    fn render(&mut self, f: &mut Frame, _ctx: &AppContext, shared: &PaneShared, area: Rect) {
        view::render(f, self, shared, area);
    }

    fn collect_search_matches(&self, _shared: &PaneShared, query: &str) -> Vec<SearchMatch> {
        self.collect_search_matches_impl(query)
    }

    fn drain_background(&mut self) {
        self.highlight.drain_bg_highlights();
    }

    fn jump_to_match(&mut self, _shared: &PaneShared, search_match: &SearchMatch) {
        if let SearchMatch::DiffLine {
            row,
            col_start,
            side,
            ..
        } = search_match
        {
            self.navigate_to_search_match(*row, *col_start, *side);
        }
    }
}
