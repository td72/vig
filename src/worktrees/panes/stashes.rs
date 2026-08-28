//! Bottom-left pane of the Worktrees page: the `git stash list`.

use crate::core::app::AppContext;
use crate::core::keymap::{
    nav_bindings, search_bindings, ActionHelp, Keymap, NavAction, SearchAction,
};
use crate::core::pane::{self, Pane, PaneEvent, PaneShared};
use crate::core::search::SearchMatch;
use crate::core::theme;
use crate::worktrees::domain::types::Stash;
use crate::worktrees::panes::fit_head;
use crossterm::event::KeyCode;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::{layout::Rect, widgets::ListItem, Frame};

#[derive(Debug, Clone)]
pub enum StashesAction {
    Nav(NavAction),
    /// Move focus to the preview pane.
    FocusPreview,
    Search(SearchAction),
    Esc,
}

crate::impl_pane_action_from_str!(
    StashesAction, nav: Nav, search: Search, esc: Esc,
    FocusPreview
);

impl ActionHelp for StashesAction {
    fn label(&self) -> Option<&'static str> {
        match self {
            StashesAction::Nav(nav) => nav.label(),
            StashesAction::FocusPreview => Some("Focus preview"),
            StashesAction::Search(sa) => sa.label(),
            StashesAction::Esc => Some("Clear search"),
        }
    }
}

pub fn default_keymap() -> Keymap<StashesAction> {
    Keymap::new()
        .bindings(nav_bindings(StashesAction::Nav))
        .bindings(search_bindings(StashesAction::Search))
        .key(KeyCode::Char('i'), StashesAction::FocusPreview)
        .key(KeyCode::Enter, StashesAction::FocusPreview)
        .key(KeyCode::Char('l'), StashesAction::FocusPreview)
        .key(KeyCode::Esc, StashesAction::Esc)
}

pub struct StashesPane {
    pub items: Vec<Stash>,
    pub selected_idx: usize,
    keymap: Keymap<StashesAction>,
    pane_id: usize,
    preview_pane_id: usize,
    view_height: u16,
}

impl StashesPane {
    pub fn new(pane_id: usize, preview_pane_id: usize) -> Self {
        Self {
            items: Vec::new(),
            selected_idx: 0,
            keymap: default_keymap(),
            pane_id,
            preview_pane_id,
            view_height: 20,
        }
    }

    pub fn set_keymap(&mut self, km: Keymap<StashesAction>) {
        self.keymap = km;
    }

    pub fn keymap(&self) -> &Keymap<StashesAction> {
        &self.keymap
    }

    pub fn selected(&self) -> Option<&Stash> {
        self.items.get(self.selected_idx)
    }

    /// Replace the listing, keeping the selection on the same stash commit
    /// when it still exists (indices shift when entries are dropped).
    pub fn set_items(&mut self, items: Vec<Stash>) {
        let keep = self.selected().map(|s| s.hash.clone());
        self.items = items;
        self.selected_idx = keep
            .and_then(|h| self.items.iter().position(|s| s.hash == h))
            .unwrap_or(self.selected_idx)
            .min(self.items.len().saturating_sub(1));
    }

    fn execute(&mut self, shared: &PaneShared, action: StashesAction) -> Vec<PaneEvent> {
        if let Some(events) = pane::try_dispatch_search_esc(&action, shared, self.pane_id, vec![]) {
            return events;
        }
        match action {
            StashesAction::Nav(nav) => pane::execute_list_nav(
                nav,
                &mut self.selected_idx,
                self.items.len(),
                Some(self.view_height),
            ),
            StashesAction::FocusPreview if !self.items.is_empty() => {
                vec![PaneEvent::SetFocus(self.preview_pane_id)]
            }
            _ => vec![],
        }
    }

