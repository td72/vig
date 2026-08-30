//! Jobs pane: the jobs of the selected run with their steps nested
//! underneath (`gh run view <id> --json jobs`). Failed steps are
//! highlighted; `Enter` opens the job's log.

use crate::actions::domain::client;
use crate::actions::domain::time::{duration_between, now_secs};
use crate::actions::domain::types::{Job, RunState, Step, WorkflowRun};
use crate::actions::panes::runs::state_color;
use crate::actions::state::ActionsBgMessage;
use crate::core::app::AppContext;
use crate::core::keymap::{
    nav_bindings, search_bindings, ActionHelp, Keymap, NavAction, SearchAction,
};
use crate::core::pane::{self, Pane, PaneEvent, PaneShared};
use crate::core::search::SearchMatch;
use crate::core::theme;
use crate::core::tree::{nest_by, TreePos};
use crossterm::event::KeyCode;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::ListItem,
    Frame,
};
use std::sync::mpsc;

#[derive(Debug, Clone)]
pub enum JobsAction {
    Nav(NavAction),
    Search(SearchAction),
    OpenLog,
    OpenBrowser,
    Back,
    Esc,
}

crate::impl_pane_action_from_str!(
    JobsAction, nav: Nav, search: Search, esc: Esc,
    OpenLog, OpenBrowser, Back
);

impl ActionHelp for JobsAction {
    fn label(&self) -> Option<&'static str> {
        match self {
            JobsAction::Nav(nav) => nav.label(),
            JobsAction::Search(sa) => sa.label(),
            JobsAction::OpenLog => Some("Open job log"),
            JobsAction::OpenBrowser => Some("Open job in browser"),
            JobsAction::Back => Some("Back to runs"),
            JobsAction::Esc => Some("Clear search / back"),
        }
    }
}

pub fn default_keymap() -> Keymap<JobsAction> {
    Keymap::new()
        .bindings(nav_bindings(JobsAction::Nav))
        .bindings(search_bindings(JobsAction::Search))
        .key(KeyCode::Char('i'), JobsAction::OpenLog)
        .key(KeyCode::Enter, JobsAction::OpenLog)
        .key(KeyCode::Char('o'), JobsAction::OpenBrowser)
        .key(KeyCode::Char('h'), JobsAction::Back)
        .key(KeyCode::Esc, JobsAction::Esc)
}

/// One list row: a job or one of its steps.
#[derive(Debug, Clone)]
pub enum JobRow {
    Job(Job),
    Step { job_idx: usize, step: Step },
}

impl JobRow {
    /// Stable identity used to keep the selection across refreshes.
    fn key(&self, rows: &[JobRow]) -> String {
        match self {
            JobRow::Job(j) => format!("j:{}", j.id),
            JobRow::Step { job_idx, step } => match &rows[*job_idx] {
                JobRow::Job(j) => format!("s:{}:{}", j.id, step.number),
                JobRow::Step { .. } => format!("s:?:{}", step.number),
            },
        }
    }

    fn search_text(&self) -> String {
        match self {
            JobRow::Job(j) => j.name.clone(),
            JobRow::Step { step, .. } => step.name.clone(),
        }
    }
}

/// Flatten jobs into `job, step, step, …, job, …` rows with tree guides.
pub fn build_rows(jobs: Vec<Job>) -> (Vec<JobRow>, Vec<TreePos>) {
    let mut rows: Vec<JobRow> = Vec::new();
    for job in jobs {
        let job_idx = rows.len();
        let steps = job.steps.clone();
        rows.push(JobRow::Job(job));
        for step in steps {
            rows.push(JobRow::Step { job_idx, step });
        }
    }
    let order = nest_by(
        rows.len(),
        |i| i as u64,
        |i| match &rows[i] {
            JobRow::Job(_) => None,
            JobRow::Step { job_idx, .. } => Some(*job_idx as u64),
        },
    );
    let positions = order.into_iter().map(|(_, pos)| pos).collect();
    (rows, positions)
}

/// What the log pane needs to know about the job the user picked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogTarget {
    pub run_id: u64,
    pub job_id: u64,
    pub job_name: String,
    pub in_progress: bool,
    /// Names of the job's failed steps, for the `]` / `[` jumps.
    pub failed_steps: Vec<String>,
}

impl LogTarget {
    fn from_job(run_id: u64, job: &Job) -> Self {
        Self {
            run_id,
            job_id: job.id,
            job_name: job.name.clone(),
            in_progress: job.state().is_active(),
            failed_steps: job
                .steps
                .iter()
                .filter(|s| s.state() == RunState::Failure)
                .map(|s| s.name.clone())
                .collect(),
        }
    }
}

