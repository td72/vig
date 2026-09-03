pub(crate) mod jobs;
pub(crate) mod log;
pub(crate) mod view;

use crate::core::app::AppContext;
use crate::core::keymap::{
    half_page_step, nav_bindings, ActionHelp, Keymap, NavAction, SearchAction,
};
use crate::core::pane::{self, Pane, PaneEvent, PaneShared, SubPaneScroll};
use crate::core::search::SearchMatch;
use crate::github::domain::actions::types::{Job, WorkflowRun};
use crate::github::domain::types::*;
use crate::github::domain::{client, disk_cache};
use crate::github::state::{GhBgMessage, GhDetailContent, GhDetailKind, GhDetailPane};
use crossterm::event::{KeyCode, KeyEvent};
use jobs::JobsView;
use log::LogView;
use ratatui::{layout::Rect, Frame};
use std::collections::HashMap;
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime};

/// A workflow run shown in the detail area: the run itself (from the list)
/// plus the Jobs and Log sub-panes. Unlike issues and PRs it is not cached;
/// the jobs are fetched when the run is selected and polled while it runs.
#[derive(Debug, Clone)]
pub struct GhRunDetail {
    pub run: WorkflowRun,
    pub jobs: JobsView,
    pub log: LogView,
}

/// Abstraction over the two detail payload types (issue, PR) so that load/apply
/// logic can be written once over `D: DetailType`.
pub trait DetailType: Sized + Clone + Send + 'static {
    const KIND: GhDetailKind;
    fn number(&self) -> u64;
    fn save_to_disk(&self);
    fn load_from_disk(number: u64) -> Option<Self>;
    fn fetch(number: u64) -> Result<Self, String>;
    fn into_content(self) -> GhDetailContent;
    fn to_bg_message(result: Result<Self, String>) -> GhBgMessage;
    fn cache_of(pane: &mut GhDetailViewPane) -> &mut HashMap<u64, Self>;
}

impl DetailType for GhIssueDetail {
    const KIND: GhDetailKind = GhDetailKind::Issue;
    fn number(&self) -> u64 {
        self.number
    }
    fn save_to_disk(&self) {
        disk_cache::save_issue_detail(self);
    }
    fn load_from_disk(number: u64) -> Option<Self> {
        disk_cache::load_issue_detail(number)
    }
    fn fetch(number: u64) -> Result<Self, String> {
        client::get_issue(number)
    }
    fn into_content(self) -> GhDetailContent {
        GhDetailContent::Issue(Box::new(self))
    }
    fn to_bg_message(result: Result<Self, String>) -> GhBgMessage {
        GhBgMessage::IssueDetail(result)
    }
    fn cache_of(pane: &mut GhDetailViewPane) -> &mut HashMap<u64, Self> {
        &mut pane.issue_cache
    }
}

impl DetailType for GhPrDetail {
    const KIND: GhDetailKind = GhDetailKind::Pr;
    fn number(&self) -> u64 {
        self.number
    }
    fn save_to_disk(&self) {
        disk_cache::save_pr_detail(self);
    }
    fn load_from_disk(number: u64) -> Option<Self> {
        disk_cache::load_pr_detail(number)
    }
    fn fetch(number: u64) -> Result<Self, String> {
        client::get_pr(number)
    }
    fn into_content(self) -> GhDetailContent {
        GhDetailContent::Pr(Box::new(self))
    }
    fn to_bg_message(result: Result<Self, String>) -> GhBgMessage {
        GhBgMessage::PrDetail(result)
    }
    fn cache_of(pane: &mut GhDetailViewPane) -> &mut HashMap<u64, Self> {
        &mut pane.pr_cache
    }
}

