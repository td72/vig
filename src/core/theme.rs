use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};
use std::collections::HashSet;

use crate::core::pane::PaneShared;
use crate::core::search::SearchMatch;

// === Color palette ===

pub const BORDER_FOCUSED: Color = Color::Cyan;
pub const BORDER_UNFOCUSED: Color = Color::DarkGray;

pub const SEARCH_CURRENT_FG: Color = Color::Black;
pub const SEARCH_CURRENT_BG: Color = Color::Rgb(200, 120, 0);
pub const SEARCH_MATCH_BG: Color = Color::Rgb(60, 60, 0);

pub const LIST_SELECTION_BG: Color = Color::DarkGray;
pub const EMPTY_TEXT_FG: Color = Color::DarkGray;
pub const MODAL_BG: Color = Color::Rgb(30, 30, 30);

// Diff view specific
pub const SELECTION_BG: Color = Color::Rgb(60, 60, 100);
pub const CURSOR_FG: Color = Color::Black;
pub const CURSOR_BG: Color = Color::White;

// === Search highlight helper ===

pub struct SearchHighlight {
    pub bg: Option<Color>,
    pub fg_override: Option<Color>,
}

impl SearchHighlight {
    pub fn none() -> Self {
        Self {
            bg: None,
            fg_override: None,
        }
    }

    pub fn is_active(&self) -> bool {
        self.bg.is_some()
    }

    /// Apply bg/fg override onto an existing style.
    pub fn apply(&self, mut style: Style) -> Style {
        if let Some(bg) = self.bg {
            style = style.bg(bg);
        }
        if let Some(fg) = self.fg_override {
            style = style.fg(fg);
        }
        style
    }

    /// Build a style with a default fg, overridden by search highlight if active.
    pub fn style_with_fg(&self, default_fg: Color) -> Style {
        let mut s = Style::default().fg(self.fg_override.unwrap_or(default_fg));
        if let Some(bg) = self.bg {
            s = s.bg(bg);
        }
        s
    }
}

/// Compute the search highlight for a given list entry index.
pub fn search_highlight_for(
    match_set: &HashSet<usize>,
    current_match_idx: Option<usize>,
    idx: usize,
) -> SearchHighlight {
    let is_current = current_match_idx == Some(idx);
    let is_match = match_set.contains(&idx);
    if is_current {
        SearchHighlight {
            bg: Some(SEARCH_CURRENT_BG),
            fg_override: Some(SEARCH_CURRENT_FG),
        }
    } else if is_match {
        SearchHighlight {
            bg: Some(SEARCH_MATCH_BG),
            fg_override: None,
        }
    } else {
        SearchHighlight::none()
    }
}

// === UI helpers ===

/// Create a bordered block with focus-dependent border color.
pub fn pane_block(title: &str, is_focused: bool) -> Block<'_> {
    Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if is_focused {
            BORDER_FOCUSED
        } else {
            BORDER_UNFOCUSED
        }))
}

/// Render an empty-state list with a single placeholder message.
pub fn render_empty_list(f: &mut Frame, area: Rect, block: Block, message: &str) {
    let items = vec![ListItem::new(Line::from(Span::styled(
        format!("  {message}"),
        Style::default().fg(EMPTY_TEXT_FG),
    )))];
    let list = List::new(items).block(block);
    f.render_widget(list, area);
}

/// Compute highlight style for the List widget's selected row.
/// If the selected row is a search match, use BOLD only (no bg override)
/// to preserve the match background. Otherwise use selection bg.
pub fn list_highlight_style(selected_is_match: bool) -> Style {
    if selected_is_match {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .bg(LIST_SELECTION_BG)
            .add_modifier(Modifier::BOLD)
    }
}

/// Render a list with search highlighting and selection.
/// Pass `selected_idx: Some(idx)` to highlight the selected row,
/// or `None` to render without selection (e.g. when the pane is not focused).
///
/// `scroll` is the pane's first visible row from the previous frame and
/// the new one is returned: the pane must keep it. Starting every frame
/// from a fresh `ListState` would recompute the offset from scratch and
/// pin any selection past the first page to the bottom row — so `k` from
/// the end would scroll at once while `j` from the top moved within the
/// page first. With the offset kept, both directions move within the page
/// and scroll one row at the edge.
pub fn render_search_list(
    f: &mut Frame,
    area: Rect,
    items: Vec<ListItem>,
    block: Block,
    selected_idx: Option<usize>,
    match_set: &HashSet<usize>,
    scroll: usize,
) -> usize {
    let highlight_style =
        list_highlight_style(selected_idx.is_some_and(|idx| match_set.contains(&idx)));
    let list = List::new(items)
        .block(block)
        .highlight_style(highlight_style);
    let mut list_state = ListState::default().with_offset(scroll);
    list_state.select(selected_idx);
    f.render_stateful_widget(list, area, &mut list_state);
    list_state.offset()
}

