use crate::core::app::AppContext;
use crate::core::page::PageAction;
use crate::core::pane::PaneShared;
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

// === GhShared: shared state passed to pane handle_key ===

pub struct GhShared {
    pub pane: PaneShared,
}

// === GhPaneEvent: cross-pane side effects ===

pub enum GhPaneEvent {
    SetFocus(usize),
    SelectionChanged,
    OpenIssueBrowser(u64),
    OpenPrBrowser(u64),
    OpenUrl(String),
}

// === GitHubState ===

pub struct GitHubState {
    pub shared: GhShared,
    pub issue_list: GhIssueListPane,
    pub pr_list: GhPrListPane,
    pub detail_view: GhDetailViewPane,
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
            shared: GhShared {
                pane: PaneShared {
                    focused_pane: GH_PANE_ISSUE_LIST,
                    previous_pane: GH_PANE_ISSUE_LIST,
                    search: SearchState::new(),
                },
            },
            issue_list: GhIssueListPane::new(),
            pr_list: GhPrListPane::new(),
            detail_view: GhDetailViewPane::new(),
            gh_available: None,
            gh_error: None,
            bg_rx: None,
            bg_tx: None,
            initialized: false,
        }
    }

    pub fn set_focus(&mut self, id: usize) {
        self.shared.pane.previous_pane = self.shared.pane.focused_pane;
        self.shared.pane.focused_pane = id;
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
        self.issue_list.initialize(&tx);
        self.pr_list.initialize(&tx);

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
                        self.issue_list.loading = false;
                        self.pr_list.loading = false;
                    }
                },
                GhBgMessage::IssueList(result) => {
                    self.issue_list.loading = false;
                    match result {
                        Ok(issues) => {
                            self.issue_list.apply_list(issues);
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
                    self.pr_list.loading = false;
                    match result {
                        Ok(prs) => {
                            self.pr_list.apply_list(prs);
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
                    Ok(detail) => self.detail_view.apply_issue_detail(detail),
                    Err(e) => self.detail_view.content = GhDetailContent::Error(e),
                },
                GhBgMessage::PrDetail(result) => {
                    self.detail_view.apply_pr_detail_result(result);
                }
            }
        }

        // Auto-load detail when a fresh list arrives for the active tab
        let on_pr = self.shared.pane.focused_pane == GH_PANE_PR_LIST
            || (self.shared.pane.focused_pane == GH_PANE_DETAIL
                && self.shared.pane.previous_pane == GH_PANE_PR_LIST);
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
        let origin = if self.shared.pane.focused_pane == GH_PANE_DETAIL {
            self.shared.pane.previous_pane
        } else {
            self.shared.pane.focused_pane
        };
        match origin {
            GH_PANE_ISSUE_LIST => {
                if let Some(n) = self.issue_list.selected_number() {
                    self.detail_view.load_issue(n, tx);
                }
            }
            GH_PANE_PR_LIST => {
                if let Some(n) = self.pr_list.selected_number() {
                    self.detail_view.load_pr(n, tx);
                }
            }
            _ => {}
        }
    }

    /// Refresh only the currently displayed detail item (cache-bust + re-fetch).
    pub fn refresh_detail(&mut self) {
        let (kind, number) = match &self.detail_view.content {
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
                self.detail_view.invalidate_issue(number);
                if let Some(tx) = &self.bg_tx {
                    self.detail_view.load_issue(number, tx);
                }
            }
            GhDetailKind::Pr => {
                self.detail_view.invalidate_pr(number);
                if let Some(tx) = &self.bg_tx {
                    self.detail_view.load_pr(number, tx);
                }
            }
        }
    }

    /// Refresh: re-fetch issue and PR lists, clear caches.
    pub fn refresh(&mut self) {
        self.gh_error = None;
        self.detail_view.clear_caches();
        if let Some(tx) = &self.bg_tx {
            self.issue_list.spawn_fetch(tx);
            self.pr_list.spawn_fetch(tx);
        }
    }

    // === Dispatch ===

    pub fn dispatch_key(&mut self, key: KeyEvent) -> Vec<GhPaneEvent> {
        match self.shared.pane.focused_pane {
            GH_PANE_ISSUE_LIST => match key.code {
                KeyCode::Char('l') | KeyCode::Tab => {
                    vec![GhPaneEvent::SetFocus(GH_PANE_PR_LIST)]
                }
                _ => self.issue_list.handle_key(&self.shared.pane, key),
            },
            GH_PANE_PR_LIST => match key.code {
                KeyCode::Char('h') | KeyCode::BackTab => {
                    vec![GhPaneEvent::SetFocus(GH_PANE_ISSUE_LIST)]
                }
                _ => self.pr_list.handle_key(&self.shared.pane, key),
            },
            GH_PANE_DETAIL => self.detail_view.handle_key(&self.shared.pane, key),
            _ => vec![],
        }
    }

    // === Event processing ===

    pub fn process_events(
        &mut self,
        ctx: &mut AppContext,
        events: Vec<GhPaneEvent>,
    ) -> Result<PageAction> {
        for event in events {
            match event {
                GhPaneEvent::SetFocus(pane) => {
                    self.set_focus(pane);
                    if pane == GH_PANE_DETAIL {
                        self.load_detail();
                    }
                }
                GhPaneEvent::SelectionChanged => {
                    self.load_detail();
                }
                GhPaneEvent::OpenIssueBrowser(n) => match client::open_issue_in_browser(n) {
                    Ok(()) => {
                        ctx.status_message = Some(format!("Opening issue #{n} in browser..."));
                    }
                    Err(e) => {
                        ctx.status_message = Some(format!("Failed to open browser: {e}"));
                    }
                },
                GhPaneEvent::OpenPrBrowser(n) => match client::open_pr_in_browser(n) {
                    Ok(()) => {
                        ctx.status_message = Some(format!("Opening PR #{n} in browser..."));
                    }
                    Err(e) => {
                        ctx.status_message = Some(format!("Failed to open browser: {e}"));
                    }
                },
                GhPaneEvent::OpenUrl(url) => match client::open_url(&url) {
                    Ok(()) => {
                        ctx.status_message = Some("Opening in browser...".to_string());
                    }
                    Err(e) => {
                        ctx.status_message = Some(e);
                    }
                },
            }
        }
        Ok(PageAction::None)
    }

    // === View-level key handling ===

    pub fn handle_view_key(&mut self, ctx: &mut AppContext, key: KeyEvent) -> Result<PageAction> {
        match key.code {
            KeyCode::Char('q') => {
                ctx.should_quit = true;
            }
            KeyCode::Char('?') => {
                ctx.show_help = true;
            }
            KeyCode::Char('r') => {
                if self.shared.pane.focused_pane == GH_PANE_DETAIL {
                    self.refresh_detail();
                } else {
                    self.refresh();
                }
            }
            KeyCode::Char('w') => {
                self.detail_view.toggle_watch_mode();
            }
            _ => {
                let events = self.dispatch_key(key);
                return self.process_events(ctx, events);
            }
        }
        Ok(PageAction::None)
    }

    // === Unified handle_key ===

    pub fn handle_key(&mut self, ctx: &mut AppContext, key: KeyEvent) -> Result<PageAction> {
        self.handle_view_key(ctx, key)
    }

    // === Help bindings ===

    pub fn help_bindings_list() -> Vec<(&'static str, &'static str)> {
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

    // === Render ===

    pub fn render(&mut self, f: &mut Frame, ctx: &AppContext, area: Rect) {
        let gl = crate::github::layout::compute_gh_layout(area);
        status_bar::render_gh_header(f, ctx, gl.header);
        self.issue_list
            .render(f, ctx, &self.shared.pane, gl.issue_list);
        self.pr_list.render(f, ctx, &self.shared.pane, gl.pr_list);
        self.detail_view
            .render(f, ctx, &self.shared.pane, gl.main_pane);
        status_bar::render_gh_status_bar(f, ctx, self, gl.status_bar);
    }

    // === Lifecycle ===

    pub fn on_tick(&mut self) {
        if let Some(tx) = &self.bg_tx {
            self.detail_view.handle_watch_tick(tx);
        }
    }

    pub fn on_activate(&mut self) {
        self.initialize();
    }
}

impl crate::core::app::PageState for GitHubState {
    fn label(&self) -> &'static str {
        "GitHub"
    }
    fn help_bindings(&self) -> Vec<(&'static str, &'static str)> {
        Self::help_bindings_list()
    }
    fn handle_key(&mut self, ctx: &mut AppContext, key: KeyEvent) -> Result<PageAction> {
        GitHubState::handle_key(self, ctx, key)
    }
    fn render(&mut self, f: &mut ratatui::Frame, ctx: &AppContext, area: ratatui::layout::Rect) {
        GitHubState::render(self, f, ctx, area);
    }
    fn intercepts_all_keys(&self) -> bool {
        self.shared.pane.search.active
    }
    fn on_tick(&mut self, _ctx: &mut AppContext) {
        GitHubState::on_tick(self);
    }
    fn on_activate(&mut self, _ctx: &mut AppContext) {
        GitHubState::on_activate(self);
    }
    fn drain_background(&mut self) {
        self.drain_bg_messages();
    }
}
