use crate::app::App;
use crate::github::state::GhFocusedPane;
use crate::core::pane::SelectPane;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};

pub struct GhIssueListPane;

impl SelectPane for GhIssueListPane {
    fn handle_key(&self, app: &mut App, key: KeyEvent) {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if !app.github.issues.is_empty()
                    && app.github.issue_selected_idx + 1 < app.github.issues.len()
                {
                    app.github.issue_selected_idx += 1;
                    app.github.load_selected_issue_detail();
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if app.github.issue_selected_idx > 0 {
                    app.github.issue_selected_idx -= 1;
                    app.github.load_selected_issue_detail();
                }
            }
            KeyCode::Char('g') => {
                app.github.issue_selected_idx = 0;
                app.github.load_selected_issue_detail();
            }
            KeyCode::Char('G') => {
                if !app.github.issues.is_empty() {
                    app.github.issue_selected_idx = app.github.issues.len() - 1;
                    app.github.load_selected_issue_detail();
                }
            }
            KeyCode::Char('i') | KeyCode::Enter => {
                if !app.github.issues.is_empty() {
                    app.github.previous_pane = GhFocusedPane::IssueList;
                    app.github.focused_pane = GhFocusedPane::Detail;
                    app.github.load_selected_issue_detail();
                }
            }
            KeyCode::Char('o') => {
                if let Some(issue) = app.github.issues.get(app.github.issue_selected_idx) {
                    let number = issue.number;
                    match crate::github::client::open_issue_in_browser(number) {
                        Ok(()) => {
                            app.status_message =
                                Some(format!("Opening issue #{number} in browser..."));
                        }
                        Err(e) => {
                            app.status_message = Some(format!("Failed to open browser: {e}"));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn render(&self, f: &mut Frame, app: &mut App, area: Rect) {
        let is_focused = app.github.focused_pane == GhFocusedPane::IssueList;
        let border_color = if is_focused { Color::Cyan } else { Color::DarkGray };

        let block = Block::default()
            .title(" Issues ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color));

        if app.github.issues_loading && app.github.issues.is_empty() {
            let items = vec![ListItem::new(Line::from(Span::styled(
                "  Loading...",
                Style::default().fg(Color::DarkGray),
            )))];
            let list = List::new(items).block(block);
            f.render_widget(list, area);
            return;
        }

        if app.github.issues.is_empty() {
            let items = vec![ListItem::new(Line::from(Span::styled(
                "  No issues",
                Style::default().fg(Color::DarkGray),
            )))];
            let list = List::new(items).block(block);
            f.render_widget(list, area);
            return;
        }

        let items: Vec<ListItem> = app
            .github
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

        let mut state = ListState::default();
        if is_focused
            || (app.github.focused_pane == GhFocusedPane::Detail
                && app.github.previous_pane == GhFocusedPane::IssueList)
        {
            state.select(Some(app.github.issue_selected_idx));
        }
        f.render_stateful_widget(list, area, &mut state);
    }
}
