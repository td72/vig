use crate::git::state::GitState;

/// Re-execute DiffView search when file selection changes (preserves query).
/// `detail_pane` is the pane ID of the detail view that should be re-searched.
pub(crate) fn re_search_on_file_change(git: &mut GitState, detail_pane: usize) {
    if git.pane.search.origin == detail_pane && git.pane.search.query.is_some() {
        git.panes.file_tab.detail.content_lines_cache = None;
        git.pane.execute_search(&mut git.panes);
    }
}
