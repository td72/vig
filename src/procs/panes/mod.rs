pub mod detail;
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

    #[test]
    fn truncate_marks_the_cut() {
        assert_eq!(truncate_chars("abc", 5), "abc");
        assert_eq!(truncate_chars("abcdef", 4), "abc…");
        assert_eq!(truncate_chars("abcdef", 0), "");
        assert_eq!(truncate_chars("日本語テキスト", 4), "日本語…");
    }
}
