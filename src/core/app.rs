use crate::git::repository::Repo;
pub use crate::git::state::*;
pub use crate::core::search::{SearchMatch, SearchOrigin, SearchState};
use crate::github::state::GitHubState;
use crate::core::syntax::SyntaxHighlighter;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Git,
    GitHub,
}

pub struct ErrorDialogState {
    pub title: String,
    pub message: String,
}

pub struct App {
    pub should_quit: bool,
    pub view_mode: ViewMode,
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
            show_help: false,
            status_message: None,
            error_dialog: None,
            git: GitState {
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
        app.git.load_branches();
        app.git.load_reflog();
        app.git.spawn_bg_highlight();
        Ok(app)
    }

    pub fn active_search(&self) -> Option<&SearchState> {
        match self.view_mode {
            ViewMode::Git => Some(&self.git.search),
            ViewMode::GitHub => None,
        }
    }

    pub fn refresh_diff(&mut self) -> Result<()> {
        let old_path = self.git.selected_file().map(|f| f.path.clone());
        match self.git.repo.diff_workdir(self.git.diff_base_ref.as_deref()) {
            Ok(state) => self.git.diff_state = state,
            Err(e) => {
                self.git.diff_base_ref = None;
                self.git.diff_state = self.git.repo.diff_workdir(None)?;
                self.status_message = Some(format!("Invalid ref, fell back to HEAD: {e}"));
            }
        }
        // Preserve selection by path
        if let Some(path) = old_path {
            let entries = self.git.build_tree_entries();
            self.git.selected_tree_idx = entries
                .iter()
                .position(|e| matches!(e, TreeEntry::File { file_idx, .. } if self.git.diff_state.files.get(*file_idx).map(|f| &f.path) == Some(&path)))
                .unwrap_or(0);
        }
        let entries = self.git.build_tree_entries();
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
        self.git.spawn_bg_highlight();
        Ok(())
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
