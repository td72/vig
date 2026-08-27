use super::GhDetailViewPane;
use crate::core::pane::PaneShared;
use crate::core::theme;
use crate::github::domain::types::*;
use crate::github::state::{GhDetailContent, GhDetailPane};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Row, Table, TableState, Wrap},
    Frame,
};

/// Sort checks by workflow_name then name. Used for both rendering and key handling.
pub fn sorted_checks(detail: &GhPrDetail) -> Vec<&GhStatusCheck> {
    let checks = match detail.status_check_rollup {
        Some(ref checks) => checks.as_slice(),
        None => return Vec::new(),
    };
    let mut sorted: Vec<&GhStatusCheck> = checks.iter().collect();
    sorted.sort_by(|a, b| {
        let a_wf = a.workflow_name.as_deref().unwrap_or("");
        let b_wf = b.workflow_name.as_deref().unwrap_or("");
        a_wf.cmp(b_wf).then_with(|| a.name.cmp(&b.name))
    });
    sorted
}

/// Get meaningful reviews (non-empty body or non-COMMENTED state).
pub fn meaningful_reviews(reviews: &[GhReview]) -> Vec<&GhReview> {
    reviews
        .iter()
        .filter(|r| !r.body.is_empty() || r.state != "COMMENTED")
        .collect()
}

pub fn render(f: &mut Frame, dv: &mut GhDetailViewPane, shared: &PaneShared, area: Rect) {
    let is_focused = shared.focused_pane == dv.pane_id;
    let block = theme::pane_block("Detail", shared.focused_pane == dv.pane_id);

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Early return for non-loaded states
    match &dv.content {
        GhDetailContent::None => {
            let para = Paragraph::new(Line::from(Span::styled(
                "  Select an issue or PR to view details",
                Style::default().fg(Color::DarkGray),
            )));
            f.render_widget(para, inner);
            return;
        }
        GhDetailContent::Loading { kind, number } => {
            let label = match kind {
                crate::github::state::GhDetailKind::Issue => "issue",
                crate::github::state::GhDetailKind::Pr => "PR",
            };
            let para = Paragraph::new(Line::from(Span::styled(
                format!("  Loading {label} #{number}..."),
                Style::default().fg(Color::DarkGray),
            )));
            f.render_widget(para, inner);
            return;
        }
        GhDetailContent::Error(e) => {
            let para = Paragraph::new(Line::from(Span::styled(
                format!("  Error: {e}"),
                Style::default().fg(Color::Red),
            )));
            f.render_widget(para, inner);
            return;
        }
        _ => {}
    }

    // Build header lines
    let header_lines = match &dv.content {
        GhDetailContent::Issue(detail) => build_issue_header(detail),
        GhDetailContent::Pr(detail) => build_pr_header(detail),
        _ => unreachable!(),
    };

    let header_height = header_lines.len() as u16;

    // Layout: header (fixed) + content area (side-by-side)
    let vert =
        Layout::vertical([Constraint::Length(header_height), Constraint::Min(1)]).split(inner);

    // Render header
    let header_para = Paragraph::new(header_lines).wrap(Wrap { trim: false });
    f.render_widget(header_para, vert[0]);

    // Split content area into left and right columns
    let cols =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(vert[1]);

    let active_pane = dv.active_pane;

    // Left pane: Body
    let body_lines = match &dv.content {
        GhDetailContent::Issue(detail) => build_body_lines(&detail.body, inner_width(cols[0])),
        GhDetailContent::Pr(detail) => build_body_lines(&detail.body, inner_width(cols[0])),
        _ => unreachable!(),
    };
    render_pane(
        f,
        cols[0],
        "Body",
        body_lines,
        active_pane == GhDetailPane::Body,
        is_focused,
        dv.body.scroll_y,
    );

    // Right side
    match &dv.content {
        GhDetailContent::Issue(detail) => {
            // Issue: single Comments pane on the right
            let count = detail.comments.len();
            let title = format!("Comments ({count})");
            if active_pane == GhDetailPane::Body {
                dv.view_height = cols[0].height;
            } else {
                dv.view_height = cols[1].height;
            }
            let (comments_lines, sel_scroll) = build_comments_lines(
                &detail.comments,
                dv.comments.selected_idx,
                inner_width(cols[1]),
            );
            render_pane(
                f,
                cols[1],
                &title,
                comments_lines,
                active_pane == GhDetailPane::Comments,
                is_focused,
                sel_scroll + dv.comments.scroll_y,
            );
        }
        GhDetailContent::Pr(detail) => {
            // PR: split right into Checks / Reviews / Comments
            let right_rows = Layout::vertical([
                Constraint::Percentage(30),
                Constraint::Percentage(30),
                Constraint::Percentage(40),
            ])
            .split(cols[1]);

            dv.view_height = match active_pane {
                GhDetailPane::Body => cols[0].height,
                GhDetailPane::Status => right_rows[0].height,
                GhDetailPane::Reviews => right_rows[1].height,
                GhDetailPane::Comments => right_rows[2].height,
            };

            let checks_count = detail.status_check_rollup.as_ref().map_or(0, |c| c.len());
            let checks_title = format!("Checks ({checks_count})");
            render_status_table(
                f,
                right_rows[0],
                &checks_title,
                detail,
                active_pane == GhDetailPane::Status,
                is_focused,
                dv.status.selected_idx,
            );

            let review_count = detail
                .reviews
                .iter()
                .filter(|r| !r.body.is_empty() || r.state != "COMMENTED")
                .count();
            let reviews_title = format!("Reviews ({review_count})");
            let (reviews_lines, rev_scroll) = build_reviews_lines(
                &detail.reviews,
                dv.reviews.selected_idx,
                inner_width(cols[1]),
            );
            render_pane(
                f,
                right_rows[1],
                &reviews_title,
                reviews_lines,
                active_pane == GhDetailPane::Reviews,
                is_focused,
                rev_scroll + dv.reviews.scroll_y,
            );

            let comments_count = detail.comments.len();
            let comments_title = format!("Comments ({comments_count})");
            let (comments_lines, cmt_scroll) = build_comments_lines(
                &detail.comments,
                dv.comments.selected_idx,
                inner_width(cols[1]),
            );
            render_pane(
                f,
                right_rows[2],
                &comments_title,
                comments_lines,
                active_pane == GhDetailPane::Comments,
                is_focused,
                cmt_scroll + dv.comments.scroll_y,
            );
        }
        _ => unreachable!(),
    }
}

