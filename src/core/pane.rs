use crate::core::app::App;
use crossterm::event::KeyEvent;
use ratatui::{layout::Rect, Frame};

pub trait SelectPane: Sync {
    fn handle_key(&self, app: &mut App, key: KeyEvent);
    fn render(&self, f: &mut Frame, app: &mut App, area: Rect);
}

pub trait DetailPane: Sync {
    fn handle_key(&self, app: &mut App, key: KeyEvent);
    fn render(&self, f: &mut Frame, app: &mut App, area: Rect);
}
