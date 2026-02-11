use crate::app::{App, CursorPos, DiffSide, DiffViewMode, FocusedPane, SearchMatch};
use crate::git::diff::{FileDiff, LineType, SideBySideRow};
use std::collections::HashMap;
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
const SEARCH_MATCH_BG: Color = Color::Rgb(60, 60, 0);
const SEARCH_CURRENT_BG: Color = Color::Rgb(200, 120, 0);
const SEARCH_CURRENT_FG: Color = Color::Black;

/// Pre-computed search highlight info for the current file
struct SearchHighlightInfo {
    /// row_idx → Vec<(col_start, col_end, is_current, side)>
    row_matches: HashMap<usize, Vec<(usize, usize, bool, DiffSide)>>,
}

impl SearchHighlightInfo {
    fn from_app(app: &App) -> Option<Self> {
        let query = app.search.query.as_ref()?;
        if query.is_empty() || app.search.matches.is_empty() {
            return None;
        }

        let current_idx = app.search.current_match_idx;
        let mut row_matches: HashMap<usize, Vec<(usize, usize, bool, DiffSide)>> = HashMap::new();

        for (i, m) in app.search.matches.iter().enumerate() {
            if let SearchMatch::DiffLine {
                row,
                col_start,
                col_end,
                side,
            } = m
            {
                let is_current = current_idx == Some(i);
                row_matches
                    .entry(*row)
                    .or_default()
                    .push((*col_start, *col_end, is_current, *side));
            }
        }

        Some(Self { row_matches })
    }

    /// Check if a character at (row, col) on the given side has a search highlight.
    /// Returns Some(is_current) if highlighted, None otherwise.
    fn get_highlight(&self, row_idx: usize, col: usize, is_left: bool) -> Option<bool> {
        let side = if is_left { DiffSide::Left } else { DiffSide::Right };
        if let Some(matches) = self.row_matches.get(&row_idx) {
            for &(col_start, col_end, is_current, match_side) in matches {
                if match_side == side && col >= col_start && col < col_end {
                    return Some(is_current);
                }
            }
        }
        None
    }
}

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

    // Reserve 1 line at bottom for status line
    let content_area = Rect {
        height: inner.height.saturating_sub(1),
        ..inner
    };
    let statusline_area = Rect {
        y: inner.y + content_area.height,
        height: 1.min(inner.height),
        ..inner
    };

    // Ensure syntax highlighting covers the visible range (incremental)
    let visible_end = (app.diff_scroll_y as usize) + (content_area.height as usize) + 1;
    app.ensure_file_highlight(&file, visible_end);

    // Split content area: left half | separator | right half
    let left_width = (content_area.width.saturating_sub(1)) / 2;
    let right_width = content_area.width.saturating_sub(left_width + 1);
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(left_width),
            Constraint::Length(1),
            Constraint::Length(right_width),
        ])
        .split(content_area);

    // Build selection info if in visual mode
    let selection = build_selection_info(app);

    // Build search highlight info
    let search_hl = SearchHighlightInfo::from_app(app);

    // Access cached highlight colors by reference (no clone)
    let (left_lines, right_lines) = {
        let empty: Vec<Vec<Color>> = Vec::new();
        let (lc, rc) = match &app.highlight_cache {
            Some(c) => (&c.left_colors, &c.right_colors),
            None => (&empty, &empty),
        };
        build_side_by_side_lines(
            &file,
            left_width as usize,
            right_width as usize,
            app.diff_scroll_x,
            &selection,
            lc,
            rc,
            &search_hl,
        )
    };

    let total_lines = left_lines.len() as u16;
    app.diff_total_lines = total_lines;
    app.diff_view_height = content_area.height;

    let left_para = Paragraph::new(left_lines).scroll((app.diff_scroll_y, 0));
    f.render_widget(left_para, panes[0]);

    // Separator
    let sep_lines: Vec<Line> = (0..content_area.height)
        .map(|_| Line::from(Span::styled("│", Style::default().fg(Color::DarkGray))))
        .collect();
    let sep = Paragraph::new(sep_lines).scroll((0, 0));
    f.render_widget(sep, panes[1]);

    let right_para = Paragraph::new(right_lines).scroll((app.diff_scroll_y, 0));
    f.render_widget(right_para, panes[2]);

    // Status line
    render_diff_statusline(f, app, &file.path, total_lines, statusline_area);
}

