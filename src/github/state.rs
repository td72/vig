use crate::core::pane::PaneShared;
use crate::core::search::SearchState;
use crate::github::domain::client;
use crate::github::domain::types::*;
use crate::github::panes::detail_view::GhDetailViewPane;
use crate::github::panes::issue_list::GhIssueListPane;
use crate::github::panes::pr_list::GhPrListPane;
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
    LoadDetail,
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
}

impl crate::core::app::PageState for GitHubState {
    fn drain_background(&mut self) {
        self.drain_bg_messages();
    }
    fn search(&self) -> &SearchState {
        &self.shared.pane.search
    }
    fn search_mut(&mut self) -> &mut SearchState {
        &mut self.shared.pane.search
    }
}
