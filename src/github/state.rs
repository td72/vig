use crate::core::app::SearchState;
use crate::github::domain::client;
use crate::github::domain::types::*;
use crate::github::panes::detail_view::GhDetailViewPane;
use crate::github::panes::issue_list::GhIssueListPane;
use crate::github::panes::pr_list::GhPrListPane;
use std::sync::mpsc;
use std::time::{Instant, SystemTime};

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
    pub focused_pane: GhFocusedPane,
    pub previous_pane: GhFocusedPane,
    pub search: SearchState,
}

// === GhPaneEvent: cross-pane side effects ===

pub enum GhPaneEvent {
    SetFocus(GhFocusedPane),
    LoadSelectedIssueDetail,
    LoadSelectedPrDetail,
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
    bg_tx: Option<mpsc::Sender<GhBgMessage>>,
    pub initialized: bool,
    // Watch mode
    pub watch_mode: bool,
    watch_last_refresh: Option<Instant>,
    watch_last_update: Option<SystemTime>,
    watch_in_flight_since: Option<Instant>,
    pub watch_error: Option<String>,
}

impl GitHubState {
    pub fn new() -> Self {
        Self {
            shared: GhShared {
                focused_pane: GhFocusedPane::IssueList,
                previous_pane: GhFocusedPane::IssueList,
                search: SearchState::new(),
            },
            issue_list: GhIssueListPane::new(),
            pr_list: GhPrListPane::new(),
            detail_view: GhDetailViewPane::new(),
            gh_available: None,
            gh_error: None,
            bg_rx: None,
            bg_tx: None,
            initialized: false,
            watch_mode: false,
            watch_last_refresh: None,
            watch_last_update: None,
            watch_in_flight_since: None,
            watch_error: None,
        }
    }

