use crate::core::app::SearchState;
use crate::core::syntax::{HighlightCache, HighlightPair, SyntaxHighlighter};
use crate::git::domain::diff::{DiffState, FileDiff};
use crate::git::domain::graph::{self, GraphRow};
use crate::git::domain::repository::{BranchInfo, CommitFileChange, CommitInfo, ReflogEntry, Repo};
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusedPane {
    FileTree,
    BranchList,
    GitLog,
    Reflog,
    DiffView,
}

pub struct BranchListState {
    pub branches: Vec<BranchInfo>,
    pub selected_idx: usize,
}

pub struct GitLogState {
    pub commits: Vec<CommitInfo>,
    pub selected_idx: usize,
    pub view_height: u16,
    pub ref_name: String,
    pub graph: Vec<GraphRow>,
    pub detail_scroll: u16,
    pub detail_view_height: u16,
    pub detail_changed_files: Vec<CommitFileChange>,
}

pub struct ReflogState {
    pub entries: Vec<ReflogEntry>,
    pub selected_idx: usize,
    pub view_height: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchAction {
    Switch,
    Delete,
    DiffBase,
}

impl BranchAction {
    pub const ALL: [BranchAction; 3] = [
        BranchAction::Switch,
        BranchAction::Delete,
        BranchAction::DiffBase,
    ];

    pub fn label(self) -> &'static str {
        match self {
            BranchAction::Switch => "Switch",
            BranchAction::Delete => "Delete",
            BranchAction::DiffBase => "Set as diff base",
        }
    }

    pub fn key(self) -> char {
        match self {
            BranchAction::Switch => 's',
            BranchAction::Delete => 'd',
            BranchAction::DiffBase => 'b',
        }
    }
}

pub struct BranchActionMenuState {
    pub branch_name: String,
    pub is_head: bool,
    pub selected_idx: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffViewMode {
    Scroll,
    Normal,
    Visual,
    VisualLine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffSide {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorPos {
    pub row: usize,
    pub col: usize,
    pub side: DiffSide,
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

pub struct GitState {
    pub repo: Repo,
    pub diff_state: DiffState,
    pub collapsed_dirs: HashSet<String>,
    pub selected_tree_idx: usize,
    pub focused_pane: FocusedPane,
    pub previous_pane: FocusedPane,
    pub diff_scroll_y: u16,
    pub diff_scroll_x: u16,
    pub diff_total_lines: u16,
    pub diff_view_height: u16,
    pub diff_view_mode: DiffViewMode,
    pub cursor_pos: CursorPos,
    pub visual_anchor: Option<CursorPos>,
    pub pending_key: Option<char>,
    pub count: Option<usize>,
    pub highlighter: SyntaxHighlighter,
    pub highlight_cache: Option<HighlightCache>,
    /// Cached content_lines result: (file_path, side, lines). Invalidated on file/side switch.
    pub(crate) content_lines_cache: Option<(String, DiffSide, Vec<String>)>,
    /// Pre-computed highlight results from background thread, keyed by file path.
    pub(crate) bg_highlights: HashMap<String, HighlightPair>,
    /// Receiver for background highlight results.
    pub(crate) bg_highlight_rx: Option<mpsc::Receiver<(String, HighlightPair)>>,
    pub diff_base_ref: Option<String>,
    pub branch_list: BranchListState,
    pub git_log: GitLogState,
    pub reflog: ReflogState,
    pub branch_action_menu: Option<BranchActionMenuState>,
    pub search: SearchState,
}

impl GitState {
    pub fn new(cwd: &Path) -> Result<Self> {
        let repo = Repo::discover(cwd)?;
        let diff_state = repo.diff_workdir(None)?;
        let mut state = Self {
            repo,
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
            cursor_pos: CursorPos {
                row: 0,
                col: 0,
                side: DiffSide::Left,
            },
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
        };
        state.load_branches();
        state.load_reflog();
        state.spawn_bg_highlight();
        Ok(state)
    }

    /// Refresh the diff state from the working directory.
    /// Returns `Ok(Some(message))` if a fallback occurred, `Ok(None)` on clean refresh.
    pub fn refresh_diff(&mut self) -> Result<Option<String>> {
        let old_path = self.selected_file().map(|f| f.path.clone());
        let fallback_msg = match self.repo.diff_workdir(self.diff_base_ref.as_deref()) {
            Ok(state) => {
                self.diff_state = state;
                None
            }
            Err(e) => {
                self.diff_base_ref = None;
                self.diff_state = self.repo.diff_workdir(None)?;
                Some(format!("Invalid ref, fell back to HEAD: {e}"))
            }
        };
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
        self.highlight_cache = None;
        self.content_lines_cache = None;
        self.bg_highlights.clear();
        self.bg_highlight_rx = None;
        self.search.reset_matches();
        self.spawn_bg_highlight();
        Ok(fallback_msg)
    }

    pub(crate) fn set_focus(&mut self, pane: FocusedPane) {
        self.previous_pane = self.focused_pane;
        self.focused_pane = pane;
    }

    pub fn selected_file(&self) -> Option<&FileDiff> {
        let entries = self.build_tree_entries();
        if let Some(TreeEntry::File { file_idx, .. }) = entries.get(self.selected_tree_idx) {
            self.diff_state.files.get(*file_idx)
        } else {
            None
        }
    }

    /// Ensure syntax highlighting is available up to `up_to` rows for the given file.
    /// Uses pre-computed background results if available, otherwise falls back to on-demand.
    pub fn ensure_file_highlight(&mut self, file: &FileDiff, up_to: usize) {
        let needs_init = self
            .highlight_cache
            .as_ref()
            .map(|c| c.file_path != file.path)
            .unwrap_or(true);

        if needs_init {
            // Check for pre-computed background highlight results first
            if let Some((lc, rc)) = self.bg_highlights.remove(&file.path) {
                self.highlight_cache =
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
                        row.left
                            .as_ref()
                            .map(|s| s.content.clone())
                            .unwrap_or_default(),
                    );
                    right_lines.push(
                        row.right
                            .as_ref()
                            .map(|s| s.content.clone())
                            .unwrap_or_default(),
                    );
                }
            }
            self.highlight_cache =
                self.highlighter
                    .create_cache(&file.path, left_lines, right_lines, hunk_starts);
        }

        if let Some(ref mut cache) = self.highlight_cache {
            self.highlighter.extend_cache(cache, up_to);
        }
    }

    /// Spawn a background thread to pre-highlight all files.
    pub(crate) fn spawn_bg_highlight(&mut self) {
        let mut file_data: Vec<_> = Vec::new();
        for file in &self.diff_state.files {
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
                        row.left
                            .as_ref()
                            .map(|s| s.content.clone())
                            .unwrap_or_default(),
                    );
                    right_lines.push(
                        row.right
                            .as_ref()
                            .map(|s| s.content.clone())
                            .unwrap_or_default(),
                    );
                }
            }
            file_data.push((file.path.clone(), left_lines, right_lines, hunk_starts));
        }