pub struct JobsPane {
    pub rows: Vec<JobRow>,
    positions: Vec<TreePos>,
    pub selected_idx: usize,
    /// `(id, title)` of the run whose jobs are shown.
    run: Option<(u64, String)>,
    run_active: bool,
    loading: bool,
    error: Option<String>,
    keymap: Keymap<JobsAction>,
    pane_id: usize,
    log_pane_id: usize,
    view_height: u16,
}

impl JobsPane {
    pub fn new(pane_id: usize, log_pane_id: usize) -> Self {
        Self {
            rows: Vec::new(),
            positions: Vec::new(),
            selected_idx: 0,
            run: None,
            run_active: false,
            loading: false,
            error: None,
            keymap: default_keymap(),
            pane_id,
            log_pane_id,
            view_height: 20,
        }
    }

    pub fn set_keymap(&mut self, km: Keymap<JobsAction>) {
        self.keymap = km;
    }

    pub fn keymap(&self) -> &Keymap<JobsAction> {
        &self.keymap
    }

    pub fn is_loading(&self) -> bool {
        self.loading
    }

    pub fn run_id(&self) -> Option<u64> {
        self.run.as_ref().map(|(id, _)| *id)
    }

    /// The run is still queued or running, so its jobs are worth polling.
    pub fn run_is_active(&self) -> bool {
        self.run_active
    }

    pub fn selected(&self) -> Option<&JobRow> {
        self.rows.get(self.selected_idx)
    }

    /// The job under the cursor (a step row resolves to its job) and, for
    /// a step row, the step's name.
    pub fn selected_target(&self) -> Option<(LogTarget, Option<String>)> {
        let run_id = self.run_id()?;
        match self.selected()? {
            JobRow::Job(j) => Some((LogTarget::from_job(run_id, j), None)),
            JobRow::Step { job_idx, step } => match &self.rows[*job_idx] {
                JobRow::Job(j) => Some((LogTarget::from_job(run_id, j), Some(step.name.clone()))),
                JobRow::Step { .. } => None,
            },
        }
    }

    /// Current state of job `job_id` after a refresh, if it is listed.
    pub fn target_for(&self, job_id: u64) -> Option<LogTarget> {
        let run_id = self.run_id()?;
        self.rows.iter().find_map(|r| match r {
            JobRow::Job(j) if j.id == job_id => Some(LogTarget::from_job(run_id, j)),
            _ => None,
        })
    }

    /// Follow `run` (or nothing). Re-selecting the same run keeps the rows.
    pub fn load(&mut self, run: Option<&WorkflowRun>, tx: &mpsc::Sender<ActionsBgMessage>) {
        match run {
            Some(run) if self.run_id() == Some(run.id) => {
                self.run_active = run.state().is_active();
            }
            Some(run) => {
                self.run = Some((run.id, format!("{} #{}", run.title(), run.number)));
                self.run_active = run.state().is_active();
                self.rows.clear();
                self.positions.clear();
                self.selected_idx = 0;
                self.error = None;
                self.spawn_fetch(tx);
            }
            None => {
                self.run = None;
                self.run_active = false;
                self.rows.clear();
                self.positions.clear();
                self.selected_idx = 0;
                self.error = None;
                self.loading = false;
            }
        }
    }

