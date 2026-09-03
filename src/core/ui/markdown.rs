//! Markdown rendering shared by the GitHub / Projects detail views and the
//! Files preview: a `pulldown-cmark` walk producing styled ratatui lines,
//! including fixed-width GFM tables fitted to the pane width.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// Render a markdown body as styled lines. `max_width` is the text width
/// available in the pane (including `padding`); block elements that cannot
/// wrap gracefully (tables) are fitted into it.
pub fn markdown_to_lines(text: &str, padding: &str, max_width: usize) -> Vec<Line<'static>> {
    use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

    let opts = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS | Options::ENABLE_TABLES;
    let parser = Parser::new_ext(text, opts);

    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut current_spans: Vec<Span<'static>> = Vec::new();
    let mut style_stack: Vec<Style> = Vec::new();
    let mut in_code_block = false;
    let mut in_heading = false;
    let mut in_list_item = false;
    let mut heading_style = Style::default();
    let code_style = Style::default().fg(Color::DarkGray);
    // Table cells are buffered until `TagEnd::Table` so that column widths can
    // be computed over the whole table before emitting any line.
    let mut table_aligns: Vec<pulldown_cmark::Alignment> = Vec::new();
    let mut table_rows: Vec<Vec<Vec<Span<'static>>>> = Vec::new();
    let mut table_row: Vec<Vec<Span<'static>>> = Vec::new();

    let flush_line = |lines: &mut Vec<Line<'static>>,
                      spans: &mut Vec<Span<'static>>,
                      padding: &str,
                      line_style: Style| {
        spans.insert(0, Span::raw(padding.to_string()));
        lines.push(Line::from(std::mem::take(spans)).style(line_style));
    };

    let current_style = |stack: &[Style]| -> Style {
        stack
            .iter()
            .copied()
            .fold(Style::default(), |acc, s| acc.patch(s))
    };

    for event in parser {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                if !lines.is_empty() {
                    flush_line(&mut lines, &mut current_spans, padding, Style::default());
                }
                in_heading = true;
                heading_style = match level {
                    HeadingLevel::H1 => Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                    HeadingLevel::H2 => Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                    HeadingLevel::H3 => Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD | Modifier::ITALIC),
                    _ => Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::ITALIC),
                };
                let prefix = match level {
                    HeadingLevel::H1 => "# ",
                    HeadingLevel::H2 => "## ",
                    HeadingLevel::H3 => "### ",
                    HeadingLevel::H4 => "#### ",
                    HeadingLevel::H5 => "##### ",
                    HeadingLevel::H6 => "###### ",
                };
                current_spans.push(Span::raw(prefix.to_string()));
            }
            Event::End(TagEnd::Heading(_)) => {
                flush_line(&mut lines, &mut current_spans, padding, heading_style);
                in_heading = false;
                heading_style = Style::default();
            }
            Event::Start(Tag::Paragraph)
                if !lines.is_empty() && !in_code_block && !in_list_item =>
            {
                flush_line(&mut lines, &mut current_spans, padding, Style::default());
            }
            Event::End(TagEnd::Paragraph) => {
                let style = if in_heading {
                    heading_style
                } else {
                    Style::default()
                };
                flush_line(&mut lines, &mut current_spans, padding, style);
            }
            Event::Start(Tag::CodeBlock(_)) => {
                in_code_block = true;
                flush_line(&mut lines, &mut current_spans, padding, Style::default());
            }
            Event::End(TagEnd::CodeBlock) => {
                if !current_spans.is_empty() {
                    flush_line(&mut lines, &mut current_spans, padding, Style::default());
                }
                in_code_block = false;
            }
            Event::Start(Tag::Emphasis) => {
                style_stack.push(Style::default().add_modifier(Modifier::ITALIC));
            }
            Event::End(TagEnd::Emphasis) => {
                style_stack.pop();
            }
            Event::Start(Tag::Strong) => {
                style_stack.push(Style::default().add_modifier(Modifier::BOLD));
            }
            Event::End(TagEnd::Strong) => {
                style_stack.pop();
            }
            Event::Start(Tag::Strikethrough) => {
                style_stack.push(Style::default().add_modifier(Modifier::CROSSED_OUT));
            }
            Event::End(TagEnd::Strikethrough) => {
                style_stack.pop();
            }
            Event::Start(Tag::List(_)) | Event::End(TagEnd::List(_)) => {}
            Event::Start(Tag::Item) => {
                if !current_spans.is_empty() {
                    flush_line(&mut lines, &mut current_spans, padding, Style::default());
                }
                in_list_item = true;
                current_spans.push(Span::raw("- "));
            }
            Event::End(TagEnd::Item) => {
                if !current_spans.is_empty() {
                    flush_line(&mut lines, &mut current_spans, padding, Style::default());
                }
                in_list_item = false;
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                style_stack.push(
                    Style::default()
                        .fg(Color::Blue)
                        .add_modifier(Modifier::UNDERLINED),
                );
                let _ = dest_url;
            }
            Event::End(TagEnd::Link) => {
                style_stack.pop();
            }
            Event::Start(Tag::BlockQuote(_)) => {
                style_stack.push(Style::default().fg(Color::DarkGray));
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                style_stack.pop();
            }
            Event::Text(t) => {
                if in_code_block {
                    for line in t.as_ref().lines() {
                        if !current_spans.is_empty() {
                            flush_line(&mut lines, &mut current_spans, padding, Style::default());
                        }
                        current_spans.push(Span::styled(line.to_string(), code_style));
                    }
                } else {
                    let style = current_style(&style_stack);
                    current_spans.push(Span::styled(t.into_string(), style));
                }
            }
            Event::Code(code) => {
                let style = Style::default()
                    .fg(Color::Yellow)
                    .bg(Color::Rgb(50, 50, 50));
                current_spans.push(Span::styled(format!(" {} ", code.as_ref()), style));
            }
            Event::SoftBreak => {
                current_spans.push(Span::raw(" "));
            }
            Event::HardBreak => {
                let style = if in_heading {
                    heading_style
                } else {
                    Style::default()
                };
                flush_line(&mut lines, &mut current_spans, padding, style);
            }
            Event::Rule => {
                flush_line(&mut lines, &mut current_spans, padding, Style::default());
                current_spans.push(Span::styled("───", Style::default().fg(Color::DarkGray)));
                flush_line(&mut lines, &mut current_spans, padding, Style::default());
            }
            Event::Start(Tag::Table(aligns)) => {
                if !current_spans.is_empty() {
                    flush_line(&mut lines, &mut current_spans, padding, Style::default());
                }
                table_aligns = aligns;
                table_rows.clear();
            }
            Event::Start(Tag::TableHead) | Event::Start(Tag::TableRow) => {
                table_row.clear();
            }
            Event::Start(Tag::TableCell) => {}
            Event::End(TagEnd::TableCell) => {
                table_row.push(std::mem::take(&mut current_spans));
            }
            Event::End(TagEnd::TableHead) | Event::End(TagEnd::TableRow) => {
                table_rows.push(std::mem::take(&mut table_row));
            }
            Event::End(TagEnd::Table) => {
                lines.extend(table_to_lines(
                    &table_aligns,
                    std::mem::take(&mut table_rows),
                    padding,
                    max_width.saturating_sub(padding.len()),
                ));
            }
            Event::TaskListMarker(checked) => {
                let marker = if checked { "[x] " } else { "[ ] " };
                if let Some(last) = current_spans.last_mut() {
                    if last.content.as_ref() == "- " {
                        *last = Span::raw(format!("- {marker}"));
                    }
                }
            }
            _ => {}
        }
    }
    if !current_spans.is_empty() {
        flush_line(&mut lines, &mut current_spans, padding, Style::default());
    }
    lines
}

