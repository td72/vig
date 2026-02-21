use crate::core::app::SearchState;
use crate::core::pane::{DetailState, SubPaneScroll};
use crate::github::domain::client;
use crate::github::domain::disk_cache;
use crate::github::domain::types::*;
use std::collections::HashMap;
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
    pub detail_body: SubPaneScroll,
    pub detail_status: SubPaneScroll,
    pub detail_reviews: SubPaneScroll,
    pub detail_comments: SubPaneScroll,
    pub detail_view_height: u16,
    issue_cache: HashMap<u64, GhIssueDetail>,
    pr_cache: HashMap<u64, GhPrDetail>,
    bg_rx: Option<mpsc::Receiver<GhBgMessage>>,
    bg_tx: Option<mpsc::Sender<GhBgMessage>>,
    pub initialized: bool,
    pub watch_mode: bool,
    watch_last_refresh: Option<Instant>,
    watch_last_update: Option<SystemTime>,
    watch_in_flight_since: Option<Instant>,
    pub watch_error: Option<String>,
    pub search: SearchState,
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
            detail_body: SubPaneScroll::default(),
            detail_status: SubPaneScroll::default(),
            detail_reviews: SubPaneScroll::default(),
            detail_comments: SubPaneScroll::default(),
            detail_view_height: 0,
            issue_cache: HashMap::new(),
            pr_cache: HashMap::new(),
            bg_rx: None,
            bg_tx: None,
            initialized: false,
            watch_mode: false,
            watch_last_refresh: None,
            watch_last_update: None,
            watch_in_flight_since: None,
            watch_error: None,
            search: SearchState::new(),
        }
    }

    pub fn is_pr(&self) -> bool {
        matches!(&self.detail, GhDetailContent::Pr(_))
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
        if let Some(issues) = disk_cache::load_issue_list() {
            self.issues = issues;
        }
        if let Some(prs) = disk_cache::load_pr_list() {
            self.prs = prs;
        }

        // Auto-load detail for the first item from disk cache
        if !self.issues.is_empty() {
            self.load_selected_issue_detail();
        }

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
                        self.issues_loading = false;
                        self.prs_loading = false;
                    }
                },
                GhBgMessage::IssueList(result) => {
                    self.issues_loading = false;
                    match result {
                        Ok(issues) => {
                            disk_cache::save_issue_list(&issues);
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
                        Ok(prs) => {
                            disk_cache::save_pr_list(&prs);
                            self.prs = prs;
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
                    Ok(detail) => {
                        disk_cache::save_issue_detail(&detail);
                        self.issue_cache.insert(detail.number, detail.clone());
                        self.detail = GhDetailContent::Issue(Box::new(detail));
                    }
                    Err(e) => self.detail = GhDetailContent::Error(e),
                },
                GhBgMessage::PrDetail(result) => {
                    self.watch_in_flight_since = None;
                    match result {
                        Ok(detail) => {
                            self.watch_error = None;
                            disk_cache::save_pr_detail(&detail);
                            self.pr_cache.insert(detail.number, detail.clone());
                            self.detail = GhDetailContent::Pr(Box::new(detail));
                        }
                        Err(e) => {
                            if self.watch_mode {
                                // Keep current detail visible, show error in status bar
                                self.watch_error = Some(e);
                            } else {
                                self.detail = GhDetailContent::Error(e);
                            }
                        }
                    }
                }
            }
        }

        // Auto-load detail for the currently focused/selected list
        let on_pr = self.focused_pane == GhFocusedPane::PrList
            || (self.focused_pane == GhFocusedPane::Detail
                && self.previous_pane == GhFocusedPane::PrList);
        if on_pr {
            if pr_list_arrived {
                self.load_selected_pr_detail();
            }
        } else if issue_list_arrived {
            self.load_selected_issue_detail();
        }
    }

    /// Load issue detail — serves from cache if available, otherwise fetches in background.
    pub fn load_issue_detail(&mut self, number: u64) {
        if let Some(cached) = self.issue_cache.get(&number) {
            self.detail = GhDetailContent::Issue(Box::new(cached.clone()));
            self.reset_sub_panes();
            return;
        }
        // Disk cache fallback
        if let Some(cached) = disk_cache::load_issue_detail(number) {
            self.issue_cache.insert(number, cached.clone());
            self.detail = GhDetailContent::Issue(Box::new(cached));
            self.reset_sub_panes();
            return;
        }
        self.detail = GhDetailContent::Loading {
            kind: GhDetailKind::Issue,
            number,
        };
        self.reset_sub_panes();
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
            self.reset_sub_panes();
            return;
        }
        // Disk cache fallback
        if let Some(cached) = disk_cache::load_pr_detail(number) {
            self.pr_cache.insert(number, cached.clone());
            self.detail = GhDetailContent::Pr(Box::new(cached));
            self.reset_sub_panes();
            return;
        }
        self.detail = GhDetailContent::Loading {
            kind: GhDetailKind::Pr,
            number,
        };
        self.reset_sub_panes();
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

    /// Refresh only the currently displayed detail item (cache-bust + re-fetch).
    pub fn refresh_detail(&mut self) {
        let (kind, number) = match &self.detail {
            GhDetailContent::Issue(detail) => (GhDetailKind::Issue, detail.number),
            GhDetailContent::Pr(detail) => (GhDetailKind::Pr, detail.number),
            GhDetailContent::Loading { kind, number } => (*kind, *number),
            GhDetailContent::Error(_) => {
                match self.previous_pane {
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
                self.issue_cache.remove(&number);
                self.load_issue_detail(number);
            }
            GhDetailKind::Pr => {
                self.pr_cache.remove(&number);
                self.load_pr_detail(number);
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
        if !matches!(&self.detail, GhDetailContent::Pr(_)) {
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
        // Auto-disable if no longer on PR detail (but allow PR Loading state during fetch)
        if !matches!(
            &self.detail,
            GhDetailContent::Pr(_)
                | GhDetailContent::Loading {
                    kind: GhDetailKind::Pr,
                    ..
                }
        ) {
            self.watch_mode = false;
            return;
        }
        // Skip if a refresh is already in flight (timeout after 30s to self-heal)
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
    /// Unlike `refresh_detail()`, this keeps the current detail visible (no Loading state)
    /// and preserves scroll/selection positions.
    fn refresh_detail_silent(&mut self) {
        let number = match &self.detail {
            GhDetailContent::Pr(detail) => detail.number,
            _ => return,
        };
        self.pr_cache.remove(&number);
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

impl crate::core::pane::FocusState for GitHubState {
    type PaneId = GhFocusedPane;
    fn focused_pane(&self) -> GhFocusedPane {
        self.focused_pane
    }
    fn set_focus(&mut self, id: GhFocusedPane) {
        self.previous_pane = self.focused_pane;
        self.focused_pane = id;
    }
}

impl DetailState for GitHubState {
    type SubPaneId = GhDetailPane;
    fn active_sub_pane(&self) -> GhDetailPane {
        self.detail_pane
    }
    fn set_sub_pane(&mut self, id: GhDetailPane) {
        self.detail_pane = id;
    }
    fn sub_scroll(&self, id: GhDetailPane) -> &SubPaneScroll {
        match id {
            GhDetailPane::Body => &self.detail_body,
            GhDetailPane::Status => &self.detail_status,
            GhDetailPane::Reviews => &self.detail_reviews,
            GhDetailPane::Comments => &self.detail_comments,
        }
    }
    fn sub_scroll_mut(&mut self, id: GhDetailPane) -> &mut SubPaneScroll {
        match id {
            GhDetailPane::Body => &mut self.detail_body,
            GhDetailPane::Status => &mut self.detail_status,
            GhDetailPane::Reviews => &mut self.detail_reviews,
            GhDetailPane::Comments => &mut self.detail_comments,
        }
    }
    fn detail_view_height(&self) -> u16 {
        self.detail_view_height
    }
    fn set_detail_view_height(&mut self, h: u16) {
        self.detail_view_height = h;
    }
    fn reset_sub_panes(&mut self) {
        self.detail_pane = GhDetailPane::Body;
        self.detail_body.reset();
        self.detail_status.reset();
        self.detail_reviews.reset();
        self.detail_comments.reset();
    }
}

impl crate::core::app::PageState for GitHubState {
    fn drain_background(&mut self) {
        self.drain_bg_messages();
    }
    fn search(&self) -> &SearchState {
        &self.search
    }
    fn search_mut(&mut self) -> &mut SearchState {
        &mut self.search
    }
}
