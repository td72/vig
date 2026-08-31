//! Pre-app trust prompt for a tracked repository-local `.vig.kdl`.
//!
//! Runs its own tiny event loop *before* the app exists, because the answer
//! decides the config the app is built from (pages, layouts, keybindings).
//! `[v]` shows the file content in a scrollable view so the user can read
//! what they are about to load.

use crate::core::tui::Tui;
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
use std::path::Path;

use super::pad_line;

const BG: Color = Color::Rgb(30, 30, 30);
const ACCENT: Color = Color::Cyan;

/// What the user chose in the dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustChoice {
    /// `[y]`: load the layer and remember the decision.
    LoadRemember,
    /// `[n]`: ignore the layer and remember the decision.
    IgnoreRemember,
    /// `Esc` / `q` / `Ctrl+c`: ignore for this run only, ask again next time.
    IgnoreOnce,
}

/// Block until the user decides about the tracked `.vig.kdl` at `path`.
pub fn run(terminal: &mut Tui, path: &Path, text: &str) -> Result<TrustChoice> {
    let mut viewing = false;
    let mut scroll: u16 = 0;
    let line_count = text.lines().count() as u16;
    loop {
        terminal.draw(|f| {
            let area = f.area();
            if viewing {
                render_viewer(f, area, path, text, scroll);
            } else {
                render_dialog(f, area, path);
            }
        })?;
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Ok(TrustChoice::IgnoreOnce)
            }
            KeyCode::Char('y') => return Ok(TrustChoice::LoadRemember),
            KeyCode::Char('n') => return Ok(TrustChoice::IgnoreRemember),
            KeyCode::Char('v') => viewing = true,
            KeyCode::Esc | KeyCode::Char('q') if viewing => viewing = false,
            KeyCode::Esc | KeyCode::Char('q') => return Ok(TrustChoice::IgnoreOnce),
            KeyCode::Char('j') | KeyCode::Down if viewing => {
                scroll = (scroll + 1).min(line_count.saturating_sub(1));
            }
            KeyCode::Char('k') | KeyCode::Up if viewing => scroll = scroll.saturating_sub(1),
            KeyCode::Char('g') if viewing => scroll = 0,
            KeyCode::Char('G') if viewing => scroll = line_count.saturating_sub(1),
            _ => {}
        }
    }
}

fn render_dialog(f: &mut Frame, area: Rect, path: &Path) {
    let message = vec![
        "This repository ships a tracked .vig.kdl:".to_string(),
        String::new(),
        format!("  {}", path.display()),
        String::new(),
        "It is a full vig config (pages, layouts, keybindings)".to_string(),
        "provided by the repository, not by you.".to_string(),
        String::new(),
        "[y] load and remember    [n] ignore and remember".to_string(),
        "[v] view the file        [Esc] ignore this time".to_string(),
    ];

    let dialog_width = 58u16.min(area.width.saturating_sub(4));
    let inner_w = dialog_width.saturating_sub(2) as usize;
    let total_lines = 2 + message.len(); // title + blank + message
    let dialog_height = (total_lines as u16 + 2).min(area.height.saturating_sub(2));
    let x = (area.width.saturating_sub(dialog_width)) / 2;
    let y = (area.height.saturating_sub(dialog_height)) / 2;
    let dialog_area = Rect::new(x, y, dialog_width, dialog_height);

    f.render_widget(Clear, dialog_area);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(pad_line(
        Line::from(Span::styled(
            " Repository-local config",
            Style::default()
                .fg(ACCENT)
                .bg(BG)
                .add_modifier(Modifier::BOLD),
        )),
        inner_w,
        BG,
    ));
    lines.push(pad_line(
        Line::from(Span::styled(String::new(), Style::default().bg(BG))),
        inner_w,
        BG,
    ));
    for (i, msg) in message.iter().enumerate() {
        let style = if i >= message.len() - 2 {
            Style::default().fg(Color::Yellow).bg(BG)
        } else {
            Style::default().fg(Color::White).bg(BG)
        };
        lines.push(pad_line(
            Line::from(Span::styled(format!(" {msg}"), style)),
            inner_w,
            BG,
        ));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT).bg(BG))
        .style(Style::default().bg(BG));
    f.render_widget(Paragraph::new(lines).block(block), dialog_area);
}

fn render_viewer(f: &mut Frame, area: Rect, path: &Path, text: &str, scroll: u16) {
    let margin_x = area.width / 10;
    let margin_y = area.height / 10;
    let view_area = Rect::new(
        margin_x,
        margin_y,
        area.width.saturating_sub(margin_x * 2).max(20),
        area.height.saturating_sub(margin_y * 2).max(6),
    );

    f.render_widget(Clear, view_area);

    let lines: Vec<Line> = text
        .lines()
        .map(|l| {
            Line::from(Span::styled(
                l.to_string(),
                Style::default().fg(Color::White).bg(BG),
            ))
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT).bg(BG))
        .style(Style::default().bg(BG))
        .title(format!(" {} ", path.display()))
        .title_bottom(" j/k scroll · y load · n ignore · Esc back ");
    f.render_widget(
        Paragraph::new(lines).block(block).scroll((scroll, 0)),
        view_area,
    );
}
