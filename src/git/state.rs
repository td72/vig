use crate::core::app::SearchState;
use crate::core::pane::{DetailState, SubPaneScroll};
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

// === PaneEvent: cross-pane side effects ===

#[allow(dead_code)]
pub enum PaneEvent {
    SetFocus(FocusedPane),
    ResetDiffScroll,
    RefreshDiff,
    SetDiffBase(Option<String>),
    OpenBranchActionMenu,
    SwitchBranch(String),
    DeleteBranch(String),
    UpdateBranchLog,
    LoadCommitDetail,
    ReSearchOnFileChange,
    StartSearch(crate::core::app::SearchOrigin),
    ClearSearch,
    OpenEditor(String),
    Quit,
    ShowHelp,
    StatusMessage(String),
    ErrorDialog { title: String, message: String },
    CopyToClipboard(String),
    OpenUrl(String),
}

// === GitShared: immutable shared state for pane handle_key ===

pub struct GitShared {
    pub repo: Repo,
    pub diff_state: DiffState,
    pub diff_base_ref: Option<String>,
    pub focused_pane: FocusedPane,
    pub previous_pane: FocusedPane,
    pub search: SearchState,
}

pub struct BranchListState {
    pub branches: Vec<BranchInfo>,
    pub selected_idx: usize,
    pub action_menu: Option<BranchActionMenuState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitLogDetailPane {
    Detail,
}

pub struct GitLogState {
    pub commits: Vec<CommitInfo>,
    pub selected_idx: usize,
    pub view_height: u16,
    pub ref_name: String,
    pub graph: Vec<GraphRow>,
    pub detail: SubPaneScroll,
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

#[derive(Default)]
pub struct DiffScroll {
    pub y: u16,
    pub x: u16,
    pub total_lines: u16,
    pub view_height: u16,
}

pub struct VimState {
    pub mode: DiffViewMode,
    pub cursor: CursorPos,
    pub visual_anchor: Option<CursorPos>,
    pub pending_key: Option<char>,
    pub count: Option<usize>,
}

impl Default for VimState {
    fn default() -> Self {
        Self {
            mode: DiffViewMode::Scroll,
            cursor: CursorPos {
                row: 0,
                col: 0,
                side: DiffSide::Left,
            },
            visual_anchor: None,
            pending_key: None,
            count: None,
        }
    }
}

pub struct HighlightState {
    pub highlighter: SyntaxHighlighter,
    pub cache: Option<HighlightCache>,
    pub(crate) content_lines_cache: Option<(String, DiffSide, Vec<String>)>,
    pub(crate) bg_highlights: HashMap<String, HighlightPair>,
    pub(crate) bg_highlight_rx: Option<mpsc::Receiver<(String, HighlightPair)>>,
}

impl HighlightState {
    pub fn new() -> Self {
        Self {
            highlighter: SyntaxHighlighter::new(),
            cache: None,
            content_lines_cache: None,
            bg_highlights: HashMap::new(),
            bg_highlight_rx: None,
        }
    }

    pub fn reset(&mut self) {
        self.cache = None;
        self.content_lines_cache = None;
        self.bg_highlights.clear();
        self.bg_highlight_rx = None;
    }

