use crate::github::client;
use crate::github::types::*;
use std::collections::HashMap;
use std::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GhFocusedPane {
    IssueList,
    PrList,
    Detail,
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
    Comments,
}

pub enum GhBgMessage {
    AuthStatus(Result<(), String>),
    IssueList(Result<Vec<GhIssueListItem>, String>),
    PrList(Result<Vec<GhPrListItem>, String>),
    IssueDetail(Result<GhIssueDetail, String>),
    PrDetail(Result<GhPrDetail, String>),
}

pub struct GitHubState {
    pub gh_available: Option<bool>,
    pub gh_error: Option<String>,
    pub issues: Vec<GhIssueListItem>,
    pub prs: Vec<GhPrListItem>,
    pub issues_loading: bool,
    pub prs_loading: bool,
    pub issue_selected_idx: usize,
    pub pr_selected_idx: usize,
    pub focused_pane: GhFocusedPane,
    pub previous_pane: GhFocusedPane,
    pub detail: GhDetailContent,
    pub detail_pane: GhDetailPane,
    pub detail_scroll_body: u16,
    pub detail_scroll_status: u16,
    pub detail_scroll_comments: u16,
    pub detail_view_height: u16,
    issue_cache: HashMap<u64, GhIssueDetail>,
    pr_cache: HashMap<u64, GhPrDetail>,
    bg_rx: Option<mpsc::Receiver<GhBgMessage>>,
    bg_tx: Option<mpsc::Sender<GhBgMessage>>,
    pub initialized: bool,
}

impl GitHubState {
    pub fn new() -> Self {
        Self {
            gh_available: None,
            gh_error: None,
            issues: Vec::new(),
            prs: Vec::new(),
            issues_loading: false,
            prs_loading: false,
            issue_selected_idx: 0,
            pr_selected_idx: 0,
            focused_pane: GhFocusedPane::IssueList,
            previous_pane: GhFocusedPane::IssueList,
            detail: GhDetailContent::None,
            detail_pane: GhDetailPane::Body,
            detail_scroll_body: 0,
            detail_scroll_status: 0,
            detail_scroll_comments: 0,
            detail_view_height: 0,
            issue_cache: HashMap::new(),
            pr_cache: HashMap::new(),
            bg_rx: None,
            bg_tx: None,
            initialized: false,
        }
    }

    pub fn active_detail_scroll_mut(&mut self) -> &mut u16 {
        match self.detail_pane {
            GhDetailPane::Body => &mut self.detail_scroll_body,
            GhDetailPane::Status => &mut self.detail_scroll_status,
            GhDetailPane::Comments => &mut self.detail_scroll_comments,
        }
    }

    pub fn is_pr(&self) -> bool {
        matches!(&self.detail, GhDetailContent::Pr(_))
    }

    fn reset_detail_panes(&mut self) {
        self.detail_pane = GhDetailPane::Body;
        self.detail_scroll_body = 0;
        self.detail_scroll_status = 0;
        self.detail_scroll_comments = 0;
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

        self.issues_loading = true;
        self.prs_loading = true;

        // Auth check + issue list
        let tx2 = tx.clone();
        std::thread::spawn(move || {
            let auth = client::check_gh_available();
            let _ = tx2.send(GhBgMessage::AuthStatus(auth.clone()));
            if auth.is_ok() {
                let issues = client::list_issues(50);
                let _ = tx2.send(GhBgMessage::IssueList(issues));
            }
        });

        // PR list (parallel)
        let tx3 = tx;
        std::thread::spawn(move || {
            // Small delay to let auth check land first
            let prs = client::list_prs(50);
            let _ = tx3.send(GhBgMessage::PrList(prs));
        });
    }