/// Watch mode status for display in the status bar.
pub struct WatchStatus {
    pub last_update_time: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub enum DetailAction {
    Nav(NavAction),
    Search(SearchAction),
    FocusBody,
    FocusRight,
    CycleForward,
    CycleBackward,
    ToggleWatch,
    OpenItem,
    /// Copy the shown issue / PR / run's URL (built locally).
    CopyUrl,
    /// Run detail: show the selected job's log (a step row scrolls to it).
    OpenLog,
    /// Run detail: jump to the next / previous failed step in the log.
    NextFailed,
    PrevFailed,
    Esc,
}

crate::impl_pane_action_from_str!(
    DetailAction, nav: Nav, search: Search, esc: Esc,
    FocusBody, FocusRight, CycleForward, CycleBackward, ToggleWatch, OpenItem,
    CopyUrl, OpenLog, NextFailed, PrevFailed
);

impl ActionHelp for DetailAction {
    fn label(&self) -> Option<&'static str> {
        match self {
            DetailAction::Nav(nav) => nav.label(),
            DetailAction::Search(sa) => sa.label(),
            DetailAction::FocusBody => Some("Body / Jobs pane"),
            DetailAction::FocusRight => Some("Right / Log pane"),
            DetailAction::CycleForward => Some("Next right pane"),
            DetailAction::CycleBackward => Some("Prev right pane"),
            DetailAction::ToggleWatch => Some("Toggle watch mode"),
            DetailAction::OpenItem => Some("Open in browser"),
            DetailAction::CopyUrl => Some("Copy item URL"),
            DetailAction::OpenLog => Some("Open job log"),
            DetailAction::NextFailed => Some("Next failed step"),
            DetailAction::PrevFailed => Some("Prev failed step"),
            DetailAction::Esc => Some("Back to list"),
        }
    }
}

pub fn default_keymap() -> Keymap<DetailAction> {
    Keymap::new()
        .bindings(nav_bindings(DetailAction::Nav))
        .key(KeyCode::Char('h'), DetailAction::FocusBody)
        .key(KeyCode::Char('l'), DetailAction::FocusRight)
        .key(KeyCode::Tab, DetailAction::CycleForward)
        .key(KeyCode::BackTab, DetailAction::CycleBackward)
        .key(KeyCode::Char('w'), DetailAction::ToggleWatch)
        .key(KeyCode::Char('o'), DetailAction::OpenItem)
        .key(KeyCode::Char('y'), DetailAction::CopyUrl)
        .key(KeyCode::Esc, DetailAction::Esc)
}

pub struct GhDetailViewPane {
    pub pane_id: usize,
    pub content: GhDetailContent,
    pub active_pane: GhDetailPane,
    pub body: SubPaneScroll,
    pub status: SubPaneScroll,
    pub reviews: SubPaneScroll,
    pub comments: SubPaneScroll,
    pub view_height: u16,
    pub(crate) issue_cache: HashMap<u64, GhIssueDetail>,
    pub(crate) pr_cache: HashMap<u64, GhPrDetail>,
    // Watch mode
    pub watch_mode: bool,
    watch_last_refresh: Option<Instant>,
    watch_last_update: Option<SystemTime>,
    watch_in_flight_since: Option<Instant>,
    pub watch_error: Option<String>,
    /// How often the checks watch and the jobs / log polls fire
    /// (`github-poll-interval`).
    poll_interval: Duration,
    keymap: Keymap<DetailAction>,
}

impl GhDetailViewPane {
    pub fn new(pane_id: usize) -> Self {
        Self {
            pane_id,
            content: GhDetailContent::None,
            active_pane: GhDetailPane::Body,
            body: SubPaneScroll::default(),
            status: SubPaneScroll::default(),
            reviews: SubPaneScroll::default(),
            comments: SubPaneScroll::default(),
            view_height: 0,
            issue_cache: HashMap::new(),
            pr_cache: HashMap::new(),
            watch_mode: false,
            watch_last_refresh: None,
            watch_last_update: None,
            watch_in_flight_since: None,
            watch_error: None,
            poll_interval: jobs::JOBS_POLL_INTERVAL,
            keymap: default_keymap(),
        }
    }

    pub fn set_keymap(&mut self, km: Keymap<DetailAction>) {
        self.keymap = km;
    }

    /// How often the checks watch and the jobs / log polls fire
    /// (`github-poll-interval`).
    pub fn set_poll_interval(&mut self, interval: Duration) {
        self.poll_interval = interval;
    }

    pub fn keymap(&self) -> &Keymap<DetailAction> {
        &self.keymap
    }

    /// Set the content to an error state.
    pub fn set_error(&mut self, msg: String) {
        self.content = GhDetailContent::Error(msg);
    }

    /// Return the kind and number of the currently displayed or loading
    /// issue / PR, if any. Runs are not cached and report `None`.
    pub fn current_detail_info(&self) -> Option<(GhDetailKind, u64)> {
        match &self.content {
            GhDetailContent::Issue(detail) => Some((GhDetailKind::Issue, detail.number)),
            GhDetailContent::Pr(detail) => Some((GhDetailKind::Pr, detail.number)),
            GhDetailContent::Loading { kind, number } => Some((*kind, *number)),
            GhDetailContent::Run(_) | GhDetailContent::Error(_) | GhDetailContent::None => None,
        }
    }

    pub fn is_pr(&self) -> bool {
        matches!(&self.content, GhDetailContent::Pr(_))
    }

