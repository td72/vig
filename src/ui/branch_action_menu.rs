use crate::app::{App, BranchAction};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let menu = match &app.branch_action_menu {
        Some(m) => m,
        None => return,
    };

    let menu_width = 25u16.min(area.width.saturating_sub(4));
    let menu_height = (BranchAction::ALL.len() as u16 + 4).min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(menu_width)) / 2;
    let y = (area.height.saturating_sub(menu_height)) / 2;
    let menu_area = Rect::new(x, y, menu_width, menu_height);

    f.render_widget(Clear, menu_area);

    let mut lines: Vec<Line> = Vec::new();

    // Branch name header
    let name_style = if menu.is_head {
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().add_modifier(Modifier::BOLD)
    };
    lines.push(Line::from(Span::styled(
        format!(" {}", menu.branch_name),
        name_style,
    )));
    lines.push(Line::from(Span::styled(
        " ─────────────────────",
        Style::default().fg(Color::DarkGray),
    )));

    // Menu items
    for (idx, action) in BranchAction::ALL.iter().enumerate() {
        let is_selected = idx == menu.selected_idx;
        let key_char = action.key();
        let label = action.label();
        let style = if is_selected {
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let key_style = if is_selected {
            Style::default()
                .fg(Color::Cyan)
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        };
        lines.push(Line::from(vec![
            Span::styled(format!(" {key_char}  "), key_style),
            Span::styled(label.to_string(), style),
        ]));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let para = Paragraph::new(lines).block(block);
    f.render_widget(para, menu_area);
}