    /// Drain background messages from worker threads.
    pub fn drain_bg_messages(&mut self) {
        // Collect all pending messages first to avoid borrow conflict
        let messages: Vec<_> = match &self.bg_rx {
            Some(rx) => rx.try_iter().collect(),
            None => return,
        };

        let mut issue_list_arrived = false;
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
                        self.issues_loading = false;
                        self.prs_loading = false;
                    }
                },
                GhBgMessage::IssueList(result) => {
                    self.issues_loading = false;
                    match result {
                        Ok(issues) => {
                            self.issues = issues;
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
                    self.prs_loading = false;
                    match result {
                        Ok(prs) => self.prs = prs,
                        Err(e) => {
                            if self.gh_error.is_none() {
                                self.gh_error = Some(e);
                            }
                        }
                    }
                }
                GhBgMessage::IssueDetail(result) => match result {
                    Ok(detail) => {
                        self.issue_cache.insert(detail.number, detail.clone());
                        self.detail = GhDetailContent::Issue(Box::new(detail));
                    }
                    Err(e) => self.detail = GhDetailContent::Error(e),
                },
                GhBgMessage::PrDetail(result) => match result {
                    Ok(detail) => {
                        self.pr_cache.insert(detail.number, detail.clone());
                        self.detail = GhDetailContent::Pr(Box::new(detail));
                    }
                    Err(e) => self.detail = GhDetailContent::Error(e),
                },
            }
        }

        // Auto-load detail for the first issue when the list arrives
        if issue_list_arrived {
            self.load_selected_issue_detail();
        }
    }

    /// Load issue detail — serves from cache if available, otherwise fetches in background.
    pub fn load_issue_detail(&mut self, number: u64) {
        if let Some(cached) = self.issue_cache.get(&number) {
            self.detail = GhDetailContent::Issue(Box::new(cached.clone()));
            self.reset_detail_panes();
            return;
        }
        self.detail = GhDetailContent::Loading {
            kind: GhDetailKind::Issue,
            number,
        };
        self.reset_detail_panes();
        if let Some(tx) = &self.bg_tx {
            let tx = tx.clone();
            std::thread::spawn(move || {
                let result = client::get_issue(number);
                let _ = tx.send(GhBgMessage::IssueDetail(result));
            });
        }
    }

    /// Load PR detail — serves from cache if available, otherwise fetches in background.
    pub fn load_pr_detail(&mut self, number: u64) {
        if let Some(cached) = self.pr_cache.get(&number) {
            self.detail = GhDetailContent::Pr(Box::new(cached.clone()));
            self.reset_detail_panes();
            return;
        }
        self.detail = GhDetailContent::Loading {
            kind: GhDetailKind::Pr,
            number,
        };
        self.reset_detail_panes();
        if let Some(tx) = &self.bg_tx {
            let tx = tx.clone();
            std::thread::spawn(move || {
                let result = client::get_pr(number);
                let _ = tx.send(GhBgMessage::PrDetail(result));
            });
        }
    }

    /// Auto-load detail for the currently selected issue.
    pub fn load_selected_issue_detail(&mut self) {
        if let Some(issue) = self.issues.get(self.issue_selected_idx) {
            let number = issue.number;
            self.load_issue_detail(number);
        }
    }

    /// Auto-load detail for the currently selected PR.
    pub fn load_selected_pr_detail(&mut self) {
        if let Some(pr) = self.prs.get(self.pr_selected_idx) {
            let number = pr.number;
            self.load_pr_detail(number);
        }
    }

    /// Refresh: re-fetch issue and PR lists, clear caches.
    pub fn refresh(&mut self) {
        self.issues_loading = true;
        self.prs_loading = true;
        self.gh_error = None;
        self.issue_cache.clear();
        self.pr_cache.clear();

        if let Some(tx) = &self.bg_tx {
            let tx2 = tx.clone();
            std::thread::spawn(move || {
                let issues = client::list_issues(50);
                let _ = tx2.send(GhBgMessage::IssueList(issues));
            });
            let tx3 = tx.clone();
            std::thread::spawn(move || {
                let prs = client::list_prs(50);
                let _ = tx3.send(GhBgMessage::PrList(prs));
            });
        }
    }
}
