use crate::core::app::{AppContext, ErrorDialogState};
use crate::core::page::{ExternalCommand, PageAction};
pub use crate::core::pane::PaneEvent;
use crate::core::pane::{self, Pane, PaneSet, PaneShared};
use crate::core::search::SearchState;
use crate::core::syntax::{HighlightCache, HighlightPair, SyntaxHighlighter};
use crate::core::ui::status_bar;
use crate::git::domain::diff::{DiffMeta, FileDiff};
use crate::git::domain::repository::Repo;
use crate::git::domain::search;
use crate::git::layout;
use crate::git::panes::{BranchListPane, DiffViewPane, FileTreePane, GitLogPane, ReflogPane};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{layout::Rect, Frame};
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;
use std::sync::mpsc;

pub const PANE_FILE_TREE: usize = 0;
pub const PANE_BRANCH_LIST: usize = 1;
pub const PANE_GIT_LOG: usize = 2;
pub const PANE_REFLOG: usize = 3;
pub const PANE_DIFF_VIEW: usize = 4;

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

pub use crate::core::search::DiffSide;

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

// === Tab structs (list + detail pairing) ===

/// FileTree (list) + DiffView (detail)
pub struct FileTab {
    pub file_tree: FileTreePane,
    pub diff_view: DiffViewPane,
}

impl FileTab {
    /// Sync DiffView to match FileTree selection.
    pub fn sync_detail(&mut self) {
        self.diff_view.set_file(self.file_tree.selected_file_idx());
        self.diff_view.reset_scroll();
    }
}

/// BranchList (list) + GitLog (detail)
pub struct BranchTab {
    pub branch_list: BranchListPane,
    pub git_log: GitLogPane,
}

impl BranchTab {
    /// Sync GitLog to show commits for the selected branch.
    pub fn sync_detail(&mut self, repo: &Repo) {
        if let Some(branch) = self.branch_list.selected_branch() {
            let name = branch.name.clone();
            self.git_log.load_for_ref(repo, &name);
        } else {
            self.git_log.clear_log();
        }
    }
}

// === GitPanes (grouping struct for disjoint borrows) ===

pub struct GitPanes {
    pub file_tab: FileTab,
    pub branch_tab: BranchTab,
    pub reflog: ReflogPane,
}

impl PaneSet for GitPanes {
    fn get_mut(&mut self, idx: usize) -> Option<&mut dyn Pane<PaneEvent>> {
        match idx {
            PANE_FILE_TREE => Some(&mut self.file_tab.file_tree),
            PANE_BRANCH_LIST => Some(&mut self.branch_tab.branch_list),
            PANE_GIT_LOG => Some(&mut self.branch_tab.git_log),
            PANE_REFLOG => Some(&mut self.reflog),
            PANE_DIFF_VIEW => Some(&mut self.file_tab.diff_view),
            _ => None,
        }
    }
}

pub struct GitState {
    pub pane: PaneShared,
    pub panes: GitPanes,
    pub repo: Repo,
    pub diff_meta: DiffMeta,
    pub diff_base_ref: Option<String>,
}

impl GitState {
    pub fn new(cwd: &Path) -> Result<Self> {
        let repo = Repo::discover(cwd)?;
        let result = repo.diff_workdir(None)?;
        let files = Rc::new(result.files);
        let mut state = Self {
            pane: PaneShared {
                focused_pane: PANE_FILE_TREE,
                previous_pane: PANE_FILE_TREE,
                search: SearchState::new(),
            },
            panes: GitPanes {
                file_tab: FileTab {
                    file_tree: FileTreePane::new(Rc::clone(&files)),
                    diff_view: DiffViewPane::new(Rc::clone(&files)),
                },
                branch_tab: BranchTab {
                    branch_list: BranchListPane::new(),
                    git_log: GitLogPane::new(),
                },
                reflog: ReflogPane::new(),
            },
            repo,
            diff_meta: DiffMeta {
                branch_name: result.branch_name,
                stats: result.stats,
                file_count: files.len(),
            },
            diff_base_ref: None,
        };
        state.panes.file_tab.diff_view.current_file_idx =
            state.panes.file_tab.file_tree.selected_file_idx();
        state.load_branches();
        state.load_reflog();
        state
            .panes
            .file_tab
            .diff_view
            .highlight
            .spawn_bg_highlight(&files);
        Ok(state)
    }

