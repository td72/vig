use crate::core::app::AppContext;
use crate::core::keymap::{
    nav_bindings, search_bindings, ActionHelp, Keymap, NavAction, SearchAction,
};
use crate::core::pane::{self, Pane, PaneShared};
use crate::core::search::SearchMatch;
use crate::core::theme;
use crate::git::domain::diff::{FileDiff, FileStatus};
use crate::git::state::PaneEvent;

pub use crate::git::domain::tree::TreeEntry;
use crossterm::event::KeyCode;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::ListItem,
    Frame,
};
use std::collections::HashSet;
use std::rc::Rc;

#[derive(Debug, Clone)]
pub enum FileTreeAction {
    Nav(NavAction),
    ToggleDir,
    ExpandOrOpen,
    FocusDiff,
    Search(SearchAction),
    Esc,
}

crate::impl_pane_action_from_str!(
    FileTreeAction, nav: Nav, search: Search, esc: Esc,
    ToggleDir, ExpandOrOpen, FocusDiff
);

impl ActionHelp for FileTreeAction {
    fn label(&self) -> Option<&'static str> {
        match self {
            FileTreeAction::Nav(nav) => nav.label(),
            FileTreeAction::ToggleDir => Some("Toggle directory"),
            FileTreeAction::ExpandOrOpen => Some("Expand / Open file"),
            FileTreeAction::FocusDiff => Some("Focus diff view"),
            FileTreeAction::Search(sa) => sa.label(),
            FileTreeAction::Esc => Some("Clear search / Back"),
        }
    }
}

pub fn default_keymap() -> Keymap<FileTreeAction> {
    Keymap::new()
        .bindings(nav_bindings(FileTreeAction::Nav))
        .bindings(search_bindings(FileTreeAction::Search))
        .key(KeyCode::Char(' '), FileTreeAction::ToggleDir)
        .key(KeyCode::Right, FileTreeAction::ExpandOrOpen)
        .key(KeyCode::Enter, FileTreeAction::ExpandOrOpen)
        .key(KeyCode::Char('i'), FileTreeAction::FocusDiff)
        .key(KeyCode::Esc, FileTreeAction::Esc)
}

pub struct FileTreePane {
    pub selected_idx: usize,
    pub collapsed_dirs: HashSet<String>,
    pub files: Rc<Vec<FileDiff>>,
    keymap: Keymap<FileTreeAction>,
    pane_id: usize,
    diff_view_id: usize,
}

impl FileTreePane {
    pub fn new(files: Rc<Vec<FileDiff>>, pane_id: usize, diff_view_id: usize) -> Self {
        Self {
            selected_idx: 0,
            collapsed_dirs: HashSet::new(),
            files,
            keymap: default_keymap(),
            pane_id,
            diff_view_id,
        }
    }

    pub fn set_files(&mut self, files: Rc<Vec<FileDiff>>) {
        self.files = files;
    }

    pub fn set_keymap(&mut self, km: Keymap<FileTreeAction>) {
        self.keymap = km;
    }

    /// Collect highlight data for all non-binary files.
    #[allow(clippy::type_complexity)]
    pub fn highlight_file_data(&self) -> Vec<(String, Vec<String>, Vec<String>, Vec<usize>)> {
        self.files
            .iter()
            .filter(|f| !f.is_binary)
            .map(|f| f.highlight_data())
            .collect()
    }

    /// Restore selection to the file at `old_path` after a file list change,
    /// or clamp to valid bounds.
    pub fn restore_selection(&mut self, old_path: Option<String>) {
        let entries = self.tree_entries();
        if let Some(path) = old_path {
            self.selected_idx = entries
                .iter()
                .position(|e| {
                    matches!(e, TreeEntry::File { file_idx, .. }
                        if self.files.get(*file_idx).map(|f| &f.path) == Some(&path))
                })
                .unwrap_or(0);
        }
        if self.selected_idx >= entries.len() && !entries.is_empty() {
            self.selected_idx = entries.len() - 1;
        }
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

    fn execute(&mut self, shared: &PaneShared, action: FileTreeAction) -> Vec<PaneEvent> {
        // Handle Search/Esc first
        if let Some(events) = pane::try_dispatch_search_esc(&action, shared, self.pane_id, vec![]) {
            return events;
        }
        if let FileTreeAction::FocusDiff = action {
            return vec![PaneEvent::SetFocus(self.diff_view_id)];
        }

        let entries = self.tree_entries();
        if entries.is_empty() {
            return vec![];
        }

        match action {
            FileTreeAction::Nav(nav) => {
                return pane::execute_list_nav(nav, &mut self.selected_idx, entries.len(), None);
            }
            FileTreeAction::ToggleDir => {
                self.toggle_dir(&entries);
            }
            FileTreeAction::ExpandOrOpen => match entries.get(self.selected_idx) {
                Some(TreeEntry::Dir { .. }) => {
                    self.toggle_dir(&entries);
                }
                Some(TreeEntry::File { .. }) => {
                    return vec![PaneEvent::SetFocus(self.diff_view_id)];
                }
                None => {}
            },
            _ => {}
        }
        vec![]
    }

    fn toggle_dir(&mut self, entries: &[TreeEntry]) {
        if let Some(TreeEntry::Dir { path, .. }) = entries.get(self.selected_idx) {
            let path = path.clone();
            if self.collapsed_dirs.contains(&path) {
                self.collapsed_dirs.remove(&path);
            } else {
                self.collapsed_dirs.insert(path);
            }
        }
    }

    fn render_impl(&mut self, f: &mut Frame, _ctx: &AppContext, shared: &PaneShared, area: Rect) {
        let entries = self.tree_entries();
        let empty = entries.is_empty().then_some("Working tree clean");
        theme::render_list_pane(
            f,
            area,
            shared,
            self.pane_id,
            "Files",
            Some(self.selected_idx),
            empty,
            |match_set, current_match_idx| {
                entries
                    .iter()
                    .enumerate()
                    .map(|(entry_idx, entry)| {
                        let hl =
                            theme::search_highlight_for(match_set, current_match_idx, entry_idx);
                        match entry {
                            TreeEntry::Dir {
                                path,
                                depth,
                                collapsed,
                            } => {
                                let indent = " ".repeat(depth * 2);
                                let icon = if *collapsed { "▶" } else { "▼" };
                                let dir_name = path.rsplit('/').next().unwrap_or(path);
                                let name_style = hl.style_with_fg(Color::DarkGray);
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
                                let name_style = hl.apply(Style::default());
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
                        }
                    })
                    .collect()
            },
        );
    }
}

impl Pane<PaneEvent> for FileTreePane {
    crate::impl_handle_key!(keymap);

    fn render(&mut self, f: &mut Frame, ctx: &AppContext, shared: &PaneShared, area: Rect) {
        self.render_impl(f, ctx, shared, area)
    }

    fn collect_search_matches(&self, _shared: &PaneShared, query: &str) -> Vec<SearchMatch> {
        let entries = self.tree_entries();
        pane::collect_list_search_matches(&entries, query, |entry| match entry {
            TreeEntry::Dir { path, .. } => path.clone(),
            TreeEntry::File { file_idx, .. } => self
                .files
                .get(*file_idx)
                .map(|f| f.path.clone())
                .unwrap_or_default(),
        })
    }

    crate::impl_list_pane_selection!();
}
