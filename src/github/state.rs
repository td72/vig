use crate::core::app::AppContext;
use crate::core::page::PageAction;
use crate::core::pane::{self, Pane, PaneEvent, PaneSet, PaneShared};
use crate::core::search::SearchState;
use crate::core::ui::status_bar;
use crate::github::domain::client;
use crate::github::domain::types::*;
use crate::github::panes::detail_view::GhDetailViewPane;
use crate::github::panes::issue_list::GhIssueListPane;
use crate::github::panes::pr_list::GhPrListPane;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{layout::Rect, Frame};
use std::sync::mpsc;

pub const GH_PANE_ISSUE_LIST: usize = 0;
pub const GH_PANE_PR_LIST: usize = 1;
pub const GH_PANE_ISSUE_DETAIL: usize = 2;
pub const GH_PANE_PR_DETAIL: usize = 3;

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

// === Tab structs (list + detail pairing) ===

/// IssueList (list) + DetailView (detail)
pub struct IssueTab {
    pub issue_list: GhIssueListPane,
    pub detail_view: GhDetailViewPane,
}

impl IssueTab {
    /// Sync DetailView to show the selected issue.
    pub fn sync_detail(&mut self, tx: &mpsc::Sender<GhBgMessage>) {
        if let Some(n) = self.issue_list.selected_number() {
            self.detail_view.load_issue(n, tx);
        }
    }
}

/// PrList (list) + DetailView (detail)
pub struct PrTab {
    pub pr_list: GhPrListPane,
    pub detail_view: GhDetailViewPane,
}

impl PrTab {
    /// Sync DetailView to show the selected PR.
    pub fn sync_detail(&mut self, tx: &mpsc::Sender<GhBgMessage>) {
        if let Some(n) = self.pr_list.selected_number() {
            self.detail_view.load_pr(n, tx);
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
        match idx {
            GH_PANE_ISSUE_LIST => Some(&mut self.issue_tab.issue_list),
            GH_PANE_PR_LIST => Some(&mut self.pr_tab.pr_list),
            GH_PANE_ISSUE_DETAIL => Some(&mut self.issue_tab.detail_view),
            GH_PANE_PR_DETAIL => Some(&mut self.pr_tab.detail_view),
            _ => None,
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
                issue_tab: IssueTab {
                    issue_list: GhIssueListPane::new(),
                    detail_view: GhDetailViewPane::new(GH_PANE_ISSUE_DETAIL),
                },
                pr_tab: PrTab {
                    pr_list: GhPrListPane::new(),
                    detail_view: GhDetailViewPane::new(GH_PANE_PR_DETAIL),
                },
            },
            gh_available: None,
            gh_error: None,
            bg_rx: None,
            bg_tx: None,
            initialized: false,
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
        self.panes.issue_tab.issue_list.initialize(&tx);
        self.panes.pr_tab.pr_list.initialize(&tx);

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
                        self.panes.issue_tab.issue_list.loading = false;
                        self.panes.pr_tab.pr_list.loading = false;
                    }
                },
                GhBgMessage::IssueList(result) => {
                    self.panes.issue_tab.issue_list.loading = false;
                    match result {
                        Ok(issues) => {
                            self.panes.issue_tab.issue_list.apply_list(issues);
                            issue_list_arrived = true;
                        }
                        Err(e) => {
                            if self.gh_error.is_none() {
                                self.gh_error = Some(e);
                            }
                        }
                    }
                }
                GhBgMessage::PrList(result) => {
                    self.panes.pr_tab.pr_list.loading = false;
                    match result {
                        Ok(prs) => {
                            self.panes.pr_tab.pr_list.apply_list(prs);
                            pr_list_arrived = true;
                        }
                        Err(e) => {
                            if self.gh_error.is_none() {
                                self.gh_error = Some(e);
                            }
                        }
                    }
                }
                GhBgMessage::IssueDetail(result) => match result {
                    Ok(detail) => self.panes.issue_tab.detail_view.apply_issue_detail(detail),
                    Err(e) => self.panes.issue_tab.detail_view.content = GhDetailContent::Error(e),
                },
                GhBgMessage::PrDetail(result) => {
                    self.panes.pr_tab.detail_view.apply_pr_detail_result(result);
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
        let dv = if self.is_on_pr_tab() {
            &self.panes.pr_tab.detail_view
        } else {
            &self.panes.issue_tab.detail_view
        };
        let action = match &dv.content {
            GhDetailContent::Issue(detail) => Some((GhDetailKind::Issue, detail.number)),
            GhDetailContent::Pr(detail) => Some((GhDetailKind::Pr, detail.number)),
            GhDetailContent::Loading { kind, number } => Some((*kind, *number)),
            GhDetailContent::Error(_) | GhDetailContent::None => None,
        };
        match action {
            None => self.sync_active_detail(),
            Some((kind, number)) => {
                if let Some(tx) = &self.bg_tx {
                    let dv = if self.is_on_pr_tab() {
                        &mut self.panes.pr_tab.detail_view
                    } else {
                        &mut self.panes.issue_tab.detail_view
                    };
                    match kind {
                        GhDetailKind::Issue => {
                            dv.invalidate_issue(number);
                            dv.load_issue(number, tx);
                        }
                        GhDetailKind::Pr => {
                            dv.invalidate_pr(number);
                            dv.load_pr(number, tx);
                        }
                    }
                }
            }
        }
    }