    pub fn run_detail(&self) -> Option<&GhRunDetail> {
        match &self.content {
            GhDetailContent::Run(d) => Some(d),
            _ => None,
        }
    }

    pub fn run_detail_mut(&mut self) -> Option<&mut GhRunDetail> {
        match &mut self.content {
            GhDetailContent::Run(d) => Some(d),
            _ => None,
        }
    }

    pub fn active_scroll_mut(&mut self) -> &mut SubPaneScroll {
        match self.active_pane {
            GhDetailPane::Body | GhDetailPane::Jobs => &mut self.body,
            GhDetailPane::Status => &mut self.status,
            GhDetailPane::Reviews => &mut self.reviews,
            GhDetailPane::Comments | GhDetailPane::Log => &mut self.comments,
        }
    }

    // === Workflow runs ===

    /// Show `run`: a run already on display only picks up its new state
    /// (the list was refreshed), anything else starts a fresh Jobs / Log
    /// pair and fetches the jobs.
    pub fn load_run(&mut self, run: &WorkflowRun, tx: &mpsc::Sender<GhBgMessage>) {
        if let Some(d) = self.run_detail_mut() {
            if d.run.id == run.id {
                d.run = run.clone();
                d.jobs.update_run(run);
                return;
            }
        }
        self.content = GhDetailContent::Run(Box::new(GhRunDetail {
            run: run.clone(),
            jobs: JobsView::new(run, tx),
            log: LogView::new(),
        }));
        self.reset_sub_panes();
        self.active_pane = GhDetailPane::Jobs;
    }

    /// A jobs fetch answered: update the rows and tell the log whether its
    /// job finished (results for another run are dropped).
    pub fn apply_jobs(
        &mut self,
        run_id: u64,
        result: Result<Vec<Job>, String>,
        tx: &mpsc::Sender<GhBgMessage>,
    ) {
        let Some(d) = self.run_detail_mut() else {
            return;
        };
        d.jobs.apply(run_id, result);
        if let Some(job_id) = d.log.target().map(|t| t.job_id) {
            if let Some(latest) = d.jobs.target_for(job_id) {
                d.log.update_target(latest, tx);
            }
        }
    }

    /// A log fetch answered.
    pub fn apply_log(
        &mut self,
        request_id: u64,
        append: bool,
        result: Result<Vec<String>, String>,
    ) {
        if let Some(d) = self.run_detail_mut() {
            d.log.apply(request_id, append, result);
        }
    }

    /// `Enter` on a job or step: load its log and focus the Log sub-pane.
    pub fn open_selected_log(&mut self, tx: &mpsc::Sender<GhBgMessage>) {
        let Some(d) = self.run_detail_mut() else {
            return;
        };
        if let Some((target, step)) = d.jobs.selected_target() {
            d.log.load(target, step, tx);
            self.active_pane = GhDetailPane::Log;
        }
    }

    /// Tick: poll the jobs of a queued / running run and a running job's log.
    pub fn handle_run_tick(&mut self, tx: &mpsc::Sender<GhBgMessage>) {
        let interval = self.poll_interval;
        if let Some(d) = self.run_detail_mut() {
            d.jobs.on_tick(tx, interval);
            d.log.on_tick(tx, interval);
        }
    }

    /// `r`: re-fetch the jobs and the log.
    pub fn refresh_run(&mut self, tx: &mpsc::Sender<GhBgMessage>) {
        if let Some(d) = self.run_detail_mut() {
            d.jobs.spawn_fetch(tx);
            d.log.refresh(tx);
        }
    }

    /// Whether a jobs or log fetch is outstanding for the shown run.
    pub fn is_run_loading(&self) -> bool {
        self.run_detail()
            .is_some_and(|d| d.jobs.is_loading() || d.log.is_loading())
    }

