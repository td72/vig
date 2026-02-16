use crate::core::app::SearchState;
use crate::core::syntax::{HighlightCache, SyntaxHighlighter};
use crate::git::diff::DiffState;
use crate::git::graph::GraphRow;
use crate::git::repository::{BranchInfo, CommitFileChange, CommitInfo, ReflogEntry};
use ratatui::style::Color;
use std::collections::{HashMap, HashSet};
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

pub struct GitState {
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
    pub(crate) bg_highlights: HashMap<String, (Vec<Vec<Color>>, Vec<Vec<Color>>)>,
    /// Receiver for background highlight results.
    pub(crate) bg_highlight_rx: Option<mpsc::Receiver<(String, Vec<Vec<Color>>, Vec<Vec<Color>>)>>,
    pub diff_base_ref: Option<String>,
    pub branch_list: BranchListState,
    pub git_log: GitLogState,
    pub reflog: ReflogState,
    pub branch_action_menu: Option<BranchActionMenuState>,
    pub search: SearchState,
}

impl GitState {
    pub(crate) fn set_focus(&mut self, pane: FocusedPane) {
        self.previous_pane = self.focused_pane;
        self.focused_pane = pane;
    }
}
