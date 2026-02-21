use crate::core::app::{AppContext, SearchOrigin, ViewEntry};
use crate::core::container::PaneContainer;
use crate::core::pane::{DetailPane, SelectPane};
use crate::core::ui::{branch_action_menu, status_bar};
use crate::core::view::{ExternalCommand, View, ViewAction};
use crate::git::branch_action;
use crate::git::layout;
use crate::git::panes::{
    BranchListPane, DiffViewPane, FileTreePane, GitLogSelectPane, ReflogPane,
};
use crate::git::search;
use crate::git::state::{DiffViewMode, FocusedPane, GitState};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{layout::Rect, Frame};
use std::any::Any;
use std::path::{Path, PathBuf};

/// Create a Git view entry. Returns the entry and the resolved workdir path.
pub fn new_entry(cwd: &Path) -> Result<(ViewEntry, PathBuf)> {
    let git = GitState::new(cwd)?;
    let workdir = git.repo.workdir().to_path_buf();
    let entry = ViewEntry { view: &GitView, state: Box::new(git) };
    Ok((entry, workdir))
}

// === Domain types ===

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitDetailId {
    DiffView,
    CommitLog,
}

pub struct GitPaneGroup {
    pub select: &'static dyn SelectPane<GitState>,
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
pub fn dispatch_git_key(ctx: &mut AppContext, git: &mut GitState, key: KeyEvent) {
    match git.focused_pane {
        FocusedPane::GitLog => GitLogSelectPane.handle_key(ctx, git, key),
        FocusedPane::DiffView => DiffViewPane.handle_key(ctx, git, key),
        _ => GitContainer.dispatch(ctx, git, key),
    }
}

// === View-level key handling ===

pub fn handle_git_view_key(ctx: &mut AppContext, git: &mut GitState, key: KeyEvent) -> Result<ViewAction> {
    // In Normal/Visual modes, keys are handled by the mode handler exclusively
    if git.focused_pane == FocusedPane::DiffView && git.diff_view_mode != DiffViewMode::Scroll {
        dispatch_git_key(ctx, git, key);
        return Ok(ViewAction::None);
    }

    match key.code {
        KeyCode::Char('q') => {
            ctx.should_quit = true;
        }
        KeyCode::Char('?') => {
            ctx.show_help = true;
        }
        KeyCode::Char('/') => {
            git.search.start(search_origin_for(git.focused_pane));
        }
        KeyCode::Char('n') => {
            search::jump_to_git_match(ctx, git, true);
        }
        KeyCode::Char('N') => {
            search::jump_to_git_match(ctx, git, false);
        }
        KeyCode::Char('r') => {
            refresh_diff(ctx, git);
            git.load_branches();
            git.load_reflog();
        }
        KeyCode::Char('e') => {
            if let Some(file) = git.selected_file() {
                let file_path = ctx.workdir.join(&file.path);
                let editor = std::env::var("EDITOR")
                    .or_else(|_| std::env::var("VISUAL"))
                    .unwrap_or_else(|_| "vi".to_string());
                return Ok(ViewAction::Suspend(ExternalCommand {
                    program: editor,
                    args: vec![file_path.into()],
                }));
            }
        }
        KeyCode::Tab => {
            git.set_focus(next_git_tab(git.focused_pane));
        }
        KeyCode::BackTab => {
            git.set_focus(prev_git_tab(git.focused_pane));
        }
        _ => dispatch_git_key(ctx, git, key),
    }
    Ok(ViewAction::None)
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

pub(crate) fn refresh_diff(ctx: &mut AppContext, git: &mut GitState) {
    if let Some(msg) = git.refresh_diff().unwrap_or_else(|e| Some(format!("Diff error: {e}"))) {
        ctx.status_message = Some(msg);
    } else {
        ctx.status_message = None;
    }
}

// === Container ===

pub(crate) struct GitContainer;

impl PaneContainer<GitState> for GitContainer {
    fn current_index(&self, state: &GitState) -> Option<usize> {
        GIT_GROUPS.iter().position(|g| g.id == state.focused_pane)
    }
    fn focus_index(&self, _ctx: &mut AppContext, state: &mut GitState, idx: usize) {
        state.set_focus(GIT_GROUPS[idx].id);
    }
    fn pane_at(&self, idx: usize) -> &'static dyn SelectPane<GitState> {
        GIT_GROUPS[idx].select
    }
    fn len(&self) -> usize {
        GIT_GROUPS.len()
    }

    fn handle_common_key(&self, _ctx: &mut AppContext, state: &mut GitState, key: KeyEvent, idx: usize) -> bool {
        match key.code {
            KeyCode::Char('i') => {
                let target = match GIT_GROUPS[idx].detail {
                    GitDetailId::DiffView => FocusedPane::DiffView,
                    GitDetailId::CommitLog => FocusedPane::GitLog,
                };
                state.set_focus(target);
                true
            }
            KeyCode::Esc if state.search.query.is_some() => {
                state.search.clear();
                true
            }
            _ => false,
        }
    }
}

// === View ===

pub struct GitView;

impl View for GitView {
    fn label(&self) -> &'static str { "Git" }

