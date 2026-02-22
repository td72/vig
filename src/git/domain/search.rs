use crate::core::app::AppContext;
use crate::core::search::SearchOrigin;
use crate::git::state::GitState;

pub(crate) fn execute_git_search(git: &mut GitState) {
    git.shared.search.matches.clear();
    git.shared.search.current_match_idx = None;
    let query = match &git.shared.search.query {
        Some(q) => q.clone(),
        None => return,
    };
    let matches = match git.shared.search.origin {
        SearchOrigin::DiffView => {
            let file = git.file_tree.selected_file(&git.shared).cloned();
            git.diff_view.collect_search_matches(file.as_ref(), &query)
        }
        SearchOrigin::FileTree => git.file_tree.collect_search_matches(&git.shared, &query),
        SearchOrigin::CommitLog => git.git_log.collect_search_matches(&query),
        SearchOrigin::BranchList => git.branch_list.collect_search_matches(&query),
        SearchOrigin::Reflog => git.reflog.collect_search_matches(&query),
    };
    git.shared.search.matches = matches;
}

pub(crate) fn jump_to_git_match(ctx: &mut AppContext, git: &mut GitState, forward: bool) {
    // If no active query but last_query exists, re-execute search
    if git.shared.search.query.is_none() {
        if let Some(last) = git.shared.search.last_query.clone() {
            git.shared.search.query = Some(last);
            execute_git_search(git);
        } else {
            return;
        }
    }

    if git.shared.search.matches.is_empty() {
        ctx.status_message = Some("Pattern not found".to_string());
        return;
    }

    let total = git.shared.search.matches.len();
    let new_idx = match git.shared.search.current_match_idx {
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
    git.shared.search.current_match_idx = Some(new_idx);

    match git.shared.search.matches[new_idx] {
        crate::core::search::SearchMatch::DiffLine {
            row,
            col_start,
            side,
            ..
        } => {
            git.diff_view.navigate_to_search_match(row, col_start, side);
        }
        crate::core::search::SearchMatch::TreeEntry(idx) => {
            git.file_tree.selected_idx = idx;
        }
        crate::core::search::SearchMatch::CommitEntry(idx) => {
            git.git_log.selected_idx = idx;
            git.load_commit_detail();
        }
        crate::core::search::SearchMatch::BranchEntry(idx) => {
            git.branch_list.selected_idx = idx;
            git.update_branch_log();
        }
        crate::core::search::SearchMatch::ReflogEntry(idx) => {
            git.reflog.selected_idx = idx;
        }
    }

    ctx.status_message = Some(format!("[{}/{}]", new_idx + 1, total));
}

/// Re-execute DiffView search when file selection changes (preserves query)
pub(crate) fn re_search_on_file_change(git: &mut GitState) {
    if git.shared.search.origin == SearchOrigin::DiffView && git.shared.search.query.is_some() {
        git.shared.search.reset_matches();
        git.diff_view.highlight.content_lines_cache = None;
        let query = git.shared.search.query.clone().unwrap();
        let file = git.file_tree.selected_file(&git.shared).cloned();
        git.shared.search.matches = git.diff_view.collect_search_matches(file.as_ref(), &query);
    }
}
