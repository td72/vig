use crate::core::app::{SearchMatch, SearchOrigin};
use crate::git::graph::{GraphCell, GraphRow, NUM_GRAPH_COLORS};
use crate::git::state::{FocusedPane, GitState};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};
use std::collections::HashSet;

const GRAPH_COLORS: [Color; NUM_GRAPH_COLORS] = [
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
                GraphCell::Commit => "●",
                GraphCell::Vertical => "│",
                GraphCell::Horizontal => "─",
                GraphCell::DownRight => "╮",
                GraphCell::DownLeft => "╭",
                GraphCell::UpRight => "╯",
                GraphCell::UpLeft => "╰",
                GraphCell::Cross => "┼",
                GraphCell::Empty => " ",
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
            spans.push(Span::styled(ch, style));
        } else {
            spans.push(Span::raw(" "));
        }
    }
    spans.push(Span::raw(" "));
    spans
}

/// Render the Git Log component: outer border with left (commit list) and right (detail).
pub fn render(f: &mut Frame, state: &mut GitState, area: Rect) {
    let is_focused = state.focused_pane == FocusedPane::GitLog
        || state.focused_pane == FocusedPane::BranchList
        || state.focused_pane == FocusedPane::Reflog;
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
    let cols =
        Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)]).split(inner);

    render_list(f, state, cols[0]);
    render_detail(f, state, cols[1]);
}

/// Render the commit list (left pane inside Git Log).
fn render_list(f: &mut Frame, state: &mut GitState, area: Rect) {
    state.git_log.view_height = area.height;

    if state.git_log.commits.is_empty() {
        let items: Vec<ListItem> = vec![ListItem::new(Line::from(Span::styled(
            "  No commits",
            Style::default().fg(Color::DarkGray),
        )))];
        let list = List::new(items);
        f.render_widget(list, area);
        return;
    }

    // Compute max graph width for alignment
    let max_graph_width = state
        .git_log
        .graph
        .iter()
        .map(|r| r.cells.len())
        .max()
        .unwrap_or(0);

    // Build set of matched commit entry indices
    let (match_set, current_match_idx) = if state.search.origin == SearchOrigin::CommitLog {
        let set: HashSet<usize> = state
            .search
            .matches
            .iter()
            .filter_map(|m| match m {
                SearchMatch::CommitEntry(idx) => Some(*idx),
                _ => None,
            })
            .collect();
        let current =
            state
                .search
                .current_match_idx
                .and_then(|ci| match state.search.matches.get(ci) {
                    Some(SearchMatch::CommitEntry(idx)) => Some(*idx),
                    _ => None,
                });
        (set, current)
    } else {
        (HashSet::new(), None)
    };

    // Highlight pipes originating from the selected commit (lazygit-style)
    let highlight_from = if state.focused_pane == FocusedPane::GitLog
        || state.focused_pane == FocusedPane::BranchList
        || state.focused_pane == FocusedPane::Reflog
    {
        Some(state.git_log.selected_idx)
    } else {
        None
    };

    let items: Vec<ListItem> = state
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
                if let Some(bg) = bg {
                    s = s.bg(bg);
                }
                s
            };
            let date_style = {
                let mut s = Style::default().fg(fg_override.unwrap_or(Color::DarkGray));
                if let Some(bg) = bg {
                    s = s.bg(bg);
                }
                s
            };
            let author_style = {
                let mut s = Style::default().fg(fg_override.unwrap_or(Color::Cyan));
                if let Some(bg) = bg {
                    s = s.bg(bg);
                }
                s
            };
            let msg_style = {
                let mut s = Style::default();
                if let Some(fg) = fg_override {
                    s = s.fg(fg);
                }
                if let Some(bg) = bg {
                    s = s.bg(bg);
                }
                s
            };

            let mut spans = Vec::new();

            // Graph prefix
            if let Some(graph_row) = state.git_log.graph.get(idx) {
                spans.extend(graph_spans(graph_row, max_graph_width, highlight_from));
            } else {
                for _ in 0..max_graph_width {
                    spans.push(Span::raw(" "));
                }
                spans.push(Span::raw(" "));
            }

            spans.push(Span::styled(format!("{} ", commit.short_hash), hash_style));
            spans.push(Span::styled(format!("{} ", commit.date), date_style));
            spans.push(Span::styled(
                format!("{:<12} ", commit.author),
                author_style,
            ));
            spans.push(Span::styled(commit.message.clone(), msg_style));

            ListItem::new(Line::from(spans))
        })
        .collect();

    let selected = state.git_log.selected_idx;
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

