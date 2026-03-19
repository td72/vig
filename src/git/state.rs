use crate::core::app::AppContext;
use crate::core::keymap::{view_bindings, Keymap, ViewAction};
use crate::core::layout::{
    resolve_layout, split_page_frame, LayoutNode, PageLayoutConfig, SlotRule, SplitDirection,
};
use crate::core::page::{ExternalCommand, PageAction};
pub use crate::core::pane::PaneEvent;
use crate::core::pane::{self, Pane, PaneSet, PaneShared};
use crate::core::search::SearchState;
use crate::core::tab::Tab;
use crate::core::ui::status_bar;
use crate::git::domain::diff::{DiffMeta, FileDiff};
use crate::git::domain::repository::Repo;
use crate::git::domain::search;
use crate::git::panes::{BranchListPane, DiffViewPane, FileTreePane, GitLogPane, ReflogPane};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Rect};
use ratatui::Frame;
use std::path::Path;
use std::rc::Rc;

pub const PANE_FILE_TREE: usize = 0;
pub const PANE_BRANCH_LIST: usize = 1;
pub const PANE_GIT_LOG: usize = 2;
pub const PANE_REFLOG: usize = 3;
pub const PANE_DIFF_VIEW: usize = 4;

const SLOT_MAIN: usize = 0;

fn default_layout_config() -> PageLayoutConfig {
    PageLayoutConfig {
        tree: LayoutNode::Split {
            direction: SplitDirection::Vertical,
            children: vec![
                (
                    Constraint::Percentage(40),
                    LayoutNode::Split {
                        direction: SplitDirection::Horizontal,
                        children: vec![
                            (Constraint::Length(30), LayoutNode::Pane(PANE_FILE_TREE)),
                            (
                                Constraint::Percentage(35),
                                LayoutNode::Pane(PANE_BRANCH_LIST),
                            ),
                            (Constraint::Min(20), LayoutNode::Pane(PANE_REFLOG)),
                        ],
                    },
                ),
                (Constraint::Min(3), LayoutNode::Slot(SLOT_MAIN)),
            ],
        },
        tab_panes: vec![PANE_FILE_TREE, PANE_BRANCH_LIST, PANE_REFLOG],
        slot_rules: vec![SlotRule {
            slot_id: SLOT_MAIN,
            trigger_panes: vec![PANE_BRANCH_LIST, PANE_REFLOG, PANE_GIT_LOG],
            then_pane: PANE_GIT_LOG,
            default_pane: PANE_DIFF_VIEW,
        }],
    }
}

// === Tab type aliases ===

pub type FileTab = Tab<FileTreePane, DiffViewPane>;
pub type BranchTab = Tab<BranchListPane, GitLogPane>;

impl FileTab {
    /// Sync DiffView to match FileTree selection.
    pub fn sync_detail(&mut self) {
        self.detail.set_file(self.list.selected_file_idx());
        self.detail.reset_scroll();
    }

    /// Called after the file list has been replaced (refresh_diff).
    /// Restores selection, resets DiffView, and spawns background highlighting.
    pub fn on_files_changed(&mut self, old_path: Option<String>) {
        self.list.restore_selection(old_path);
        self.detail.reset_to_file(self.list.selected_file_idx());
        let file_data = self.list.highlight_file_data();
        self.detail.spawn_highlight(file_data);
    }
}

impl BranchTab {
    /// Sync GitLog to show commits for the selected branch.
    pub fn sync_detail(&mut self, repo: &Repo) {
        if let Some(branch) = self.list.selected_branch() {
            let name = branch.name.clone();
            self.detail.load_for_ref(repo, &name);
        } else {
            self.detail.clear_log();
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
            PANE_FILE_TREE | PANE_DIFF_VIEW => {
                self.file_tab
                    .get_pane_mut(PANE_FILE_TREE, PANE_DIFF_VIEW, idx)
            }
            PANE_BRANCH_LIST | PANE_GIT_LOG => {
                self.branch_tab
                    .get_pane_mut(PANE_BRANCH_LIST, PANE_GIT_LOG, idx)
            }
            PANE_REFLOG => Some(&mut self.reflog),
            _ => None,
        }
    }

