use crate::core::app::AppContext;
use crate::core::search::{SearchMatch, SearchOrigin};
use crate::git::state::{DiffSide, DiffViewMode, GitState, TreeEntry};

pub(crate) fn execute_git_search(git: &mut GitState) {
    git.search.matches.clear();
    git.search.current_match_idx = None;
    let query = match &git.search.query {
        Some(q) => q.clone(),
        None => return,
    };
    match git.search.origin {
        SearchOrigin::DiffView => search_git_diff_view(git, &query),
        SearchOrigin::FileTree => search_git_file_tree(git, &query),
        SearchOrigin::CommitLog => search_git_commit_log(git, &query),
        SearchOrigin::BranchList => search_git_branch_list(git, &query),
        SearchOrigin::Reflog => search_git_reflog(git, &query),
    }
}

pub(crate) fn search_git_diff_view(git: &mut GitState, query: &str) {
    let query_lower = query.to_lowercase();
    let file = match git.selected_file() {
        Some(f) => f.clone(),
        None => return,
    };
    let mut row_idx: usize = 0;
    for hunk in &file.hunks {
        // Search hunk header
        for (col_start, _) in hunk.header.to_lowercase().match_indices(&query_lower) {
            let col_end = col_start + query.len();
            git.search.matches.push(SearchMatch::DiffLine {
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
                    git.search.matches.push(SearchMatch::DiffLine {
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
                    git.search.matches.push(SearchMatch::DiffLine {
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

fn search_git_file_tree(git: &mut GitState, query: &str) {
    let query_lower = query.to_lowercase();
    let entries = git.tree_entries();
    for (idx, entry) in entries.iter().enumerate() {
        let name = match entry {
            TreeEntry::Dir { path, .. } => path.clone(),
            TreeEntry::File { file_idx, .. } => match git.diff_state.files.get(*file_idx) {
                Some(f) => f.path.clone(),
                None => continue,
            },
        };
        if name.to_lowercase().contains(&query_lower) {
            git.search.matches.push(SearchMatch::TreeEntry(idx));
        }
    }
}

fn search_git_commit_log(git: &mut GitState, query: &str) {
    let query_lower = query.to_lowercase();
    for (idx, commit) in git.git_log.commits.iter().enumerate() {
        let text = format!(
            "{} {} {} {}",
            commit.short_hash, commit.author, commit.date, commit.message
        );
        if text.to_lowercase().contains(&query_lower) {
            git.search.matches.push(SearchMatch::CommitEntry(idx));
        }
    }
}

fn search_git_branch_list(git: &mut GitState, query: &str) {
    let query_lower = query.to_lowercase();
    for (idx, branch) in git.branch_list.branches.iter().enumerate() {
        if branch.name.to_lowercase().contains(&query_lower) {
            git.search.matches.push(SearchMatch::BranchEntry(idx));
        }
    }
}

fn search_git_reflog(git: &mut GitState, query: &str) {
    let query_lower = query.to_lowercase();
    for (idx, entry) in git.reflog.entries.iter().enumerate() {
        if entry.short_hash.to_lowercase().contains(&query_lower)
            || entry.selector.to_lowercase().contains(&query_lower)
            || entry.action.to_lowercase().contains(&query_lower)
            || entry.message.to_lowercase().contains(&query_lower)
        {
            git.search.matches.push(SearchMatch::ReflogEntry(idx));
        }
    }
}

pub(crate) fn jump_to_git_match(ctx: &mut AppContext, git: &mut GitState, forward: bool) {
    // If no active query but last_query exists, re-execute search
    if git.search.query.is_none() {
        if let Some(last) = git.search.last_query.clone() {
            git.search.query = Some(last);
            execute_git_search(git);
        } else {
            return;
        }
    }

    if git.search.matches.is_empty() {
        ctx.status_message = Some("Pattern not found".to_string());
        return;
    }

    let total = git.search.matches.len();
    let new_idx = match git.search.current_match_idx {
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
    git.search.current_match_idx = Some(new_idx);

    match &git.search.matches[new_idx] {
        SearchMatch::DiffLine {
            row,
            col_start,
            side,
            ..
        } => {
            let row = *row;
            let col_start = *col_start;
            let side = *side;
            if git.vim.mode == DiffViewMode::Scroll {
                // In scroll mode, just scroll to the row
                git.scroll.y = row.saturating_sub((git.scroll.view_height / 3) as usize) as u16;
            } else {
                // In Normal/Visual mode, move cursor
                git.vim.cursor.row = row;
                git.vim.cursor.col = col_start;
                git.vim.cursor.side = side;
                git.highlight.content_lines_cache = None; // side may have changed
                scroll_to_cursor(git);
            }
        }
        SearchMatch::TreeEntry(idx) => {
            git.file_tree.selected_idx = *idx;
        }
        SearchMatch::CommitEntry(idx) => {
            git.git_log.selected_idx = *idx;
            git.load_commit_detail();
        }
        SearchMatch::BranchEntry(idx) => {
            git.branch_list.selected_idx = *idx;
            git.update_branch_log();
        }
        SearchMatch::ReflogEntry(idx) => {
            git.reflog.selected_idx = *idx;
        }
    }

    ctx.status_message = Some(format!("[{}/{}]", new_idx + 1, total));
}

pub(crate) fn scroll_to_cursor(git: &mut GitState) {
    let row = git.vim.cursor.row as u16;
    let height = git.scroll.view_height;
    if height == 0 {
        return;
    }
    if row < git.scroll.y {
        git.scroll.y = row;
    } else if row >= git.scroll.y + height {
        git.scroll.y = row - height + 1;
    }
}

/// Re-execute DiffView search when file selection changes (preserves query)
pub(crate) fn re_search_on_file_change(git: &mut GitState) {
    if git.search.origin == SearchOrigin::DiffView && git.search.query.is_some() {
        git.search.reset_matches();
        git.highlight.content_lines_cache = None;
        let query = git.search.query.clone().unwrap();
        search_git_diff_view(git, &query);
    }
}
