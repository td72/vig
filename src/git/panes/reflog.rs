use crate::core::app::AppContext;
use crate::core::keymap::{
    execute_nav, nav_bindings, search_bindings, ActionHelp, Keymap, NavAction, SearchAction,
};
use crate::core::pane::{self, Pane, PaneShared};
use crate::core::search::SearchMatch;
use crate::core::theme;
use crate::git::domain::repository::{ReflogEntry, Repo};
use crate::git::state::{PaneEvent, PANE_BRANCH_LIST, PANE_GIT_LOG, PANE_REFLOG};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, ListState},
    Frame,
};

#[derive(Debug, Clone)]
pub enum ReflogAction {
    Nav(NavAction),
    SetDiffBase,
    FocusLog,
    Search(SearchAction),
    Esc,
}

impl ActionHelp for ReflogAction {
    fn label(&self) -> Option<&'static str> {
        match self {
            ReflogAction::Nav(nav) => nav.label(),
            ReflogAction::SetDiffBase => Some("Set as diff base"),
            ReflogAction::FocusLog => Some("Focus log"),
            ReflogAction::Search(sa) => sa.label(),
            ReflogAction::Esc => Some("Clear search / Back"),
        }
    }
}

pub fn default_keymap() -> Keymap<ReflogAction> {
    Keymap::new()
        .bindings(nav_bindings(ReflogAction::Nav))
        .bindings(search_bindings(ReflogAction::Search))
        .key(KeyCode::Enter, ReflogAction::SetDiffBase)
        .key(KeyCode::Char('i'), ReflogAction::FocusLog)
        .key(KeyCode::Esc, ReflogAction::Esc)
}

pub struct ReflogPane {
    pub entries: Vec<ReflogEntry>,
    pub selected_idx: usize,
    pub view_height: u16,
    keymap: Keymap<ReflogAction>,
}

impl ReflogPane {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            selected_idx: 0,
            view_height: 0,
            keymap: default_keymap(),
        }
    }

    pub fn load(&mut self, repo: &Repo) {
        self.entries = repo.reflog(500);
        if self.selected_idx >= self.entries.len() {
            self.selected_idx = 0;
        }
    }

    pub fn handle_key(&mut self, shared: &PaneShared, key: KeyEvent) -> Vec<PaneEvent> {
        let action = match self.keymap.lookup(key) {
            Some(a) => a.clone(),
            None => return vec![],
        };
        self.execute(shared, action)
    }

    fn execute(&mut self, shared: &PaneShared, action: ReflogAction) -> Vec<PaneEvent> {
        match action {
            ReflogAction::Search(sa) => {
                return pane::execute_search(sa, PANE_REFLOG);
            }
            ReflogAction::FocusLog => {
                return vec![PaneEvent::SetFocus(PANE_GIT_LOG)];
            }
            ReflogAction::Esc => {
                return pane::execute_esc(shared, vec![PaneEvent::SetFocus(PANE_BRANCH_LIST)]);
            }
            ReflogAction::Nav(nav) => {
                execute_nav(
                    nav,
                    &mut self.selected_idx,
                    self.entries.len(),
                    Some(self.view_height),
                );
            }
            ReflogAction::SetDiffBase => {
                if let Some(entry) = self.entries.get(self.selected_idx) {
                    return vec![PaneEvent::SetDiffBase(Some(entry.full_hash.clone()))];
                }
            }
        }
        vec![]
    }

    pub fn collect_search_matches(&self, _shared: &PaneShared, query: &str) -> Vec<SearchMatch> {
        let query_lower = query.to_lowercase();
        self.entries
            .iter()
            .enumerate()
            .filter_map(|(idx, entry)| {
                if entry.short_hash.to_lowercase().contains(&query_lower)
                    || entry.selector.to_lowercase().contains(&query_lower)
                    || entry.action.to_lowercase().contains(&query_lower)
                    || entry.message.to_lowercase().contains(&query_lower)
                {
                    Some(SearchMatch::ListEntry(idx))
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn render(&mut self, f: &mut Frame, _ctx: &AppContext, shared: &PaneShared, area: Rect) {
        self.view_height = area.height.saturating_sub(2);
        let block = theme::pane_block("Reflog", shared.focused_pane == PANE_REFLOG);

        if self.entries.is_empty() {
            theme::render_empty_list(f, area, block, "No reflog entries");
            return;
        }

        let (match_set, current_match_idx) = theme::list_search_highlights(shared, PANE_REFLOG);

        let items: Vec<ListItem> = self
            .entries
            .iter()
            .enumerate()
            .map(|(idx, entry)| {
                let is_current = current_match_idx == Some(idx);
                let is_match = match_set.contains(&idx);
                let bg = if is_current {
                    Some(theme::SEARCH_CURRENT_BG)
                } else if is_match {
                    Some(theme::SEARCH_MATCH_BG)
                } else {
                    None
                };
                let fg_override = if is_current {
                    Some(theme::SEARCH_CURRENT_FG)
                } else {
                    None
                };

                let hash_style = {
                    let mut s = Style::default().fg(fg_override.unwrap_or(Color::Yellow));
                    if let Some(bg) = bg {
                        s = s.bg(bg);
                    }
                    s
                };
                let selector_style = {
                    let mut s = Style::default().fg(fg_override.unwrap_or(Color::DarkGray));
                    if let Some(bg) = bg {
                        s = s.bg(bg);
                    }
                    s
                };
                let action_style = {
                    let mut s = Style::default()
                        .fg(fg_override.unwrap_or(Color::Cyan))
                        .add_modifier(Modifier::BOLD);
                    if let Some(bg) = bg {
                        s = s.bg(bg);
                    }
                    s
                };
                let msg_style = {
                    let mut s = Style::default();
                    if let Some(fg) = fg_override {
                        s = s.fg(fg);
                    }
                    if let Some(bg) = bg {
                        s = s.bg(bg);
                    }
                    s
                };

                ListItem::new(Line::from(vec![
                    Span::styled(format!(" {} ", entry.short_hash), hash_style),
                    Span::styled(format!("{} ", entry.selector), selector_style),
                    Span::styled(format!("{}: ", entry.action), action_style),
                    Span::styled(entry.message.clone(), msg_style),
                ]))
            })
            .collect();

        let selected = self.selected_idx;
        let highlight_style = theme::list_highlight_style(match_set.contains(&selected));

        let list = List::new(items)
            .block(block)
            .highlight_style(highlight_style);

        let mut list_state = ListState::default();
        list_state.select(Some(selected));
        f.render_stateful_widget(list, area, &mut list_state);
    }
}

impl Pane<PaneEvent> for ReflogPane {
    fn handle_key(&mut self, shared: &PaneShared, key: KeyEvent) -> Vec<PaneEvent> {
        self.handle_key(shared, key)
    }

    fn render(&mut self, f: &mut Frame, ctx: &AppContext, shared: &PaneShared, area: Rect) {
        self.render(f, ctx, shared, area)
    }

    fn set_selected_idx(&mut self, idx: usize) {
        self.selected_idx = idx;
    }

    fn collect_search_matches(&self, shared: &PaneShared, query: &str) -> Vec<SearchMatch> {
        self.collect_search_matches(shared, query)
    }

    fn jump_to_match(&mut self, _shared: &PaneShared, search_match: &SearchMatch) {
        if let SearchMatch::ListEntry(idx) = search_match {
            self.selected_idx = *idx;
        }
    }
}
