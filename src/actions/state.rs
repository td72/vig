//! The Actions page: GitHub Actions workflow runs, the jobs and steps of the
//! selected run, and one job's log. Read-only: only `gh run list`, `gh run
//! view` and a GET of the job-log endpoint are ever run.

use crate::actions::domain::client;
use crate::actions::domain::types::{Job, WorkflowRun};
use crate::actions::panes::jobs::{JobsAction, JobsPane};
use crate::actions::panes::log::{LogAction, LogPane};
use crate::actions::panes::runs::{RunsAction, RunsPane};
use crate::core::app::{AppContext, PageState};
use crate::core::config::{Config, LoadedPageConfig};
use crate::core::keymap::{Keymap, ViewAction};
use crate::core::layout::{split_page_frame, PageLayoutConfig};
use crate::core::page::PageAction;
use crate::core::pane::{self, Pane, PaneEvent, PaneSet, PaneShared};
use crate::core::search::SearchState;
use crate::core::ui::status_bar;
use crate::github::domain::client::check_gh_available;
use anyhow::Result;
use crossterm::event::KeyEvent;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// How often the run list (and the selected run's jobs) are re-fetched
/// while something is still queued or running.
pub const ACTIVE_REFRESH_INTERVAL: Duration = Duration::from_secs(5);

/// Pane IDs resolved from the KDL config at construction time.
#[derive(Debug, Clone, Copy)]
pub struct ActionsPaneIds {
    pub runs: usize,
    pub jobs: usize,
    pub log: usize,
}

impl ActionsPaneIds {
    fn from_config(cfg: &LoadedPageConfig) -> Self {
        Self {
            runs: cfg.resolve_id_expect("runs"),
            jobs: cfg.resolve_id_expect("jobs"),
            log: cfg.resolve_id_expect("log"),
        }
    }
}

pub enum ActionsBgMessage {
    Auth(Result<(), String>),
    RunList(Result<Vec<WorkflowRun>, String>),
    Jobs {
        run_id: u64,
        result: Result<Vec<Job>, String>,
    },
    Log {
        request_id: u64,
        append: bool,
        result: Result<Vec<String>, String>,
    },
}

pub struct ActionsPanes {
    pub runs: RunsPane,
    pub jobs: JobsPane,
    pub log: LogPane,
    pub ids: ActionsPaneIds,
}

impl PaneSet for ActionsPanes {
    fn get_mut(&mut self, idx: usize) -> Option<&mut dyn Pane<PaneEvent>> {
        if idx == self.ids.runs {
            Some(&mut self.runs)
        } else if idx == self.ids.jobs {
            Some(&mut self.jobs)
        } else if idx == self.ids.log {
            Some(&mut self.log)
        } else {
            None
        }
    }
}

pub struct ActionsState {
    pub pane: PaneShared,
    pub panes: ActionsPanes,
    /// `None` until `gh auth status` has answered.
    pub gh_available: Option<bool>,
    pub gh_error: Option<String>,
    bg_rx: Option<mpsc::Receiver<ActionsBgMessage>>,
    bg_tx: Option<mpsc::Sender<ActionsBgMessage>>,
    initialized: bool,
    last_list_refresh: Option<Instant>,
    last_jobs_refresh: Option<Instant>,
    layout_config: PageLayoutConfig,
    view_keymap: Keymap<ViewAction>,
}

