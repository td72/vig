mod git;
pub(crate) mod github;

pub use git::*;
pub use github::*;

use crate::app::App;
use crossterm::event::KeyEvent;
use ratatui::{layout::Rect, Frame};

// === Traits ===

pub trait SelectPane: Sync {
    fn handle_key(&self, app: &mut App, key: KeyEvent);
    fn render(&self, f: &mut Frame, app: &mut App, area: Rect);
}

pub trait DetailPane: Sync {
    fn handle_key(&self, app: &mut App, key: KeyEvent);
    fn render(&self, f: &mut Frame, app: &mut App, area: Rect);
}
