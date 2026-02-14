use crate::app::App;
use crate::github::state::{GhDetailContent, GhDetailPane, GhFocusedPane};
use crate::github::types::*;
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
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

    app.github.detail_view_height = cols[0].height;

    let active_pane = app.github.detail_pane;

    // Left pane: Body
    let left_title = pane_title("Body", active_pane == GhDetailPane::Left, is_focused);
    let body_lines = match &app.github.detail {
        GhDetailContent::Issue(detail) => build_body_lines(&detail.body),
        GhDetailContent::Pr(detail) => build_body_lines(&detail.body),
        _ => unreachable!(),
    };
    let left_block = Block::default()
        .title(left_title)
        .borders(Borders::ALL)
        .border_style(pane_border_style(active_pane == GhDetailPane::Left, is_focused));
    let left_para = Paragraph::new(body_lines)
        .block(left_block)
        .wrap(Wrap { trim: false })
        .scroll((app.github.detail_scroll_left, 0));
    f.render_widget(left_para, cols[0]);

    // Right pane: Status + Comments (PR) or Comments only (Issue)
    let right_title = match &app.github.detail {
        GhDetailContent::Pr(_) => "Status / Comments",
        _ => "Comments",
    };
    let right_title = pane_title(right_title, active_pane == GhDetailPane::Right, is_focused);
    let right_lines = match &app.github.detail {
        GhDetailContent::Issue(detail) => build_comments_lines(&detail.comments),
        GhDetailContent::Pr(detail) => build_right_pane_lines(detail),
        _ => unreachable!(),
    };
    let right_block = Block::default()
        .title(right_title)
        .borders(Borders::ALL)
        .border_style(pane_border_style(active_pane == GhDetailPane::Right, is_focused));
    let right_para = Paragraph::new(right_lines)
        .block(right_block)
        .wrap(Wrap { trim: false })
        .scroll((app.github.detail_scroll_right, 0));
    f.render_widget(right_para, cols[1]);
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

fn state_style(state: &str) -> Style {
    match state {
        "OPEN" => Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        "CLOSED" => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        "MERGED" => Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
        _ => Style::default(),
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
    let mut lines = Vec::new();

    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            format!("#{} {}", detail.number, detail.title),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    let author = detail
        .author
        .as_ref()
        .map(|a| a.login.as_str())
        .unwrap_or("unknown");
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(author.to_string(), Style::default().fg(Color::Cyan)),
        Span::raw(format!(" opened {} ", format_date(&detail.created_at))),
        Span::styled(detail.state.clone(), state_style(&detail.state)),
    ]));

    if !detail.labels.is_empty() {
        let mut spans = vec![Span::raw(" ")];
        spans.extend(build_label_spans(&detail.labels));
        lines.push(Line::from(spans));
    }

    lines
}

fn build_pr_header(detail: &GhPrDetail) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            format!("#{} {}", detail.number, detail.title),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    let author = detail
        .author
        .as_ref()
        .map(|a| a.login.as_str())
        .unwrap_or("unknown");
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(author.to_string(), Style::default().fg(Color::Cyan)),
        Span::raw(format!(" opened {} ", format_date(&detail.created_at))),
        Span::styled(detail.state.clone(), state_style(&detail.state)),
        Span::raw("  "),
        Span::styled(
            detail.head_ref_name.clone(),
            Style::default().fg(Color::Magenta),
        ),
    ]));

    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            format!("+{}", detail.additions),
            Style::default().fg(Color::Green),
        ),
        Span::raw(" "),
        Span::styled(
            format!("-{}", detail.deletions),
            Style::default().fg(Color::Red),
        ),
        Span::raw(format!("  {} files changed", detail.changed_files)),
    ]));

    if let Some(ref decision) = detail.review_decision {
        let (icon, color) = match decision.as_str() {
            "APPROVED" => ("✓ APPROVED", Color::Green),
            "CHANGES_REQUESTED" => ("✗ CHANGES REQUESTED", Color::Red),
            "REVIEW_REQUIRED" => ("◯ REVIEW REQUIRED", Color::Yellow),
            _ => (decision.as_str(), Color::White),
        };
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                icon.to_string(),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
        ]));
    }

    if !detail.labels.is_empty() {
        let mut spans = vec![Span::raw(" ")];
        spans.extend(build_label_spans(&detail.labels));
        lines.push(Line::from(spans));
    }

    lines
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

