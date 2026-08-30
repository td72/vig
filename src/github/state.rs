use crate::core::app::AppContext;
use crate::core::config::{Config, LoadedPageConfig};
use crate::core::keymap::{Keymap, ViewAction};
use crate::core::layout::{split_page_frame, PageLayoutConfig};
use crate::core::page::PageAction;
use crate::core::pane::{self, Pane, PaneEvent, PaneSet, PaneShared};
use crate::core::search::SearchState;
use crate::core::tab::Tab;
use crate::core::ui::status_bar;
use crate::github::domain::client;
use crate::github::domain::types::*;
use crate::github::panes::detail_view::{DetailAction, GhDetailViewPane};
use crate::github::panes::gh_list::{GhListAction, GhListItem, GhListPane};
use crate::github::panes::issue_list::{self, GhIssueListPane};
use crate::github::panes::pr_list::{self, GhPrListPane};
use anyhow::Result;
use crossterm::event::KeyEvent;
use ratatui::layout::Rect;
use ratatui::Frame;
use std::collections::HashMap;
use std::sync::mpsc;

// === Pane ID registry ===

pub struct GhPaneIds {
    pub issue_list: usize,
    pub pr_list: usize,
    pub issue_detail: usize,
    pub pr_detail: usize,
}

impl GhPaneIds {
    fn from_config(cfg: &LoadedPageConfig) -> Self {
        Self {
            issue_list: cfg.resolve_id_expect("issue_list"),
            pr_list: cfg.resolve_id_expect("pr_list"),
            issue_detail: cfg.resolve_id_expect("issue_detail"),
            pr_detail: cfg.resolve_id_expect("pr_detail"),
        }
    }
}

#[cfg(test)]
use crate::core::layout::{LayoutNode, SlotRule, SplitDirection};
#[cfg(test)]
use ratatui::layout::Constraint;

