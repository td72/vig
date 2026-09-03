use super::GhDetailViewPane;
use crate::core::pane::PaneShared;
use crate::core::theme;
use crate::core::ui::markdown::markdown_to_lines;
use crate::github::domain::actions::time::{
    duration_between, format_relative, now_secs, parse_iso8601,
};
use crate::github::domain::actions::types::{RunState, WorkflowRun};
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
        GhDetailContent::Run(_) => {
            render_run(f, dv, shared, inner, is_focused);
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
                GhDetailPane::Status => right_rows[0].height,
                GhDetailPane::Reviews => right_rows[1].height,
                GhDetailPane::Comments => right_rows[2].height,
                _ => cols[0].height,
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

/// A workflow run: a header with its state, branch, event, duration and
/// age, then the Jobs (job → steps tree) and Log sub-panes side by side.
fn render_run(
    f: &mut Frame,
    dv: &mut GhDetailViewPane,
    shared: &PaneShared,
    inner: Rect,
    is_focused: bool,
) {
    let active_pane = dv.active_pane;
    let (match_set, current_match) = theme::list_search_highlights(shared, dv.pane_id);
    let no_matches = std::collections::HashSet::new();
    let GhDetailContent::Run(d) = &mut dv.content else {
        return;
    };

    let header_lines = build_run_header(&d.run, now_secs());
    let vert = Layout::vertical([
        Constraint::Length(header_lines.len() as u16),
        Constraint::Min(1),
    ])
    .split(inner);
    f.render_widget(
        Paragraph::new(header_lines).wrap(Wrap { trim: false }),
        vert[0],
    );

    let cols =
        Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)]).split(vert[1]);

    let jobs_active = active_pane == GhDetailPane::Jobs;
    let jobs_count = d
        .jobs
        .rows
        .iter()
        .filter(|r| matches!(r, super::jobs::JobRow::Job(_)))
        .count();
    let jobs_title = if d.jobs.rows.is_empty() {
        "Jobs".to_string()
    } else {
        format!("Jobs ({jobs_count})")
    };
    let (jobs_matches, jobs_current) = if jobs_active {
        (&match_set, current_match)
    } else {
        (&no_matches, None)
    };
    d.jobs.render(
        f,
        cols[0],
        sub_block(&jobs_title, jobs_active, is_focused),
        jobs_active && is_focused,
        jobs_matches,
        jobs_current,
    );

    let log_active = active_pane == GhDetailPane::Log;
    let (log_matches, log_current) = if log_active {
        (&match_set, current_match)
    } else {
        (&no_matches, None)
    };
    let log_title = d.log.title();
    d.log.render(
        f,
        cols[1],
        sub_block(&log_title, log_active, is_focused),
        log_matches,
        log_current,
    );
}

fn build_run_header(run: &WorkflowRun, now: i64) -> Vec<Line<'static>> {
    let state = run.state();
    let title_line = Line::from(vec![
        Span::raw("  "),
        Span::styled(
            format!("{} #{}", run.title(), run.number),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    let state_label = match state {
        RunState::Queued => "queued",
        RunState::InProgress => "in progress",
        RunState::Success => "success",
        RunState::Failure => "failure",
        RunState::Cancelled => "cancelled",
        RunState::Skipped => "skipped",
        RunState::Other => "unknown",
    };
    let state_bg = match state {
        RunState::Success => Color::Rgb(35, 134, 54),
        RunState::Failure => Color::Rgb(218, 54, 51),
        RunState::InProgress => Color::Rgb(187, 128, 9),
        _ => Color::Rgb(110, 119, 129),
    };
    let mut spans = vec![
        Span::raw(" "),
        badge(&format!("{} {state_label}", state.icon()), state_bg),
        Span::raw(" "),
        badge(&run.head_branch, Color::Rgb(130, 80, 160)),
        Span::raw(" "),
        badge(&run.event, Color::Rgb(68, 71, 78)),
    ];
    let duration = match state {
        RunState::Queued => String::new(),
        RunState::InProgress => duration_between(Some(&run.created_at), None, now),
        _ => duration_between(Some(&run.created_at), Some(&run.updated_at), now),
    };
    if !duration.is_empty() {
        spans.push(Span::raw(" "));
        spans.push(badge(&duration, Color::Rgb(31, 111, 139)));
    }
    if let Some(t) = parse_iso8601(&run.created_at) {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            format_relative(now - t),
            Style::default().fg(Color::DarkGray),
        ));
    }
    vec![title_line, Line::from(spans)]
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

pub(crate) fn format_date(iso: &str) -> &str {
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

pub(crate) fn build_issue_header(detail: &GhIssueDetail) -> Vec<Line<'static>> {
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

pub(crate) fn build_pr_header(detail: &GhPrDetail) -> Vec<Line<'static>> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_header_shows_title_state_branch_event_duration_and_age() {
        let run = WorkflowRun {
            id: 1,
            number: 191,
            name: "CI".into(),
            workflow_name: "CI".into(),
            status: "completed".into(),
            conclusion: "success".into(),
            head_branch: "main".into(),
            event: "push".into(),
            created_at: "2026-08-28T08:17:23Z".into(),
            updated_at: "2026-08-28T08:18:44Z".into(),
            url: String::new(),
        };
        let now = parse_iso8601("2026-08-28T09:17:23Z").unwrap();
        let lines = build_run_header(&run, now);
        let text: Vec<String> = lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        assert_eq!(text[0], "  CI #191");
        assert_eq!(text[1], "  ✓ success   main   push   1m21s  1h ago");
    }
}
