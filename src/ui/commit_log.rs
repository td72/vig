use crate::app::{App, FocusedPane, SearchMatch, SearchOrigin};
use crate::git::graph::{GraphCell, GraphRow};
use crate::ui::commit_detail;
use std::collections::HashSet;
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};

const GRAPH_COLORS: [Color; 6] = [
    Color::Red,
    Color::Green,
    Color::Yellow,
    Color::Blue,
    Color::Magenta,
    Color::Cyan,
];

fn graph_spans(
    row: &GraphRow,
    max_width: usize,
    highlight_from: Option<usize>,
) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for i in 0..max_width {
        if i < row.cells.len() {
            let ch = match row.cells[i] {
                GraphCell::Commit    => "●",
                GraphCell::Vertical  => "│",
                GraphCell::Horizontal=> "─",
                GraphCell::DownRight => "╮",
                GraphCell::DownLeft  => "╭",
                GraphCell::UpRight   => "╯",
                GraphCell::UpLeft    => "╰",
                GraphCell::Cross     => "┼",
                GraphCell::Empty     => " ",
            };
            let is_highlighted = highlight_from.is_some()
                && row.from_indices.get(i).copied().flatten() == highlight_from;
            let style = if is_highlighted {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                let color = GRAPH_COLORS[row.colors[i] % GRAPH_COLORS.len()];
                Style::default().fg(color)
            };
            spans.push(Span::styled(ch.to_string(), style));
        } else {
            spans.push(Span::raw(" "));
        }
    }
    spans.push(Span::raw(" "));
    spans
}

/// Render the Git Log component: outer border with left (commit list) and right (detail).
pub fn render(f: &mut Frame, app: &mut App, area: Rect) {
    let is_focused = app.focused_pane == FocusedPane::GitLog
        || app.focused_pane == FocusedPane::BranchList
        || app.focused_pane == FocusedPane::Reflog;
    let border_color = if is_focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let block = Block::default()
        .title(" Git Log ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Split inner area: left (commit list) | right (detail)
    let cols = Layout::horizontal([
        Constraint::Percentage(60),
        Constraint::Percentage(40),
    ])
    .split(inner);

    render_list(f, app, cols[0]);
    commit_detail::render(f, app, cols[1]);
}

/// Render the commit list (left pane inside Git Log).
fn render_list(f: &mut Frame, app: &mut App, area: Rect) {
    app.git_log.view_height = area.height;

    if app.git_log.commits.is_empty() {
        let items: Vec<ListItem> = vec![ListItem::new(Line::from(Span::styled(
            "  No commits",
            Style::default().fg(Color::DarkGray),
        )))];
        let list = List::new(items);
        f.render_widget(list, area);
        return;
    }

    // Compute max graph width for alignment
    let max_graph_width = app
        .git_log
        .graph
        .iter()
        .map(|r| r.cells.len())
        .max()
        .unwrap_or(0);

    // Build set of matched commit entry indices
    let (match_set, current_match_idx) = if app.search.origin == SearchOrigin::CommitLog {
        let set: HashSet<usize> = app
            .search
            .matches
            .iter()
            .filter_map(|m| match m {
                SearchMatch::CommitEntry(idx) => Some(*idx),
                _ => None,
            })
            .collect();
        let current = app.search.current_match_idx.and_then(|ci| {
            match app.search.matches.get(ci) {
                Some(SearchMatch::CommitEntry(idx)) => Some(*idx),
                _ => None,
            }
        });
        (set, current)
    } else {
        (HashSet::new(), None)
    };

    // Highlight pipes originating from the selected commit (lazygit-style)
    let highlight_from = if app.focused_pane == FocusedPane::GitLog
        || app.focused_pane == FocusedPane::BranchList
        || app.focused_pane == FocusedPane::Reflog
    {
        Some(app.git_log.selected_idx)
    } else {
        None
    };

    let items: Vec<ListItem> = app
        .git_log
        .commits
        .iter()
        .enumerate()
        .map(|(idx, commit)| {
            let is_current = current_match_idx == Some(idx);
            let is_match = match_set.contains(&idx);
            let bg = if is_current {
                Some(Color::Rgb(200, 120, 0))
            } else if is_match {
                Some(Color::Rgb(60, 60, 0))
            } else {
                None
            };
            let fg_override = if is_current { Some(Color::Black) } else { None };

            let hash_style = {
                let mut s = Style::default().fg(fg_override.unwrap_or(Color::Yellow));
                if let Some(bg) = bg { s = s.bg(bg); }
                s
            };
            let date_style = {
                let mut s = Style::default().fg(fg_override.unwrap_or(Color::DarkGray));
                if let Some(bg) = bg { s = s.bg(bg); }
                s
            };
            let author_style = {
                let mut s = Style::default().fg(fg_override.unwrap_or(Color::Cyan));
                if let Some(bg) = bg { s = s.bg(bg); }
                s
            };
            let msg_style = {
                let mut s = Style::default();
                if let Some(fg) = fg_override { s = s.fg(fg); }
                if let Some(bg) = bg { s = s.bg(bg); }
                s
            };

            let mut spans = Vec::new();

            // Graph prefix
            if let Some(graph_row) = app.git_log.graph.get(idx) {
                spans.extend(graph_spans(graph_row, max_graph_width, highlight_from));
            } else {
                for _ in 0..max_graph_width {
                    spans.push(Span::raw(" "));
                }
                spans.push(Span::raw(" "));
            }

            spans.push(Span::styled(format!("{} ", commit.short_hash), hash_style));
            spans.push(Span::styled(format!("{} ", commit.date), date_style));
            spans.push(Span::styled(format!("{:<12} ", commit.author), author_style));
            spans.push(Span::styled(commit.message.clone(), msg_style));

            ListItem::new(Line::from(spans))
        })
        .collect();

    let selected = app.git_log.selected_idx;
    let selected_is_match = match_set.contains(&selected);

    let highlight_style = if selected_is_match {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD)
    };

    let list = List::new(items).highlight_style(highlight_style);

    let mut state = ListState::default();
    state.select(Some(selected));
    f.render_stateful_widget(list, area, &mut state);
}
