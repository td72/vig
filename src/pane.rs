use crate::app::{App, DiffViewMode, FocusedPane, SearchOrigin, TreeEntry};
use crate::github::state::{GhDetailContent, GhDetailPane, GhFocusedPane};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

// === Traits ===

pub trait SelectPane: Sync {
    fn handle_key(&self, app: &mut App, key: KeyEvent);
}

pub trait DetailPane: Sync {
    fn handle_key(&self, app: &mut App, key: KeyEvent);
}

// === Select Pane implementations (Git) ===

pub struct FileTreePane;
pub struct BranchListPane;
pub struct ReflogPane;
pub struct GitLogSelectPane;

impl SelectPane for FileTreePane {
    fn handle_key(&self, app: &mut App, key: KeyEvent) {
        // Pane navigation must work even when file list is empty
        match key.code {
            KeyCode::Char('l') => {
                app.set_focus(FocusedPane::BranchList);
                return;
            }
            KeyCode::Char('i') => {
                app.set_focus(FocusedPane::DiffView);
                return;
            }
            KeyCode::Esc => {
                if app.search.query.is_some() {
                    app.search.clear();
                }
                return;
            }
            _ => {}
        }

        let entries = app.build_tree_entries();
        if entries.is_empty() {
            return;
        }
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if app.selected_tree_idx + 1 < entries.len() {
                    app.selected_tree_idx += 1;
                    app.diff_scroll_y = 0;
                    app.diff_scroll_x = 0;
                    app.re_search_on_file_change();
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if app.selected_tree_idx > 0 {
                    app.selected_tree_idx -= 1;
                    app.diff_scroll_y = 0;
                    app.diff_scroll_x = 0;
                    app.re_search_on_file_change();
                }
            }
            KeyCode::Char(' ') => {
                if let Some(TreeEntry::Dir { path, .. }) = entries.get(app.selected_tree_idx) {
                    let path = path.clone();
                    if app.collapsed_dirs.contains(&path) {
                        app.collapsed_dirs.remove(&path);
                    } else {
                        app.collapsed_dirs.insert(path);
                    }
                }
            }
            KeyCode::Right | KeyCode::Enter => {
                match entries.get(app.selected_tree_idx) {
                    Some(TreeEntry::Dir { path, .. }) => {
                        let path = path.clone();
                        if app.collapsed_dirs.contains(&path) {
                            app.collapsed_dirs.remove(&path);
                        } else {
                            app.collapsed_dirs.insert(path);
                        }
                    }
                    Some(TreeEntry::File { .. }) => {
                        app.set_focus(FocusedPane::DiffView);
                        app.diff_scroll_y = 0;
                        app.diff_scroll_x = 0;
                    }
                    None => {}
                }
            }
            KeyCode::Char('/') => {
                app.search.start(SearchOrigin::FileTree);
            }
            KeyCode::Char('n') => {
                app.jump_to_match(true);
            }
            KeyCode::Char('N') => {
                app.jump_to_match(false);
            }
            _ => {}
        }
    }
}