/// Format a GFM table as fixed-width text lines.
///
/// The first row is the header (pulldown-cmark always emits `TableHead` first)
/// and is rendered bold, followed by a `───┼───` separator. Cells keep their
/// inline styling (code, emphasis, ...) and are padded to the column width
/// according to the column alignment.
///
/// If the natural width exceeds `max_width`, the widest columns are narrowed
/// (down to `MIN_COL_WIDTH`) until the table fits, and overflowing cells are
/// truncated with `…`. A table that still does not fit with every column at
/// `MIN_COL_WIDTH` is emitted as-is; the enclosing `Paragraph`
/// (`Wrap { trim: false }`) then wraps those lines like any other long line.
fn table_to_lines(
    aligns: &[pulldown_cmark::Alignment],
    rows: Vec<Vec<Vec<Span<'static>>>>,
    padding: &str,
    max_width: usize,
) -> Vec<Line<'static>> {
    use pulldown_cmark::Alignment;

    let ncols = rows
        .iter()
        .map(Vec::len)
        .max()
        .unwrap_or(0)
        .max(aligns.len());
    if ncols == 0 || rows.is_empty() {
        return Vec::new();
    }

    let cell_width = |cell: &[Span<'static>]| cell.iter().map(Span::width).sum::<usize>();
    let mut widths = vec![1usize; ncols];
    for row in &rows {
        for (c, cell) in row.iter().enumerate() {
            widths[c] = widths[c].max(cell_width(cell));
        }
    }
    fit_widths(&mut widths, max_width);

    let border_style = Style::default().fg(Color::DarkGray);
    let mut lines = Vec::with_capacity(rows.len() + 1);

    for (r, mut row) in rows.into_iter().enumerate() {
        row.resize_with(ncols, Vec::new);
        let is_header = r == 0;
        let mut spans: Vec<Span<'static>> = vec![Span::raw(padding.to_string())];
        for (c, cell) in row.into_iter().enumerate() {
            if c > 0 {
                spans.push(Span::styled(" │ ", border_style));
            }
            let mut cell = truncate_spans(cell, widths[c]);
            let pad = widths[c].saturating_sub(cell_width(&cell));
            let (left, right) = match aligns.get(c).copied().unwrap_or(Alignment::None) {
                Alignment::Right => (pad, 0),
                Alignment::Center => (pad / 2, pad - pad / 2),
                Alignment::None | Alignment::Left => (0, pad),
            };
            if is_header {
                for span in &mut cell {
                    span.style = span.style.add_modifier(Modifier::BOLD);
                }
            }
            if left > 0 {
                spans.push(Span::raw(" ".repeat(left)));
            }
            spans.extend(cell);
            if right > 0 {
                spans.push(Span::raw(" ".repeat(right)));
            }
        }
        lines.push(Line::from(spans));

        if is_header {
            let rule = widths
                .iter()
                .map(|w| "─".repeat(*w))
                .collect::<Vec<_>>()
                .join("─┼─");
            lines.push(Line::from(vec![
                Span::raw(padding.to_string()),
                Span::styled(rule, border_style),
            ]));
        }
    }
    lines
}

