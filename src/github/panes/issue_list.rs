use crate::core::app::AppContext;
use crate::core::pane::SelectPane;
use crate::github::state::{GhFocusedPane, GitHubState};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};

pub struct GhIssueListPane;

impl SelectPane<GitHubState> for GhIssueListPane {
    fn handle_key(&self, ctx: &mut AppContext, state: &mut GitHubState, key: KeyEvent) {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if !state.issues.is_empty() && state.issue_selected_idx + 1 < state.issues.len() {
                    state.issue_selected_idx += 1;
                    state.load_selected_issue_detail();
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if state.issue_selected_idx > 0 {
                    state.issue_selected_idx -= 1;
                    state.load_selected_issue_detail();
                }
            }
            KeyCode::Char('g') => {
                state.issue_selected_idx = 0;
                state.load_selected_issue_detail();
            }
            KeyCode::Char('G') => {
                if !state.issues.is_empty() {
                    state.issue_selected_idx = state.issues.len() - 1;
                    state.load_selected_issue_detail();
                }
            }
            KeyCode::Char('i') | KeyCode::Enter => {
                if !state.issues.is_empty() {
                    state.previous_pane = GhFocusedPane::IssueList;
                    state.focused_pane = GhFocusedPane::Detail;
                    state.load_selected_issue_detail();
                }
            }
            KeyCode::Char('o') => {
                if let Some(issue) = state.issues.get(state.issue_selected_idx) {
                    let number = issue.number;
                    match crate::github::client::open_issue_in_browser(number) {
                        Ok(()) => {
                            ctx.status_message =
                                Some(format!("Opening issue #{number} in browser..."));
                        }
                        Err(e) => {
                            ctx.status_message = Some(format!("Failed to open browser: {e}"));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn render(&self, f: &mut Frame, _ctx: &AppContext, state: &mut GitHubState, area: Rect) {
        let is_focused = state.focused_pane == GhFocusedPane::IssueList;
        let border_color = if is_focused {
            Color::Cyan
        } else {
            Color::DarkGray
        };

        let block = Block::default()
            .title(" Issues ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color));

        if state.issues_loading && state.issues.is_empty() {
            let items = vec![ListItem::new(Line::from(Span::styled(
                "  Loading...",
                Style::default().fg(Color::DarkGray),
            )))];
            let list = List::new(items).block(block);
            f.render_widget(list, area);
            return;
        }

        if state.issues.is_empty() {
            let items = vec![ListItem::new(Line::from(Span::styled(
                "  No issues",
                Style::default().fg(Color::DarkGray),
            )))];
            let list = List::new(items).block(block);
            f.render_widget(list, area);
            return;
        }

        let items: Vec<ListItem> = state
            .issues
            .iter()
            .map(|issue| {
                let icon = if issue.state == "OPEN" { "●" } else { "✓" };
                let icon_color = if issue.state == "OPEN" {
                    Color::Green
                } else {
                    Color::Red
                };

                ListItem::new(Line::from(vec![
                    Span::raw(" "),
                    Span::styled(icon, Style::default().fg(icon_color)),
                    Span::raw(" "),
                    Span::styled(
                        format!("#{}", issue.number),
                        Style::default().fg(Color::Yellow),
                    ),
                    Span::raw(" "),
                    Span::raw(&issue.title),
                ]))
            })
            .collect();

        let highlight_style = Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD);

        let list = List::new(items)
            .block(block)
            .highlight_style(highlight_style);

        let mut list_state = ListState::default();
        if is_focused
            || (state.focused_pane == GhFocusedPane::Detail
                && state.previous_pane == GhFocusedPane::IssueList)
        {
            list_state.select(Some(state.issue_selected_idx));
        }
        f.render_stateful_widget(list, area, &mut list_state);
    }
}