fn render_diff_statusline(f: &mut Frame, app: &App, file_path: &str, total_lines: u16, area: Rect) {
    let width = area.width as usize;

    // Mode badge
    let (mode_label, mode_style) = match app.diff_view_mode {
        DiffViewMode::Scroll => ("SCROLL", Style::default().fg(Color::Black).bg(Color::DarkGray)),
        DiffViewMode::Normal => ("NORMAL", Style::default().fg(Color::Black).bg(Color::Cyan)),
        DiffViewMode::Visual => ("VISUAL", Style::default().fg(Color::Black).bg(Color::Magenta)),
        DiffViewMode::VisualLine => ("V-LINE", Style::default().fg(Color::Black).bg(Color::Magenta)),
    };

    // File type from extension
    let filetype = file_path
        .rsplit('.')
        .next()
        .filter(|ext| ext.len() < 10 && !ext.contains('/'))
        .unwrap_or("");

    // Side indicator
    let side = match app.diff_view_mode {
        DiffViewMode::Scroll => "",
        _ => match app.cursor_pos.side {
            DiffSide::Left => "LEFT",
            DiffSide::Right => "RIGHT",
        },
    };

    // Cursor position / scroll percentage
    let position_info = match app.diff_view_mode {
        DiffViewMode::Scroll => {
            if total_lines == 0 {
                "Empty".to_string()
            } else if total_lines <= app.diff_view_height {
                "All".to_string()
            } else if app.diff_scroll_y == 0 {
                "Top".to_string()
            } else if app.diff_scroll_y >= total_lines.saturating_sub(app.diff_view_height) {
                "Bot".to_string()
            } else {
                format!("{}%", app.diff_scroll_y as u32 * 100 / total_lines.saturating_sub(1) as u32)
            }
        }
        _ => {
            format!("{}:{}", app.cursor_pos.row + 1, app.cursor_pos.col + 1)
        }
    };

    // Build left part: " MODE  filetype  side "
    let mut spans = vec![
        Span::styled(format!(" {mode_label} "), mode_style),
    ];
    if !filetype.is_empty() {
        spans.push(Span::styled(
            format!(" {filetype} "),
            Style::default().fg(Color::White).bg(Color::Rgb(50, 50, 50)),
        ));
    }
    if !side.is_empty() {
        spans.push(Span::styled(
            format!(" {side} "),
            Style::default().fg(Color::White).bg(Color::Rgb(50, 50, 50)),
        ));
    }

    // Calculate left part width
    let left_len: usize = spans.iter().map(|s| s.content.chars().count()).sum();

    // Showcmd (pending key sequence)
    let mut showcmd = String::new();
    if let Some(c) = app.count {
        showcmd.push_str(&c.to_string());
    }
    if let Some(k) = app.pending_key {
        if k == 'w' {
            showcmd.push_str("Ctrl+w");
        } else {
            showcmd.push(k);
        }
    }

    // Right-aligned showcmd + position info
    let right_part = format!(" {position_info} ");
    let right_len = right_part.chars().count();
    let showcmd_part = if showcmd.is_empty() {
        String::new()
    } else {
        format!(" {showcmd} ")
    };
    let showcmd_len = showcmd_part.chars().count();
    let gap = width.saturating_sub(left_len + showcmd_len + right_len);

    spans.push(Span::styled(
        " ".repeat(gap),
        Style::default().bg(Color::Rgb(30, 30, 30)),
    ));
    if !showcmd_part.is_empty() {
        spans.push(Span::styled(
            showcmd_part,
            Style::default().fg(Color::Yellow).bg(Color::Rgb(30, 30, 30)),
        ));
    }
    spans.push(Span::styled(
        right_part,
        Style::default().fg(Color::White).bg(Color::Rgb(50, 50, 50)),
    ));

    let line = Line::from(spans);
    f.render_widget(Paragraph::new(line), area);
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
    left_colors: &[Vec<Color>],
    right_colors: &[Vec<Color>],
    search_hl: &Option<SearchHighlightInfo>,
) -> (Vec<Line<'a>>, Vec<Line<'a>>) {
    let mut left_lines = Vec::new();
    let mut right_lines = Vec::new();
    let mut row_idx: usize = 0;

    for hunk in &file.hunks {
        // Hunk header — no syntax highlighting for headers
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

        // Apply selection/search to hunk header if needed
        if let Some(sel) = selection {
            if sel.cursor.side == DiffSide::Left {
                let idx = left_lines.len() - 1;
                left_lines[idx] = apply_selection_to_line(
                    &hunk.header, row_idx, left_width, scroll_x as usize, sel, header_style, None,
                    search_hl, true,
                );
            }
            if sel.cursor.side == DiffSide::Right {
                let idx = right_lines.len() - 1;
                right_lines[idx] = apply_selection_to_line(
                    &hunk.header, row_idx, right_width, scroll_x as usize, sel, header_style, None,
                    search_hl, false,
                );
            }
        } else if search_hl.is_some() {
            // No selection but search highlights may apply
            let idx = left_lines.len() - 1;
            left_lines[idx] = apply_search_to_line(
                &hunk.header, row_idx, left_width, scroll_x as usize, header_style, None,
                search_hl, true,
            );
            let idx = right_lines.len() - 1;
            right_lines[idx] = apply_search_to_line(
                &hunk.header, row_idx, right_width, scroll_x as usize, header_style, None,
                search_hl, false,
            );
        }

        row_idx += 1;

        for row in &hunk.rows {
            // Colors are pre-expanded in cache; just get a slice reference
            let left_syntax = left_colors.get(row_idx).map(|v| v.as_slice());
            let right_syntax = right_colors.get(row_idx).map(|v| v.as_slice());
            let (left, right) = render_row(
                row, left_width, right_width, scroll_x as usize, row_idx, selection,
                left_syntax, right_syntax, search_hl,
            );
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
    left_syntax: Option<&[Color]>,
    right_syntax: Option<&[Color]>,
    search_hl: &Option<SearchHighlightInfo>,
) -> (Line<'a>, Line<'a>) {
    let left = render_side_with_selection(
        row.left.as_ref(), row.line_type, true, left_width, scroll_x, row_idx, selection,
        left_syntax, search_hl,
    );
    let right = render_side_with_selection(
        row.right.as_ref(), row.line_type, false, right_width, scroll_x, row_idx, selection,
        right_syntax, search_hl,
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
    syntax_colors: Option<&[Color]>,
    search_hl: &Option<SearchHighlightInfo>,
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
                    // In Normal mode, only the cursor row needs per-char spans;
                    // other rows can use the cheaper syntax-only path (unless search highlights exist).
                    let needs_highlight = match sel.mode {
                        DiffViewMode::Normal => sel.cursor.row == row_idx,
                        DiffViewMode::Visual | DiffViewMode::VisualLine => true,
                        DiffViewMode::Scroll => false,
                    };
                    let has_search = search_hl.as_ref().is_some_and(|sh| sh.row_matches.contains_key(&row_idx));
                    if needs_highlight || has_search {
                        let content = &line.content;
                        let spans = build_highlighted_spans(
                            content, row_idx, content_width, scroll_x, sel, base_style,
                            syntax_colors, search_hl, is_left,
                        );
                        let mut all_spans = vec![
                            Span::styled(gutter, Style::default().fg(Color::DarkGray)),
                        ];
                        all_spans.extend(spans);
                        return Line::from(all_spans);
                    }
                }
            }

            // Non-active side or scroll mode — still apply syntax highlighting + search
            let has_search = search_hl.as_ref().is_some_and(|sh| sh.row_matches.contains_key(&row_idx));
            if syntax_colors.is_some() || has_search {
                let syn_colors = syntax_colors.unwrap_or(&[]);
                let spans = build_syntax_spans(
                    &line.content, content_width, scroll_x, base_style, syn_colors,
                    search_hl, row_idx, is_left,
                );
                let mut all_spans = vec![
                    Span::styled(gutter, Style::default().fg(Color::DarkGray)),
                ];
                all_spans.extend(spans);
                return Line::from(all_spans);
            }

            let content = scroll_content(&line.content, scroll_x, content_width);
            Line::from(vec![
                Span::styled(gutter, Style::default().fg(Color::DarkGray)),
                Span::styled(pad_to_width(&content, content_width), base_style),
            ])
        }
        None => {
            Line::from(Span::styled(pad_to_width("", width), Style::default()))
        }
    }
}

