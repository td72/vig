use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
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
