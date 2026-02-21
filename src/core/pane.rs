use crate::core::app::AppContext;
use crossterm::event::KeyEvent;
use ratatui::{layout::Rect, Frame};

pub trait SelectPane<S>: Sync {
    fn handle_key(&self, ctx: &mut AppContext, state: &mut S, key: KeyEvent);
    fn render(&self, f: &mut Frame, ctx: &AppContext, state: &mut S, area: Rect);
}

pub trait DetailPane<S>: Sync {
    fn handle_key(&self, ctx: &mut AppContext, state: &mut S, key: KeyEvent);
    fn render(&self, f: &mut Frame, ctx: &AppContext, state: &mut S, area: Rect);
}
