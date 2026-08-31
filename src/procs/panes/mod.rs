pub mod detail;
pub mod graphs;
pub mod ports;
pub mod processes;

use crate::core::pane::PaneShared;
use crate::core::theme;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};
use ratatui::{layout::Rect, Frame};
use std::collections::HashSet;

/// Text shown for values the current user is not allowed to read.
pub const NO_ACCESS: &str = "(no access)";

/// Dim style for secondary text (guides, headers, unavailable values).
pub fn dim() -> Style {
    Style::default().fg(Color::DarkGray)
}

/// Style for a load percentage: green below 50 %, yellow from 50 %, red
/// from 80 % — the btop-like gradient every Procs graph shares.
pub fn load_style(pct: f32) -> Style {
    if pct >= 80.0 {
        Style::default().fg(Color::Red)
    } else if pct >= 50.0 {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::Green)
    }
}

/// btop-style filled area chart: each sample is one vertical bar built from
/// eighth-block characters stacked across `rows` lines, colored by its load
/// ([`load_style`] of the value as a percentage of `max`). The newest sample
/// sits at the right edge; a series shorter than `width` is right-aligned,
/// a longer one keeps only the newest `width` samples.
pub fn area_chart(samples: &[f32], rows: usize, width: usize, max: f32) -> Vec<Line<'static>> {
    area_chart_with(samples, rows, width, max, load_style)
}

/// [`area_chart`] with every column in one fixed style — for series whose
/// scale is relative (e.g. RSS against its own recent peak), where load
/// colors would mislead.
pub fn area_chart_plain(
    samples: &[f32],
    rows: usize,
    width: usize,
    max: f32,
    style: Style,
) -> Vec<Line<'static>> {
    area_chart_with(samples, rows, width, max, move |_| style)
}

fn area_chart_with(
    samples: &[f32],
    rows: usize,
    width: usize,
    max: f32,
    style_of: impl Fn(f32) -> Style,
) -> Vec<Line<'static>> {
    const LEVELS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    if rows == 0 || width == 0 {
        return vec![];
    }
    let max = if max > 0.0 { max } else { 1.0 };
    let take = samples.len().min(width);
    let slice = &samples[samples.len() - take..];
    // Column height in eighths of a cell (0 ..= rows * 8) plus its color;
    // anything above zero shows at least the lowest block.
    let cols: Vec<(usize, Style)> = slice
        .iter()
        .map(|&v| {
            let frac = (f64::from(v) / f64::from(max)).clamp(0.0, 1.0);
            let mut eighths = (frac * (rows * 8) as f64).round() as usize;
            if v > 0.0 {
                eighths = eighths.max(1);
            }
            (eighths, style_of(frac as f32 * 100.0))
        })
        .collect();
    (0..rows)
        .map(|row| {
            // Eighths consumed by the rows below this one.
            let below = (rows - 1 - row) * 8;
            let mut spans = Vec::new();
            if take < width {
                spans.push(Span::raw(" ".repeat(width - take)));
            }
            // Adjacent cells sharing a style merge into one span.
            let mut run = String::new();
            let mut run_style = Style::default();
            for &(eighths, style) in &cols {
                let fill = eighths.saturating_sub(below).min(8);
                let (ch, st) = if fill == 0 {
                    (' ', Style::default())
                } else {
                    (LEVELS[fill - 1], style)
                };
                if st != run_style && !run.is_empty() {
                    spans.push(Span::styled(std::mem::take(&mut run), run_style));
                }
                run_style = st;
                run.push(ch);
            }
            if !run.is_empty() {
                spans.push(Span::styled(run, run_style));
            }
            Line::from(spans)
        })
        .collect()
}

/// Keep at most `max` characters of `s`, marking the cut with `…`.
pub fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let mut out: String = s.chars().take(max - 1).collect();
    out.push('…');
    out
}