#[derive(Debug, Clone)]
pub enum GhDetailContent {
    None,
    Loading { kind: GhDetailKind, number: u64 },
    Issue(Box<GhIssueDetail>),
    Pr(Box<GhPrDetail>),
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GhDetailKind {
    Issue,
    Pr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GhDetailPane {
    Body,
    Status,
    Reviews,
    Comments,
}

pub enum GhBgMessage {
    AuthStatus(Result<(), String>),
    IssueList(Result<Vec<GhIssueListItem>, String>),
    PrList(Result<Vec<GhPrListItem>, String>),
    IssueDetail(Result<GhIssueDetail, String>),
    PrDetail(Result<GhPrDetail, String>),
}

// === Tab type aliases ===

pub type IssueTab = Tab<GhIssueListPane, GhDetailViewPane>;
pub type PrTab = Tab<GhPrListPane, GhDetailViewPane>;

/// Apply a list-fetch result to a `GhListPane` and update the arrived/error flags.
fn apply_list_result<T: GhListItem>(
    list: &mut GhListPane<T>,
    result: Result<Vec<T>, String>,
    arrived: &mut bool,
    gh_error: &mut Option<String>,
) {
    list.set_loading(false);
    match result {
        Ok(items) => {
            list.apply_list(items);
            *arrived = true;
        }
        Err(e) => {
            if gh_error.is_none() {
                *gh_error = Some(e);
            }
        }
    }
}

impl IssueTab {
    /// Sync DetailView to show the selected issue.
    pub fn sync_detail(&mut self, tx: &mpsc::Sender<GhBgMessage>) {
        if let Some(n) = self.list.selected_number() {
            self.detail.load(GhDetailKind::Issue, n, tx);
        }
    }
}

impl PrTab {
    /// Sync DetailView to show the selected PR.
    pub fn sync_detail(&mut self, tx: &mpsc::Sender<GhBgMessage>) {
        if let Some(n) = self.list.selected_number() {
            self.detail.load(GhDetailKind::Pr, n, tx);
        }
    }
}

// === GhPanes (grouping struct for disjoint borrows) ===

pub struct GhPanes {
    pub issue_tab: IssueTab,
    pub pr_tab: PrTab,
    pub ids: GhPaneIds,
}

impl pane::PageLayout for GitHubState {
    type Panes = GhPanes;
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

impl PaneSet for GhPanes {
    fn get_mut(&mut self, idx: usize) -> Option<&mut dyn Pane<PaneEvent>> {
        let (il, id_, pl, pd) = (
            self.ids.issue_list,
            self.ids.issue_detail,
            self.ids.pr_list,
            self.ids.pr_detail,
        );
        if idx == il || idx == id_ {
            self.issue_tab.get_pane_mut(il, id_, idx)
        } else if idx == pl || idx == pd {
            self.pr_tab.get_pane_mut(pl, pd, idx)
        } else {
            None
        }
    }
}

// === GitHubState ===

pub struct GitHubState {
    pub pane: PaneShared,
    pub panes: GhPanes,
    // Page-level
    pub gh_available: Option<bool>,
    pub gh_error: Option<String>,
    bg_rx: Option<mpsc::Receiver<GhBgMessage>>,
    pub(crate) bg_tx: Option<mpsc::Sender<GhBgMessage>>,
    pub initialized: bool,
    /// select_id → detail_id, built from KDL `bind` declarations at construction time.
    select_bindings: HashMap<usize, usize>,
    /// detail_id → select_id (reverse of select_bindings).
    detail_bindings: HashMap<usize, usize>,
    layout_config: PageLayoutConfig,
    view_keymap: Keymap<ViewAction>,
}

impl GitHubState {
    pub fn new(cfg: &Config) -> Result<Self> {
        let page_cfg = cfg.github_page()?;

        let ids = GhPaneIds::from_config(&page_cfg);
        // Build select→detail and reverse detail→select dispatch maps.
        let select_bindings = page_cfg.resolve_select_bindings();
        let detail_bindings: HashMap<usize, usize> =
            select_bindings.iter().map(|(&s, &d)| (d, s)).collect();

        // Build pane keymaps from KDL entries.
        let issue_list_km = page_cfg.keymap::<GhListAction>("issue_list")?;
        let pr_list_km = page_cfg.keymap::<GhListAction>("pr_list")?;
        let issue_detail_km = page_cfg.keymap::<DetailAction>("issue_detail")?;
        let pr_detail_km = page_cfg.keymap::<DetailAction>("pr_detail")?;
        let view_km = page_cfg.keymap::<ViewAction>("view")?;

        let mut issue_list = issue_list::new_pane(ids.issue_list, ids.issue_detail, ids.pr_list);
        issue_list.set_keymap(issue_list_km);

        let mut pr_list = pr_list::new_pane(ids.pr_list, ids.pr_detail, ids.issue_list);
        pr_list.set_keymap(pr_list_km);

        let mut issue_detail = GhDetailViewPane::new(ids.issue_detail);
        issue_detail.set_keymap(issue_detail_km);

        let mut pr_detail = GhDetailViewPane::new(ids.pr_detail);
        pr_detail.set_keymap(pr_detail_km);

        let initial_focus = ids.issue_list;

        Ok(Self {
            pane: PaneShared {
                focused_pane: initial_focus,
                previous_pane: initial_focus,
                search: SearchState::new(),
            },
            panes: GhPanes {
                issue_tab: Tab {
                    list: issue_list,
                    detail: issue_detail,
                },
                pr_tab: Tab {
                    list: pr_list,
                    detail: pr_detail,
                },
                ids,
            },
            gh_available: None,
            gh_error: None,
            bg_rx: None,
            bg_tx: None,
            initialized: false,
            select_bindings,
            detail_bindings,
            layout_config: page_cfg.layout,
            view_keymap: view_km,
        })
    }

    /// Initialize on first switch to GitHub View.
    /// Creates channel and spawns background threads for auth check + list fetch.
    pub fn initialize(&mut self) {
        if self.initialized {
            return;
        }
        self.initialized = true;

        let (tx, rx) = mpsc::channel();
        self.bg_tx = Some(tx.clone());
        self.bg_rx = Some(rx);

        // Auth check (page-level concern)
        let tx_auth = tx.clone();
        std::thread::spawn(move || {
            let auth = client::check_gh_available();
            let _ = tx_auth.send(GhBgMessage::AuthStatus(auth));
        });

        // Each pane loads its disk cache + spawns background fetch
        self.panes.issue_tab.list.initialize(&tx);
        self.panes.pr_tab.list.initialize(&tx);

        // Auto-load detail for the first item from disk cache
        self.sync_active_detail();
    }

    /// Drain background messages from worker threads.
    pub fn drain_bg_messages(&mut self) {
        let messages: Vec<_> = match &self.bg_rx {
            Some(rx) => rx.try_iter().collect(),
            None => return,
        };

        let mut issue_list_arrived = false;
        let mut pr_list_arrived = false;
        for msg in messages {
            match msg {
                GhBgMessage::AuthStatus(result) => match result {
                    Ok(()) => {
                        self.gh_available = Some(true);
                        self.gh_error = None;
                    }
                    Err(e) => {
                        self.gh_available = Some(false);
                        self.gh_error = Some(e);
                        self.panes.issue_tab.list.set_loading(false);
                        self.panes.pr_tab.list.set_loading(false);
                    }
                },
                GhBgMessage::IssueList(result) => {
                    apply_list_result(
                        &mut self.panes.issue_tab.list,
                        result,
                        &mut issue_list_arrived,
                        &mut self.gh_error,
                    );
                }
                GhBgMessage::PrList(result) => {
                    apply_list_result(
                        &mut self.panes.pr_tab.list,
                        result,
                        &mut pr_list_arrived,
                        &mut self.gh_error,
                    );
                }
                GhBgMessage::IssueDetail(result) => match result {
                    Ok(detail) => self.panes.issue_tab.detail.apply_detail(detail),
                    Err(e) => self.panes.issue_tab.detail.set_error(e),
                },
                GhBgMessage::PrDetail(result) => {
                    self.panes.pr_tab.detail.apply_pr_detail_result(result);
                }
            }
        }

        // Auto-load detail when a fresh list arrives for the active tab
        let on_pr = self.is_on_pr_tab();
        if (on_pr && pr_list_arrived) || (!on_pr && issue_list_arrived) {
            self.sync_active_detail();
        }
    }

    /// Is the user currently on the PR tab (list or detail)?
    /// Uses `detail_bindings` to check whether the focused detail pane is paired with `pr_list`.
    fn is_on_pr_tab(&self) -> bool {
        let fp = self.pane.focused_pane;
        let pr_list = self.panes.ids.pr_list;
        fp == pr_list || self.detail_bindings.get(&fp).copied() == Some(pr_list)
    }

    /// The detail pane of the currently active tab (issue or PR).
    fn active_detail(&self) -> &GhDetailViewPane {
        if self.is_on_pr_tab() {
            &self.panes.pr_tab.detail
        } else {
            &self.panes.issue_tab.detail
        }
    }

    fn active_detail_mut(&mut self) -> &mut GhDetailViewPane {
        if self.is_on_pr_tab() {
            &mut self.panes.pr_tab.detail
        } else {
            &mut self.panes.issue_tab.detail
        }
    }

    /// Sync the active tab's detail view.
    /// Routes via KDL-resolved `select_bindings`/`detail_bindings`; dispatches to the
    /// typed tab whose select pane matches the currently focused pane (or its paired select).
    pub fn sync_active_detail(&mut self) {
        let tx = match &self.bg_tx {
            Some(tx) => tx,
            None => return,
        };
        let fp = self.pane.focused_pane;
        // Resolve focused pane to its select pane: fp itself if it's a select pane,
        // or look up the reverse map if fp is a detail pane.
        let select_id = if self.select_bindings.contains_key(&fp) {
            fp
        } else {
            self.detail_bindings.get(&fp).copied().unwrap_or(fp)
        };
        if select_id == self.panes.ids.pr_list {
            self.panes.pr_tab.sync_detail(tx);
        } else {
            self.panes.issue_tab.sync_detail(tx);
        }
    }

    /// Refresh only the currently displayed detail item (cache-bust + re-fetch).
    pub fn refresh_detail(&mut self) {
        match self.active_detail().current_detail_info() {
            None => self.sync_active_detail(),
            Some((kind, number)) => {
                if let Some(tx) = self.bg_tx.clone() {
                    let dv = self.active_detail_mut();
                    dv.invalidate(kind, number);
                    dv.load(kind, number, &tx);
                }
            }
        }
    }

    /// Refresh: re-fetch issue and PR lists, clear caches.
    pub fn refresh(&mut self) {
        self.gh_error = None;
        self.panes.issue_tab.detail.clear_caches();
        self.panes.pr_tab.detail.clear_caches();
        if let Some(tx) = &self.bg_tx {
            self.panes.issue_tab.list.spawn_fetch(tx);
            self.panes.pr_tab.list.spawn_fetch(tx);
        }
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
        // Copy IDs as usize (Copy) before any mutable borrows.
        let issue_detail_id = self.panes.ids.issue_detail;
        let pr_detail_id = self.panes.ids.pr_detail;
        let issue_list_id = self.panes.ids.issue_list;
        let pr_list_id = self.panes.ids.pr_list;

        for event in events {
            if pane::process_common_event(&mut self.pane, ctx, &event) {
                continue;
            }
            match event {
                PaneEvent::SetFocus(id) if id == issue_detail_id || id == pr_detail_id => {
                    self.sync_active_detail();
                }
                PaneEvent::SelectionChanged => {
                    self.sync_active_detail();
                }
                PaneEvent::OpenIssueBrowser(n) => match client::open_issue_in_browser(n) {
                    Ok(()) => {
                        ctx.status_message = Some(format!("Opening issue #{n} in browser..."));
                    }
                    Err(e) => {
                        ctx.status_message = Some(format!("Failed to open browser: {e}"));
                    }
                },
                PaneEvent::OpenPrBrowser(n) => match client::open_pr_in_browser(n) {
                    Ok(()) => {
                        ctx.status_message = Some(format!("Opening PR #{n} in browser..."));
                    }
                    Err(e) => {
                        ctx.status_message = Some(format!("Failed to open browser: {e}"));
                    }
                },
                PaneEvent::JumpToMatch(forward) => {
                    if let Some(origin) =
                        self.pane
                            .jump_to_search_match(&mut self.panes, ctx, forward)
                    {
                        if origin == issue_list_id || origin == pr_list_id {
                            self.sync_active_detail();
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(PageAction::None)
    }

    // === View-level key handling ===

    pub fn handle_view_key(&mut self, ctx: &mut AppContext, key: KeyEvent) -> Result<PageAction> {
        // View-level actions (quit, help, refresh, navigation)
        if let Some(action) = self.view_keymap.lookup(key) {
            if let Some(page_action) = pane::execute_common_view_action(ctx, *action) {
                return Ok(page_action);
            }
            if *action == ViewAction::Refresh {
                let fp = self.pane.focused_pane;
                let is_on_detail =
                    fp == self.panes.ids.issue_detail || fp == self.panes.ids.pr_detail;
                if is_on_detail {
                    self.refresh_detail();
                } else {
                    self.refresh();
                }
                return Ok(PageAction::None);
            }
        }

        let events = self.dispatch_key(key);
        self.process_events(ctx, events)
    }
}

impl crate::core::app::PageState for GitHubState {
    fn id(&self) -> &'static str {
        "github"
    }

    fn label(&self) -> &'static str {
        "GitHub"
    }

    fn help_bindings(&self) -> Vec<(String, String)> {
        use crate::core::keymap::help_section;

        let mut entries = self.view_keymap.help_entries();
        entries.extend(help_section("Issues"));
        entries.extend(self.panes.issue_tab.list.keymap().help_entries());
        entries.extend(help_section("Pull Requests"));
        entries.extend(self.panes.pr_tab.list.keymap().help_entries());
        entries.extend(help_section("Detail View"));
        entries.extend(self.panes.issue_tab.detail.keymap().help_entries());
        entries
    }

    fn handle_key(&mut self, ctx: &mut AppContext, key: KeyEvent) -> Result<PageAction> {
        // Search input mode intercepts all keys
        if self.pane.handle_search_input(&mut self.panes, ctx, key) {
            return Ok(PageAction::None);
        }

        self.handle_view_key(ctx, key)
    }

    fn render(&mut self, f: &mut Frame, ctx: &AppContext, area: Rect) {
        let frame = split_page_frame(area);
        status_bar::render_gh_header(f, ctx, frame.header);
        pane::render_page_content(self, f, ctx, frame.content);
        status_bar::render_gh_status_bar(f, ctx, self, frame.status_bar);
    }

    fn intercepts_all_keys(&self) -> bool {
        self.pane.search.active
    }

    fn on_tick(&mut self, _ctx: &mut AppContext) {
        if let Some(tx) = &self.bg_tx {
            self.panes.pr_tab.detail.handle_watch_tick(tx);
        }
    }

    fn on_activate(&mut self, _ctx: &mut AppContext) {
        self.initialize();
    }

    fn drain_background(&mut self) {
        self.drain_bg_messages();
    }
}

#[cfg(test)]
mod kdl_regression {
    use super::*;
    use crate::core::config::{build_keymap, load_github_page_config};
    use crate::core::keymap::KeyInput;
    use crate::core::layout::resolve_layout;
    use crate::github::panes::detail_view::DetailAction;
    use crate::github::panes::gh_list::GhListAction;
    use crossterm::event::{KeyCode, KeyEvent};
    use ratatui::layout::Rect;

    // Test-local pane ID constants (match default.kdl declaration order)
    const GH_PANE_ISSUE_LIST: usize = 0;
    const GH_PANE_PR_LIST: usize = 1;
    const GH_PANE_ISSUE_DETAIL: usize = 2;
    const GH_PANE_PR_DETAIL: usize = 3;
    const GH_SLOT_DETAIL: usize = 0;

    fn key(s: &str) -> KeyEvent {
        let ki: KeyInput = s.parse().unwrap();
        KeyEvent::new(ki.code, ki.modifiers)
    }

    fn kdl_cfg() -> crate::core::config::loader::LoadedPageConfig {
        load_github_page_config().unwrap()
    }

    fn default_gh_layout_config() -> PageLayoutConfig {
        PageLayoutConfig {
            tree: LayoutNode::Split {
                direction: SplitDirection::Vertical,
                children: vec![
                    (
                        Constraint::Percentage(40),
                        LayoutNode::Split {
                            direction: SplitDirection::Horizontal,
                            children: vec![
                                (
                                    Constraint::Percentage(50),
                                    LayoutNode::Pane(GH_PANE_ISSUE_LIST),
                                ),
                                (
                                    Constraint::Percentage(50),
                                    LayoutNode::Pane(GH_PANE_PR_LIST),
                                ),
                            ],
                        },
                    ),
                    (Constraint::Min(3), LayoutNode::Slot(GH_SLOT_DETAIL)),
                ],
            },
            tab_panes: vec![GH_PANE_ISSUE_LIST, GH_PANE_PR_LIST],
            slot_rules: vec![SlotRule {
                slot_id: GH_SLOT_DETAIL,
                trigger_panes: vec![GH_PANE_PR_LIST, GH_PANE_PR_DETAIL],
                then_pane: GH_PANE_PR_DETAIL,
                default_pane: GH_PANE_ISSUE_DETAIL,
            }],
        }
    }

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

    #[test]
    fn layout_tree_structure_matches() {
        let hardcoded = default_gh_layout_config();
        let from_kdl = kdl_cfg().layout;
        let area = Rect::new(0, 0, 200, 60);
        let slots_hc = hardcoded.resolve_slots(GH_PANE_ISSUE_LIST);
        let slots_kd = from_kdl.resolve_slots(GH_PANE_ISSUE_LIST);
        let layout_hc = resolve_layout(area, &hardcoded.tree, &slots_hc);
        let layout_kd = resolve_layout(area, &from_kdl.tree, &slots_kd);
        assert_eq!(
            layout_hc, layout_kd,
            "layout resolution differs for issue_list focus"
        );
    }

    #[test]
    fn tab_panes_match() {
        let hardcoded = default_gh_layout_config();
        let from_kdl = kdl_cfg().layout;
        assert_eq!(hardcoded.tab_panes, from_kdl.tab_panes);
    }

    #[test]
    fn slot_rules_match() {
        let hardcoded = default_gh_layout_config();
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

    #[test]
    fn issue_list_keymap_matches() {
        use crate::github::panes::gh_list::default_keymap as gh_default;
        let hc = gh_default(KeyCode::Tab);
        let entries = kdl_cfg().pane_keys;
        let kd: crate::core::keymap::Keymap<GhListAction> =
            build_keymap(entries["issue_list"].as_slice()).unwrap();
        let test_keys = [
            "j", "k", "G", "g", "Ctrl+d", "Ctrl+u", "/", "n", "N", "i", "Enter", "Tab", "o", "Esc",
        ];
        check_keys(&hc, &kd, &test_keys);
    }

    #[test]
    fn pr_list_keymap_matches() {
        use crate::github::panes::gh_list::default_keymap as gh_default;
        let hc = gh_default(KeyCode::BackTab);
        let entries = kdl_cfg().pane_keys;
        let kd: crate::core::keymap::Keymap<GhListAction> =
            build_keymap(entries["pr_list"].as_slice()).unwrap();
        let test_keys = [
            "j", "k", "G", "g", "Ctrl+d", "Ctrl+u", "/", "n", "N", "i", "Enter", "BackTab", "o",
            "Esc",
        ];
        check_keys(&hc, &kd, &test_keys);
    }

    #[test]
    fn detail_keymap_matches() {
        use crate::github::panes::detail_view::default_keymap as detail_default;
        let hc = detail_default();
        let entries = kdl_cfg().pane_keys;
        let kd: crate::core::keymap::Keymap<DetailAction> =
            build_keymap(entries["issue_detail"].as_slice()).unwrap();
        let test_keys = [
            "j", "k", "G", "g", "Ctrl+d", "Ctrl+u", "h", "l", "Tab", "BackTab", "w", "o", "Esc",
        ];
        check_keys(&hc, &kd, &test_keys);
    }

    #[test]
    fn pane_ids_from_config() {
        let cfg = kdl_cfg();
        let ids = GhPaneIds::from_config(&cfg);
        assert_eq!(ids.issue_list, GH_PANE_ISSUE_LIST);
        assert_eq!(ids.pr_list, GH_PANE_PR_LIST);
        assert_eq!(ids.issue_detail, GH_PANE_ISSUE_DETAIL);
        assert_eq!(ids.pr_detail, GH_PANE_PR_DETAIL);
    }

    #[test]
    fn bindings_match_expected_pairs() {
        let cfg = kdl_cfg();
        assert!(
            cfg.bindings
                .contains(&("issue_list".to_string(), "issue_detail".to_string())),
            "missing bind issue_list→issue_detail"
        );
        assert!(
            cfg.bindings
                .contains(&("pr_list".to_string(), "pr_detail".to_string())),
            "missing bind pr_list→pr_detail"
        );
        let issue_detail_id = cfg.resolve_id("issue_detail").unwrap();
        let pr_detail_id = cfg.resolve_id("pr_detail").unwrap();
        assert_ne!(
            issue_detail_id, pr_detail_id,
            "issue_detail and pr_detail must be distinct instances"
        );
    }

    #[test]
    fn gh_select_bindings_drive_sync_dispatch() {
        let cfg = kdl_cfg();
        let ids = GhPaneIds::from_config(&cfg);
        let select_bindings = cfg.resolve_select_bindings();
        // issue_list → issue_detail
        assert_eq!(
            select_bindings.get(&ids.issue_list),
            Some(&ids.issue_detail),
            "select_bindings must map issue_list→issue_detail"
        );
        // pr_list → pr_detail
        assert_eq!(
            select_bindings.get(&ids.pr_list),
            Some(&ids.pr_detail),
            "select_bindings must map pr_list→pr_detail"
        );
        assert_eq!(
            select_bindings.len(),
            2,
            "expected exactly 2 select bindings"
        );
        // Verify reverse (detail_bindings)
        let detail_bindings: HashMap<usize, usize> =
            select_bindings.iter().map(|(&s, &d)| (d, s)).collect();
        assert_eq!(
            detail_bindings.get(&ids.issue_detail),
            Some(&ids.issue_list),
            "detail_bindings must map issue_detail→issue_list"
        );
        assert_eq!(
            detail_bindings.get(&ids.pr_detail),
            Some(&ids.pr_list),
            "detail_bindings must map pr_detail→pr_list"
        );
    }
}
