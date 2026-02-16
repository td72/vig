pub use crate::git::state::*;
pub use crate::core::search::{SearchMatch, SearchOrigin, SearchState};
use crate::github::state::GitHubState;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Git,
    GitHub,
}

pub struct ErrorDialogState {
    pub title: String,
    pub message: String,
}

pub struct App {
    pub should_quit: bool,
    pub view_mode: ViewMode,
    pub show_help: bool,
    pub status_message: Option<String>,
    pub error_dialog: Option<ErrorDialogState>,
    pub git: GitState,
    pub github: GitHubState,
}

impl App {
    pub fn new(cwd: &Path) -> Result<Self> {
        Ok(Self {
            should_quit: false,
            view_mode: ViewMode::Git,
            show_help: false,
            status_message: None,
            error_dialog: None,
            git: GitState::new(cwd)?,
            github: GitHubState::new(),
        })
    }

    pub fn active_search(&self) -> Option<&SearchState> {
        match self.view_mode {
            ViewMode::Git => Some(&self.git.search),
            ViewMode::GitHub => None,
        }
    }

    pub fn refresh_diff(&mut self) -> Result<()> {
        if let Some(msg) = self.git.refresh_diff()? {
            self.status_message = Some(msg);
        } else {
            self.status_message = None;
        }
        Ok(())
    }

    /// Refresh git state after a successful external editor session.
    pub fn post_edit_refresh(&mut self) -> Result<()> {
        self.refresh_diff()
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Result<bool> {
        if self.show_help {
            self.show_help = false;
            return Ok(false);
        }

        // Error dialog: any key dismisses
        if self.error_dialog.is_some() {
            self.error_dialog = None;
            return Ok(false);
        }

        // Action menu intercepts all keys when open
        if self.git.branch_action_menu.is_some() {
            self.handle_branch_action_menu_key(key);
            return Ok(false);
        }

        // Search input mode intercepts all keys
        match self.view_mode {
            ViewMode::Git if self.git.search.active => {
                if self.git.search.handle_input_key(key) {
                    self.execute_git_search();
                    self.jump_to_git_match(true);
                }
                return Ok(false);
            }
            _ => {}
        }

        // Ctrl+c always quits
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return Ok(false);
        }

        // View switching
        match key.code {
            KeyCode::Char('1') => {
                self.view_mode = ViewMode::Git;
                return Ok(false);
            }
            KeyCode::Char('2') => {
                self.view_mode = ViewMode::GitHub;
                self.github.initialize();
                return Ok(false);
            }
            _ => {}
        }

        // Delegate to domain container
        match self.view_mode {
            ViewMode::Git => crate::git::container::handle_git_view_key(self, key),
            ViewMode::GitHub => crate::github::container::handle_gh_view_key(self, key),
        }
    }
}