    /// Refresh the diff state from the working directory.
    /// Returns `Ok(Some(message))` if a fallback occurred, `Ok(None)` on clean refresh.
    pub fn refresh_diff(&mut self) -> Result<Option<String>> {
        let old_path = self.selected_file().map(|f| f.path.clone());
        let fallback_msg = match self.repo.diff_workdir(self.diff_base_ref.as_deref()) {
            Ok(result) => {
                let files = Rc::new(result.files);
                self.diff_meta = DiffMeta {
                    branch_name: result.branch_name,
                    stats: result.stats,
                    file_count: files.len(),
                };
                self.panes.file_tab.file_tree.set_files(Rc::clone(&files));
                self.panes.file_tab.diff_view.set_files(Rc::clone(&files));
                None
            }
            Err(e) => {
                self.diff_base_ref = None;
                let result = self.repo.diff_workdir(None)?;
                let files = Rc::new(result.files);
                self.diff_meta = DiffMeta {
                    branch_name: result.branch_name,
                    stats: result.stats,
                    file_count: files.len(),
                };
                self.panes.file_tab.file_tree.set_files(Rc::clone(&files));
                self.panes.file_tab.diff_view.set_files(Rc::clone(&files));
                Some(format!("Invalid ref, fell back to HEAD: {e}"))
            }
        };
        // Preserve selection by path
        if let Some(path) = old_path {
            let entries = self.tree_entries();
            self.panes.file_tab.file_tree.selected_idx = entries
                .iter()
                .position(|e| matches!(e, TreeEntry::File { file_idx, .. } if self.panes.file_tab.file_tree.files.get(*file_idx).map(|f| &f.path) == Some(&path)))
                .unwrap_or(0);
        }
        let entries = self.tree_entries();
        if self.panes.file_tab.file_tree.selected_idx >= entries.len() && !entries.is_empty() {
            self.panes.file_tab.file_tree.selected_idx = entries.len() - 1;
        }
        self.panes.file_tab.diff_view.current_file_idx =
            self.panes.file_tab.file_tree.selected_file_idx();
        self.panes.file_tab.diff_view.scroll.y = 0;
        self.panes.file_tab.diff_view.scroll.x = 0;
        self.panes.file_tab.diff_view.highlight.reset();
        self.pane.search.reset_matches();
        self.panes
            .file_tab
            .diff_view
            .highlight
            .spawn_bg_highlight(&self.panes.file_tab.file_tree.files);
        Ok(fallback_msg)
    }

    pub fn selected_file(&self) -> Option<&FileDiff> {
        self.panes.file_tab.file_tree.selected_file()
    }

    pub fn tree_entries(&self) -> Vec<TreeEntry> {
        self.panes.file_tab.file_tree.tree_entries()
    }

    pub fn load_branches(&mut self) {
        self.panes.branch_tab.branch_list.load(&self.repo);
        self.panes.branch_tab.sync_detail(&self.repo);
    }

    pub fn load_reflog(&mut self) {
        self.panes.reflog.load(&self.repo);
    }

    const TAB_PANES: [usize; 3] = [PANE_FILE_TREE, PANE_BRANCH_LIST, PANE_REFLOG];

    fn is_commit_log_detail(focused: usize) -> bool {
        matches!(focused, PANE_BRANCH_LIST | PANE_REFLOG | PANE_GIT_LOG)
    }

    // === Dispatch ===

    pub fn dispatch_key(&mut self, key: KeyEvent) -> Vec<PaneEvent> {
        // h/l tab navigation
        if let Some(events) = self.pane.dispatch_tab_key(&Self::TAB_PANES, key) {
            return events;
        }

        // Tab/BackTab: cycle tab panes (works from any pane)
        match key.code {
            KeyCode::Tab => {
                return vec![PaneEvent::SetFocus(self.pane.next_tab_id(&Self::TAB_PANES))];
            }
            KeyCode::BackTab => {
                return vec![PaneEvent::SetFocus(self.pane.prev_tab_id(&Self::TAB_PANES))];
            }
            _ => {}
        }

        // Per-pane delegation
        self.pane.dispatch_to_pane(&mut self.panes, key)
    }

