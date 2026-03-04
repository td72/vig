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

impl PaneShared {
    pub fn set_focus(&mut self, id: usize) {
        self.previous_pane = self.focused_pane;
        self.focused_pane = id;
    }

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

    /// Handle h/l tab navigation common to all pages.
    /// Returns `Some(events)` if the key was consumed, `None` otherwise.
    pub fn dispatch_tab_key(&self, tab_panes: &[usize], key: KeyEvent) -> Option<Vec<PaneEvent>> {
        let tab_idx = self.tab_index(tab_panes)?;
        match key.code {
            KeyCode::Char('h') if tab_idx > 0 => {
                Some(vec![PaneEvent::SetFocus(tab_panes[tab_idx - 1])])
            }
            KeyCode::Char('l') if tab_idx + 1 < tab_panes.len() => {
                Some(vec![PaneEvent::SetFocus(tab_panes[tab_idx + 1])])
            }
            _ => None,
        }
    }

    /// Delegate a key event to the currently focused pane via dynamic dispatch.
    pub fn dispatch_to_pane(&self, panes: &mut impl PaneSet, key: KeyEvent) -> Vec<PaneEvent> {
        panes
            .get_mut(self.focused_pane)
            .map(|p| p.handle_key(self, key))
            .unwrap_or_default()
    }

    /// Render all panes for the given layout areas.
    pub fn render_panes(
        &self,
        panes: &mut impl PaneSet,
        f: &mut Frame,
        ctx: &AppContext,
        areas: &[(usize, Rect)],
    ) {
        for &(idx, rect) in areas {
            if let Some(p) = panes.get_mut(idx) {
                p.render(f, ctx, self, rect);
            }
        }
    }
}

use crate::core::page::PageAction;

/// Handle common PaneEvents shared by all pages.
/// Returns `true` if the event was fully consumed (caller should skip page-specific handling).
/// `SetFocus` is partially consumed: `set_focus()` is called, but returns `false` so pages
/// can add post-focus logic (e.g. loading detail views).
pub fn process_common_event(
    shared: &mut PaneShared,
    ctx: &mut AppContext,
    event: &PaneEvent,
) -> bool {
    match event {
        PaneEvent::SetFocus(id) => {
            shared.set_focus(*id);
            false // pages may need post-focus processing
        }
        PaneEvent::StartSearch(origin) => {
            shared.search.start(*origin);
            true
        }
        PaneEvent::ClearSearch => {
            shared.search.clear();
            true
        }
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

pub trait PaneSet {
    fn get_mut(&mut self, idx: usize) -> Option<&mut dyn Pane<PaneEvent>>;
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
