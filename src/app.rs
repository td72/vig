use crate::git::diff::{DiffState, FileDiff};
use crate::git::repository::Repo;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusedPane {
    FileTree,
    DiffView,
}

#[derive(Debug, Clone)]
pub enum TreeEntry {
    Dir {
        path: String,
        depth: usize,
        collapsed: bool,
    },
    File {
        file_idx: usize,
        depth: usize,
    },
}

pub struct App {
    pub should_quit: bool,
    pub repo: Repo,
    pub diff_state: DiffState,
    pub collapsed_dirs: HashSet<String>,
    pub selected_tree_idx: usize,
    pub focused_pane: FocusedPane,
    pub diff_scroll_y: u16,
    pub diff_scroll_x: u16,
    pub diff_total_lines: u16,
    pub diff_view_height: u16,
    pub show_help: bool,
    pub status_message: Option<String>,
}

impl App {
    pub fn new(repo: Repo) -> Result<Self> {
        let diff_state = repo.diff_workdir()?;
        Ok(Self {
            should_quit: false,
            repo,
            diff_state,
            collapsed_dirs: HashSet::new(),
            selected_tree_idx: 0,
            focused_pane: FocusedPane::FileTree,
            diff_scroll_y: 0,
            diff_scroll_x: 0,
            diff_total_lines: 0,
            diff_view_height: 0,
            show_help: false,
            status_message: None,
        })
    }

    pub fn selected_file(&self) -> Option<&FileDiff> {
        let entries = self.build_tree_entries();
        if let Some(TreeEntry::File { file_idx, .. }) = entries.get(self.selected_tree_idx) {
            self.diff_state.files.get(*file_idx)
        } else {
            None
        }
    }

    pub fn refresh_diff(&mut self) -> Result<()> {
        let old_path = self.selected_file().map(|f| f.path.clone());
        self.diff_state = self.repo.diff_workdir()?;
        // Preserve selection by path
        if let Some(path) = old_path {
            let entries = self.build_tree_entries();
            self.selected_tree_idx = entries
                .iter()
                .position(|e| matches!(e, TreeEntry::File { file_idx, .. } if self.diff_state.files.get(*file_idx).map(|f| &f.path) == Some(&path)))
                .unwrap_or(0);
        }
        let entries = self.build_tree_entries();
        if self.selected_tree_idx >= entries.len() && !entries.is_empty() {
            self.selected_tree_idx = entries.len() - 1;
        }
        self.diff_scroll_y = 0;
        self.diff_scroll_x = 0;
        self.status_message = None;
        Ok(())
    }

