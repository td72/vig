use crate::app::{App, FocusedPane, SearchMatch, SearchOrigin, TreeEntry};
use crate::git::diff::FileStatus;
use std::collections::HashSet;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let border_color = if app.focused_pane == FocusedPane::FileTree {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let block = Block::default()
        .title(" Files ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let entries = app.build_tree_entries();

    if entries.is_empty() {
        let items: Vec<ListItem> = vec![ListItem::new(Line::from(Span::styled(
            "  Working tree clean",
            Style::default().fg(Color::DarkGray),
        )))];
        let list = List::new(items).block(block);
        f.render_widget(list, area);
        return;
    }

    // Build set of matched tree entry indices and current match index
    let (match_set, current_match_idx) = if app.search.origin == SearchOrigin::FileTree {
        let set: HashSet<usize> = app
            .search
            .matches
            .iter()
            .filter_map(|m| match m {
                SearchMatch::TreeEntry(idx) => Some(*idx),
                _ => None,
            })
            .collect();
        let current = app.search.current_match_idx.and_then(|ci| {
            match app.search.matches.get(ci) {
                Some(SearchMatch::TreeEntry(idx)) => Some(*idx),
                _ => None,
            }
        });
        (set, current)
    } else {
        (HashSet::new(), None)
    };

    let items: Vec<ListItem> = entries
        .iter()
        .enumerate()
        .map(|(entry_idx, entry)| {
            let is_match = match_set.contains(&entry_idx);
            let is_current = current_match_idx == Some(entry_idx);
            match entry {
            TreeEntry::Dir {
                path,
                depth,
                collapsed,
            } => {
                let indent = " ".repeat(depth * 2);
                let icon = if *collapsed { "▶" } else { "▼" };
                let dir_name = path.rsplit('/').next().unwrap_or(path);
                let name_style = if is_current {
                    Style::default().fg(Color::Black).bg(Color::Rgb(200, 120, 0))
                } else if is_match {
                    Style::default().fg(Color::DarkGray).bg(Color::Rgb(60, 60, 0))
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                let line = Line::from(vec![
                    Span::raw(format!(" {indent}  ")),
                    Span::styled(format!("{icon} {dir_name}/"), name_style),
                ]);
                ListItem::new(line)
            }
            TreeEntry::File { file_idx, depth } => {
                let file = &app.diff_state.files[*file_idx];
                let indent = " ".repeat(depth * 2);
                let icon_color = match file.status {
                    FileStatus::Modified => Color::Yellow,
                    FileStatus::Added => Color::Green,
                    FileStatus::Deleted => Color::Red,
                    FileStatus::Renamed => Color::Blue,
                    FileStatus::Untracked => Color::DarkGray,
                };
                // For depth > 0, show only filename; for depth 0, show full path
                let display_name = if *depth > 0 {
                    file.path.rsplit('/').next().unwrap_or(&file.path)
                } else {
                    &file.path
                };
                let name_style = if is_current {
                    Style::default().fg(Color::Black).bg(Color::Rgb(200, 120, 0))
                } else if is_match {
                    Style::default().bg(Color::Rgb(60, 60, 0))
                } else {
                    Style::default()
                };
                let line = Line::from(vec![
                    Span::raw(format!(" {indent}")),
                    Span::styled(
                        format!("{} ", file.status.icon()),
                        Style::default()
                            .fg(icon_color)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(display_name.to_string(), name_style),
                ]);
                ListItem::new(line)
            }
        }})
        .collect();

    // Use custom selection rendering: if selected item is a search match,
    // use search highlight instead of default highlight_style.
    let selected = app.selected_tree_idx;
    let selected_is_match = match_set.contains(&selected);

    let highlight_style = if selected_is_match {
        // Let the item's own search-highlight style take precedence
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD)
    };

    let list = List::new(items).block(block).highlight_style(highlight_style);

    let mut state = ListState::default();
    state.select(Some(selected));
    f.render_stateful_widget(list, area, &mut state);
}
