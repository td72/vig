use crate::core::app::{AppContext, SearchMatch, SearchOrigin};
use crate::git::domain::diff::{FileDiff, FileStatus};
use crate::git::state::{FocusedPane, GitShared, PaneEvent, TreeEntry};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};
use std::collections::HashSet;

pub struct FileTreePane {
    pub selected_idx: usize,
    pub collapsed_dirs: HashSet<String>,
}

impl FileTreePane {
    pub fn new() -> Self {
        Self {
            selected_idx: 0,
            collapsed_dirs: HashSet::new(),
        }
    }

    pub fn tree_entries(&self, shared: &GitShared) -> Vec<TreeEntry> {
        crate::git::domain::tree::build_tree_entries(&shared.diff_state.files, &self.collapsed_dirs)
    }

    pub fn selected_file<'a>(&self, shared: &'a GitShared) -> Option<&'a FileDiff> {
        let entries = self.tree_entries(shared);
        if let Some(TreeEntry::File { file_idx, .. }) = entries.get(self.selected_idx) {
            shared.diff_state.files.get(*file_idx)
        } else {
            None
        }
    }

    pub fn handle_key(&mut self, shared: &GitShared, key: KeyEvent) -> Vec<PaneEvent> {
        let entries = self.tree_entries(shared);
        if entries.is_empty() {
            return vec![];
        }
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if self.selected_idx + 1 < entries.len() {
                    self.selected_idx += 1;
                    return vec![PaneEvent::ResetDiffScroll, PaneEvent::ReSearchOnFileChange];
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.selected_idx > 0 {
                    self.selected_idx -= 1;
                    return vec![PaneEvent::ResetDiffScroll, PaneEvent::ReSearchOnFileChange];
                }
            }
            KeyCode::Char(' ') => {
                if let Some(TreeEntry::Dir { path, .. }) = entries.get(self.selected_idx) {
                    let path = path.clone();
                    if self.collapsed_dirs.contains(&path) {
                        self.collapsed_dirs.remove(&path);
                    } else {
                        self.collapsed_dirs.insert(path);
                    }
                }
            }
            KeyCode::Right | KeyCode::Enter => match entries.get(self.selected_idx) {
                Some(TreeEntry::Dir { path, .. }) => {
                    let path = path.clone();
                    if self.collapsed_dirs.contains(&path) {
                        self.collapsed_dirs.remove(&path);
                    } else {
                        self.collapsed_dirs.insert(path);
                    }
                }
                Some(TreeEntry::File { .. }) => {
                    return vec![
                        PaneEvent::SetFocus(FocusedPane::DiffView),
                        PaneEvent::ResetDiffScroll,
                    ];
                }
                None => {}
            },
            _ => {}
        }
        vec![]
    }

    pub fn collect_search_matches(&self, shared: &GitShared, query: &str) -> Vec<SearchMatch> {
        let query_lower = query.to_lowercase();
        let entries = self.tree_entries(shared);
        entries
            .iter()
            .enumerate()
            .filter_map(|(idx, entry)| {
                let name = match entry {
                    TreeEntry::Dir { path, .. } => path.clone(),
                    TreeEntry::File { file_idx, .. } => {
                        shared.diff_state.files.get(*file_idx)?.path.clone()
                    }
                };
                if name.to_lowercase().contains(&query_lower) {
                    Some(SearchMatch::TreeEntry(idx))
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn render(&self, f: &mut Frame, _ctx: &AppContext, shared: &GitShared, area: Rect) {
        let border_color = if shared.focused_pane == FocusedPane::FileTree {
            Color::Cyan
        } else {
            Color::DarkGray
        };

        let block = Block::default()
            .title(" Files ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color));

        let entries = self.tree_entries(shared);

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
        let (match_set, current_match_idx) = if shared.search.origin == SearchOrigin::FileTree {
            let set: HashSet<usize> = shared
                .search
                .matches
                .iter()
                .filter_map(|m| match m {
                    SearchMatch::TreeEntry(idx) => Some(*idx),
                    _ => None,
                })
                .collect();
            let current = shared.search.current_match_idx.and_then(|ci| {
                match shared.search.matches.get(ci) {
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
                        let file = &shared.diff_state.files[*file_idx];
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

        let selected = self.selected_idx;
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

        let mut list_state = ListState::default();
        list_state.select(Some(selected));
        f.render_stateful_widget(list, area, &mut list_state);
    }
}
