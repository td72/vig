use crate::core::app::AppContext;
use crate::core::keymap::{
    nav_bindings, search_bindings, ActionHelp, Keymap, NavAction, SearchAction,
};
use crate::core::pane::{self, Pane, PaneEvent, PaneShared};
use crate::core::search::SearchMatch;
use crate::core::theme;
pub use crate::core::tree::{nest_by, TreePos};
use crate::github::state::GhBgMessage;
use crossterm::event::KeyCode;
use ratatui::{layout::Rect, widgets::ListItem, Frame};
use std::sync::mpsc;

// === Action enum ===

#[derive(Debug, Clone)]
pub enum GhListAction {
    Nav(NavAction),
    OpenDetail,
    SwitchTab,
    OpenBrowser,
    CopyUrl,
    Search(SearchAction),
    Esc,
}

crate::impl_pane_action_from_str!(
    GhListAction, nav: Nav, search: Search, esc: Esc,
    OpenDetail, SwitchTab, OpenBrowser, CopyUrl
);

impl ActionHelp for GhListAction {
    fn label(&self) -> Option<&'static str> {
        match self {
            GhListAction::Nav(nav) => nav.label(),
            GhListAction::OpenDetail => Some("Open detail"),
            GhListAction::SwitchTab => Some("Switch tab"),
            GhListAction::OpenBrowser => Some("Open in browser"),
            GhListAction::CopyUrl => Some("Copy URL"),
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
        .key(KeyCode::Char('y'), GhListAction::CopyUrl)
        .key(KeyCode::Esc, GhListAction::Esc)
}

// === Trait for item-specific behavior ===

pub trait GhListItem: Sized + Send + 'static {
    fn pane_title() -> &'static str;
    fn empty_message() -> &'static str;
    /// Render one row; `tree` carries the indent / guide prefix when the
    /// item is nested under another one.
    fn render_item(&self, tree: &TreePos) -> ListItem<'static>;
    fn number(&self) -> u64;
    /// Number of the item this one is nested under, if any. `items` is the
    /// whole fetched list, for parents that must be resolved by lookup.
    fn parent_number(&self, _items: &[Self]) -> Option<u64> {
        None
    }
    fn search_text(&self) -> String;
    fn browser_event(&self) -> PaneEvent;
    /// The item's URL, built locally (no API request); `None` when it
    /// cannot be derived (e.g. a non-github.com remote).
    fn copy_url(&self) -> Option<String>;
    fn load_disk_cache() -> Option<Vec<Self>>;
    fn save_disk_cache(items: &[Self]);
    fn fetch_list() -> Result<Vec<Self>, String>;
    fn wrap_bg_message(result: Result<Vec<Self>, String>) -> GhBgMessage;
}

/// Reorder `items` into their nested display order.
pub fn nest_items<T: GhListItem>(items: Vec<T>) -> (Vec<T>, Vec<TreePos>) {
    let order = nest_by(
        items.len(),
        |i| items[i].number(),
        |i| items[i].parent_number(&items),
    );
    let mut slots: Vec<Option<T>> = items.into_iter().map(Some).collect();
    let mut out = Vec::with_capacity(slots.len());
    let mut positions = Vec::with_capacity(slots.len());
    for (i, pos) in order {
        if let Some(item) = slots[i].take() {
            out.push(item);
            positions.push(pos);
        }
    }
    (out, positions)
}

/// Copy `url` to the clipboard, or say why there is nothing to copy.
pub fn copy_url_event(url: Option<String>) -> PaneEvent {
    match url.filter(|u| !u.is_empty()) {
        Some(u) => PaneEvent::CopyToClipboard(u),
        None => PaneEvent::StatusMessage("No URL for this item (not a github.com remote?)".into()),
    }
}

// === Generic list pane ===

pub struct GhListPane<T: GhListItem> {
    pub items: Vec<T>,
    /// Tree placement per row of `items`.
    positions: Vec<TreePos>,
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
            positions: Vec::new(),
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

    pub fn set_keymap(&mut self, km: Keymap<GhListAction>) {
        self.keymap = km;
    }