    /// Re-fetch the current run's jobs (poll / `r`).
    pub fn spawn_fetch(&mut self, tx: &mpsc::Sender<ActionsBgMessage>) {
        let Some(run_id) = self.run_id() else {
            return;
        };
        self.loading = true;
        let tx = tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(ActionsBgMessage::Jobs {
                run_id,
                result: client::list_jobs(run_id),
            });
        });
    }

    /// Apply a fetch result; results for another run are dropped.
    pub fn apply(&mut self, run_id: u64, result: Result<Vec<Job>, String>) {
        if self.run_id() != Some(run_id) {
            return;
        }
        self.loading = false;
        match result {
            Ok(jobs) => {
                self.error = None;
                self.set_jobs(jobs);
            }
            Err(e) => self.error = Some(e),
        }
    }

    /// Replace the rows, keeping the selection on the same job / step.
    pub fn set_jobs(&mut self, jobs: Vec<Job>) {
        let keep = self.selected().map(|r| r.key(&self.rows));
        let (rows, positions) = build_rows(jobs);
        self.rows = rows;
        self.positions = positions;
        self.selected_idx = keep
            .and_then(|k| self.rows.iter().position(|r| r.key(&self.rows) == k))
            .unwrap_or(self.selected_idx)
            .min(self.rows.len().saturating_sub(1));
    }

    fn execute(&mut self, shared: &PaneShared, action: JobsAction) -> Vec<PaneEvent> {
        let esc_fallback = vec![PaneEvent::SetFocus(shared.previous_pane)];
        if let Some(events) =
            pane::try_dispatch_search_esc(&action, shared, self.pane_id, esc_fallback)
        {
            return events;
        }
        match action {
            JobsAction::Nav(nav) => pane::execute_list_nav(
                nav,
                &mut self.selected_idx,
                self.rows.len(),
                Some(self.view_height),
            ),
            JobsAction::OpenLog if self.selected_target().is_some() => {
                vec![PaneEvent::SetFocus(self.log_pane_id)]
            }
            JobsAction::OpenBrowser => {
                let url = match self.selected() {
                    Some(JobRow::Job(j)) => Some(&j.url),
                    Some(JobRow::Step { job_idx, .. }) => match &self.rows[*job_idx] {
                        JobRow::Job(j) => Some(&j.url),
                        JobRow::Step { .. } => None,
                    },
                    None => None,
                };
                match url {
                    Some(u) if !u.is_empty() => vec![PaneEvent::OpenUrl(u.clone())],
                    _ => vec![],
                }
            }
            JobsAction::Back => vec![PaneEvent::SetFocus(shared.previous_pane)],
            _ => vec![],
        }
    }

    fn render_row(row: &JobRow, tree: &TreePos, now: i64) -> ListItem<'static> {
        let dim = Style::default().fg(Color::DarkGray);
        let mut spans = vec![Span::raw(" "), Span::styled(tree.prefix.clone(), dim)];
        let (state, name, started, completed) = match row {
            JobRow::Job(j) => (
                j.state(),
                j.name.clone(),
                j.started_at.as_deref(),
                j.completed_at.as_deref(),
            ),
            JobRow::Step { step, .. } => (
                step.state(),
                step.name.clone(),
                step.started_at.as_deref(),
                step.completed_at.as_deref(),
            ),
        };
        let name_style = match (row, state) {
            (_, RunState::Failure) => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            (JobRow::Job(_), _) => Style::default().add_modifier(Modifier::BOLD),
            (_, RunState::Skipped | RunState::Cancelled) => dim,
            (_, RunState::InProgress) => Style::default().fg(Color::Yellow),
            _ => Style::default(),
        };
        spans.push(Span::styled(
            state.icon(),
            Style::default().fg(state_color(state)),
        ));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(name, name_style));
        let duration = match state {
            RunState::Queued | RunState::Skipped => String::new(),
            RunState::InProgress => duration_between(started, None, now),
            _ => duration_between(started, completed, now),
        };
        if !duration.is_empty() {
            spans.push(Span::styled(
                format!("  {duration}"),
                Style::default().fg(Color::Blue),
            ));
        }
        if state == RunState::Skipped {
            spans.push(Span::styled("  skipped", dim));
        }
        ListItem::new(Line::from(spans))
    }
}

impl Pane<PaneEvent> for JobsPane {
    crate::impl_handle_key!(keymap);

    fn render(&mut self, f: &mut Frame, _ctx: &AppContext, shared: &PaneShared, area: Rect) {
        self.view_height = area.height.saturating_sub(2);
        let title = match &self.run {
            Some((_, title)) => format!("Jobs: {title}"),
            None => "Jobs".to_string(),
        };
        let error = self.error.as_ref().map(|e| format!("Error: {e}"));
        let empty = match (&error, &self.run) {
            (Some(e), _) => Some(e.as_str()),
            (None, None) => Some("Select a run to list its jobs"),
            (None, Some(_)) if self.rows.is_empty() && self.loading => Some("Loading..."),
            (None, Some(_)) if self.rows.is_empty() => Some("No jobs yet"),
            _ => None,
        };
        let is_focused = shared.focused_pane == self.pane_id;
        let show_selection = is_focused || shared.focused_pane == self.log_pane_id;
        let selected = show_selection.then_some(self.selected_idx);
        let now = now_secs();
        theme::render_list_pane(
            f,
            area,
            shared,
            self.pane_id,
            &title,
            selected,
            empty,
            |match_set, current_match_idx| {
                self.rows
                    .iter()
                    .enumerate()
                    .map(|(idx, row)| {
                        let tree = self.positions.get(idx).cloned().unwrap_or_default();
                        let mut li = Self::render_row(row, &tree, now);
                        let hl = theme::search_highlight_for(match_set, current_match_idx, idx);
                        if hl.is_active() {
                            li = li.style(hl.apply(Style::default()));
                        }
                        li
                    })
                    .collect()
            },
        );
    }

    fn collect_search_matches(&self, _shared: &PaneShared, query: &str) -> Vec<SearchMatch> {
        pane::collect_list_search_matches(&self.rows, query, JobRow::search_text)
    }

    crate::impl_list_pane_selection!();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(number: u64, name: &str, conclusion: &str) -> Step {
        Step {
            number,
            name: name.into(),
            status: "completed".into(),
            conclusion: Some(conclusion.into()),
            started_at: Some("2026-08-28T08:17:29Z".into()),
            completed_at: Some("2026-08-28T08:17:32Z".into()),
        }
    }

