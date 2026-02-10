use crate::app::App;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

pub fn render_header(f: &mut Frame, app: &App, area: Rect) {
    let title = Line::from(vec![
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
            Style::default()
                .fg(Color::Black)
                .bg(Color::Magenta),
        ),
        Span::raw("  "),
        Span::styled("? help", Style::default().fg(Color::DarkGray)),
    ]);
    f.render_widget(Paragraph::new(title), area);
}

pub fn render_status_bar(f: &mut Frame, app: &App, area: Rect) {
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

pub fn render_help_overlay(f: &mut Frame, area: Rect) {
    use ratatui::widgets::{Block, Borders, Clear};

    let help_width = 50u16.min(area.width.saturating_sub(4));
    let help_height = 22u16.min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(help_width)) / 2;
    let y = (area.height.saturating_sub(help_height)) / 2;
    let help_area = Rect::new(x, y, help_width, help_height);

    f.render_widget(Clear, help_area);

    let keybindings = vec![
        ("j / ↓", "Next file / Scroll down"),
        ("k / ↑", "Prev file / Scroll up"),
        ("Enter", "Select file → Diff"),
        ("Tab", "Switch pane"),
        ("Ctrl+d", "Half page down"),
        ("Ctrl+u", "Half page up"),
        ("g / G", "Top / Bottom"),
        ("h / l", "Scroll left / right"),
        ("i", "Normal mode (cursor)"),
        ("v / V", "Visual / Visual Line"),
        ("y", "Yank (copy) selection"),
        ("Esc", "Back to file tree"),
        ("e", "Open in $EDITOR"),
        ("r", "Refresh diff"),
        ("?", "Toggle help"),
        ("q", "Quit"),
    ];

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
