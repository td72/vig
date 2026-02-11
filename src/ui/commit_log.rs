use crate::app::App;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Git Log ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    if app.branch_selector.log_cache.is_empty() {
        let msg = Paragraph::new(Line::from(Span::styled(
            "  No commits",
            Style::default().fg(Color::DarkGray),
        )))
        .block(block);
        f.render_widget(msg, area);
        return;
    }

    let lines: Vec<Line> = app
        .branch_selector
        .log_cache
        .iter()
        .map(|commit| {
            Line::from(vec![
                Span::styled(
                    format!(" {} ", commit.short_hash),
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled(
                    format!("{} ", commit.date),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    format!("{:<12} ", commit.author),
                    Style::default().fg(Color::Cyan),
                ),
                Span::raw(commit.message.clone()),
            ])
        })
        .collect();

    let para = Paragraph::new(lines)
        .block(block)
        .scroll((app.branch_selector.log_scroll, 0));
    f.render_widget(para, area);
}
