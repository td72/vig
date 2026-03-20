use crate::core::pane::PaneShared;
use crate::core::theme;
use crate::git::domain::graph::{GraphCell, GraphRow, NUM_GRAPH_COLORS};
use crate::git::panes::GitLogPane;
use crate::git::state::{PANE_BRANCH_LIST, PANE_GIT_LOG, PANE_REFLOG};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

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
pub fn render(f: &mut Frame, pane: &mut GitLogPane, shared: &PaneShared, area: Rect) {
    let is_focused = shared.focused_pane == PANE_GIT_LOG
        || shared.focused_pane == PANE_BRANCH_LIST
        || shared.focused_pane == PANE_REFLOG;

    let block = theme::pane_block("Git Log", is_focused);

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Split inner area: left (commit list) | right (detail)
    let cols =
        Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)]).split(inner);

    render_list(f, pane, shared, cols[0]);
    render_detail(f, pane, cols[1]);
}

/// Render the commit list (left pane inside Git Log).
fn render_list(f: &mut Frame, pane: &mut GitLogPane, shared: &PaneShared, area: Rect) {
    pane.view_height = area.height;

    if pane.commits.is_empty() {
        let items: Vec<ListItem> = vec![ListItem::new(Line::from(Span::styled(
            "  No commits",
            Style::default().fg(Color::DarkGray),
        )))];
        let list = List::new(items);
        f.render_widget(list, area);
        return;
    }

    // Compute max graph width for alignment
    let max_graph_width = pane.graph.iter().map(|r| r.cells.len()).max().unwrap_or(0);

    // Build set of matched commit entry indices
    let (match_set, current_match_idx) = theme::list_search_highlights(shared, PANE_GIT_LOG);

    // Highlight pipes originating from the selected commit (lazygit-style)
    let highlight_from = if shared.focused_pane == PANE_GIT_LOG
        || shared.focused_pane == PANE_BRANCH_LIST
        || shared.focused_pane == PANE_REFLOG
    {
        Some(pane.selected_idx)
    } else {
        None
    };

    let items: Vec<ListItem> = pane
        .commits
        .iter()
        .enumerate()
        .map(|(idx, commit)| {
            let hl = theme::search_highlight_for(&match_set, current_match_idx, idx);

            let hash_style = hl.style_with_fg(Color::Yellow);
            let date_style = hl.style_with_fg(Color::DarkGray);
            let author_style = hl.style_with_fg(Color::Cyan);
            let msg_style = hl.apply(Style::default());

            let mut spans = Vec::new();

            // Graph prefix
            if let Some(graph_row) = pane.graph.get(idx) {
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

    let selected = pane.selected_idx;
    let selected_is_match = match_set.contains(&selected);

    let highlight_style = theme::list_highlight_style(selected_is_match);

    let list = List::new(items).highlight_style(highlight_style);

    let mut state = ListState::default();
    state.select(Some(selected));
    f.render_stateful_widget(list, area, &mut state);
}

/// Render commit detail content (left border as separator inside the parent Git Log block).
fn render_detail(f: &mut Frame, pane: &mut GitLogPane, area: Rect) {
    let separator = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = separator.inner(area);
    f.render_widget(separator, area);

    pane.detail_view_height = inner.height;

    let commit = match pane.commits.get(pane.selected_idx) {
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
    if pane.detail_changed_files.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (no changes)",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for change in &pane.detail_changed_files {
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
    let view_height = pane.detail_view_height;
    let max_scroll = total_lines.saturating_sub(view_height);
    if pane.detail.scroll_y > max_scroll {
        pane.detail.scroll_y = max_scroll;
    }

    let para = Paragraph::new(lines).scroll((pane.detail.scroll_y, 0));
    f.render_widget(para, inner);
}
