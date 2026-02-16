use crate::core::app::{
    App, DiffSide, DiffViewMode, SearchMatch, SearchOrigin, TreeEntry,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

impl App {
    pub(crate) fn handle_search_input_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                let query = self.search.input.clone();
                if query.is_empty() {
                    self.search.active = false;
                    return;
                }
                self.search.push_history(&query);
                self.search.active = false;
                self.search.query = Some(query);
                self.execute_search();
                self.jump_to_match(true);
            }
            KeyCode::Esc => {
                self.search.active = false;
                self.search.input.clear();
            }
            KeyCode::Backspace => {
                self.search.input.pop();
                self.search.history_idx = None;
            }
            KeyCode::Up | KeyCode::Char('p') if key.code == KeyCode::Up || key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.search.history_prev();
            }
            KeyCode::Down | KeyCode::Char('n') if key.code == KeyCode::Down || key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.search.history_next();
            }
            KeyCode::Char(c) => {
                self.search.input.push(c);
                self.search.history_idx = None;
            }
            _ => {}
        }
    }

    fn execute_search(&mut self) {
        self.search.matches.clear();
        self.search.current_match_idx = None;
        let query = match &self.search.query {
            Some(q) => q.clone(),
            None => return,
        };
        match self.search.origin {
            SearchOrigin::DiffView => self.search_diff_view(&query),
            SearchOrigin::FileTree => self.search_file_tree(&query),
            SearchOrigin::CommitLog => self.search_commit_log(&query),
            SearchOrigin::BranchList => self.search_branch_list(&query),
            SearchOrigin::Reflog => self.search_reflog(&query),
        }
    }

    pub(crate) fn search_diff_view(&mut self, query: &str) {
        let query_lower = query.to_lowercase();
        let file = match self.selected_file() {
            Some(f) => f.clone(),
            None => return,
        };
        let mut row_idx: usize = 0;
        for hunk in &file.hunks {
            // Search hunk header
            for (col_start, _) in hunk.header.to_lowercase().match_indices(&query_lower) {
                let col_end = col_start + query.len();
                self.search.matches.push(SearchMatch::DiffLine {
                    row: row_idx,
                    col_start,
                    col_end,
                    side: DiffSide::Left,
                });
            }
            row_idx += 1;

            for row in &hunk.rows {
                // Search left side
                if let Some(ref side_line) = row.left {
                    for (col_start, _) in side_line.content.to_lowercase().match_indices(&query_lower) {
                        let col_end = col_start + query.len();
                        self.search.matches.push(SearchMatch::DiffLine {
                            row: row_idx,
                            col_start,
                            col_end,
                            side: DiffSide::Left,
                        });
                    }
                }
                // Search right side
                if let Some(ref side_line) = row.right {
                    for (col_start, _) in side_line.content.to_lowercase().match_indices(&query_lower) {
                        let col_end = col_start + query.len();
                        self.search.matches.push(SearchMatch::DiffLine {
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
    }

    fn search_file_tree(&mut self, query: &str) {
        let query_lower = query.to_lowercase();
        let entries = self.build_tree_entries();
        for (idx, entry) in entries.iter().enumerate() {
            let name = match entry {
                TreeEntry::Dir { path, .. } => path.clone(),
                TreeEntry::File { file_idx, .. } => {
                    match self.diff_state.files.get(*file_idx) {
                        Some(f) => f.path.clone(),
                        None => continue,
                    }
                }
            };
            if name.to_lowercase().contains(&query_lower) {
                self.search.matches.push(SearchMatch::TreeEntry(idx));
            }
        }
    }

    fn search_commit_log(&mut self, query: &str) {
        let query_lower = query.to_lowercase();
        for (idx, commit) in self.git_log.commits.iter().enumerate() {
            let text = format!(
                "{} {} {} {}",
                commit.short_hash,
                commit.author,
                commit.date,
                commit.message
            );
            if text.to_lowercase().contains(&query_lower) {
                self.search.matches.push(SearchMatch::CommitEntry(idx));
            }
        }
    }

    fn search_branch_list(&mut self, query: &str) {
        let query_lower = query.to_lowercase();
        for (idx, branch) in self.branch_list.branches.iter().enumerate() {
            if branch.name.to_lowercase().contains(&query_lower) {
                self.search.matches.push(SearchMatch::BranchEntry(idx));
            }
        }
    }

    fn search_reflog(&mut self, query: &str) {
        let query_lower = query.to_lowercase();
        for (idx, entry) in self.reflog.entries.iter().enumerate() {
            if entry.short_hash.to_lowercase().contains(&query_lower)
                || entry.selector.to_lowercase().contains(&query_lower)
                || entry.action.to_lowercase().contains(&query_lower)
                || entry.message.to_lowercase().contains(&query_lower)
            {
                self.search.matches.push(SearchMatch::ReflogEntry(idx));
            }
        }
    }

    pub(crate) fn jump_to_match(&mut self, forward: bool) {
        // If no active query but last_query exists, re-execute search
        if self.search.query.is_none() {
            if let Some(last) = self.search.last_query.clone() {
                self.search.query = Some(last);
                self.execute_search();
            } else {
                return;
            }
        }

        if self.search.matches.is_empty() {
            self.status_message = Some("Pattern not found".to_string());
            return;
        }

        let total = self.search.matches.len();
        let new_idx = match self.search.current_match_idx {
            Some(idx) => {
                if forward {
                    (idx + 1) % total
                } else {
                    (idx + total - 1) % total
                }
            }
            None => {
                if forward {
                    0
                } else {
                    total - 1
                }
            }
        };
        self.search.current_match_idx = Some(new_idx);

        match &self.search.matches[new_idx] {
            SearchMatch::DiffLine { row, col_start, side, .. } => {
                let row = *row;
                let col_start = *col_start;
                let side = *side;
                if self.diff_view_mode == DiffViewMode::Scroll {
                    // In scroll mode, just scroll to the row
                    self.diff_scroll_y = row.saturating_sub(
                        (self.diff_view_height / 3) as usize,
                    ) as u16;
                } else {
                    // In Normal/Visual mode, move cursor
                    self.cursor_pos.row = row;
                    self.cursor_pos.col = col_start;
                    self.cursor_pos.side = side;
                    self.content_lines_cache = None; // side may have changed
                    self.scroll_to_cursor();
                }
            }
            SearchMatch::TreeEntry(idx) => {
                self.selected_tree_idx = *idx;
            }
            SearchMatch::CommitEntry(idx) => {
                self.git_log.selected_idx = *idx;
                self.load_commit_detail();
            }
            SearchMatch::BranchEntry(idx) => {
                self.branch_list.selected_idx = *idx;
                self.update_branch_log();
            }
            SearchMatch::ReflogEntry(idx) => {
                self.reflog.selected_idx = *idx;
            }
        }

        self.status_message = Some(format!("[{}/{}]", new_idx + 1, total));
    }
}