fn render_pane(
    f: &mut Frame,
    area: Rect,
    title: &str,
    lines: Vec<Line<'static>>,
    is_active: bool,
    is_detail_focused: bool,
    scroll: u16,
) {
    let para = Paragraph::new(lines)
        .block(sub_block(title, is_active, is_detail_focused))
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));
    f.render_widget(para, area);
}

/// Bordered block used for all sub-panes inside the detail view.
fn sub_block(title: &str, is_active: bool, is_detail_focused: bool) -> Block<'static> {
    Block::default()
        .title(pane_title(title, is_active, is_detail_focused))
        .borders(Borders::ALL)
        .border_style(pane_border_style(is_active, is_detail_focused))
}

fn pane_title(label: &str, is_active: bool, is_detail_focused: bool) -> Line<'static> {
    let style = if is_active && is_detail_focused {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    Line::from(Span::styled(format!(" {label} "), style))
}

fn pane_border_style(is_active: bool, is_detail_focused: bool) -> Style {
    if is_active && is_detail_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

// --- Helpers ---

fn format_date(iso: &str) -> &str {
    if iso.len() >= 10 {
        &iso[..10]
    } else {
        iso
    }
}

fn label_to_color(hex: &str) -> Color {
    let hex = hex.trim_start_matches('#');
    if hex.len() == 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&hex[0..2], 16),
            u8::from_str_radix(&hex[2..4], 16),
            u8::from_str_radix(&hex[4..6], 16),
        ) {
            return Color::Rgb(r, g, b);
        }
    }
    Color::White
}

fn build_label_spans(labels: &[GhLabel]) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for label in labels {
        let bg = label_to_color(&label.color);
        let (r, g, b) = match bg {
            Color::Rgb(r, g, b) => (r, g, b),
            _ => (255, 255, 255),
        };
        let brightness = (r as u32 * 299 + g as u32 * 587 + b as u32 * 114) / 1000;
        let fg = if brightness > 128 {
            Color::Black
        } else {
            Color::White
        };

        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            format!(" {} ", label.name),
            Style::default().fg(fg).bg(bg),
        ));
    }
    spans
}

// --- Header builders ---