/// Build spans with syntax fg colors but no cursor/selection (for scroll mode / inactive side).
fn build_syntax_spans<'a>(
    content: &str,
    content_width: usize,
    scroll_x: usize,
    base_style: Style,
    syntax_colors: &[Color],
    search_hl: &Option<SearchHighlightInfo>,
    row_idx: usize,
    is_left: bool,
) -> Vec<Span<'a>> {
    let chars: Vec<char> = content.chars().collect();
    let start = scroll_x.min(chars.len());

    let mut spans = Vec::new();
    let mut i = 0;
    while i < content_width {
        let content_idx = start + i;
        let ch = if content_idx < chars.len() {
            chars[content_idx]
        } else {
            ' '
        };
        let fg = if content_idx < syntax_colors.len() {
            syntax_colors[content_idx]
        } else {
            base_style.fg.unwrap_or(Color::Reset)
        };
        let search_highlight = search_hl.as_ref().and_then(|sh| sh.get_highlight(row_idx, content_idx, is_left));

        // Batch consecutive chars with same fg and same search state
        let mut j = i + 1;
        let mut run = String::new();
        run.push(ch);
        while j < content_width {
            let cidx = start + j;
            let next_ch = if cidx < chars.len() { chars[cidx] } else { ' ' };
            let next_fg = if cidx < syntax_colors.len() {
                syntax_colors[cidx]
            } else {
                base_style.fg.unwrap_or(Color::Reset)
            };
            let next_search = search_hl.as_ref().and_then(|sh| sh.get_highlight(row_idx, cidx, is_left));
            if next_fg != fg || next_search != search_highlight {
                break;
            }
            run.push(next_ch);
            j += 1;
        }

        let style = if let Some(is_current) = search_highlight {
            if is_current {
                base_style.fg(SEARCH_CURRENT_FG).bg(SEARCH_CURRENT_BG)
            } else {
                base_style.fg(fg).bg(SEARCH_MATCH_BG)
            }
        } else {
            base_style.fg(fg)
        };
        spans.push(Span::styled(run, style));
        i = j;
    }

    spans
}

