use crate::core::app::AppContext;
use crossterm::event::KeyEvent;
use ratatui::{layout::Rect, Frame};

pub trait FocusState<P: Copy + PartialEq + 'static> {
    fn focused_pane(&self) -> P;
    fn set_focus(&mut self, id: P);
}

pub trait SelectPane<S>: Sync {
    fn handle_key(&self, ctx: &mut AppContext, state: &mut S, key: KeyEvent);
    fn render(&self, f: &mut Frame, ctx: &AppContext, state: &mut S, area: Rect);
}

#[derive(Debug, Default, Clone)]
pub struct SubPaneScroll {
    pub scroll_y: u16,
    pub selected_idx: usize,
}

impl SubPaneScroll {
    pub fn reset(&mut self) {
        self.scroll_y = 0;
        self.selected_idx = 0;
    }
}

#[allow(dead_code)]
pub trait DetailState: FocusState<Self::SubPaneId> {
    type SubPaneId: Copy + PartialEq + 'static;
    fn sub_scroll(&self, id: Self::SubPaneId) -> &SubPaneScroll;
    fn sub_scroll_mut(&mut self, id: Self::SubPaneId) -> &mut SubPaneScroll;
    fn detail_view_height(&self) -> u16;
    fn set_detail_view_height(&mut self, h: u16);
    fn reset_sub_panes(&mut self);

    fn active_scroll(&self) -> &SubPaneScroll {
        self.sub_scroll(self.focused_pane())
    }
    fn active_scroll_mut(&mut self) -> &mut SubPaneScroll {
        let id = self.focused_pane();
        self.sub_scroll_mut(id)
    }
}

pub trait DetailPane<S>: Sync {
    fn handle_key(&self, ctx: &mut AppContext, state: &mut S, key: KeyEvent);
    fn render(&self, f: &mut Frame, ctx: &AppContext, state: &mut S, area: Rect);
}
