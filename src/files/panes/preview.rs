//! Right column of the Files page: syntax-highlighted file contents or a
//! directory listing for the selected entry.

use crate::core::app::AppContext;
use crate::core::keymap::{
    nav_bindings, search_bindings, ActionHelp, Keymap, NavAction, SearchAction,
};
use crate::core::pane::{self, Pane, PaneEvent, PaneShared};
use crate::core::search::SearchMatch;
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
    /// Horizontal scroll of long lines.
    ScrollLeft,
    ScrollRight,
    /// Content search (`/`, `n`, `N`) over the shown lines.
    Search(SearchAction),
    /// Return focus to the directory list.
    Back,
    Esc,
}

crate::impl_pane_action_from_str!(
    PreviewAction, nav: Nav, search: Search, esc: Esc,
    ScrollLeft, ScrollRight, Back
);

impl ActionHelp for PreviewAction {
    fn label(&self) -> Option<&'static str> {
        match self {
            PreviewAction::Nav(NavAction::MoveDown) => Some("Scroll down"),
            PreviewAction::Nav(NavAction::MoveUp) => Some("Scroll up"),
            PreviewAction::Nav(nav) => nav.label(),
            PreviewAction::ScrollLeft => Some("Scroll left"),
            PreviewAction::ScrollRight => Some("Scroll right"),
            PreviewAction::Search(sa) => sa.label(),
            PreviewAction::Back => Some("Back to file list"),
            PreviewAction::Esc => Some("Clear search / back to file list"),
        }
    }
}

/// Columns one `h` / `l` press scrolls by.
const HSCROLL_STEP: usize = 8;

pub fn default_keymap() -> Keymap<PreviewAction> {
    Keymap::new()
        .bindings(nav_bindings(PreviewAction::Nav))
        .bindings(search_bindings(PreviewAction::Search))
        .key(KeyCode::Char('h'), PreviewAction::ScrollLeft)
        .key(KeyCode::Left, PreviewAction::ScrollLeft)
        .key(KeyCode::Char('l'), PreviewAction::ScrollRight)
        .key(KeyCode::Right, PreviewAction::ScrollRight)
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
    /// Horizontal scroll: content columns hidden on the left (the line
    /// number gutter of a raw preview stays put).
    scroll_x: usize,
    view_height: u16,
    /// Content columns available for text (from the last render).
    view_width: usize,
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
            scroll_x: 0,
            view_height: 20,
            view_width: 80,
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
        self.scroll_x = 0;
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
        self.scroll_x = 0;
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

    /// The text lines as shown: the rendered Markdown when active, else
    /// the raw file lines. Search and horizontal scroll work on these.
    fn shown_texts(&self) -> Vec<String> {
        if self.markdown_active() {
            return self
                .markdown_lines
                .as_deref()
                .unwrap_or(&[])
                .iter()
                .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
                .collect();
        }
        match &self.content {
            Preview::Text { lines, .. } => lines.clone(),
            _ => Vec::new(),
        }
    }

    /// Width of the line number gutter (`"123 "`) in raw mode, 0 otherwise.
    fn gutter_width(&self) -> usize {
        match (&self.content, self.markdown_active()) {
            (Preview::Text { lines, .. }, false) => lines.len().to_string().len() + 1,
            _ => 0,
        }
    }

    /// Width in chars of the longest shown line (no allocation: the raw
    /// lines and the rendered spans are measured in place).
    fn longest_shown(&self) -> usize {
        if self.markdown_active() {
            return self
                .markdown_lines
                .as_deref()
                .unwrap_or(&[])
                .iter()
                .map(|l| l.spans.iter().map(|s| s.content.chars().count()).sum())
                .max()
                .unwrap_or(0);
        }
        match &self.content {
            Preview::Text { lines, .. } => {
                lines.iter().map(|l| l.chars().count()).max().unwrap_or(0)
            }
            _ => 0,
        }
    }

    /// The furthest `scroll_x` that still shows something: the longest
    /// shown line minus the text width (0 when everything fits).
    fn max_scroll_x(&self) -> usize {
        let text_width = self.view_width.saturating_sub(self.gutter_width()).max(1);
        self.longest_shown().saturating_sub(text_width)
    }

    /// Bring `(row, col)` of the shown text on screen, scrolling both ways
    /// as little as needed.
    fn scroll_into_view(&mut self, row: usize, col_start: usize, col_end: usize) {
        let height = (self.view_height as usize).max(1);
        if row < self.scroll {
            self.scroll = row;
        } else if row >= self.scroll + height {
            self.scroll = row + 1 - height;
        }
        let text_width = self.view_width.saturating_sub(self.gutter_width()).max(1);
        if col_start < self.scroll_x {
            self.scroll_x = col_start.saturating_sub(HSCROLL_STEP);
        } else if col_end > self.scroll_x + text_width {
            self.scroll_x = col_end.saturating_sub(text_width);
        }
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

    fn execute(&mut self, shared: &PaneShared, action: PreviewAction) -> Vec<PaneEvent> {
        let back = vec![PaneEvent::SetFocus(self.list_pane_id)];
        if let Some(events) = pane::try_dispatch_search_esc(&action, shared, self.pane_id, back) {
            return events;
        }
        match action {
            PreviewAction::ScrollLeft => {
                self.scroll_x = self.scroll_x.saturating_sub(HSCROLL_STEP);
                vec![]
            }
            PreviewAction::ScrollRight => {
                self.scroll_x = (self.scroll_x + HSCROLL_STEP).min(self.max_scroll_x());
                vec![]
            }
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
            PreviewAction::Back | PreviewAction::Esc | PreviewAction::Search(_) => {
                vec![PaneEvent::SetFocus(self.list_pane_id)]
            }
        }
    }

    /// The syntax-colored content of raw line `row` (no gutter).
    fn text_spans(&self, row: usize, text: &str) -> Vec<Span<'static>> {
        let mut spans = Vec::new();
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
        spans
    }
}

