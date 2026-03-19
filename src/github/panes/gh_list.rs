use crate::core::app::AppContext;
use crate::core::keymap::{
    execute_nav, nav_bindings, search_bindings, ActionHelp, Keymap, NavAction, SearchAction,
};
use crate::core::pane::{self, Pane, PaneEvent, PaneShared};
use crate::core::search::SearchMatch;
use crate::core::theme;
use crate::github::state::GhBgMessage;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    widgets::{List, ListItem, ListState},
    Frame,
};
use std::sync::mpsc;

// === Action enum ===

#[derive(Debug, Clone)]
pub enum GhListAction {
    Nav(NavAction),
    OpenDetail,
    SwitchTab,
    OpenBrowser,
    Search(SearchAction),
    Esc,
}

crate::impl_pane_action_from_str!(
    GhListAction, nav: Nav, search: Search,
    OpenDetail, SwitchTab, OpenBrowser, Esc
);

impl ActionHelp for GhListAction {
    fn label(&self) -> Option<&'static str> {
        match self {
            GhListAction::Nav(nav) => nav.label(),
            GhListAction::OpenDetail => Some("Open detail"),
            GhListAction::SwitchTab => Some("Switch tab"),
            GhListAction::OpenBrowser => Some("Open in browser"),
            GhListAction::Search(sa) => sa.label(),
            GhListAction::Esc => Some("Clear search"),
        }
    }
}

pub fn default_keymap(switch_key: KeyCode) -> Keymap<GhListAction> {
    Keymap::new()
        .bindings(nav_bindings(GhListAction::Nav))
        .bindings(search_bindings(GhListAction::Search))
        .key(KeyCode::Char('i'), GhListAction::OpenDetail)
        .key(KeyCode::Enter, GhListAction::OpenDetail)
        .key(switch_key, GhListAction::SwitchTab)
        .key(KeyCode::Char('o'), GhListAction::OpenBrowser)
        .key(KeyCode::Esc, GhListAction::Esc)
}

// === Trait for item-specific behavior ===

pub trait GhListItem: Sized + Send + 'static {
    fn pane_title() -> &'static str;
    fn empty_message() -> &'static str;
    fn render_item(&self) -> ListItem<'static>;
    fn number(&self) -> u64;
    fn search_text(&self) -> String;
    fn browser_event(&self) -> PaneEvent;
    fn load_disk_cache() -> Option<Vec<Self>>;
    fn save_disk_cache(items: &[Self]);
    fn fetch_list() -> Result<Vec<Self>, String>;
    fn wrap_bg_message(result: Result<Vec<Self>, String>) -> GhBgMessage;
}

// === Generic list pane ===

pub struct GhListPane<T: GhListItem> {
    pub items: Vec<T>,
    pub selected_idx: usize,
    pub loading: bool,
    keymap: Keymap<GhListAction>,
    pane_id: usize,
    detail_pane_id: usize,
    switch_target: usize,
}

impl<T: GhListItem> GhListPane<T> {
    pub fn new(
        pane_id: usize,
        detail_pane_id: usize,
        switch_key: KeyCode,
        switch_target: usize,
    ) -> Self {
        Self {
            items: Vec::new(),
            selected_idx: 0,
            loading: false,
            keymap: default_keymap(switch_key),
            pane_id,
            detail_pane_id,
            switch_target,
        }
    }

    /// Set the loading state (e.g. when auth fails and loading should stop).
    pub fn set_loading(&mut self, loading: bool) {
        self.loading = loading;
    }

    /// Number of items in the list.
    pub fn item_count(&self) -> usize {
        self.items.len()
    }

    /// Whether the pane is currently loading data.
    pub fn is_loading(&self) -> bool {
        self.loading
    }

    pub fn selected_number(&self) -> Option<u64> {
        self.items.get(self.selected_idx).map(|i| i.number())
    }

    /// Load disk cache + spawn background fetch.
    pub fn initialize(&mut self, tx: &mpsc::Sender<GhBgMessage>) {
        if let Some(items) = T::load_disk_cache() {
            self.items = items;
        }
        self.loading = true;
        self.spawn_fetch(tx);
    }

