use crate::core::app::App;
use anyhow::Result;
use crossterm::event::KeyEvent;
use ratatui::{layout::Rect, Frame};

pub trait View: Sync {
    /// Handle a key event. Returns true if the caller should open an external editor.
    fn handle_key(&self, app: &mut App, key: KeyEvent) -> Result<bool>;

    /// Render the view (layout, panes, header, status bar, view-specific overlays).
    /// Shared overlays (help, error_dialog) are rendered by the caller after this.
    fn render(&self, f: &mut Frame, app: &mut App, area: Rect);

    /// Returns true if the view is in a mode that intercepts all key input
    /// (e.g., modal menu, text input). When true, shared keybindings
    /// (Ctrl+c, view switching) are bypassed in favor of the view.
    fn intercepts_all_keys(&self, _app: &App) -> bool {
        false
    }
}