    /// Refresh: re-fetch issue and PR lists, clear caches.
    pub fn refresh(&mut self) {
        self.gh_error = None;
        self.panes.issue_tab.detail_view.clear_caches();
        self.panes.pr_tab.detail_view.clear_caches();
        if let Some(tx) = &self.bg_tx {
            self.panes.issue_tab.issue_list.spawn_fetch(tx);
            self.panes.pr_tab.pr_list.spawn_fetch(tx);
        }
    }

    const TAB_PANES: [usize; 2] = [GH_PANE_ISSUE_LIST, GH_PANE_PR_LIST];

    // === Dispatch ===

    pub fn dispatch_key(&mut self, key: KeyEvent) -> Vec<PaneEvent> {
        // Phase 1: shared h/l tab navigation
        if let Some(events) = self.pane.dispatch_tab_key(&Self::TAB_PANES, key) {
            return events;
        }

        // Per-pane delegation (dynamic dispatch)
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
                _ => {}
            }
        }
        Ok(PageAction::None)
    }

    // === View-level key handling ===

    pub fn handle_view_key(&mut self, ctx: &mut AppContext, key: KeyEvent) -> Result<PageAction> {
        if let Some(action) = pane::handle_common_view_key(ctx, key) {
            return Ok(action);
        }

        match key.code {
            KeyCode::Char('r') => {
                if matches!(
                    self.pane.focused_pane,
                    GH_PANE_ISSUE_DETAIL | GH_PANE_PR_DETAIL
                ) {
                    self.refresh_detail();
                } else {
                    self.refresh();
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

impl crate::core::app::PageState for GitHubState {
    fn label(&self) -> &'static str {
        "GitHub"
    }

    fn help_bindings(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            ("1 / 2", "Switch view"),
            ("h / l", "Issues ↔ PRs (list)"),
            ("j / k", "Navigate list"),
            ("i / Enter", "Open detail"),
            ("o", "Open in browser"),
            ("Esc", "Back to list"),
            ("h / l", "Body ↔ Right pane (detail)"),
            ("Tab / S-Tab", "Cycle right panes (detail)"),
            ("Ctrl+d", "Half page down (detail)"),
            ("Ctrl+u", "Half page up (detail)"),
            ("g / G", "Top / Bottom"),
            ("r", "Refresh data"),
            ("w", "Toggle watch mode (PR)"),
            ("?", "Toggle help"),
            ("q", "Quit"),
        ]
    }

    fn handle_key(&mut self, ctx: &mut AppContext, key: KeyEvent) -> Result<PageAction> {
        self.handle_view_key(ctx, key)
    }

    fn render(&mut self, f: &mut Frame, ctx: &AppContext, area: Rect) {
        let gl = crate::github::layout::compute_gh_layout(area);
        status_bar::render_gh_header(f, ctx, gl.header);

        let detail_id = if self.is_on_pr_tab() {
            GH_PANE_PR_DETAIL
        } else {
            GH_PANE_ISSUE_DETAIL
        };
        self.pane
            .render_panes(&mut self.panes, f, ctx, &gl.pane_areas(detail_id));

        status_bar::render_gh_status_bar(f, ctx, self, gl.status_bar);
    }

    fn intercepts_all_keys(&self) -> bool {
        self.pane.search.active
    }

    fn on_tick(&mut self, _ctx: &mut AppContext) {
        if let Some(tx) = &self.bg_tx {
            self.panes.pr_tab.detail_view.handle_watch_tick(tx);
        }
    }

    fn on_activate(&mut self, _ctx: &mut AppContext) {
        self.initialize();
    }

    fn drain_background(&mut self) {
        self.drain_bg_messages();
    }
}
