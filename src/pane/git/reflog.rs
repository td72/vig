use crate::app::{App, FocusedPane, SearchMatch, SearchOrigin};
use crate::pane::SelectPane;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::HashSet;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};

pub struct ReflogPane;

impl SelectPane for ReflogPane {
    fn handle_key(&self, app: &mut App, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                app.set_focus(FocusedPane::BranchList);
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if !app.reflog.entries.is_empty()
                    && app.reflog.selected_idx + 1 < app.reflog.entries.len()
                {
                    app.reflog.selected_idx += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if app.reflog.selected_idx > 0 {
                    app.reflog.selected_idx -= 1;
                }
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = (app.reflog.view_height / 2).max(1) as usize;
                let new_idx = app.reflog.selected_idx.saturating_add(half);
                app.reflog.selected_idx =
                    new_idx.min(app.reflog.entries.len().saturating_sub(1));
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = (app.reflog.view_height / 2).max(1) as usize;
                app.reflog.selected_idx = app.reflog.selected_idx.saturating_sub(half);
            }
            KeyCode::Char('g') => {
                app.reflog.selected_idx = 0;
            }
            KeyCode::Char('G') => {
                if !app.reflog.entries.is_empty() {
                    app.reflog.selected_idx = app.reflog.entries.len() - 1;
                }
            }
            KeyCode::Enter => {
                if let Some(entry) = app.reflog.entries.get(app.reflog.selected_idx) {
                    app.diff_base_ref = Some(entry.full_hash.clone());
                    if let Err(e) = app.refresh_diff() {
                        app.status_message = Some(format!("Diff error: {e}"));
                    }
                }
            }
            _ => {}
        }
    }

    fn render(&self, f: &mut Frame, app: &mut App, area: Rect) {
        app.reflog.view_height = area.height.saturating_sub(2); // minus borders
        let border_color = if app.focused_pane == FocusedPane::Reflog {
            Color::Cyan
        } else {
            Color::DarkGray
        };

        let block = Block::default()
            .title(" Reflog ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color));

        if app.reflog.entries.is_empty() {
            let items: Vec<ListItem> = vec![ListItem::new(Line::from(Span::styled(
                "  No reflog entries",
                Style::default().fg(Color::DarkGray),
            )))];
            let list = List::new(items).block(block);
            f.render_widget(list, area);
            return;
        }

        // Build set of matched reflog entry indices
        let (match_set, current_match_idx) = if app.search.origin == SearchOrigin::Reflog {
            let set: HashSet<usize> = app
                .search
                .matches
                .iter()
                .filter_map(|m| match m {
                    SearchMatch::ReflogEntry(idx) => Some(*idx),
                    _ => None,
                })
                .collect();
            let current = app.search.current_match_idx.and_then(|ci| {
                match app.search.matches.get(ci) {
                    Some(SearchMatch::ReflogEntry(idx)) => Some(*idx),
                    _ => None,
                }
            });
            (set, current)
        } else {
            (HashSet::new(), None)
        };

        let items: Vec<ListItem> = app
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
                    if let Some(bg) = bg { s = s.bg(bg); }
                    s
                };
                let selector_style = {
                    let mut s = Style::default().fg(fg_override.unwrap_or(Color::DarkGray));
                    if let Some(bg) = bg { s = s.bg(bg); }
                    s
                };
                let action_style = {
                    let mut s = Style::default()
                        .fg(fg_override.unwrap_or(Color::Cyan))
                        .add_modifier(Modifier::BOLD);
                    if let Some(bg) = bg { s = s.bg(bg); }
                    s
                };
                let msg_style = {
                    let mut s = Style::default();
                    if let Some(fg) = fg_override { s = s.fg(fg); }
                    if let Some(bg) = bg { s = s.bg(bg); }
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

        let selected = app.reflog.selected_idx;
        let selected_is_match = match_set.contains(&selected);

        let highlight_style = if selected_is_match {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD)
        };

        let list = List::new(items).block(block).highlight_style(highlight_style);

        let mut state = ListState::default();
        state.select(Some(selected));
        f.render_stateful_widget(list, area, &mut state);
    }
}
