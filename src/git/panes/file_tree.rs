use crate::core::app::AppContext;
use crate::core::pane::{Pane, PaneShared};
use crate::core::search::SearchMatch;
use crate::git::domain::diff::{FileDiff, FileStatus};
use crate::git::state::{PaneEvent, TreeEntry, PANE_DIFF_VIEW, PANE_FILE_TREE};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};
use std::collections::HashSet;
use std::rc::Rc;

pub struct FileTreePane {
    pub selected_idx: usize,
    pub collapsed_dirs: HashSet<String>,
    pub files: Rc<Vec<FileDiff>>,
}

impl FileTreePane {
    pub fn new(files: Rc<Vec<FileDiff>>) -> Self {
        Self {
            selected_idx: 0,
            collapsed_dirs: HashSet::new(),
            files,
        }
    }

    pub fn set_files(&mut self, files: Rc<Vec<FileDiff>>) {
        self.files = files;
    }

    pub fn tree_entries(&self) -> Vec<TreeEntry> {
        crate::git::domain::tree::build_tree_entries(&self.files, &self.collapsed_dirs)
    }

    pub fn selected_file(&self) -> Option<&FileDiff> {
        let entries = self.tree_entries();
        if let Some(TreeEntry::File { file_idx, .. }) = entries.get(self.selected_idx) {
            self.files.get(*file_idx)
        } else {
            None
        }
    }

    pub fn selected_file_idx(&self) -> Option<usize> {
        let entries = self.tree_entries();
        if let Some(TreeEntry::File { file_idx, .. }) = entries.get(self.selected_idx) {
            Some(*file_idx)
        } else {
            None
        }
    }

    pub fn handle_key(&mut self, _shared: &PaneShared, key: KeyEvent) -> Vec<PaneEvent> {
        match key.code {
            KeyCode::Char('/') => {
                return vec![PaneEvent::StartSearch(PANE_FILE_TREE)];
            }
            KeyCode::Char('n') => {
                return vec![PaneEvent::JumpToMatch(true)];
            }
            KeyCode::Char('N') => {
                return vec![PaneEvent::JumpToMatch(false)];
            }
            _ => {}
        }
        let entries = self.tree_entries();
        if entries.is_empty() {
            return vec![];
        }
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if self.selected_idx + 1 < entries.len() {
                    self.selected_idx += 1;
                    return vec![PaneEvent::SelectionChanged];
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.selected_idx > 0 {
                    self.selected_idx -= 1;
                    return vec![PaneEvent::SelectionChanged];
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
                    return vec![PaneEvent::SetFocus(PANE_DIFF_VIEW)];
                }
                None => {}
            },
            _ => {}
        }
        vec![]
    }

    pub fn collect_search_matches(&self, _shared: &PaneShared, query: &str) -> Vec<SearchMatch> {
        let query_lower = query.to_lowercase();
        let entries = self.tree_entries();
        entries
            .iter()
            .enumerate()
            .filter_map(|(idx, entry)| {
                let name = match entry {
                    TreeEntry::Dir { path, .. } => path.clone(),
                    TreeEntry::File { file_idx, .. } => self.files.get(*file_idx)?.path.clone(),
                };
                if name.to_lowercase().contains(&query_lower) {
                    Some(SearchMatch::ListEntry(idx))
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn render(&mut self, f: &mut Frame, _ctx: &AppContext, shared: &PaneShared, area: Rect) {
        let border_color = if shared.focused_pane == PANE_FILE_TREE {
            Color::Cyan
        } else {
            Color::DarkGray
        };

        let block = Block::default()
            .title(" Files ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color));

        let entries = self.tree_entries();

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
        let (match_set, current_match_idx) = if shared.search.origin == PANE_FILE_TREE {
            let set: HashSet<usize> = shared
                .search
                .matches
                .iter()
                .filter_map(|m| match m {
                    SearchMatch::ListEntry(idx) => Some(*idx),
                    _ => None,
                })
                .collect();
            let current = shared.search.current_match_idx.and_then(|ci| {
                match shared.search.matches.get(ci) {
                    Some(SearchMatch::ListEntry(idx)) => Some(*idx),
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
                        let file = &self.files[*file_idx];
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

impl Pane<PaneEvent> for FileTreePane {
    fn handle_key(&mut self, shared: &PaneShared, key: KeyEvent) -> Vec<PaneEvent> {
        self.handle_key(shared, key)
    }

    fn render(&mut self, f: &mut Frame, ctx: &AppContext, shared: &PaneShared, area: Rect) {
        self.render(f, ctx, shared, area)
    }

    fn set_selected_idx(&mut self, idx: usize) {
        self.selected_idx = idx;
    }

    fn collect_search_matches(&self, shared: &PaneShared, query: &str) -> Vec<SearchMatch> {
        self.collect_search_matches(shared, query)
    }

    fn jump_to_match(&mut self, _shared: &PaneShared, search_match: &SearchMatch) {
        if let SearchMatch::ListEntry(idx) = search_match {
            self.selected_idx = *idx;
        }
    }
}