impl pane::PageLayout for ActionsState {
    type Panes = ActionsPanes;
    fn page_parts_mut(
        &mut self,
    ) -> (
        &mut PaneShared,
        &mut Self::Panes,
        &Keymap<ViewAction>,
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

impl ActionsState {
    pub fn new(cfg: &Config) -> Result<Self> {
        let page_cfg = cfg.actions_page()?;
        let ids = ActionsPaneIds::from_config(&page_cfg);
        // Validates the bind declaration (runs → jobs).
        let _ = page_cfg.resolve_select_bindings();

        let runs_km = page_cfg.keymap::<RunsAction>("runs")?;
        let jobs_km = page_cfg.keymap::<JobsAction>("jobs")?;
        let log_km = page_cfg.keymap::<LogAction>("log")?;
        let view_km = page_cfg.keymap::<ViewAction>("view")?;

        let mut runs = RunsPane::new(ids.runs, ids.jobs, ids.log);
        runs.set_keymap(runs_km);
        let mut jobs = JobsPane::new(ids.jobs, ids.log);
        jobs.set_keymap(jobs_km);
        let mut log = LogPane::new(ids.log);
        log.set_keymap(log_km);

        Ok(Self {
            pane: PaneShared {
                focused_pane: ids.runs,
                previous_pane: ids.runs,
                search: SearchState::new(),
            },
            panes: ActionsPanes {
                runs,
                jobs,
                log,
                ids,
            },
            gh_available: None,
            gh_error: None,
            bg_rx: None,
            bg_tx: None,
            initialized: false,
            last_list_refresh: None,
            last_jobs_refresh: None,
            layout_config: page_cfg.layout,
            view_keymap: view_km,
        })
    }

    /// First switch to the page: check `gh`, show the cached list, fetch.
    fn initialize(&mut self) {
        if self.initialized {
            return;
        }
        self.initialized = true;
        let (tx, rx) = mpsc::channel();
        self.bg_tx = Some(tx);
        self.bg_rx = Some(rx);
        self.check_auth();
        if let Some(runs) = client::load_run_list() {
            self.panes.runs.set_runs(runs);
            self.sync_jobs();
        }
        self.spawn_runs();
    }

    fn check_auth(&mut self) {
        let Some(tx) = self.bg_tx.clone() else {
            return;
        };
        std::thread::spawn(move || {
            let _ = tx.send(ActionsBgMessage::Auth(check_gh_available()));
        });
    }

    fn spawn_runs(&mut self) {
        let Some(tx) = self.bg_tx.clone() else {
            return;
        };
        self.last_list_refresh = Some(Instant::now());
        self.panes.runs.set_loading(true);
        std::thread::spawn(move || {
            let _ = tx.send(ActionsBgMessage::RunList(client::list_runs(
                client::RUN_LIST_LIMIT,
            )));
        });
    }

    /// `r`: re-check `gh`, re-fetch the runs, the jobs and the log.
    fn refresh(&mut self) {
        self.gh_error = None;
        if self.gh_available == Some(false) {
            self.gh_available = None;
        }
        self.check_auth();
        self.spawn_runs();
        if let Some(tx) = self.bg_tx.clone() {
            self.last_jobs_refresh = Some(Instant::now());
            self.panes.jobs.spawn_fetch(&tx);
            self.panes.log.refresh(&tx);
        }
    }

    /// Point the jobs pane at the selected run.
    fn sync_jobs(&mut self) {
        let Some(tx) = self.bg_tx.clone() else {
            return;
        };
        let selected = self.panes.runs.selected().cloned();
        let changed = self.panes.jobs.run_id() != selected.as_ref().map(|r| r.id);
        self.panes.jobs.load(selected.as_ref(), &tx);
        if changed {
            self.last_jobs_refresh = Some(Instant::now());
            // The log belonged to a job of the previous run.
            self.panes.log.clear();
        }
    }

    /// Load the log of the job selected in the jobs pane (a step row
    /// scrolls to that step once the log is in).
    fn open_selected_log(&mut self) {
        let Some(tx) = self.bg_tx.clone() else {
            return;
        };
        if let Some((target, step)) = self.panes.jobs.selected_target() {
            self.panes.log.load(target, step, &tx);
        }
    }

    /// After a jobs refresh, tell the log pane whether its job finished.
    fn sync_log_target(&mut self) {
        let Some(tx) = self.bg_tx.clone() else {
            return;
        };
        let job_id = self.panes.log.target().map(|t| t.job_id);
        if let Some(latest) = job_id.and_then(|id| self.panes.jobs.target_for(id)) {
            self.panes.log.update_target(latest, &tx);
        }
    }

    fn drain_bg_messages(&mut self) {
        let messages: Vec<_> = match &self.bg_rx {
            Some(rx) => rx.try_iter().collect(),
            None => return,
        };
        for msg in messages {
            match msg {
                ActionsBgMessage::Auth(result) => match result {
                    Ok(()) => {
                        self.gh_available = Some(true);
                    }
                    Err(e) => {
                        self.gh_available = Some(false);
                        self.gh_error = Some(e);
                        self.panes.runs.set_loading(false);
                    }
                },
                ActionsBgMessage::RunList(result) => {
                    self.panes.runs.set_loading(false);
                    match result {
                        Ok(runs) => {
                            client::save_run_list(&runs);
                            self.panes.runs.set_runs(runs);
                            self.sync_jobs();
                        }
                        Err(e) => self.note_error(e),
                    }
                }
                ActionsBgMessage::Jobs { run_id, result } => {
                    self.panes.jobs.apply(run_id, result);
                    self.sync_log_target();
                }
                ActionsBgMessage::Log {
                    request_id,
                    append,
                    result,
                } => {
                    self.panes.log.apply(request_id, append, result);
                }
            }
        }
    }

    fn note_error(&mut self, e: String) {
        if self.gh_error.is_none() {
            self.gh_error = Some(e);
        }
    }

    fn process_events(
        &mut self,
        ctx: &mut AppContext,
        events: Vec<PaneEvent>,
    ) -> Result<PageAction> {
        let ids = self.panes.ids;
        for event in events {
            if pane::process_common_event(&mut self.pane, ctx, &event) {
                continue;
            }
            match event {
                PaneEvent::SetFocus(id) if id == ids.jobs => {
                    self.sync_jobs();
                }
                PaneEvent::SetFocus(id) if id == ids.log => {
                    self.open_selected_log();
                }
                PaneEvent::SelectionChanged if self.pane.focused_pane == ids.runs => {
                    self.sync_jobs();
                }
                PaneEvent::JumpToMatch(forward) => {
                    if let Some(origin) =
                        self.pane
                            .jump_to_search_match(&mut self.panes, ctx, forward)
                    {
                        if origin == ids.runs {
                            self.sync_jobs();
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(PageAction::None)
    }

    fn handle_view_key(&mut self, ctx: &mut AppContext, key: KeyEvent) -> Result<PageAction> {
        if let Some(action) = self.view_keymap.lookup(key) {
            if let Some(page_action) = pane::execute_common_view_action(ctx, *action) {
                return Ok(page_action);
            }
            if *action == ViewAction::Refresh {
                self.refresh();
                return Ok(PageAction::None);
            }
        }
        let events = pane::dispatch_page_key(self, key);
        self.process_events(ctx, events)
    }

    /// Summary for the status bar: `(runs, active runs)`.
    pub fn counts(&self) -> (usize, usize) {
        self.panes.runs.counts()
    }

    pub fn is_updating(&self) -> bool {
        self.panes.runs.is_loading() || self.panes.jobs.is_loading() || self.panes.log.is_loading()
    }

    fn render_unavailable(&self, f: &mut Frame, area: Rect) {
        let err = self.gh_error.as_deref().unwrap_or("gh not found");
        let lines = vec![
            Line::default(),
            Line::from(Span::styled(
                format!("  gh not available: {err}"),
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "  Install the GitHub CLI and run `gh auth login`, then press r.",
                Style::default().fg(Color::DarkGray),
            )),
        ];
        f.render_widget(Paragraph::new(lines), area);
    }
}

impl PageState for ActionsState {
    fn id(&self) -> &'static str {
        "actions"
    }

    fn label(&self) -> &'static str {
        "Actions"
    }

    fn help_bindings(&self) -> Vec<(String, String)> {
        use crate::core::keymap::help_section;
        let s = |k: &str, v: &str| (k.to_string(), v.to_string());
        let mut entries = vec![s("1 … 7", "Switch view")];
        entries.extend(self.view_keymap.help_entries());
        entries.extend(help_section("Runs"));
        entries.extend(self.panes.runs.keymap().help_entries());
        entries.extend(help_section("Jobs"));
        entries.extend(self.panes.jobs.keymap().help_entries());
        entries.extend(help_section("Log"));
        entries.extend(self.panes.log.keymap().help_entries());
        entries
    }

    fn handle_key(&mut self, ctx: &mut AppContext, key: KeyEvent) -> Result<PageAction> {
        if self.pane.handle_search_input(&mut self.panes, ctx, key) {
            // A confirmed search moves the list selection without an event.
            let origin = self.pane.search.origin;
            if !self.pane.search.active && origin == self.panes.ids.runs {
                self.sync_jobs();
            }
            return Ok(PageAction::None);
        }
        self.handle_view_key(ctx, key)
    }

    fn render(&mut self, f: &mut Frame, ctx: &AppContext, area: Rect) {
        let frame = split_page_frame(area);
        status_bar::render_actions_header(f, ctx, frame.header);
        if self.gh_available == Some(false) {
            self.render_unavailable(f, frame.content);
        } else {
            pane::render_page_content(self, f, ctx, frame.content);
        }
        status_bar::render_actions_status_bar(f, ctx, self, frame.status_bar);
    }

    fn intercepts_all_keys(&self) -> bool {
        self.pane.search.active
    }

    fn on_tick(&mut self, _ctx: &mut AppContext) {
        if !self.initialized || self.gh_available != Some(true) {
            return;
        }
        let Some(tx) = self.bg_tx.clone() else {
            return;
        };
        let due = |t: Option<Instant>| t.is_none_or(|t| t.elapsed() >= ACTIVE_REFRESH_INTERVAL);
        if self.panes.runs.has_active()
            && due(self.last_list_refresh)
            && !self.panes.runs.is_loading()
        {
            self.spawn_runs();
        }
        if self.panes.jobs.run_is_active()
            && due(self.last_jobs_refresh)
            && !self.panes.jobs.is_loading()
        {
            self.last_jobs_refresh = Some(Instant::now());
            self.panes.jobs.spawn_fetch(&tx);
        }
        self.panes.log.on_tick(&tx);
    }

    fn on_activate(&mut self, _ctx: &mut AppContext) {
        self.initialize();
    }

    fn drain_background(&mut self) {
        self.drain_bg_messages();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_page_builds_from_builtin_config() {
        let cfg = Config::builtin();
        let state = ActionsState::new(&cfg).expect("actions page");
        assert_eq!(state.id(), "actions");
        assert_eq!(state.label(), "Actions");
        assert_eq!(state.pane.focused_pane, state.panes.ids.runs);
        assert_eq!(
            state.layout_config.tab_panes,
            vec![
                state.panes.ids.runs,
                state.panes.ids.jobs,
                state.panes.ids.log
            ]
        );
        let help = state.help_bindings();
        assert_eq!(help[0].0, "1 … 7");
        assert!(help.iter().any(|(_, d)| d.contains("Log")));
        assert!(help.iter().any(|(k, _)| k == "]"));
    }
}