    /// Run detail: handle the actions that behave differently there. `None`
    /// hands the action to the common issue / PR path.
    fn execute_run(&mut self, action: &DetailAction) -> Option<Vec<PaneEvent>> {
        let active = self.active_pane;
        let d = self.run_detail_mut()?;
        let events = match action {
            DetailAction::Nav(nav) => {
                match active {
                    GhDetailPane::Log => d.log.nav(*nav),
                    _ => {
                        d.jobs.nav(*nav);
                    }
                }
                vec![]
            }
            DetailAction::FocusBody => {
                self.active_pane = GhDetailPane::Jobs;
                vec![]
            }
            DetailAction::FocusRight => {
                self.active_pane = GhDetailPane::Log;
                vec![]
            }
            DetailAction::CycleForward | DetailAction::CycleBackward => {
                self.active_pane = match active {
                    GhDetailPane::Log => GhDetailPane::Jobs,
                    _ => GhDetailPane::Log,
                };
                vec![]
            }
            DetailAction::OpenLog => vec![PaneEvent::OpenRunLog],
            DetailAction::NextFailed => d.log.jump_failed(true),
            DetailAction::PrevFailed => d.log.jump_failed(false),
            DetailAction::OpenItem => {
                let url = match active {
                    GhDetailPane::Log => d.log.target().map(|t| t.url.clone()),
                    _ => d.jobs.selected_url(),
                }
                .filter(|u| !u.is_empty())
                .unwrap_or_else(|| d.run.url.clone());
                if url.is_empty() {
                    vec![]
                } else {
                    vec![PaneEvent::OpenUrl(url)]
                }
            }
            DetailAction::CopyUrl => {
                vec![crate::github::panes::gh_list::copy_url_event(Some(
                    d.run.url.clone(),
                ))]
            }
            DetailAction::ToggleWatch => vec![],
            DetailAction::Search(_) | DetailAction::Esc => return None,
        };
        Some(events)
    }

    /// Cycle right-side panes forward (Status → Reviews → Comments → Status).
    fn cycle_right_pane_forward(&mut self) {
        if self.is_pr() {
            self.active_pane = match self.active_pane {
                GhDetailPane::Status => GhDetailPane::Reviews,
                GhDetailPane::Reviews => GhDetailPane::Comments,
                GhDetailPane::Comments => GhDetailPane::Status,
                other => other,
            };
        }
    }

    /// Cycle right-side panes backward (Status → Comments → Reviews → Status).
    fn cycle_right_pane_backward(&mut self) {
        if self.is_pr() {
            self.active_pane = match self.active_pane {
                GhDetailPane::Status => GhDetailPane::Comments,
                GhDetailPane::Reviews => GhDetailPane::Status,
                GhDetailPane::Comments => GhDetailPane::Reviews,
                other => other,
            };
        }
    }

    pub fn reset_sub_panes(&mut self) {
        self.active_pane = GhDetailPane::Body;
        self.body.reset();
        self.status.reset();
        self.reviews.reset();
        self.comments.reset();
    }

    /// Load issue/PR detail — serves from cache if available, otherwise fetches in background.
    pub fn load(&mut self, kind: GhDetailKind, number: u64, tx: &mpsc::Sender<GhBgMessage>) {
        match kind {
            GhDetailKind::Issue => self.load_typed::<GhIssueDetail>(number, tx),
            GhDetailKind::Pr => self.load_typed::<GhPrDetail>(number, tx),
        }
    }