        if file_data.is_empty() {
            return;
        }

        let (tx, rx) = mpsc::channel();
        self.bg_highlight_rx = Some(rx);

        std::thread::spawn(move || {
            let highlighter = SyntaxHighlighter::new();
            for (path, left_lines, right_lines, hunk_starts) in file_data {
                if let Some(pair) =
                    highlighter.highlight_all_lines(&path, &left_lines, &right_lines, &hunk_starts)
                {
                    if tx.send((path, pair)).is_err() {
                        break; // Receiver dropped
                    }
                }
            }
        });
    }

    /// Drain completed background highlight results into the local cache.
    pub fn drain_bg_highlights(&mut self) {
        if let Some(ref rx) = self.bg_highlight_rx {
            while let Ok((path, pair)) = rx.try_recv() {
                self.bg_highlights.insert(path, pair);
            }
        }
    }

    pub fn load_branches(&mut self) {
        self.branch_list.branches = self.repo.list_local_branches();
        if self.branch_list.selected_idx >= self.branch_list.branches.len() {
            self.branch_list.selected_idx = 0;
        }
        self.update_branch_log();
    }

    pub fn update_branch_log(&mut self) {
        if let Some(branch) = self.branch_list.branches.get(self.branch_list.selected_idx) {
            self.git_log.ref_name = branch.name.clone();
            self.git_log.commits = self.repo.log_for_ref(&branch.name, 100);
            self.git_log.graph = graph::build_graph(&self.git_log.commits);
            self.git_log.selected_idx = 0;
            self.git_log.detail_scroll = 0;
            self.git_log.detail_changed_files.clear();
            self.load_commit_detail();
        } else {
            self.git_log.commits.clear();
            self.git_log.graph.clear();
            self.git_log.ref_name.clear();
            self.git_log.detail_changed_files.clear();
        }
    }

    pub fn load_commit_detail(&mut self) {
        if let Some(commit) = self.git_log.commits.get(self.git_log.selected_idx) {
            self.git_log.detail_changed_files = self.repo.commit_changed_files(&commit.full_hash);
            self.git_log.detail_scroll = 0;
        } else {
            self.git_log.detail_changed_files.clear();
        }
    }

    pub fn load_reflog(&mut self) {
        self.reflog.entries = self.repo.reflog(500);
        if self.reflog.selected_idx >= self.reflog.entries.len() {
            self.reflog.selected_idx = 0;
        }
    }

    pub fn build_tree_entries(&self) -> Vec<TreeEntry> {
        let files = &self.diff_state.files;
        if files.is_empty() {
            return Vec::new();
        }

        // Count files per directory to detect single-file directories
        let mut dir_file_count: HashMap<String, usize> = HashMap::new();
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
                    entries.push(TreeEntry::File { file_idx, depth: 0 });
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
                entries.push(TreeEntry::File { file_idx, depth: 0 });
            }
        }

        entries
    }
}

impl crate::core::app::PageState for GitState {
    fn drain_background(&mut self) {
        self.drain_bg_highlights();
    }
}
