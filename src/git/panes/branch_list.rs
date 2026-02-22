use crate::core::app::{AppContext, SearchMatch, SearchOrigin};
use crate::core::pane::SelectPane;
use crate::git::domain::branch_action;
use crate::git::page::refresh_diff;
use crate::git::state::{FocusedPane, GitState};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};
use std::collections::HashSet;

pub struct BranchListPane;

impl SelectPane<GitState> for BranchListPane {
    fn handle_key(&self, ctx: &mut AppContext, state: &mut GitState, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                if state.shared.diff_base_ref.is_some() {
                    state.shared.diff_base_ref = None;
                    refresh_diff(ctx, state);
                }
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if !state.branch_list.branches.is_empty()
                    && state.branch_list.selected_idx + 1 < state.branch_list.branches.len()
                {
                    state.branch_list.selected_idx += 1;
                    state.update_branch_log();
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if state.branch_list.selected_idx > 0 {
                    state.branch_list.selected_idx -= 1;
                    state.update_branch_log();
                }
            }
            KeyCode::Enter => {
                branch_action::open_branch_action_menu(state);
            }
            _ => {}
        }
    }

    fn render(&self, f: &mut Frame, _ctx: &AppContext, state: &mut GitState, area: Rect) {
        let border_color = if state.shared.focused_pane == FocusedPane::BranchList {
            Color::Cyan
        } else {
            Color::DarkGray
        };

        let block = Block::default()
            .title(" Branches ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color));

        if state.branch_list.branches.is_empty() {
            let items: Vec<ListItem> = vec![ListItem::new(Line::from(Span::styled(
                "  No branches",
                Style::default().fg(Color::DarkGray),
            )))];
            let list = List::new(items).block(block);
            f.render_widget(list, area);
            return;
        }

        // Build set of matched branch entry indices
        let (match_set, current_match_idx) =
            if state.shared.search.origin == SearchOrigin::BranchList {
                let set: HashSet<usize> = state
                    .shared
                    .search
                    .matches
                    .iter()
                    .filter_map(|m| match m {
                        SearchMatch::BranchEntry(idx) => Some(*idx),
                        _ => None,
                    })
                    .collect();
                let current = state.shared.search.current_match_idx.and_then(|ci| {
                    match state.shared.search.matches.get(ci) {
                        Some(SearchMatch::BranchEntry(idx)) => Some(*idx),
                        _ => None,
                    }
                });
                (set, current)
            } else {
                (HashSet::new(), None)
            };

        let items: Vec<ListItem> = state
            .branch_list
            .branches
            .iter()
            .enumerate()
            .map(|(idx, branch)| {
                let is_current = current_match_idx == Some(idx);
                let is_match = match_set.contains(&idx);

                let mut spans = vec![Span::raw(" ")];

                let name_style = if is_current {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Rgb(200, 120, 0))
                } else if is_match {
                    Style::default().bg(Color::Rgb(60, 60, 0))
                } else if branch.is_head {
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                if branch.is_head {
                    let star_style = if is_current {
                        Style::default()
                            .fg(Color::Black)
                            .bg(Color::Rgb(200, 120, 0))
                            .add_modifier(Modifier::BOLD)
                    } else if is_match {
                        Style::default()
                            .fg(Color::Green)
                            .bg(Color::Rgb(60, 60, 0))
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD)
                    };
                    spans.push(Span::styled("* ", star_style));
                    spans.push(Span::styled(branch.name.clone(), name_style));
                } else {
                    spans.push(Span::raw("  "));
                    spans.push(Span::styled(branch.name.clone(), name_style));
                }

                ListItem::new(Line::from(spans))
            })
            .collect();

        let selected = state.branch_list.selected_idx;
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
