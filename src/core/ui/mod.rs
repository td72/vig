pub mod confirm_dialog;
pub mod status_bar;
pub mod tail_pane;
pub mod trust_dialog;

use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

/// Pad a line with trailing spaces (styled with `bg`) so it fills `width`
/// columns. Used to give modal/dialog lines a solid background.
pub fn pad_line(line: Line<'static>, width: usize, bg: Color) -> Line<'static> {
    let content_len: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
    if content_len < width {
        let mut spans = line.spans;
        spans.push(Span::styled(
            " ".repeat(width - content_len),
            Style::default().bg(bg),
        ));
        Line::from(spans)
    } else {
        line
    }
}
