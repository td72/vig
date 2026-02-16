use crate::app::App;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

/// Render commit detail content (left border as separator inside the parent Git Log block).
pub fn render(f: &mut Frame, app: &mut App, area: Rect) {
    let separator = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = separator.inner(area);
    f.render_widget(separator, area);

    app.git_log.detail_view_height = inner.height;

    let commit = match app.git_log.commits.get(app.git_log.selected_idx) {
        Some(c) => c,
        None => {
            let para = Paragraph::new(Line::from(Span::styled(
                "  No commit selected",
                Style::default().fg(Color::DarkGray),
            )));
            f.render_widget(para, inner);
            return;
        }
    };

    let mut lines: Vec<Line> = Vec::new();

    // Commit hash
    lines.push(Line::from(vec![
        Span::styled("Commit: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            commit.full_hash.clone(),
            Style::default().fg(Color::Yellow),
        ),
    ]));

    // Parent hashes
    if !commit.parent_ids.is_empty() {
        let parent_spans: Vec<Span> = commit
            .parent_ids
            .iter()
            .enumerate()
            .flat_map(|(i, pid)| {
                let short = &pid[..7.min(pid.len())];
                let mut v = Vec::new();
                if i > 0 {
                    v.push(Span::raw(" "));
                }
                v.push(Span::styled(
                    short.to_string(),
                    Style::default().fg(Color::Yellow),
                ));
                v
            })
            .collect();
        let mut spans = vec![Span::styled(
            "Parent: ",
            Style::default().fg(Color::DarkGray),
        )];
        spans.extend(parent_spans);
        lines.push(Line::from(spans));
    }

    // Author
    lines.push(Line::from(vec![
        Span::styled("Author: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            commit.author.clone(),
            Style::default().fg(Color::Cyan),
        ),
    ]));

    // Date
    lines.push(Line::from(vec![
        Span::styled("Date:   ", Style::default().fg(Color::DarkGray)),
        Span::raw(commit.date.clone()),
    ]));

    // Blank line + full message
    lines.push(Line::from(""));
    for msg_line in commit.full_message.lines() {
        lines.push(Line::from(Span::styled(
            format!("  {msg_line}"),
            Style::default().add_modifier(Modifier::BOLD),
        )));
    }

    // Changed files separator
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "── Changed Files ──",
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    )));

    // Changed files list
    if app.git_log.detail_changed_files.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (no changes)",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for change in &app.git_log.detail_changed_files {
            let (status_char, status_color) = match change.status {
                'A' => ("A", Color::Green),
                'D' => ("D", Color::Red),
                'R' => ("R", Color::Blue),
                'C' => ("C", Color::Magenta),
                _ => ("M", Color::Yellow),
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {status_char} "),
                    Style::default().fg(status_color),
                ),
                Span::raw(change.path.clone()),
            ]));
        }
    }

    // Clamp scroll
    let total_lines = lines.len() as u16;
    let view_height = app.git_log.detail_view_height;
    let max_scroll = total_lines.saturating_sub(view_height);
    if app.git_log.detail_scroll > max_scroll {
        app.git_log.detail_scroll = max_scroll;
    }

    let para = Paragraph::new(lines)
        .scroll((app.git_log.detail_scroll, 0));
    f.render_widget(para, inner);
}