/// Build the title line + the leading "author / date / state" badge spans shared
/// by issue and PR headers. Callers extend `spans` with type-specific badges.
fn build_detail_header_base(
    number: u64,
    title: &str,
    author: Option<&GhAuthor>,
    created_at: &str,
    state: &str,
) -> (Line<'static>, Vec<Span<'static>>) {
    let title_line = Line::from(vec![
        Span::raw("  "),
        Span::styled(
            format!("#{number} {title}"),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    let author_login = author.map(|a| a.login.as_str()).unwrap_or("unknown");
    let spans = vec![
        Span::raw(" "),
        badge(author_login, Color::Rgb(31, 111, 139)),
        Span::raw(" "),
        badge(format_date(created_at), Color::Rgb(68, 71, 78)),
        Span::raw(" "),
        state_badge(state),
    ];
    (title_line, spans)
}

fn build_issue_header(detail: &GhIssueDetail) -> Vec<Line<'static>> {
    let (title_line, mut spans) = build_detail_header_base(
        detail.number,
        &detail.title,
        detail.author.as_ref(),
        &detail.created_at,
        &detail.state,
    );
    spans.extend(build_label_spans(&detail.labels));
    vec![title_line, Line::from(spans)]
}

fn build_pr_header(detail: &GhPrDetail) -> Vec<Line<'static>> {
    let (title_line, mut spans) = build_detail_header_base(
        detail.number,
        &detail.title,
        detail.author.as_ref(),
        &detail.created_at,
        &detail.state,
    );
    spans.push(Span::raw(" "));
    spans.push(badge(&detail.head_ref_name, Color::Rgb(130, 80, 160)));
    spans.push(Span::raw(" "));
    spans.push(badge(
        &format!("+{}", detail.additions),
        Color::Rgb(35, 134, 54),
    ));
    spans.push(badge(
        &format!("-{}", detail.deletions),
        Color::Rgb(218, 54, 51),
    ));
    spans.push(Span::raw(" "));
    spans.push(badge(
        &format!("{} files", detail.changed_files),
        Color::Rgb(68, 71, 78),
    ));

    if let Some(ref decision) = detail.review_decision {
        let badge_opt = match decision.as_str() {
            "APPROVED" => Some(("✓ APPROVED", Color::Rgb(35, 134, 54))),
            "CHANGES_REQUESTED" => Some(("✗ CHANGES REQUESTED", Color::Rgb(218, 54, 51))),
            "REVIEW_REQUIRED" => Some(("◯ REVIEW REQUIRED", Color::Rgb(187, 128, 9))),
            _ => None,
        };
        if let Some((label, color)) = badge_opt {
            spans.push(Span::raw(" "));
            spans.push(badge(label, color));
        }
    }

    spans.extend(build_label_spans(&detail.labels));
    vec![title_line, Line::from(spans)]
}

fn badge(text: &str, bg: Color) -> Span<'static> {
    let fg = badge_fg(bg);
    Span::styled(format!(" {text} "), Style::default().fg(fg).bg(bg))
}

fn badge_fg(bg: Color) -> Color {
    let (r, g, b) = match bg {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => return Color::White,
    };
    let brightness = (r as u32 * 299 + g as u32 * 587 + b as u32 * 114) / 1000;
    if brightness > 128 {
        Color::Black
    } else {
        Color::White
    }
}

fn state_badge(state: &str) -> Span<'static> {
    let bg = match state {
        "OPEN" => Color::Rgb(35, 134, 54),
        "CLOSED" => Color::Rgb(218, 54, 51),
        "MERGED" => Color::Rgb(130, 80, 160),
        _ => Color::Rgb(110, 119, 129),
    };
    badge(state, bg)
}

// --- Content builders ---

/// Text width available inside a bordered sub-pane.
fn inner_width(area: Rect) -> usize {
    area.width.saturating_sub(2) as usize
}

fn build_body_lines(body: &str, max_width: usize) -> Vec<Line<'static>> {
    if body.is_empty() {
        return vec![Line::from(Span::styled(
            "  (no description)",
            Style::default().fg(Color::DarkGray),
        ))];
    }
    markdown_to_lines(body, "  ", max_width)
}

