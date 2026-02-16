use crate::app::{App, FocusedPane, SearchOrigin};
use crate::core::container::PaneContainer;
use crate::core::pane::{DetailPane, SelectPane};
use crate::git::panes::{
    BranchListPane, DiffViewPane, FileTreePane, GitLogSelectPane, ReflogPane,
};
use crossterm::event::{KeyCode, KeyEvent};

// === Domain types ===

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitDetailId {
    DiffView,
    CommitLog,
}

pub struct GitPaneGroup {
    pub select: &'static dyn SelectPane,
    pub detail: GitDetailId,
    pub id: FocusedPane,
    pub search_origin: SearchOrigin,
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

// === Container ===

pub(crate) struct GitContainer;

impl PaneContainer for GitContainer {
    fn current_index(&self, app: &App) -> Option<usize> {
        GIT_GROUPS.iter().position(|g| g.id == app.focused_pane)
    }
    fn focus_index(&self, app: &mut App, idx: usize) {
        app.set_focus(GIT_GROUPS[idx].id);
    }
    fn pane_at(&self, idx: usize) -> &'static dyn SelectPane {
        GIT_GROUPS[idx].select
    }
    fn len(&self) -> usize {
        GIT_GROUPS.len()
    }

    fn handle_common_key(&self, app: &mut App, key: KeyEvent, idx: usize) -> bool {
        match key.code {
            KeyCode::Char('i') => {
                let target = match GIT_GROUPS[idx].detail {
                    GitDetailId::DiffView => FocusedPane::DiffView,
                    GitDetailId::CommitLog => FocusedPane::GitLog,
                };
                app.set_focus(target);
                true
            }
            KeyCode::Esc if app.search.query.is_some() => {
                app.search.clear();
                true
            }
            KeyCode::Char('/') => {
                app.search.start(GIT_GROUPS[idx].search_origin);
                true
            }
            KeyCode::Char('n') => {
                app.jump_to_match(true);
                true
            }
            KeyCode::Char('N') => {
                app.jump_to_match(false);
                true
            }
            _ => false,
        }
    }
}
