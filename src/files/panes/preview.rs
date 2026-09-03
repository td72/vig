//! Right column of the Files page: syntax-highlighted file contents or a
//! directory listing for the selected entry.

use crate::core::app::AppContext;
use crate::core::keymap::{nav_bindings, ActionHelp, Keymap, NavAction};
use crate::core::pane::{Pane, PaneEvent, PaneShared};
use crate::core::syntax::SyntaxHighlighter;
use crate::core::theme;
use crate::files::domain::fs::{self, DirEntry, Preview};
use crate::files::domain::image::IMAGE_MAX_BYTES;
use crate::files::panes::entry_line;
use crossterm::event::KeyCode;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::{layout::Rect, Frame};
use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;
use ratatui_image::StatefulImage;

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
    icons: bool,
    /// `None` when image previews are disabled (`image-preview "none"`).
    picker: Option<Picker>,
    /// Decoded image for `Preview::Image`, ready to draw.
    image: Option<StatefulProtocol>,
    /// An image was just replaced or removed: Sixel / iTerm2 output outside
    /// the new content is not covered by ratatui's cell diff, so the screen
    /// must be cleared once.
    needs_full_redraw: bool,
    /// Whether Markdown files are rendered (session toggle, seeded from the
    /// `markdown-preview` config node).
    markdown: bool,
    /// Rendered lines for a Markdown preview, rebuilt when the pane width
    /// changes (tables are fitted to it).
    markdown_lines: Option<Vec<Line<'static>>>,
    /// The width `markdown_lines` was rendered for.
    markdown_width: usize,
}

impl PreviewPane {
    pub fn new(
        pane_id: usize,
        list_pane_id: usize,
        theme: &str,
        icons: bool,
        picker: Option<Picker>,
        markdown: bool,
    ) -> Self {
        Self {
            icons,
            picker,
            image: None,
            needs_full_redraw: false,
            markdown,
            markdown_lines: None,
            markdown_width: 0,
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
        self.markdown_lines = None;
        self.content = match entry {
            Some(e) => fs::preview(e),
            None => Preview::Empty,
        };
        if let (Some(e), Preview::Text { lines, .. }) = (entry, &self.content) {
            self.colors = self
                .highlighter
                .highlight_lines(&e.path.to_string_lossy(), lines);
        }
        if self.image.take().is_some() {
            self.needs_full_redraw = true;
        }
        if let (Some(e), Preview::Image(_), Some(picker)) = (entry, &self.content, &self.picker) {
            if e.size <= IMAGE_MAX_BYTES {
                if let Ok(img) = ::image::open(&e.path) {
                    self.image = Some(picker.new_resize_protocol(img));
                }
            }
        }
    }

    /// Consume the pending full-redraw request (see `needs_full_redraw`).
    pub fn take_full_redraw(&mut self) -> bool {
        std::mem::take(&mut self.needs_full_redraw)
    }

    /// Toggle Markdown rendering for the current session.
    pub fn toggle_markdown(&mut self) {
        self.markdown = !self.markdown;
        self.scroll = 0;
    }

    /// Whether the selected entry is a Markdown file (by extension).
    pub fn is_markdown_entry(&self) -> bool {
        self.entry.as_ref().is_some_and(|e| {
            !e.is_dir
                && e.path
                    .extension()
                    .and_then(|x| x.to_str())
                    .is_some_and(|x| {
                        x.eq_ignore_ascii_case("md") || x.eq_ignore_ascii_case("markdown")
                    })
        })
    }

    /// Whether the preview currently renders Markdown (file + toggle).
    pub fn markdown_active(&self) -> bool {
        self.markdown && self.is_markdown_entry() && matches!(self.content, Preview::Text { .. })
    }

    /// Build (or reuse) the rendered Markdown lines for `width` columns.
    ///
    /// A YAML front matter block (`---` ... `---` at the very top) is not fed
    /// to the renderer; its lines are shown as-is in a dim style.
    fn markdown_lines(&mut self, width: usize) -> &[Line<'static>] {
        if self.markdown_lines.is_none() || self.markdown_width != width {
            let Preview::Text { lines, .. } = &self.content else {
                self.markdown_lines = Some(Vec::new());
                return self.markdown_lines.as_deref().unwrap();
            };
            let dim = Style::default().fg(Color::DarkGray);
            let mut out: Vec<Line<'static>> = Vec::new();
            let mut body_start = 0;
            if lines.first().map(String::as_str) == Some("---") {
                if let Some(end) = lines.iter().skip(1).position(|l| l == "---") {
                    for l in &lines[..end + 2] {
                        out.push(Line::from(Span::styled(format!(" {l}"), dim)));
                    }
                    body_start = end + 2;
                }
            }
            let body = lines[body_start..].join("\n");
            out.extend(crate::core::ui::markdown::markdown_to_lines(
                &body, " ", width,
            ));
            self.markdown_width = width;
            self.markdown_lines = Some(out);
        }
        self.markdown_lines.as_deref().unwrap()
    }

    /// Status line shown above an image: `PNG 1920×1080  2.3M`, plus why the
    /// image itself is not drawn, if it is not.
    fn image_lines(&self, dim: Style) -> Vec<Line<'static>> {
        let Preview::Image(info) = &self.content else {
            return vec![];
        };
        let size = self.entry.as_ref().map_or(0, |e| e.size);
        let protocol = match &self.picker {
            Some(p) if self.image.is_some() => format!("  {:?}", p.protocol_type()).to_lowercase(),
            _ => String::new(),
        };
        let mut lines = vec![Line::from(Span::styled(
            format!(
                "  {} {}×{}  {}{protocol}",
                info.format,
                info.width,
                info.height,
                fs::human_size(size)
            ),
            dim,
        ))];
        if self.image.is_none() {
            let why = if self.picker.is_none() {
                "(image preview disabled)"
            } else if size > IMAGE_MAX_BYTES {
                "(image too large to preview)"
            } else {
                "(could not decode image)"
            };
            lines.push(Line::from(Span::styled(format!("  {why}"), dim)));
        }
        lines
    }