fn build_right_pane_lines(detail: &GhPrDetail) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    // CI Status
    if let Some(ref checks) = detail.status_check_rollup {
        if !checks.is_empty() {
            lines.push(Line::from(Span::styled(
                format!("  ─── CI Status ({}) ───", checks.len()),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )));
            for check in checks {
                let (icon, color) = match check.conclusion.as_deref() {
                    Some("SUCCESS") => ("✓", Color::Green),
                    Some("FAILURE") => ("✗", Color::Red),
                    Some("NEUTRAL") | Some("SKIPPED") => ("○", Color::DarkGray),
                    _ => match check.status.as_str() {
                        "IN_PROGRESS" => ("◐", Color::Yellow),
                        "QUEUED" | "WAITING" => ("◯", Color::DarkGray),
                        _ => ("?", Color::DarkGray),
                    },
                };
                let conclusion_label = check.conclusion.as_deref().unwrap_or(&check.status);
                let workflow = check.workflow_name.as_deref().unwrap_or("");
                let name = if workflow.is_empty() {
                    check.name.clone()
                } else {
                    format!("{} / {}", workflow, check.name)
                };
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(icon.to_string(), Style::default().fg(color)),
                    Span::raw(format!(" {name} ({conclusion_label})")),
                ]));
            }
        }
    }

    // Reviews
    if !detail.reviews.is_empty() {
        let meaningful_reviews: Vec<&GhReview> = detail
            .reviews
            .iter()
            .filter(|r| !r.body.is_empty() || r.state != "COMMENTED")
            .collect();
        if !meaningful_reviews.is_empty() {
            if !lines.is_empty() {
                lines.push(Line::from(""));
            }
            lines.push(Line::from(Span::styled(
                format!("  ─── Reviews ({}) ───", meaningful_reviews.len()),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )));
            for review in meaningful_reviews {
                lines.push(Line::from(""));
                let author = review
                    .author
                    .as_ref()
                    .map(|a| a.login.as_str())
                    .unwrap_or("unknown");
                let date = review
                    .submitted_at
                    .as_deref()
                    .map(format_date)
                    .unwrap_or("");
                let (state_icon, state_color) = match review.state.as_str() {
                    "APPROVED" => ("✓", Color::Green),
                    "CHANGES_REQUESTED" => ("✗", Color::Red),
                    "COMMENTED" => ("💬", Color::DarkGray),
                    "DISMISSED" => ("⊘", Color::DarkGray),
                    _ => ("", Color::White),
                };
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(author.to_string(), Style::default().fg(Color::Cyan)),
                    Span::raw(format!("  {date}  ")),
                    Span::styled(
                        format!("{state_icon} {}", review.state),
                        Style::default().fg(state_color),
                    ),
                ]));
                if !review.body.is_empty() {
                    for line in review.body.lines() {
                        lines.push(Line::from(format!("  {line}")));
                    }
                }
            }
        }
    }

    // Comments
    if !detail.comments.is_empty() {
        if !lines.is_empty() {
            lines.push(Line::from(""));
        }
        append_comments_lines(&mut lines, &detail.comments);
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (no status, reviews, or comments)",
            Style::default().fg(Color::DarkGray),
        )));
    }

    lines
}

fn build_comments_lines(comments: &[GhComment]) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if comments.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (no comments)",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        append_comments_lines(&mut lines, comments);
    }
    lines
}

fn append_comments_lines(lines: &mut Vec<Line<'static>>, comments: &[GhComment]) {
    lines.push(Line::from(Span::styled(
        format!("  ─── Comments ({}) ───", comments.len()),
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    )));
    for (i, comment) in comments.iter().enumerate() {
        if i > 0 {
            lines.push(Line::from(""));
        }
        build_comment_lines(lines, comment);
    }
}

fn build_comment_lines(lines: &mut Vec<Line<'static>>, comment: &GhComment) {
    let author = comment
        .author
        .as_ref()
        .map(|a| a.login.as_str())
        .unwrap_or("unknown");
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(author.to_string(), Style::default().fg(Color::Cyan)),
        Span::raw(format!("  {}", format_date(&comment.created_at))),
    ]));
    for line in comment.body.lines() {
        lines.push(Line::from(format!("  {line}")));
    }
}