    /// Ensure syntax highlighting is available up to `up_to` rows for the given file.
    /// Uses pre-computed background results if available, otherwise falls back to on-demand.
    pub fn ensure_file_highlight(&mut self, file: &FileDiff, up_to: usize) {
        let needs_init = self
            .cache
            .as_ref()
            .map(|c| c.file_path != file.path)
            .unwrap_or(true);

        if needs_init {
            // Check for pre-computed background highlight results first
            if let Some((lc, rc)) = self.bg_highlights.remove(&file.path) {
                self.cache = Some(HighlightCache::from_precomputed(file.path.clone(), lc, rc));
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
            self.cache =
                self.highlighter
                    .create_cache(&file.path, left_lines, right_lines, hunk_starts);
        }

        if let Some(ref mut cache) = self.cache {
            self.highlighter.extend_cache(cache, up_to);
        }
    }

    /// Spawn a background thread to pre-highlight all files.
    pub(crate) fn spawn_bg_highlight(&mut self, files: &[FileDiff]) {
        let mut file_data: Vec<_> = Vec::new();
        for file in files {
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
}

pub use crate::git::domain::tree::TreeEntry;

pub struct FileTreeState {
    pub selected_idx: usize,
    pub collapsed_dirs: HashSet<String>,
}

pub struct DiffViewState {
    pub scroll: DiffScroll,
    pub vim: VimState,
    pub highlight: HighlightState,
}

impl DiffViewState {
    pub fn scroll_to_cursor(&mut self) {
        let row = self.vim.cursor.row as u16;
        let height = self.scroll.view_height;
        if height == 0 {
            return;
        }
        if row < self.scroll.y {
            self.scroll.y = row;
        } else if row >= self.scroll.y + height {
            self.scroll.y = row - height + 1;
        }
    }
}

pub struct GitState {
    pub shared: GitShared,
    pub file_tree: FileTreeState,
    pub diff_view: DiffViewState,
    pub branch_list: BranchListState,
    pub git_log: GitLogState,
    pub reflog: ReflogState,
}

impl GitState {
    pub fn new(cwd: &Path) -> Result<Self> {
        let repo = Repo::discover(cwd)?;
        let diff_state = repo.diff_workdir(None)?;
        let mut state = Self {
            shared: GitShared {
                repo,
                diff_state,
                diff_base_ref: None,
                focused_pane: FocusedPane::FileTree,
                previous_pane: FocusedPane::FileTree,
                search: SearchState::new(),
            },
            file_tree: FileTreeState {
                selected_idx: 0,
                collapsed_dirs: HashSet::new(),
            },
            diff_view: DiffViewState {
                scroll: DiffScroll::default(),
                vim: VimState::default(),
                highlight: HighlightState::new(),
            },
            branch_list: BranchListState {
                branches: Vec::new(),
                selected_idx: 0,
                action_menu: None,
            },
            git_log: GitLogState {
                commits: Vec::new(),
                selected_idx: 0,
                view_height: 0,
                ref_name: String::new(),
                graph: Vec::new(),
                detail: SubPaneScroll::default(),
                detail_view_height: 0,
                detail_changed_files: Vec::new(),
            },
            reflog: ReflogState {
                entries: Vec::new(),
                selected_idx: 0,
                view_height: 0,
            },
        };
        state.load_branches();
        state.load_reflog();
        state
            .diff_view
            .highlight
            .spawn_bg_highlight(&state.shared.diff_state.files);
        Ok(state)
    }

    /// Refresh the diff state from the working directory.
    /// Returns `Ok(Some(message))` if a fallback occurred, `Ok(None)` on clean refresh.
    pub fn refresh_diff(&mut self) -> Result<Option<String>> {
        let old_path = self.selected_file().map(|f| f.path.clone());
        let fallback_msg = match self
            .shared
            .repo
            .diff_workdir(self.shared.diff_base_ref.as_deref())
        {
            Ok(state) => {
                self.shared.diff_state = state;
                None
            }
            Err(e) => {
                self.shared.diff_base_ref = None;
                self.shared.diff_state = self.shared.repo.diff_workdir(None)?;
                Some(format!("Invalid ref, fell back to HEAD: {e}"))
            }
        };
        // Preserve selection by path
        if let Some(path) = old_path {
            let entries = self.tree_entries();
            self.file_tree.selected_idx = entries
                .iter()
                .position(|e| matches!(e, TreeEntry::File { file_idx, .. } if self.shared.diff_state.files.get(*file_idx).map(|f| &f.path) == Some(&path)))
                .unwrap_or(0);
        }
        let entries = self.tree_entries();
        if self.file_tree.selected_idx >= entries.len() && !entries.is_empty() {
            self.file_tree.selected_idx = entries.len() - 1;
        }
        self.diff_view.scroll.y = 0;
        self.diff_view.scroll.x = 0;
        self.diff_view.highlight.reset();
        self.shared.search.reset_matches();
        self.diff_view
            .highlight
            .spawn_bg_highlight(&self.shared.diff_state.files);
        Ok(fallback_msg)
    }

    pub fn selected_file(&self) -> Option<&FileDiff> {
        let entries = self.tree_entries();
        if let Some(TreeEntry::File { file_idx, .. }) = entries.get(self.file_tree.selected_idx) {
            self.shared.diff_state.files.get(*file_idx)
        } else {
            None
        }
    }

    pub fn load_branches(&mut self) {
        self.branch_list.branches = self.shared.repo.list_local_branches();
        if self.branch_list.selected_idx >= self.branch_list.branches.len() {
            self.branch_list.selected_idx = 0;
        }
        self.update_branch_log();
    }

    pub fn update_branch_log(&mut self) {
        if let Some(branch) = self.branch_list.branches.get(self.branch_list.selected_idx) {
            self.git_log.ref_name = branch.name.clone();
            self.git_log.commits = self.shared.repo.log_for_ref(&branch.name, 100);
            self.git_log.graph = graph::build_graph(&self.git_log.commits);
            self.git_log.selected_idx = 0;
            self.git_log.detail.reset();
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
            self.git_log.detail_changed_files =
                self.shared.repo.commit_changed_files(&commit.full_hash);
            self.git_log.detail.reset();
        } else {
            self.git_log.detail_changed_files.clear();
        }
    }

    pub fn load_reflog(&mut self) {
        self.reflog.entries = self.shared.repo.reflog(500);
        if self.reflog.selected_idx >= self.reflog.entries.len() {
            self.reflog.selected_idx = 0;
        }
    }

    pub fn tree_entries(&self) -> Vec<TreeEntry> {
        crate::git::domain::tree::build_tree_entries(
            &self.shared.diff_state.files,
            &self.file_tree.collapsed_dirs,
        )
    }
}

impl crate::core::pane::FocusState<FocusedPane> for GitState {
    fn focused_pane(&self) -> FocusedPane {
        self.shared.focused_pane
    }
    fn set_focus(&mut self, id: FocusedPane) {
        self.shared.previous_pane = self.shared.focused_pane;
        self.shared.focused_pane = id;
    }
}

impl crate::core::pane::FocusState<GitLogDetailPane> for GitLogState {
    fn focused_pane(&self) -> GitLogDetailPane {
        GitLogDetailPane::Detail
    }
    fn set_focus(&mut self, _id: GitLogDetailPane) {}
}

impl DetailState for GitLogState {
    type SubPaneId = GitLogDetailPane;
    fn sub_scroll(&self, _id: GitLogDetailPane) -> &SubPaneScroll {
        &self.detail
    }
    fn sub_scroll_mut(&mut self, _id: GitLogDetailPane) -> &mut SubPaneScroll {
        &mut self.detail
    }
    fn detail_view_height(&self) -> u16 {
        self.detail_view_height
    }
    fn set_detail_view_height(&mut self, h: u16) {
        self.detail_view_height = h;
    }
    fn reset_sub_panes(&mut self) {
        self.detail.reset();
    }
}

impl crate::core::app::PageState for GitState {
    fn drain_background(&mut self) {
        self.diff_view.highlight.drain_bg_highlights();
    }
    fn search(&self) -> &crate::core::app::SearchState {
        &self.shared.search
    }
    fn search_mut(&mut self) -> &mut crate::core::app::SearchState {
        &mut self.shared.search
    }
}
