use crate::core::app::AppContext;
use crate::github::state::{GhFocusedPane, GitHubState};
use crate::core::pane::SelectPane;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};

pub struct GhPrListPane;

impl SelectPane<GitHubState> for GhPrListPane {
    fn handle_key(&self, ctx: &mut AppContext, state: &mut GitHubState, key: KeyEvent) {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if !state.prs.is_empty()
                    && state.pr_selected_idx + 1 < state.prs.len()
                {
                    state.pr_selected_idx += 1;
                    state.load_selected_pr_detail();
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if state.pr_selected_idx > 0 {
                    state.pr_selected_idx -= 1;
                    state.load_selected_pr_detail();
                }
            }
            KeyCode::Char('g') => {
                state.pr_selected_idx = 0;
                state.load_selected_pr_detail();
            }
            KeyCode::Char('G') => {
                if !state.prs.is_empty() {
                    state.pr_selected_idx = state.prs.len() - 1;
                    state.load_selected_pr_detail();
                }
            }
            KeyCode::Char('i') | KeyCode::Enter => {
                if !state.prs.is_empty() {
                    state.previous_pane = GhFocusedPane::PrList;
                    state.focused_pane = GhFocusedPane::Detail;
                    state.load_selected_pr_detail();
                }
            }
            KeyCode::Char('o') => {
                if let Some(pr) = state.prs.get(state.pr_selected_idx) {
                    let number = pr.number;
                    match crate::github::client::open_pr_in_browser(number) {
                        Ok(()) => {
                            ctx.status_message =
                                Some(format!("Opening PR #{number} in browser..."));
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
        let is_focused = state.focused_pane == GhFocusedPane::PrList;
        let border_color = if is_focused { Color::Cyan } else { Color::DarkGray };

        let block = Block::default()
            .title(" Pull Requests ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color));

        if state.prs_loading && state.prs.is_empty() {
            let items = vec![ListItem::new(Line::from(Span::styled(
                "  Loading...",
                Style::default().fg(Color::DarkGray),
            )))];
            let list = List::new(items).block(block);
            f.render_widget(list, area);
            return;
        }

        if state.prs.is_empty() {
            let items = vec![ListItem::new(Line::from(Span::styled(
                "  No pull requests",
                Style::default().fg(Color::DarkGray),
            )))];
            let list = List::new(items).block(block);
            f.render_widget(list, area);
            return;
        }

        let items: Vec<ListItem> = state
            .prs
            .iter()
            .map(|pr| {
                let (icon, icon_color) = match pr.state.as_str() {
                    "MERGED" => ("⊕", Color::Magenta),
                    "CLOSED" => ("✓", Color::Red),
                    _ => ("●", Color::Green), // OPEN
                };

                let mut spans = vec![
                    Span::raw(" "),
                    Span::styled(icon, Style::default().fg(icon_color)),
                    Span::raw(" "),
                    Span::styled(
                        format!("#{}", pr.number),
                        Style::default().fg(Color::Yellow),
                    ),
                    Span::raw(" "),
                    Span::raw(&pr.title),
                ];

                // Review badge
                if let Some(ref decision) = pr.review_decision {
                    match decision.as_str() {
                        "APPROVED" => {
                            spans.push(Span::raw(" "));
                            spans.push(Span::styled("✓", Style::default().fg(Color::Green)));
                        }
                        "CHANGES_REQUESTED" => {
                            spans.push(Span::raw(" "));
                            spans.push(Span::styled("✗", Style::default().fg(Color::Red)));
                        }
                        _ => {}
                    }
                }

                // Draft badge
                if pr.is_draft {
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(
                        "[draft]",
                        Style::default().fg(Color::DarkGray),
                    ));
                }

                ListItem::new(Line::from(spans))
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
                && state.previous_pane == GhFocusedPane::PrList)
        {
            list_state.select(Some(state.pr_selected_idx));
        }
        f.render_stateful_widget(list, area, &mut list_state);
    }
}
