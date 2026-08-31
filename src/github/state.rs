use crate::core::app::AppContext;
use crate::core::config::{Config, LoadedPageConfig};
use crate::core::keymap::{Keymap, ViewAction};
use crate::core::layout::{split_page_frame, PageLayoutConfig};
use crate::core::page::PageAction;
use crate::core::pane::{self, Pane, PaneEvent, PaneSet, PaneShared};
use crate::core::search::SearchState;
use crate::core::tab::Tab;
use crate::core::ui::status_bar;
use crate::github::domain::actions::types::{Job, WorkflowRun};
use crate::github::domain::client;
use crate::github::domain::types::*;
use crate::github::panes::detail_view::{DetailAction, GhDetailViewPane, GhRunDetail};
use crate::github::panes::gh_list::{GhListAction, GhListItem, GhListPane};
use crate::github::panes::issue_list::{self, GhIssueListPane};
use crate::github::panes::pr_list::{self, GhPrListPane};
use crate::github::panes::run_list::{self, GhRunListPane};
use anyhow::Result;
use crossterm::event::KeyEvent;
use ratatui::layout::Rect;
use ratatui::Frame;
use std::collections::HashMap;
use std::sync::mpsc;
use std::time::{Duration, Instant};

// === Rate-limit backoff ===

/// First polling suspension after a rate-limit rejection.
const RATE_LIMIT_BASE_BACKOFF: Duration = Duration::from_secs(30);
/// Longest polling suspension the backoff can grow to.
const RATE_LIMIT_MAX_BACKOFF: Duration = Duration::from_secs(600);

/// The delay following `current` in the 30s -> 60s -> ... -> 10min progression.
fn next_backoff(current: Option<Duration>) -> Duration {
    match current {
        None => RATE_LIMIT_BASE_BACKOFF,
        Some(d) => d.saturating_mul(2).min(RATE_LIMIT_MAX_BACKOFF),
    }
}

/// `resets in Nm` from the quota reset time (minutes round up), or
/// `resets soon` once it has passed.
fn format_reset(reset_at: i64, now: i64) -> String {
    if reset_at <= now {
        "resets soon".to_string()
    } else {
        format!("resets in {}m", (reset_at - now + 59) / 60)
    }
}

/// Polling suspension after GitHub rejected a fetch as rate-limited.
///
/// While `since.elapsed() < delay` every automatic poll of the page is
/// skipped. A rejection *after* the window (i.e. from a retry) doubles the
/// delay; any successful fetch clears the whole backoff. Manual `r` is
/// never gated.
struct RateLimitBackoff {
    since: Instant,
    delay: Duration,
    /// Unix time the quota resets (from `gh api rate_limit`, fetched once
    /// per episode); the status bar recomputes the remaining minutes.
    reset_at: Option<i64>,
}

// === Pane ID registry ===

pub struct GhPaneIds {
    pub issue_list: usize,
    pub pr_list: usize,
    pub run_list: usize,
    pub issue_detail: usize,
    pub pr_detail: usize,
    pub run_detail: usize,
}

impl GhPaneIds {
    fn from_config(cfg: &LoadedPageConfig) -> Self {
        Self {
            issue_list: cfg.resolve_id_expect("issue_list"),
            pr_list: cfg.resolve_id_expect("pr_list"),
            run_list: cfg.resolve_id_expect("run_list"),
            issue_detail: cfg.resolve_id_expect("issue_detail"),
            pr_detail: cfg.resolve_id_expect("pr_detail"),
            run_detail: cfg.resolve_id_expect("run_detail"),
        }
    }
}

#[cfg(test)]
use crate::core::layout::{LayoutNode, SlotCase, SlotRule, SplitDirection};
#[cfg(test)]
use ratatui::layout::Constraint;

#[derive(Debug, Clone)]
pub enum GhDetailContent {
    None,
    Loading {
        kind: GhDetailKind,
        number: u64,
    },
    Issue(Box<GhIssueDetail>),
    Pr(Box<GhPrDetail>),
    /// A workflow run: Jobs and Log sub-panes (not cached, polled while active).
    Run(Box<GhRunDetail>),
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GhDetailKind {
    Issue,
    Pr,
}

/// The sub-pane of the detail area that has focus. Issues use Body and
/// Comments, PRs Body / Status / Reviews / Comments, runs Jobs / Log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GhDetailPane {
    Body,
    Status,
    Reviews,
    Comments,
    Jobs,
    Log,
}

