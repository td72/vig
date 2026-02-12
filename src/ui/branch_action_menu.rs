use crate::app::{App, BranchAction};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

const BG: Color = Color::Rgb(30, 30, 30);

fn pad_line(line: Line<'static>, width: usize) -> Line<'static> {
    let content_len: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
    if content_len < width {
        let mut spans = line.spans;
        spans.push(Span::styled(
            " ".repeat(width - content_len),
            Style::default().bg(BG),
        ));
        Line::from(spans)
    } else {
        line
    }
}

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

    let inner_w = menu_width.saturating_sub(2) as usize;
    let mut lines: Vec<Line> = Vec::new();

    // Branch name header
    let name_style = if menu.is_head {
        Style::default()
            .fg(Color::Green)
            .bg(BG)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().bg(BG).add_modifier(Modifier::BOLD)
    };
    lines.push(pad_line(
        Line::from(Span::styled(format!(" {}", menu.branch_name), name_style)),
        inner_w,
    ));
    lines.push(pad_line(
        Line::from(Span::styled(
            " ─────────────────────",
            Style::default().fg(Color::DarkGray).bg(BG),
        )),
        inner_w,
    ));

    // Menu items
    for (idx, action) in BranchAction::ALL.iter().enumerate() {
        let is_selected = idx == menu.selected_idx;
        let key_char = action.key();
        let label = action.label();
        let item_bg = if is_selected { Color::DarkGray } else { BG };
        let style = Style::default()
            .bg(item_bg)
            .add_modifier(if is_selected { Modifier::BOLD } else { Modifier::empty() });
        let key_style = Style::default()
            .fg(Color::Cyan)
            .bg(item_bg)
            .add_modifier(Modifier::BOLD);
        lines.push(pad_line(
            Line::from(vec![
                Span::styled(format!(" {key_char}  "), key_style),
                Span::styled(label.to_string(), style),
            ]),
            inner_w,
        ));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan).bg(BG))
        .style(Style::default().bg(BG));

    let para = Paragraph::new(lines).block(block);
    f.render_widget(para, menu_area);
}
