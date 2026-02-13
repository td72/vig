use crate::app::App;
use crate::github::state::{GhDetailContent, GhFocusedPane};
use crate::github::types::*;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

pub fn render(f: &mut Frame, app: &mut App, area: Rect) {
    let is_focused = app.github.focused_pane == GhFocusedPane::Detail;
    let border_color = if is_focused { Color::Cyan } else { Color::DarkGray };

    let block = Block::default()
        .title(" Detail ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let inner_height = area.height.saturating_sub(2);
    app.github.detail_view_height = inner_height;

    let lines = match &app.github.detail {
        GhDetailContent::None => {
            vec![Line::from(Span::styled(
                "  Select an issue or PR to view details",
                Style::default().fg(Color::DarkGray),
            ))]
        }
        GhDetailContent::Loading { kind, number } => {
            let label = match kind {
                crate::github::state::GhDetailKind::Issue => "issue",
                crate::github::state::GhDetailKind::Pr => "PR",
            };
            vec![Line::from(Span::styled(
                format!("  Loading {label} #{number}..."),
                Style::default().fg(Color::DarkGray),
            ))]
        }
        GhDetailContent::Error(e) => {
            vec![Line::from(Span::styled(
                format!("  Error: {e}"),
                Style::default().fg(Color::Red),
            ))]
        }
        GhDetailContent::Issue(detail) => build_issue_lines(detail),
        GhDetailContent::Pr(detail) => build_pr_lines(detail),
    };

    let para = Paragraph::new(lines)
        .block(block)
        .scroll((app.github.detail_scroll, 0));

    f.render_widget(para, area);
}

fn format_date(iso: &str) -> &str {
    // Return just the date part (YYYY-MM-DD) from ISO 8601
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
        // Choose text color based on background brightness
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

fn build_issue_lines(detail: &GhIssueDetail) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    // Title
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            format!("#{} {}", detail.number, detail.title),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    // Author + date + state
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

    // Labels
    if !detail.labels.is_empty() {
        let mut spans = vec![Span::raw(" ")];
        spans.extend(build_label_spans(&detail.labels));
        lines.push(Line::from(spans));
    }

    lines.push(Line::from(""));

    // Body
    if !detail.body.is_empty() {
        lines.push(Line::from(Span::styled(
            "  ─── Body ───",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )));
        for line in detail.body.lines() {
            lines.push(Line::from(format!("  {line}")));
        }
    }

    // Comments
    if !detail.comments.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("  ─── Comments ({}) ───", detail.comments.len()),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )));
        for comment in &detail.comments {
            lines.push(Line::from(""));
            build_comment_lines(&mut lines, comment);
        }
    }

    lines
}

fn build_pr_lines(detail: &GhPrDetail) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    // Title
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            format!("#{} {}", detail.number, detail.title),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    // Author + date + state + branch
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

    // Stats
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

    // Review decision
    if let Some(ref decision) = detail.review_decision {
        let (icon, color) = match decision.as_str() {
            "APPROVED" => ("✓ APPROVED", Color::Green),
            "CHANGES_REQUESTED" => ("✗ CHANGES REQUESTED", Color::Red),
            "REVIEW_REQUIRED" => ("◯ REVIEW REQUIRED", Color::Yellow),
            _ => (decision.as_str(), Color::White),
        };
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(icon.to_string(), Style::default().fg(color).add_modifier(Modifier::BOLD)),
        ]));
    }

    // Labels
    if !detail.labels.is_empty() {
        let mut spans = vec![Span::raw(" ")];
        spans.extend(build_label_spans(&detail.labels));
        lines.push(Line::from(spans));
    }

    lines.push(Line::from(""));

    // Body
    if !detail.body.is_empty() {
        lines.push(Line::from(Span::styled(
            "  ─── Body ───",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )));
        for line in detail.body.lines() {
            lines.push(Line::from(format!("  {line}")));
        }
    }

    // CI Status
    if let Some(ref checks) = detail.status_check_rollup {
        if !checks.is_empty() {
            lines.push(Line::from(""));
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
                    _ => {
                        // Still in progress
                        match check.status.as_str() {
                            "IN_PROGRESS" => ("◐", Color::Yellow),
                            "QUEUED" | "WAITING" => ("◯", Color::DarkGray),
                            _ => ("?", Color::DarkGray),
                        }
                    }
                };
                let conclusion_label = check
                    .conclusion
                    .as_deref()
                    .unwrap_or(&check.status);
                let workflow = check
                    .workflow_name
                    .as_deref()
                    .unwrap_or("");
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
        // Filter out reviews with empty body to avoid noise
        let meaningful_reviews: Vec<&GhReview> = detail
            .reviews
            .iter()
            .filter(|r| !r.body.is_empty() || r.state != "COMMENTED")
            .collect();
        if !meaningful_reviews.is_empty() {
            lines.push(Line::from(""));
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
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("  ─── Comments ({}) ───", detail.comments.len()),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )));
        for comment in &detail.comments {
            lines.push(Line::from(""));
            build_comment_lines(&mut lines, comment);
        }
    }

    lines
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
