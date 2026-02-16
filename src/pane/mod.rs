mod git;
pub(crate) mod github;

pub use git::*;
pub use github::*;

use crate::app::{App, FocusedPane, SearchOrigin};
use crate::container::git::GitContainer;
use crate::container::github::GhContainer;
use crate::container::PaneContainer;
use crate::github::state::GhFocusedPane;
use crossterm::event::KeyEvent;
use ratatui::{layout::Rect, Frame};

// === Traits ===

pub trait SelectPane: Sync {
    fn handle_key(&self, app: &mut App, key: KeyEvent);
    fn render(&self, f: &mut Frame, app: &mut App, area: Rect);
}

pub trait DetailPane: Sync {
    fn handle_key(&self, app: &mut App, key: KeyEvent);
    fn render(&self, f: &mut Frame, app: &mut App, area: Rect);
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
    pub search_origin: SearchOrigin,
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
        search_origin: SearchOrigin::FileTree,
    },
    GitPaneGroup {
        select: &BranchListPane,
        detail: GitDetailId::CommitLog,
        id: FocusedPane::BranchList,
        search_origin: SearchOrigin::BranchList,
    },
    GitPaneGroup {
        select: &ReflogPane,
        detail: GitDetailId::CommitLog,
        id: FocusedPane::Reflog,
        search_origin: SearchOrigin::Reflog,
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

// === Dispatch ===

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

/// Dispatch a key event to the currently focused Git pane.
/// Covers all 5 panes: the 3 select panes in GIT_GROUPS + GitLog (nested select) + DiffView (detail).
pub fn dispatch_git_key(app: &mut App, key: KeyEvent) {
    match app.focused_pane {
        FocusedPane::GitLog => GitLogSelectPane.handle_key(app, key),
        FocusedPane::DiffView => DiffViewPane.handle_key(app, key),
        _ => GitContainer.dispatch(app, key),
    }
}

/// Dispatch a key event to the currently focused GitHub pane.
pub fn dispatch_gh_key(app: &mut App, key: KeyEvent) {
    match app.github.focused_pane {
        GhFocusedPane::Detail => GhDetailViewPane.handle_key(app, key),
        _ => GhContainer.dispatch(app, key),
    }
}