    /// ` stash@{0}  message                 branch · 3 days ago`
    fn row(&self, stash: &Stash, hl: &theme::SearchHighlight, width: usize) -> Line<'static> {
        let name = format!(" {:<10} ", stash.name());
        let used = name.chars().count();
        let (right, msg_w) = stash_right_column(stash, width.saturating_sub(used));
        let message = fit_head(&stash.message, msg_w);
        let mut spans = vec![
            Span::styled(name, hl.style_with_fg(Color::Yellow)),
            Span::styled(message.clone(), hl.apply(Style::default())),
        ];
        if let Some(right) = right {
            let gap = msg_w - message.chars().count() + 2;
            spans.push(Span::raw(" ".repeat(gap)));
            // Gray (not DarkGray) stays readable on the selection bg.
            spans.push(Span::styled(right, hl.style_with_fg(Color::Gray)));
        }
        Line::from(spans)
    }
}

/// Shortest message width kept before the right column is sacrificed.
const MIN_MESSAGE_WIDTH: usize = 20;

/// The right-aligned `branch · relative date` column for a row with `avail`
/// columns after the name, and the width left for the message. The branch
/// is dropped first, then the date, so the message keeps at least
/// [`MIN_MESSAGE_WIDTH`] columns.
fn stash_right_column(stash: &Stash, avail: usize) -> (Option<String>, usize) {
    let mut candidates = Vec::new();
    if let (Some(b), false) = (&stash.branch, stash.relative_date.is_empty()) {
        candidates.push(format!("{b} · {}", stash.relative_date));
    }
    if !stash.relative_date.is_empty() {
        candidates.push(stash.relative_date.clone());
    } else if let Some(b) = &stash.branch {
        candidates.push(b.clone());
    }
    for right in candidates {
        let len = right.chars().count();
        if avail >= MIN_MESSAGE_WIDTH + 2 + len {
            return (Some(right), avail - len - 2);
        }
    }
    (None, avail)
}

impl Pane<PaneEvent> for StashesPane {
    crate::impl_handle_key!(keymap);

    fn render(&mut self, f: &mut Frame, _ctx: &AppContext, shared: &PaneShared, area: Rect) {
        self.view_height = area.height.saturating_sub(2);
        let width = area.width.saturating_sub(2) as usize;
        let empty = self.items.is_empty().then_some("No stashes");
        let is_focused = shared.focused_pane == self.pane_id;
        let show_selection = is_focused
            || (shared.focused_pane == self.preview_pane_id
                && shared.previous_pane == self.pane_id);
        let selected = show_selection.then_some(self.selected_idx);
        theme::render_list_pane(
            f,
            area,
            shared,
            self.pane_id,
            "Stashes",
            selected,
            empty,
            |match_set, current_match_idx| {
                self.items
                    .iter()
                    .enumerate()
                    .map(|(idx, stash)| {
                        let hl = theme::search_highlight_for(match_set, current_match_idx, idx);
                        let mut li = ListItem::new(self.row(stash, &hl, width));
                        if hl.is_active() {
                            li = li.style(hl.apply(Style::default()));
                        }
                        li
                    })
                    .collect()
            },
        );
    }

    fn collect_search_matches(&self, _shared: &PaneShared, query: &str) -> Vec<SearchMatch> {
        pane::collect_list_search_matches(&self.items, query, |s| {
            format!(
                "{} {} {}",
                s.name(),
                s.branch.as_deref().unwrap_or(""),
                s.message
            )
        })
    }

    crate::impl_list_pane_selection!();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stash(branch: Option<&str>, date: &str) -> Stash {
        Stash {
            index: 0,
            branch: branch.map(str::to_string),
            message: "wip: greeting tweak".to_string(),
            relative_date: date.to_string(),
            hash: "abc".to_string(),
        }
    }

    #[test]
    fn right_column_degrades_gracefully() {
        let s = stash(Some("main"), "3 days ago");
        // Plenty of room: branch and date.
        assert_eq!(
            stash_right_column(&s, 60),
            (Some("main · 3 days ago".to_string()), 60 - 17 - 2)
        );
        // Tight: the branch goes first.
        assert_eq!(
            stash_right_column(&s, 34),
            (Some("3 days ago".to_string()), 34 - 10 - 2)
        );
        // Too narrow for anything but the message.
        assert_eq!(stash_right_column(&s, 30), (None, 30));
        // No date recorded: the branch alone.
        assert_eq!(
            stash_right_column(&stash(Some("main"), ""), 40),
            (Some("main".to_string()), 40 - 4 - 2)
        );
        assert_eq!(stash_right_column(&stash(None, ""), 40), (None, 40));
    }
}
