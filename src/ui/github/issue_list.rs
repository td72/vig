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
    let is_focused = app.github.focused_pane == GhFocusedPane::IssueList;
    let border_color = if is_focused { Color::Cyan } else { Color::DarkGray };

    let block = Block::default()
        .title(" Issues ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    if app.github.issues_loading {
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
