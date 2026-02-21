use crate::core::app::{AppContext, SearchMatch, SearchOrigin};
use crate::core::pane::{FocusState, SelectPane};
use crate::git::page::refresh_diff;
use crate::git::state::{FocusedPane, GitState};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};
use std::collections::HashSet;

pub struct ReflogPane;

impl SelectPane<GitState> for ReflogPane {
    fn handle_key(&self, ctx: &mut AppContext, state: &mut GitState, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                state.set_focus(FocusedPane::BranchList);
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if !state.reflog.entries.is_empty()
                    && state.reflog.selected_idx + 1 < state.reflog.entries.len()
                {
                    state.reflog.selected_idx += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if state.reflog.selected_idx > 0 {
                    state.reflog.selected_idx -= 1;
                }
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = (state.reflog.view_height / 2).max(1) as usize;
                let new_idx = state.reflog.selected_idx.saturating_add(half);
                state.reflog.selected_idx =
                    new_idx.min(state.reflog.entries.len().saturating_sub(1));
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = (state.reflog.view_height / 2).max(1) as usize;
                state.reflog.selected_idx = state.reflog.selected_idx.saturating_sub(half);
            }
            KeyCode::Char('g') => {
                state.reflog.selected_idx = 0;
            }
            KeyCode::Char('G') => {
                if !state.reflog.entries.is_empty() {
                    state.reflog.selected_idx = state.reflog.entries.len() - 1;
                }
            }
            KeyCode::Enter => {
                if let Some(entry) = state.reflog.entries.get(state.reflog.selected_idx) {
                    state.diff_base_ref = Some(entry.full_hash.clone());
                    refresh_diff(ctx, state);
                }
            }
            _ => {}
        }
    }

    fn render(&self, f: &mut Frame, _ctx: &AppContext, state: &mut GitState, area: Rect) {
        state.reflog.view_height = area.height.saturating_sub(2);
        let border_color = if state.focused_pane == FocusedPane::Reflog {
            Color::Cyan
        } else {
            Color::DarkGray
        };

        let block = Block::default()
            .title(" Reflog ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color));

        if state.reflog.entries.is_empty() {
            let items: Vec<ListItem> = vec![ListItem::new(Line::from(Span::styled(
                "  No reflog entries",
                Style::default().fg(Color::DarkGray),
            )))];
            let list = List::new(items).block(block);
            f.render_widget(list, area);
            return;
        }

        // Build set of matched reflog entry indices
        let (match_set, current_match_idx) =
            if state.search.origin == SearchOrigin::Reflog {
                let set: HashSet<usize> = state
                    .search
                    .matches
                    .iter()
                    .filter_map(|m| match m {
                        SearchMatch::ReflogEntry(idx) => Some(*idx),
                        _ => None,
                    })
                    .collect();
                let current = state.search.current_match_idx.and_then(|ci| {
                    match state.search.matches.get(ci) {
                        Some(SearchMatch::ReflogEntry(idx)) => Some(*idx),
                        _ => None,
                    }
                });
                (set, current)
            } else {
                (HashSet::new(), None)
            };

        let items: Vec<ListItem> = state
            .reflog
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

        let selected = state.reflog.selected_idx;
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
