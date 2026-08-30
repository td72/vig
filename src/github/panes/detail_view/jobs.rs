//! Jobs sub-pane of a run detail: the jobs of the run with their steps
//! nested underneath (`gh run view <id> --json jobs`). Failed steps are
//! highlighted; `Enter` opens the job's log in the Log sub-pane.

use crate::core::keymap::{execute_nav, NavAction};
use crate::core::pane;
use crate::core::search::SearchMatch;
use crate::core::theme;
use crate::core::tree::{nest_by, TreePos};
use crate::github::domain::actions::client;
use crate::github::domain::actions::time::{duration_between, now_secs};
use crate::github::domain::actions::types::{Job, RunState, Step, WorkflowRun};
use crate::github::panes::run_list::state_color;
use crate::github::state::GhBgMessage;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, ListItem},
    Frame,
};
use std::collections::HashSet;
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// How often the jobs of a queued / running run are re-fetched.
pub const JOBS_POLL_INTERVAL: Duration = Duration::from_secs(5);

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

/// What the log sub-pane needs to know about the job the user picked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogTarget {
    pub run_id: u64,
    pub job_id: u64,
    pub job_name: String,
    pub url: String,
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
            url: job.url.clone(),
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

/// State of the Jobs sub-pane: the rows of one run and their fetch status.
#[derive(Debug, Clone)]
pub struct JobsView {
    pub rows: Vec<JobRow>,
    positions: Vec<TreePos>,
    pub selected_idx: usize,
    run_id: u64,
    run_active: bool,
    loading: bool,
    last_fetch: Option<Instant>,
    error: Option<String>,
    view_height: u16,
}

impl JobsView {
    /// Start following `run` and fetch its jobs.
    pub fn new(run: &WorkflowRun, tx: &mpsc::Sender<GhBgMessage>) -> Self {
        let mut view = Self {
            rows: Vec::new(),
            positions: Vec::new(),
            selected_idx: 0,
            run_id: run.id,
            run_active: run.state().is_active(),
            loading: false,
            last_fetch: None,
            error: None,
            view_height: 20,
        };
        view.spawn_fetch(tx);
        view
    }

    pub fn is_loading(&self) -> bool {
        self.loading
    }

    /// The run is still queued or running, so its jobs are worth polling.
    #[cfg(test)]
    pub fn run_is_active(&self) -> bool {
        self.run_active
    }

    /// The run list was refreshed: pick up the run's new state.
    pub fn update_run(&mut self, run: &WorkflowRun) {
        if run.id == self.run_id {
            self.run_active = run.state().is_active();
        }
    }

    pub fn selected(&self) -> Option<&JobRow> {
        self.rows.get(self.selected_idx)
    }

    /// The job under the cursor (a step row resolves to its job) and, for
    /// a step row, the step's name.
    pub fn selected_target(&self) -> Option<(LogTarget, Option<String>)> {
        let run_id = self.run_id;
        match self.selected()? {
            JobRow::Job(j) => Some((LogTarget::from_job(run_id, j), None)),
            JobRow::Step { job_idx, step } => match &self.rows[*job_idx] {
                JobRow::Job(j) => Some((LogTarget::from_job(run_id, j), Some(step.name.clone()))),
                JobRow::Step { .. } => None,
            },
        }
    }

    /// URL of the job under the cursor (a step row resolves to its job).
    pub fn selected_url(&self) -> Option<String> {
        self.selected_target()
            .map(|(t, _)| t.url)
            .filter(|u| !u.is_empty())
    }

    /// Current state of job `job_id` after a refresh, if it is listed.
    pub fn target_for(&self, job_id: u64) -> Option<LogTarget> {
        let run_id = self.run_id;
        self.rows.iter().find_map(|r| match r {
            JobRow::Job(j) if j.id == job_id => Some(LogTarget::from_job(run_id, j)),
            _ => None,
        })
    }