/// Apply the horizontal scroll and the search highlights to one line's
/// content spans: `hl` holds `(col_start, col_end, is_current)` char
/// ranges in the unscrolled text; the first `skip` chars are dropped.
fn shift_and_highlight(
    spans: Vec<Span<'static>>,
    skip: usize,
    hl: &[(usize, usize, bool)],
) -> Vec<Span<'static>> {
    if skip == 0 && hl.is_empty() {
        return spans;
    }
    let mut out: Vec<Span<'static>> = Vec::new();
    let mut run = String::new();
    let mut run_style = Style::default();
    let mut col = 0usize;
    for span in spans {
        for ch in span.content.chars() {
            let c = col;
            col += 1;
            if c < skip {
                continue;
            }
            let mut style = span.style;
            if let Some((_, _, current)) = hl.iter().find(|(s, e, _)| c >= *s && c < *e) {
                style = if *current {
                    style
                        .bg(theme::SEARCH_CURRENT_BG)
                        .fg(theme::SEARCH_CURRENT_FG)
                } else {
                    style.bg(theme::SEARCH_MATCH_BG)
                };
            }
            if style != run_style && !run.is_empty() {
                out.push(Span::styled(std::mem::take(&mut run), run_style));
            }
            run_style = style;
            run.push(ch);
        }
    }
    if !run.is_empty() {
        out.push(Span::styled(run, run_style));
    }
    out
}

/// One-char case folding: columns must stay those of the original text,
/// so a char that lowercases to several (`İ` → `i̇`) keeps its first.
fn fold_char(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// Every start index of `needle` in `hay` (overlaps allowed), by char.
fn find_all(hay: &[char], needle: &[char]) -> Vec<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return Vec::new();
    }
    (0..=hay.len() - needle.len())
        .filter(|&i| hay[i..i + needle.len()] == *needle)
        .collect()
}

