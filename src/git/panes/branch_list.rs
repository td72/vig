use crate::core::app::{App, SearchMatch, SearchOrigin};
use crate::git::state::FocusedPane;
use crate::core::pane::SelectPane;
use crossterm::event::{KeyCode, KeyEvent};
use std::collections::HashSet;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};

pub struct BranchListPane;

impl SelectPane for BranchListPane {
    fn handle_key(&self, app: &mut App, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                if app.git.diff_base_ref.is_some() {
                    app.git.diff_base_ref = None;
                    if let Err(e) = app.refresh_diff() {
                        app.ctx.status_message = Some(format!("Diff error: {e}"));
                    }
                }
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if !app.git.branch_list.branches.is_empty()
                    && app.git.branch_list.selected_idx + 1 < app.git.branch_list.branches.len()
                {
                    app.git.branch_list.selected_idx += 1;
                    app.git.update_branch_log();
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if app.git.branch_list.selected_idx > 0 {
                    app.git.branch_list.selected_idx -= 1;
                    app.git.update_branch_log();
                }
            }
            KeyCode::Enter => {
                app.open_branch_action_menu();
            }
            _ => {}
        }
    }

    fn render(&self, f: &mut Frame, app: &mut App, area: Rect) {
        let border_color = if app.git.focused_pane == FocusedPane::BranchList {
            Color::Cyan
        } else {
            Color::DarkGray
        };

        let block = Block::default()
            .title(" Branches ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color));

        if app.git.branch_list.branches.is_empty() {
            let items: Vec<ListItem> = vec![ListItem::new(Line::from(Span::styled(
                "  No branches",
                Style::default().fg(Color::DarkGray),
            )))];
            let list = List::new(items).block(block);
            f.render_widget(list, area);
            return;
        }

        // Build set of matched branch entry indices
        let (match_set, current_match_idx) = if app.git.search.origin == SearchOrigin::BranchList {
            let set: HashSet<usize> = app
                .git.search
                .matches
                .iter()
                .filter_map(|m| match m {
                    SearchMatch::BranchEntry(idx) => Some(*idx),
                    _ => None,
                })
                .collect();
            let current = app.git.search.current_match_idx.and_then(|ci| {
                match app.git.search.matches.get(ci) {
                    Some(SearchMatch::BranchEntry(idx)) => Some(*idx),
                    _ => None,
                }
            });
            (set, current)
        } else {
            (HashSet::new(), None)
        };

        let items: Vec<ListItem> = app
            .git.branch_list
            .branches
            .iter()
            .enumerate()
            .map(|(idx, branch)| {
                let is_current = current_match_idx == Some(idx);
                let is_match = match_set.contains(&idx);

                let mut spans = vec![Span::raw(" ")];

                let name_style = if is_current {
                    Style::default().fg(Color::Black).bg(Color::Rgb(200, 120, 0))
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

        let selected = app.git.branch_list.selected_idx;
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
