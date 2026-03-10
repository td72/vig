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
pub const GH_PANE_DETAIL: usize = 2;

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

// === GhPanes (grouping struct for disjoint borrows) ===

pub struct GhPanes {
    pub issue_list: GhIssueListPane,
    pub pr_list: GhPrListPane,
    pub detail_view: GhDetailViewPane,
}

impl PaneSet for GhPanes {
    fn get_mut(&mut self, idx: usize) -> Option<&mut dyn Pane<PaneEvent>> {
        match idx {
            GH_PANE_ISSUE_LIST => Some(&mut self.issue_list),
            GH_PANE_PR_LIST => Some(&mut self.pr_list),
            GH_PANE_DETAIL => Some(&mut self.detail_view),
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
                issue_list: GhIssueListPane::new(),
                pr_list: GhPrListPane::new(),
                detail_view: GhDetailViewPane::new(),
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
        self.panes.issue_list.initialize(&tx);
        self.panes.pr_list.initialize(&tx);

        // Auto-load detail for the first item from disk cache
        self.load_detail();
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
                        self.panes.issue_list.loading = false;
                        self.panes.pr_list.loading = false;
                    }
                },
                GhBgMessage::IssueList(result) => {
                    self.panes.issue_list.loading = false;
                    match result {
                        Ok(issues) => {
                            self.panes.issue_list.apply_list(issues);
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
                    self.panes.pr_list.loading = false;
                    match result {
                        Ok(prs) => {
                            self.panes.pr_list.apply_list(prs);
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
                    Ok(detail) => self.panes.detail_view.apply_issue_detail(detail),
                    Err(e) => self.panes.detail_view.content = GhDetailContent::Error(e),
                },
                GhBgMessage::PrDetail(result) => {
                    self.panes.detail_view.apply_pr_detail_result(result);
                }
            }
        }

        // Auto-load detail when a fresh list arrives for the active tab
        let on_pr = self.pane.focused_pane == GH_PANE_PR_LIST
            || (self.pane.focused_pane == GH_PANE_DETAIL
                && self.pane.previous_pane == GH_PANE_PR_LIST);
        if (on_pr && pr_list_arrived) || (!on_pr && issue_list_arrived) {
            self.load_detail();
        }
    }

    /// Load detail for whichever list pane is currently active (or was, if in Detail).
    pub fn load_detail(&mut self) {
        let tx = match &self.bg_tx {
            Some(tx) => tx,
            None => return,
        };
        let origin = if self.pane.focused_pane == GH_PANE_DETAIL {
            self.pane.previous_pane
        } else {
            self.pane.focused_pane
        };
        match origin {
            GH_PANE_ISSUE_LIST => {
                if let Some(n) = self.panes.issue_list.selected_number() {
                    self.panes.detail_view.load_issue(n, tx);
                }
            }
            GH_PANE_PR_LIST => {
                if let Some(n) = self.panes.pr_list.selected_number() {
                    self.panes.detail_view.load_pr(n, tx);
                }
            }
            _ => {}
        }
    }

    /// Refresh only the currently displayed detail item (cache-bust + re-fetch).
    pub fn refresh_detail(&mut self) {
        let (kind, number) = match &self.panes.detail_view.content {
            GhDetailContent::Issue(detail) => (GhDetailKind::Issue, detail.number),
            GhDetailContent::Pr(detail) => (GhDetailKind::Pr, detail.number),
            GhDetailContent::Loading { kind, number } => (*kind, *number),
            GhDetailContent::Error(_) => {
                self.load_detail();
                return;
            }
            _ => return,
        };
        match kind {
            GhDetailKind::Issue => {
                self.panes.detail_view.invalidate_issue(number);
                if let Some(tx) = &self.bg_tx {
                    self.panes.detail_view.load_issue(number, tx);
                }
            }
            GhDetailKind::Pr => {
                self.panes.detail_view.invalidate_pr(number);
                if let Some(tx) = &self.bg_tx {
                    self.panes.detail_view.load_pr(number, tx);
                }
            }
        }
    }

    /// Refresh: re-fetch issue and PR lists, clear caches.
    pub fn refresh(&mut self) {
        self.gh_error = None;
        self.panes.detail_view.clear_caches();
        if let Some(tx) = &self.bg_tx {
            self.panes.issue_list.spawn_fetch(tx);
            self.panes.pr_list.spawn_fetch(tx);
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
                PaneEvent::SetFocus(GH_PANE_DETAIL) => {
                    self.load_detail();
                }
                PaneEvent::SelectionChanged => {
                    self.load_detail();
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
                if self.pane.focused_pane == GH_PANE_DETAIL {
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

        self.pane
            .render_panes(&mut self.panes, f, ctx, &gl.pane_areas());

        status_bar::render_gh_status_bar(f, ctx, self, gl.status_bar);
    }

    fn intercepts_all_keys(&self) -> bool {
        self.pane.search.active
    }

    fn on_tick(&mut self, _ctx: &mut AppContext) {
        if let Some(tx) = &self.bg_tx {
            self.panes.detail_view.handle_watch_tick(tx);
        }
    }

    fn on_activate(&mut self, _ctx: &mut AppContext) {
        self.initialize();
    }

    fn drain_background(&mut self) {
        self.drain_bg_messages();
    }
}