    fn line_count(&self) -> usize {
        if self.markdown_active() {
            if let Some(md) = &self.markdown_lines {
                return md.len();
            }
        }
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
        // A taller terminal lowers the maximum scroll; never leave the view blank.
        self.scroll = self
            .scroll
            .min(self.line_count().saturating_sub(self.view_height as usize));
        let mut title = self
            .entry
            .as_ref()
            .map(|e| e.display_name())
            .unwrap_or_else(|| "Preview".to_string());
        if self.is_markdown_entry() && matches!(self.content, Preview::Text { .. }) {
            title.push_str(if self.markdown { "  markdown" } else { "  raw" });
        }
        let block = theme::pane_block(&title, shared.focused_pane == self.pane_id);
        let height = self.view_height as usize;
        let width = area.width.saturating_sub(2) as usize;
        let dim = Style::default().fg(Color::DarkGray);

        if self.markdown_active() {
            // Build for the current width before borrowing content below.
            self.markdown_lines(width);
        }
        let lines: Vec<Line> = if self.markdown_active() {
            let truncated = matches!(
                &self.content,
                Preview::Text {
                    truncated: true,
                    ..
                }
            );
            let md = self.markdown_lines.as_deref().unwrap_or(&[]);
            let mut out: Vec<Line> = md.iter().skip(self.scroll).take(height).cloned().collect();
            if truncated && self.scroll + height >= md.len() {
                out.push(Line::from(Span::styled(" … (truncated)", dim)));
            }
            out
        } else {
            match &self.content {
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
                    .map(|e| entry_line(e, width, self.icons))
                    .collect(),
                Preview::Image(_) => self.image_lines(dim),
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
            }
        };
        let inner = block.inner(area);
        f.render_widget(Paragraph::new(lines).block(block), area);
        if let Some(protocol) = self.image.as_mut() {
            // Below the metadata line, inside the border.
            let img_area = Rect {
                y: inner.y.saturating_add(1),
                height: inner.height.saturating_sub(1),
                ..inner
            };
            if img_area.height > 0 && img_area.width > 0 {
                f.render_stateful_widget(StatefulImage::default(), img_area, protocol);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn pane_with(name: &str, lines: &[&str], markdown: bool) -> PreviewPane {
        let mut p = PreviewPane::new(0, 1, "base16-ocean.dark", false, None, markdown);
        p.entry = Some(DirEntry {
            name: name.to_string(),
            path: PathBuf::from(name),
            is_dir: false,
            is_symlink: false,
            size: 1,
        });
        p.content = Preview::Text {
            lines: lines.iter().map(|s| s.to_string()).collect(),
            truncated: false,
        };
        p
    }

    fn texts(lines: &[Line<'_>]) -> Vec<String> {
        lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect()
    }

    #[test]
    fn markdown_detection_is_by_extension() {
        assert!(pane_with("a.md", &[], true).is_markdown_entry());
        assert!(pane_with("a.MD", &[], true).is_markdown_entry());
        assert!(pane_with("b.markdown", &[], true).is_markdown_entry());
        assert!(!pane_with("c.rs", &[], true).is_markdown_entry());
        assert!(!pane_with("README", &[], true).is_markdown_entry());
    }

    #[test]
    fn toggle_flips_markdown_and_resets_scroll() {
        let mut p = pane_with("a.md", &["# t"], true);
        assert!(p.markdown_active());
        p.scroll = 3;
        p.toggle_markdown();
        assert!(!p.markdown_active());
        assert_eq!(p.scroll, 0);
        p.toggle_markdown();
        assert!(p.markdown_active());
    }

    #[test]
    fn raw_default_needs_toggle_to_render() {
        let p = pane_with("a.md", &["# t"], false);
        assert!(!p.markdown_active());
    }

    #[test]
    fn front_matter_is_kept_verbatim_and_body_rendered() {
        let mut p = pane_with("a.md", &["---", "title: x", "---", "# Head", "body"], true);
        let lines = texts(p.markdown_lines(80));
        assert_eq!(lines[0], " ---");
        assert_eq!(lines[1], " title: x");
        assert_eq!(lines[2], " ---");
        assert!(lines.contains(&" # Head".to_string()), "{lines:?}");
    }

    #[test]
    fn markdown_lines_reflow_on_width_change() {
        let mut p = pane_with(
            "a.md",
            &["| a | b |", "|---|---|", "| one two three four | x |"],
            true,
        );
        let wide = texts(p.markdown_lines(80));
        let narrow = texts(p.markdown_lines(16));
        assert_ne!(wide, narrow);
        assert!(narrow.iter().all(|l| l.chars().count() <= 16), "{narrow:?}");
    }
}
