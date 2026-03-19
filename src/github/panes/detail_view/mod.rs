pub(crate) mod view;

use crate::core::app::AppContext;
use crate::core::keymap::{nav_bindings, ActionHelp, Keymap, NavAction};
use crate::core::pane::{Pane, PaneEvent, PaneShared, SubPaneScroll};
use crate::github::domain::types::*;
use crate::github::domain::{client, disk_cache};
use crate::github::state::{GhBgMessage, GhDetailContent, GhDetailKind, GhDetailPane};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{layout::Rect, Frame};
use std::collections::HashMap;
use std::sync::mpsc;
use std::time::{Instant, SystemTime};

/// Watch mode status for display in the status bar.
pub struct WatchStatus {
    pub last_update_time: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub enum DetailAction {
    Nav(NavAction),
    FocusBody,
    FocusRight,
    CycleForward,
    CycleBackward,
    ToggleWatch,
    OpenItem,
    Esc,
}

crate::impl_pane_action_from_str!(
    DetailAction, nav: Nav,
    FocusBody, FocusRight, CycleForward, CycleBackward, ToggleWatch, OpenItem, Esc
);

impl ActionHelp for DetailAction {
    fn label(&self) -> Option<&'static str> {
        match self {
            DetailAction::Nav(nav) => nav.label(),
            DetailAction::FocusBody => Some("Body pane"),
            DetailAction::FocusRight => Some("Right pane"),
            DetailAction::CycleForward => Some("Next right pane"),
            DetailAction::CycleBackward => Some("Prev right pane"),
            DetailAction::ToggleWatch => Some("Toggle watch mode"),
            DetailAction::OpenItem => Some("Open in browser"),
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
            keymap: default_keymap(),
        }
    }

    /// Set the content to an error state.
    pub fn set_error(&mut self, msg: String) {
        self.content = GhDetailContent::Error(msg);
    }

    /// Return the kind and number of the currently displayed or loading item, if any.
    pub fn current_detail_info(&self) -> Option<(GhDetailKind, u64)> {
        match &self.content {
            GhDetailContent::Issue(detail) => Some((GhDetailKind::Issue, detail.number)),
            GhDetailContent::Pr(detail) => Some((GhDetailKind::Pr, detail.number)),
            GhDetailContent::Loading { kind, number } => Some((*kind, *number)),
            GhDetailContent::Error(_) | GhDetailContent::None => None,
        }
    }

    pub fn is_pr(&self) -> bool {
        matches!(&self.content, GhDetailContent::Pr(_))
    }

    pub fn active_scroll_mut(&mut self) -> &mut SubPaneScroll {
        match self.active_pane {
            GhDetailPane::Body => &mut self.body,
            GhDetailPane::Status => &mut self.status,
            GhDetailPane::Reviews => &mut self.reviews,
            GhDetailPane::Comments => &mut self.comments,
        }
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

    /// Load issue detail — serves from cache if available, otherwise fetches in background.
    pub fn load_issue(&mut self, number: u64, tx: &mpsc::Sender<GhBgMessage>) {
        self.load_detail(GhDetailKind::Issue, number, tx);
    }

    /// Load PR detail — serves from cache if available, otherwise fetches in background.
    pub fn load_pr(&mut self, number: u64, tx: &mpsc::Sender<GhBgMessage>) {
        self.load_detail(GhDetailKind::Pr, number, tx);
    }

    /// Shared logic for loading issue/PR detail with cache hierarchy.
    fn load_detail(&mut self, kind: GhDetailKind, number: u64, tx: &mpsc::Sender<GhBgMessage>) {
        // Already loading this exact item — skip duplicate request
        if matches!(&self.content, GhDetailContent::Loading { kind: k, number: n } if *k == kind && *n == number)
        {
            return;
        }

        // Check in-memory cache
        let cached = match kind {
            GhDetailKind::Issue => self
                .issue_cache
                .get(&number)
                .map(|c| GhDetailContent::Issue(Box::new(c.clone()))),
            GhDetailKind::Pr => self
                .pr_cache
                .get(&number)
                .map(|c| GhDetailContent::Pr(Box::new(c.clone()))),
        };
        if let Some(content) = cached {
            self.content = content;
            self.reset_sub_panes();
            return;
        }

        // Check disk cache
        let from_disk = match kind {
            GhDetailKind::Issue => disk_cache::load_issue_detail(number).map(|c| {
                self.issue_cache.insert(number, c.clone());
                GhDetailContent::Issue(Box::new(c))
            }),
            GhDetailKind::Pr => disk_cache::load_pr_detail(number).map(|c| {
                self.pr_cache.insert(number, c.clone());
                GhDetailContent::Pr(Box::new(c))
            }),
        };
        if let Some(content) = from_disk {
            self.content = content;
            self.reset_sub_panes();
            return;
        }

        // Fetch in background
        self.content = GhDetailContent::Loading { kind, number };
        self.reset_sub_panes();
        let tx = tx.clone();
        match kind {
            GhDetailKind::Issue => {
                std::thread::spawn(move || {
                    let _ = tx.send(GhBgMessage::IssueDetail(client::get_issue(number)));
                });
            }
            GhDetailKind::Pr => {
                std::thread::spawn(move || {
                    let _ = tx.send(GhBgMessage::PrDetail(client::get_pr(number)));
                });
            }
        }
    }

    pub fn clear_caches(&mut self) {
        self.issue_cache.clear();
        self.pr_cache.clear();
    }

    pub fn invalidate_issue(&mut self, number: u64) {
        self.issue_cache.remove(&number);
    }

    pub fn invalidate_pr(&mut self, number: u64) {
        self.pr_cache.remove(&number);
    }

    /// Apply a fetched issue detail — save to disk cache and display.
    pub fn apply_issue_detail(&mut self, detail: GhIssueDetail) {
        disk_cache::save_issue_detail(&detail);
        self.issue_cache.insert(detail.number, detail.clone());
        self.content = GhDetailContent::Issue(Box::new(detail));
    }

    /// Apply a fetched PR detail — save to disk cache and display.
    pub fn apply_pr_detail(&mut self, detail: GhPrDetail) {
        disk_cache::save_pr_detail(&detail);
        self.pr_cache.insert(detail.number, detail.clone());
        self.content = GhDetailContent::Pr(Box::new(detail));
    }

    /// Apply a PR detail fetch result, handling watch-mode error semantics.
    pub fn apply_pr_detail_result(&mut self, result: Result<GhPrDetail, String>) {
        self.watch_in_flight_since = None;
        match result {
            Ok(detail) => {
                self.watch_error = None;
                self.apply_pr_detail(detail);
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

    /// Toggle watch mode (auto-refresh checks every 10s). Only activates on PR detail.
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

    /// Called on every tick. If watch mode is active and 10s have elapsed, refresh the detail.
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
            if last.elapsed() >= std::time::Duration::from_secs(10) {
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
        self.invalidate_pr(number);
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
                let half = (self.view_height / 2).max(1);
                let s = self.active_scroll_mut();
                s.scroll_y = s.scroll_y.saturating_add(half);
            }
            DetailAction::Nav(NavAction::HalfPageUp) => {
                let half = (self.view_height / 2).max(1);
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
            DetailAction::Esc => {
                return vec![PaneEvent::SetFocus(shared.previous_pane)];
            }
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
            GhDetailPane::Body => 0,
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
            GhDetailPane::Body => match &self.content {
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
