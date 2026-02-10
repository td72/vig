use crate::app::{App, FocusedPane};
use crate::git::diff::{FileDiff, LineType, SideBySideRow};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

const GUTTER_WIDTH: usize = 5; // "1234 "

pub fn render(f: &mut Frame, app: &mut App, area: Rect) {
    let border_color = if app.focused_pane == FocusedPane::DiffView {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let block = Block::default()
        .title(" Diff ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let file = match app.selected_file() {
        Some(f) => f.clone(),
        None => {
            let msg = Paragraph::new(Line::from(Span::styled(
                "  No file selected",
                Style::default().fg(Color::DarkGray),
            )));
            f.render_widget(msg, inner);
            app.diff_total_lines = 0;
            return;
        }
    };

    if file.is_binary {
        let msg = Paragraph::new(Line::from(Span::styled(
            "  Binary file",
            Style::default().fg(Color::DarkGray),
        )));
        f.render_widget(msg, inner);
        app.diff_total_lines = 0;
        return;
    }

    // Split inner area: left half | separator | right half
    let left_width = (inner.width.saturating_sub(1)) / 2;
    let right_width = inner.width.saturating_sub(left_width + 1);
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(left_width),
            Constraint::Length(1),
            Constraint::Length(right_width),
        ])
        .split(inner);

    let (left_lines, right_lines) = build_side_by_side_lines(
        &file,
        left_width as usize,
        right_width as usize,
        app.diff_scroll_x,
    );

    let total_lines = left_lines.len() as u16;
    app.diff_total_lines = total_lines;
    app.diff_view_height = inner.height;

    let left_para = Paragraph::new(left_lines).scroll((app.diff_scroll_y, 0));
    f.render_widget(left_para, panes[0]);

    // Separator
    let sep_lines: Vec<Line> = (0..inner.height)
        .map(|_| Line::from(Span::styled("│", Style::default().fg(Color::DarkGray))))
        .collect();
    let sep = Paragraph::new(sep_lines).scroll((0, 0));
    f.render_widget(sep, panes[1]);

    let right_para = Paragraph::new(right_lines).scroll((app.diff_scroll_y, 0));
    f.render_widget(right_para, panes[2]);
}

fn build_side_by_side_lines<'a>(
    file: &FileDiff,
    left_width: usize,
    right_width: usize,
    scroll_x: u16,
) -> (Vec<Line<'a>>, Vec<Line<'a>>) {
    let mut left_lines = Vec::new();
    let mut right_lines = Vec::new();

    for hunk in &file.hunks {
        // Hunk header
        let header_style = Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD);
        left_lines.push(Line::from(Span::styled(
            pad_to_width(&hunk.header, left_width),
            header_style,
        )));
        right_lines.push(Line::from(Span::styled(
            pad_to_width(&hunk.header, right_width),
            header_style,
        )));

        for row in &hunk.rows {
            let (left, right) = render_row(row, left_width, right_width, scroll_x as usize);
            left_lines.push(left);
            right_lines.push(right);
        }
    }

    if left_lines.is_empty() {
        left_lines.push(Line::from(Span::styled(
            "  No changes",
            Style::default().fg(Color::DarkGray),
        )));
        right_lines.push(Line::from(Span::raw("")));
    }

    (left_lines, right_lines)
}

fn render_row<'a>(
    row: &SideBySideRow,
    left_width: usize,
    right_width: usize,
    scroll_x: usize,
) -> (Line<'a>, Line<'a>) {
    let left = render_side(row.left.as_ref(), row.line_type, true, left_width, scroll_x);
    let right = render_side(row.right.as_ref(), row.line_type, false, right_width, scroll_x);
    (left, right)
}

fn render_side<'a>(
    side: Option<&crate::git::diff::SideLine>,
    line_type: LineType,
    is_left: bool,
    width: usize,
    scroll_x: usize,
) -> Line<'a> {
    match side {
        Some(line) => {
            let content_width = width.saturating_sub(GUTTER_WIDTH);
            let gutter = format!("{:>4} ", line.line_no);
            let content = scroll_content(&line.content, scroll_x, content_width);
            let (fg, bg) = line_colors(line_type, is_left);
            Line::from(vec![
                Span::styled(gutter, Style::default().fg(Color::DarkGray)),
                Span::styled(pad_to_width(&content, content_width), style_for(fg, bg)),
            ])
        }
        None => {
            let bg = match line_type {
                LineType::Added => Some(Color::Rgb(0, 40, 0)),
                LineType::Deleted => Some(Color::Rgb(40, 0, 0)),
                _ => None,
            };
            let style = if let Some(bg) = bg {
                Style::default().bg(bg)
            } else {
                Style::default()
            };
            Line::from(Span::styled(pad_to_width("", width), style))
        }
    }
}

fn line_colors(line_type: LineType, is_left: bool) -> (Color, Option<Color>) {
    match line_type {
        LineType::Context => (Color::Reset, None),
        LineType::Added => {
            if is_left {
                (Color::Reset, Some(Color::Rgb(0, 40, 0)))
            } else {
                (Color::Green, Some(Color::Rgb(0, 40, 0)))
            }
        }
        LineType::Deleted => {
            if is_left {
                (Color::Red, Some(Color::Rgb(40, 0, 0)))
            } else {
                (Color::Reset, Some(Color::Rgb(40, 0, 0)))
            }
        }
        LineType::HunkHeader => (Color::Cyan, None),
    }
}

fn style_for(fg: Color, bg: Option<Color>) -> Style {
    let mut s = Style::default().fg(fg);
    if let Some(bg) = bg {
        s = s.bg(bg);
    }
    s
}

fn scroll_content(content: &str, scroll_x: usize, width: usize) -> String {
    let chars: Vec<char> = content.chars().collect();
    let start = scroll_x.min(chars.len());
    let end = (start + width).min(chars.len());
    chars[start..end].iter().collect()
}

fn pad_to_width(s: &str, width: usize) -> String {
    let char_count = s.chars().count();
    if char_count >= width {
        s.chars().take(width).collect()
    } else {
        let mut result = s.to_string();
        result.extend(std::iter::repeat(' ').take(width - char_count));
        result
    }
}
