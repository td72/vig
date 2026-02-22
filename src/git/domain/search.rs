use crate::core::app::AppContext;
use crate::core::pane::Pane;
use crate::git::state::{GitState, PANE_BRANCH_LIST, PANE_DIFF_VIEW, PANE_GIT_LOG};

pub(crate) fn execute_git_search(git: &mut GitState) {
    git.shared.pane.search.matches.clear();
    git.shared.pane.search.current_match_idx = None;
    let query = match &git.shared.pane.search.query {
        Some(q) => q.clone(),
        None => return,
    };
    let origin = git.shared.pane.search.origin;
    let matches = if let Some(pane) = git.pane(origin) {
        pane.collect_search_matches(&git.shared.pane, &query)
    } else {
        vec![]
    };
    git.shared.pane.search.matches = matches;
}

pub(crate) fn jump_to_git_match(ctx: &mut AppContext, git: &mut GitState, forward: bool) {
    // If no active query but last_query exists, re-execute search
    if git.shared.pane.search.query.is_none() {
        if let Some(last) = git.shared.pane.search.last_query.clone() {
            git.shared.pane.search.query = Some(last);
            execute_git_search(git);
        } else {
            return;
        }
    }

    if git.shared.pane.search.matches.is_empty() {
        ctx.status_message = Some("Pattern not found".to_string());
        return;
    }

    let total = git.shared.pane.search.matches.len();
    let new_idx = match git.shared.pane.search.current_match_idx {
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
    git.shared.pane.search.current_match_idx = Some(new_idx);

    let search_match = git.shared.pane.search.matches[new_idx].clone();
    let origin = git.shared.pane.search.origin;

    // Jump + post-jump cross-pane sync
    // Note: pane_mut() can't be used here due to borrow conflict with &git.shared.pane
    match origin {
        PANE_DIFF_VIEW => {
            git.diff_view.jump_to_match(&git.shared.pane, &search_match);
        }
        PANE_GIT_LOG => {
            git.git_log.jump_to_match(&git.shared.pane, &search_match);
            git.git_log.load_detail(&git.shared.repo);
        }
        PANE_BRANCH_LIST => {
            git.branch_list
                .jump_to_match(&git.shared.pane, &search_match);
            if let Some(branch) = git.branch_list.branches.get(git.branch_list.selected_idx) {
                let name = branch.name.clone();
                git.git_log.load_for_ref(&git.shared.repo, &name);
            }
        }
        _ => {
            // FileTree, Reflog, etc. — use dynamic dispatch via set_selected_idx
            if let Some(pane) = git.pane_mut(origin) {
                if let crate::core::search::SearchMatch::ListEntry(idx) = search_match {
                    pane.set_selected_idx(idx);
                }
            }
        }
    }

    ctx.status_message = Some(format!("[{}/{}]", new_idx + 1, total));
}

/// Re-execute DiffView search when file selection changes (preserves query)
pub(crate) fn re_search_on_file_change(git: &mut GitState) {
    if git.shared.pane.search.origin == PANE_DIFF_VIEW && git.shared.pane.search.query.is_some() {
        git.shared.pane.search.reset_matches();
        git.diff_view.highlight.content_lines_cache = None;
        let query = git.shared.pane.search.query.clone().unwrap();
        git.shared.pane.search.matches = git
            .diff_view
            .collect_search_matches(&git.shared.pane, &query);
    }
}
