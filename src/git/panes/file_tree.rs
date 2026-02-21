use crate::core::app::{AppContext, SearchMatch, SearchOrigin};
use crate::core::pane::{FocusState, SelectPane};
use crate::git::domain::diff::FileStatus;
use crate::git::domain::search;
use crate::git::state::{FocusedPane, GitState, TreeEntry};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};
use std::collections::HashSet;

pub struct FileTreePane;

impl SelectPane<GitState> for FileTreePane {
    fn handle_key(&self, _ctx: &mut AppContext, state: &mut GitState, key: KeyEvent) {
        let entries = state.build_tree_entries();
        if entries.is_empty() {
            return;
        }
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if state.selected_tree_idx + 1 < entries.len() {
                    state.selected_tree_idx += 1;
                    state.diff_scroll_y = 0;
                    state.diff_scroll_x = 0;
                    search::re_search_on_file_change(state);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if state.selected_tree_idx > 0 {
                    state.selected_tree_idx -= 1;
                    state.diff_scroll_y = 0;
                    state.diff_scroll_x = 0;
                    search::re_search_on_file_change(state);
                }
            }
            KeyCode::Char(' ') => {
                if let Some(TreeEntry::Dir { path, .. }) = entries.get(state.selected_tree_idx) {
                    let path = path.clone();
                    if state.collapsed_dirs.contains(&path) {
                        state.collapsed_dirs.remove(&path);
                    } else {
                        state.collapsed_dirs.insert(path);
                    }
                }
            }
            KeyCode::Right | KeyCode::Enter => match entries.get(state.selected_tree_idx) {
                Some(TreeEntry::Dir { path, .. }) => {
                    let path = path.clone();
                    if state.collapsed_dirs.contains(&path) {
                        state.collapsed_dirs.remove(&path);
                    } else {
                        state.collapsed_dirs.insert(path);
                    }
                }
                Some(TreeEntry::File { .. }) => {
                    state.set_focus(FocusedPane::DiffView);
                    state.diff_scroll_y = 0;
                    state.diff_scroll_x = 0;
                }
                None => {}
            },
            _ => {}
        }
    }

    fn render(&self, f: &mut Frame, _ctx: &AppContext, state: &mut GitState, area: Rect) {
        let border_color = if state.focused_pane == FocusedPane::FileTree {
            Color::Cyan
        } else {
            Color::DarkGray
        };

        let block = Block::default()
            .title(" Files ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color));

        let entries = state.build_tree_entries();

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
        let (match_set, current_match_idx) =
            if state.search.origin == SearchOrigin::FileTree {
                let set: HashSet<usize> = state
                    .search
                    .matches
                    .iter()
                    .filter_map(|m| match m {
                        SearchMatch::TreeEntry(idx) => Some(*idx),
                        _ => None,
                    })
                    .collect();
                let current = state.search.current_match_idx.and_then(|ci| {
                    match state.search.matches.get(ci) {
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
                            Style::default()
                                .fg(Color::Black)
                                .bg(Color::Rgb(200, 120, 0))
                        } else if is_match {
                            Style::default()
                                .fg(Color::DarkGray)
                                .bg(Color::Rgb(60, 60, 0))
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
                        let file = &state.diff_state.files[*file_idx];
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
                            Style::default()
                                .fg(Color::Black)
                                .bg(Color::Rgb(200, 120, 0))
                        } else if is_match {
                            Style::default().bg(Color::Rgb(60, 60, 0))
                        } else {
                            Style::default()
                        };
                        let line = Line::from(vec![
                            Span::raw(format!(" {indent}")),
                            Span::styled(
                                format!("{} ", file.status.icon()),
                                Style::default().fg(icon_color).add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(display_name.to_string(), name_style),
                        ]);
                        ListItem::new(line)
                    }
                }
            })
            .collect();

        let selected = state.selected_tree_idx;
        let selected_is_match = match_set.contains(&selected);

        let highlight_style = if selected_is_match {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD)
        };

        let list = List::new(items)
            .block(block)
            .highlight_style(highlight_style);

        let mut state2 = ListState::default();
        state2.select(Some(selected));
        f.render_stateful_widget(list, area, &mut state2);
    }
}