fn render_status_table(
    f: &mut Frame,
    area: Rect,
    title: &str,
    detail: &GhPrDetail,
    is_active: bool,
    is_detail_focused: bool,
    selected_idx: usize,
) {
    let block = sub_block(title, is_active, is_detail_focused);

    let sorted = sorted_checks(detail);

    if sorted.is_empty() {
        let para = Paragraph::new(Line::from(Span::styled(
            "  (no checks)",
            Style::default().fg(Color::DarkGray),
        )))
        .block(block);
        f.render_widget(para, area);
        return;
    }

    let dim = Style::default().fg(Color::DarkGray);
    let rows: Vec<Row> = sorted
        .iter()
        .map(|check| {
            let (icon, color) = check_icon(check);
            let workflow = check.workflow_name.as_deref().unwrap_or("");
            let (job, params) = parse_check_name(&check.name);
            let duration =
                format_duration(check.started_at.as_deref(), check.completed_at.as_deref());
            Row::new(vec![
                Line::from(Span::styled(icon, Style::default().fg(color))),
                Line::from(Span::styled(workflow.to_string(), dim)),
                Line::from(job.to_string()),
                Line::from(Span::styled(params.to_string(), dim)),
                Line::from(Span::styled(duration, dim)),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(2),
        Constraint::Length(12),
        Constraint::Length(8),
        Constraint::Fill(1),
        Constraint::Length(6),
    ];

    let highlight_style = if is_active && is_detail_focused {
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };

    let table = Table::new(rows, widths)
        .column_spacing(1)
        .block(block)
        .row_highlight_style(highlight_style);

    let mut state = TableState::default();
    if is_active && is_detail_focused {
        state.select(Some(selected_idx));
    }
    f.render_stateful_widget(table, area, &mut state);
}

/// Format duration between two ISO 8601 timestamps (e.g. "1m23s", "45s").
fn format_duration(started: Option<&str>, completed: Option<&str>) -> String {
    let (Some(s), Some(c)) = (started, completed) else {
        return String::new();
    };
    // Parse "2024-01-02T03:04:05Z" to seconds since epoch (UTC)
    let parse = |iso: &str| -> Option<i64> {
        if iso.len() < 19 {
            return None;
        }
        let y: i64 = iso[0..4].parse().ok()?;
        let mo: i64 = iso[5..7].parse().ok()?;
        let d: i64 = iso[8..10].parse().ok()?;
        let h: i64 = iso[11..13].parse().ok()?;
        let mi: i64 = iso[14..16].parse().ok()?;
        let se: i64 = iso[17..19].parse().ok()?;
        let mut days = 365 * y + y / 4 - y / 100 + y / 400;
        const MONTH_DAYS: [i64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
        for m in 1..mo {
            days += MONTH_DAYS[(m - 1) as usize];
        }
        if mo > 2 && (y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)) {
            days += 1;
        }
        days += d;
        Some(days * 86400 + h * 3600 + mi * 60 + se)
    };
    let Some(start_secs) = parse(s) else {
        return String::new();
    };
    let Some(end_secs) = parse(c) else {
        return String::new();
    };
    let diff = (end_secs - start_secs).max(0);
    let mins = diff / 60;
    let secs = diff % 60;
    if mins > 0 {
        format!("{mins}m{secs:02}s")
    } else {
        format!("{secs}s")
    }
}

/// Parse "job_name (param1, param2)" into ("job_name", "param1, param2").
/// If no parentheses, returns (name, "").
fn parse_check_name(name: &str) -> (&str, &str) {
    if let Some(paren_start) = name.find('(') {
        let job = name[..paren_start].trim();
        let rest = &name[paren_start + 1..];
        let params = rest.trim_end_matches(')').trim();
        (job, params)
    } else {
        (name.trim(), "")
    }
}

/// Look up a status key in a table and return the matching (icon, color), if any.
fn status_icon_lookup(
    key: &str,
    entries: &[(&'static str, &'static str, Color)],
) -> Option<(&'static str, Color)> {
    entries
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, icon, color)| (*icon, *color))
}

fn check_icon(check: &GhStatusCheck) -> (&'static str, Color) {
    const CONCLUSIONS: &[(&str, &str, Color)] = &[
        ("SUCCESS", "✓", Color::Green),
        ("FAILURE", "✗", Color::Red),
        ("NEUTRAL", "○", Color::DarkGray),
        ("SKIPPED", "○", Color::DarkGray),
    ];
    const STATUSES: &[(&str, &str, Color)] = &[
        ("IN_PROGRESS", "◐", Color::Yellow),
        ("QUEUED", "◯", Color::DarkGray),
        ("WAITING", "◯", Color::DarkGray),
    ];
    check
        .conclusion
        .as_deref()
        .and_then(|c| status_icon_lookup(c, CONCLUSIONS))
        .or_else(|| status_icon_lookup(&check.status, STATUSES))
        .unwrap_or(("?", Color::DarkGray))
}

fn review_icon(review: &GhReview) -> (&'static str, Color) {
    const ENTRIES: &[(&str, &str, Color)] = &[
        ("APPROVED", "✓", Color::Green),
        ("CHANGES_REQUESTED", "✗", Color::Red),
        ("COMMENTED", "💬", Color::DarkGray),
        ("DISMISSED", "⊘", Color::DarkGray),
    ];
    status_icon_lookup(&review.state, ENTRIES).unwrap_or(("?", Color::White))
}

/// Build a list of items where each entry has a one-line header followed by
/// a markdown-rendered body. Used by both reviews and comments.
/// Returns (lines, selected_header_line_offset).
fn build_items_lines<T>(
    items: &[&T],
    selected_idx: usize,
    empty_msg: &str,
    header_spans: impl Fn(&T) -> Vec<Span<'static>>,
    body: impl Fn(&T) -> &str,
    max_width: usize,
) -> (Vec<Line<'static>>, u16) {
    if items.is_empty() {
        return (
            vec![Line::from(Span::styled(
                empty_msg.to_string(),
                Style::default().fg(Color::DarkGray),
            ))],
            0,
        );
    }
    let sel_bg = Style::default().bg(Color::DarkGray);
    let mut lines = Vec::new();
    let mut sel_offset: u16 = 0;
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            lines.push(Line::from(""));
        }
        let is_sel = i == selected_idx;
        if is_sel {
            sel_offset = lines.len() as u16;
        }
        let mut header = Line::from(header_spans(item));
        if is_sel {
            header = header.style(sel_bg);
        }
        lines.push(header);
        let body_text = body(item);
        if !body_text.is_empty() {
            lines.extend(markdown_to_lines(body_text, "    ", max_width));
        }
    }
    (lines, sel_offset)
}

