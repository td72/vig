use crate::core::app::AppContext;
use crate::core::keymap::{execute_nav, Keymap, NavAction, SearchAction, ViewAction};
use crate::core::search::{SearchMatch, SearchState};
use crossterm::event::KeyEvent;
use ratatui::{layout::Rect, Frame};

// === collect_list_search_matches ===

/// Generic search-match collector for list panes.
/// `extractor` returns the searchable text for each item.
pub fn collect_list_search_matches<T>(
    items: &[T],
    query: &str,
    extractor: impl Fn(&T) -> String,
) -> Vec<SearchMatch> {
    let query_lower = query.to_lowercase();
    items
        .iter()
        .enumerate()
        .filter_map(|(idx, item)| {
            if extractor(item).to_lowercase().contains(&query_lower) {
                Some(SearchMatch::ListEntry(idx))
            } else {
                None
            }
        })
        .collect()
}

/// Execute a nav action and return `SelectionChanged` if the selection moved.
pub fn execute_list_nav(
    nav: NavAction,
    selected_idx: &mut usize,
    len: usize,
    view_height: Option<u16>,
) -> Vec<PaneEvent> {
    if execute_nav(nav, selected_idx, len, view_height) {
        vec![PaneEvent::SelectionChanged]
    } else {
        vec![]
    }
}

// === HasSearchEsc trait ===

/// Trait for pane action enums that have Search and Esc variants.
pub trait HasSearchEsc {
    fn as_search(&self) -> Option<&SearchAction>;
    fn is_esc(&self) -> bool;
}

/// Try to dispatch Search/Esc from a pane action.
/// Returns `Some(events)` if consumed, `None` otherwise.
pub fn try_dispatch_search_esc<A: HasSearchEsc>(
    action: &A,
    shared: &PaneShared,
    pane_id: usize,
    esc_fallback: Vec<PaneEvent>,
) -> Option<Vec<PaneEvent>> {
    if let Some(sa) = action.as_search() {
        return Some(execute_search(*sa, pane_id));
    }
    if action.is_esc() {
        return Some(execute_esc(shared, esc_fallback));
    }
    None
}

// === Macros ===

/// Generate `set_selected_idx` + `jump_to_match` for list panes that use `self.selected_idx`.
#[macro_export]
macro_rules! impl_list_pane_selection {
    () => {
        fn set_selected_idx(&mut self, idx: usize) {
            self.selected_idx = idx;
        }
        fn jump_to_match(
            &mut self,
            _shared: &$crate::core::pane::PaneShared,
            search_match: &$crate::core::search::SearchMatch,
        ) {
            if let $crate::core::search::SearchMatch::ListEntry(idx) = search_match {
                self.selected_idx = *idx;
            }
        }
    };
}

/// Generate `handle_key` that does keymap lookup + execute.
///
/// Basic form: `impl_handle_key!(keymap)`
///
/// Modal form: `impl_handle_key!(keymap, modal: field_name => handler_method)`
/// — checks if `self.field_name.is_some()` and delegates to `self.handler_method(key)`
/// before the normal keymap lookup path.
#[macro_export]
macro_rules! impl_handle_key {
    ($keymap:ident) => {
        fn handle_key(
            &mut self,
            shared: &$crate::core::pane::PaneShared,
            key: crossterm::event::KeyEvent,
        ) -> Vec<PaneEvent> {
            let action = match self.$keymap.lookup(key) {
                Some(a) => a.clone(),
                None => return vec![],
            };
            self.execute(shared, action)
        }
    };
    ($keymap:ident, modal: $field:ident => $handler:ident) => {
        fn handle_key(
            &mut self,
            shared: &$crate::core::pane::PaneShared,
            key: crossterm::event::KeyEvent,
        ) -> Vec<PaneEvent> {
            if self.$field.is_some() {
                return self.$handler(key);
            }
            let action = match self.$keymap.lookup(key) {
                Some(a) => a.clone(),
                None => return vec![],
            };
            self.execute(shared, action)
        }
    };
}