    /// Re-fetch the run's jobs (poll / `r`).
    pub fn spawn_fetch(&mut self, tx: &mpsc::Sender<GhBgMessage>) {
        let run_id = self.run_id;
        self.loading = true;
        self.last_fetch = Some(Instant::now());
        let tx = tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(GhBgMessage::RunJobs {
                run_id,
                result: client::list_jobs(run_id),
            });
        });
    }

    /// Poll a queued / running run once per [`JOBS_POLL_INTERVAL`].
    pub fn on_tick(&mut self, tx: &mpsc::Sender<GhBgMessage>) {
        if !self.run_active || self.loading {
            return;
        }
        let due = self
            .last_fetch
            .is_none_or(|t| t.elapsed() >= JOBS_POLL_INTERVAL);
        if due {
            self.spawn_fetch(tx);
        }
    }

    /// Apply a fetch result; results for another run are dropped.
    pub fn apply(&mut self, run_id: u64, result: Result<Vec<Job>, String>) {
        if self.run_id != run_id {
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

    /// Move the selection; `true` if it changed.
    pub fn nav(&mut self, nav: NavAction) -> bool {
        execute_nav(
            nav,
            &mut self.selected_idx,
            self.rows.len(),
            Some(self.view_height),
        )
    }

    pub fn search_matches(&self, query: &str) -> Vec<SearchMatch> {
        pane::collect_list_search_matches(&self.rows, query, JobRow::search_text)
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

    /// Render into `block`. The selection is highlighted while this is the
    /// active sub-pane of a focused detail pane.
    pub fn render(
        &mut self,
        f: &mut Frame,
        area: Rect,
        block: Block<'static>,
        highlight_selection: bool,
        match_set: &HashSet<usize>,
        current_match: Option<usize>,
    ) {
        self.view_height = area.height.saturating_sub(2);
        let empty = match &self.error {
            Some(e) => Some(format!("Error: {e}")),
            None if self.rows.is_empty() && self.loading => Some("Loading...".to_string()),
            None if self.rows.is_empty() => Some("No jobs yet".to_string()),
            None => None,
        };
        if let Some(message) = empty {
            theme::render_empty_list(f, area, block, &message);
            return;
        }
        let now = now_secs();
        let items: Vec<ListItem<'static>> = self
            .rows
            .iter()
            .enumerate()
            .map(|(idx, row)| {
                let tree = self.positions.get(idx).cloned().unwrap_or_default();
                let mut li = Self::render_row(row, &tree, now);
                let hl = theme::search_highlight_for(match_set, current_match, idx);
                if hl.is_active() {
                    li = li.style(hl.apply(Style::default()));
                }
                li
            })
            .collect();
        let selected = highlight_selection.then_some(self.selected_idx);
        theme::render_search_list(f, area, items, block, selected, match_set);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn step(number: u64, name: &str, conclusion: &str) -> Step {
        Step {
            number,
            name: name.into(),
            status: "completed".into(),
            conclusion: Some(conclusion.into()),
            started_at: Some("2026-08-28T08:17:29Z".into()),
            completed_at: Some("2026-08-28T08:17:32Z".into()),
        }
    }

    pub(crate) fn job(id: u64, name: &str, conclusion: &str, steps: Vec<Step>) -> Job {
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
        let mut view = JobsView::new(&run(), &tx);
        assert!(view.is_loading());
        assert!(!view.run_is_active());
        view.apply(
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
        assert!(!view.is_loading());
        let (target, step_name) = view.selected_target().unwrap();
        assert_eq!(target.job_id, 10);
        assert_eq!(target.job_name, "test");
        assert!(target.url.ends_with("/job/10"));
        assert!(!target.in_progress);
        assert_eq!(target.failed_steps, ["cargo test"]);
        assert_eq!(step_name, None);
        view.selected_idx = 2;
        let (target, step_name) = view.selected_target().unwrap();
        assert_eq!(target.job_id, 10);
        assert_eq!(step_name.as_deref(), Some("cargo test"));
        assert!(view.selected_url().unwrap().ends_with("/job/10"));
        // Results for another run are ignored.
        view.apply(99, Ok(vec![]));
        assert_eq!(view.rows.len(), 3);
        // A refresh keeps the selection on the same step.
        view.apply(
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
        assert_eq!(view.selected_idx, 3);
        assert_eq!(view.target_for(9).unwrap().job_name, "new first");
        assert!(view.target_for(1234).is_none());
        // Errors are kept for display.
        view.apply(1, Err("boom".into()));
        assert_eq!(view.error.as_deref(), Some("boom"));
    }

    #[test]
    fn polls_only_while_the_run_is_active() {
        let (tx, rx) = mpsc::channel();
        let mut active = run();
        active.status = "in_progress".into();
        let mut view = JobsView::new(&active, &tx);
        assert!(view.run_is_active());
        view.apply(1, Ok(vec![]));
        // Not due yet: the fetch just happened.
        view.on_tick(&tx);
        assert!(!view.is_loading());
        view.last_fetch = None;
        view.on_tick(&tx);
        assert!(view.is_loading(), "a due tick re-fetches");
        // The run finished: no more polling.
        view.apply(1, Ok(vec![]));
        view.update_run(&run());
        assert!(!view.run_is_active());
        view.last_fetch = None;
        view.on_tick(&tx);
        assert!(!view.is_loading());
        drop(rx);
    }

    #[test]
    fn nav_and_search_over_rows() {
        let (tx, _rx) = mpsc::channel();
        let mut view = JobsView::new(&run(), &tx);
        view.apply(
            1,
            Ok(vec![job(
                10,
                "test",
                "success",
                vec![
                    step(1, "cargo build", "success"),
                    step(2, "cargo test", "success"),
                ],
            )]),
        );
        assert!(view.nav(NavAction::MoveDown));
        assert_eq!(view.selected_idx, 1);
        assert!(view.nav(NavAction::JumpBottom));
        assert_eq!(view.selected_idx, 2);
        assert!(!view.nav(NavAction::MoveDown));
        assert_eq!(view.search_matches("cargo").len(), 2);
        assert_eq!(view.search_matches("test").len(), 2);
        assert!(view.search_matches("nope").is_empty());
    }
}
