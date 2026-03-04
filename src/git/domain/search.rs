use crate::git::state::{GitState, PANE_DIFF_VIEW};

/// Re-execute DiffView search when file selection changes (preserves query)
pub(crate) fn re_search_on_file_change(git: &mut GitState) {
    if git.pane.search.origin == PANE_DIFF_VIEW && git.pane.search.query.is_some() {
        git.panes.diff_view.highlight.content_lines_cache = None;
        git.pane.execute_search(&mut git.panes);
    }
}
