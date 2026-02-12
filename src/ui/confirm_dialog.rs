use crate::app::App;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

const BG: Color = Color::Rgb(30, 30, 30);

fn pad_line(line: Line<'static>, width: usize) -> Line<'static> {
    let content_len: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
    if content_len < width {
        let mut spans = line.spans;
        spans.push(Span::styled(
            " ".repeat(width - content_len),
            Style::default().bg(BG),
        ));
        Line::from(spans)
    } else {
        line
    }
}

/// Word-wrap text into lines of at most `width` characters.
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let mut result = Vec::new();
    for raw_line in text.lines() {
        if raw_line.is_empty() {
            result.push(String::new());
            continue;
        }
        let mut current = String::new();
        for word in raw_line.split_whitespace() {
            if current.is_empty() {
                current = word.to_string();
            } else if current.chars().count() + 1 + word.chars().count() <= width {
                current.push(' ');
                current.push_str(word);
            } else {
                result.push(current);
                current = word.to_string();
            }
        }
        if !current.is_empty() {
            result.push(current);
        }
    }
    if result.is_empty() {
        result.push(String::new());
    }
    result
}

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let dialog = match &app.error_dialog {
        Some(d) => d,
        None => return,
    };

    let dialog_width = 54u16.min(area.width.saturating_sub(4));
    let inner_w = dialog_width.saturating_sub(2) as usize;
    let text_w = inner_w.saturating_sub(2); // 1 char padding each side

    let msg_lines = wrap_text(&dialog.message, text_w);
    let total_lines = 1 + 1 + msg_lines.len() + 1 + 1; // title, blank, msg..., blank, dismiss
    let dialog_height = (total_lines as u16 + 2).min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(dialog_width)) / 2;
    let y = (area.height.saturating_sub(dialog_height)) / 2;
    let dialog_area = Rect::new(x, y, dialog_width, dialog_height);

    f.render_widget(Clear, dialog_area);

    let mut lines: Vec<Line> = Vec::new();

    // Title
    lines.push(pad_line(
        Line::from(Span::styled(
            format!(" {}", dialog.title),
            Style::default()
                .fg(Color::Red)
                .bg(BG)
                .add_modifier(Modifier::BOLD),
        )),
        inner_w,
    ));

    // Blank
    lines.push(pad_line(
        Line::from(Span::styled(String::new(), Style::default().bg(BG))),
        inner_w,
    ));

    // Message lines
    for msg_line in &msg_lines {
        lines.push(pad_line(
            Line::from(Span::styled(
                format!(" {msg_line}"),
                Style::default().fg(Color::White).bg(BG),
            )),
            inner_w,
        ));
    }

    // Blank
    lines.push(pad_line(
        Line::from(Span::styled(String::new(), Style::default().bg(BG))),
        inner_w,
    ));

    // Dismiss hint
    lines.push(pad_line(
        Line::from(Span::styled(
            " Press any key to dismiss".to_string(),
            Style::default().fg(Color::DarkGray).bg(BG),
        )),
        inner_w,
    ));

    // Fill remaining rows with background so nothing shows through
    let inner_h = dialog_height.saturating_sub(2) as usize; // minus top/bottom border
    while lines.len() < inner_h {
        lines.push(pad_line(
            Line::from(Span::styled(String::new(), Style::default().bg(BG))),
            inner_w,
        ));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red).bg(BG))
        .style(Style::default().bg(BG));

    let para = Paragraph::new(lines).block(block);
    f.render_widget(para, dialog_area);
}
