use crate::app::{App, FocusedPane};
use crate::github::state::GhFocusedPane;
use crossterm::event::KeyEvent;

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
        app.handle_file_tree_key(key);
    }
}

impl SelectPane for BranchListPane {
    fn handle_key(&self, app: &mut App, key: KeyEvent) {
        app.handle_branch_list_key(key);
    }
}

impl SelectPane for ReflogPane {
    fn handle_key(&self, app: &mut App, key: KeyEvent) {
        app.handle_reflog_key(key);
    }
}

impl SelectPane for GitLogSelectPane {
    fn handle_key(&self, app: &mut App, key: KeyEvent) {
        app.handle_git_log_key(key);
    }
}

// === Detail Pane implementations (Git) ===

pub struct DiffViewPane;

impl DetailPane for DiffViewPane {
    fn handle_key(&self, app: &mut App, key: KeyEvent) {
        app.handle_diff_view_key(key);
    }
}

// === Select Pane implementations (GitHub) ===

pub struct GhIssueListPane;
pub struct GhPrListPane;

impl SelectPane for GhIssueListPane {
    fn handle_key(&self, app: &mut App, key: KeyEvent) {
        app.handle_gh_issue_list_key(key);
    }
}

impl SelectPane for GhPrListPane {
    fn handle_key(&self, app: &mut App, key: KeyEvent) {
        app.handle_gh_pr_list_key(key);
    }
}

// === Detail Pane implementations (GitHub) ===

pub struct GhDetailViewPane;

impl DetailPane for GhDetailViewPane {
    fn handle_key(&self, app: &mut App, key: KeyEvent) {
        app.handle_gh_detail_key(key);
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
