use crate::core::app::AppContext;
use anyhow::Result;
use crossterm::event::KeyEvent;
use ratatui::{layout::Rect, Frame};
use std::any::Any;
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
    /// Short label shown in the view tab bar (e.g. "Git", "GitHub").
    fn label(&self) -> &'static str;

    /// Keybinding help entries shown in the help overlay: `(key, description)`.
    fn help_bindings(&self) -> Vec<(&'static str, &'static str)> {
        vec![]
    }

    /// Handle a key event. Returns a `ViewAction` indicating what the caller should do.
    fn handle_key(&self, ctx: &mut AppContext, state: &mut dyn Any, key: KeyEvent) -> Result<ViewAction>;

    /// Render the view (layout, panes, header, status bar, view-specific overlays).
    /// Shared overlays (help, error_dialog) are rendered by the caller after this.
    fn render(&self, f: &mut Frame, ctx: &AppContext, state: &mut dyn Any, area: Rect);

    /// Returns true if the view is in a mode that intercepts all key input
    /// (e.g., modal menu, text input). When true, shared keybindings
    /// (Ctrl+c, view switching) are bypassed in favor of the view.
    fn intercepts_all_keys(&self, _ctx: &AppContext, _state: &dyn Any) -> bool {
        false
    }

    /// Called on each tick event (active view only).
    fn on_tick(&self, _ctx: &mut AppContext, _state: &mut dyn Any) {}

    /// Called when a filesystem change is detected (all views).
    fn on_fs_change(&self, _ctx: &mut AppContext, _state: &mut dyn Any) -> Result<()> {
        Ok(())
    }

    /// Called after returning from a suspended external process.
    fn on_suspend_return(&self, _ctx: &mut AppContext, _state: &mut dyn Any, _status: io::Result<ExitStatus>) -> Result<()> {
        Ok(())
    }

    /// Called when this view becomes active (view switching).
    fn on_activate(&self, _ctx: &mut AppContext, _state: &mut dyn Any) {}
}
