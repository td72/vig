use crate::core::app::App;
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
pub enum ViewAction {
    None,
    Suspend(ExternalCommand),
}

pub trait View: Sync {
    /// Handle a key event. Returns a `ViewAction` indicating what the caller should do.
    fn handle_key(&self, app: &mut App, key: KeyEvent) -> Result<ViewAction>;

    /// Render the view (layout, panes, header, status bar, view-specific overlays).
    /// Shared overlays (help, error_dialog) are rendered by the caller after this.
    fn render(&self, f: &mut Frame, app: &mut App, area: Rect);

    /// Returns true if the view is in a mode that intercepts all key input
    /// (e.g., modal menu, text input). When true, shared keybindings
    /// (Ctrl+c, view switching) are bypassed in favor of the view.
    fn intercepts_all_keys(&self, _app: &App) -> bool {
        false
    }

    /// Called on each tick event (active view only).
    fn on_tick(&self, _app: &mut App) {}

    /// Called when a filesystem change is detected (all views).
    fn on_fs_change(&self, _app: &mut App) -> Result<()> {
        Ok(())
    }

    /// Called after returning from a suspended external process.
    fn on_suspend_return(&self, _app: &mut App, _status: io::Result<ExitStatus>) -> Result<()> {
        Ok(())
    }

    /// Called when this view becomes active (view switching).
    fn on_activate(&self, _app: &mut App) {}
}