/// Render commit detail content (left border as separator inside the parent Git Log block).
fn render_detail(f: &mut Frame, state: &mut GitState, area: Rect) {
    let separator = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = separator.inner(area);
    f.render_widget(separator, area);

    state.git_log.detail_view_height = inner.height;

    let commit = match state.git_log.commits.get(state.git_log.selected_idx) {
        Some(c) => c,
        None => {
            let para = Paragraph::new(Line::from(Span::styled(
                "  No commit selected",
                Style::default().fg(Color::DarkGray),
            )));
            f.render_widget(para, inner);
            return;
        }
    };

    let mut lines: Vec<Line> = Vec::new();

    // Commit hash
    lines.push(Line::from(vec![
        Span::styled("Commit: ", Style::default().fg(Color::DarkGray)),
        Span::styled(commit.full_hash.clone(), Style::default().fg(Color::Yellow)),
    ]));

    // Parent hashes
    if !commit.parent_ids.is_empty() {
        let parent_spans: Vec<Span> = commit
            .parent_ids
            .iter()
            .enumerate()
            .flat_map(|(i, pid)| {
                let short = &pid[..7.min(pid.len())];
                let mut v = Vec::new();
                if i > 0 {
                    v.push(Span::raw(" "));
                }
                v.push(Span::styled(
                    short.to_string(),
                    Style::default().fg(Color::Yellow),
                ));
                v
            })
            .collect();
        let mut spans = vec![Span::styled(
            "Parent: ",
            Style::default().fg(Color::DarkGray),
        )];
        spans.extend(parent_spans);
        lines.push(Line::from(spans));
    }

    // Author
    lines.push(Line::from(vec![
        Span::styled("Author: ", Style::default().fg(Color::DarkGray)),
        Span::styled(commit.author.clone(), Style::default().fg(Color::Cyan)),
    ]));

    // Date
    lines.push(Line::from(vec![
        Span::styled("Date:   ", Style::default().fg(Color::DarkGray)),
        Span::raw(commit.date.clone()),
    ]));

    // Blank line + full message
    lines.push(Line::from(""));
    for msg_line in commit.full_message.lines() {
        lines.push(Line::from(Span::styled(
            format!("  {msg_line}"),
            Style::default().add_modifier(Modifier::BOLD),
        )));
    }

    // Changed files separator
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "── Changed Files ──",
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    )));

    // Changed files list
    if state.git_log.detail_changed_files.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (no changes)",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for change in &state.git_log.detail_changed_files {
            let (status_char, status_color) = match change.status {
                'A' => ("A", Color::Green),
                'D' => ("D", Color::Red),
                'R' => ("R", Color::Blue),
                'C' => ("C", Color::Magenta),
                _ => ("M", Color::Yellow),
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {status_char} "),
                    Style::default().fg(status_color),
                ),
                Span::raw(change.path.clone()),
            ]));
        }
    }

    // Clamp scroll
    let total_lines = lines.len() as u16;
    let view_height = state.git_log.detail_view_height;
    let max_scroll = total_lines.saturating_sub(view_height);
    if state.git_log.detail_scroll > max_scroll {
        state.git_log.detail_scroll = max_scroll;
    }

    let para = Paragraph::new(lines).scroll((state.git_log.detail_scroll, 0));
    f.render_widget(para, inner);
}