/// Returns (lines, selected_header_line_offset).
fn build_reviews_lines(
    reviews: &[GhReview],
    selected_idx: usize,
    max_width: usize,
) -> (Vec<Line<'static>>, u16) {
    let meaningful = meaningful_reviews(reviews);
    build_items_lines(
        &meaningful,
        selected_idx,
        "  (no reviews)",
        |review| {
            let (icon, color) = review_icon(review);
            let author = review
                .author
                .as_ref()
                .map(|a| a.login.as_str())
                .unwrap_or("unknown");
            vec![
                Span::raw("  "),
                Span::styled(icon, Style::default().fg(color)),
                Span::raw(" "),
                Span::styled(author.to_string(), Style::default().fg(Color::Cyan)),
                Span::raw("  "),
                Span::styled(review.state.clone(), Style::default().fg(color)),
            ]
        },
        |review| &review.body,
        max_width,
    )
}

/// Returns (lines, selected_header_line_offset).
fn build_comments_lines(
    comments: &[GhComment],
    selected_idx: usize,
    max_width: usize,
) -> (Vec<Line<'static>>, u16) {
    let items: Vec<&GhComment> = comments.iter().collect();
    build_items_lines(
        &items,
        selected_idx,
        "  (no comments)",
        |comment| {
            let author = comment
                .author
                .as_ref()
                .map(|a| a.login.as_str())
                .unwrap_or("unknown");
            vec![
                Span::raw("  "),
                Span::styled(author.to_string(), Style::default().fg(Color::Cyan)),
                Span::raw(format!("  {}", format_date(&comment.created_at))),
            ]
        },
        |comment| &comment.body,
        max_width,
    )
}

