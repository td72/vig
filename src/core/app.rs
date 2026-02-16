use crate::git::diff::FileDiff;
use crate::git::graph;
use crate::git::repository::Repo;
pub use crate::git::state::*;
pub use crate::core::search::{SearchMatch, SearchOrigin, SearchState};
use crate::github::state::GitHubState;
use crate::core::syntax::{HighlightCache, SyntaxHighlighter};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Git,
    GitHub,
}

pub struct ErrorDialogState {
    pub title: String,
    pub message: String,
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
    pub view_mode: ViewMode,
    pub repo: Repo,
    pub show_help: bool,
    pub status_message: Option<String>,
    pub error_dialog: Option<ErrorDialogState>,
    pub git: GitState,
    pub github: GitHubState,
}

impl App {
    pub fn new(repo: Repo) -> Result<Self> {
        let diff_state = repo.diff_workdir(None)?;
        let mut app = Self {
            should_quit: false,
            view_mode: ViewMode::Git,
            repo,
            show_help: false,
            status_message: None,
            error_dialog: None,
            git: GitState {
                diff_state,
                collapsed_dirs: HashSet::new(),
                selected_tree_idx: 0,
                focused_pane: FocusedPane::FileTree,
                previous_pane: FocusedPane::FileTree,
                diff_scroll_y: 0,
                diff_scroll_x: 0,
                diff_total_lines: 0,
                diff_view_height: 0,
                diff_view_mode: DiffViewMode::Scroll,
                cursor_pos: CursorPos { row: 0, col: 0, side: DiffSide::Left },
                visual_anchor: None,
                pending_key: None,
                count: None,
                highlighter: SyntaxHighlighter::new(),
                highlight_cache: None,
                content_lines_cache: None,
                bg_highlights: HashMap::new(),
                bg_highlight_rx: None,
                diff_base_ref: None,
                branch_list: BranchListState {
                    branches: Vec::new(),
                    selected_idx: 0,
                },
                git_log: GitLogState {
                    commits: Vec::new(),
                    selected_idx: 0,
                    view_height: 0,
                    ref_name: String::new(),
                    graph: Vec::new(),
                    detail_scroll: 0,
                    detail_view_height: 0,
                    detail_changed_files: Vec::new(),
                },
                reflog: ReflogState {
                    entries: Vec::new(),
                    selected_idx: 0,
                    view_height: 0,
                },
                branch_action_menu: None,
                search: SearchState::new(),
            },
            github: GitHubState::new(),
        };
        app.load_branches();
        app.load_reflog();
        app.spawn_bg_highlight();
        Ok(app)
    }

    pub fn active_search(&self) -> Option<&SearchState> {
        match self.view_mode {
            ViewMode::Git => Some(&self.git.search),
            ViewMode::GitHub => None,
        }
    }

    pub fn selected_file(&self) -> Option<&FileDiff> {
        let entries = self.build_tree_entries();
        if let Some(TreeEntry::File { file_idx, .. }) = entries.get(self.git.selected_tree_idx) {
            self.git.diff_state.files.get(*file_idx)
        } else {
            None
        }
    }

    /// Ensure syntax highlighting is available up to `up_to` rows for the given file.
    /// Uses pre-computed background results if available, otherwise falls back to on-demand.
    pub fn ensure_file_highlight(&mut self, file: &FileDiff, up_to: usize) {
        let needs_init = self
            .git.highlight_cache
            .as_ref()
            .map(|c| c.file_path != file.path)
            .unwrap_or(true);

        if needs_init {
            // Check for pre-computed background highlight results first
            if let Some((lc, rc)) = self.git.bg_highlights.remove(&file.path) {
                self.git.highlight_cache =
                    Some(HighlightCache::from_precomputed(file.path.clone(), lc, rc));
                return;
            }

            // Fall back to on-demand highlighting
            let mut left_lines = Vec::new();
            let mut right_lines = Vec::new();
            let mut hunk_starts = Vec::new();
            for hunk in &file.hunks {
                hunk_starts.push(left_lines.len());
                left_lines.push(String::new());
                right_lines.push(String::new());
                for row in &hunk.rows {
                    left_lines.push(
                        row.left.as_ref().map(|s| s.content.clone()).unwrap_or_default(),
                    );
                    right_lines.push(
                        row.right.as_ref().map(|s| s.content.clone()).unwrap_or_default(),
                    );
                }
            }
            self.git.highlight_cache =
                self.git.highlighter
                    .create_cache(&file.path, left_lines, right_lines, hunk_starts);
        }

        if let Some(ref mut cache) = self.git.highlight_cache {
            self.git.highlighter.extend_cache(cache, up_to);
        }
    }

    pub fn refresh_diff(&mut self) -> Result<()> {
        let old_path = self.selected_file().map(|f| f.path.clone());
        match self.repo.diff_workdir(self.git.diff_base_ref.as_deref()) {
            Ok(state) => self.git.diff_state = state,
            Err(e) => {
                self.git.diff_base_ref = None;
                self.git.diff_state = self.repo.diff_workdir(None)?;
                self.status_message = Some(format!("Invalid ref, fell back to HEAD: {e}"));
            }
        }
        // Preserve selection by path
        if let Some(path) = old_path {
            let entries = self.build_tree_entries();
            self.git.selected_tree_idx = entries
                .iter()
                .position(|e| matches!(e, TreeEntry::File { file_idx, .. } if self.git.diff_state.files.get(*file_idx).map(|f| &f.path) == Some(&path)))
                .unwrap_or(0);
        }
        let entries = self.build_tree_entries();
        if self.git.selected_tree_idx >= entries.len() && !entries.is_empty() {
            self.git.selected_tree_idx = entries.len() - 1;
        }
        self.git.diff_scroll_y = 0;
        self.git.diff_scroll_x = 0;
        self.status_message = None;
        self.git.highlight_cache = None;
        self.git.content_lines_cache = None;
        self.git.bg_highlights.clear();
        self.git.bg_highlight_rx = None; // Drop old receiver, stops old thread
        self.git.search.reset_matches();
        self.spawn_bg_highlight();
        Ok(())
    }

