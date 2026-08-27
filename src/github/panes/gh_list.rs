use crate::core::app::AppContext;
use crate::core::keymap::{
    nav_bindings, search_bindings, ActionHelp, Keymap, NavAction, SearchAction,
};
use crate::core::pane::{self, Pane, PaneEvent, PaneShared};
use crate::core::search::SearchMatch;
use crate::core::theme;
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
    Search(SearchAction),
    Esc,
}

crate::impl_pane_action_from_str!(
    GhListAction, nav: Nav, search: Search, esc: Esc,
    OpenDetail, SwitchTab, OpenBrowser
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
    fn load_disk_cache() -> Option<Vec<Self>>;
    fn save_disk_cache(items: &[Self]);
    fn fetch_list() -> Result<Vec<Self>, String>;
    fn wrap_bg_message(result: Result<Vec<Self>, String>) -> GhBgMessage;
}

// === Tree layout ===

/// Placement of a row in the nested list.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TreePos {
    pub depth: usize,
    /// Guides drawn before the item (`│ ├─ └─`), empty for top-level rows.
    pub prefix: String,
}

/// Depth-first order of `n` items nested under their parents.
///
/// Top-level items (no parent, or a parent that is not in the list) keep
/// their original relative order, and so do siblings. Members of a parent
/// cycle (an item whose parent chain leads back to itself) are top-level;
/// items merely hanging off a cycle still nest under their parent. Returns
/// `(original index, position)` per output row.
pub fn nest_by(
    n: usize,
    number: impl Fn(usize) -> u64,
    parent: impl Fn(usize) -> Option<u64>,
) -> Vec<(usize, TreePos)> {
    let index_of = |num: u64| (0..n).find(|&i| number(i) == num);
    let parent_idx: Vec<Option<usize>> = (0..n)
        .map(|i| parent(i).and_then(index_of).filter(|&p| p != i))
        .collect();
    // An item whose parent chain comes back to itself is in a cycle; drop
    // its parent link so every cycle member becomes a root.
    let in_cycle = |i: usize| {
        let mut cur = parent_idx[i];
        for _ in 0..n {
            match cur {
                Some(p) if p == i => return true,
                Some(p) => cur = parent_idx[p],
                None => return false,
            }
        }
        false
    };
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut has_parent = vec![false; n];
    for (i, hp) in has_parent.iter_mut().enumerate() {
        if let Some(p) = parent_idx[i].filter(|_| !in_cycle(i)) {
            children[p].push(i);
            *hp = true;
        }
    }

    let mut out = Vec::with_capacity(n);
    let mut visited = vec![false; n];
    // (index, depth, ancestors' "more siblings follow" flags, is last sibling)
    fn walk(
        i: usize,
        depth: usize,
        trail: &mut Vec<bool>,
        last: bool,
        children: &[Vec<usize>],
        visited: &mut [bool],
        out: &mut Vec<(usize, TreePos)>,
    ) {
        if visited[i] {
            return;
        }
        visited[i] = true;
        let mut prefix = String::new();
        if depth > 0 {
            for &more in trail.iter() {
                prefix.push_str(if more { "│  " } else { "   " });
            }
            prefix.push_str(if last { "└─ " } else { "├─ " });
        }
        out.push((i, TreePos { depth, prefix }));
        let kids: Vec<usize> = children[i]
            .iter()
            .copied()
            .filter(|&c| !visited[c])
            .collect();
        if depth > 0 {
            trail.push(!last);
        }
        for (k, &c) in kids.iter().enumerate() {
            walk(
                c,
                depth + 1,
                trail,
                k + 1 == kids.len(),
                children,
                visited,
                out,
            );
        }
        if depth > 0 {
            trail.pop();
        }
    }
    let mut trail = Vec::new();
    for (i, _) in has_parent.iter().enumerate().filter(|(_, hp)| !**hp) {
        walk(i, 0, &mut trail, true, &children, &mut visited, &mut out);
    }
    // Safety net: anything still unreached is surfaced as a top-level row.
    for i in 0..n {
        if !visited[i] {
            walk(i, 0, &mut trail, true, &children, &mut visited, &mut out);
        }
    }
    out
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

    fn set_items(&mut self, items: Vec<T>) {
        let (items, positions) = nest_items(items);
        self.items = items;
        self.positions = positions;
        self.selected_idx = self.selected_idx.min(self.items.len().saturating_sub(1));
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

    /// (number, parent) pairs → rendered rows `"<prefix>#<number>"`.
    fn rows(spec: &[(u64, Option<u64>)]) -> Vec<String> {
        nest_by(spec.len(), |i| spec[i].0, |i| spec[i].1)
            .into_iter()
            .map(|(i, pos)| format!("{}#{}", pos.prefix, spec[i].0))
            .collect()
    }

    #[test]
    fn flat_list_keeps_order() {
        assert_eq!(rows(&[(3, None), (2, None), (1, None)]), ["#3", "#2", "#1"]);
    }

    #[test]
    fn children_follow_their_parent_with_guides() {
        let spec = [
            (5, None),
            (4, Some(5)),
            (3, None),
            (2, Some(5)),
            (1, Some(2)),
        ];
        assert_eq!(rows(&spec), ["#5", "├─ #4", "└─ #2", "   └─ #1", "#3"]);
    }

    #[test]
    fn guides_continue_past_siblings_with_children() {
        let spec = [(9, None), (8, Some(9)), (7, Some(8)), (6, Some(9))];
        assert_eq!(rows(&spec), ["#9", "├─ #8", "│  └─ #7", "└─ #6"]);
    }

    #[test]
    fn orphans_and_self_parent_are_top_level() {
        assert_eq!(rows(&[(2, Some(99)), (1, Some(1))]), ["#2", "#1"]);
    }

    #[test]
    fn cycles_do_not_drop_items() {
        // 1 → 2 → 1 plus a child hanging off the cycle: both cycle members
        // are top-level, the child still nests under its parent.
        let spec = [(1, Some(2)), (2, Some(1)), (3, Some(2))];
        assert_eq!(rows(&spec), ["#1", "#2", "└─ #3"]);
        // A longer cycle behaves the same.
        let spec = [(1, Some(3)), (2, Some(1)), (3, Some(2))];
        assert_eq!(rows(&spec), ["#1", "#2", "#3"]);
    }
}