const MIN_COL_WIDTH: usize = 3;
const COL_SEP_WIDTH: usize = 3; // " │ "

/// Narrow the widest columns until the table fits in `max_width`.
///
/// Water-filling: the widest columns are lowered together (evenly) to the
/// next widest level, and so on, so that no single column absorbs all of the
/// excess. Columns never go below `MIN_COL_WIDTH`.
fn fit_widths(widths: &mut [usize], max_width: usize) {
    let total = |w: &[usize]| w.iter().sum::<usize>() + COL_SEP_WIDTH * w.len().saturating_sub(1);
    loop {
        let excess = total(widths).saturating_sub(max_width);
        if excess == 0 {
            break;
        }
        let Some(w) = widths.iter().copied().filter(|w| *w > MIN_COL_WIDTH).max() else {
            break;
        };
        let next = widths
            .iter()
            .copied()
            .filter(|x| *x < w)
            .max()
            .unwrap_or(MIN_COL_WIDTH)
            .max(MIN_COL_WIDTH);
        let tied = widths.iter().filter(|x| **x == w).count();
        let reduce = excess.div_ceil(tied).min(w - next);
        for x in widths.iter_mut().filter(|x| **x == w) {
            *x = w - reduce;
        }
    }
}

/// Cut a styled cell to `width` columns, ending with `…` when truncated.
fn truncate_spans(cell: Vec<Span<'static>>, width: usize) -> Vec<Span<'static>> {
    use unicode_width::UnicodeWidthChar;

    let full: usize = cell.iter().map(Span::width).sum();
    if full <= width {
        return cell;
    }
    let budget = width.saturating_sub(1); // leave room for "…"
    let mut out = Vec::with_capacity(cell.len() + 1);
    let mut used = 0;
    for span in cell {
        if used >= budget {
            break;
        }
        let mut s = String::new();
        for ch in span.content.chars() {
            let cw = ch.width().unwrap_or(0);
            if used + cw > budget {
                break;
            }
            used += cw;
            s.push(ch);
        }
        if !s.is_empty() {
            out.push(Span::styled(s, span.style));
        }
    }
    out.push(Span::raw("…"));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIDE: usize = 200;

    fn render(md: &str) -> Vec<String> {
        markdown_to_lines(md, "", WIDE)
            .into_iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect()
    }

    #[test]
    fn table_renders_aligned_columns_with_header_rule() {
        let md = "| Input | Error |\n|---|---|\n| `a` | missing |\n| longer cell | x |\n";
        let lines = render(md);
        assert_eq!(
            lines,
            vec![
                "Input       │ Error  ",
                "────────────┼────────",
                " a          │ missing",
                "longer cell │ x      ",
            ]
        );
    }

    #[test]
    fn table_header_is_bold_and_inline_code_keeps_style() {
        let md = "| H |\n|---|\n| `code` |\n";
        let lines = markdown_to_lines(md, "", WIDE);
        let header = &lines[0];
        assert!(header
            .spans
            .iter()
            .any(|s| s.content.as_ref() == "H" && s.style.add_modifier.contains(Modifier::BOLD)));
        let body = &lines[2];
        assert!(body
            .spans
            .iter()
            .any(|s| s.content.as_ref() == " code " && s.style.fg == Some(Color::Yellow)));
    }

    #[test]
    fn table_honours_column_alignment() {
        let md = "| L | C | R |\n|:--|:-:|--:|\n| a | b | c |\n";
        let lines = render(md);
        assert_eq!(lines[2], "a │ b │ c");
        let md = "| L | C | R |\n|:--|:-:|--:|\n| aaa | bbb | ccc |\n";
        let lines = render(md);
        assert_eq!(lines[2], "aaa │ bbb │ ccc");
        let md = "| Left | Center | Right |\n|:--|:-:|--:|\n| a | b | c |\n";
        let lines = render(md);
        assert_eq!(lines[2], "a    │   b    │     c");
    }

    #[test]
    fn table_between_paragraphs_keeps_surrounding_text() {
        let md = "Before\n\n| A | B |\n|---|---|\n| 1 | 2 |\n\nAfter\n";
        let lines = render(md);
        assert_eq!(lines.first().map(String::as_str), Some("Before"));
        assert_eq!(lines[1], "A │ B");
        assert_eq!(lines[2], "──┼──");
        assert_eq!(lines[3], "1 │ 2");
        assert_eq!(lines.last().map(String::as_str), Some("After"));
    }

    #[test]
    fn padding_is_prepended_to_table_lines() {
        let lines = render_with_padding("| A |\n|---|\n| 1 |\n", "  ");
        assert_eq!(lines, vec!["  A", "  ─", "  1"]);
    }

    fn render_width(md: &str, padding: &str, width: usize) -> Vec<String> {
        markdown_to_lines(md, padding, width)
            .into_iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect()
    }

    #[test]
    fn wide_table_is_narrowed_to_fit_and_cells_truncated() {
        let md = "| Key | Description |\n|---|---|\n| a | this is a rather long description |\n";
        // natural width: 3 + 3 + 34 = 40; fit into 20
        let lines = render_width(md, "", 20);
        assert_eq!(lines[0], "Key │ Description   ");
        assert_eq!(lines[1], "────┼───────────────");
        assert_eq!(lines[2], "a   │ this is a rat…");
        assert!(lines.iter().all(|l| l.chars().count() <= 20));
    }

    #[test]
    fn narrowing_accounts_for_padding_and_wide_chars() {
        let md = "| 名前 | 説明 |\n|---|---|\n| 太郎 | とても長い説明文です |\n";
        let lines = render_width(md, "  ", 18);
        // 2 (padding) + 4 + 3 + 9 = 18
        assert_eq!(lines[2], "  太郎 │ とても長…");
    }

    #[test]
    fn table_with_too_many_columns_keeps_min_width() {
        let md = "| a | b | c | d |\n|---|---|---|---|\n| 1 | 2 | 3 | 4 |\n";
        // Cannot fit into 5 columns; natural widths are kept (never widened
        // to MIN_COL_WIDTH, never narrowed below it) and the paragraph wraps.
        let lines = render_width(md, "", 5);
        assert_eq!(lines[2], "1 │ 2 │ 3 │ 4");
    }

    #[test]
    fn fit_widths_lowers_widest_columns_evenly() {
        let mut w = vec![34, 85, 28];
        fit_widths(&mut w, 62);
        assert_eq!(w, vec![18, 18, 18]);
        let mut w = vec![10, 10];
        fit_widths(&mut w, 22);
        assert_eq!(w, vec![9, 9]); // odd excess rounds up per column
        let mut w = vec![3, 3];
        fit_widths(&mut w, 4);
        assert_eq!(w, vec![3, 3]); // never below MIN_COL_WIDTH
    }

    fn render_with_padding(md: &str, padding: &str) -> Vec<String> {
        markdown_to_lines(md, padding, WIDE)
            .into_iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect()
    }
}