impl SelectPane for BranchListPane {
    fn handle_key(&self, app: &mut App, key: KeyEvent) {
        match key.code {
            KeyCode::Char('h') => {
                app.set_focus(FocusedPane::FileTree);
            }
            KeyCode::Char('l') => {
                app.set_focus(FocusedPane::Reflog);
            }
            KeyCode::Char('i') => {
                app.set_focus(FocusedPane::GitLog);
            }
            KeyCode::Esc => {
                if app.search.query.is_some() {
                    app.search.clear();
                } else if app.diff_base_ref.is_some() {
                    app.diff_base_ref = None;
                    if let Err(e) = app.refresh_diff() {
                        app.status_message = Some(format!("Diff error: {e}"));
                    }
                }
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if !app.branch_list.branches.is_empty()
                    && app.branch_list.selected_idx + 1 < app.branch_list.branches.len()
                {
                    app.branch_list.selected_idx += 1;
                    app.update_branch_log();
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if app.branch_list.selected_idx > 0 {
                    app.branch_list.selected_idx -= 1;
                    app.update_branch_log();
                }
            }
            KeyCode::Enter => {
                app.open_branch_action_menu();
            }
            KeyCode::Char('/') => {
                app.search.start(SearchOrigin::BranchList);
            }
            KeyCode::Char('n') => {
                app.jump_to_match(true);
            }
            KeyCode::Char('N') => {
                app.jump_to_match(false);
            }
            _ => {}
        }
    }
}

impl SelectPane for ReflogPane {
    fn handle_key(&self, app: &mut App, key: KeyEvent) {
        match key.code {
            KeyCode::Char('h') => {
                app.set_focus(FocusedPane::BranchList);
            }
            KeyCode::Char('i') => {
                app.set_focus(FocusedPane::GitLog);
            }
            KeyCode::Esc => {
                if app.search.query.is_some() {
                    app.search.clear();
                } else {
                    app.set_focus(FocusedPane::BranchList);
                }
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if !app.reflog.entries.is_empty()
                    && app.reflog.selected_idx + 1 < app.reflog.entries.len()
                {
                    app.reflog.selected_idx += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if app.reflog.selected_idx > 0 {
                    app.reflog.selected_idx -= 1;
                }
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = (app.reflog.view_height / 2).max(1) as usize;
                let new_idx = app.reflog.selected_idx.saturating_add(half);
                app.reflog.selected_idx =
                    new_idx.min(app.reflog.entries.len().saturating_sub(1));
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = (app.reflog.view_height / 2).max(1) as usize;
                app.reflog.selected_idx = app.reflog.selected_idx.saturating_sub(half);
            }
            KeyCode::Char('g') => {
                app.reflog.selected_idx = 0;
            }
            KeyCode::Char('G') => {
                if !app.reflog.entries.is_empty() {
                    app.reflog.selected_idx = app.reflog.entries.len() - 1;
                }
            }
            KeyCode::Enter => {
                if let Some(entry) = app.reflog.entries.get(app.reflog.selected_idx) {
                    app.diff_base_ref = Some(entry.full_hash.clone());
                    if let Err(e) = app.refresh_diff() {
                        app.status_message = Some(format!("Diff error: {e}"));
                    }
                }
            }
            KeyCode::Char('/') => {
                app.search.start(SearchOrigin::Reflog);
            }
            KeyCode::Char('n') => {
                app.jump_to_match(true);
            }
            KeyCode::Char('N') => {
                app.jump_to_match(false);
            }
            _ => {}
        }
    }
}

impl SelectPane for GitLogSelectPane {
    fn handle_key(&self, app: &mut App, key: KeyEvent) {
        match key.code {
            KeyCode::Char('h') => {
                app.set_focus(FocusedPane::Reflog);
            }
            KeyCode::Esc => {
                if app.search.query.is_some() {
                    app.search.clear();
                } else {
                    app.set_focus(app.previous_pane);
                }
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if !app.git_log.commits.is_empty()
                    && app.git_log.selected_idx + 1 < app.git_log.commits.len()
                {
                    app.git_log.selected_idx += 1;
                    app.load_commit_detail();
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if app.git_log.selected_idx > 0 {
                    app.git_log.selected_idx -= 1;
                    app.load_commit_detail();
                }
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = (app.git_log.view_height / 2).max(1) as usize;
                let new_idx = app.git_log.selected_idx.saturating_add(half);
                app.git_log.selected_idx =
                    new_idx.min(app.git_log.commits.len().saturating_sub(1));
                app.load_commit_detail();
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = (app.git_log.view_height / 2).max(1) as usize;
                app.git_log.selected_idx = app.git_log.selected_idx.saturating_sub(half);
                app.load_commit_detail();
            }
            KeyCode::Char('g') => {
                app.git_log.selected_idx = 0;
                app.load_commit_detail();
            }
            KeyCode::Char('G') => {
                if !app.git_log.commits.is_empty() {
                    app.git_log.selected_idx = app.git_log.commits.len() - 1;
                    app.load_commit_detail();
                }
            }
            KeyCode::Char('y') => {
                if let Some(commit) = app.git_log.commits.get(app.git_log.selected_idx) {
                    let hash = commit.full_hash.clone();
                    app.copy_to_clipboard(&hash);
                }
            }
            KeyCode::Char('o') => {
                if let Some(commit) = app.git_log.commits.get(app.git_log.selected_idx) {
                    let hash = commit.full_hash.clone();
                    if let Some(nwo) = crate::github::client::repo_nwo() {
                        let url = format!("https://github.com/{nwo}/commit/{hash}");
                        match crate::github::client::open_url(&url) {
                            Ok(()) => {
                                app.status_message =
                                    Some("Opening in browser...".to_string());
                            }
                            Err(e) => {
                                app.status_message =
                                    Some(format!("Failed to open URL: {e}"));
                            }
                        }
                    } else {
                        app.status_message =
                            Some("Could not determine GitHub repository".to_string());
                    }
                }
            }
            KeyCode::Char('/') => {
                app.search.start(SearchOrigin::CommitLog);
            }
            KeyCode::Char('n') => {
                app.jump_to_match(true);
            }
            KeyCode::Char('N') => {
                app.jump_to_match(false);
            }
            _ => {}
        }
    }
}

// === Detail Pane implementations (Git) ===

pub struct DiffViewPane;

impl DetailPane for DiffViewPane {
    fn handle_key(&self, app: &mut App, key: KeyEvent) {
        match app.diff_view_mode {
            DiffViewMode::Scroll => app.handle_diff_scroll_key(key),
            DiffViewMode::Normal => app.handle_diff_normal_key(key),
            DiffViewMode::Visual | DiffViewMode::VisualLine => app.handle_diff_visual_key(key),
        }
    }
}

// === Select Pane implementations (GitHub) ===

pub struct GhIssueListPane;
pub struct GhPrListPane;

impl SelectPane for GhIssueListPane {
    fn handle_key(&self, app: &mut App, key: KeyEvent) {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if !app.github.issues.is_empty()
                    && app.github.issue_selected_idx + 1 < app.github.issues.len()
                {
                    app.github.issue_selected_idx += 1;
                    app.github.load_selected_issue_detail();
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if app.github.issue_selected_idx > 0 {
                    app.github.issue_selected_idx -= 1;
                    app.github.load_selected_issue_detail();
                }
            }
            KeyCode::Char('g') => {
                app.github.issue_selected_idx = 0;
                app.github.load_selected_issue_detail();
            }
            KeyCode::Char('G') => {
                if !app.github.issues.is_empty() {
                    app.github.issue_selected_idx = app.github.issues.len() - 1;
                    app.github.load_selected_issue_detail();
                }
            }
            KeyCode::Char('l') | KeyCode::Tab => {
                app.github.focused_pane = GhFocusedPane::PrList;
                app.github.load_selected_pr_detail();
            }
            KeyCode::Char('i') | KeyCode::Enter => {
                if !app.github.issues.is_empty() {
                    app.github.previous_pane = GhFocusedPane::IssueList;
                    app.github.focused_pane = GhFocusedPane::Detail;
                    app.github.load_selected_issue_detail();
                }
            }
            KeyCode::Char('o') => {
                if let Some(issue) = app.github.issues.get(app.github.issue_selected_idx) {
                    let number = issue.number;
                    match crate::github::client::open_issue_in_browser(number) {
                        Ok(()) => {
                            app.status_message =
                                Some(format!("Opening issue #{number} in browser..."));
                        }
                        Err(e) => {
                            app.status_message = Some(format!("Failed to open browser: {e}"));
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

impl SelectPane for GhPrListPane {
    fn handle_key(&self, app: &mut App, key: KeyEvent) {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if !app.github.prs.is_empty()
                    && app.github.pr_selected_idx + 1 < app.github.prs.len()
                {
                    app.github.pr_selected_idx += 1;
                    app.github.load_selected_pr_detail();
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if app.github.pr_selected_idx > 0 {
                    app.github.pr_selected_idx -= 1;
                    app.github.load_selected_pr_detail();
                }
            }
            KeyCode::Char('g') => {
                app.github.pr_selected_idx = 0;
                app.github.load_selected_pr_detail();
            }
            KeyCode::Char('G') => {
                if !app.github.prs.is_empty() {
                    app.github.pr_selected_idx = app.github.prs.len() - 1;
                    app.github.load_selected_pr_detail();
                }
            }
            KeyCode::Char('h') | KeyCode::BackTab => {
                app.github.focused_pane = GhFocusedPane::IssueList;
                app.github.load_selected_issue_detail();
            }
            KeyCode::Char('i') | KeyCode::Enter => {
                if !app.github.prs.is_empty() {
                    app.github.previous_pane = GhFocusedPane::PrList;
                    app.github.focused_pane = GhFocusedPane::Detail;
                    app.github.load_selected_pr_detail();
                }
            }
            KeyCode::Char('o') => {
                if let Some(pr) = app.github.prs.get(app.github.pr_selected_idx) {
                    let number = pr.number;
                    match crate::github::client::open_pr_in_browser(number) {
                        Ok(()) => {
                            app.status_message =
                                Some(format!("Opening PR #{number} in browser..."));
                        }
                        Err(e) => {
                            app.status_message = Some(format!("Failed to open browser: {e}"));
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

// === Detail Pane implementations (GitHub) ===

pub struct GhDetailViewPane;

impl DetailPane for GhDetailViewPane {
    fn handle_key(&self, app: &mut App, key: KeyEvent) {
        // Determine item count for selection-based panes
        let pane = app.github.detail_pane;
        let item_count = match pane {
            GhDetailPane::Status => {
                if let GhDetailContent::Pr(ref detail) = app.github.detail {
                    crate::ui::github::detail_view::sorted_checks(detail).len()
                } else {
                    0
                }
            }
            GhDetailPane::Reviews => {
                if let GhDetailContent::Pr(ref detail) = app.github.detail {
                    crate::ui::github::detail_view::meaningful_reviews(&detail.reviews).len()
                } else {
                    0
                }
            }
            GhDetailPane::Comments => match &app.github.detail {
                GhDetailContent::Issue(detail) => detail.comments.len(),
                GhDetailContent::Pr(detail) => detail.comments.len(),
                _ => 0,
            },
            GhDetailPane::Body => 0, // scroll-based
        };
        let selectable = pane != GhDetailPane::Body;

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if selectable && item_count > 0 {
                    let idx = app.github.active_selected_idx_mut();
                    if *idx + 1 < item_count {
                        *idx += 1;
                        // Reset intra-item scroll when selection moves
                        *app.github.active_detail_scroll_mut() = 0;
                    } else {
                        // At last item — scroll within
                        let scroll = app.github.active_detail_scroll_mut();
                        *scroll = scroll.saturating_add(1);
                    }
                } else if !selectable {
                    let scroll = app.github.active_detail_scroll_mut();
                    *scroll = scroll.saturating_add(1);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if selectable {
                    let scroll_val = *app.github.active_detail_scroll_mut();
                    if scroll_val > 0 {
                        // Scroll back within current item first
                        *app.github.active_detail_scroll_mut() = scroll_val - 1;
                    } else {
                        let idx = app.github.active_selected_idx_mut();
                        *idx = idx.saturating_sub(1);
                    }
                } else {
                    let scroll = app.github.active_detail_scroll_mut();
                    *scroll = scroll.saturating_sub(1);
                }
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = (app.github.detail_view_height / 2).max(1);
                let scroll = app.github.active_detail_scroll_mut();
                *scroll = scroll.saturating_add(half);
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = (app.github.detail_view_height / 2).max(1);
                let scroll = app.github.active_detail_scroll_mut();
                *scroll = scroll.saturating_sub(half);
            }
            KeyCode::Char('g') => {
                if selectable {
                    *app.github.active_selected_idx_mut() = 0;
                }
                *app.github.active_detail_scroll_mut() = 0;
            }
            KeyCode::Char('G') => {
                if selectable && item_count > 0 {
                    *app.github.active_selected_idx_mut() = item_count - 1;
                }
                if !selectable || item_count > 0 {
                    *app.github.active_detail_scroll_mut() = u16::MAX / 2;
                }
            }
            KeyCode::Char('h') => {
                app.github.detail_pane = GhDetailPane::Body;
            }
            KeyCode::Char('l') => {
                match app.github.detail_pane {
                    GhDetailPane::Body => {
                        if app.github.is_pr() {
                            app.github.detail_pane = GhDetailPane::Status;
                        } else {
                            app.github.detail_pane = GhDetailPane::Comments;
                        }
                    }
                    _ if app.github.is_pr() => {
                        // Cycle right panes like Tab
                        app.github.detail_pane = match app.github.detail_pane {
                            GhDetailPane::Status => GhDetailPane::Reviews,
                            GhDetailPane::Reviews => GhDetailPane::Comments,
                            GhDetailPane::Comments => GhDetailPane::Status,
                            other => other,
                        };
                    }
                    _ => {}
                }
            }
            KeyCode::Tab => {
                // Cycle right panes forward: Status → Reviews → Comments → Status (PR only)
                if app.github.is_pr() {
                    app.github.detail_pane = match app.github.detail_pane {
                        GhDetailPane::Status => GhDetailPane::Reviews,
                        GhDetailPane::Reviews => GhDetailPane::Comments,
                        GhDetailPane::Comments => GhDetailPane::Status,
                        other => other,
                    };
                }
            }
            KeyCode::BackTab => {
                // Cycle right panes backward (PR only)
                if app.github.is_pr() {
                    app.github.detail_pane = match app.github.detail_pane {
                        GhDetailPane::Status => GhDetailPane::Comments,
                        GhDetailPane::Reviews => GhDetailPane::Status,
                        GhDetailPane::Comments => GhDetailPane::Reviews,
                        other => other,
                    };
                }
            }
            KeyCode::Char('o') => {
                app.open_gh_detail_item();
            }
            KeyCode::Esc => {
                app.github.focused_pane = app.github.previous_pane;
                app.github.watch_mode = false;
            }
            _ => {}
        }
    }
}

// === Detail Pane identifiers ===

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitDetailId {
    DiffView,
    CommitLog,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum GhDetailId {
    DetailView,
}

// === PaneGroup: Select + Detail pair ===

pub struct GitPaneGroup {
    pub select: &'static dyn SelectPane,
    pub detail: GitDetailId,
    pub id: FocusedPane,
}

pub struct GhPaneGroup {
    pub select: &'static dyn SelectPane,
    #[allow(dead_code)]
    pub detail: GhDetailId,
    pub id: GhFocusedPane,
}

// === Tab definitions ===

pub static GIT_GROUPS: &[GitPaneGroup] = &[
    GitPaneGroup {
        select: &FileTreePane,
        detail: GitDetailId::DiffView,
        id: FocusedPane::FileTree,
    },
    GitPaneGroup {
        select: &BranchListPane,
        detail: GitDetailId::CommitLog,
        id: FocusedPane::BranchList,
    },
    GitPaneGroup {
        select: &ReflogPane,
        detail: GitDetailId::CommitLog,
        id: FocusedPane::Reflog,
    },
];

pub static GH_GROUPS: &[GhPaneGroup] = &[
    GhPaneGroup {
        select: &GhIssueListPane,
        detail: GhDetailId::DetailView,
        id: GhFocusedPane::IssueList,
    },
    GhPaneGroup {
        select: &GhPrListPane,
        detail: GhDetailId::DetailView,
        id: GhFocusedPane::PrList,
    },
];

// === Tab cycling ===

pub fn next_git_tab(current: FocusedPane) -> FocusedPane {
    let idx = GIT_GROUPS.iter().position(|g| g.id == current).unwrap_or(0);
    GIT_GROUPS[(idx + 1) % GIT_GROUPS.len()].id
}

pub fn prev_git_tab(current: FocusedPane) -> FocusedPane {
    let idx = GIT_GROUPS.iter().position(|g| g.id == current).unwrap_or(0);
    GIT_GROUPS[(idx + GIT_GROUPS.len() - 1) % GIT_GROUPS.len()].id
}

#[allow(dead_code)]
pub fn next_gh_tab(current: GhFocusedPane) -> GhFocusedPane {
    let idx = GH_GROUPS.iter().position(|g| g.id == current).unwrap_or(0);
    GH_GROUPS[(idx + 1) % GH_GROUPS.len()].id
}

#[allow(dead_code)]
pub fn prev_gh_tab(current: GhFocusedPane) -> GhFocusedPane {
    let idx = GH_GROUPS.iter().position(|g| g.id == current).unwrap_or(0);
    GH_GROUPS[(idx + GH_GROUPS.len() - 1) % GH_GROUPS.len()].id
}

// === Dispatch ===

pub fn git_select_pane(focused: FocusedPane) -> &'static dyn SelectPane {
    GIT_GROUPS
        .iter()
        .find(|g| g.id == focused)
        .map(|g| g.select)
        .unwrap_or(&FileTreePane)
}

pub fn git_detail_for(focused: FocusedPane) -> GitDetailId {
    // GitLog is a nested select inside CommitLog detail
    if focused == FocusedPane::GitLog {
        return GitDetailId::CommitLog;
    }
    GIT_GROUPS
        .iter()
        .find(|g| g.id == focused)
        .map(|g| g.detail)
        .unwrap_or(GitDetailId::DiffView)
}

pub fn gh_select_pane(focused: GhFocusedPane) -> &'static dyn SelectPane {
    GH_GROUPS
        .iter()
        .find(|g| g.id == focused)
        .map(|g| g.select)
        .unwrap_or(&GhIssueListPane)
}

/// Dispatch a key event to the currently focused Git pane.
/// Covers all 5 panes: the 3 select panes in GIT_GROUPS + GitLog (nested select) + DiffView (detail).
pub fn dispatch_git_key(app: &mut App, key: KeyEvent) {
    match app.focused_pane {
        FocusedPane::GitLog => GitLogSelectPane.handle_key(app, key),
        FocusedPane::DiffView => DiffViewPane.handle_key(app, key),
        other => git_select_pane(other).handle_key(app, key),
    }
}

/// Dispatch a key event to the currently focused GitHub pane.
pub fn dispatch_gh_key(app: &mut App, key: KeyEvent) {
    match app.github.focused_pane {
        GhFocusedPane::Detail => GhDetailViewPane.handle_key(app, key),
        other => gh_select_pane(other).handle_key(app, key),
    }
}