/// Render a markdown body as styled lines. `max_width` is the text width
/// available in the pane (including `padding`); block elements that cannot
/// wrap gracefully (tables) are fitted into it.
fn markdown_to_lines(text: &str, padding: &str, max_width: usize) -> Vec<Line<'static>> {
    use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

    let opts = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS | Options::ENABLE_TABLES;
    let parser = Parser::new_ext(text, opts);

    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut current_spans: Vec<Span<'static>> = Vec::new();
    let mut style_stack: Vec<Style> = Vec::new();
    let mut in_code_block = false;
    let mut in_heading = false;
    let mut in_list_item = false;
    let mut heading_style = Style::default();
    let code_style = Style::default().fg(Color::DarkGray);
    // Table cells are buffered until `TagEnd::Table` so that column widths can
    // be computed over the whole table before emitting any line.
    let mut table_aligns: Vec<pulldown_cmark::Alignment> = Vec::new();
    let mut table_rows: Vec<Vec<Vec<Span<'static>>>> = Vec::new();
    let mut table_row: Vec<Vec<Span<'static>>> = Vec::new();

    let flush_line = |lines: &mut Vec<Line<'static>>,
                      spans: &mut Vec<Span<'static>>,
                      padding: &str,
                      line_style: Style| {
        spans.insert(0, Span::raw(padding.to_string()));
        lines.push(Line::from(std::mem::take(spans)).style(line_style));
    };

    let current_style = |stack: &[Style]| -> Style {
        stack
            .iter()
            .copied()
            .fold(Style::default(), |acc, s| acc.patch(s))
    };

    for event in parser {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                if !lines.is_empty() {
                    flush_line(&mut lines, &mut current_spans, padding, Style::default());
                }
                in_heading = true;
                heading_style = match level {
                    HeadingLevel::H1 => Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                    HeadingLevel::H2 => Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                    HeadingLevel::H3 => Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD | Modifier::ITALIC),
                    _ => Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::ITALIC),
                };
                let prefix = match level {
                    HeadingLevel::H1 => "# ",
                    HeadingLevel::H2 => "## ",
                    HeadingLevel::H3 => "### ",
                    HeadingLevel::H4 => "#### ",
                    HeadingLevel::H5 => "##### ",
                    HeadingLevel::H6 => "###### ",
                };
                current_spans.push(Span::raw(prefix.to_string()));
            }
            Event::End(TagEnd::Heading(_)) => {
                flush_line(&mut lines, &mut current_spans, padding, heading_style);
                in_heading = false;
                heading_style = Style::default();
            }
            Event::Start(Tag::Paragraph)
                if !lines.is_empty() && !in_code_block && !in_list_item =>
            {
                flush_line(&mut lines, &mut current_spans, padding, Style::default());
            }
            Event::End(TagEnd::Paragraph) => {
                let style = if in_heading {
                    heading_style
                } else {
                    Style::default()
                };
                flush_line(&mut lines, &mut current_spans, padding, style);
            }
            Event::Start(Tag::CodeBlock(_)) => {
                in_code_block = true;
                flush_line(&mut lines, &mut current_spans, padding, Style::default());
            }
            Event::End(TagEnd::CodeBlock) => {
                if !current_spans.is_empty() {
                    flush_line(&mut lines, &mut current_spans, padding, Style::default());
                }
                in_code_block = false;
            }
            Event::Start(Tag::Emphasis) => {
                style_stack.push(Style::default().add_modifier(Modifier::ITALIC));
            }
            Event::End(TagEnd::Emphasis) => {
                style_stack.pop();
            }
            Event::Start(Tag::Strong) => {
                style_stack.push(Style::default().add_modifier(Modifier::BOLD));
            }
            Event::End(TagEnd::Strong) => {
                style_stack.pop();
            }
            Event::Start(Tag::Strikethrough) => {
                style_stack.push(Style::default().add_modifier(Modifier::CROSSED_OUT));
            }
            Event::End(TagEnd::Strikethrough) => {
                style_stack.pop();
            }
            Event::Start(Tag::List(_)) | Event::End(TagEnd::List(_)) => {}
            Event::Start(Tag::Item) => {
                if !current_spans.is_empty() {
                    flush_line(&mut lines, &mut current_spans, padding, Style::default());
                }
                in_list_item = true;
                current_spans.push(Span::raw("- "));
            }
            Event::End(TagEnd::Item) => {
                if !current_spans.is_empty() {
                    flush_line(&mut lines, &mut current_spans, padding, Style::default());
                }
                in_list_item = false;
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                style_stack.push(
                    Style::default()
                        .fg(Color::Blue)
                        .add_modifier(Modifier::UNDERLINED),
                );
                let _ = dest_url;
            }
            Event::End(TagEnd::Link) => {
                style_stack.pop();
            }
            Event::Start(Tag::BlockQuote(_)) => {
                style_stack.push(Style::default().fg(Color::DarkGray));
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                style_stack.pop();
            }
            Event::Text(t) => {
                if in_code_block {
                    for line in t.as_ref().lines() {
                        if !current_spans.is_empty() {
                            flush_line(&mut lines, &mut current_spans, padding, Style::default());
                        }
                        current_spans.push(Span::styled(line.to_string(), code_style));
                    }
                } else {
                    let style = current_style(&style_stack);
                    current_spans.push(Span::styled(t.into_string(), style));
                }
            }
            Event::Code(code) => {
                let style = Style::default()
                    .fg(Color::Yellow)
                    .bg(Color::Rgb(50, 50, 50));
                current_spans.push(Span::styled(format!(" {} ", code.as_ref()), style));
            }
            Event::SoftBreak => {
                current_spans.push(Span::raw(" "));
            }
            Event::HardBreak => {
                let style = if in_heading {
                    heading_style
                } else {
                    Style::default()
                };
                flush_line(&mut lines, &mut current_spans, padding, style);
            }
            Event::Rule => {
                flush_line(&mut lines, &mut current_spans, padding, Style::default());
                current_spans.push(Span::styled("───", Style::default().fg(Color::DarkGray)));
                flush_line(&mut lines, &mut current_spans, padding, Style::default());
            }
            Event::Start(Tag::Table(aligns)) => {
                if !current_spans.is_empty() {
                    flush_line(&mut lines, &mut current_spans, padding, Style::default());
                }
                table_aligns = aligns;
                table_rows.clear();
            }
            Event::Start(Tag::TableHead) | Event::Start(Tag::TableRow) => {
                table_row.clear();
            }
            Event::Start(Tag::TableCell) => {}
            Event::End(TagEnd::TableCell) => {
                table_row.push(std::mem::take(&mut current_spans));
            }
            Event::End(TagEnd::TableHead) | Event::End(TagEnd::TableRow) => {
                table_rows.push(std::mem::take(&mut table_row));
            }
            Event::End(TagEnd::Table) => {
                lines.extend(table_to_lines(
                    &table_aligns,
                    std::mem::take(&mut table_rows),
                    padding,
                    max_width.saturating_sub(padding.len()),
                ));
            }
            Event::TaskListMarker(checked) => {
                let marker = if checked { "[x] " } else { "[ ] " };
                if let Some(last) = current_spans.last_mut() {
                    if last.content.as_ref() == "- " {
                        *last = Span::raw(format!("- {marker}"));
                    }
                }
            }
            _ => {}
        }
    }
    if !current_spans.is_empty() {
        flush_line(&mut lines, &mut current_spans, padding, Style::default());
    }
    lines
}

