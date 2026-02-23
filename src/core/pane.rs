use crate::core::app::AppContext;
use crate::core::search::{SearchMatch, SearchState};
use crossterm::event::KeyEvent;
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
