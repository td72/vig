use crate::core::app::{
    App, DiffSide, DiffViewMode, SearchMatch, SearchOrigin, TreeEntry,
};

impl App {
    pub(crate) fn execute_git_search(&mut self) {
        self.git.search.matches.clear();
        self.git.search.current_match_idx = None;
        let query = match &self.git.search.query {
            Some(q) => q.clone(),
            None => return,
        };
        match self.git.search.origin {
            SearchOrigin::DiffView => self.search_git_diff_view(&query),
            SearchOrigin::FileTree => self.search_git_file_tree(&query),
            SearchOrigin::CommitLog => self.search_git_commit_log(&query),
            SearchOrigin::BranchList => self.search_git_branch_list(&query),
            SearchOrigin::Reflog => self.search_git_reflog(&query),
        }
    }

    pub(crate) fn search_git_diff_view(&mut self, query: &str) {
        let query_lower = query.to_lowercase();
        let file = match self.git.selected_file() {
            Some(f) => f.clone(),
            None => return,
        };
        let mut row_idx: usize = 0;
        for hunk in &file.hunks {
            // Search hunk header
            for (col_start, _) in hunk.header.to_lowercase().match_indices(&query_lower) {
                let col_end = col_start + query.len();
                self.git.search.matches.push(SearchMatch::DiffLine {
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
                        self.git.search.matches.push(SearchMatch::DiffLine {
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
                        self.git.search.matches.push(SearchMatch::DiffLine {
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

    fn search_git_file_tree(&mut self, query: &str) {
        let query_lower = query.to_lowercase();
        let entries = self.git.build_tree_entries();
        for (idx, entry) in entries.iter().enumerate() {
            let name = match entry {
                TreeEntry::Dir { path, .. } => path.clone(),
                TreeEntry::File { file_idx, .. } => {
                    match self.git.diff_state.files.get(*file_idx) {
                        Some(f) => f.path.clone(),
                        None => continue,
                    }
                }
            };
            if name.to_lowercase().contains(&query_lower) {
                self.git.search.matches.push(SearchMatch::TreeEntry(idx));
            }
        }
    }

    fn search_git_commit_log(&mut self, query: &str) {
        let query_lower = query.to_lowercase();
        for (idx, commit) in self.git.git_log.commits.iter().enumerate() {
            let text = format!(
                "{} {} {} {}",
                commit.short_hash,
                commit.author,
                commit.date,
                commit.message
            );
            if text.to_lowercase().contains(&query_lower) {
                self.git.search.matches.push(SearchMatch::CommitEntry(idx));
            }
        }
    }

    fn search_git_branch_list(&mut self, query: &str) {
        let query_lower = query.to_lowercase();
        for (idx, branch) in self.git.branch_list.branches.iter().enumerate() {
            if branch.name.to_lowercase().contains(&query_lower) {
                self.git.search.matches.push(SearchMatch::BranchEntry(idx));
            }
        }
    }

    fn search_git_reflog(&mut self, query: &str) {
        let query_lower = query.to_lowercase();
        for (idx, entry) in self.git.reflog.entries.iter().enumerate() {
            if entry.short_hash.to_lowercase().contains(&query_lower)
                || entry.selector.to_lowercase().contains(&query_lower)
                || entry.action.to_lowercase().contains(&query_lower)
                || entry.message.to_lowercase().contains(&query_lower)
            {
                self.git.search.matches.push(SearchMatch::ReflogEntry(idx));
            }
        }
    }

    pub(crate) fn jump_to_git_match(&mut self, forward: bool) {
        // If no active query but last_query exists, re-execute search
        if self.git.search.query.is_none() {
            if let Some(last) = self.git.search.last_query.clone() {
                self.git.search.query = Some(last);
                self.execute_git_search();
            } else {
                return;
            }
        }

        if self.git.search.matches.is_empty() {
            self.status_message = Some("Pattern not found".to_string());
            return;
        }

        let total = self.git.search.matches.len();
        let new_idx = match self.git.search.current_match_idx {
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
        self.git.search.current_match_idx = Some(new_idx);

        match &self.git.search.matches[new_idx] {
            SearchMatch::DiffLine { row, col_start, side, .. } => {
                let row = *row;
                let col_start = *col_start;
                let side = *side;
                if self.git.diff_view_mode == DiffViewMode::Scroll {
                    // In scroll mode, just scroll to the row
                    self.git.diff_scroll_y = row.saturating_sub(
                        (self.git.diff_view_height / 3) as usize,
                    ) as u16;
                } else {
                    // In Normal/Visual mode, move cursor
                    self.git.cursor_pos.row = row;
                    self.git.cursor_pos.col = col_start;
                    self.git.cursor_pos.side = side;
                    self.git.content_lines_cache = None; // side may have changed
                    self.scroll_to_cursor();
                }
            }
            SearchMatch::TreeEntry(idx) => {
                self.git.selected_tree_idx = *idx;
            }
            SearchMatch::CommitEntry(idx) => {
                self.git.git_log.selected_idx = *idx;
                self.git.load_commit_detail();
            }
            SearchMatch::BranchEntry(idx) => {
                self.git.branch_list.selected_idx = *idx;
                self.git.update_branch_log();
            }
            SearchMatch::ReflogEntry(idx) => {
                self.git.reflog.selected_idx = *idx;
            }
        }

        self.status_message = Some(format!("[{}/{}]", new_idx + 1, total));
    }
}