/// Render a standard search-enabled list pane: focused border, empty-state
/// fallback, search highlighting and selection. The caller supplies the
/// per-item rendering via `build_items`, which receives the match set and the
/// current match index so it can style matched/selected rows.
///
/// Pass `empty: Some(message)` to short-circuit into the empty-state view
/// (used both for "no items" and transient states like "Loading...").
/// `scroll` is the pane's kept scroll position; the new one is returned
/// (see [`render_search_list`]).
#[allow(clippy::too_many_arguments)]
pub fn render_list_pane(
    f: &mut Frame,
    area: Rect,
    shared: &PaneShared,
    pane_id: usize,
    title: &str,
    selected: Option<usize>,
    empty: Option<&str>,
    scroll: usize,
    build_items: impl FnOnce(&HashSet<usize>, Option<usize>) -> Vec<ListItem<'static>>,
) -> usize {
    let block = pane_block(title, shared.focused_pane == pane_id);
    if let Some(message) = empty {
        render_empty_list(f, area, block, message);
        return 0;
    }
    let (match_set, current_match_idx) = list_search_highlights(shared, pane_id);
    let items = build_items(&match_set, current_match_idx);
    render_search_list(f, area, items, block, selected, &match_set, scroll)
}

/// Extract list-entry search highlights for a given pane.
/// Returns (set of matched indices, current match index).
pub fn list_search_highlights(
    shared: &PaneShared,
    pane_id: usize,
) -> (HashSet<usize>, Option<usize>) {
    if shared.search.origin != pane_id {
        return (HashSet::new(), None);
    }
    let set: HashSet<usize> = shared
        .search
        .matches
        .iter()
        .filter_map(|m| match m {
            SearchMatch::ListEntry(idx) => Some(*idx),
            _ => None,
        })
        .collect();
    let current =
        shared
            .search
            .current_match_idx
            .and_then(|ci| match shared.search.matches.get(ci) {
                Some(SearchMatch::ListEntry(idx)) => Some(*idx),
                _ => None,
            });
    (set, current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    /// The kept offset makes the list scroll the same way in both
    /// directions: the selection moves within the page and the page
    /// scrolls one row at the edge. A fresh state per frame would pin a
    /// selection past the first page to the bottom row on the way up.
    #[test]
    fn kept_scroll_makes_list_scrolling_symmetric() {
        let mut term = Terminal::new(TestBackend::new(30, 10)).unwrap();
        let rows = 8; // 10 minus the block's two border rows
        let mut scroll = 0;
        let mut frame = |selected: usize, scroll: usize| -> usize {
            let mut out = 0;
            term.draw(|f| {
                let items: Vec<ListItem> =
                    (0..30).map(|i| ListItem::new(format!("row {i}"))).collect();
                let block = pane_block("List", true);
                out = render_search_list(
                    f,
                    f.area(),
                    items,
                    block,
                    Some(selected),
                    &HashSet::new(),
                    scroll,
                );
            })
            .unwrap();
            out
        };
        // Down from the top: within the page until the edge, then one row
        // at a time with the selection on the bottom row.
        for sel in 0..rows {
            scroll = frame(sel, scroll);
            assert_eq!(scroll, 0, "sel {sel}");
        }
        scroll = frame(rows, scroll);
        assert_eq!(scroll, 1);
        scroll = frame(20, scroll);
        assert_eq!(scroll, 20 - rows + 1);
        // Up from there: the selection moves within the page first …
        scroll = frame(19, scroll);
        assert_eq!(scroll, 13, "moving up within the page keeps the offset");
        scroll = frame(13, scroll);
        assert_eq!(scroll, 13);
        // … and scrolls one row once it reaches the top edge.
        scroll = frame(12, scroll);
        assert_eq!(scroll, 12);
        scroll = frame(11, scroll);
        assert_eq!(scroll, 11);
    }
}
