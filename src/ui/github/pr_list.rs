use crate::app::App;
use crate::github::state::GhFocusedPane;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let is_focused = app.github.focused_pane == GhFocusedPane::PrList;
    let border_color = if is_focused { Color::Cyan } else { Color::DarkGray };

    let block = Block::default()
        .title(" Pull Requests ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    if app.github.prs_loading {
        let items = vec![ListItem::new(Line::from(Span::styled(
            "  Loading...",
            Style::default().fg(Color::DarkGray),
        )))];
        let list = List::new(items).block(block);
        f.render_widget(list, area);
        return;
    }

    if app.github.prs.is_empty() {
        let items = vec![ListItem::new(Line::from(Span::styled(
            "  No pull requests",
            Style::default().fg(Color::DarkGray),
        )))];
        let list = List::new(items).block(block);
        f.render_widget(list, area);
        return;
    }

    let items: Vec<ListItem> = app
        .github
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

    let mut state = ListState::default();
    if is_focused
        || (app.github.focused_pane == GhFocusedPane::Detail
            && app.github.previous_pane == GhFocusedPane::PrList)
    {
        state.select(Some(app.github.pr_selected_idx));
    }
    f.render_stateful_widget(list, area, &mut state);
}
