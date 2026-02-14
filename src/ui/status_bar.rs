use crate::app::{App, ViewMode};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

fn view_tab_spans(active: ViewMode) -> Vec<Span<'static>> {
    let git_style = if active == ViewMode::Git {
        Style::default()
            .fg(Color::Black)
            .bg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let gh_style = if active == ViewMode::GitHub {
        Style::default()
            .fg(Color::Black)
            .bg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    vec![
        Span::raw("  "),
        Span::styled(" 1:Git ", git_style),
        Span::raw(" "),
        Span::styled(" 2:GitHub ", gh_style),
    ]
}

pub fn render_header(f: &mut Frame, app: &App, area: Rect) {
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
            format!(" {} ", app.diff_state.branch_name),
            Style::default().fg(Color::Black).bg(Color::Magenta),
        ),
    ];

    {
        let base_label = match &app.diff_base_ref {
            Some(base) => format!(" vs {base} "),
            None => " vs HEAD ".to_string(),
        };
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            base_label,
            Style::default().fg(Color::Black).bg(Color::Yellow),
        ));
    }

    spans.extend(view_tab_spans(app.view_mode));

    spans.push(Span::raw("  "));
    spans.push(Span::styled(
        "? help",
        Style::default().fg(Color::DarkGray),
    ));

    let title = Line::from(spans);
    f.render_widget(Paragraph::new(title), area);
}

pub fn render_gh_header(f: &mut Frame, app: &App, area: Rect) {
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
            Style::default()
                .fg(Color::Black)
                .bg(Color::Rgb(36, 41, 47)),
        ),
    ];

    spans.extend(view_tab_spans(app.view_mode));

    spans.push(Span::raw("  "));
    spans.push(Span::styled(
        "? help",
        Style::default().fg(Color::DarkGray),
    ));

    let title = Line::from(spans);
    f.render_widget(Paragraph::new(title), area);
}

pub fn render_status_bar(f: &mut Frame, app: &App, area: Rect) {
    if app.search.active {
        let prompt = format!("/{}\u{2588}", app.search.input);
        let line = Line::from(Span::styled(
            format!(" {prompt}"),
            Style::default().fg(Color::White),
        ));
        f.render_widget(Paragraph::new(line), area);
        return;
    }

    let file_count = app.diff_state.files.len();
    let adds = app.diff_state.stats.additions;
    let dels = app.diff_state.stats.deletions;

    let status = if let Some(ref msg) = app.status_message {
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
                format!(" {file_count} file{}", if file_count == 1 { "" } else { "s" }),
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

pub fn render_gh_status_bar(f: &mut Frame, app: &App, area: Rect) {
    if let Some(ref err) = app.github.gh_error {
        let line = Line::from(Span::styled(
            format!(" {err}"),
            Style::default().fg(Color::Red),
        ));
        f.render_widget(Paragraph::new(line), area);
        return;
    }

    let issue_count = app.github.issues.len();
    let pr_count = app.github.prs.len();

    let mut spans = Vec::new();
    if app.github.issues_loading || app.github.prs_loading {
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
    }

    let line = Line::from(spans);
    f.render_widget(Paragraph::new(line), area);
}

pub fn render_help_overlay(f: &mut Frame, area: Rect, view_mode: ViewMode) {
    use ratatui::widgets::{Block, Borders, Clear};

    let keybindings = match view_mode {
        ViewMode::Git => vec![
            ("1 / 2", "Switch to Git / GitHub"),
            ("j / ↓", "Next item / Scroll down"),
            ("k / ↑", "Prev item / Scroll up"),
            ("Enter", "Select file/branch"),
            ("Tab", "Next pane"),
            ("Shift+Tab", "Prev pane"),
            ("Ctrl+d", "Half page down"),
            ("Ctrl+u", "Half page up"),
            ("g / G", "Top / Bottom"),
            ("h / l", "Scroll left / right"),
            ("i", "Normal mode (cursor)"),
            ("v / V", "Visual / Visual Line"),
            ("y", "Yank (copy) selection"),
            ("/", "Search"),
            ("n / N", "Next / Prev match"),
            ("Esc", "Clear search / Back"),
            ("e", "Open in $EDITOR"),
            ("r", "Refresh diff + branches"),
            ("?", "Toggle help"),
            ("q", "Quit"),
            ("", ""),
            ("", "── Branch List ──"),
            ("/", "Search branches"),
            ("Enter", "Action menu"),
            ("", ""),
            ("", "── Git Log ──"),
            ("j / k", "Navigate commits"),
            ("Ctrl+d/u", "Half page scroll"),
            ("g / G", "Top / Bottom"),
            ("y", "Copy commit hash"),
            ("o", "Open in GitHub"),
            ("/", "Search commits"),
            ("", ""),
            ("", "── Reflog ──"),
            ("j / k", "Navigate entries"),
            ("Ctrl+d/u", "Half page scroll"),
            ("g / G", "Top / Bottom"),
            ("Enter", "Set as diff base"),
            ("/", "Search reflog"),
        ],
        ViewMode::GitHub => vec![
            ("1 / 2", "Switch to Git / GitHub"),
            ("h / l", "Issues ↔ PRs (list)"),
            ("j / k", "Navigate list"),
            ("i / Enter", "Open detail"),
            ("o", "Open in browser"),
            ("Esc", "Back to list"),
            ("h / l", "Body ↔ Right pane (detail)"),
            ("Tab / S-Tab", "Cycle right panes (detail)"),
            ("Ctrl+d", "Half page down (detail)"),
            ("Ctrl+u", "Half page up (detail)"),
            ("g / G", "Top / Bottom"),
            ("r", "Refresh data"),
            ("?", "Toggle help"),
            ("q", "Quit"),
        ],
    };

    let help_width = 50u16.min(area.width.saturating_sub(4));
    let help_height = ((keybindings.len() as u16) + 2).min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(help_width)) / 2;
    let y = (area.height.saturating_sub(help_height)) / 2;
    let help_area = Rect::new(x, y, help_width, help_height);

    f.render_widget(Clear, help_area);

    let lines: Vec<Line> = keybindings
        .into_iter()
        .map(|(key, desc)| {
            Line::from(vec![
                Span::styled(
                    format!("  {key:<12}"),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(desc),
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
