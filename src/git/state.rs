use crate::core::app::AppContext;
use crate::core::config::{Config, LoadedPageConfig};
use crate::core::keymap::{Keymap, ViewAction};
use crate::core::layout::{split_page_frame, PageLayoutConfig};
use crate::core::page::{ExternalCommand, PageAction};
pub use crate::core::pane::PaneEvent;
use crate::core::pane::{self, Pane, PaneSet, PaneShared};
use crate::core::search::SearchState;
use crate::core::tab::Tab;
use crate::core::ui::status_bar;
use crate::git::domain::diff::{DiffMeta, FileDiff};
use crate::git::domain::repository::Repo;
use crate::git::domain::search;
use crate::git::panes::branch_list::BranchListAction;
use crate::git::panes::diff_view::keys::DiffScrollAction;
use crate::git::panes::file_tree::FileTreeAction;
use crate::git::panes::git_log::GitLogAction;
use crate::git::panes::reflog::ReflogAction;
use crate::git::panes::{BranchListPane, DiffViewPane, FileTreePane, GitLogPane, ReflogPane};
use anyhow::Result;
use crossterm::event::KeyEvent;
use ratatui::layout::Rect;
use ratatui::Frame;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

#[cfg(test)]
use crate::core::keymap::view_bindings;
#[cfg(test)]
use crate::core::layout::{LayoutNode, SlotRule, SplitDirection};
#[cfg(test)]
use crossterm::event::KeyCode;
#[cfg(test)]
use ratatui::layout::Constraint;

/// Resolved pane IDs for the git page, built from the KDL config's pane_ids.
pub struct GitPaneIds {
    pub file_tree: usize,
    pub branch_list: usize,
    pub git_log: usize,
    pub reflog: usize,
    pub diff_view: usize,
}

impl GitPaneIds {
    fn from_config(cfg: &LoadedPageConfig) -> Self {
        Self {
            file_tree: cfg.resolve_id_expect("file_tree"),
            branch_list: cfg.resolve_id_expect("branch_list"),
            git_log: cfg.resolve_id_expect("git_log"),
            reflog: cfg.resolve_id_expect("reflog"),
            diff_view: cfg.resolve_id_expect("diff_view"),
        }
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
    pub ids: GitPaneIds,
}

impl pane::PageLayout for GitState {
    type Panes = GitPanes;
    fn page_parts_mut(
        &mut self,
    ) -> (
        &mut PaneShared,
        &mut Self::Panes,
        &crate::core::keymap::Keymap<ViewAction>,
        &PageLayoutConfig,
    ) {
        (
            &mut self.pane,
            &mut self.panes,
            &self.view_keymap,
            &self.layout_config,
        )
    }
}

impl PaneSet for GitPanes {
    fn get_mut(&mut self, idx: usize) -> Option<&mut dyn Pane<PaneEvent>> {
        let (ft, dv, bl, gl, rl) = (
            self.ids.file_tree,
            self.ids.diff_view,
            self.ids.branch_list,
            self.ids.git_log,
            self.ids.reflog,
        );
        if idx == ft || idx == dv {
            self.file_tab.get_pane_mut(ft, dv, idx)
        } else if idx == bl || idx == gl {
            self.branch_tab.get_pane_mut(bl, gl, idx)
        } else if idx == rl {
            Some(&mut self.reflog)
        } else {
            None
        }
    }