    // === Event processing ===

    pub fn process_events(
        &mut self,
        ctx: &mut AppContext,
        events: Vec<PaneEvent>,
    ) -> Result<PageAction> {
        for event in events {
            if pane::process_common_event(&mut self.pane, ctx, &event) {
                continue;
            }
            match event {
                PaneEvent::SelectionChanged => {
                    self.sync_detail(self.pane.focused_pane);
                }
                PaneEvent::SetDiffBase(base) => {
                    self.diff_base_ref = base;
                    self.apply_refresh(ctx);
                }
                PaneEvent::SwitchBranch(name) => match self.repo.switch_branch(&name) {
                    Ok(()) => {
                        ctx.status_message = Some(format!("Switched to {name}"));
                        self.load_branches();
                        self.apply_refresh(ctx);
                    }
                    Err(e) => {
                        ctx.error_dialog = Some(ErrorDialogState {
                            title: "Switch failed".to_string(),
                            message: format!("{e}"),
                        });
                    }
                },
                PaneEvent::DeleteBranch(name) => match self.repo.delete_branch(&name) {
                    Ok(()) => {
                        ctx.status_message = Some(format!("Deleted {name}"));
                        self.load_branches();
                    }
                    Err(e) => {
                        ctx.error_dialog = Some(ErrorDialogState {
                            title: "Delete failed".to_string(),
                            message: format!("{e}"),
                        });
                    }
                },
                PaneEvent::JumpToMatch(forward) => {
                    if let Some(origin) =
                        self.pane
                            .jump_to_search_match(&mut self.panes, ctx, forward)
                    {
                        match origin {
                            PANE_GIT_LOG => self.panes.branch_tab.git_log.load_detail(&self.repo),
                            PANE_BRANCH_LIST => self.panes.branch_tab.sync_detail(&self.repo),
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(PageAction::None)
    }

    /// Synchronize detail pane when the selected item in `selected` pane changes.
    fn sync_detail(&mut self, selected: usize) {
        match selected {
            PANE_FILE_TREE => {
                self.panes.file_tab.sync_detail();
                search::re_search_on_file_change(self);
            }
            PANE_BRANCH_LIST => {
                self.panes.branch_tab.sync_detail(&self.repo);
            }
            PANE_GIT_LOG => {
                self.panes.branch_tab.git_log.load_detail(&self.repo);
            }
            _ => {}
        }
    }

    // === Refresh helper ===

    pub fn apply_refresh(&mut self, ctx: &mut AppContext) {
        if let Some(msg) = self
            .refresh_diff()
            .unwrap_or_else(|e| Some(format!("Diff error: {e}")))
        {
            ctx.status_message = Some(msg);
        } else {
            ctx.status_message = None;
        }
    }

    // === View-level key handling ===

    pub fn handle_view_key(&mut self, ctx: &mut AppContext, key: KeyEvent) -> Result<PageAction> {
        // In Normal/Visual modes, keys are handled by the mode handler exclusively
        if self.pane.focused_pane == PANE_DIFF_VIEW
            && self.panes.file_tab.diff_view.vim.mode != DiffViewMode::Scroll
        {
            let events = self.dispatch_key(key);
            return self.process_events(ctx, events);
        }

        if let Some(action) = pane::handle_common_view_key(ctx, key) {
            return Ok(action);
        }

        match key.code {
            KeyCode::Char('r') => {
                self.apply_refresh(ctx);
                self.load_branches();
                self.load_reflog();
            }
            KeyCode::Char('e') => {
                if let Some(file) = self.selected_file() {
                    let file_path = ctx.workdir.join(&file.path);
                    let editor = std::env::var("EDITOR")
                        .or_else(|_| std::env::var("VISUAL"))
                        .unwrap_or_else(|_| "vi".to_string());
                    return Ok(PageAction::Suspend(ExternalCommand {
                        program: editor,
                        args: vec![file_path.into()],
                    }));
                }
            }
            _ => {
                let events = self.dispatch_key(key);
                return self.process_events(ctx, events);
            }
        }
        Ok(PageAction::None)
    }
}

impl crate::core::app::PageState for GitState {
    fn label(&self) -> &'static str {
        "Git"
    }

    fn help_bindings(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            ("1 / 2", "Switch view"),
            ("j / ↓", "Next item / Scroll down"),
            ("k / ↑", "Prev item / Scroll up"),
            ("Enter", "Select file/branch"),
            ("Tab", "Next pane"),
            ("Shift+Tab", "Prev pane"),
            ("Ctrl+d", "Half page down"),
            ("Ctrl+u", "Half page up"),
            ("g / G", "Top / Bottom"),
            ("h / l", "Scroll left / right"),
            ("i", "Normal mode (cursor)"),
            ("v / V", "Visual / Visual Line"),
            ("y", "Yank (copy) selection"),
            ("/", "Search"),
            ("n / N", "Next / Prev match"),
            ("Esc", "Clear search / Back"),
            ("e", "Open in $EDITOR"),
            ("r", "Refresh diff + branches"),
            ("?", "Toggle help"),
            ("q", "Quit"),
            ("", ""),
            ("", "── Branch List ──"),
            ("/", "Search branches"),
            ("Enter", "Action menu"),
            ("", ""),
            ("", "── Git Log ──"),
            ("j / k", "Navigate commits"),
            ("Ctrl+d/u", "Half page scroll"),
            ("g / G", "Top / Bottom"),
            ("y", "Copy commit hash"),
            ("o", "Open in GitHub"),
            ("/", "Search commits"),
            ("", ""),
            ("", "── Reflog ──"),
            ("j / k", "Navigate entries"),
            ("Ctrl+d/u", "Half page scroll"),
            ("g / G", "Top / Bottom"),
            ("Enter", "Set as diff base"),
            ("/", "Search reflog"),
        ]
    }

    fn handle_key(&mut self, ctx: &mut AppContext, key: KeyEvent) -> Result<PageAction> {
        // Branch action menu intercepts all keys when open
        if Pane::<PaneEvent>::is_modal(&self.panes.branch_tab.branch_list) {
            let events = self
                .panes
                .branch_tab
                .branch_list
                .handle_key(&self.pane, key);
            return self.process_events(ctx, events);
        }

        // Search input mode intercepts all keys
        if self.pane.search.active {
            if self.pane.search.handle_input_key(key) {
                self.pane.execute_search(&mut self.panes);
                self.pane.jump_to_search_match(&mut self.panes, ctx, true);
            }
            return Ok(PageAction::None);
        }

        self.handle_view_key(ctx, key)
    }

    fn render(&mut self, f: &mut Frame, ctx: &AppContext, area: Rect) {
        let ly = layout::compute_layout(area);
        status_bar::render_header(f, ctx, self, ly.header);

        let main_idx = if Self::is_commit_log_detail(self.pane.focused_pane) {
            PANE_GIT_LOG
        } else {
            PANE_DIFF_VIEW
        };
        self.pane
            .render_panes(&mut self.panes, f, ctx, &ly.pane_areas(main_idx));

        status_bar::render_status_bar(f, ctx, self, ly.status_bar);

        if self.panes.branch_tab.branch_list.action_menu.is_some() {
            self.panes
                .branch_tab
                .branch_list
                .render_action_menu(f, area);
        }
    }

    fn intercepts_all_keys(&self) -> bool {
        self.pane.search.active || Pane::<PaneEvent>::is_modal(&self.panes.branch_tab.branch_list)
    }

    fn on_fs_change(&mut self, ctx: &mut AppContext) -> Result<()> {
        self.load_branches();
        self.load_reflog();
        self.apply_refresh(ctx);
        Ok(())
    }

    fn on_suspend_return(
        &mut self,
        ctx: &mut AppContext,
        status: std::io::Result<std::process::ExitStatus>,
    ) -> Result<()> {
        match status {
            Ok(s) if s.success() => {
                self.apply_refresh(ctx);
                Ok(())
            }
            Ok(s) => {
                ctx.status_message = Some(format!("Editor exited with: {s}"));
                Ok(())
            }
            Err(e) => {
                ctx.status_message = Some(format!("Failed to open editor: {e}"));
                Ok(())
            }
        }
    }

    fn drain_background(&mut self) {
        self.panes.file_tab.diff_view.drain_background();
    }
}
