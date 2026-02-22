use crate::core::app::{AppContext, ErrorDialogState, Page, SearchOrigin};
use crate::core::page::{ExternalCommand, PageAction, PageHandler};
use crate::core::pane::FocusState;
use crate::core::ui::status_bar;
use crate::git::domain::search;
use crate::git::layout;
use crate::git::state::{BranchActionMenuState, DiffViewMode, FocusedPane, GitState, PaneEvent};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{layout::Rect, Frame};
use std::path::{Path, PathBuf};

/// Create a Git page. Returns the page and the resolved workdir path.
pub fn new_page(cwd: &Path) -> Result<(Page, PathBuf)> {
    let git = GitState::new(cwd)?;
    let workdir = git.shared.repo.workdir().to_path_buf();
    let page = Page::new(&GitPageHandler, git);
    Ok((page, workdir))
}

// === Tab navigation ===

const TAB_PANES: [FocusedPane; 3] = [
    FocusedPane::FileTree,
    FocusedPane::BranchList,
    FocusedPane::Reflog,
];

fn tab_index(pane: FocusedPane) -> Option<usize> {
    TAB_PANES.iter().position(|&p| p == pane)
}

fn next_tab_id(focused: FocusedPane) -> FocusedPane {
    match tab_index(focused) {
        Some(idx) => TAB_PANES[(idx + 1) % TAB_PANES.len()],
        None => TAB_PANES[0],
    }
}

fn prev_tab_id(focused: FocusedPane) -> FocusedPane {
    match tab_index(focused) {
        Some(idx) => TAB_PANES[(idx + TAB_PANES.len() - 1) % TAB_PANES.len()],
        None => TAB_PANES[0],
    }
}

// === Dispatch ===

/// Returns true if the focused pane should show the commit log detail.
fn is_commit_log_detail(focused: FocusedPane) -> bool {
    matches!(
        focused,
        FocusedPane::BranchList | FocusedPane::Reflog | FocusedPane::GitLog
    )
}

/// Dispatch a key event to the currently focused Git pane.
fn dispatch_git_key(git: &mut GitState, key: KeyEvent) -> Vec<PaneEvent> {
    let focused = git.shared.focused_pane;

    // Tab pane navigation (h/l/i/Esc) for FileTree, BranchList, Reflog
    if let Some(tab_idx) = tab_index(focused) {
        match key.code {
            KeyCode::Char('h') if tab_idx > 0 => {
                return vec![PaneEvent::SetFocus(TAB_PANES[tab_idx - 1])];
            }
            KeyCode::Char('l') if tab_idx + 1 < TAB_PANES.len() => {
                return vec![PaneEvent::SetFocus(TAB_PANES[tab_idx + 1])];
            }
            KeyCode::Char('i') => {
                let target = if is_commit_log_detail(focused) {
                    FocusedPane::GitLog
                } else {
                    FocusedPane::DiffView
                };
                return vec![PaneEvent::SetFocus(target)];
            }
            KeyCode::Esc if git.shared.search.query.is_some() => {
                return vec![PaneEvent::ClearSearch];
            }
            _ => {}
        }
    }

    // Delegate to pane
    match focused {
        FocusedPane::FileTree => git.file_tree.handle_key(&git.shared, key),
        FocusedPane::BranchList => git.branch_list.handle_key(&git.shared, key),
        FocusedPane::Reflog => git.reflog.handle_key(&git.shared, key),
        FocusedPane::GitLog => git.git_log.handle_key(&git.shared, key),
        FocusedPane::DiffView => {
            let file = git.file_tree.selected_file(&git.shared).cloned();
            git.diff_view.handle_key(&git.shared, file.as_ref(), key)
        }
    }
}

// === Event processing ===

