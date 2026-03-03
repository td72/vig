use crate::core::app::AppContext;
use crate::core::search::{SearchMatch, SearchState};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{layout::Rect, Frame};

pub enum PaneEvent {
    // Common
    SetFocus(usize),
    SelectionChanged,
    StatusMessage(String),
    CopyToClipboard(String),
    OpenUrl(String),
    // Git-specific
    RefreshDiff,
    SetDiffBase(Option<String>),
    SwitchBranch(String),
    DeleteBranch(String),
    StartSearch(usize),
    ClearSearch,
    JumpToMatch(bool),
    // GitHub-specific
    OpenIssueBrowser(u64),
    OpenPrBrowser(u64),
}

pub struct PaneShared {
    pub focused_pane: usize,
    pub previous_pane: usize,
    pub search: SearchState,
}

impl FocusState<usize> for PaneShared {
    fn focused_pane(&self) -> usize {
        self.focused_pane
    }
    fn set_focus(&mut self, id: usize) {
        self.previous_pane = self.focused_pane;
        self.focused_pane = id;
    }
}

impl PaneShared {
    pub fn tab_index(&self, tab_panes: &[usize]) -> Option<usize> {
        tab_panes.iter().position(|&p| p == self.focused_pane)
    }

    pub fn next_tab_id(&self, tab_panes: &[usize]) -> usize {
        match self.tab_index(tab_panes) {
            Some(idx) => tab_panes[(idx + 1) % tab_panes.len()],
            None => tab_panes[0],
        }
    }

    pub fn prev_tab_id(&self, tab_panes: &[usize]) -> usize {
        match self.tab_index(tab_panes) {
            Some(idx) => tab_panes[(idx + tab_panes.len() - 1) % tab_panes.len()],
            None => tab_panes[0],
        }
    }
}

use crate::core::page::PageAction;

/// Handle common PaneEvents shared by all pages.
/// Returns `true` if the event was consumed (caller should skip page-specific handling).
pub fn process_common_event(ctx: &mut AppContext, event: &PaneEvent) -> bool {
    match event {
        PaneEvent::StatusMessage(msg) => {
            ctx.status_message = Some(msg.clone());
            true
        }
        PaneEvent::CopyToClipboard(text) => {
            ctx.copy_to_clipboard(text);
            true
        }
        _ => false,
    }
}

pub fn handle_common_view_key(ctx: &mut AppContext, key: KeyEvent) -> Option<PageAction> {
    match key.code {
        KeyCode::Char('q') => {
            ctx.should_quit = true;
            Some(PageAction::None)
        }
        KeyCode::Char('?') => {
            ctx.show_help = true;
            Some(PageAction::None)
        }
        _ => None,
    }
}

// Note: #[allow(dead_code)] needed because rustc doesn't track usage through dyn dispatch.
#[allow(dead_code)]
pub trait Pane<Event> {
    fn handle_key(&mut self, shared: &PaneShared, key: KeyEvent) -> Vec<Event>;
    fn render(&mut self, f: &mut Frame, ctx: &AppContext, shared: &PaneShared, area: Rect);

    fn is_modal(&self) -> bool {
        false
    }
    fn set_selected_idx(&mut self, _idx: usize) {}

    fn collect_search_matches(&self, _shared: &PaneShared, _query: &str) -> Vec<SearchMatch> {
        vec![]
    }
    fn jump_to_match(&mut self, _shared: &PaneShared, _search_match: &SearchMatch) {}
    fn drain_background(&mut self) {}
}

// Note: #[allow(dead_code)] needed because set_focus is resolved as a trait method
// but rustc doesn't always track inherent-looking calls to trait methods.
#[allow(dead_code)]
pub trait FocusState<P: Copy + PartialEq + 'static> {
    fn focused_pane(&self) -> P;
    fn set_focus(&mut self, id: P);
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
