use crate::core::app::AppContext;
use crate::core::keymap::{view_bindings, Keymap, ViewAction};
use crate::core::layout::{
    resolve_layout, split_page_frame, LayoutNode, PageLayoutConfig, SlotRule, SplitDirection,
};
use crate::core::page::PageAction;
use crate::core::pane::{self, Pane, PaneEvent, PaneSet, PaneShared};
use crate::core::search::SearchState;
use crate::core::tab::Tab;
use crate::core::ui::status_bar;
use crate::github::domain::client;
use crate::github::domain::types::*;
use crate::github::panes::detail_view::GhDetailViewPane;
use crate::github::panes::gh_list::{GhListItem, GhListPane};
use crate::github::panes::issue_list::{self, GhIssueListPane};
use crate::github::panes::pr_list::{self, GhPrListPane};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Rect};
use ratatui::Frame;
use std::sync::mpsc;

pub const GH_PANE_ISSUE_LIST: usize = 0;
pub const GH_PANE_PR_LIST: usize = 1;
pub const GH_PANE_ISSUE_DETAIL: usize = 2;
pub const GH_PANE_PR_DETAIL: usize = 3;

const GH_SLOT_DETAIL: usize = 0;

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
}

impl PaneSet for GhPanes {
    fn get_mut(&mut self, idx: usize) -> Option<&mut dyn Pane<PaneEvent>> {
        self.issue_tab
            .get_pane_mut(GH_PANE_ISSUE_LIST, GH_PANE_ISSUE_DETAIL, idx)
            .or_else(|| {
                self.pr_tab
                    .get_pane_mut(GH_PANE_PR_LIST, GH_PANE_PR_DETAIL, idx)
            })
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
    layout_config: PageLayoutConfig,
    view_keymap: Keymap<ViewAction>,
}

impl GitHubState {
    pub fn new() -> Self {
        Self {
            pane: PaneShared {
                focused_pane: GH_PANE_ISSUE_LIST,
                previous_pane: GH_PANE_ISSUE_LIST,
                search: SearchState::new(),
            },
            panes: GhPanes {
                issue_tab: Tab {
                    list: issue_list::new_pane(),
                    detail: GhDetailViewPane::new(GH_PANE_ISSUE_DETAIL),
                },
                pr_tab: Tab {
                    list: pr_list::new_pane(),
                    detail: GhDetailViewPane::new(GH_PANE_PR_DETAIL),
                },
            },
            gh_available: None,
            gh_error: None,
            bg_rx: None,
            bg_tx: None,
            initialized: false,
            layout_config: default_gh_layout_config(),
            view_keymap: Keymap::new().bindings(view_bindings(|v| v)),
        }
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
    fn is_on_pr_tab(&self) -> bool {
        matches!(self.pane.focused_pane, GH_PANE_PR_LIST | GH_PANE_PR_DETAIL)
    }

    /// Sync the active tab's detail view.
    pub fn sync_active_detail(&mut self) {
        let tx = match &self.bg_tx {
            Some(tx) => tx,
            None => return,
        };
        if self.is_on_pr_tab() {
            self.panes.pr_tab.sync_detail(tx);
        } else {
            self.panes.issue_tab.sync_detail(tx);
        }
    }

    /// Refresh only the currently displayed detail item (cache-bust + re-fetch).
    pub fn refresh_detail(&mut self) {
        let info = if self.is_on_pr_tab() {
            self.panes.pr_tab.detail.current_detail_info()
        } else {
            self.panes.issue_tab.detail.current_detail_info()
        };
        match info {
            None => self.sync_active_detail(),
            Some((kind, number)) => {
                if let Some(tx) = &self.bg_tx {
                    let dv = if self.is_on_pr_tab() {
                        &mut self.panes.pr_tab.detail
                    } else {
                        &mut self.panes.issue_tab.detail
                    };
                    dv.invalidate(kind, number);
                    dv.load(kind, number, tx);
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
                PaneEvent::SetFocus(GH_PANE_ISSUE_DETAIL | GH_PANE_PR_DETAIL) => {
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
                        if matches!(origin, GH_PANE_ISSUE_LIST | GH_PANE_PR_LIST) {
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
                if matches!(
                    self.pane.focused_pane,
                    GH_PANE_ISSUE_DETAIL | GH_PANE_PR_DETAIL
                ) {
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
    fn label(&self) -> &'static str {
        "GitHub"
    }

    fn help_bindings(&self) -> Vec<(String, String)> {
        use crate::core::keymap::help_section;
        use crate::github::panes::detail_view;

        let s = |k: &str, v: &str| (k.to_string(), v.to_string());
        let mut entries = vec![
            s("1 / 2", "Switch view"),
            s("r", "Refresh data"),
            s("?", "Toggle help"),
            s("q", "Quit"),
        ];
        entries.extend(help_section("Issues"));
        entries.extend(crate::github::panes::gh_list::default_keymap(KeyCode::Tab).help_entries());
        entries.extend(help_section("Pull Requests"));
        entries
            .extend(crate::github::panes::gh_list::default_keymap(KeyCode::BackTab).help_entries());
        entries.extend(help_section("Detail View"));
        entries.extend(detail_view::default_keymap().help_entries());
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

        let slots = self.layout_config.resolve_slots(self.pane.focused_pane);
        let areas = resolve_layout(frame.content, &self.layout_config.tree, &slots);
        self.pane.render_panes(&mut self.panes, f, ctx, &areas);

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