pub enum PaneEvent {
    // Common
    SetFocus(usize),
    SelectionChanged,
    StatusMessage(String),
    CopyToClipboard(String),
    OpenUrl(String),
    // Git-specific
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

    /// Handle tab navigation via ViewAction.
    /// Returns `Some(events)` if the action was consumed, `None` otherwise.
    pub fn dispatch_view_nav(
        &self,
        action: ViewAction,
        tab_panes: &[usize],
    ) -> Option<Vec<PaneEvent>> {
        match action {
            ViewAction::PrevTab => {
                let tab_idx = self.tab_index(tab_panes)?;
                if tab_idx > 0 {
                    Some(vec![PaneEvent::SetFocus(tab_panes[tab_idx - 1])])
                } else {
                    Some(vec![])
                }
            }
            ViewAction::NextTab => {
                let tab_idx = self.tab_index(tab_panes)?;
                if tab_idx + 1 < tab_panes.len() {
                    Some(vec![PaneEvent::SetFocus(tab_panes[tab_idx + 1])])
                } else {
                    Some(vec![])
                }
            }
            ViewAction::CyclePaneForward => {
                Some(vec![PaneEvent::SetFocus(self.next_tab_id(tab_panes))])
            }
            ViewAction::CyclePaneBackward => {
                Some(vec![PaneEvent::SetFocus(self.prev_tab_id(tab_panes))])
            }
            _ => None,
        }
    }

    /// Try view-level navigation first, then delegate to the focused/modal pane.
    pub fn dispatch_key(
        &self,
        panes: &mut impl PaneSet,
        view_keymap: &Keymap<ViewAction>,
        tab_panes: &[usize],
        key: KeyEvent,
    ) -> Vec<PaneEvent> {
        if let Some(action) = view_keymap.lookup(key) {
            if let Some(events) = self.dispatch_view_nav(*action, tab_panes) {
                return events;
            }
        }
        self.dispatch_to_pane(panes, key)
    }

    /// Handle search input mode: if search is active, consume the key event
    /// for search editing. Returns `true` if the key was consumed (caller
    /// should return early with `PageAction::None`).
    pub fn handle_search_input(
        &mut self,
        panes: &mut impl PaneSet,
        ctx: &mut AppContext,
        key: KeyEvent,
    ) -> bool {
        if !self.search.active {
            return false;
        }
        if self.search.handle_input_key(key) {
            self.execute_search(panes);
            self.jump_to_search_match(panes, ctx, true);
        }
        true
    }

    /// Delegate a key event to the currently focused pane (or a modal pane if one is open).
    pub fn dispatch_to_pane(&self, panes: &mut impl PaneSet, key: KeyEvent) -> Vec<PaneEvent> {
        let target = panes.find_modal().unwrap_or(self.focused_pane);
        panes
            .get_mut(target)
            .map(|p| p.handle_key(self, key))
            .unwrap_or_default()
    }

    /// Collect search matches from the origin pane and store them in the search state.
    pub fn execute_search(&mut self, panes: &mut impl PaneSet) {
        self.search.matches.clear();
        self.search.current_match_idx = None;
        let query = match &self.search.query {
            Some(q) => q.clone(),
            None => return,
        };
        let origin = self.search.origin;
        let matches = if let Some(pane) = panes.get_mut(origin) {
            pane.collect_search_matches(self, &query)
        } else {
            vec![]
        };
        self.search.matches = matches;
    }