    /// Spawn a background thread to pre-highlight all files.
    fn spawn_bg_highlight(&mut self) {
        let mut file_data: Vec<(String, Vec<String>, Vec<String>, Vec<usize>)> = Vec::new();
        for file in &self.git.diff_state.files {
            if file.is_binary {
                continue;
            }
            let mut left_lines = Vec::new();
            let mut right_lines = Vec::new();
            let mut hunk_starts = Vec::new();
            for hunk in &file.hunks {
                hunk_starts.push(left_lines.len());
                left_lines.push(String::new());
                right_lines.push(String::new());
                for row in &hunk.rows {
                    left_lines.push(
                        row.left.as_ref().map(|s| s.content.clone()).unwrap_or_default(),
                    );
                    right_lines.push(
                        row.right.as_ref().map(|s| s.content.clone()).unwrap_or_default(),
                    );
                }
            }
            file_data.push((file.path.clone(), left_lines, right_lines, hunk_starts));
        }

        if file_data.is_empty() {
            return;
        }

        let (tx, rx) = mpsc::channel();
        self.git.bg_highlight_rx = Some(rx);

        std::thread::spawn(move || {
            let highlighter = SyntaxHighlighter::new();
            for (path, left_lines, right_lines, hunk_starts) in file_data {
                if let Some((lc, rc)) = highlighter.highlight_all_lines(
                    &path, &left_lines, &right_lines, &hunk_starts,
                ) {
                    if tx.send((path, lc, rc)).is_err() {
                        break; // Receiver dropped
                    }
                }
            }
        });
    }

    /// Drain completed background highlight results into the local cache.
    pub fn drain_bg_highlights(&mut self) {
        if let Some(ref rx) = self.git.bg_highlight_rx {
            while let Ok((path, left, right)) = rx.try_recv() {
                self.git.bg_highlights.insert(path, (left, right));
            }
        }
    }

    pub fn load_branches(&mut self) {
        self.git.branch_list.branches = self.repo.list_local_branches();
        if self.git.branch_list.selected_idx >= self.git.branch_list.branches.len() {
            self.git.branch_list.selected_idx = 0;
        }
        self.update_branch_log();
    }

    pub(crate) fn set_focus(&mut self, pane: FocusedPane) {
        self.git.set_focus(pane);
    }

    pub fn update_branch_log(&mut self) {
        if let Some(branch) = self
            .git.branch_list
            .branches
            .get(self.git.branch_list.selected_idx)
        {
            self.git.git_log.ref_name = branch.name.clone();
            self.git.git_log.commits = self.repo.log_for_ref(&branch.name, 100);
            self.git.git_log.graph = graph::build_graph(&self.git.git_log.commits);
            self.git.git_log.selected_idx = 0;
            self.git.git_log.detail_scroll = 0;
            self.git.git_log.detail_changed_files.clear();
            self.load_commit_detail();
        } else {
            self.git.git_log.commits.clear();
            self.git.git_log.graph.clear();
            self.git.git_log.ref_name.clear();
            self.git.git_log.detail_changed_files.clear();
        }
    }

    pub fn load_commit_detail(&mut self) {
        if let Some(commit) = self.git.git_log.commits.get(self.git.git_log.selected_idx) {
            self.git.git_log.detail_changed_files =
                self.repo.commit_changed_files(&commit.full_hash);
            self.git.git_log.detail_scroll = 0;
        } else {
            self.git.git_log.detail_changed_files.clear();
        }
    }

    pub fn load_reflog(&mut self) {
        self.git.reflog.entries = self.repo.reflog(500);
        if self.git.reflog.selected_idx >= self.git.reflog.entries.len() {
            self.git.reflog.selected_idx = 0;
        }
    }


    pub fn build_tree_entries(&self) -> Vec<TreeEntry> {
        let files = &self.git.diff_state.files;
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
                    let is_collapsed = self.git.collapsed_dirs.contains(&dir_path);
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
                    if self.git.collapsed_dirs.contains(&check_path) {
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

        // Error dialog: any key dismisses
        if self.error_dialog.is_some() {
            self.error_dialog = None;
            return Ok(false);
        }

        // Action menu intercepts all keys when open
        if self.git.branch_action_menu.is_some() {
            self.handle_branch_action_menu_key(key);
            return Ok(false);
        }

        // Search input mode intercepts all keys
        match self.view_mode {
            ViewMode::Git if self.git.search.active => {
                if self.git.search.handle_input_key(key) {
                    self.execute_git_search();
                    self.jump_to_git_match(true);
                }
                return Ok(false);
            }
            _ => {}
        }

        // Ctrl+c always quits
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return Ok(false);
        }

        // View switching
        match key.code {
            KeyCode::Char('1') => {
                self.view_mode = ViewMode::Git;
                return Ok(false);
            }
            KeyCode::Char('2') => {
                self.view_mode = ViewMode::GitHub;
                self.github.initialize();
                return Ok(false);
            }
            _ => {}
        }

        // Delegate to domain container
        match self.view_mode {
            ViewMode::Git => crate::git::container::handle_git_view_key(self, key),
            ViewMode::GitHub => crate::github::container::handle_gh_view_key(self, key),
        }
    }
}