/// Format a GFM table as fixed-width text lines.
///
/// The first row is the header (pulldown-cmark always emits `TableHead` first)
/// and is rendered bold, followed by a `───┼───` separator. Cells keep their
/// inline styling (code, emphasis, ...) and are padded to the column width
/// according to the column alignment.
///
/// If the natural width exceeds `max_width`, the widest columns are narrowed
/// (down to `MIN_COL_WIDTH`) until the table fits, and overflowing cells are
/// truncated with `…`. A table with too many columns to fit even at the
/// minimum width is left to the paragraph's wrapping.
fn table_to_lines(
    aligns: &[pulldown_cmark::Alignment],
    rows: Vec<Vec<Vec<Span<'static>>>>,
    padding: &str,
    max_width: usize,
) -> Vec<Line<'static>> {
    use pulldown_cmark::Alignment;

    let ncols = rows
        .iter()
        .map(Vec::len)
        .max()
        .unwrap_or(0)
        .max(aligns.len());
    if ncols == 0 || rows.is_empty() {
        return Vec::new();
    }

    let cell_width = |cell: &[Span<'static>]| cell.iter().map(Span::width).sum::<usize>();
    let mut widths = vec![1usize; ncols];
    for row in &rows {
        for (c, cell) in row.iter().enumerate() {
            widths[c] = widths[c].max(cell_width(cell));
        }
    }
    fit_widths(&mut widths, max_width);

    let border_style = Style::default().fg(Color::DarkGray);
    let mut lines = Vec::with_capacity(rows.len() + 1);

    for (r, mut row) in rows.into_iter().enumerate() {
        row.resize_with(ncols, Vec::new);
        let is_header = r == 0;
        let mut spans: Vec<Span<'static>> = vec![Span::raw(padding.to_string())];
        for (c, cell) in row.into_iter().enumerate() {
            if c > 0 {
                spans.push(Span::styled(" │ ", border_style));
            }
            let mut cell = truncate_spans(cell, widths[c]);
            let pad = widths[c].saturating_sub(cell_width(&cell));
            let (left, right) = match aligns.get(c).copied().unwrap_or(Alignment::None) {
                Alignment::Right => (pad, 0),
                Alignment::Center => (pad / 2, pad - pad / 2),
                Alignment::None | Alignment::Left => (0, pad),
            };
            if is_header {
                for span in &mut cell {
                    span.style = span.style.add_modifier(Modifier::BOLD);
                }
            }
            if left > 0 {
                spans.push(Span::raw(" ".repeat(left)));
            }
            spans.extend(cell);
            if right > 0 {
                spans.push(Span::raw(" ".repeat(right)));
            }
        }
        lines.push(Line::from(spans));

        if is_header {
            let rule = widths
                .iter()
                .map(|w| "─".repeat(*w))
                .collect::<Vec<_>>()
                .join("─┼─");
            lines.push(Line::from(vec![
                Span::raw(padding.to_string()),
                Span::styled(rule, border_style),
            ]));
        }
    }
    lines
}

const MIN_COL_WIDTH: usize = 3;
const COL_SEP_WIDTH: usize = 3; // " │ "

/// Narrow the widest columns until the table fits in `max_width`.
///
/// Water-filling: the widest columns are lowered together (evenly) to the
/// next widest level, and so on, so that no single column absorbs all of the
/// excess. Columns never go below `MIN_COL_WIDTH`.
fn fit_widths(widths: &mut [usize], max_width: usize) {
    let total = |w: &[usize]| w.iter().sum::<usize>() + COL_SEP_WIDTH * w.len().saturating_sub(1);
    loop {
        let excess = total(widths).saturating_sub(max_width);
        if excess == 0 {
            break;
        }
        let Some(w) = widths.iter().copied().filter(|w| *w > MIN_COL_WIDTH).max() else {
            break;
        };
        let next = widths
            .iter()
            .copied()
            .filter(|x| *x < w)
            .max()
            .unwrap_or(MIN_COL_WIDTH)
            .max(MIN_COL_WIDTH);
        let tied = widths.iter().filter(|x| **x == w).count();
        let reduce = excess.div_ceil(tied).min(w - next);
        for x in widths.iter_mut().filter(|x| **x == w) {
            *x = w - reduce;
        }
    }
}