    pub fn build_tree_entries(&self) -> Vec<TreeEntry> {
        let files = &self.diff_state.files;
        if files.is_empty() {
            return Vec::new();
        }

        // Count files per directory to detect single-file directories
        let mut dir_file_count: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for file in files {
            let parts: Vec<&str> = file.path.rsplitn(2, '/').collect();
            if parts.len() == 2 {
                // Has a directory component
                let dir = parts[1];
                // Count for this dir and all ancestor dirs
                let mut current = String::new();
                for segment in dir.split('/') {
                    if !current.is_empty() {
                        current.push('/');
                    }
                    current.push_str(segment);
                    *dir_file_count.entry(current.clone()).or_insert(0) += 1;
                }
            }
        }

        let mut entries = Vec::new();
        let mut prev_dir_parts: Vec<&str> = Vec::new();

        for (file_idx, file) in files.iter().enumerate() {
            let parts: Vec<&str> = file.path.rsplitn(2, '/').collect();
            if parts.len() == 2 {
                let dir = parts[1];
                let dir_parts: Vec<&str> = dir.split('/').collect();

                // Check if the entire path from root is single-file at every level
                // If so, inline the file (show full path, no directory node)
                let leaf_dir = dir.to_string();
                if dir_file_count.get(&leaf_dir).copied().unwrap_or(0) == 1 {
                    // Single file in this directory — inline with full path at depth 0
                    entries.push(TreeEntry::File {
                        file_idx,
                        depth: 0,
                    });
                    // Don't update prev_dir_parts since we inlined
                    prev_dir_parts = Vec::new();
                    continue;
                }

                // Find common prefix with previous directory
                let common_len = prev_dir_parts
                    .iter()
                    .zip(dir_parts.iter())
                    .take_while(|(a, b)| a == b)
                    .count();

                // Emit new directory entries for parts beyond common prefix
                let mut collapsed_ancestor = false;
                for i in common_len..dir_parts.len() {
                    let dir_path: String = dir_parts[..=i].join("/");
                    let is_collapsed = self.collapsed_dirs.contains(&dir_path);
                    if !collapsed_ancestor {
                        entries.push(TreeEntry::Dir {
                            path: dir_path.clone(),
                            depth: i,
                            collapsed: is_collapsed,
                        });
                    }
                    if is_collapsed {
                        collapsed_ancestor = true;
                    }
                }

                // Check if any ancestor dir is collapsed
                let mut skip_file = false;
                let mut check_path = String::new();
                for part in &dir_parts {
                    if !check_path.is_empty() {
                        check_path.push('/');
                    }
                    check_path.push_str(part);
                    if self.collapsed_dirs.contains(&check_path) {
                        skip_file = true;
                        break;
                    }
                }

                if !skip_file {
                    entries.push(TreeEntry::File {
                        file_idx,
                        depth: dir_parts.len(),
                    });
                }

                prev_dir_parts = dir_parts;
            } else {
                // Root-level file (no directory component)
                prev_dir_parts = Vec::new();
                entries.push(TreeEntry::File {
                    file_idx,
                    depth: 0,
                });
            }
        }

        entries
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Result<bool> {
        if self.show_help {
            self.show_help = false;
            return Ok(false);
        }

        match key.code {
            KeyCode::Char('q') => {
                self.should_quit = true;
                return Ok(false);
            }
            KeyCode::Char('?') => {
                self.show_help = true;
            }
            KeyCode::Char('r') => {
                self.refresh_diff()?;
            }
            KeyCode::Char('e') => {
                return Ok(true); // Signal to open editor
            }
            KeyCode::Tab | KeyCode::BackTab => {
                self.focused_pane = match self.focused_pane {
                    FocusedPane::FileTree => FocusedPane::DiffView,
                    FocusedPane::DiffView => FocusedPane::FileTree,
                };
            }
            _ => match self.focused_pane {
                FocusedPane::FileTree => self.handle_file_tree_key(key),
                FocusedPane::DiffView => self.handle_diff_view_key(key),
            },
        }
        Ok(false)
    }

    fn handle_file_tree_key(&mut self, key: KeyEvent) {
        let entries = self.build_tree_entries();
        if entries.is_empty() {
            return;
        }
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if self.selected_tree_idx + 1 < entries.len() {
                    self.selected_tree_idx += 1;
                    self.diff_scroll_y = 0;
                    self.diff_scroll_x = 0;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.selected_tree_idx > 0 {
                    self.selected_tree_idx -= 1;
                    self.diff_scroll_y = 0;
                    self.diff_scroll_x = 0;
                }
            }
            KeyCode::Char(' ') => {
                if let Some(TreeEntry::Dir { path, .. }) = entries.get(self.selected_tree_idx) {
                    let path = path.clone();
                    if self.collapsed_dirs.contains(&path) {
                        self.collapsed_dirs.remove(&path);
                    } else {
                        self.collapsed_dirs.insert(path);
                    }
                }
            }
            KeyCode::Enter => {
                match entries.get(self.selected_tree_idx) {
                    Some(TreeEntry::Dir { path, .. }) => {
                        let path = path.clone();
                        if self.collapsed_dirs.contains(&path) {
                            self.collapsed_dirs.remove(&path);
                        } else {
                            self.collapsed_dirs.insert(path);
                        }
                    }
                    Some(TreeEntry::File { .. }) => {
                        self.focused_pane = FocusedPane::DiffView;
                        self.diff_scroll_y = 0;
                        self.diff_scroll_x = 0;
                    }
                    None => {}
                }
            }
            _ => {}
        }
    }

    fn handle_diff_view_key(&mut self, key: KeyEvent) {
        let max_scroll = self.diff_total_lines.saturating_sub(self.diff_view_height);
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.diff_scroll_y = (self.diff_scroll_y + 1).min(max_scroll);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.diff_scroll_y = self.diff_scroll_y.saturating_sub(1);
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = self.diff_view_height / 2;
                self.diff_scroll_y = (self.diff_scroll_y + half).min(max_scroll);
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = self.diff_view_height / 2;
                self.diff_scroll_y = self.diff_scroll_y.saturating_sub(half);
            }
            KeyCode::Char('g') => {
                self.diff_scroll_y = 0;
            }
            KeyCode::Char('G') => {
                self.diff_scroll_y = max_scroll;
            }
            KeyCode::Char('h') => {
                self.focused_pane = FocusedPane::FileTree;
            }
            KeyCode::Left => {
                self.diff_scroll_x = self.diff_scroll_x.saturating_sub(4);
            }
            KeyCode::Char('l') | KeyCode::Right => {
                self.diff_scroll_x = self.diff_scroll_x.saturating_add(4);
            }
            _ => {}
        }
    }
}
