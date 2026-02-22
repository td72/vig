use crate::core::app::{AppContext, SearchMatch, SearchOrigin};
use crate::git::domain::repository::ReflogEntry;
use crate::git::state::{FocusedPane, GitShared, PaneEvent};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};
use std::collections::HashSet;

pub struct ReflogPane {
    pub entries: Vec<ReflogEntry>,
    pub selected_idx: usize,
    pub view_height: u16,
}

impl ReflogPane {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            selected_idx: 0,
            view_height: 0,
        }
    }

    pub fn handle_key(&mut self, _shared: &GitShared, key: KeyEvent) -> Vec<PaneEvent> {
        match key.code {
            KeyCode::Esc => {
                return vec![PaneEvent::SetFocus(FocusedPane::BranchList)];
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if !self.entries.is_empty() && self.selected_idx + 1 < self.entries.len() {
                    self.selected_idx += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.selected_idx > 0 {
                    self.selected_idx -= 1;
                }
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = (self.view_height / 2).max(1) as usize;
                let new_idx = self.selected_idx.saturating_add(half);
                self.selected_idx = new_idx.min(self.entries.len().saturating_sub(1));
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = (self.view_height / 2).max(1) as usize;
                self.selected_idx = self.selected_idx.saturating_sub(half);
            }
            KeyCode::Char('g') => {
                self.selected_idx = 0;
            }
            KeyCode::Char('G') => {
                if !self.entries.is_empty() {
                    self.selected_idx = self.entries.len() - 1;
                }
            }
            KeyCode::Enter => {
                if let Some(entry) = self.entries.get(self.selected_idx) {
                    return vec![
                        PaneEvent::SetDiffBase(Some(entry.full_hash.clone())),
                        PaneEvent::RefreshDiff,
                    ];
                }
            }
            _ => {}
        }
        vec![]
    }

    pub fn collect_search_matches(&self, query: &str) -> Vec<SearchMatch> {
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
                    Some(SearchMatch::ReflogEntry(idx))
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn render(&mut self, f: &mut Frame, _ctx: &AppContext, shared: &GitShared, area: Rect) {
        self.view_height = area.height.saturating_sub(2);
        let border_color = if shared.focused_pane == FocusedPane::Reflog {
            Color::Cyan
        } else {
            Color::DarkGray
        };

        let block = Block::default()
            .title(" Reflog ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color));

        if self.entries.is_empty() {
            let items: Vec<ListItem> = vec![ListItem::new(Line::from(Span::styled(
                "  No reflog entries",
                Style::default().fg(Color::DarkGray),
            )))];
            let list = List::new(items).block(block);
            f.render_widget(list, area);
            return;
        }

        // Build set of matched reflog entry indices
        let (match_set, current_match_idx) = if shared.search.origin == SearchOrigin::Reflog {
            let set: HashSet<usize> = shared
                .search
                .matches
                .iter()
                .filter_map(|m| match m {
                    SearchMatch::ReflogEntry(idx) => Some(*idx),
                    _ => None,
                })
                .collect();
            let current = shared.search.current_match_idx.and_then(|ci| {
                match shared.search.matches.get(ci) {
                    Some(SearchMatch::ReflogEntry(idx)) => Some(*idx),
                    _ => None,
                }
            });
            (set, current)
        } else {
            (HashSet::new(), None)
        };

        let items: Vec<ListItem> = self
            .entries
            .iter()
            .enumerate()
            .map(|(idx, entry)| {
                let is_current = current_match_idx == Some(idx);
                let is_match = match_set.contains(&idx);
                let bg = if is_current {
                    Some(Color::Rgb(200, 120, 0))
                } else if is_match {
                    Some(Color::Rgb(60, 60, 0))
                } else {
                    None
                };
                let fg_override = if is_current { Some(Color::Black) } else { None };

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
        let selected_is_match = match_set.contains(&selected);

        let highlight_style = if selected_is_match {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD)
        };

        let list = List::new(items)
            .block(block)
            .highlight_style(highlight_style);

        let mut list_state = ListState::default();
        list_state.select(Some(selected));
        f.render_stateful_widget(list, area, &mut list_state);
    }
}