/// Cut a styled cell to `width` columns, ending with `…` when truncated.
fn truncate_spans(cell: Vec<Span<'static>>, width: usize) -> Vec<Span<'static>> {
    use unicode_width::UnicodeWidthChar;

    let full: usize = cell.iter().map(Span::width).sum();
    if full <= width {
        return cell;
    }
    let budget = width.saturating_sub(1); // leave room for "…"
    let mut out = Vec::with_capacity(cell.len() + 1);
    let mut used = 0;
    for span in cell {
        if used >= budget {
            break;
        }
        let mut s = String::new();
        for ch in span.content.chars() {
            let cw = ch.width().unwrap_or(0);
            if used + cw > budget {
                break;
            }
            used += cw;
            s.push(ch);
        }
        if !s.is_empty() {
            out.push(Span::styled(s, span.style));
        }
    }
    out.push(Span::raw("…"));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIDE: usize = 200;

    fn render(md: &str) -> Vec<String> {
        markdown_to_lines(md, "", WIDE)
            .into_iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect()
    }

    #[test]
    fn table_renders_aligned_columns_with_header_rule() {
        let md = "| Input | Error |\n|---|---|\n| `a` | missing |\n| longer cell | x |\n";
        let lines = render(md);
        assert_eq!(
            lines,
            vec![
                "Input       │ Error  ",
                "────────────┼────────",
                " a          │ missing",
                "longer cell │ x      ",
            ]
        );
    }

    #[test]
    fn table_header_is_bold_and_inline_code_keeps_style() {
        let md = "| H |\n|---|\n| `code` |\n";
        let lines = markdown_to_lines(md, "", WIDE);
        let header = &lines[0];
        assert!(header
            .spans
            .iter()
            .any(|s| s.content.as_ref() == "H" && s.style.add_modifier.contains(Modifier::BOLD)));
        let body = &lines[2];
        assert!(body
            .spans
            .iter()
            .any(|s| s.content.as_ref() == " code " && s.style.fg == Some(Color::Yellow)));
    }

    #[test]
    fn table_honours_column_alignment() {
        let md = "| L | C | R |\n|:--|:-:|--:|\n| a | b | c |\n";
        let lines = render(md);
        assert_eq!(lines[2], "a │ b │ c");
        let md = "| L | C | R |\n|:--|:-:|--:|\n| aaa | bbb | ccc |\n";
        let lines = render(md);
        assert_eq!(lines[2], "aaa │ bbb │ ccc");
        let md = "| Left | Center | Right |\n|:--|:-:|--:|\n| a | b | c |\n";
        let lines = render(md);
        assert_eq!(lines[2], "a    │   b    │     c");
    }

    #[test]
    fn table_between_paragraphs_keeps_surrounding_text() {
        let md = "Before\n\n| A | B |\n|---|---|\n| 1 | 2 |\n\nAfter\n";
        let lines = render(md);
        assert_eq!(lines.first().map(String::as_str), Some("Before"));
        assert_eq!(lines[1], "A │ B");
        assert_eq!(lines[2], "──┼──");
        assert_eq!(lines[3], "1 │ 2");
        assert_eq!(lines.last().map(String::as_str), Some("After"));
    }

    #[test]
    fn padding_is_prepended_to_table_lines() {
        let lines = render_with_padding("| A |\n|---|\n| 1 |\n", "  ");
        assert_eq!(lines, vec!["  A", "  ─", "  1"]);
    }

    fn render_width(md: &str, padding: &str, width: usize) -> Vec<String> {
        markdown_to_lines(md, padding, width)
            .into_iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect()
    }

    #[test]
    fn wide_table_is_narrowed_to_fit_and_cells_truncated() {
        let md = "| Key | Description |\n|---|---|\n| a | this is a rather long description |\n";
        // natural width: 3 + 3 + 34 = 40; fit into 20
        let lines = render_width(md, "", 20);
        assert_eq!(lines[0], "Key │ Description   ");
        assert_eq!(lines[1], "────┼───────────────");
        assert_eq!(lines[2], "a   │ this is a rat…");
        assert!(lines.iter().all(|l| l.chars().count() <= 20));
    }

    #[test]
    fn narrowing_accounts_for_padding_and_wide_chars() {
        let md = "| 名前 | 説明 |\n|---|---|\n| 太郎 | とても長い説明文です |\n";
        let lines = render_width(md, "  ", 18);
        // 2 (padding) + 4 + 3 + 9 = 18
        assert_eq!(lines[2], "  太郎 │ とても長…");
    }

    #[test]
    fn table_with_too_many_columns_keeps_min_width() {
        let md = "| a | b | c | d |\n|---|---|---|---|\n| 1 | 2 | 3 | 4 |\n";
        // Cannot fit into 5 columns; natural widths are kept (never widened
        // to MIN_COL_WIDTH, never narrowed below it) and the paragraph wraps.
        let lines = render_width(md, "", 5);
        assert_eq!(lines[2], "1 │ 2 │ 3 │ 4");
    }

    #[test]
    fn fit_widths_lowers_widest_columns_evenly() {
        let mut w = vec![34, 85, 28];
        fit_widths(&mut w, 62);
        assert_eq!(w, vec![18, 18, 18]);
        let mut w = vec![10, 10];
        fit_widths(&mut w, 22);
        assert_eq!(w, vec![9, 9]); // odd excess rounds up per column
        let mut w = vec![3, 3];
        fit_widths(&mut w, 4);
        assert_eq!(w, vec![3, 3]); // never below MIN_COL_WIDTH
    }

    fn render_with_padding(md: &str, padding: &str) -> Vec<String> {
        markdown_to_lines(md, padding, WIDE)
            .into_iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect()
    }
}