pub enum GhBgMessage {
    AuthStatus(Result<(), String>),
    IssueList(Result<Vec<GhIssueListItem>, String>),
    PrList(Result<Vec<GhPrListItem>, String>),
    RunList(Result<Vec<WorkflowRun>, String>),
    IssueDetail(Result<GhIssueDetail, String>),
    PrDetail(Result<GhPrDetail, String>),
    RunJobs {
        run_id: u64,
        result: Result<Vec<Job>, String>,
    },
    RunLog {
        request_id: u64,
        append: bool,
        result: Result<Vec<String>, String>,
    },
    /// Answer of the one-shot `gh api rate_limit` probe.
    RateLimitReset(Option<i64>),
}

/// The error string of a failed fetch carried by `msg`, if any.
fn fetch_error(msg: &GhBgMessage) -> Option<&str> {
    match msg {
        GhBgMessage::IssueList(Err(e))
        | GhBgMessage::PrList(Err(e))
        | GhBgMessage::RunList(Err(e))
        | GhBgMessage::IssueDetail(Err(e))
        | GhBgMessage::PrDetail(Err(e))
        | GhBgMessage::RunJobs { result: Err(e), .. }
        | GhBgMessage::RunLog { result: Err(e), .. } => Some(e),
        _ => None,
    }
}

/// Whether `msg` carries a successful fetch (the auth check and the
/// reset-time probe don't count as one).
fn fetch_succeeded(msg: &GhBgMessage) -> bool {
    matches!(
        msg,
        GhBgMessage::IssueList(Ok(_))
            | GhBgMessage::PrList(Ok(_))
            | GhBgMessage::RunList(Ok(_))
            | GhBgMessage::IssueDetail(Ok(_))
            | GhBgMessage::PrDetail(Ok(_))
            | GhBgMessage::RunJobs { result: Ok(_), .. }
            | GhBgMessage::RunLog { result: Ok(_), .. }
    )
}

/// Which of the three list → detail pairs the focus is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GhTab {
    Issue,
    Pr,
    Run,
}

// === Tab type aliases ===

