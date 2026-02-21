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
pub trait DetailState {
    type SubPaneId: Copy + PartialEq;
    fn active_sub_pane(&self) -> Self::SubPaneId;
    fn set_sub_pane(&mut self, id: Self::SubPaneId);
    fn sub_scroll(&self, id: Self::SubPaneId) -> &SubPaneScroll;
    fn sub_scroll_mut(&mut self, id: Self::SubPaneId) -> &mut SubPaneScroll;
    fn detail_view_height(&self) -> u16;
    fn set_detail_view_height(&mut self, h: u16);
    fn reset_sub_panes(&mut self);

    fn active_scroll(&self) -> &SubPaneScroll {
        self.sub_scroll(self.active_sub_pane())
    }
    fn active_scroll_mut(&mut self) -> &mut SubPaneScroll {
        let id = self.active_sub_pane();
        self.sub_scroll_mut(id)
    }
}

pub trait DetailPane<S>: Sync {
    fn handle_key(&self, ctx: &mut AppContext, state: &mut S, key: KeyEvent);
    fn render(&self, f: &mut Frame, ctx: &AppContext, state: &mut S, area: Rect);
}