    fn find_modal(&mut self) -> Option<usize> {
        if Pane::<PaneEvent>::is_modal(&self.branch_tab.list) {
            Some(self.ids.branch_list)
        } else {
            None
        }
    }
}

pub struct GitState {
    pub pane: PaneShared,
    pub panes: GitPanes,
    pub repo: Repo,
    pub diff_meta: DiffMeta,
    pub diff_base_ref: Option<String>,
    /// select_id → detail_id, built from KDL `bind` declarations at construction time.
    select_bindings: HashMap<usize, usize>,
    layout_config: PageLayoutConfig,
    view_keymap: Keymap<ViewAction>,
}

impl GitState {
    pub fn new(cwd: &Path, cfg: &Config) -> Result<Self> {
        let page_cfg = cfg.git_page()?;
        let theme = cfg.theme()?;

        // Resolve pane IDs from config (declaration order = current 0..4)
        let ids = GitPaneIds::from_config(&page_cfg);
        // Build select→detail dispatch map from KDL `bind` declarations.
        let select_bindings = page_cfg.resolve_select_bindings();

        let repo = Repo::discover(cwd)?;
        let result = repo.diff_workdir(None)?;
        let files = Rc::new(result.files);

        // Build pane keymaps from KDL entries.
        let file_tree_km = page_cfg.keymap::<FileTreeAction>("file_tree")?;
        let branch_list_km = page_cfg.keymap::<BranchListAction>("branch_list")?;
        let git_log_km = page_cfg.keymap::<GitLogAction>("git_log")?;
        let reflog_km = page_cfg.keymap::<ReflogAction>("reflog")?;
        let diff_view_km = page_cfg.keymap::<DiffScrollAction>("diff_view")?;
        let view_km = page_cfg.keymap::<ViewAction>("view")?;

        let mut file_tree = FileTreePane::new(Rc::clone(&files), ids.file_tree, ids.diff_view);
        file_tree.set_keymap(file_tree_km);

        let mut branch_list = BranchListPane::new(ids.branch_list, ids.git_log);
        branch_list.set_keymap(branch_list_km);

        let mut git_log = GitLogPane::new(ids.git_log, ids.reflog, ids.branch_list);
        git_log.set_keymap(git_log_km);

        let mut reflog = ReflogPane::new(ids.reflog, ids.branch_list, ids.git_log);
        reflog.set_keymap(reflog_km);

        let mut diff_view = DiffViewPane::new(Rc::clone(&files), ids.diff_view, &theme);
        diff_view.set_scroll_keymap(diff_view_km);

        let focused = ids.file_tree;
        let mut state = Self {
            pane: PaneShared {
                focused_pane: focused,
                previous_pane: focused,
                search: SearchState::new(),
            },
            panes: GitPanes {
                file_tab: Tab {
                    list: file_tree,
                    detail: diff_view,
                },
                branch_tab: Tab {
                    list: branch_list,
                    detail: git_log,
                },
                reflog,
                ids,
            },
            repo,
            diff_meta: DiffMeta {
                branch_name: result.branch_name,
                stats: result.stats,
                file_count: files.len(),
            },
            diff_base_ref: None,
            select_bindings,
            layout_config: page_cfg.layout,
            view_keymap: view_km,
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
        pane::dispatch_page_key(self, key)
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
                        let gl = self.panes.ids.git_log;
                        let bl = self.panes.ids.branch_list;
                        if origin == gl {
                            self.panes.branch_tab.detail.load_detail(&self.repo);
                        } else if origin == bl {
                            self.panes.branch_tab.sync_detail(&self.repo);
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(PageAction::None)
    }

    /// Synchronize detail pane when the selected item in `selected` pane changes.
    /// Routes via KDL-resolved `select_bindings`; typed dispatch follows from the detail pane ID.
    fn sync_detail(&mut self, selected: usize) {
        let dv = self.panes.ids.diff_view;
        let gl = self.panes.ids.git_log;
        if let Some(&detail) = self.select_bindings.get(&selected) {
            if detail == dv {
                self.panes.file_tab.sync_detail();
                search::re_search_on_file_change(self, dv);
            } else if detail == gl {
                self.panes.branch_tab.sync_detail(&self.repo);
            }
        } else if selected == gl {
            // git_log is itself a detail pane; navigate within it to load commit detail.
            self.panes.branch_tab.detail.load_detail(&self.repo);
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
        let dv = self.panes.ids.diff_view;
        if self.pane.focused_pane == dv && self.panes.file_tab.detail.intercepts_keys() {
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
    fn id(&self) -> &'static str {
        "git"
    }

    fn label(&self) -> &'static str {
        "Git"
    }

    fn help_bindings(&self) -> Vec<(String, String)> {
        use crate::core::keymap::help_section;

        let s = |k: &str, v: &str| (k.to_string(), v.to_string());
        let mut entries = self.view_keymap.help_entries();
        entries.push(s("v / V", "Visual / Visual Line"));
        entries.push(s("y", "Yank (copy) selection"));
        entries.extend(help_section("File Tree"));
        entries.extend(self.panes.file_tab.list.keymap().help_entries());
        entries.extend(help_section("Branch List"));
        entries.extend(self.panes.branch_tab.list.keymap().help_entries());
        entries.extend(help_section("Git Log"));
        entries.extend(self.panes.branch_tab.detail.keymap().help_entries());
        entries.extend(help_section("Reflog"));
        entries.extend(self.panes.reflog.keymap().help_entries());
        entries.extend(help_section("Diff View (Scroll)"));
        entries.extend(self.panes.file_tab.detail.scroll_keymap.help_entries());
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
        pane::render_page_content(self, f, ctx, frame.content);
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

#[cfg(test)]
mod kdl_regression {
    use super::*;
    use crate::core::config::{build_keymap, load_git_page_config};
    use crate::core::keymap::{KeyInput, ViewAction};
    use crate::core::layout::resolve_layout;
    use crate::git::panes::branch_list::{default_keymap as branch_default, BranchListAction};
    use crate::git::panes::diff_view::keys::{default_scroll_keymap, DiffScrollAction};
    use crate::git::panes::file_tree::{default_keymap as file_tree_default, FileTreeAction};
    use crate::git::panes::git_log::{default_keymap as git_log_default, GitLogAction};
    use crate::git::panes::reflog::{default_keymap as reflog_default, ReflogAction};
    use crossterm::event::KeyEvent;
    use ratatui::layout::Rect;

    // Test-local constants matching the expected IDs (pane block declaration order)
    const PANE_FILE_TREE: usize = 0;
    const PANE_BRANCH_LIST: usize = 1;
    const PANE_GIT_LOG: usize = 2;
    const PANE_REFLOG: usize = 3;
    const PANE_DIFF_VIEW: usize = 4;
    const SLOT_MAIN: usize = 0;

    fn key(s: &str) -> KeyEvent {
        let ki: KeyInput = s.parse().unwrap();
        KeyEvent::new(ki.code, ki.modifiers)
    }

    fn kdl_cfg() -> crate::core::config::loader::LoadedPageConfig {
        load_git_page_config().unwrap()
    }

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

    fn default_view_keymap() -> Keymap<ViewAction> {
        Keymap::new()
            .bindings(view_bindings(|v| v))
            .key(KeyCode::Char('e'), ViewAction::OpenEditor)
    }

    // ── Layout regression ─────────────────────────────────────────────────────

    #[test]
    fn layout_tree_structure_matches() {
        let hardcoded = default_layout_config();
        let from_kdl = kdl_cfg().layout;

        let area = Rect::new(0, 0, 200, 60);

        // Compare for focus on file_tree (default slot: diff_view)
        let slots_hc = hardcoded.resolve_slots(PANE_FILE_TREE);
        let slots_kd = from_kdl.resolve_slots(PANE_FILE_TREE);

        let layout_hc = resolve_layout(area, &hardcoded.tree, &slots_hc);
        let layout_kd = resolve_layout(area, &from_kdl.tree, &slots_kd);

        assert_eq!(
            layout_hc, layout_kd,
            "layout resolution differs for file_tree focus"
        );

        // Compare for focus on branch_list (slot → git_log)
        let slots_hc2 = hardcoded.resolve_slots(PANE_BRANCH_LIST);
        let slots_kd2 = from_kdl.resolve_slots(PANE_BRANCH_LIST);
        let layout_hc2 = resolve_layout(area, &hardcoded.tree, &slots_hc2);
        let layout_kd2 = resolve_layout(area, &from_kdl.tree, &slots_kd2);
        assert_eq!(
            layout_hc2, layout_kd2,
            "layout resolution differs for branch_list focus"
        );
    }

    #[test]
    fn tab_panes_match() {
        let hardcoded = default_layout_config();
        let from_kdl = kdl_cfg().layout;
        assert_eq!(hardcoded.tab_panes, from_kdl.tab_panes);
    }

    #[test]
    fn slot_rules_match() {
        let hardcoded = default_layout_config();
        let from_kdl = kdl_cfg().layout;
        assert_eq!(hardcoded.slot_rules.len(), from_kdl.slot_rules.len());
        let r_hc = &hardcoded.slot_rules[0];
        let r_kd = &from_kdl.slot_rules[0];
        assert_eq!(r_hc.slot_id, r_kd.slot_id);
        assert_eq!(r_hc.then_pane, r_kd.then_pane);
        assert_eq!(r_hc.default_pane, r_kd.default_pane);
        let mut tp_hc = r_hc.trigger_panes.clone();
        tp_hc.sort();
        let mut tp_kd = r_kd.trigger_panes.clone();
        tp_kd.sort();
        assert_eq!(tp_hc, tp_kd);
    }

    // ── Keymap regression helpers ─────────────────────────────────────────────

    fn check_keys<A: Clone + std::fmt::Debug>(
        hc: &crate::core::keymap::Keymap<A>,
        kd: &crate::core::keymap::Keymap<A>,
        test_keys: &[&str],
    ) {
        for k in test_keys {
            let ev = key(k);
            let a_hc = hc.lookup(ev);
            let a_kd = kd.lookup(ev);
            assert_eq!(
                a_hc.is_some(),
                a_kd.is_some(),
                "key {k:?}: hardcoded={a_hc:?}, kdl={a_kd:?}"
            );
            if let (Some(h), Some(d)) = (a_hc, a_kd) {
                assert_eq!(
                    format!("{h:?}"),
                    format!("{d:?}"),
                    "key {k:?} action mismatch"
                );
            }
        }
    }

    // ── FileTreeAction ─────────────────────────────────────────────────────────

    #[test]
    fn file_tree_keymap_matches() {
        let hc = file_tree_default();
        let entries = kdl_cfg().pane_keys;
        let kd: crate::core::keymap::Keymap<FileTreeAction> =
            build_keymap(entries["file_tree"].as_slice()).unwrap();
        let test_keys = [
            "j", "k", "G", "g", "Ctrl+d", "Ctrl+u", "/", "n", "N", "Space", "Right", "Enter", "i",
            "Esc", "Down", "Up",
        ];
        check_keys(&hc, &kd, &test_keys);
    }

    // ── BranchListAction ───────────────────────────────────────────────────────

    #[test]
    fn branch_list_keymap_matches() {
        let hc = branch_default();
        let entries = kdl_cfg().pane_keys;
        let kd: crate::core::keymap::Keymap<BranchListAction> =
            build_keymap(entries["branch_list"].as_slice()).unwrap();
        let test_keys = [
            "j", "k", "G", "g", "Ctrl+d", "Ctrl+u", "/", "n", "N", "Enter", "i", "Esc",
        ];
        check_keys(&hc, &kd, &test_keys);
    }

    // ── GitLogAction ───────────────────────────────────────────────────────────

    #[test]
    fn git_log_keymap_matches() {
        let hc = git_log_default();
        let entries = kdl_cfg().pane_keys;
        let kd: crate::core::keymap::Keymap<GitLogAction> =
            build_keymap(entries["git_log"].as_slice()).unwrap();
        let test_keys = [
            "j", "k", "G", "g", "Ctrl+d", "Ctrl+u", "/", "n", "N", "y", "o", "h", "Esc",
        ];
        check_keys(&hc, &kd, &test_keys);
    }

    // ── ReflogAction ───────────────────────────────────────────────────────────

    #[test]
    fn reflog_keymap_matches() {
        let hc = reflog_default();
        let entries = kdl_cfg().pane_keys;
        let kd: crate::core::keymap::Keymap<ReflogAction> =
            build_keymap(entries["reflog"].as_slice()).unwrap();
        let test_keys = [
            "j", "k", "G", "g", "Ctrl+d", "Ctrl+u", "/", "n", "N", "Enter", "i", "Esc",
        ];
        check_keys(&hc, &kd, &test_keys);
    }

    // ── DiffScrollAction ───────────────────────────────────────────────────────

    #[test]
    fn diff_view_keymap_matches() {
        let hc = default_scroll_keymap();
        let entries = kdl_cfg().pane_keys;
        let kd: crate::core::keymap::Keymap<DiffScrollAction> =
            build_keymap(entries["diff_view"].as_slice()).unwrap();
        let test_keys = [
            "j", "k", "G", "g", "Ctrl+d", "Ctrl+u", "/", "n", "N", "h", "Left", "l", "Right", "i",
            "Esc", "Down", "Up",
        ];
        check_keys(&hc, &kd, &test_keys);
    }

    // ── ViewAction (git page) ─────────────────────────────────────────────────

    #[test]
    fn view_keymap_matches() {
        let hc = default_view_keymap();
        let entries = kdl_cfg().pane_keys;
        let kd: crate::core::keymap::Keymap<ViewAction> =
            build_keymap(entries["view"].as_slice()).unwrap();
        let test_keys = ["q", "?", "r", "h", "l", "Tab", "BackTab", "e"];
        check_keys(&hc, &kd, &test_keys);
    }

    // ── Bindings regression ───────────────────────────────────────────────────

    #[test]
    fn bindings_match_expected_pairs() {
        let cfg = kdl_cfg();
        let ids = GitPaneIds::from_config(&cfg);
        // Verify bindings resolve to the correct ID pairs
        let resolved: Vec<(usize, usize)> = cfg
            .bindings
            .iter()
            .filter_map(|(sel, det)| {
                let s = cfg.resolve_id(sel)?;
                let d = cfg.resolve_id(det)?;
                Some((s, d))
            })
            .collect();
        assert!(
            resolved.contains(&(ids.file_tree, ids.diff_view)),
            "binding file_tree→diff_view missing"
        );
        assert!(
            resolved.contains(&(ids.branch_list, ids.git_log)),
            "binding branch_list→git_log missing"
        );
    }

    #[test]
    fn select_bindings_drive_sync_dispatch() {
        let cfg = kdl_cfg();
        let ids = GitPaneIds::from_config(&cfg);
        let bindings = cfg.resolve_select_bindings();
        // file_tree → diff_view
        assert_eq!(
            bindings.get(&ids.file_tree),
            Some(&ids.diff_view),
            "select_bindings must map file_tree→diff_view"
        );
        // branch_list → git_log
        assert_eq!(
            bindings.get(&ids.branch_list),
            Some(&ids.git_log),
            "select_bindings must map branch_list→git_log"
        );
        // Exactly these two bindings exist
        assert_eq!(bindings.len(), 2, "expected exactly 2 select bindings");
    }
}