/// `(col_start, col_end, is_current)` of this pane's matches per row.
fn match_ranges(shared: &PaneShared, pane_id: usize) -> Vec<(usize, usize, usize, bool)> {
    if shared.search.origin != pane_id || shared.search.query.is_none() {
        return Vec::new();
    }
    let current = shared.search.current_match_idx;
    shared
        .search
        .matches
        .iter()
        .enumerate()
        .filter_map(|(i, m)| match m {
            SearchMatch::TextLine {
                row,
                col_start,
                col_end,
            } => Some((*row, *col_start, *col_end, current == Some(i))),
            _ => None,
        })
        .collect()
}

fn colored(text: String, color: Option<Color>) -> Span<'static> {
    match color {
        Some(c) => Span::styled(text, Style::default().fg(c)),
        None => Span::raw(text),
    }
}

impl Pane<PaneEvent> for PreviewPane {
    crate::impl_handle_key!(keymap);

    /// Content search over the shown lines (rendered Markdown or raw
    /// text), case-insensitive, one match per occurrence.
    fn collect_search_matches(&self, _shared: &PaneShared, query: &str) -> Vec<SearchMatch> {
        let needle: Vec<char> = query.chars().map(fold_char).collect();
        if needle.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        for (row, text) in self.shown_texts().iter().enumerate() {
            let hay: Vec<char> = text.chars().map(fold_char).collect();
            for col_start in find_all(&hay, &needle) {
                out.push(SearchMatch::TextLine {
                    row,
                    col_start,
                    col_end: col_start + needle.len(),
                });
            }
        }
        out
    }

    fn jump_to_match(&mut self, _shared: &PaneShared, search_match: &SearchMatch) {
        if let SearchMatch::TextLine {
            row,
            col_start,
            col_end,
        } = search_match
        {
            self.scroll_into_view(*row, *col_start, *col_end);
        }
    }

