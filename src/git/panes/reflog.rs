use crate::core::app::AppContext;
use crate::core::keymap::{
    nav_bindings, search_bindings, ActionHelp, Keymap, NavAction, SearchAction,
};
use crate::core::pane::{self, Pane, PaneShared};
use crate::core::search::SearchMatch;
use crate::core::theme;
use crate::git::domain::repository::{ReflogEntry, Repo};
use crate::git::state::{PaneEvent, PANE_BRANCH_LIST, PANE_GIT_LOG, PANE_REFLOG};
use crossterm::event::KeyCode;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::ListItem,
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

crate::impl_pane_action_from_str!(
    ReflogAction, nav: Nav, search: Search, esc: Esc,
    SetDiffBase, FocusLog
);

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

    fn execute(&mut self, shared: &PaneShared, action: ReflogAction) -> Vec<PaneEvent> {
        if let Some(events) = pane::try_dispatch_search_esc(
            &action,
            shared,
            PANE_REFLOG,
            vec![PaneEvent::SetFocus(PANE_BRANCH_LIST)],
        ) {
            return events;
        }
        match action {
            ReflogAction::FocusLog => {
                return vec![PaneEvent::SetFocus(PANE_GIT_LOG)];
            }
            ReflogAction::Nav(nav) => {
                return pane::execute_list_nav(
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
            _ => {}
        }
        vec![]
    }

    fn render_impl(&mut self, f: &mut Frame, _ctx: &AppContext, shared: &PaneShared, area: Rect) {
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
                let hl = theme::search_highlight_for(&match_set, current_match_idx, idx);

                let hash_style = hl.style_with_fg(Color::Yellow);
                let selector_style = hl.style_with_fg(Color::DarkGray);
                let action_style = hl.style_with_fg(Color::Cyan).add_modifier(Modifier::BOLD);
                let msg_style = hl.apply(Style::default());

                ListItem::new(Line::from(vec![
                    Span::styled(format!(" {} ", entry.short_hash), hash_style),
                    Span::styled(format!("{} ", entry.selector), selector_style),
                    Span::styled(format!("{}: ", entry.action), action_style),
                    Span::styled(entry.message.clone(), msg_style),
                ]))
            })
            .collect();

        theme::render_search_list(f, area, items, block, Some(self.selected_idx), &match_set);
    }
}

impl Pane<PaneEvent> for ReflogPane {
    crate::impl_handle_key!(keymap);

    fn render(&mut self, f: &mut Frame, ctx: &AppContext, shared: &PaneShared, area: Rect) {
        self.render_impl(f, ctx, shared, area)
    }

    fn collect_search_matches(&self, _shared: &PaneShared, query: &str) -> Vec<SearchMatch> {
        pane::collect_list_search_matches(&self.entries, query, |entry| {
            format!(
                "{} {} {} {}",
                entry.short_hash, entry.selector, entry.action, entry.message
            )
        })
    }

    crate::impl_list_pane_selection!();
}
