use crate::core::app::AppContext;
use crossterm::event::KeyEvent;
use ratatui::{layout::Rect, Frame};

pub trait FocusState {
    type PaneId: Copy + PartialEq + 'static;
    fn focused_pane(&self) -> Self::PaneId;
    fn set_focus(&mut self, id: Self::PaneId);
}

pub trait SelectPane<S>: Sync {
    fn handle_key(&self, ctx: &mut AppContext, state: &mut S, key: KeyEvent);
    fn render(&self, f: &mut Frame, ctx: &AppContext, state: &mut S, area: Rect);
}

pub trait DetailPane<S>: Sync {
    fn handle_key(&self, ctx: &mut AppContext, state: &mut S, key: KeyEvent);
    fn render(&self, f: &mut Frame, ctx: &AppContext, state: &mut S, area: Rect);
}
