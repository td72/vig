use crate::app::{App, CursorPos, DiffSide, DiffViewMode, FocusedPane};
use crate::git::diff::{FileDiff, LineType, SideBySideRow};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

const GUTTER_WIDTH: usize = 5; // "1234 "
const SELECTION_BG: Color = Color::Rgb(60, 60, 100);
const CURSOR_FG: Color = Color::Black;
const CURSOR_BG: Color = Color::White;

/// Selection range info passed to rendering functions
struct SelectionInfo {
    start: CursorPos,
    end: CursorPos,
    mode: DiffViewMode,
    cursor: CursorPos,
}

pub fn render(f: &mut Frame, app: &mut App, area: Rect) {
    let border_color = if app.focused_pane == FocusedPane::DiffView {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let mode_indicator = match app.diff_view_mode {
        DiffViewMode::Normal => " Diff [NORMAL] ",
        DiffViewMode::Visual => " Diff [VISUAL] ",
        DiffViewMode::VisualLine => " Diff [V-LINE] ",
        DiffViewMode::Scroll => " Diff ",
    };

    let block = Block::default()
        .title(mode_indicator)
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

    // Build selection info if in visual mode
    let selection = build_selection_info(app);

    let (left_lines, right_lines) = build_side_by_side_lines(
        &file,
        left_width as usize,
        right_width as usize,
        app.diff_scroll_x,
        &selection,
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

fn build_selection_info(app: &App) -> Option<SelectionInfo> {
    match app.diff_view_mode {
        DiffViewMode::Normal => Some(SelectionInfo {
            start: app.cursor_pos,
            end: app.cursor_pos,
            mode: DiffViewMode::Normal,
            cursor: app.cursor_pos,
        }),
        DiffViewMode::Visual => {
            let anchor = app.visual_anchor?;
            let (start, end) = if anchor.row < app.cursor_pos.row
                || (anchor.row == app.cursor_pos.row && anchor.col <= app.cursor_pos.col)
            {
                (anchor, app.cursor_pos)
            } else {
                (app.cursor_pos, anchor)
            };
            Some(SelectionInfo {
                start,
                end,
                mode: DiffViewMode::Visual,
                cursor: app.cursor_pos,
            })
        }
        DiffViewMode::VisualLine => {
            let anchor = app.visual_anchor?;
            let start_row = anchor.row.min(app.cursor_pos.row);
            let end_row = anchor.row.max(app.cursor_pos.row);
            Some(SelectionInfo {
                start: CursorPos { row: start_row, col: 0, side: app.cursor_pos.side },
                end: CursorPos { row: end_row, col: usize::MAX, side: app.cursor_pos.side },
                mode: DiffViewMode::VisualLine,
                cursor: app.cursor_pos,
            })
        }
        DiffViewMode::Scroll => None,
    }
}

fn build_side_by_side_lines<'a>(
    file: &FileDiff,
    left_width: usize,
    right_width: usize,
    scroll_x: u16,
    selection: &Option<SelectionInfo>,
) -> (Vec<Line<'a>>, Vec<Line<'a>>) {
    let mut left_lines = Vec::new();
    let mut right_lines = Vec::new();
    let mut row_idx: usize = 0;

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

        // Apply selection to hunk header if needed
        if let Some(sel) = selection {
            if sel.cursor.side == DiffSide::Left {
                let idx = left_lines.len() - 1;
                left_lines[idx] = apply_selection_to_line(
                    &hunk.header, row_idx, left_width, scroll_x as usize, sel, header_style, true,
                );
            }
            if sel.cursor.side == DiffSide::Right {
                let idx = right_lines.len() - 1;
                right_lines[idx] = apply_selection_to_line(
                    &hunk.header, row_idx, right_width, scroll_x as usize, sel, header_style, false,
                );
            }
        }

        row_idx += 1;

        for row in &hunk.rows {
            let (left, right) = render_row(row, left_width, right_width, scroll_x as usize, row_idx, selection);
            left_lines.push(left);
            right_lines.push(right);
            row_idx += 1;
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
    row_idx: usize,
    selection: &Option<SelectionInfo>,
) -> (Line<'a>, Line<'a>) {
    let left = render_side_with_selection(
        row.left.as_ref(), row.line_type, true, left_width, scroll_x, row_idx, selection,
    );
    let right = render_side_with_selection(
        row.right.as_ref(), row.line_type, false, right_width, scroll_x, row_idx, selection,
    );
    (left, right)
}

fn render_side_with_selection<'a>(
    side: Option<&crate::git::diff::SideLine>,
    line_type: LineType,
    is_left: bool,
    width: usize,
    scroll_x: usize,
    row_idx: usize,
    selection: &Option<SelectionInfo>,
) -> Line<'a> {
    match side {
        Some(line) => {
            let content_width = width.saturating_sub(GUTTER_WIDTH);
            let gutter = format!("{:>4} ", line.line_no);
            let (fg, bg) = line_colors(line_type, is_left);
            let base_style = style_for(fg, bg);

            let sel_side = selection.as_ref().map(|s| s.cursor.side);
            let on_active_side = match (is_left, sel_side) {
                (true, Some(DiffSide::Left)) => true,
                (false, Some(DiffSide::Right)) => true,
                _ => false,
            };

            if on_active_side {
                if let Some(sel) = selection {
                    let content = &line.content;
                    let spans = build_highlighted_spans(
                        content, row_idx, content_width, scroll_x, sel, base_style,
                    );
                    let mut all_spans = vec![
                        Span::styled(gutter, Style::default().fg(Color::DarkGray)),
                    ];
                    all_spans.extend(spans);
                    return Line::from(all_spans);
                }
            }

            let content = scroll_content(&line.content, scroll_x, content_width);
            Line::from(vec![
                Span::styled(gutter, Style::default().fg(Color::DarkGray)),
                Span::styled(pad_to_width(&content, content_width), base_style),
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

/// Build spans for a content area with cursor/selection highlighting
fn build_highlighted_spans<'a>(
    content: &str,
    row_idx: usize,
    content_width: usize,
    scroll_x: usize,
    sel: &SelectionInfo,
    base_style: Style,
) -> Vec<Span<'a>> {
    let chars: Vec<char> = content.chars().collect();
    // Pad to content_width
    let mut display: Vec<char> = Vec::with_capacity(content_width);
    let start = scroll_x.min(chars.len());
    for i in start..(start + content_width) {
        if i < chars.len() {
            display.push(chars[i]);
        } else {
            display.push(' ');
        }
    }

    // Determine which columns (in content coords, pre-scroll) are selected
    let mut spans = Vec::new();
    let mut i = 0;
    while i < display.len() {
        let content_col = i + scroll_x;
        let is_cursor = sel.cursor.row == row_idx && sel.cursor.col == content_col;
        let is_selected = is_in_selection(row_idx, content_col, sel);

        // Find run of chars with same highlight state
        let mut j = i + 1;
        while j < display.len() {
            let cc = j + scroll_x;
            let next_cursor = sel.cursor.row == row_idx && sel.cursor.col == cc;
            let next_selected = is_in_selection(row_idx, cc, sel);
            if next_cursor != is_cursor || next_selected != is_selected {
                break;
            }
            j += 1;
        }

        let text: String = display[i..j].iter().collect();
        let style = if is_cursor {
            Style::default().fg(CURSOR_FG).bg(CURSOR_BG)
        } else if is_selected {
            base_style.bg(SELECTION_BG)
        } else {
            base_style
        };
        spans.push(Span::styled(text, style));
        i = j;
    }

    spans
}

fn is_in_selection(row: usize, col: usize, sel: &SelectionInfo) -> bool {
    match sel.mode {
        DiffViewMode::Normal => false,
        DiffViewMode::VisualLine => row >= sel.start.row && row <= sel.end.row,
        DiffViewMode::Visual => {
            if row < sel.start.row || row > sel.end.row {
                return false;
            }
            if sel.start.row == sel.end.row {
                col >= sel.start.col && col <= sel.end.col
            } else if row == sel.start.row {
                col >= sel.start.col
            } else if row == sel.end.row {
                col <= sel.end.col
            } else {
                true
            }
        }
        DiffViewMode::Scroll => false,
    }
}

fn apply_selection_to_line<'a>(
    content: &str,
    row_idx: usize,
    width: usize,
    scroll_x: usize,
    sel: &SelectionInfo,
    base_style: Style,
    _is_left: bool,
) -> Line<'a> {
    let spans = build_highlighted_spans(content, row_idx, width, scroll_x, sel, base_style);
    Line::from(spans)
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
