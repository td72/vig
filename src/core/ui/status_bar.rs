use crate::core::app::AppContext;
use crate::git::state::GitState;
use crate::github::state::GitHubState;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

fn page_tab_spans(ctx: &AppContext) -> Vec<Span<'static>> {
    let active_style = Style::default()
        .fg(Color::Black)
        .bg(Color::White)
        .add_modifier(Modifier::BOLD);
    let inactive_style = Style::default().fg(Color::DarkGray);

    let mut spans = vec![Span::raw("  ")];
    for (i, label) in ctx.page_labels.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(" "));
        }
        let style = if i == ctx.active_page {
            active_style
        } else {
            inactive_style
        };
        spans.push(Span::styled(format!(" {}:{} ", i + 1, label), style));
    }
    spans
}

pub fn render_header(f: &mut Frame, ctx: &AppContext, git: &GitState, area: Rect) {
    let mut spans = vec![
        Span::styled(
            " vig ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            format!(" {} ", git.diff_meta.branch_name),
            Style::default().fg(Color::Black).bg(Color::Magenta),
        ),
    ];

    {
        let base_label = match &git.diff_base_ref {
            Some(base) => format!(" vs {base} "),
            None => " vs HEAD ".to_string(),
        };
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            base_label,
            Style::default().fg(Color::Black).bg(Color::Yellow),
        ));
    }

    spans.extend(page_tab_spans(ctx));

    spans.push(Span::raw("  "));
    spans.push(Span::styled("? help", Style::default().fg(Color::DarkGray)));

    let title = Line::from(spans);
    f.render_widget(Paragraph::new(title), area);
}

pub fn render_gh_header(f: &mut Frame, ctx: &AppContext, area: Rect) {
    let mut spans = vec![
        Span::styled(
            " vig ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            " GitHub ",
            Style::default().fg(Color::Black).bg(Color::Rgb(36, 41, 47)),
        ),
    ];

    spans.extend(page_tab_spans(ctx));

    spans.push(Span::raw("  "));
    spans.push(Span::styled("? help", Style::default().fg(Color::DarkGray)));

    let title = Line::from(spans);
    f.render_widget(Paragraph::new(title), area);
}

pub fn render_status_bar(f: &mut Frame, ctx: &AppContext, git: &GitState, area: Rect) {
    if git.pane.search.active {
        let prompt = format!("/{}\u{2588}", git.pane.search.input);
        let line = Line::from(Span::styled(
            format!(" {prompt}"),
            Style::default().fg(Color::White),
        ));
        f.render_widget(Paragraph::new(line), area);
        return;
    }

    let file_count = git.diff_meta.file_count;
    let adds = git.diff_meta.stats.additions;
    let dels = git.diff_meta.stats.deletions;

    let status = if let Some(ref msg) = ctx.status_message {
        Line::from(Span::styled(
            format!(" {msg}"),
            Style::default().fg(Color::Yellow),
        ))
    } else if file_count == 0 {
        Line::from(Span::styled(
            " Working tree clean",
            Style::default().fg(Color::Green),
        ))
    } else {
        Line::from(vec![
            Span::styled(
                format!(
                    " {file_count} file{}",
                    if file_count == 1 { "" } else { "s" }
                ),
                Style::default().fg(Color::White),
            ),
            Span::raw("  "),
            Span::styled(format!("+{adds}"), Style::default().fg(Color::Green)),
            Span::raw(" "),
            Span::styled(format!("-{dels}"), Style::default().fg(Color::Red)),
        ])
    };

    f.render_widget(Paragraph::new(status), area);
}

pub fn render_gh_status_bar(f: &mut Frame, ctx: &AppContext, gh: &GitHubState, area: Rect) {
    if let Some(ref err) = gh.gh_error {
        let line = Line::from(Span::styled(
            format!(" {err}"),
            Style::default().fg(Color::Red),
        ));
        f.render_widget(Paragraph::new(line), area);
        return;
    }

    let issue_count = gh.panes.issue_tab.list.items.len();
    let pr_count = gh.panes.pr_tab.list.items.len();

    let loading = gh.panes.issue_tab.list.loading || gh.panes.pr_tab.list.loading;
    let has_data = issue_count > 0 || pr_count > 0;

    let mut spans = Vec::new();
    if loading && !has_data {
        // Initial load (no cache) — show Loading...
        spans.push(Span::styled(
            " Loading...",
            Style::default().fg(Color::DarkGray),
        ));
    } else {
        spans.push(Span::styled(
            format!(
                " {} issue{}",
                issue_count,
                if issue_count == 1 { "" } else { "s" }
            ),
            Style::default().fg(Color::White),
        ));
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!("{} PR{}", pr_count, if pr_count == 1 { "" } else { "s" }),
            Style::default().fg(Color::White),
        ));
        if loading {
            // Background refresh with cached data visible
            spans.push(Span::raw("  "));
            spans.push(Span::styled(
                "↻ Updating...",
                Style::default().fg(Color::DarkGray),
            ));
        }
    }

    if let Some(time) = gh.panes.pr_tab.detail.watch_last_update_time() {
        spans.push(Span::raw("  "));
        if let Some(ref err) = gh.panes.pr_tab.detail.watch_error {
            spans.push(Span::styled(
                format!("\u{23f1} Watch (err: {err})"),
                Style::default().fg(Color::Red),
            ));
        } else {
            spans.push(Span::styled(
                format!("\u{23f1} Watch (last: {time})"),
                Style::default().fg(Color::Yellow),
            ));
        }
    }

    // Status message (overrides if present)
    if let Some(ref msg) = ctx.status_message {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            msg.clone(),
            Style::default().fg(Color::Yellow),
        ));
    }

    let line = Line::from(spans);
    f.render_widget(Paragraph::new(line), area);
}

pub fn render_help_overlay(f: &mut Frame, area: Rect, keybindings: &[(String, String)]) {
    use ratatui::widgets::{Block, Borders, Clear};

    let help_width = 50u16.min(area.width.saturating_sub(4));
    let help_height = ((keybindings.len() as u16) + 2).min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(help_width)) / 2;
    let y = (area.height.saturating_sub(help_height)) / 2;
    let help_area = Rect::new(x, y, help_width, help_height);

    f.render_widget(Clear, help_area);

    let lines: Vec<Line> = keybindings
        .iter()
        .map(|(key, desc)| {
            Line::from(vec![
                Span::styled(
                    format!("  {key:<12}"),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(desc.as_str()),
            ])
        })
        .collect();

    let block = Block::default()
        .title(" Keybindings ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let para = Paragraph::new(lines).block(block);
    f.render_widget(para, help_area);
}