    /// Advance to the next/prev search match and jump the origin pane.
    /// Returns `Some(origin_pane_idx)` if a jump occurred, so the caller
    /// can perform page-specific post-jump sync (e.g. loading detail views).
    pub fn jump_to_search_match(
        &mut self,
        panes: &mut impl PaneSet,
        ctx: &mut AppContext,
        forward: bool,
    ) -> Option<usize> {
        // Re-execute search if no active query but last_query exists (n/N reuse)
        if self.search.query.is_none() {
            if let Some(last) = self.search.last_query.clone() {
                self.search.query = Some(last);
                self.execute_search(panes);
            } else {
                return None;
            }
        }

        if self.search.matches.is_empty() {
            ctx.status_message = Some("Pattern not found".to_string());
            return None;
        }

        let total = self.search.matches.len();
        let new_idx = match self.search.current_match_idx {
            Some(idx) => {
                if forward {
                    (idx + 1) % total
                } else {
                    (idx + total - 1) % total
                }
            }
            None => {
                if forward {
                    0
                } else {
                    total - 1
                }
            }
        };
        self.search.current_match_idx = Some(new_idx);

        let search_match = self.search.matches[new_idx].clone();
        let origin = self.search.origin;

        if let Some(pane) = panes.get_mut(origin) {
            pane.jump_to_match(self, &search_match);
        }

        ctx.status_message = Some(format!("[{}/{}]", new_idx + 1, total));
        Some(origin)
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
        PaneEvent::OpenUrl(url) => {
            ctx.open_url(url);
            true
        }
        _ => false,
    }
}

/// Execute a ViewAction that is common across all pages.
/// Returns `Some(PageAction)` if the action was fully handled, `None` for page-specific actions.
pub fn execute_common_view_action(ctx: &mut AppContext, action: ViewAction) -> Option<PageAction> {
    match action {
        ViewAction::Quit => {
            ctx.should_quit = true;
            Some(PageAction::None)
        }
        ViewAction::Help => {
            ctx.show_help = true;
            Some(PageAction::None)
        }
        _ => None,
    }
}

/// Execute a SearchAction, returning the appropriate PaneEvents.
pub fn execute_search(action: SearchAction, pane_id: usize) -> Vec<PaneEvent> {
    match action {
        SearchAction::Start => vec![PaneEvent::StartSearch(pane_id)],
        SearchAction::Next => vec![PaneEvent::JumpToMatch(true)],
        SearchAction::Prev => vec![PaneEvent::JumpToMatch(false)],
    }
}

/// Execute Esc: clear search if active, otherwise return the fallback events.
pub fn execute_esc(shared: &PaneShared, fallback: Vec<PaneEvent>) -> Vec<PaneEvent> {
    if shared.search.query.is_some() {
        vec![PaneEvent::ClearSearch]
    } else {
        fallback
    }
}

pub trait PaneSet {
    fn get_mut(&mut self, idx: usize) -> Option<&mut dyn Pane<PaneEvent>>;
    /// Return the index of a modal pane if one is currently open.
    fn find_modal(&mut self) -> Option<usize> {
        None
    }
}

/// A page that owns a `PaneShared`, a `PaneSet`, a view keymap, and a
/// `PageLayoutConfig` can implement this trait to share the generic
/// `dispatch_page_key` / `render_page_content` helpers below.
pub trait PageLayout {
    type Panes: PaneSet;
    fn page_parts_mut(
        &mut self,
    ) -> (
        &mut PaneShared,
        &mut Self::Panes,
        &Keymap<ViewAction>,
        &crate::core::layout::PageLayoutConfig,
    );
}

/// Dispatch a key event through the page's shared `PaneShared`.
pub fn dispatch_page_key<S: PageLayout>(state: &mut S, key: KeyEvent) -> Vec<PaneEvent> {
    let (shared, panes, keymap, layout) = state.page_parts_mut();
    shared.dispatch_key(panes, keymap, &layout.tab_panes, key)
}

/// Render the layout tree's panes into the given content area.
pub fn render_page_content<S: PageLayout>(
    state: &mut S,
    f: &mut Frame,
    ctx: &AppContext,
    area: Rect,
) {
    let (shared, panes, _, layout) = state.page_parts_mut();
    let slots = layout.resolve_slots(shared.focused_pane);
    let areas = crate::core::layout::resolve_layout(area, &layout.tree, &slots);
    shared.render_panes(panes, f, ctx, &areas);
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
