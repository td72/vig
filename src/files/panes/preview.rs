//! Right column of the Files page: syntax-highlighted file contents or a
//! directory listing for the selected entry.

use crate::core::app::AppContext;
use crate::core::keymap::{nav_bindings, ActionHelp, Keymap, NavAction};
use crate::core::pane::{Pane, PaneEvent, PaneShared};
use crate::core::syntax::SyntaxHighlighter;
use crate::core::theme;
use crate::files::domain::fs::{self, DirEntry, Preview};
use crate::files::panes::entry_line;
use crossterm::event::KeyCode;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::{layout::Rect, Frame};

#[derive(Debug, Clone)]
pub enum PreviewAction {
    Nav(NavAction),
    /// Return focus to the directory list.
    Back,
    Esc,
}

crate::impl_pane_action_from_str!(PreviewAction, nav: Nav, Back, Esc);

impl ActionHelp for PreviewAction {
    fn label(&self) -> Option<&'static str> {
        match self {
            PreviewAction::Nav(NavAction::MoveDown) => Some("Scroll down"),
            PreviewAction::Nav(NavAction::MoveUp) => Some("Scroll up"),
            PreviewAction::Nav(nav) => nav.label(),
            PreviewAction::Back => Some("Back to file list"),
            PreviewAction::Esc => Some("Back to file list"),
        }
    }
}

pub fn default_keymap() -> Keymap<PreviewAction> {
    Keymap::new()
        .bindings(nav_bindings(PreviewAction::Nav))
        .key(KeyCode::Char('h'), PreviewAction::Back)
        .key(KeyCode::Left, PreviewAction::Back)
        .key(KeyCode::Esc, PreviewAction::Esc)
}

pub struct PreviewPane {
    pane_id: usize,
    list_pane_id: usize,
    keymap: Keymap<PreviewAction>,
    highlighter: SyntaxHighlighter,
    entry: Option<DirEntry>,
    content: Preview,
    colors: Option<Vec<Vec<Color>>>,
    scroll: usize,
    view_height: u16,
}

impl PreviewPane {
    pub fn new(pane_id: usize, list_pane_id: usize, theme: &str) -> Self {
        Self {
            pane_id,
            list_pane_id,
            keymap: default_keymap(),
            highlighter: SyntaxHighlighter::with_theme(theme).unwrap_or_default(),
            entry: None,
            content: Preview::Empty,
            colors: None,
            scroll: 0,
            view_height: 20,
        }
    }

    pub fn set_keymap(&mut self, km: Keymap<PreviewAction>) {
        self.keymap = km;
    }

    pub fn keymap(&self) -> &Keymap<PreviewAction> {
        &self.keymap
    }

    /// Load the preview for `entry` (or clear it).
    pub fn load(&mut self, entry: Option<&DirEntry>) {
        self.scroll = 0;
        self.entry = entry.cloned();
        self.colors = None;
        self.content = match entry {
            Some(e) => fs::preview(e),
            None => Preview::Empty,
        };
        if let (Some(e), Preview::Text { lines, .. }) = (entry, &self.content) {
            self.colors = self
                .highlighter
                .highlight_lines(&e.path.to_string_lossy(), lines);
        }
    }

    fn line_count(&self) -> usize {
        match &self.content {
            Preview::Text { lines, .. } => lines.len(),
            Preview::Dir(entries) => entries.len(),
            _ => 0,
        }
    }

    fn execute(&mut self, _shared: &PaneShared, action: PreviewAction) -> Vec<PaneEvent> {
        match action {
            PreviewAction::Nav(nav) => {
                let max = self.line_count().saturating_sub(self.view_height as usize);
                let half = crate::core::keymap::half_page_step(self.view_height) as usize;
                self.scroll = match nav {
                    NavAction::MoveDown => self.scroll + 1,
                    NavAction::MoveUp => self.scroll.saturating_sub(1),
                    NavAction::HalfPageDown => self.scroll + half,
                    NavAction::HalfPageUp => self.scroll.saturating_sub(half),
                    NavAction::JumpTop => 0,
                    NavAction::JumpBottom => max,
                }
                .min(max);
                vec![]
            }
            PreviewAction::Back | PreviewAction::Esc => {
                vec![PaneEvent::SetFocus(self.list_pane_id)]
            }
        }
    }

    fn text_line(&self, row: usize, text: &str, gutter: usize) -> Line<'static> {
        let mut spans = vec![Span::styled(
            format!("{:>gutter$} ", row + 1),
            Style::default().fg(Color::DarkGray),
        )];
        match self.colors.as_ref().and_then(|c| c.get(row)) {
            Some(colors) if !colors.is_empty() => {
                // Group runs of identical color into one span.
                let mut run = String::new();
                let mut run_color: Option<Color> = None;
                for (i, ch) in text.chars().enumerate() {
                    let color = colors.get(i).copied();
                    if color != run_color && !run.is_empty() {
                        spans.push(colored(std::mem::take(&mut run), run_color));
                    }
                    run_color = color;
                    run.push(ch);
                }
                if !run.is_empty() {
                    spans.push(colored(run, run_color));
                }
            }
            _ => spans.push(Span::raw(text.to_string())),
        }
        Line::from(spans)
    }
}

fn colored(text: String, color: Option<Color>) -> Span<'static> {
    match color {
        Some(c) => Span::styled(text, Style::default().fg(c)),
        None => Span::raw(text),
    }
}

impl Pane<PaneEvent> for PreviewPane {
    crate::impl_handle_key!(keymap);

    fn render(&mut self, f: &mut Frame, _ctx: &AppContext, shared: &PaneShared, area: Rect) {
        self.view_height = area.height.saturating_sub(2);
        let title = self
            .entry
            .as_ref()
            .map(|e| e.display_name())
            .unwrap_or_else(|| "Preview".to_string());
        let block = theme::pane_block(&title, shared.focused_pane == self.pane_id);
        let height = self.view_height as usize;
        let width = area.width.saturating_sub(2) as usize;
        let dim = Style::default().fg(Color::DarkGray);

        let lines: Vec<Line> = match &self.content {
            Preview::Text { lines, truncated } => {
                let gutter = lines.len().to_string().len();
                let mut out: Vec<Line> = lines
                    .iter()
                    .enumerate()
                    .skip(self.scroll)
                    .take(height)
                    .map(|(row, text)| self.text_line(row, text, gutter))
                    .collect();
                if *truncated && self.scroll + height >= lines.len() {
                    out.push(Line::from(Span::styled(" … (truncated)", dim)));
                }
                out
            }
            Preview::Dir(entries) if entries.is_empty() => {
                vec![Line::from(Span::styled("  (empty directory)", dim))]
            }
            Preview::Dir(entries) => entries
                .iter()
                .skip(self.scroll)
                .take(height)
                .map(|e| entry_line(e, width))
                .collect(),
            Preview::Binary => vec![Line::from(Span::styled("  (binary file)", dim))],
            Preview::Empty => vec![Line::from(Span::styled(
                if self.entry.is_some() {
                    "  (empty file)"
                } else {
                    "  Select a file to preview"
                },
                dim,
            ))],
            Preview::Error(e) => vec![Line::from(Span::styled(
                format!("  {e}"),
                Style::default().fg(Color::Red),
            ))],
        };
        f.render_widget(Paragraph::new(lines).block(block), area);
    }
}
