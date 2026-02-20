pub use crate::git::state::*;
pub use crate::core::search::{SearchMatch, SearchOrigin, SearchState};
use crate::core::view::{View, ViewAction};
use crate::git::container::GitView;
use crate::github::container::GhView;
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

pub struct AppContext {
    pub should_quit: bool,
    pub view_mode: ViewMode,
    pub show_help: bool,
    pub status_message: Option<String>,
    pub error_dialog: Option<ErrorDialogState>,
}

pub struct App {
    pub ctx: AppContext,
    pub git: GitState,
    pub github: GitHubState,
}

impl App {
    pub fn new(cwd: &Path) -> Result<Self> {
        Ok(Self {
            ctx: AppContext {
                should_quit: false,
                view_mode: ViewMode::Git,
                show_help: false,
                status_message: None,
                error_dialog: None,
            },
            git: GitState::new(cwd)?,
            github: GitHubState::new(),
        })
    }

    pub fn workdir(&self) -> &Path {
        self.git.repo.workdir()
    }

    pub fn drain_all_background(&mut self) {
        self.git.drain_bg_highlights();
        self.github.drain_bg_messages();
    }

    pub fn all_views() -> [&'static dyn View; 2] {
        [&GitView, &GhView]
    }

    pub fn refresh_diff(&mut self) -> Result<()> {
        if let Some(msg) = self.git.refresh_diff()? {
            self.ctx.status_message = Some(msg);
        } else {
            self.ctx.status_message = None;
        }
        Ok(())
    }

    pub fn active_view(&self) -> &'static dyn View {
        match self.ctx.view_mode {
            ViewMode::Git => &GitView,
            ViewMode::GitHub => &GhView,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Result<ViewAction> {
        if self.ctx.show_help {
            self.ctx.show_help = false;
            return Ok(ViewAction::None);
        }

        // Error dialog: any key dismisses
        if self.ctx.error_dialog.is_some() {
            self.ctx.error_dialog = None;
            return Ok(ViewAction::None);
        }

        // If the view intercepts all keys (modal menu, search input), delegate immediately
        let view = self.active_view();
        if view.intercepts_all_keys(self) {
            return view.handle_key(self, key);
        }

        // Ctrl+c always quits
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.ctx.should_quit = true;
            return Ok(ViewAction::None);
        }

        // View switching
        match key.code {
            KeyCode::Char('1') => {
                self.ctx.view_mode = ViewMode::Git;
                return Ok(ViewAction::None);
            }
            KeyCode::Char('2') => {
                self.ctx.view_mode = ViewMode::GitHub;
                GhView.on_activate(self);
                return Ok(ViewAction::None);
            }
            _ => {}
        }

        // Delegate to active view
        view.handle_key(self, key)
    }
}