    fn job(id: u64, name: &str, conclusion: &str, steps: Vec<Step>) -> Job {
        Job {
            id,
            name: name.into(),
            status: "completed".into(),
            conclusion: Some(conclusion.into()),
            started_at: Some("2026-08-28T08:17:27Z".into()),
            completed_at: Some("2026-08-28T08:18:43Z".into()),
            url: format!("https://github.com/td72/vig/actions/runs/1/job/{id}"),
            steps,
        }
    }

    fn run() -> WorkflowRun {
        WorkflowRun {
            id: 1,
            number: 188,
            name: "CI".into(),
            workflow_name: "CI".into(),
            status: "completed".into(),
            conclusion: "failure".into(),
            head_branch: "main".into(),
            event: "push".into(),
            created_at: String::new(),
            updated_at: String::new(),
            url: String::new(),
        }
    }

    fn rendered(rows: &[JobRow], positions: &[TreePos]) -> Vec<String> {
        rows.iter()
            .zip(positions)
            .map(|(r, p)| {
                let name = match r {
                    JobRow::Job(j) => format!("[{}]", j.name),
                    JobRow::Step { step, .. } => step.name.clone(),
                };
                format!("{}{name}", p.prefix)
            })
            .collect()
    }

    #[test]
    fn steps_nest_under_their_job() {
        let (rows, positions) = build_rows(vec![
            job(
                10,
                "test (macos)",
                "failure",
                vec![
                    step(1, "Set up job", "success"),
                    step(2, "cargo test", "failure"),
                ],
            ),
            job(
                11,
                "build",
                "success",
                vec![step(1, "Set up job", "success")],
            ),
            job(12, "lint", "skipped", vec![]),
        ]);
        assert_eq!(
            rendered(&rows, &positions),
            [
                "[test (macos)]",
                "├─ Set up job",
                "└─ cargo test",
                "[build]",
                "└─ Set up job",
                "[lint]",
            ]
        );
    }

    #[test]
    fn selected_target_resolves_steps_to_their_job() {
        let (tx, _rx) = mpsc::channel();
        let mut pane = JobsPane::new(1, 2);
        pane.load(Some(&run()), &tx);
        assert!(pane.is_loading());
        pane.apply(
            1,
            Ok(vec![job(
                10,
                "test",
                "failure",
                vec![
                    step(1, "Set up job", "success"),
                    step(2, "cargo test", "failure"),
                ],
            )]),
        );
        assert!(!pane.is_loading());
        let (target, step_name) = pane.selected_target().unwrap();
        assert_eq!(target.job_id, 10);
        assert_eq!(target.job_name, "test");
        assert!(!target.in_progress);
        assert_eq!(target.failed_steps, ["cargo test"]);
        assert_eq!(step_name, None);
        pane.selected_idx = 2;
        let (target, step_name) = pane.selected_target().unwrap();
        assert_eq!(target.job_id, 10);
        assert_eq!(step_name.as_deref(), Some("cargo test"));
        // Results for another run are ignored.
        pane.apply(99, Ok(vec![]));
        assert_eq!(pane.rows.len(), 3);
        // A refresh keeps the selection on the same step.
        pane.apply(
            1,
            Ok(vec![
                job(9, "new first", "success", vec![]),
                job(
                    10,
                    "test",
                    "failure",
                    vec![
                        step(1, "Set up job", "success"),
                        step(2, "cargo test", "failure"),
                    ],
                ),
            ]),
        );
        assert_eq!(pane.selected_idx, 3);
        assert_eq!(pane.target_for(9).unwrap().job_name, "new first");
        assert!(pane.target_for(1234).is_none());
    }

    #[test]
    fn open_log_and_browser_events() {
        let (tx, _rx) = mpsc::channel();
        let mut pane = JobsPane::new(1, 2);
        let shared = PaneShared {
            focused_pane: 1,
            previous_pane: 0,
            search: crate::core::search::SearchState::new(),
        };
        // Nothing loaded: Enter does nothing, h goes back.
        assert!(pane.execute(&shared, JobsAction::OpenLog).is_empty());
        assert!(matches!(
            pane.execute(&shared, JobsAction::Back).as_slice(),
            [PaneEvent::SetFocus(0)]
        ));
        pane.load(Some(&run()), &tx);
        pane.apply(
            1,
            Ok(vec![job(
                10,
                "test",
                "success",
                vec![step(1, "a", "success")],
            )]),
        );
        assert!(matches!(
            pane.execute(&shared, JobsAction::OpenLog).as_slice(),
            [PaneEvent::SetFocus(2)]
        ));
        pane.selected_idx = 1;
        assert!(matches!(
            pane.execute(&shared, JobsAction::OpenBrowser).as_slice(),
            [PaneEvent::OpenUrl(u)] if u.ends_with("/job/10")
        ));
    }
}