/// Build spans for a content area with cursor/selection highlighting + optional syntax colors
fn build_highlighted_spans<'a>(
    content: &str,
    row_idx: usize,
    content_width: usize,
    scroll_x: usize,
    sel: &SelectionInfo,
    base_style: Style,
    syntax_colors: Option<&[Color]>,
    search_hl: &Option<SearchHighlightInfo>,
    is_left: bool,
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
        let search_highlight = search_hl.as_ref().and_then(|sh| sh.get_highlight(row_idx, content_col, is_left));
        // Get syntax fg for this character
        let syn_fg = syntax_colors.and_then(|sc| sc.get(content_col).copied());

        // Find run of chars with same highlight state AND same syntax color
        let mut j = i + 1;
        while j < display.len() {
            let cc = j + scroll_x;
            let next_cursor = sel.cursor.row == row_idx && sel.cursor.col == cc;
            let next_selected = is_in_selection(row_idx, cc, sel);
            let next_search = search_hl.as_ref().and_then(|sh| sh.get_highlight(row_idx, cc, is_left));
            let next_syn_fg = syntax_colors.and_then(|sc| sc.get(cc).copied());
            if next_cursor != is_cursor || next_selected != is_selected || next_syn_fg != syn_fg || next_search != search_highlight {
                break;
            }
            j += 1;
        }

        let text: String = display[i..j].iter().collect();
        let syn_fg_or_default = syn_fg.unwrap_or(base_style.fg.unwrap_or(Color::Reset));
        let style = if is_cursor {
            base_style.fg(CURSOR_FG).bg(CURSOR_BG)
        } else if let Some(is_current) = search_highlight {
            if is_current {
                base_style.fg(SEARCH_CURRENT_FG).bg(SEARCH_CURRENT_BG)
            } else {
                base_style.fg(syn_fg_or_default).bg(SEARCH_MATCH_BG)
            }
        } else if is_selected {
            base_style.fg(syn_fg_or_default).bg(SELECTION_BG)
        } else {
            base_style.fg(syn_fg_or_default)
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
    syntax_colors: Option<&[Color]>,
    search_hl: &Option<SearchHighlightInfo>,
    is_left: bool,
) -> Line<'a> {
    let spans = build_highlighted_spans(content, row_idx, width, scroll_x, sel, base_style, syntax_colors, search_hl, is_left);
    Line::from(spans)
}

/// Apply search highlighting to a line without selection
fn apply_search_to_line<'a>(
    content: &str,
    row_idx: usize,
    width: usize,
    scroll_x: usize,
    base_style: Style,
    syntax_colors: Option<&[Color]>,
    search_hl: &Option<SearchHighlightInfo>,
    is_left: bool,
) -> Line<'a> {
    let syn_colors = syntax_colors.unwrap_or(&[]);
    let spans = build_syntax_spans(content, width, scroll_x, base_style, syn_colors, search_hl, row_idx, is_left);
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
                (Color::Green, Some(Color::Rgb(0, 40, 0)))
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