    fn render(&mut self, f: &mut Frame, _ctx: &AppContext, shared: &PaneShared, area: Rect) {
        self.view_height = area.height.saturating_sub(2);
        if self.markdown_active() {
            // Rebuild for the current width before clamping the scroll below:
            // a reflow can shrink the line count.
            self.markdown_lines(area.width.saturating_sub(2) as usize);
        }
        // A taller terminal (or a reflow) lowers the maximum scroll; never
        // leave the view blank.
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
        self.view_width = width;
        self.scroll_x = self.scroll_x.min(self.max_scroll_x());
        let dim = Style::default().fg(Color::DarkGray);
        let ranges = match_ranges(shared, self.pane_id);
        let row_hl = |row: usize| -> Vec<(usize, usize, bool)> {
            ranges
                .iter()
                .filter(|(r, ..)| *r == row)
                .map(|(_, s, e, c)| (*s, *e, *c))
                .collect()
        };
        let skip = self.scroll_x;

        let lines: Vec<Line> = if self.markdown_active() {
            let truncated = matches!(
                &self.content,
                Preview::Text {
                    truncated: true,
                    ..
                }
            );
            let md = self.markdown_lines.as_deref().unwrap_or(&[]);
            let mut out: Vec<Line> = md
                .iter()
                .enumerate()
                .skip(self.scroll)
                .take(height)
                .map(|(row, line)| {
                    let base = line.style;
                    Line::from(shift_and_highlight(line.spans.clone(), skip, &row_hl(row)))
                        .style(base)
                })
                .collect();
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
                        .map(|(row, text)| {
                            let mut spans = vec![Span::styled(
                                format!("{:>gutter$} ", row + 1),
                                Style::default().fg(Color::DarkGray),
                            )];
                            spans.extend(shift_and_highlight(
                                self.text_spans(row, text),
                                skip,
                                &row_hl(row),
                            ));
                            Line::from(spans)
                        })
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

    fn shared_for(p: &PreviewPane) -> PaneShared {
        PaneShared {
            focused_pane: p.pane_id,
            previous_pane: p.list_pane_id,
            search: crate::core::search::SearchState::new(),
        }
    }

    #[test]
    fn content_search_finds_char_ranges_in_raw_and_rendered_text() {
        let p = pane_with(
            "a.rs",
            &["fn main() {", "    let Needle = needle();", "}"],
            false,
        );
        let m = p.collect_search_matches(&shared_for(&p), "needle");
        assert_eq!(
            m,
            vec![
                SearchMatch::TextLine {
                    row: 1,
                    col_start: 8,
                    col_end: 14
                },
                SearchMatch::TextLine {
                    row: 1,
                    col_start: 17,
                    col_end: 23
                },
            ]
        );
        assert!(p.collect_search_matches(&shared_for(&p), "").is_empty());
        // Columns are those of the original text even when a char
        // lowercases to several (`İ` → `i̇`).
        let p = pane_with("t.txt", &["İstanbul needle"], false);
        let m = p.collect_search_matches(&shared_for(&p), "NEEDLE");
        assert_eq!(
            m,
            vec![SearchMatch::TextLine {
                row: 0,
                col_start: 9,
                col_end: 15
            }]
        );
        // Rendered markdown: the match sits in the rendered line, not the source.
        let mut p = pane_with("a.md", &["# Title", "", "some **bold** word"], true);
        p.markdown_lines(80);
        let m = p.collect_search_matches(&shared_for(&p), "bold");
        assert_eq!(m.len(), 1);
        let SearchMatch::TextLine { row, col_start, .. } = m[0] else {
            panic!("text match");
        };
        let shown = p.shown_texts();
        assert_eq!(&shown[row][col_start..col_start + 4], "bold");
    }

    #[test]
    fn jump_scrolls_the_match_into_view_both_ways() {
        let lines: Vec<String> = (0..50)
            .map(|i| format!("{:>3}: {}needle{}", i, "x".repeat(60), i))
            .collect();
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let mut p = pane_with("long.txt", &refs, false);
        p.view_height = 10;
        p.view_width = 40;
        let sh = shared_for(&p);
        p.jump_to_match(
            &sh,
            &SearchMatch::TextLine {
                row: 30,
                col_start: 65,
                col_end: 71,
            },
        );
        assert!(
            p.scroll <= 30 && 30 < p.scroll + 10,
            "row on screen: {}",
            p.scroll
        );
        let text_width = 40 - p.gutter_width();
        assert!(
            p.scroll_x <= 65 && 71 <= p.scroll_x + text_width,
            "col on screen: {}",
            p.scroll_x
        );
        // Jumping back to a match near the start scrolls left again.
        p.jump_to_match(
            &sh,
            &SearchMatch::TextLine {
                row: 2,
                col_start: 0,
                col_end: 3,
            },
        );
        assert_eq!(p.scroll, 2);
        assert_eq!(p.scroll_x, 0);
    }

    #[test]
    fn horizontal_scroll_clamps_to_the_longest_line() {
        let mut p = pane_with("w.txt", &["short", &"y".repeat(30)], false);
        p.view_width = 20;
        let sh = shared_for(&p);
        for _ in 0..10 {
            p.execute(&sh, PreviewAction::ScrollRight);
        }
        assert_eq!(p.scroll_x, p.max_scroll_x());
        assert!(p.scroll_x > 0);
        for _ in 0..10 {
            p.execute(&sh, PreviewAction::ScrollLeft);
        }
        assert_eq!(p.scroll_x, 0);
        // Everything fits: no horizontal scroll at all.
        p.view_width = 80;
        p.execute(&sh, PreviewAction::ScrollRight);
        assert_eq!(p.scroll_x, 0);
    }

    #[test]
    fn shift_and_highlight_drops_scrolled_chars_and_marks_matches() {
        let spans = vec![Span::raw("abcdef"), Span::raw("gh")];
        let out = shift_and_highlight(spans, 2, &[(3, 5, true)]);
        let text: String = out.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "cdefgh");
        let hit = out
            .iter()
            .find(|s| s.content.as_ref() == "de")
            .expect("highlighted run");
        assert_eq!(hit.style.bg, Some(theme::SEARCH_CURRENT_BG));
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
