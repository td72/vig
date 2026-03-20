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
