use crate::app::App;
use crate::github::state::{GhDetailContent, GhDetailPane, GhFocusedPane};
use crate::github::types::*;
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Row, Table, TableState, Wrap},
    Frame,
};

pub fn render(f: &mut Frame, app: &mut App, area: Rect) {
    let is_focused = app.github.focused_pane == GhFocusedPane::Detail;
    let border_color = if is_focused { Color::Cyan } else { Color::DarkGray };

    let block = Block::default()
        .title(" Detail ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Early return for non-loaded states
    match &app.github.detail {
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
    let header_lines = match &app.github.detail {
        GhDetailContent::Issue(detail) => build_issue_header(detail),
        GhDetailContent::Pr(detail) => build_pr_header(detail),
        _ => unreachable!(),
    };

    let header_height = header_lines.len() as u16;

    // Layout: header (fixed) + content area (side-by-side)
    let vert = Layout::vertical([
        Constraint::Length(header_height),
        Constraint::Min(1),
    ])
    .split(inner);

    // Render header
    let header_para = Paragraph::new(header_lines).wrap(Wrap { trim: false });
    f.render_widget(header_para, vert[0]);

    // Split content area into left and right columns
    let cols = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(vert[1]);

    let active_pane = app.github.detail_pane;

    // Left pane: Body
    let body_lines = match &app.github.detail {
        GhDetailContent::Issue(detail) => build_body_lines(&detail.body),
        GhDetailContent::Pr(detail) => build_body_lines(&detail.body),
        _ => unreachable!(),
    };
    render_pane(
        f,
        cols[0],
        "Body",
        body_lines,
        active_pane == GhDetailPane::Body,
        is_focused,
        app.github.detail_scroll_body,
    );

    // Right side
    match &app.github.detail {
        GhDetailContent::Issue(detail) => {
            // Issue: single Comments pane on the right
            let count = detail.comments.len();
            let title = format!("Comments ({count})");
            app.github.detail_view_height = cols[1].height;
            let (comments_lines, sel_scroll) = build_comments_lines(&detail.comments, app.github.detail_comment_idx);
            render_pane(
                f,
                cols[1],
                &title,
                comments_lines,
                active_pane == GhDetailPane::Comments,
                is_focused,
                sel_scroll + app.github.detail_scroll_comments,
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

            app.github.detail_view_height = right_rows[0].height;

            let checks_count = detail
                .status_check_rollup
                .as_ref()
                .map_or(0, |c| c.len());
            let checks_title = format!("Checks ({checks_count})");
            render_status_table(
                f,
                right_rows[0],
                &checks_title,
                detail,
                active_pane == GhDetailPane::Status,
                is_focused,
                app.github.detail_check_idx,
            );

            let review_count = detail
                .reviews
                .iter()
                .filter(|r| !r.body.is_empty() || r.state != "COMMENTED")
                .count();
            let reviews_title = format!("Reviews ({review_count})");
            let (reviews_lines, rev_scroll) = build_reviews_lines(&detail.reviews, app.github.detail_review_idx);
            render_pane(
                f,
                right_rows[1],
                &reviews_title,
                reviews_lines,
                active_pane == GhDetailPane::Reviews,
                is_focused,
                rev_scroll + app.github.detail_scroll_reviews,
            );

            let comments_count = detail.comments.len();
            let comments_title = format!("Comments ({comments_count})");
            let (comments_lines, cmt_scroll) = build_comments_lines(&detail.comments, app.github.detail_comment_idx);
            render_pane(
                f,
                right_rows[2],
                &comments_title,
                comments_lines,
                active_pane == GhDetailPane::Comments,
                is_focused,
                cmt_scroll + app.github.detail_scroll_comments,
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
    let block = Block::default()
        .title(pane_title(title, is_active, is_detail_focused))
        .borders(Borders::ALL)
        .border_style(pane_border_style(is_active, is_detail_focused));
    let para = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));
    f.render_widget(para, area);
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
        let fg = if brightness > 128 { Color::Black } else { Color::White };

        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            format!(" {} ", label.name),
            Style::default().fg(fg).bg(bg),
        ));
    }
    spans
}

// --- Header builders ---

fn build_issue_header(detail: &GhIssueDetail) -> Vec<Line<'static>> {
    // Line 1: title
    let title_line = Line::from(vec![
        Span::raw("  "),
        Span::styled(
            format!("#{} {}", detail.number, detail.title),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    // Line 2: badges
    let author = detail
        .author
        .as_ref()
        .map(|a| a.login.as_str())
        .unwrap_or("unknown");
    let mut spans = vec![Span::raw(" ")];
    spans.push(badge(author, Color::Rgb(31, 111, 139)));
    spans.push(Span::raw(" "));
    spans.push(badge(format_date(&detail.created_at), Color::Rgb(68, 71, 78)));
    spans.push(Span::raw(" "));
    spans.push(state_badge(&detail.state));
    for s in build_label_spans(&detail.labels) {
        spans.push(s);
    }

    vec![title_line, Line::from(spans)]
}

fn build_pr_header(detail: &GhPrDetail) -> Vec<Line<'static>> {
    // Line 1: title
    let title_line = Line::from(vec![
        Span::raw("  "),
        Span::styled(
            format!("#{} {}", detail.number, detail.title),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    // Line 2: badges
    let author = detail
        .author
        .as_ref()
        .map(|a| a.login.as_str())
        .unwrap_or("unknown");
    let mut spans = vec![Span::raw(" ")];
    spans.push(badge(author, Color::Rgb(31, 111, 139)));
    spans.push(Span::raw(" "));
    spans.push(badge(format_date(&detail.created_at), Color::Rgb(68, 71, 78)));
    spans.push(Span::raw(" "));
    spans.push(state_badge(&detail.state));
    spans.push(Span::raw(" "));
    spans.push(badge(&detail.head_ref_name, Color::Rgb(130, 80, 160)));
    spans.push(Span::raw(" "));
    spans.push(badge(&format!("+{}", detail.additions), Color::Rgb(35, 134, 54)));
    spans.push(badge(&format!("-{}", detail.deletions), Color::Rgb(218, 54, 51)));
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

    for s in build_label_spans(&detail.labels) {
        spans.push(s);
    }

    vec![title_line, Line::from(spans)]
}

fn badge(text: &str, bg: Color) -> Span<'static> {
    let fg = badge_fg(bg);
    Span::styled(
        format!(" {text} "),
        Style::default().fg(fg).bg(bg),
    )
}

fn badge_fg(bg: Color) -> Color {
    let (r, g, b) = match bg {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => return Color::White,
    };
    let brightness = (r as u32 * 299 + g as u32 * 587 + b as u32 * 114) / 1000;
    if brightness > 128 { Color::Black } else { Color::White }
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

fn build_body_lines(body: &str) -> Vec<Line<'static>> {
    if body.is_empty() {
        return vec![Line::from(Span::styled(
            "  (no description)",
            Style::default().fg(Color::DarkGray),
        ))];
    }
    body.lines().map(|line| Line::from(format!("  {line}"))).collect()
}

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

fn render_status_table(
    f: &mut Frame,
    area: Rect,
    title: &str,
    detail: &GhPrDetail,
    is_active: bool,
    is_detail_focused: bool,
    selected_idx: usize,
) {
    let block = Block::default()
        .title(pane_title(title, is_active, is_detail_focused))
        .borders(Borders::ALL)
        .border_style(pane_border_style(is_active, is_detail_focused));

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
            let duration = format_duration(
                check.started_at.as_deref(),
                check.completed_at.as_deref(),
            );
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
    if is_active || is_detail_focused {
        state.select(Some(selected_idx));
    }
    f.render_stateful_widget(table, area, &mut state);
}

/// Format duration between two ISO 8601 timestamps (e.g. "1m23s", "45s").
fn format_duration(started: Option<&str>, completed: Option<&str>) -> String {
    let (Some(s), Some(c)) = (started, completed) else {
        return String::new();
    };
    // Parse "2024-01-02T03:04:05Z" — compare as seconds
    let parse = |iso: &str| -> Option<i64> {
        // Minimal ISO 8601 parse: YYYY-MM-DDTHH:MM:SSZ
        if iso.len() < 19 {
            return None;
        }
        let y: i64 = iso[0..4].parse().ok()?;
        let mo: i64 = iso[5..7].parse().ok()?;
        let d: i64 = iso[8..10].parse().ok()?;
        let h: i64 = iso[11..13].parse().ok()?;
        let mi: i64 = iso[14..16].parse().ok()?;
        let se: i64 = iso[17..19].parse().ok()?;
        Some(((y * 365 + mo * 30 + d) * 86400) + h * 3600 + mi * 60 + se)
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

fn check_icon(check: &GhStatusCheck) -> (&'static str, Color) {
    match check.conclusion.as_deref() {
        Some("SUCCESS") => ("✓", Color::Green),
        Some("FAILURE") => ("✗", Color::Red),
        Some("NEUTRAL") | Some("SKIPPED") => ("○", Color::DarkGray),
        _ => match check.status.as_str() {
            "IN_PROGRESS" => ("◐", Color::Yellow),
            "QUEUED" | "WAITING" => ("◯", Color::DarkGray),
            _ => ("?", Color::DarkGray),
        },
    }
}

fn review_icon(review: &GhReview) -> (&'static str, Color) {
    match review.state.as_str() {
        "APPROVED" => ("✓", Color::Green),
        "CHANGES_REQUESTED" => ("✗", Color::Red),
        "COMMENTED" => ("💬", Color::DarkGray),
        "DISMISSED" => ("⊘", Color::DarkGray),
        _ => ("?", Color::White),
    }
}

/// Get meaningful reviews (non-empty body or non-COMMENTED state).
pub fn meaningful_reviews(reviews: &[GhReview]) -> Vec<&GhReview> {
    reviews
        .iter()
        .filter(|r| !r.body.is_empty() || r.state != "COMMENTED")
        .collect()
}

/// Returns (lines, selected_header_line_offset).
fn build_reviews_lines(reviews: &[GhReview], selected_idx: usize) -> (Vec<Line<'static>>, u16) {
    let meaningful = meaningful_reviews(reviews);
    if meaningful.is_empty() {
        return (
            vec![Line::from(Span::styled(
                "  (no reviews)",
                Style::default().fg(Color::DarkGray),
            ))],
            0,
        );
    }
    let sel_bg = Style::default().bg(Color::DarkGray);
    let mut lines = Vec::new();
    let mut sel_offset: u16 = 0;
    for (i, review) in meaningful.iter().enumerate() {
        if i > 0 {
            lines.push(Line::from(""));
        }
        let is_sel = i == selected_idx;
        if is_sel {
            sel_offset = lines.len() as u16;
        }
        let (icon, color) = review_icon(review);
        let author = review
            .author
            .as_ref()
            .map(|a| a.login.as_str())
            .unwrap_or("unknown");
        let mut header = Line::from(vec![
            Span::raw("  "),
            Span::styled(icon, Style::default().fg(color)),
            Span::raw(" "),
            Span::styled(author.to_string(), Style::default().fg(Color::Cyan)),
            Span::raw("  "),
            Span::styled(review.state.clone(), Style::default().fg(color)),
        ]);
        if is_sel {
            header = header.style(sel_bg);
        }
        lines.push(header);
        if !review.body.is_empty() {
            for line in review.body.lines() {
                lines.push(Line::from(format!("  {line}")));
            }
        }
    }
    (lines, sel_offset)
}

/// Returns (lines, selected_header_line_offset).
fn build_comments_lines(comments: &[GhComment], selected_idx: usize) -> (Vec<Line<'static>>, u16) {
    if comments.is_empty() {
        return (
            vec![Line::from(Span::styled(
                "  (no comments)",
                Style::default().fg(Color::DarkGray),
            ))],
            0,
        );
    }
    let sel_bg = Style::default().bg(Color::DarkGray);
    let mut lines = Vec::new();
    let mut sel_offset: u16 = 0;
    for (i, comment) in comments.iter().enumerate() {
        if i > 0 {
            lines.push(Line::from(""));
        }
        let is_sel = i == selected_idx;
        if is_sel {
            sel_offset = lines.len() as u16;
        }
        let author = comment
            .author
            .as_ref()
            .map(|a| a.login.as_str())
            .unwrap_or("unknown");
        let mut header = Line::from(vec![
            Span::raw("  "),
            Span::styled(author.to_string(), Style::default().fg(Color::Cyan)),
            Span::raw(format!("  {}", format_date(&comment.created_at))),
        ]);
        if is_sel {
            header = header.style(sel_bg);
        }
        lines.push(header);
        for line in comment.body.lines() {
            lines.push(Line::from(format!("  {line}")));
        }
    }
    (lines, sel_offset)
}