/// A bordered list pane with a fixed column header on its first row.
/// Mirrors [`theme::render_list_pane`] but keeps the header out of the
/// scrolling / selection region so column titles stay put.
///
/// `selected` is always drawn (so the list keeps its scroll position while
/// another pane has focus); `emphasized` gives it the full selection
/// background, otherwise it is only bold.
#[allow(clippy::too_many_arguments)]
pub fn render_table_pane(
    f: &mut Frame,
    area: Rect,
    shared: &PaneShared,
    pane_id: usize,
    title: &str,
    header: &str,
    selected: Option<usize>,
    emphasized: bool,
    empty: Option<&str>,
    build_items: impl FnOnce(&HashSet<usize>, Option<usize>) -> Vec<ListItem<'static>>,
) {
    let block = theme::pane_block(title, shared.focused_pane == pane_id);
    if let Some(message) = empty {
        theme::render_empty_list(f, area, block, message);
        return;
    }
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }
    let header_area = Rect { height: 1, ..inner };
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(header.to_string(), dim()))),
        header_area,
    );
    let list_area = Rect {
        y: inner.y + 1,
        height: inner.height - 1,
        ..inner
    };
    if list_area.height == 0 {
        return;
    }
    let (match_set, current_match_idx) = theme::list_search_highlights(shared, pane_id);
    let items = build_items(&match_set, current_match_idx);
    let highlight = if emphasized {
        theme::list_highlight_style(selected.is_some_and(|idx| match_set.contains(&idx)))
    } else {
        Style::default().add_modifier(Modifier::BOLD)
    };
    let list = List::new(items).highlight_style(highlight);
    let mut state = ListState::default();
    state.select(selected);
    f.render_stateful_widget(list, list_area, &mut state);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row_text(line: &Line<'_>) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn area_chart_fills_columns_bottom_up() {
        // Two rows → 16 eighths per column.
        let lines = area_chart(&[0.0, 25.0, 50.0, 75.0, 100.0], 2, 5, 100.0);
        assert_eq!(lines.len(), 2);
        assert_eq!(row_text(&lines[0]), "   ▄█"); // top row
        assert_eq!(row_text(&lines[1]), " ▄███"); // bottom row
    }

    #[test]
    fn area_chart_right_aligns_and_keeps_newest() {
        let lines = area_chart(&[100.0], 1, 4, 100.0);
        assert_eq!(row_text(&lines[0]), "   █");
        // A longer series keeps only the newest `width` samples.
        let lines = area_chart(&[100.0, 0.0, 25.0], 1, 2, 100.0);
        assert_eq!(row_text(&lines[0]), " ▂");
        let lines = area_chart(&[], 1, 3, 100.0);
        assert_eq!(row_text(&lines[0]), "   ");
    }

    #[test]
    fn area_chart_rounds_eighths_and_shows_tiny_loads() {
        let lines = area_chart(&[6.25, 50.0, 100.0, 1.0, 0.0], 1, 5, 100.0);
        // Half an eighth rounds up; 1 % rounds to 0 but still shows the
        // lowest block; a true zero stays blank.
        assert_eq!(row_text(&lines[0]), "▁▄█▁ ");
    }

    #[test]
    fn area_chart_colors_columns_by_load() {
        let lines = area_chart(&[10.0, 60.0, 90.0], 1, 3, 100.0);
        let spans = &lines[0].spans;
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].style.fg, Some(Color::Green));
        assert_eq!(spans[1].style.fg, Some(Color::Yellow));
        assert_eq!(spans[2].style.fg, Some(Color::Red));
        // The thresholds themselves.
        assert_eq!(load_style(49.9).fg, Some(Color::Green));
        assert_eq!(load_style(50.0).fg, Some(Color::Yellow));
        assert_eq!(load_style(79.9).fg, Some(Color::Yellow));
        assert_eq!(load_style(80.0).fg, Some(Color::Red));
    }

    #[test]
    fn area_chart_scales_to_max_and_handles_degenerate_sizes() {
        // max = 200: a value of 100 is half height and colored as 50 %.
        let lines = area_chart(&[100.0, 200.0, 300.0], 1, 3, 200.0);
        assert_eq!(row_text(&lines[0]), "▄██"); // 300 clamps to full
        assert_eq!(lines[0].spans[0].style.fg, Some(Color::Yellow));
        assert_eq!(lines[0].spans[1].style.fg, Some(Color::Red));
        assert!(area_chart(&[1.0], 0, 3, 1.0).is_empty());
        assert!(area_chart(&[1.0], 1, 0, 1.0).is_empty());
        // A zero max never divides by zero.
        assert_eq!(row_text(&area_chart(&[0.5], 1, 1, 0.0)[0]), "▄");
    }

    #[test]
    fn area_chart_plain_uses_one_style() {
        let style = Style::default().fg(Color::Cyan);
        let lines = area_chart_plain(&[10.0, 90.0], 1, 2, 100.0, style);
        assert_eq!(lines[0].spans.len(), 1); // one merged run
        assert_eq!(lines[0].spans[0].style.fg, Some(Color::Cyan));
        assert_eq!(row_text(&lines[0]), "▁▇");
    }

    #[test]
    fn truncate_marks_the_cut() {
        assert_eq!(truncate_chars("abc", 5), "abc");
        assert_eq!(truncate_chars("abcdef", 4), "abc…");
        assert_eq!(truncate_chars("abcdef", 0), "");
        assert_eq!(truncate_chars("日本語テキスト", 4), "日本語…");
    }
}