fn process_events(
    ctx: &mut AppContext,
    git: &mut GitState,
    events: Vec<PaneEvent>,
) -> Result<PageAction> {
    let mut action = PageAction::None;
    for event in events {
        match event {
            PaneEvent::SetFocus(pane) => {
                git.set_focus(pane);
            }
            PaneEvent::ResetDiffScroll => {
                git.diff_view.scroll.y = 0;
                git.diff_view.scroll.x = 0;
            }
            PaneEvent::RefreshDiff => {
                refresh_diff(ctx, git);
            }
            PaneEvent::SetDiffBase(base) => {
                git.shared.diff_base_ref = base;
            }
            PaneEvent::OpenBranchActionMenu => {
                if let Some(branch) = git.branch_list.branches.get(git.branch_list.selected_idx) {
                    git.branch_list.action_menu = Some(BranchActionMenuState {
                        branch_name: branch.name.clone(),
                        is_head: branch.is_head,
                        selected_idx: 0,
                    });
                }
            }
            PaneEvent::SwitchBranch(name) => match git.shared.repo.switch_branch(&name) {
                Ok(()) => {
                    ctx.status_message = Some(format!("Switched to {name}"));
                    git.load_branches();
                    refresh_diff(ctx, git);
                }
                Err(e) => {
                    ctx.error_dialog = Some(ErrorDialogState {
                        title: "Switch failed".to_string(),
                        message: format!("{e}"),
                    });
                }
            },
            PaneEvent::DeleteBranch(name) => match git.shared.repo.delete_branch(&name) {
                Ok(()) => {
                    ctx.status_message = Some(format!("Deleted {name}"));
                    git.load_branches();
                }
                Err(e) => {
                    ctx.error_dialog = Some(ErrorDialogState {
                        title: "Delete failed".to_string(),
                        message: format!("{e}"),
                    });
                }
            },
            PaneEvent::UpdateBranchLog => {
                git.update_branch_log();
            }
            PaneEvent::LoadCommitDetail => {
                git.load_commit_detail();
            }
            PaneEvent::ReSearchOnFileChange => {
                search::re_search_on_file_change(git);
            }
            PaneEvent::StartSearch(origin) => {
                git.shared.search.start(origin);
            }
            PaneEvent::ClearSearch => {
                git.shared.search.clear();
            }
            PaneEvent::JumpToMatch(forward) => {
                search::jump_to_git_match(ctx, git, forward);
            }
            PaneEvent::OpenEditor(path) => {
                let file_path = ctx.workdir.join(&path);
                let editor = std::env::var("EDITOR")
                    .or_else(|_| std::env::var("VISUAL"))
                    .unwrap_or_else(|_| "vi".to_string());
                action = PageAction::Suspend(ExternalCommand {
                    program: editor,
                    args: vec![file_path.into()],
                });
            }
            PaneEvent::Quit => {
                ctx.should_quit = true;
            }
            PaneEvent::ShowHelp => {
                ctx.show_help = true;
            }
            PaneEvent::StatusMessage(msg) => {
                ctx.status_message = Some(msg);
            }
            PaneEvent::ErrorDialog { title, message } => {
                ctx.error_dialog = Some(ErrorDialogState { title, message });
            }
            PaneEvent::CopyToClipboard(text) => {
                copy_to_clipboard(ctx, &text);
            }
            PaneEvent::OpenUrl(url) => {
                if let Err(e) = crate::github::domain::client::open_url(&url) {
                    ctx.status_message = Some(e);
                }
            }
        }
    }
    Ok(action)
}

fn copy_to_clipboard(ctx: &mut AppContext, text: &str) {
    if text.is_empty() {
        return;
    }
    let line_count = text.lines().count().max(1);
    match arboard::Clipboard::new() {
        Ok(mut clip) => {
            if clip.set_text(text).is_ok() {
                ctx.status_message = Some(format!(
                    "Yanked {line_count} line{}",
                    if line_count == 1 { "" } else { "s" }
                ));
            } else {
                ctx.status_message = Some("Clipboard error".to_string());
            }
        }
        Err(_) => {
            ctx.status_message = Some("Clipboard unavailable".to_string());
        }
    }
}