    pub fn set_focus(&mut self, id: GhFocusedPane) {
        self.shared.previous_pane = self.shared.focused_pane;
        self.shared.focused_pane = id;
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

        // Load cached lists from disk (instant display, will be refreshed in background)
        self.issue_list.load_from_cache();
        self.pr_list.load_from_cache();

        // Auto-load detail for the first item from disk cache
        if !self.issue_list.issues.is_empty() {
            self.load_selected_issue_detail();
        }

        self.issue_list.loading = true;
        self.pr_list.loading = true;

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
            let prs = client::list_prs(50);
            let _ = tx3.send(GhBgMessage::PrList(prs));
        });
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
                    self.watch_in_flight_since = None;
                    match result {
                        Ok(detail) => {
                            self.watch_error = None;
                            self.detail_view.apply_pr_detail(detail);
                        }
                        Err(e) => {
                            if self.watch_mode {
                                self.watch_error = Some(e);
                            } else {
                                self.detail_view.content = GhDetailContent::Error(e);
                            }
                        }
                    }
                }
            }
        }

        // Auto-load detail for the currently focused/selected list
        let on_pr = self.shared.focused_pane == GhFocusedPane::PrList
            || (self.shared.focused_pane == GhFocusedPane::Detail
                && self.shared.previous_pane == GhFocusedPane::PrList);
        if on_pr {
            if pr_list_arrived {
                self.load_selected_pr_detail();
            }
        } else if issue_list_arrived {
            self.load_selected_issue_detail();
        }
    }

    /// Auto-load detail for the currently selected issue.
    pub fn load_selected_issue_detail(&mut self) {
        if let Some(issue) = self.issue_list.issues.get(self.issue_list.selected_idx) {
            let number = issue.number;
            if let Some(tx) = &self.bg_tx {
                self.detail_view.load_issue(number, tx);
            }
        }
    }

    /// Auto-load detail for the currently selected PR.
    pub fn load_selected_pr_detail(&mut self) {
        if let Some(pr) = self.pr_list.prs.get(self.pr_list.selected_idx) {
            let number = pr.number;
            if let Some(tx) = &self.bg_tx {
                self.detail_view.load_pr(number, tx);
            }
        }
    }

    /// Refresh only the currently displayed detail item (cache-bust + re-fetch).
    pub fn refresh_detail(&mut self) {
        let (kind, number) = match &self.detail_view.content {
            GhDetailContent::Issue(detail) => (GhDetailKind::Issue, detail.number),
            GhDetailContent::Pr(detail) => (GhDetailKind::Pr, detail.number),
            GhDetailContent::Loading { kind, number } => (*kind, *number),
            GhDetailContent::Error(_) => {
                match self.shared.previous_pane {
                    GhFocusedPane::IssueList => self.load_selected_issue_detail(),
                    GhFocusedPane::PrList => self.load_selected_pr_detail(),
                    _ => {}
                }
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

    /// Returns the wall-clock time of the last watch refresh as "HH:MM:SS", if active.
    pub fn watch_last_update_time(&self) -> Option<String> {
        if !self.watch_mode {
            return None;
        }
        self.watch_last_update.map(|t| {
            let secs = t
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let local_secs = secs as i64 + local_utc_offset_secs();
            let time_of_day = local_secs.rem_euclid(86400);
            let h = time_of_day / 3600;
            let m = (time_of_day % 3600) / 60;
            let s = time_of_day % 60;
            format!("{h:02}:{m:02}:{s:02}")
        })
    }

    /// Toggle watch mode (auto-refresh checks every 10s). Only activates on PR detail.
    pub fn toggle_watch_mode(&mut self) {
        if !matches!(&self.detail_view.content, GhDetailContent::Pr(_)) {
            return;
        }
        self.watch_mode = !self.watch_mode;
        if self.watch_mode {
            self.watch_last_refresh = Some(Instant::now());
            self.watch_last_update = Some(SystemTime::now());
        } else {
            self.watch_last_refresh = None;
            self.watch_last_update = None;
            self.watch_in_flight_since = None;
            self.watch_error = None;
        }
    }

    /// Called on every tick. If watch mode is active and 10s have elapsed, refresh the detail.
    pub fn handle_watch_tick(&mut self) {
        if !self.watch_mode {
            return;
        }
        if !matches!(
            &self.detail_view.content,
            GhDetailContent::Pr(_)
                | GhDetailContent::Loading {
                    kind: GhDetailKind::Pr,
                    ..
                }
        ) {
            self.watch_mode = false;
            return;
        }
        if let Some(since) = self.watch_in_flight_since {
            if since.elapsed() < std::time::Duration::from_secs(30) {
                return;
            }
        }
        if let Some(last) = self.watch_last_refresh {
            if last.elapsed() >= std::time::Duration::from_secs(10) {
                self.watch_last_refresh = Some(Instant::now());
                self.watch_last_update = Some(SystemTime::now());
                self.refresh_detail_silent();
            }
        }
    }

    /// Silently re-fetch the current PR detail in the background.
    fn refresh_detail_silent(&mut self) {
        let number = match &self.detail_view.content {
            GhDetailContent::Pr(detail) => detail.number,
            _ => return,
        };
        self.detail_view.invalidate_pr(number);
        self.watch_in_flight_since = Some(Instant::now());
        if let Some(tx) = &self.bg_tx {
            let tx = tx.clone();
            std::thread::spawn(move || {
                let result = client::get_pr(number);
                let _ = tx.send(GhBgMessage::PrDetail(result));
            });
        }
    }

    /// Refresh: re-fetch issue and PR lists, clear caches.
    pub fn refresh(&mut self) {
        self.issue_list.loading = true;
        self.pr_list.loading = true;
        self.gh_error = None;
        self.detail_view.clear_caches();

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

/// Get local UTC offset in seconds, cached after first call.
fn local_utc_offset_secs() -> i64 {
    use std::sync::OnceLock;
    static OFFSET: OnceLock<i64> = OnceLock::new();
    *OFFSET.get_or_init(|| {
        std::process::Command::new("date")
            .arg("+%z")
            .output()
            .ok()
            .and_then(|o| {
                let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if s.len() < 5 {
                    return None;
                }
                let sign: i64 = if s.starts_with('-') { -1 } else { 1 };
                let hours: i64 = s[1..3].parse().ok()?;
                let mins: i64 = s[3..5].parse().ok()?;
                Some(sign * (hours * 3600 + mins * 60))
            })
            .unwrap_or(0)
    })
}

impl crate::core::app::PageState for GitHubState {
    fn drain_background(&mut self) {
        self.drain_bg_messages();
    }
    fn search(&self) -> &SearchState {
        &self.shared.search
    }
    fn search_mut(&mut self) -> &mut SearchState {
        &mut self.shared.search
    }
}
