use crate::core::app::AppContext;
use crate::core::pane::Pane;
use crate::git::state::{GitState, PANE_BRANCH_LIST, PANE_DIFF_VIEW, PANE_GIT_LOG};

pub(crate) fn execute_git_search(git: &mut GitState) {
    git.pane.search.matches.clear();
    git.pane.search.current_match_idx = None;
    let query = match &git.pane.search.query {
        Some(q) => q.clone(),
        None => return,
    };
    let origin = git.pane.search.origin;
    let matches = if let Some(pane) = git.panes.get(origin) {
        pane.collect_search_matches(&git.pane, &query)
    } else {
        vec![]
    };
    git.pane.search.matches = matches;
}

pub(crate) fn jump_to_git_match(ctx: &mut AppContext, git: &mut GitState, forward: bool) {
    // If no active query but last_query exists, re-execute search
    if git.pane.search.query.is_none() {
        if let Some(last) = git.pane.search.last_query.clone() {
            git.pane.search.query = Some(last);
            execute_git_search(git);
        } else {
            return;
        }
    }

    if git.pane.search.matches.is_empty() {
        ctx.status_message = Some("Pattern not found".to_string());
        return;
    }

    let total = git.pane.search.matches.len();
    let new_idx = match git.pane.search.current_match_idx {
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
    git.pane.search.current_match_idx = Some(new_idx);

    let search_match = git.pane.search.matches[new_idx].clone();
    let origin = git.pane.search.origin;

    // Jump + post-jump cross-pane sync
    // Disjoint borrows: git.panes.* fields can be borrowed independently from git.pane
    match origin {
        PANE_DIFF_VIEW => {
            git.panes.diff_view.jump_to_match(&git.pane, &search_match);
        }
        PANE_GIT_LOG => {
            git.panes.git_log.jump_to_match(&git.pane, &search_match);
            git.panes.git_log.load_detail(&git.repo);
        }
        PANE_BRANCH_LIST => {
            git.panes
                .branch_list
                .jump_to_match(&git.pane, &search_match);
            if let Some(branch) = git
                .panes
                .branch_list
                .branches
                .get(git.panes.branch_list.selected_idx)
            {
                let name = branch.name.clone();
                git.panes.git_log.load_for_ref(&git.repo, &name);
            }
        }
        _ => {
            if let Some(pane) = git.panes.get_mut(origin) {
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
    if git.pane.search.origin == PANE_DIFF_VIEW && git.pane.search.query.is_some() {
        git.pane.search.reset_matches();
        git.panes.diff_view.highlight.content_lines_cache = None;
        let query = git.pane.search.query.clone().unwrap();
        git.pane.search.matches = git
            .panes
            .diff_view
            .collect_search_matches(&git.pane, &query);
    }
}
