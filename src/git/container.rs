use crate::core::app::{App, SearchOrigin};
use crate::core::container::PaneContainer;
use crate::core::pane::{DetailPane, SelectPane};
use crate::core::ui::{branch_action_menu, status_bar};
use crate::core::view::View;
use crate::git::layout;
use crate::git::panes::{
    BranchListPane, DiffViewPane, FileTreePane, GitLogSelectPane, ReflogPane,
};
use crate::git::state::{DiffViewMode, FocusedPane};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{layout::Rect, Frame};

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
    match app.git.focused_pane {
        FocusedPane::GitLog => GitLogSelectPane.handle_key(app, key),
        FocusedPane::DiffView => DiffViewPane.handle_key(app, key),
        _ => GitContainer.dispatch(app, key),
    }
}

// === View-level key handling ===

pub fn handle_git_view_key(app: &mut App, key: KeyEvent) -> Result<bool> {
    // In Normal/Visual modes, keys are handled by the mode handler exclusively
    if app.git.focused_pane == FocusedPane::DiffView && app.git.diff_view_mode != DiffViewMode::Scroll {
        dispatch_git_key(app, key);
        return Ok(false);
    }

    match key.code {
        KeyCode::Char('q') => {
            app.ctx.should_quit = true;
        }
        KeyCode::Char('?') => {
            app.ctx.show_help = true;
        }
        KeyCode::Char('/') => {
            app.git.search.start(search_origin_for(app.git.focused_pane));
        }
        KeyCode::Char('n') => {
            app.jump_to_git_match(true);
        }
        KeyCode::Char('N') => {
            app.jump_to_git_match(false);
        }
        KeyCode::Char('r') => {
            app.refresh_diff()?;
            app.git.load_branches();
            app.git.load_reflog();
        }
        KeyCode::Char('e') => {
            return Ok(true); // Signal to open editor
        }
        KeyCode::Tab => {
            app.git.set_focus(next_git_tab(app.git.focused_pane));
        }
        KeyCode::BackTab => {
            app.git.set_focus(prev_git_tab(app.git.focused_pane));
        }
        _ => dispatch_git_key(app, key),
    }
    Ok(false)
}

fn search_origin_for(pane: FocusedPane) -> SearchOrigin {
    match pane {
        FocusedPane::DiffView => SearchOrigin::DiffView,
        FocusedPane::FileTree => SearchOrigin::FileTree,
        FocusedPane::BranchList => SearchOrigin::BranchList,
        FocusedPane::GitLog => SearchOrigin::CommitLog,
        FocusedPane::Reflog => SearchOrigin::Reflog,
    }
}

// === Container ===

pub(crate) struct GitContainer;

impl PaneContainer for GitContainer {
    fn current_index(&self, app: &App) -> Option<usize> {
        GIT_GROUPS.iter().position(|g| g.id == app.git.focused_pane)
    }
    fn focus_index(&self, app: &mut App, idx: usize) {
        app.git.set_focus(GIT_GROUPS[idx].id);
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
                app.git.set_focus(target);
                true
            }
            KeyCode::Esc if app.git.search.query.is_some() => {
                app.git.search.clear();
                true
            }
            _ => false,
        }
    }
}

// === View ===

pub struct GitView;

impl View for GitView {
    fn intercepts_all_keys(&self, app: &App) -> bool {
        app.git.branch_action_menu.is_some() || app.git.search.active
    }

    fn handle_key(&self, app: &mut App, key: KeyEvent) -> Result<bool> {
        // Branch action menu intercepts all keys when open
        if app.git.branch_action_menu.is_some() {
            app.handle_branch_action_menu_key(key);
            return Ok(false);
        }

        // Search input mode intercepts all keys
        if app.git.search.active {
            if app.git.search.handle_input_key(key) {
                app.execute_git_search();
                app.jump_to_git_match(true);
            }
            return Ok(false);
        }

        handle_git_view_key(app, key)
    }

    fn render(&self, f: &mut Frame, app: &mut App, area: Rect) {
        let ly = layout::compute_layout(area);
        status_bar::render_header(f, app, ly.header);
        FileTreePane.render(f, app, ly.file_tree);
        BranchListPane.render(f, app, ly.branch_list);
        ReflogPane.render(f, app, ly.reflog);

        match git_detail_for(app.git.focused_pane) {
            GitDetailId::CommitLog => GitLogSelectPane.render(f, app, ly.main_pane),
            GitDetailId::DiffView => DiffViewPane.render(f, app, ly.main_pane),
        }

        status_bar::render_status_bar(f, app, ly.status_bar);

        if app.git.branch_action_menu.is_some() {
            branch_action_menu::render(f, app, area);
        }
    }
}