    fn help_bindings(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            ("1 / 2", "Switch view"),
            ("j / ↓", "Next item / Scroll down"),
            ("k / ↑", "Prev item / Scroll up"),
            ("Enter", "Select file/branch"),
            ("Tab", "Next pane"),
            ("Shift+Tab", "Prev pane"),
            ("Ctrl+d", "Half page down"),
            ("Ctrl+u", "Half page up"),
            ("g / G", "Top / Bottom"),
            ("h / l", "Scroll left / right"),
            ("i", "Normal mode (cursor)"),
            ("v / V", "Visual / Visual Line"),
            ("y", "Yank (copy) selection"),
            ("/", "Search"),
            ("n / N", "Next / Prev match"),
            ("Esc", "Clear search / Back"),
            ("e", "Open in $EDITOR"),
            ("r", "Refresh diff + branches"),
            ("?", "Toggle help"),
            ("q", "Quit"),
            ("", ""),
            ("", "── Branch List ──"),
            ("/", "Search branches"),
            ("Enter", "Action menu"),
            ("", ""),
            ("", "── Git Log ──"),
            ("j / k", "Navigate commits"),
            ("Ctrl+d/u", "Half page scroll"),
            ("g / G", "Top / Bottom"),
            ("y", "Copy commit hash"),
            ("o", "Open in GitHub"),
            ("/", "Search commits"),
            ("", ""),
            ("", "── Reflog ──"),
            ("j / k", "Navigate entries"),
            ("Ctrl+d/u", "Half page scroll"),
            ("g / G", "Top / Bottom"),
            ("Enter", "Set as diff base"),
            ("/", "Search reflog"),
        ]
    }

    fn intercepts_all_keys(&self, _ctx: &AppContext, state: &dyn Any) -> bool {
        let git = state.downcast_ref::<GitState>().unwrap();
        git.branch_action_menu.is_some() || git.search.active
    }

    fn handle_key(&self, ctx: &mut AppContext, state: &mut dyn Any, key: KeyEvent) -> Result<ViewAction> {
        let git = state.downcast_mut::<GitState>().unwrap();

        // Branch action menu intercepts all keys when open
        if git.branch_action_menu.is_some() {
            branch_action::handle_branch_action_menu_key(ctx, git, key);
            return Ok(ViewAction::None);
        }

        // Search input mode intercepts all keys
        if git.search.active {
            if git.search.handle_input_key(key) {
                search::execute_git_search(git);
                search::jump_to_git_match(ctx, git, true);
            }
            return Ok(ViewAction::None);
        }

        handle_git_view_key(ctx, git, key)
    }

    fn render(&self, f: &mut Frame, ctx: &AppContext, state: &mut dyn Any, area: Rect) {
        let git = state.downcast_mut::<GitState>().unwrap();
        let ly = layout::compute_layout(area);
        status_bar::render_header(f, ctx, git, ly.header);
        FileTreePane.render(f, ctx, git, ly.file_tree);
        BranchListPane.render(f, ctx, git, ly.branch_list);
        ReflogPane.render(f, ctx, git, ly.reflog);

        match git_detail_for(git.focused_pane) {
            GitDetailId::CommitLog => GitLogSelectPane.render(f, ctx, git, ly.main_pane),
            GitDetailId::DiffView => DiffViewPane.render(f, ctx, git, ly.main_pane),
        }

        status_bar::render_status_bar(f, ctx, git, ly.status_bar);

        if git.branch_action_menu.is_some() {
            branch_action_menu::render(f, git, area);
        }
    }

    fn on_fs_change(&self, ctx: &mut AppContext, state: &mut dyn Any) -> Result<()> {
        let git = state.downcast_mut::<GitState>().unwrap();
        git.load_branches();
        git.load_reflog();
        refresh_diff(ctx, git);
        Ok(())
    }

    fn on_suspend_return(
        &self,
        ctx: &mut AppContext,
        state: &mut dyn Any,
        status: std::io::Result<std::process::ExitStatus>,
    ) -> Result<()> {
        let git = state.downcast_mut::<GitState>().unwrap();
        match status {
            Ok(s) if s.success() => {
                refresh_diff(ctx, git);
                Ok(())
            }
            Ok(s) => {
                ctx.status_message = Some(format!("Editor exited with: {s}"));
                Ok(())
            }
            Err(e) => {
                ctx.status_message = Some(format!("Failed to open editor: {e}"));
                Ok(())
            }
        }
    }
}