    /// Spawn background fetch thread.
    pub fn spawn_fetch(&mut self, tx: &mpsc::Sender<GhBgMessage>) {
        self.loading = true;
        let tx = tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(T::wrap_bg_message(T::fetch_list()));
        });
    }

    /// Apply a freshly fetched list — save to disk cache and update state.
    pub fn apply_list(&mut self, items: Vec<T>) {
        T::save_disk_cache(&items);
        self.items = items;
    }

    fn handle_key_impl(&mut self, shared: &PaneShared, key: KeyEvent) -> Vec<PaneEvent> {
        let action = match self.keymap.lookup(key) {
            Some(a) => a.clone(),
            None => return vec![],
        };
        self.execute(shared, action)
    }

    fn execute(&mut self, shared: &PaneShared, action: GhListAction) -> Vec<PaneEvent> {
        match action {
            GhListAction::Search(sa) => {
                return pane::execute_search(sa, self.pane_id);
            }
            GhListAction::Esc => {
                return pane::execute_esc(shared, vec![]);
            }
            GhListAction::Nav(nav) => {
                if execute_nav(nav, &mut self.selected_idx, self.items.len(), None) {
                    return vec![PaneEvent::SelectionChanged];
                }
            }
            GhListAction::OpenDetail => {
                if !self.items.is_empty() {
                    return vec![PaneEvent::SetFocus(self.detail_pane_id)];
                }
            }
            GhListAction::SwitchTab => {
                return vec![PaneEvent::SetFocus(self.switch_target)];
            }
            GhListAction::OpenBrowser => {
                if let Some(item) = self.items.get(self.selected_idx) {
                    return vec![item.browser_event()];
                }
            }
        }
        vec![]
    }

    fn render_impl(&mut self, f: &mut Frame, _ctx: &AppContext, shared: &PaneShared, area: Rect) {
        let is_focused = shared.focused_pane == self.pane_id;
        let block = theme::pane_block(T::pane_title(), is_focused);

        if self.loading && self.items.is_empty() {
            theme::render_empty_list(f, area, block, "Loading...");
            return;
        }

        if self.items.is_empty() {
            theme::render_empty_list(f, area, block, T::empty_message());
            return;
        }

        let (match_set, current_match_idx) = theme::list_search_highlights(shared, self.pane_id);

        let items: Vec<ListItem> = self
            .items
            .iter()
            .enumerate()
            .map(|(idx, item)| {
                let mut li = item.render_item();
                let is_current = current_match_idx == Some(idx);
                let is_match = match_set.contains(&idx);
                if is_current || is_match {
                    use ratatui::style::Style;
                    let style = if is_current {
                        Style::default()
                            .fg(theme::SEARCH_CURRENT_FG)
                            .bg(theme::SEARCH_CURRENT_BG)
                    } else {
                        Style::default().bg(theme::SEARCH_MATCH_BG)
                    };
                    li = li.style(style);
                }
                li
            })
            .collect();

        let highlight_style = theme::list_highlight_style(match_set.contains(&self.selected_idx));

        let list = List::new(items)
            .block(block)
            .highlight_style(highlight_style);

        let mut list_state = ListState::default();
        if is_focused
            || (shared.focused_pane == self.detail_pane_id && shared.previous_pane == self.pane_id)
        {
            list_state.select(Some(self.selected_idx));
        }
        f.render_stateful_widget(list, area, &mut list_state);
    }

    fn collect_search_matches_impl(&self, _shared: &PaneShared, query: &str) -> Vec<SearchMatch> {
        let query_lower = query.to_lowercase();
        self.items
            .iter()
            .enumerate()
            .filter_map(|(idx, item)| {
                if item.search_text().to_lowercase().contains(&query_lower) {
                    Some(SearchMatch::ListEntry(idx))
                } else {
                    None
                }
            })
            .collect()
    }
}

impl<T: GhListItem> Pane<PaneEvent> for GhListPane<T> {
    fn handle_key(&mut self, shared: &PaneShared, key: KeyEvent) -> Vec<PaneEvent> {
        self.handle_key_impl(shared, key)
    }

    fn render(&mut self, f: &mut Frame, ctx: &AppContext, shared: &PaneShared, area: Rect) {
        self.render_impl(f, ctx, shared, area)
    }

    fn set_selected_idx(&mut self, idx: usize) {
        self.selected_idx = idx;
    }

    fn collect_search_matches(&self, shared: &PaneShared, query: &str) -> Vec<SearchMatch> {
        self.collect_search_matches_impl(shared, query)
    }

    fn jump_to_match(&mut self, _shared: &PaneShared, search_match: &SearchMatch) {
        if let SearchMatch::ListEntry(idx) = search_match {
            self.selected_idx = *idx;
        }
    }
}
