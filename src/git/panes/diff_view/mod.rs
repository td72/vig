pub(crate) mod keys;
pub(crate) mod view;

use crate::core::app::AppContext;
use crate::core::pane::Pane;
use crate::core::search::SearchMatch;
use crate::git::domain::diff::FileDiff;
use crate::git::state::{
    DiffScroll, DiffSide, DiffViewMode, GitShared, HighlightState, PaneEvent, VimState,
};
use crossterm::event::KeyEvent;
use ratatui::{layout::Rect, Frame};

pub struct DiffViewPane {
    pub scroll: DiffScroll,
    pub vim: VimState,
    pub highlight: HighlightState,
    pub current_file_idx: Option<usize>,
}

impl DiffViewPane {
    pub fn new() -> Self {
        Self {
            scroll: DiffScroll::default(),
            vim: VimState::default(),
            highlight: HighlightState::new(),
            current_file_idx: None,
        }
    }

    pub fn set_file(&mut self, idx: Option<usize>) {
        self.current_file_idx = idx;
    }

    pub fn reset_scroll(&mut self) {
        self.scroll.y = 0;
        self.scroll.x = 0;
    }

    pub fn current_file<'a>(&self, shared: &'a GitShared) -> Option<&'a FileDiff> {
        self.current_file_idx
            .and_then(|i| shared.diff_state.files.get(i))
    }

    pub fn handle_key(&mut self, shared: &GitShared, key: KeyEvent) -> Vec<PaneEvent> {
        match self.vim.mode {
            DiffViewMode::Scroll => keys::handle_diff_scroll_key(self, shared, key),
            DiffViewMode::Normal => keys::handle_diff_normal_key(self, shared, key),
            DiffViewMode::Visual | DiffViewMode::VisualLine => {
                keys::handle_diff_visual_key(self, shared, key)
            }
        }
    }

    pub fn render(&mut self, f: &mut Frame, _ctx: &AppContext, shared: &GitShared, area: Rect) {
        view::render(f, self, shared, area);
    }

    pub fn collect_search_matches(&self, shared: &GitShared, query: &str) -> Vec<SearchMatch> {
        let file = match self.current_file(shared) {
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
            self.highlight.content_lines_cache = None;
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

impl Pane<GitShared, PaneEvent> for DiffViewPane {
    fn handle_key(&mut self, shared: &GitShared, key: KeyEvent) -> Vec<PaneEvent> {
        self.handle_key(shared, key)
    }

    fn render(&mut self, f: &mut Frame, ctx: &AppContext, shared: &GitShared, area: Rect) {
        self.render(f, ctx, shared, area)
    }

    fn collect_search_matches(&self, shared: &GitShared, query: &str) -> Vec<SearchMatch> {
        self.collect_search_matches(shared, query)
    }

    fn jump_to_match(&mut self, _shared: &GitShared, search_match: &SearchMatch) {
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