// === View-level key handling ===

pub fn handle_git_view_key(
    ctx: &mut AppContext,
    git: &mut GitState,
    key: KeyEvent,
) -> Result<PageAction> {
    // In Normal/Visual modes, keys are handled by the mode handler exclusively
    if git.shared.focused_pane == FocusedPane::DiffView
        && git.diff_view.vim.mode != DiffViewMode::Scroll
    {
        let events = dispatch_git_key(git, key);
        return process_events(ctx, git, events);
    }

    match key.code {
        KeyCode::Char('q') => {
            ctx.should_quit = true;
        }
        KeyCode::Char('?') => {
            ctx.show_help = true;
        }
        KeyCode::Char('/') => {
            git.shared
                .search
                .start(search_origin_for(git.shared.focused_pane));
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
                return Ok(PageAction::Suspend(ExternalCommand {
                    program: editor,
                    args: vec![file_path.into()],
                }));
            }
        }
        KeyCode::Tab => {
            git.set_focus(next_tab_id(git.shared.focused_pane));
        }
        KeyCode::BackTab => {
            git.set_focus(prev_tab_id(git.shared.focused_pane));
        }
        _ => {
            let events = dispatch_git_key(git, key);
            return process_events(ctx, git, events);
        }
    }
    Ok(PageAction::None)
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
    if let Some(msg) = git
        .refresh_diff()
        .unwrap_or_else(|e| Some(format!("Diff error: {e}")))
    {
        ctx.status_message = Some(msg);
    } else {
        ctx.status_message = None;
    }
}

// === View ===

pub struct GitPageHandler;

impl PageHandler<GitState> for GitPageHandler {
    fn label(&self) -> &'static str {
        "Git"
    }

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

    fn intercepts_all_keys(&self, _ctx: &AppContext, git: &GitState) -> bool {
        git.branch_list.action_menu.is_some()
    }

    fn handle_key(
        &self,
        ctx: &mut AppContext,
        git: &mut GitState,
        key: KeyEvent,
    ) -> Result<PageAction> {
        // Branch action menu intercepts all keys when open
        if git.branch_list.action_menu.is_some() {
            let events = git.branch_list.handle_key(&git.shared, key);
            return process_events(ctx, git, events);
        }

        // Search input mode intercepts all keys
        if git.shared.search.active {
            if git.shared.search.handle_input_key(key) {
                search::execute_git_search(git);
                search::jump_to_git_match(ctx, git, true);
            }
            return Ok(PageAction::None);
        }

        handle_git_view_key(ctx, git, key)
    }

    fn render(&self, f: &mut Frame, ctx: &AppContext, git: &mut GitState, area: Rect) {
        let ly = layout::compute_layout(area);
        status_bar::render_header(f, ctx, git, ly.header);
        git.file_tree.render(f, ctx, &git.shared, ly.file_tree);
        git.branch_list.render(f, ctx, &git.shared, ly.branch_list);
        git.reflog.render(f, ctx, &git.shared, ly.reflog);

        if is_commit_log_detail(git.shared.focused_pane) {
            git.git_log.render(f, ctx, &git.shared, ly.main_pane);
        } else {
            let file = git.file_tree.selected_file(&git.shared).cloned();
            git.diff_view
                .render(f, ctx, &git.shared, file.as_ref(), ly.main_pane);
        }

        status_bar::render_status_bar(f, ctx, git, ly.status_bar);

        if git.branch_list.action_menu.is_some() {
            git.branch_list.render_action_menu(f, area);
        }
    }

    fn on_fs_change(&self, ctx: &mut AppContext, git: &mut GitState) -> Result<()> {
        git.load_branches();
        git.load_reflog();
        refresh_diff(ctx, git);
        Ok(())
    }

    fn on_suspend_return(
        &self,
        ctx: &mut AppContext,
        git: &mut GitState,
        status: std::io::Result<std::process::ExitStatus>,
    ) -> Result<()> {
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
