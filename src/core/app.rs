pub use crate::git::state::*;
pub use crate::core::search::{SearchMatch, SearchOrigin, SearchState};
use crate::core::view::{View, ViewAction};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::any::Any;
use std::path::PathBuf;

pub struct ErrorDialogState {
    pub title: String,
    pub message: String,
}

pub struct AppContext {
    pub should_quit: bool,
    pub active_view: usize,
    pub view_labels: Vec<&'static str>,
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

pub struct ViewEntry {
    pub view: &'static dyn View,
    pub state: Box<dyn ViewState>,
}

pub struct App {
    pub ctx: AppContext,
    entries: Vec<ViewEntry>,
}

impl App {
    pub fn new(ctx: AppContext, entries: Vec<ViewEntry>) -> Self {
        Self { ctx, entries }
    }

    pub fn drain_all_background(&mut self) {
        for entry in &mut self.entries {
            entry.state.drain_background();
        }
    }

    pub fn render(&mut self, f: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let idx = self.ctx.active_view;
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
        let idx = self.ctx.active_view;
        let view = self.entries[idx].view;
        let state = self.entries[idx].state.as_any_mut();
        view.on_tick(&mut self.ctx, state);
    }

    pub fn on_suspend_return(&mut self, status: std::io::Result<std::process::ExitStatus>) -> Result<()> {
        let idx = self.ctx.active_view;
        let view = self.entries[idx].view;
        let state = self.entries[idx].state.as_any_mut();
        view.on_suspend_return(&mut self.ctx, state, status)
    }

    pub fn active_help_bindings(&self) -> Vec<(&'static str, &'static str)> {
        self.entries[self.ctx.active_view].view.help_bindings()
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

        let idx = self.ctx.active_view;

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

        // View switching: '1'..'9' maps to view index 0..8
        if let KeyCode::Char(c @ '1'..='9') = key.code {
            let new_idx = (c as usize) - ('1' as usize);
            if new_idx < self.entries.len() && new_idx != idx {
                self.ctx.active_view = new_idx;
                let view = self.entries[new_idx].view;
                let state = self.entries[new_idx].state.as_any_mut();
                view.on_activate(&mut self.ctx, state);
            }
            return Ok(ViewAction::None);
        }

        // Delegate to active view
        let state = self.entries[idx].state.as_any_mut();
        view.handle_key(&mut self.ctx, state, key)
    }
}