    pub fn keymap(&self) -> &Keymap<GhListAction> {
        &self.keymap
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

    pub fn selected_item(&self) -> Option<&T> {
        self.items.get(self.selected_idx)
    }

    /// Load disk cache + spawn background fetch.
    pub fn initialize(&mut self, tx: &mpsc::Sender<GhBgMessage>) {
        if let Some(items) = T::load_disk_cache() {
            self.set_items(items);
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
        self.set_items(items);
    }

    /// Replace the list, keeping the selection on the same item when it is
    /// still listed (a refresh prepends new items) and clamping the index
    /// otherwise.
    pub(crate) fn set_items(&mut self, items: Vec<T>) {
        let keep = self.selected_number();
        let (items, positions) = nest_items(items);
        self.items = items;
        self.positions = positions;
        self.selected_idx = keep
            .and_then(|n| self.items.iter().position(|i| i.number() == n))
            .unwrap_or(self.selected_idx)
            .min(self.items.len().saturating_sub(1));
    }

    fn execute(&mut self, shared: &PaneShared, action: GhListAction) -> Vec<PaneEvent> {
        if let Some(events) = pane::try_dispatch_search_esc(&action, shared, self.pane_id, vec![]) {
            return events;
        }
        match action {
            GhListAction::Nav(nav) => {
                return pane::execute_list_nav(nav, &mut self.selected_idx, self.items.len(), None);
            }
            GhListAction::OpenDetail if !self.items.is_empty() => {
                return vec![PaneEvent::SetFocus(self.detail_pane_id)];
            }
            GhListAction::SwitchTab => {
                return vec![PaneEvent::SetFocus(self.switch_target)];
            }
            GhListAction::OpenBrowser => {
                if let Some(item) = self.items.get(self.selected_idx) {
                    return vec![item.browser_event()];
                }
            }
            GhListAction::CopyUrl => {
                if let Some(item) = self.items.get(self.selected_idx) {
                    return vec![copy_url_event(item.copy_url())];
                }
            }
            _ => {}
        }
        vec![]
    }

    fn render_impl(&mut self, f: &mut Frame, _ctx: &AppContext, shared: &PaneShared, area: Rect) {
        let is_focused = shared.focused_pane == self.pane_id;
        let empty = if self.loading && self.items.is_empty() {
            Some("Loading...")
        } else if self.items.is_empty() {
            Some(T::empty_message())
        } else {
            None
        };

        let show_selection = is_focused
            || (shared.focused_pane == self.detail_pane_id && shared.previous_pane == self.pane_id);
        let selected = show_selection.then_some(self.selected_idx);

        theme::render_list_pane(
            f,
            area,
            shared,
            self.pane_id,
            T::pane_title(),
            selected,
            empty,
            |match_set, current_match_idx| {
                self.items
                    .iter()
                    .enumerate()
                    .map(|(idx, item)| {
                        let tree = self.positions.get(idx).cloned().unwrap_or_default();
                        let mut li = item.render_item(&tree);
                        let hl = theme::search_highlight_for(match_set, current_match_idx, idx);
                        if hl.is_active() {
                            use ratatui::style::Style;
                            li = li.style(hl.apply(Style::default()));
                        }
                        li
                    })
                    .collect()
            },
        );
    }

    fn collect_search_matches_impl(&self, _shared: &PaneShared, query: &str) -> Vec<SearchMatch> {
        pane::collect_list_search_matches(&self.items, query, |item| item.search_text())
    }
}

impl<T: GhListItem> Pane<PaneEvent> for GhListPane<T> {
    crate::impl_handle_key!(keymap);

    fn render(&mut self, f: &mut Frame, ctx: &AppContext, shared: &PaneShared, area: Rect) {
        self.render_impl(f, ctx, shared, area)
    }

    fn collect_search_matches(&self, shared: &PaneShared, query: &str) -> Vec<SearchMatch> {
        self.collect_search_matches_impl(shared, query)
    }

    crate::impl_list_pane_selection!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_url_event_copies_or_explains() {
        assert!(matches!(
            copy_url_event(Some("https://github.com/o/r/issues/1".into())),
            PaneEvent::CopyToClipboard(u) if u.ends_with("/issues/1")
        ));
        assert!(matches!(copy_url_event(None), PaneEvent::StatusMessage(_)));
        assert!(matches!(
            copy_url_event(Some(String::new())),
            PaneEvent::StatusMessage(_)
        ));
    }
}
