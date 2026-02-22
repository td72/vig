use crate::core::app::{AppContext, ErrorDialogState};
use crate::git::state::{BranchAction, BranchActionMenuState, GitState};
use crossterm::event::{KeyCode, KeyEvent};

fn refresh_diff(ctx: &mut AppContext, git: &mut GitState) {
    if let Some(msg) = git
        .refresh_diff()
        .unwrap_or_else(|e| Some(format!("Diff error: {e}")))
    {
        ctx.status_message = Some(msg);
    } else {
        ctx.status_message = None;
    }
}

fn select_branch(ctx: &mut AppContext, git: &mut GitState) {
    if let Some(branch) = git.branch_list.branches.get(git.branch_list.selected_idx) {
        if branch.is_head {
            git.diff_base_ref = None;
        } else {
            git.diff_base_ref = Some(branch.name.clone());
        }
        refresh_diff(ctx, git);
    }
}

pub(crate) fn open_branch_action_menu(git: &mut GitState) {
    if let Some(branch) = git.branch_list.branches.get(git.branch_list.selected_idx) {
        git.branch_list.action_menu = Some(BranchActionMenuState {
            branch_name: branch.name.clone(),
            is_head: branch.is_head,
            selected_idx: 0,
        });
    }
}

pub(crate) fn handle_branch_action_menu_key(
    ctx: &mut AppContext,
    git: &mut GitState,
    key: KeyEvent,
) {
    let menu = match git.branch_list.action_menu.as_mut() {
        Some(m) => m,
        None => return,
    };

    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            git.branch_list.action_menu = None;
        }
        KeyCode::Char('j') | KeyCode::Down => {
            if menu.selected_idx + 1 < BranchAction::ALL.len() {
                menu.selected_idx += 1;
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if menu.selected_idx > 0 {
                menu.selected_idx -= 1;
            }
        }
        KeyCode::Enter => {
            let action = BranchAction::ALL[menu.selected_idx];
            execute_branch_action(ctx, git, action);
        }
        KeyCode::Char('s') => {
            execute_branch_action(ctx, git, BranchAction::Switch);
        }
        KeyCode::Char('d') => {
            execute_branch_action(ctx, git, BranchAction::Delete);
        }
        KeyCode::Char('b') => {
            execute_branch_action(ctx, git, BranchAction::DiffBase);
        }
        _ => {}
    }
}

fn execute_branch_action(ctx: &mut AppContext, git: &mut GitState, action: BranchAction) {
    let menu = match git.branch_list.action_menu.take() {
        Some(m) => m,
        None => return,
    };

    match action {
        BranchAction::Switch => {
            if menu.is_head {
                ctx.status_message = Some("Already on this branch".to_string());
                return;
            }
            match git.repo.switch_branch(&menu.branch_name) {
                Ok(()) => {
                    ctx.status_message = Some(format!("Switched to {}", menu.branch_name));
                    git.load_branches();
                    refresh_diff(ctx, git);
                }
                Err(e) => {
                    ctx.error_dialog = Some(ErrorDialogState {
                        title: "Switch failed".to_string(),
                        message: format!("{e}"),
                    });
                }
            }
        }
        BranchAction::Delete => {
            if menu.is_head {
                ctx.status_message = Some("Cannot delete the current branch".to_string());
                return;
            }
            match git.repo.delete_branch(&menu.branch_name) {
                Ok(()) => {
                    ctx.status_message = Some(format!("Deleted {}", menu.branch_name));
                    git.load_branches();
                }
                Err(e) => {
                    ctx.error_dialog = Some(ErrorDialogState {
                        title: "Delete failed".to_string(),
                        message: format!("{e}"),
                    });
                }
            }
        }
        BranchAction::DiffBase => {
            select_branch(ctx, git);
        }
    }
}