    fn load_typed<D: DetailType>(&mut self, number: u64, tx: &mpsc::Sender<GhBgMessage>) {
        // Already loading this exact item — skip duplicate request
        if matches!(
            &self.content,
            GhDetailContent::Loading { kind: k, number: n } if *k == D::KIND && *n == number,
        ) {
            return;
        }

        // Check in-memory cache
        if let Some(cached) = D::cache_of(self).get(&number).cloned() {
            self.content = cached.into_content();
            self.reset_sub_panes();
            return;
        }

        // Check disk cache
        if let Some(from_disk) = D::load_from_disk(number) {
            D::cache_of(self).insert(number, from_disk.clone());
            self.content = from_disk.into_content();
            self.reset_sub_panes();
            return;
        }

        // Fetch in background
        self.content = GhDetailContent::Loading {
            kind: D::KIND,
            number,
        };
        self.reset_sub_panes();
        let tx = tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(D::to_bg_message(D::fetch(number)));
        });
    }

    pub fn clear_caches(&mut self) {
        self.issue_cache.clear();
        self.pr_cache.clear();
    }

    pub fn invalidate(&mut self, kind: GhDetailKind, number: u64) {
        match kind {
            GhDetailKind::Issue => {
                self.issue_cache.remove(&number);
            }
            GhDetailKind::Pr => {
                self.pr_cache.remove(&number);
            }
        }
    }

    /// Apply a fetched issue/PR detail — save to disk cache, memoize, and display.
    pub fn apply_detail<D: DetailType>(&mut self, detail: D) {
        detail.save_to_disk();
        D::cache_of(self).insert(detail.number(), detail.clone());
        self.content = detail.into_content();
    }

    /// Apply a PR detail fetch result, handling watch-mode error semantics.
    pub fn apply_pr_detail_result(&mut self, result: Result<GhPrDetail, String>) {
        self.watch_in_flight_since = None;
        match result {
            Ok(detail) => {
                self.watch_error = None;
                self.apply_detail(detail);
            }
            Err(e) => {
                if self.watch_mode {
                    self.watch_error = Some(e);
                } else {
                    self.content = GhDetailContent::Error(e);
                }
            }
        }
    }

    // === Watch mode ===

    /// Returns the wall-clock time of the last watch refresh as "HH:MM:SS", if active.
    pub fn watch_last_update_time(&self) -> Option<String> {
        if !self.watch_mode {
            return None;
        }
        self.watch_last_update.map(|t| {
            let secs = t
                .duration_since(SystemTime::UNIX_EPOCH)
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

    /// Returns watch status for display in the status bar, if watch mode is active.
    pub fn watch_status(&self) -> Option<WatchStatus> {
        let last_update_time = self.watch_last_update_time()?;
        Some(WatchStatus {
            last_update_time,
            error: self.watch_error.clone(),
        })
    }

    /// Toggle watch mode (auto-refresh checks at the poll interval). Only activates on PR detail.
    pub fn toggle_watch_mode(&mut self) {
        if !self.is_pr() {
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

    /// Called on every tick. If watch mode is active and the poll interval has elapsed, refresh the detail.
    pub fn handle_watch_tick(&mut self, tx: &mpsc::Sender<GhBgMessage>) {
        if !self.watch_mode {
            return;
        }
        if !matches!(
            &self.content,
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
            if last.elapsed() >= self.poll_interval {
                self.watch_last_refresh = Some(Instant::now());
                self.watch_last_update = Some(SystemTime::now());
                self.refresh_silent(tx);
            }
        }
    }

    /// Silently re-fetch the current PR detail in the background.
    fn refresh_silent(&mut self, tx: &mpsc::Sender<GhBgMessage>) {
        let number = match &self.content {
            GhDetailContent::Pr(detail) => detail.number,
            _ => return,
        };
        self.invalidate(GhDetailKind::Pr, number);
        self.watch_in_flight_since = Some(Instant::now());
        let tx = tx.clone();
        std::thread::spawn(move || {
            let result = client::get_pr(number);
            let _ = tx.send(GhBgMessage::PrDetail(result));
        });
    }

    fn handle_key_impl(&mut self, shared: &PaneShared, key: KeyEvent) -> Vec<PaneEvent> {
        let action = match self.keymap.lookup(key) {
            Some(a) => a.clone(),
            None => return vec![],
        };
        self.execute(shared, action)
    }

    fn execute(&mut self, shared: &PaneShared, action: DetailAction) -> Vec<PaneEvent> {
        // Search and Esc (clear search, else back to the list) are shared.
        let back = vec![PaneEvent::SetFocus(shared.previous_pane)];
        if let Some(events) = pane::try_dispatch_search_esc(&action, shared, self.pane_id, back) {
            return events;
        }
        if let Some(events) = self.execute_run(&action) {
            return events;
        }
        // Determine item count for selection-based panes
        let pane = self.active_pane;
        let item_count = self.active_item_count();
        let selectable = pane != GhDetailPane::Body;

        match action {
            DetailAction::Nav(NavAction::MoveDown) => {
                if selectable && item_count > 0 {
                    let s = self.active_scroll_mut();
                    if s.selected_idx + 1 < item_count {
                        s.selected_idx += 1;
                        s.scroll_y = 0;
                    } else {
                        s.scroll_y = s.scroll_y.saturating_add(1);
                    }
                } else if !selectable {
                    let s = self.active_scroll_mut();
                    s.scroll_y = s.scroll_y.saturating_add(1);
                }
            }
            DetailAction::Nav(NavAction::MoveUp) => {
                if selectable {
                    let s = self.active_scroll_mut();
                    if s.scroll_y > 0 {
                        s.scroll_y -= 1;
                    } else {
                        s.selected_idx = s.selected_idx.saturating_sub(1);
                    }
                } else {
                    let s = self.active_scroll_mut();
                    s.scroll_y = s.scroll_y.saturating_sub(1);
                }
            }
            DetailAction::Nav(NavAction::HalfPageDown) => {
                let half = half_page_step(self.view_height);
                let s = self.active_scroll_mut();
                s.scroll_y = s.scroll_y.saturating_add(half);
            }
            DetailAction::Nav(NavAction::HalfPageUp) => {
                let half = half_page_step(self.view_height);
                let s = self.active_scroll_mut();
                s.scroll_y = s.scroll_y.saturating_sub(half);
            }
            DetailAction::Nav(NavAction::JumpTop) => {
                let s = self.active_scroll_mut();
                if selectable {
                    s.selected_idx = 0;
                }
                s.scroll_y = 0;
            }
            DetailAction::Nav(NavAction::JumpBottom) => {
                let s = self.active_scroll_mut();
                if selectable && item_count > 0 {
                    s.selected_idx = item_count - 1;
                }
                if !selectable || item_count > 0 {
                    s.scroll_y = u16::MAX / 2;
                }
            }
            DetailAction::FocusBody => {
                self.active_pane = GhDetailPane::Body;
            }
            DetailAction::FocusRight => match self.active_pane {
                GhDetailPane::Body => {
                    if self.is_pr() {
                        self.active_pane = GhDetailPane::Status;
                    } else {
                        self.active_pane = GhDetailPane::Comments;
                    }
                }
                _ => self.cycle_right_pane_forward(),
            },
            DetailAction::CycleForward => self.cycle_right_pane_forward(),
            DetailAction::CycleBackward => self.cycle_right_pane_backward(),
            DetailAction::ToggleWatch => {
                self.toggle_watch_mode();
            }
            DetailAction::OpenItem => {
                return self.open_detail_item();
            }
            DetailAction::CopyUrl => {
                let url = match &self.content {
                    GhDetailContent::Issue(d) => client::issue_url(d.number),
                    GhDetailContent::Pr(d) => client::pr_url(d.number),
                    _ => None,
                };
                return vec![crate::github::panes::gh_list::copy_url_event(url)];
            }
            // Handled above (search / esc) or only meaningful for runs.
            DetailAction::Search(_)
            | DetailAction::Esc
            | DetailAction::OpenLog
            | DetailAction::NextFailed
            | DetailAction::PrevFailed => {}
        }
        vec![]
    }

    fn active_item_count(&self) -> usize {
        match self.active_pane {
            GhDetailPane::Status => {
                if let GhDetailContent::Pr(ref detail) = self.content {
                    view::sorted_checks(detail).len()
                } else {
                    0
                }
            }
            GhDetailPane::Reviews => {
                if let GhDetailContent::Pr(ref detail) = self.content {
                    view::meaningful_reviews(&detail.reviews).len()
                } else {
                    0
                }
            }
            GhDetailPane::Comments => match &self.content {
                GhDetailContent::Issue(detail) => detail.comments.len(),
                GhDetailContent::Pr(detail) => detail.comments.len(),
                _ => 0,
            },
            GhDetailPane::Body | GhDetailPane::Jobs | GhDetailPane::Log => 0,
        }
    }

    fn open_detail_item(&self) -> Vec<PaneEvent> {
        let url: Option<String> = match self.active_pane {
            GhDetailPane::Status => {
                if let GhDetailContent::Pr(ref detail) = self.content {
                    let sorted = view::sorted_checks(detail);
                    sorted
                        .get(self.status.selected_idx)
                        .and_then(|c| c.details_url.clone())
                } else {
                    None
                }
            }
            GhDetailPane::Reviews => {
                if let GhDetailContent::Pr(ref detail) = self.content {
                    let reviews = view::meaningful_reviews(&detail.reviews);
                    reviews.get(self.reviews.selected_idx).and_then(|r| {
                        r.id.as_ref().and_then(|id| {
                            crate::github::domain::client::repo_nwo().map(|nwo| {
                                format!(
                                    "https://github.com/{}/pull/{}#pullrequestreview-{}",
                                    nwo, detail.number, id
                                )
                            })
                        })
                    })
                } else {
                    None
                }
            }
            GhDetailPane::Comments => match &self.content {
                GhDetailContent::Issue(detail) => detail
                    .comments
                    .get(self.comments.selected_idx)
                    .and_then(|c| c.url.clone()),
                GhDetailContent::Pr(detail) => detail
                    .comments
                    .get(self.comments.selected_idx)
                    .and_then(|c| c.url.clone()),
                _ => None,
            },
            GhDetailPane::Body | GhDetailPane::Jobs | GhDetailPane::Log => match &self.content {
                GhDetailContent::Issue(issue) => {
                    return vec![PaneEvent::OpenIssueBrowser(issue.number)];
                }
                GhDetailContent::Pr(pr) => {
                    return vec![PaneEvent::OpenPrBrowser(pr.number)];
                }
                _ => return vec![],
            },
        };

        if let Some(url) = url {
            vec![PaneEvent::OpenUrl(url)]
        } else {
            vec![]
        }
    }
}

impl Pane<PaneEvent> for GhDetailViewPane {
    fn handle_key(&mut self, shared: &PaneShared, key: KeyEvent) -> Vec<PaneEvent> {
        self.handle_key_impl(shared, key)
    }
    fn render(&mut self, f: &mut Frame, _ctx: &AppContext, shared: &PaneShared, area: Rect) {
        view::render(f, self, shared, area);
    }

    /// Search covers the active sub-pane of a run detail: job / step names
    /// or log lines.
    fn collect_search_matches(&self, _shared: &PaneShared, query: &str) -> Vec<SearchMatch> {
        let active = self.active_pane;
        match self.run_detail() {
            Some(d) if active == GhDetailPane::Log => d.log.search_matches(query),
            Some(d) => d.jobs.search_matches(query),
            None => vec![],
        }
    }

    fn jump_to_match(&mut self, _shared: &PaneShared, search_match: &SearchMatch) {
        let active = self.active_pane;
        let Some(d) = self.run_detail_mut() else {
            return;
        };
        match (active, search_match) {
            (GhDetailPane::Log, m) => d.log.jump_to_match(m),
            (_, SearchMatch::ListEntry(idx)) => d.jobs.selected_idx = *idx,
            _ => {}
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::domain::actions::types::Step;

    fn run(id: u64, status: &str) -> WorkflowRun {
        WorkflowRun {
            id,
            number: id,
            name: "CI".into(),
            workflow_name: "CI".into(),
            status: status.into(),
            conclusion: if status == "completed" { "failure" } else { "" }.into(),
            head_branch: "main".into(),
            event: "push".into(),
            created_at: "2026-08-28T08:17:23Z".into(),
            updated_at: "2026-08-28T08:18:44Z".into(),
            url: format!("https://github.com/td72/vig/actions/runs/{id}"),
        }
    }

    fn job(id: u64, name: &str, steps: Vec<Step>) -> Job {
        Job {
            id,
            name: name.into(),
            status: "completed".into(),
            conclusion: Some("failure".into()),
            started_at: None,
            completed_at: None,
            url: format!("https://github.com/td72/vig/actions/runs/1/job/{id}"),
            steps,
        }
    }

    fn step(number: u64, name: &str, conclusion: &str) -> Step {
        Step {
            number,
            name: name.into(),
            status: "completed".into(),
            conclusion: Some(conclusion.into()),
            started_at: None,
            completed_at: None,
        }
    }

    fn shared() -> PaneShared {
        PaneShared {
            focused_pane: 5,
            previous_pane: 2,
            search: crate::core::search::SearchState::new(),
        }
    }

    fn run_pane() -> (
        GhDetailViewPane,
        mpsc::Sender<GhBgMessage>,
        mpsc::Receiver<GhBgMessage>,
    ) {
        let (tx, rx) = mpsc::channel();
        let mut dv = GhDetailViewPane::new(5);
        dv.load_run(&run(1, "in_progress"), &tx);
        (dv, tx, rx)
    }

    #[test]
    fn loading_a_run_shows_jobs_first_and_keeps_the_same_run() {
        let (mut dv, tx, _rx) = run_pane();
        assert_eq!(dv.active_pane, GhDetailPane::Jobs);
        assert!(dv.current_detail_info().is_none());
        assert!(dv.run_detail().unwrap().jobs.run_is_active());
        assert!(dv.is_run_loading());
        // The same run again (list refresh) only updates its state.
        dv.active_pane = GhDetailPane::Log;
        dv.load_run(&run(1, "completed"), &tx);
        assert_eq!(dv.active_pane, GhDetailPane::Log, "sub-pane focus kept");
        assert!(!dv.run_detail().unwrap().jobs.run_is_active());
        // Another run starts over.
        dv.load_run(&run(2, "completed"), &tx);
        assert_eq!(dv.active_pane, GhDetailPane::Jobs);
        assert_eq!(dv.run_detail().unwrap().run.id, 2);
    }

    #[test]
    fn run_sub_panes_switch_with_h_l_and_tab() {
        let (mut dv, _tx, _rx) = run_pane();
        let sh = shared();
        assert!(dv.execute(&sh, DetailAction::FocusRight).is_empty());
        assert_eq!(dv.active_pane, GhDetailPane::Log);
        dv.execute(&sh, DetailAction::FocusBody);
        assert_eq!(dv.active_pane, GhDetailPane::Jobs);
        dv.execute(&sh, DetailAction::CycleForward);
        assert_eq!(dv.active_pane, GhDetailPane::Log);
        dv.execute(&sh, DetailAction::CycleForward);
        assert_eq!(dv.active_pane, GhDetailPane::Jobs);
        dv.execute(&sh, DetailAction::CycleBackward);
        assert_eq!(dv.active_pane, GhDetailPane::Log);
        // Esc goes back to the list the detail was opened from.
        let ev = dv.execute(&sh, DetailAction::Esc);
        assert!(matches!(ev.as_slice(), [PaneEvent::SetFocus(2)]));
        // Watch mode is a PR thing.
        dv.execute(&sh, DetailAction::ToggleWatch);
        assert!(!dv.watch_mode);
    }

    #[test]
    fn enter_loads_the_selected_jobs_log_and_focuses_it() {
        let (mut dv, tx, _rx) = run_pane();
        dv.apply_jobs(
            1,
            Ok(vec![job(
                10,
                "test",
                vec![
                    step(1, "Set up job", "success"),
                    step(2, "cargo test", "failure"),
                ],
            )]),
            &tx,
        );
        // Nav moves the jobs selection while Jobs is active.
        dv.execute(&shared(), DetailAction::Nav(NavAction::MoveDown));
        dv.execute(&shared(), DetailAction::Nav(NavAction::MoveDown));
        assert_eq!(dv.run_detail().unwrap().jobs.selected_idx, 2);
        // Enter is turned into an event; the page answers with open_selected_log.
        let ev = dv.execute(&shared(), DetailAction::OpenLog);
        assert!(matches!(ev.as_slice(), [PaneEvent::OpenRunLog]));
        dv.open_selected_log(&tx);
        assert_eq!(dv.active_pane, GhDetailPane::Log);
        let d = dv.run_detail().unwrap();
        assert_eq!(d.log.target().unwrap().job_id, 10);
        assert_eq!(d.log.target().unwrap().failed_steps, ["cargo test"]);
        // `o` on the log opens the job; on the jobs list, the selected job.
        let ev = dv.execute(&shared(), DetailAction::OpenItem);
        assert!(matches!(ev.as_slice(), [PaneEvent::OpenUrl(u)] if u.ends_with("/job/10")));
        // A jobs result for another run is ignored.
        dv.apply_jobs(99, Ok(vec![]), &tx);
        assert_eq!(dv.run_detail().unwrap().jobs.rows.len(), 3);
    }

    #[test]
    fn open_item_falls_back_to_the_run_url() {
        let (mut dv, _tx, _rx) = run_pane();
        let ev = dv.execute(&shared(), DetailAction::OpenItem);
        assert!(matches!(ev.as_slice(), [PaneEvent::OpenUrl(u)] if u.ends_with("/runs/1")));
    }

    #[test]
    fn failed_step_jumps_report_when_there_is_no_log() {
        let (mut dv, _tx, _rx) = run_pane();
        let ev = dv.execute(&shared(), DetailAction::NextFailed);
        assert!(matches!(ev.as_slice(), [PaneEvent::StatusMessage(m)] if m == "No failed steps"));
    }

    #[test]
    fn search_targets_the_active_sub_pane() {
        let (mut dv, tx, _rx) = run_pane();
        dv.apply_jobs(
            1,
            Ok(vec![job(
                10,
                "test (macos)",
                vec![step(1, "cargo test", "failure")],
            )]),
            &tx,
        );
        let sh = shared();
        assert_eq!(dv.collect_search_matches(&sh, "macos").len(), 1);
        assert_eq!(dv.collect_search_matches(&sh, "cargo").len(), 1);
        dv.jump_to_match(&sh, &SearchMatch::ListEntry(1));
        assert_eq!(dv.run_detail().unwrap().jobs.selected_idx, 1);
        // The log has nothing yet.
        dv.active_pane = GhDetailPane::Log;
        assert!(dv.collect_search_matches(&sh, "cargo").is_empty());
        // Issues have no searchable sub-panes.
        let plain = GhDetailViewPane::new(3);
        assert!(plain.collect_search_matches(&sh, "x").is_empty());
    }

    #[test]
    fn run_actions_are_no_ops_for_issue_and_pr_content() {
        let mut dv = GhDetailViewPane::new(3);
        let sh = shared();
        assert!(dv.execute(&sh, DetailAction::OpenLog).is_empty());
        assert!(dv.execute(&sh, DetailAction::NextFailed).is_empty());
        assert!(dv.execute(&sh, DetailAction::PrevFailed).is_empty());
        assert_eq!(dv.active_pane, GhDetailPane::Body);
    }
}
