use crate::core::app::AppContext;
use anyhow::Result;
use crossterm::event::KeyEvent;
use ratatui::{layout::Rect, Frame};
use std::ffi::OsString;
use std::io;
use std::process::ExitStatus;

#[derive(Debug)]
pub struct ExternalCommand {
    pub program: String,
    pub args: Vec<OsString>,
}

#[derive(Debug)]
pub enum PageAction {
    None,
    Suspend(ExternalCommand),
}

pub trait PageHandler<S>: Sync {
    /// Short label shown in the page tab bar (e.g. "Git", "GitHub").
    fn label(&self) -> &'static str;

    /// Keybinding help entries shown in the help overlay: `(key, description)`.
    fn help_bindings(&self) -> Vec<(&'static str, &'static str)> {
        vec![]
    }

    /// Handle a key event. Returns a `PageAction` indicating what the caller should do.
    fn handle_key(&self, ctx: &mut AppContext, state: &mut S, key: KeyEvent) -> Result<PageAction>;

    /// Render the page (layout, panes, header, status bar, page-specific overlays).
    /// Shared overlays (help, error_dialog) are rendered by the caller after this.
    fn render(&self, f: &mut Frame, ctx: &AppContext, state: &mut S, area: Rect);

    /// Returns true if the page is in a mode that intercepts all key input
    /// (e.g., modal menu, text input). When true, shared keybindings
    /// (Ctrl+c, page switching) are bypassed in favor of the page.
    fn intercepts_all_keys(&self, _ctx: &AppContext, _state: &S) -> bool {
        false
    }

    /// Called on each tick event (active page only).
    fn on_tick(&self, _ctx: &mut AppContext, _state: &mut S) {}

    /// Called when a filesystem change is detected (all pages).
    fn on_fs_change(&self, _ctx: &mut AppContext, _state: &mut S) -> Result<()> {
        Ok(())
    }

    /// Called after returning from a suspended external process.
    fn on_suspend_return(
        &self,
        _ctx: &mut AppContext,
        _state: &mut S,
        _status: io::Result<ExitStatus>,
    ) -> Result<()> {
        Ok(())
    }

    /// Called when this page becomes active (page switching).
    fn on_activate(&self, _ctx: &mut AppContext, _state: &mut S) {}
}