    fn find_modal(&mut self) -> Option<usize> {
        if Pane::<PaneEvent>::is_modal(&self.branch_tab.list) {
            Some(PANE_BRANCH_LIST)
        } else {
            None
        }
    }
}

fn default_view_keymap() -> Keymap<ViewAction> {
    Keymap::new()
        .bindings(view_bindings(|v| v))
        .key(KeyCode::Char('e'), ViewAction::OpenEditor)
}

pub struct GitState {
    pub pane: PaneShared,
    pub panes: GitPanes,
    pub repo: Repo,
    pub diff_meta: DiffMeta,
    pub diff_base_ref: Option<String>,
    layout_config: PageLayoutConfig,
    view_keymap: Keymap<ViewAction>,
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
                file_tab: Tab {
                    list: FileTreePane::new(Rc::clone(&files)),
                    detail: DiffViewPane::new(Rc::clone(&files)),
                },
                branch_tab: Tab {
                    list: BranchListPane::new(),
                    detail: GitLogPane::new(),
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
            layout_config: default_layout_config(),
            view_keymap: default_view_keymap(),
        };
        state.panes.file_tab.on_files_changed(None);
        state.load_branches();
        state.load_reflog();
        Ok(state)
    }

    /// Apply a diff result: update metadata and propagate files to panes.
    fn apply_diff_result(&mut self, result: crate::git::domain::diff::DiffResult) {
        let files = Rc::new(result.files);
        self.diff_meta = DiffMeta {
            branch_name: result.branch_name,
            stats: result.stats,
            file_count: files.len(),
        };
        self.panes.file_tab.list.set_files(Rc::clone(&files));
        self.panes.file_tab.detail.set_files(files);
    }

    /// Refresh the diff state from the working directory.
    /// Returns `Ok(Some(message))` if a fallback occurred, `Ok(None)` on clean refresh.
    pub fn refresh_diff(&mut self) -> Result<Option<String>> {
        let old_path = self.selected_file().map(|f| f.path.clone());
        let fallback_msg = match self.repo.diff_workdir(self.diff_base_ref.as_deref()) {
            Ok(result) => {
                self.apply_diff_result(result);
                None
            }
            Err(e) => {
                self.diff_base_ref = None;
                let result = self.repo.diff_workdir(None)?;
                self.apply_diff_result(result);
                Some(format!("Invalid ref, fell back to HEAD: {e}"))
            }
        };
        self.panes.file_tab.on_files_changed(old_path);
        self.pane.search.reset_matches();
        Ok(fallback_msg)
    }

    pub fn selected_file(&self) -> Option<&FileDiff> {
        self.panes.file_tab.list.selected_file()
    }

    pub fn load_branches(&mut self) {
        self.panes.branch_tab.list.load(&self.repo);
        self.panes.branch_tab.sync_detail(&self.repo);
    }

    pub fn load_reflog(&mut self) {
        self.panes.reflog.load(&self.repo);
    }

    // === Dispatch ===

    pub fn dispatch_key(&mut self, key: KeyEvent) -> Vec<PaneEvent> {
        self.pane.dispatch_key(
            &mut self.panes,
            &self.view_keymap,
            &self.layout_config.tab_panes,
            key,
        )
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
                    Err(e) => ctx.show_error("Switch failed", format!("{e}")),
                },
                PaneEvent::DeleteBranch(name) => match self.repo.delete_branch(&name) {
                    Ok(()) => {
                        ctx.status_message = Some(format!("Deleted {name}"));
                        self.load_branches();
                    }
                    Err(e) => ctx.show_error("Delete failed", format!("{e}")),
                },
                PaneEvent::JumpToMatch(forward) => {
                    if let Some(origin) =
                        self.pane
                            .jump_to_search_match(&mut self.panes, ctx, forward)
                    {
                        match origin {
                            PANE_GIT_LOG => self.panes.branch_tab.detail.load_detail(&self.repo),
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
                search::re_search_on_file_change(self, PANE_DIFF_VIEW);
            }
            PANE_BRANCH_LIST => {
                self.panes.branch_tab.sync_detail(&self.repo);
            }
            PANE_GIT_LOG => {
                self.panes.branch_tab.detail.load_detail(&self.repo);
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
        if self.pane.focused_pane == PANE_DIFF_VIEW && self.panes.file_tab.detail.intercepts_keys()
        {
            let events = self.dispatch_key(key);
            return self.process_events(ctx, events);
        }

        // View-level actions (quit, help, refresh, editor, navigation)
        if let Some(action) = self.view_keymap.lookup(key) {
            if let Some(page_action) = pane::execute_common_view_action(ctx, *action) {
                return Ok(page_action);
            }
            match action {
                ViewAction::Refresh => {
                    self.apply_refresh(ctx);
                    self.load_branches();
                    self.load_reflog();
                    return Ok(PageAction::None);
                }
                ViewAction::OpenEditor => {
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
                    return Ok(PageAction::None);
                }
                _ => {} // Navigation actions handled in dispatch_key
            }
        }

        let events = self.dispatch_key(key);
        self.process_events(ctx, events)
    }
}

impl crate::core::app::PageState for GitState {
    fn label(&self) -> &'static str {
        "Git"
    }

    fn help_bindings(&self) -> Vec<(String, String)> {
        use crate::core::keymap::help_section;
        use crate::git::panes::branch_list;
        use crate::git::panes::diff_view::keys;
        use crate::git::panes::file_tree;
        use crate::git::panes::git_log;
        use crate::git::panes::reflog;

        let s = |k: &str, v: &str| (k.to_string(), v.to_string());
        let mut entries = vec![
            s("1 / 2", "Switch view"),
            s("Tab", "Next pane"),
            s("S-Tab", "Prev pane"),
            s("v / V", "Visual / Visual Line"),
            s("y", "Yank (copy) selection"),
            s("e", "Open in $EDITOR"),
            s("r", "Refresh diff + branches"),
            s("?", "Toggle help"),
            s("q", "Quit"),
        ];
        entries.extend(help_section("File Tree"));
        entries.extend(file_tree::default_keymap().help_entries());
        entries.extend(help_section("Branch List"));
        entries.extend(branch_list::default_keymap().help_entries());
        entries.extend(help_section("Git Log"));
        entries.extend(git_log::default_keymap().help_entries());
        entries.extend(help_section("Reflog"));
        entries.extend(reflog::default_keymap().help_entries());
        entries.extend(help_section("Diff View (Scroll)"));
        entries.extend(keys::default_scroll_keymap().help_entries());
        entries
    }

    fn handle_key(&mut self, ctx: &mut AppContext, key: KeyEvent) -> Result<PageAction> {
        // Modal pane (e.g. action menu) intercepts all keys when open
        if self.panes.find_modal().is_some() {
            let events = self.dispatch_key(key);
            return self.process_events(ctx, events);
        }

        // Search input mode intercepts all keys
        if self.pane.handle_search_input(&mut self.panes, ctx, key) {
            return Ok(PageAction::None);
        }

        self.handle_view_key(ctx, key)
    }

    fn render(&mut self, f: &mut Frame, ctx: &AppContext, area: Rect) {
        let frame = split_page_frame(area);
        status_bar::render_header(f, ctx, self, frame.header);

        let slots = self.layout_config.resolve_slots(self.pane.focused_pane);
        let areas = resolve_layout(frame.content, &self.layout_config.tree, &slots);
        self.pane.render_panes(&mut self.panes, f, ctx, &areas);

        status_bar::render_status_bar(f, ctx, self, frame.status_bar);

        if self.panes.branch_tab.list.is_modal() {
            self.panes.branch_tab.list.render_action_menu(f, area);
        }
    }

    fn intercepts_all_keys(&self) -> bool {
        self.pane.search.active || self.panes.branch_tab.list.is_modal()
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
        self.panes.file_tab.detail.drain_background();
    }
}