pub type IssueTab = Tab<GhIssueListPane, GhDetailViewPane>;
pub type PrTab = Tab<GhPrListPane, GhDetailViewPane>;
pub type RunTab = Tab<GhRunListPane, GhDetailViewPane>;

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
        // A rate-limited fetch engages the page-wide backoff instead of
        // the error line (see `GitHubState::on_rate_limited`).
        Err(e) => {
            if !client::is_rate_limited(&e) && gh_error.is_none() {
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

impl RunTab {
    /// Sync DetailView to show the selected run (its jobs and log).
    pub fn sync_detail(&mut self, tx: &mpsc::Sender<GhBgMessage>) {
        if let Some(run) = self.list.selected_item().cloned() {
            self.detail.load_run(&run, tx);
        }
    }
}

// === GhPanes (grouping struct for disjoint borrows) ===

pub struct GhPanes {
    pub issue_tab: IssueTab,
    pub pr_tab: PrTab,
    pub run_tab: RunTab,
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
        let (il, id_, pl, pd, rl, rd) = (
            self.ids.issue_list,
            self.ids.issue_detail,
            self.ids.pr_list,
            self.ids.pr_detail,
            self.ids.run_list,
            self.ids.run_detail,
        );
        if idx == il || idx == id_ {
            self.issue_tab.get_pane_mut(il, id_, idx)
        } else if idx == pl || idx == pd {
            self.pr_tab.get_pane_mut(pl, pd, idx)
        } else if idx == rl || idx == rd {
            self.run_tab.get_pane_mut(rl, rd, idx)
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
    /// When the run list was last fetched (tick refresh while runs are active).
    last_runs_refresh: Option<Instant>,
    /// How often the page polls while something is active - the runs
    /// watch, the checks watch and the jobs / log polls
    /// (`github-poll-interval`).
    poll_interval: Duration,
    /// Active rate-limit backoff, if GitHub rejected a fetch.
    rate_limit: Option<RateLimitBackoff>,
    /// Whether entering a backoff probes `gh api rate_limit` for the reset
    /// time (tests turn it off).
    probe_reset: bool,
    layout_config: PageLayoutConfig,
    view_keymap: Keymap<ViewAction>,
}

impl GitHubState {
    pub fn new(cfg: &Config) -> Result<Self> {
        let page_cfg = cfg.github_page()?;
        let poll_interval = cfg.github_poll_interval()?;

        let ids = GhPaneIds::from_config(&page_cfg);
        // Build select→detail and reverse detail→select dispatch maps.
        let select_bindings = page_cfg.resolve_select_bindings();
        let detail_bindings: HashMap<usize, usize> =
            select_bindings.iter().map(|(&s, &d)| (d, s)).collect();

        // Build pane keymaps from KDL entries.
        let issue_list_km = page_cfg.keymap::<GhListAction>("issue_list")?;
        let pr_list_km = page_cfg.keymap::<GhListAction>("pr_list")?;
        let run_list_km = page_cfg.keymap::<GhListAction>("run_list")?;
        let issue_detail_km = page_cfg.keymap::<DetailAction>("issue_detail")?;
        let pr_detail_km = page_cfg.keymap::<DetailAction>("pr_detail")?;
        let run_detail_km = page_cfg.keymap::<DetailAction>("run_detail")?;
        let view_km = page_cfg.keymap::<ViewAction>("view")?;

        let mut issue_list = issue_list::new_pane(ids.issue_list, ids.issue_detail, ids.pr_list);
        issue_list.set_keymap(issue_list_km);

        let mut pr_list = pr_list::new_pane(ids.pr_list, ids.pr_detail, ids.issue_list);
        pr_list.set_keymap(pr_list_km);

        let mut run_list = run_list::new_pane(ids.run_list, ids.run_detail, ids.pr_list);
        run_list.set_keymap(run_list_km);

        let mut issue_detail = GhDetailViewPane::new(ids.issue_detail);
        issue_detail.set_keymap(issue_detail_km);

        let mut pr_detail = GhDetailViewPane::new(ids.pr_detail);
        pr_detail.set_keymap(pr_detail_km);

        let mut run_detail = GhDetailViewPane::new(ids.run_detail);
        run_detail.set_keymap(run_detail_km);

        // The checks watch and the jobs / log polls follow the configured
        // interval too.
        pr_detail.set_poll_interval(poll_interval);
        run_detail.set_poll_interval(poll_interval);

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
                run_tab: Tab {
                    list: run_list,
                    detail: run_detail,
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
            last_runs_refresh: None,
            poll_interval,
            rate_limit: None,
            probe_reset: true,
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
        self.panes.run_tab.list.initialize(&tx);
        self.last_runs_refresh = Some(Instant::now());

        // Auto-load detail for the first item from disk cache
        self.sync_active_detail();
    }

    /// Re-fetch the run list (initial load, `r`, tick refresh while active).
    fn spawn_runs(&mut self, tx: &mpsc::Sender<GhBgMessage>) {
        self.last_runs_refresh = Some(Instant::now());
        self.panes.run_tab.list.spawn_fetch(tx);
    }

    /// A fetch was rejected as rate-limited: start the backoff, or - when
    /// the rejection came from a retry after the current window - double
    /// it. Rejections inside the window (several fetches of one burst)
    /// leave it unchanged.
    fn on_rate_limited(&mut self) {
        match &mut self.rate_limit {
            Some(b) => {
                if b.since.elapsed() >= b.delay {
                    b.delay = next_backoff(Some(b.delay));
                    b.since = Instant::now();
                }
            }
            None => {
                self.rate_limit = Some(RateLimitBackoff {
                    since: Instant::now(),
                    delay: next_backoff(None),
                    reset_at: None,
                });
                if self.probe_reset {
                    if let Some(tx) = self.bg_tx.clone() {
                        std::thread::spawn(move || {
                            let _ = tx.send(GhBgMessage::RateLimitReset(
                                client::fetch_rate_limit_reset(),
                            ));
                        });
                    }
                }
            }
        }
    }

    /// Whether the backoff currently suspends automatic polling.
    fn polling_suspended(&self) -> bool {
        self.rate_limit
            .as_ref()
            .is_some_and(|b| b.since.elapsed() < b.delay)
    }

    /// Status-bar warning while a rate-limit backoff is active.
    pub fn rate_limit_notice(&self) -> Option<String> {
        let b = self.rate_limit.as_ref()?;
        Some(match b.reset_at {
            Some(reset) => format!(
                "\u{26a0} GitHub rate limited ({})",
                format_reset(reset, crate::github::domain::actions::time::now_secs())
            ),
            None => "\u{26a0} GitHub rate limited".to_string(),
        })
    }

    /// Drain background messages from worker threads.
    pub fn drain_bg_messages(&mut self) {
        let messages: Vec<_> = match &self.bg_rx {
            Some(rx) => rx.try_iter().collect(),
            None => return,
        };

        let mut issue_list_arrived = false;
        let mut pr_list_arrived = false;
        let mut run_list_arrived = false;
        let mut rate_limited = false;
        let mut fetch_ok = false;
        for msg in messages {
            match fetch_error(&msg) {
                Some(e) if client::is_rate_limited(e) => rate_limited = true,
                _ => fetch_ok |= fetch_succeeded(&msg),
            }
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
                        self.panes.run_tab.list.set_loading(false);
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
                GhBgMessage::RunList(result) => {
                    apply_list_result(
                        &mut self.panes.run_tab.list,
                        result,
                        &mut run_list_arrived,
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
                GhBgMessage::RunJobs { run_id, result } => {
                    if let Some(tx) = self.bg_tx.clone() {
                        self.panes.run_tab.detail.apply_jobs(run_id, result, &tx);
                    }
                }
                GhBgMessage::RunLog {
                    request_id,
                    append,
                    result,
                } => {
                    self.panes
                        .run_tab
                        .detail
                        .apply_log(request_id, append, result);
                }
                GhBgMessage::RateLimitReset(reset) => {
                    if let Some(b) = &mut self.rate_limit {
                        b.reset_at = reset;
                    }
                }
            }
        }

        if rate_limited {
            self.on_rate_limited();
        } else if fetch_ok {
            self.rate_limit = None;
        }

        // A fresh run list also carries the new state of the run on display
        // (jobs polling stops once it completes), whichever tab is active.
        if run_list_arrived {
            if let Some(tx) = self.bg_tx.clone() {
                self.panes.run_tab.sync_detail(&tx);
            }
        }
        // Auto-load detail when a fresh list arrives for the active tab
        let arrived = match self.active_tab() {
            GhTab::Issue => issue_list_arrived,
            GhTab::Pr => pr_list_arrived,
            GhTab::Run => run_list_arrived,
        };
        if arrived {
            self.sync_active_detail();
        }
    }

    /// The select pane of the pair the focus is in: the focused pane itself
    /// when it is a list, else the list its detail pane is bound to.
    fn active_select_id(&self) -> usize {
        let fp = self.pane.focused_pane;
        if self.select_bindings.contains_key(&fp) {
            fp
        } else {
            self.detail_bindings.get(&fp).copied().unwrap_or(fp)
        }
    }

    /// Which list → detail pair (issue, PR, run) the focus is in.
    /// Routes via the KDL-resolved `select_bindings` / `detail_bindings`.
    fn active_tab(&self) -> GhTab {
        let select_id = self.active_select_id();
        if select_id == self.panes.ids.pr_list {
            GhTab::Pr
        } else if select_id == self.panes.ids.run_list {
            GhTab::Run
        } else {
            GhTab::Issue
        }
    }

    /// The detail pane of the currently active tab.
    fn active_detail(&self) -> &GhDetailViewPane {
        match self.active_tab() {
            GhTab::Issue => &self.panes.issue_tab.detail,
            GhTab::Pr => &self.panes.pr_tab.detail,
            GhTab::Run => &self.panes.run_tab.detail,
        }
    }

    fn active_detail_mut(&mut self) -> &mut GhDetailViewPane {
        match self.active_tab() {
            GhTab::Issue => &mut self.panes.issue_tab.detail,
            GhTab::Pr => &mut self.panes.pr_tab.detail,
            GhTab::Run => &mut self.panes.run_tab.detail,
        }
    }

    /// Sync the active tab's detail view to its list selection.
    pub fn sync_active_detail(&mut self) {
        let tx = match &self.bg_tx {
            Some(tx) => tx,
            None => return,
        };
        match self.active_tab() {
            GhTab::Issue => self.panes.issue_tab.sync_detail(tx),
            GhTab::Pr => self.panes.pr_tab.sync_detail(tx),
            GhTab::Run => self.panes.run_tab.sync_detail(tx),
        }
    }

    /// Refresh only the currently displayed detail item (cache-bust + re-fetch;
    /// a run re-fetches its jobs and log).
    pub fn refresh_detail(&mut self) {
        let Some(tx) = self.bg_tx.clone() else {
            return;
        };
        if self.active_tab() == GhTab::Run {
            self.panes.run_tab.detail.refresh_run(&tx);
            return;
        }
        match self.active_detail().current_detail_info() {
            None => self.sync_active_detail(),
            Some((kind, number)) => {
                let dv = self.active_detail_mut();
                dv.invalidate(kind, number);
                dv.load(kind, number, &tx);
            }
        }
    }

    /// Refresh: re-fetch the issue, PR and run lists, clear caches, and
    /// re-fetch the shown run's jobs and log.
    pub fn refresh(&mut self) {
        self.gh_error = None;
        self.panes.issue_tab.detail.clear_caches();
        self.panes.pr_tab.detail.clear_caches();
        if let Some(tx) = self.bg_tx.clone() {
            self.panes.issue_tab.list.spawn_fetch(&tx);
            self.panes.pr_tab.list.spawn_fetch(&tx);
            self.spawn_runs(&tx);
            self.panes.run_tab.detail.refresh_run(&tx);
        }
    }

    /// Summary of the runs column for the status bar: `(runs, active)`.
    pub fn run_counts(&self) -> (usize, usize) {
        self.panes.run_tab.list.counts()
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
                PaneEvent::SetFocus(id) if self.detail_bindings.contains_key(&id) => {
                    self.sync_active_detail();
                }
                PaneEvent::SelectionChanged => {
                    self.sync_active_detail();
                }
                PaneEvent::OpenRunLog => {
                    if let Some(tx) = self.bg_tx.clone() {
                        self.panes.run_tab.detail.open_selected_log(&tx);
                    }
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
                        if self.select_bindings.contains_key(&origin) {
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
                let is_on_detail = self.detail_bindings.contains_key(&self.pane.focused_pane);
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
        entries.extend(help_section("Workflow Runs"));
        entries.extend(self.panes.run_tab.list.keymap().help_entries());
        entries.extend(help_section("Detail View"));
        entries.extend(self.panes.issue_tab.detail.keymap().help_entries());
        entries.extend(help_section("Run Detail (Jobs / Log)"));
        entries.extend(self.panes.run_tab.detail.keymap().help_entries());
        entries
    }

    fn handle_key(&mut self, ctx: &mut AppContext, key: KeyEvent) -> Result<PageAction> {
        // Search input mode intercepts all keys
        if self.pane.handle_search_input(&mut self.panes, ctx, key) {
            // A confirmed search moves a list selection without an event.
            let origin = self.pane.search.origin;
            if !self.pane.search.active && self.select_bindings.contains_key(&origin) {
                self.sync_active_detail();
            }
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
        let Some(tx) = self.bg_tx.clone() else {
            return;
        };
        // A rate-limit backoff suspends every automatic poll of the page
        // (checks watch, runs watch, jobs / log polls). Manual `r` still
        // goes through `refresh` / `refresh_detail` directly.
        if self.polling_suspended() {
            return;
        }
        self.panes.pr_tab.detail.handle_watch_tick(&tx);
        if self.gh_available != Some(true) {
            return;
        }
        // Runs column: refetch while anything is queued / in progress, like
        // the PR checks watch; the shown run polls its jobs and log itself.
        let due = self
            .last_runs_refresh
            .is_none_or(|t| t.elapsed() >= self.poll_interval);
        if self.panes.run_tab.list.has_active() && due && !self.panes.run_tab.list.is_loading() {
            self.spawn_runs(&tx);
        }
        self.panes.run_tab.detail.handle_run_tick(&tx);
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
    const GH_PANE_RUN_LIST: usize = 2;
    const GH_PANE_ISSUE_DETAIL: usize = 3;
    const GH_PANE_PR_DETAIL: usize = 4;
    const GH_PANE_RUN_DETAIL: usize = 5;
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
                                    Constraint::Percentage(33),
                                    LayoutNode::Pane(GH_PANE_ISSUE_LIST),
                                ),
                                (
                                    Constraint::Percentage(34),
                                    LayoutNode::Pane(GH_PANE_PR_LIST),
                                ),
                                (
                                    Constraint::Percentage(33),
                                    LayoutNode::Pane(GH_PANE_RUN_LIST),
                                ),
                            ],
                        },
                    ),
                    (Constraint::Min(3), LayoutNode::Slot(GH_SLOT_DETAIL)),
                ],
            },
            tab_panes: vec![GH_PANE_ISSUE_LIST, GH_PANE_PR_LIST, GH_PANE_RUN_LIST],
            slot_rules: vec![SlotRule {
                slot_id: GH_SLOT_DETAIL,
                cases: vec![
                    SlotCase {
                        trigger_panes: vec![GH_PANE_PR_LIST, GH_PANE_PR_DETAIL],
                        then_pane: GH_PANE_PR_DETAIL,
                    },
                    SlotCase {
                        trigger_panes: vec![GH_PANE_RUN_LIST, GH_PANE_RUN_DETAIL],
                        then_pane: GH_PANE_RUN_DETAIL,
                    },
                ],
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
        for focus in [
            GH_PANE_ISSUE_LIST,
            GH_PANE_PR_LIST,
            GH_PANE_RUN_LIST,
            GH_PANE_RUN_DETAIL,
        ] {
            let slots_hc = hardcoded.resolve_slots(focus);
            let slots_kd = from_kdl.resolve_slots(focus);
            let layout_hc = resolve_layout(area, &hardcoded.tree, &slots_hc);
            let layout_kd = resolve_layout(area, &from_kdl.tree, &slots_kd);
            assert_eq!(
                layout_hc, layout_kd,
                "layout resolution differs for focus {focus}"
            );
        }
        // Three equal columns on top, the detail slot below.
        let slots = from_kdl.resolve_slots(GH_PANE_RUN_LIST);
        let areas = resolve_layout(area, &from_kdl.tree, &slots);
        let width = |id: usize| areas.iter().find(|(p, _)| *p == id).unwrap().1.width;
        assert!(width(GH_PANE_ISSUE_LIST).abs_diff(width(GH_PANE_RUN_LIST)) <= 2);
        assert!(width(GH_PANE_PR_LIST).abs_diff(width(GH_PANE_RUN_LIST)) <= 2);
        assert!(areas.iter().any(|(p, _)| *p == GH_PANE_RUN_DETAIL));
        assert!(areas.iter().all(|(p, _)| *p != GH_PANE_PR_DETAIL));
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
        assert_eq!(r_hc.default_pane, r_kd.default_pane);
        assert_eq!(r_hc.cases.len(), r_kd.cases.len());
        for (c_hc, c_kd) in r_hc.cases.iter().zip(&r_kd.cases) {
            assert_eq!(c_hc.then_pane, c_kd.then_pane);
            let mut tp_hc = c_hc.trigger_panes.clone();
            tp_hc.sort();
            let mut tp_kd = c_kd.trigger_panes.clone();
            tp_kd.sort();
            assert_eq!(tp_hc, tp_kd);
        }
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
    fn run_list_keymap_matches() {
        use crate::github::panes::gh_list::default_keymap as gh_default;
        let hc = gh_default(KeyCode::BackTab);
        let entries = kdl_cfg().pane_keys;
        let kd: crate::core::keymap::Keymap<GhListAction> =
            build_keymap(entries["run_list"].as_slice()).unwrap();
        let test_keys = [
            "j", "k", "G", "g", "Ctrl+d", "Ctrl+u", "/", "n", "N", "i", "Enter", "BackTab", "o",
            "Esc",
        ];
        check_keys(&hc, &kd, &test_keys);
    }

    #[test]
    fn run_detail_keymap_has_log_keys_and_search() {
        let entries = kdl_cfg().pane_keys;
        let kd: crate::core::keymap::Keymap<DetailAction> =
            build_keymap(entries["run_detail"].as_slice()).unwrap();
        let expect = [
            ("Enter", "OpenLog"),
            ("i", "OpenLog"),
            ("]", "NextFailed"),
            ("[", "PrevFailed"),
            ("/", "Search(Start)"),
            ("n", "Search(Next)"),
            ("h", "FocusBody"),
            ("l", "FocusRight"),
            ("Tab", "CycleForward"),
            ("o", "OpenItem"),
            ("Esc", "Esc"),
        ];
        for (k, action) in expect {
            assert_eq!(
                kd.lookup(key(k)).map(|a| format!("{a:?}")).as_deref(),
                Some(action),
                "key {k}"
            );
        }
        assert!(kd.lookup(key("w")).is_none(), "watch mode is a PR thing");
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
        assert_eq!(ids.run_list, GH_PANE_RUN_LIST);
        assert_eq!(ids.issue_detail, GH_PANE_ISSUE_DETAIL);
        assert_eq!(ids.pr_detail, GH_PANE_PR_DETAIL);
        assert_eq!(ids.run_detail, GH_PANE_RUN_DETAIL);
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
            select_bindings.get(&ids.run_list),
            Some(&ids.run_detail),
            "select_bindings must map run_list→run_detail"
        );
        assert_eq!(
            select_bindings.len(),
            3,
            "expected exactly 3 select bindings"
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
        assert_eq!(
            detail_bindings.get(&ids.run_detail),
            Some(&ids.run_list),
            "detail_bindings must map run_detail→run_list"
        );
    }

    fn state() -> GitHubState {
        let mut st = GitHubState::new(&Config::builtin()).expect("github page");
        let (tx, _rx) = mpsc::channel();
        // Keep the receiver alive so worker threads can send without noise.
        std::mem::forget(_rx);
        st.bg_tx = Some(tx);
        st.probe_reset = false;
        st
    }

    #[test]
    fn active_tab_follows_focus_through_the_bindings() {
        let mut st = state();
        let ids = &st.panes.ids;
        let (il, pl, rl, id_, pd, rd) = (
            ids.issue_list,
            ids.pr_list,
            ids.run_list,
            ids.issue_detail,
            ids.pr_detail,
            ids.run_detail,
        );
        for (focus, tab) in [
            (il, GhTab::Issue),
            (id_, GhTab::Issue),
            (pl, GhTab::Pr),
            (pd, GhTab::Pr),
            (rl, GhTab::Run),
            (rd, GhTab::Run),
        ] {
            st.pane.focused_pane = focus;
            assert_eq!(st.active_tab(), tab, "focus {focus}");
        }
    }

    #[test]
    fn selecting_a_run_loads_its_jobs_into_the_run_detail() {
        let mut st = state();
        let ids = &st.panes.ids;
        let (rl, rd) = (ids.run_list, ids.run_detail);
        let runs = vec![
            crate::github::panes::run_list::tests::run(2, "in_progress", ""),
            crate::github::panes::run_list::tests::run(1, "completed", "success"),
        ];
        st.panes.run_tab.list.set_items(runs);
        st.pane.focused_pane = rl;
        st.sync_active_detail();
        let d = st.panes.run_tab.detail.run_detail().expect("run detail");
        assert_eq!(d.run.id, 2);
        assert!(d.jobs.run_is_active());
        assert_eq!(st.panes.run_tab.detail.active_pane, GhDetailPane::Jobs);
        assert!(st.panes.run_tab.list.has_active());
        assert_eq!(st.run_counts(), (2, 1));
        // Other detail panes are untouched.
        assert!(matches!(
            st.panes.issue_tab.detail.content,
            GhDetailContent::None
        ));
        // Enter (OpenDetail) focuses the run detail and keeps the same run.
        let mut ctx = crate::core::app::AppContext {
            should_quit: false,
            active_page: 0,
            page_labels: vec![],
            page_keys: vec![],
            show_help: false,
            status_message: None,
            error_dialog: None,
            workdir: std::path::PathBuf::new(),
            needs_full_redraw: false,
        };
        let events = st.dispatch_key(key("Enter"));
        assert!(matches!(events.as_slice(), [PaneEvent::SetFocus(id)] if *id == rd));
        st.process_events(&mut ctx, events).unwrap();
        assert_eq!(st.pane.focused_pane, rd);
        assert_eq!(st.panes.run_tab.detail.run_detail().unwrap().run.id, 2);
        // Esc goes back to the run list.
        let events = st.dispatch_key(key("Esc"));
        st.process_events(&mut ctx, events).unwrap();
        assert_eq!(st.pane.focused_pane, rl);
    }

    #[test]
    fn tab_in_a_detail_pane_cycles_its_sub_panes() {
        let mut st = state();
        let rl = st.panes.ids.run_list;
        let rd = st.panes.ids.run_detail;
        st.panes
            .run_tab
            .list
            .set_items(vec![crate::github::panes::run_list::tests::run(
                1,
                "completed",
                "success",
            )]);
        st.pane.focused_pane = rl;
        st.sync_active_detail();
        st.pane.set_focus(rd);
        assert_eq!(st.panes.run_tab.detail.active_pane, GhDetailPane::Jobs);
        // The view-level Tab (pane cycling) does not apply to detail panes.
        assert!(st.dispatch_key(key("Tab")).is_empty());
        assert_eq!(st.panes.run_tab.detail.active_pane, GhDetailPane::Log);
        assert!(st.dispatch_key(key("BackTab")).is_empty());
        assert_eq!(st.panes.run_tab.detail.active_pane, GhDetailPane::Jobs);
        assert!(st.dispatch_key(key("l")).is_empty());
        assert_eq!(st.panes.run_tab.detail.active_pane, GhDetailPane::Log);
        assert!(st.dispatch_key(key("h")).is_empty());
        assert_eq!(st.panes.run_tab.detail.active_pane, GhDetailPane::Jobs);
    }

    #[test]
    fn tab_cycles_the_three_list_columns() {
        let mut st = state();
        let ids = &st.panes.ids;
        let (il, pl, rl) = (ids.issue_list, ids.pr_list, ids.run_list);
        st.pane.focused_pane = il;
        for expected in [pl, rl, il] {
            let events = st.dispatch_key(key("Tab"));
            assert!(matches!(events.as_slice(), [PaneEvent::SetFocus(id)] if *id == expected));
            st.pane.set_focus(expected);
        }
        // `l` / `h` move between neighbouring columns and stop at the ends.
        st.pane.focused_pane = pl;
        let events = st.dispatch_key(key("l"));
        assert!(matches!(events.as_slice(), [PaneEvent::SetFocus(id)] if *id == rl));
        st.pane.set_focus(rl);
        assert!(st.dispatch_key(key("l")).is_empty());
    }
}

#[cfg(test)]
mod rate_limit {
    use super::*;
    use crate::core::app::PageState;

    const REST_LIMIT: &str =
        "HTTP 403: API rate limit exceeded for user ID 12345. (https://api.github.com/graphql)";

    fn ctx() -> AppContext {
        AppContext {
            should_quit: false,
            active_page: 0,
            page_labels: vec![],
            page_keys: vec![],
            show_help: false,
            status_message: None,
            error_dialog: None,
            workdir: std::path::PathBuf::new(),
            needs_full_redraw: false,
        }
    }

    /// A state with a live channel, no reset probe and no worker threads.
    fn state() -> (GitHubState, mpsc::Sender<GhBgMessage>) {
        let mut st = GitHubState::new(&Config::builtin()).expect("github page");
        let (tx, rx) = mpsc::channel();
        st.bg_tx = Some(tx.clone());
        st.bg_rx = Some(rx);
        st.probe_reset = false;
        (st, tx)
    }

    #[test]
    fn backoff_progression_doubles_to_the_cap() {
        let mut d = next_backoff(None);
        assert_eq!(d, Duration::from_secs(30));
        let mut seen = vec![];
        for _ in 0..6 {
            d = next_backoff(Some(d));
            seen.push(d.as_secs());
        }
        assert_eq!(seen, [60, 120, 240, 480, 600, 600]);
    }

    #[test]
    fn reset_time_formatting() {
        assert_eq!(format_reset(1_700, 1_000), "resets in 12m");
        assert_eq!(format_reset(1_060, 1_000), "resets in 1m");
        assert_eq!(format_reset(1_061, 1_000), "resets in 2m");
        assert_eq!(format_reset(1_000, 1_000), "resets soon");
        assert_eq!(format_reset(500, 1_000), "resets soon");
    }

    #[test]
    fn a_rate_limited_fetch_starts_the_backoff_not_the_error_line() {
        let (mut st, tx) = state();
        tx.send(GhBgMessage::RunList(Err(REST_LIMIT.into())))
            .unwrap();
        tx.send(GhBgMessage::PrList(Err(REST_LIMIT.into())))
            .unwrap();
        st.drain_bg_messages();
        assert!(st.polling_suspended());
        assert_eq!(st.gh_error, None, "rate limits show the warning instead");
        assert!(!st.panes.run_tab.list.is_loading());
        assert_eq!(
            st.rate_limit.as_ref().unwrap().delay,
            Duration::from_secs(30)
        );
        assert_eq!(
            st.rate_limit_notice().as_deref(),
            Some("\u{26a0} GitHub rate limited")
        );
        // The reset probe answered: the notice shows the remaining minutes.
        let reset = crate::github::domain::actions::time::now_secs() + 700;
        tx.send(GhBgMessage::RateLimitReset(Some(reset))).unwrap();
        st.drain_bg_messages();
        let notice = st.rate_limit_notice().unwrap();
        assert!(
            notice.contains("GitHub rate limited (resets in"),
            "{notice}"
        );
    }

    #[test]
    fn a_failed_retry_doubles_the_delay_and_success_clears_it() {
        let (mut st, tx) = state();
        tx.send(GhBgMessage::RunList(Err(REST_LIMIT.into())))
            .unwrap();
        st.drain_bg_messages();
        // Same burst: a second rejection inside the window changes nothing.
        tx.send(GhBgMessage::IssueList(Err(REST_LIMIT.into())))
            .unwrap();
        st.drain_bg_messages();
        assert_eq!(
            st.rate_limit.as_ref().unwrap().delay,
            Duration::from_secs(30)
        );
        // The window passed and the retry was rejected again: 30s -> 60s.
        {
            let b = st.rate_limit.as_mut().unwrap();
            b.since = Instant::now() - b.delay;
        }
        assert!(!st.polling_suspended());
        tx.send(GhBgMessage::RunList(Err(REST_LIMIT.into())))
            .unwrap();
        st.drain_bg_messages();
        assert_eq!(
            st.rate_limit.as_ref().unwrap().delay,
            Duration::from_secs(60)
        );
        assert!(st.polling_suspended());
        // A successful fetch clears the backoff.
        tx.send(GhBgMessage::RunList(Ok(vec![]))).unwrap();
        st.drain_bg_messages();
        assert!(st.rate_limit.is_none());
        assert!(st.rate_limit_notice().is_none());
    }

    #[test]
    fn other_errors_still_reach_the_error_line() {
        let (mut st, tx) = state();
        tx.send(GhBgMessage::IssueList(Err("HTTP 404: Not Found".into())))
            .unwrap();
        st.drain_bg_messages();
        assert!(st.rate_limit.is_none());
        assert_eq!(st.gh_error.as_deref(), Some("HTTP 404: Not Found"));
    }

    #[test]
    fn the_backoff_suspends_the_runs_watch() {
        let (mut st, _tx) = state();
        st.gh_available = Some(true);
        st.panes
            .run_tab
            .list
            .set_items(vec![crate::github::panes::run_list::tests::run(
                2,
                "in_progress",
                "",
            )]);
        st.last_runs_refresh = None;
        st.rate_limit = Some(RateLimitBackoff {
            since: Instant::now(),
            delay: Duration::from_secs(30),
            reset_at: None,
        });
        let mut ctx = ctx();
        st.on_tick(&mut ctx);
        assert!(!st.panes.run_tab.list.is_loading(), "suspended: no refetch");
        st.rate_limit = None;
        st.on_tick(&mut ctx);
        assert!(
            st.panes.run_tab.list.is_loading(),
            "backoff cleared: the watch resumes"
        );
    }
}
