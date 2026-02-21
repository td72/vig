pub use crate::git::state::*;
pub use crate::core::search::{SearchMatch, SearchOrigin, SearchState};
use crate::core::view::{View, ViewAction};
use crate::git::container::GitView;
use crate::github::container::GhView;
use crate::github::state::GitHubState;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::any::Any;
use std::path::{Path, PathBuf};

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
    pub workdir: PathBuf,
}

pub trait ViewState {
    fn drain_background(&mut self);
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

struct ViewEntry {
    view: &'static dyn View,
    state: Box<dyn ViewState>,
}

pub struct App {
    pub ctx: AppContext,
    entries: Vec<ViewEntry>,
}

impl App {
    pub fn new(cwd: &Path) -> Result<Self> {
        let git = GitState::new(cwd)?;
        let workdir = git.repo.workdir().to_path_buf();
        let github = GitHubState::new();

        Ok(Self {
            ctx: AppContext {
                should_quit: false,
                view_mode: ViewMode::Git,
                show_help: false,
                status_message: None,
                error_dialog: None,
                workdir,
            },
            entries: vec![
                ViewEntry { view: &GitView, state: Box::new(git) },
                ViewEntry { view: &GhView, state: Box::new(github) },
            ],
        })
    }

    pub fn workdir(&self) -> &Path {
        &self.ctx.workdir
    }

    pub fn drain_all_background(&mut self) {
        for entry in &mut self.entries {
            entry.state.drain_background();
        }
    }

    pub fn render(&mut self, f: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let idx = self.view_index();
        let view = self.entries[idx].view;
        let state = self.entries[idx].state.as_any_mut();
        view.render(f, &self.ctx, state, area);
    }

    pub fn on_fs_change(&mut self) -> Result<()> {
        for entry in &mut self.entries {
            let view = entry.view;
            let state = entry.state.as_any_mut();
            view.on_fs_change(&mut self.ctx, state)?;
        }
        Ok(())
    }

    pub fn on_tick(&mut self) {
        let idx = self.view_index();
        let view = self.entries[idx].view;
        let state = self.entries[idx].state.as_any_mut();
        view.on_tick(&mut self.ctx, state);
    }

    pub fn on_suspend_return(&mut self, status: std::io::Result<std::process::ExitStatus>) -> Result<()> {
        let idx = self.view_index();
        let view = self.entries[idx].view;
        let state = self.entries[idx].state.as_any_mut();
        view.on_suspend_return(&mut self.ctx, state, status)
    }

    fn view_index(&self) -> usize {
        match self.ctx.view_mode {
            ViewMode::Git => 0,
            ViewMode::GitHub => 1,
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

        let idx = self.view_index();

        // If the view intercepts all keys (modal menu, search input), delegate immediately
        let view = self.entries[idx].view;
        if view.intercepts_all_keys(&self.ctx, self.entries[idx].state.as_any()) {
            let state = self.entries[idx].state.as_any_mut();
            return view.handle_key(&mut self.ctx, state, key);
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
                let new_idx = self.view_index();
                let view = self.entries[new_idx].view;
                let state = self.entries[new_idx].state.as_any_mut();
                view.on_activate(&mut self.ctx, state);
                return Ok(ViewAction::None);
            }
            _ => {}
        }

        // Delegate to active view
        let state = self.entries[idx].state.as_any_mut();
        view.handle_key(&mut self.ctx, state, key)
    }
}
