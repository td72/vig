use crate::core::app::{App, ErrorDialogState};
use crate::git::state::{BranchAction, BranchActionMenuState};
use crossterm::event::{KeyCode, KeyEvent};

impl App {
    fn select_branch(&mut self) {
        if let Some(branch) = self
            .git
            .branch_list
            .branches
            .get(self.git.branch_list.selected_idx)
        {
            if branch.is_head {
                self.git.diff_base_ref = None;
            } else {
                self.git.diff_base_ref = Some(branch.name.clone());
            }
            if let Err(e) = self.refresh_diff() {
                self.ctx.status_message = Some(format!("Diff error: {e}"));
            }
        }
    }

    pub(crate) fn open_branch_action_menu(&mut self) {
        if let Some(branch) = self.git.branch_list.branches.get(self.git.branch_list.selected_idx) {
            self.git.branch_action_menu = Some(BranchActionMenuState {
                branch_name: branch.name.clone(),
                is_head: branch.is_head,
                selected_idx: 0,
            });
        }
    }

    pub(crate) fn handle_branch_action_menu_key(&mut self, key: KeyEvent) {
        let menu = match self.git.branch_action_menu.as_mut() {
            Some(m) => m,
            None => return,
        };

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.git.branch_action_menu = None;
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
                self.execute_branch_action(action);
            }
            KeyCode::Char('s') => {
                self.execute_branch_action(BranchAction::Switch);
            }
            KeyCode::Char('d') => {
                self.execute_branch_action(BranchAction::Delete);
            }
            KeyCode::Char('b') => {
                self.execute_branch_action(BranchAction::DiffBase);
            }
            _ => {}
        }
    }

    fn execute_branch_action(&mut self, action: BranchAction) {
        let menu = match self.git.branch_action_menu.take() {
            Some(m) => m,
            None => return,
        };

        match action {
            BranchAction::Switch => {
                if menu.is_head {
                    self.ctx.status_message = Some("Already on this branch".to_string());
                    return;
                }
                match self.git.repo.switch_branch(&menu.branch_name) {
                    Ok(()) => {
                        self.ctx.status_message =
                            Some(format!("Switched to {}", menu.branch_name));
                        self.git.load_branches();
                        if let Err(e) = self.refresh_diff() {
                            self.ctx.status_message = Some(format!("Diff error: {e}"));
                        }
                    }
                    Err(e) => {
                        self.ctx.error_dialog = Some(ErrorDialogState {
                            title: "Switch failed".to_string(),
                            message: format!("{e}"),
                        });
                    }
                }
            }
            BranchAction::Delete => {
                if menu.is_head {
                    self.ctx.status_message =
                        Some("Cannot delete the current branch".to_string());
                    return;
                }
                match self.git.repo.delete_branch(&menu.branch_name) {
                    Ok(()) => {
                        self.ctx.status_message =
                            Some(format!("Deleted {}", menu.branch_name));
                        self.git.load_branches();
                    }
                    Err(e) => {
                        self.ctx.error_dialog = Some(ErrorDialogState {
                            title: "Delete failed".to_string(),
                            message: format!("{e}"),
                        });
                    }
                }
            }
            BranchAction::DiffBase => {
                self.select_branch();
            }
        }
    }
}
