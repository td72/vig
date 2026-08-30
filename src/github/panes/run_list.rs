//! Workflow Runs column: the latest runs from `gh run list`, newest first.
//! Status icon, workflow, run number, branch, event, duration (elapsed
//! while running) and how long ago the run was created.

use crate::core::pane::PaneEvent;
use crate::github::domain::actions::client;
use crate::github::domain::actions::time::{
    duration_between, format_relative, now_secs, parse_iso8601,
};
use crate::github::domain::actions::types::{RunState, WorkflowRun};
use crate::github::panes::gh_list::{GhListItem, GhListPane, TreePos};
use crate::github::state::GhBgMessage;
use crossterm::event::KeyCode;
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::ListItem,
};

pub fn state_color(state: RunState) -> Color {
    match state {
        RunState::Queued => Color::DarkGray,
        RunState::InProgress => Color::Yellow,
        RunState::Success => Color::Green,
        RunState::Failure => Color::Red,
        RunState::Cancelled | RunState::Skipped | RunState::Other => Color::DarkGray,
    }
}

/// One list row for `run` as of `now` (Unix seconds).
pub fn render_run_row(run: &WorkflowRun, now: i64) -> ListItem<'static> {
    ListItem::new(Line::from(run_row_spans(run, now)))
}

/// The styled cells of a run row.
fn run_row_spans(run: &WorkflowRun, now: i64) -> Vec<Span<'static>> {
    let dim = Style::default().fg(Color::DarkGray);
    let state = run.state();
    let name_style = match state {
        RunState::InProgress => Style::default().add_modifier(Modifier::BOLD),
        RunState::Failure => Style::default().fg(Color::Red),
        RunState::Cancelled | RunState::Skipped => dim,
        _ => Style::default(),
    };
    let duration = match state {
        RunState::Queued => String::new(),
        RunState::InProgress => duration_between(Some(&run.created_at), None, now),
        _ => duration_between(Some(&run.created_at), Some(&run.updated_at), now),
    };
    let age = parse_iso8601(&run.created_at)
        .map(|t| format_relative(now - t))
        .unwrap_or_default();
    let mut spans = vec![
        Span::raw(" "),
        Span::styled(state.icon(), Style::default().fg(state_color(state))),
        Span::raw(" "),
        Span::styled(run.title().to_string(), name_style),
        Span::raw(" "),
        Span::styled(
            format!("#{}", run.number),
            Style::default().fg(Color::Yellow),
        ),
        Span::raw("  "),
        Span::styled(run.head_branch.clone(), Style::default().fg(Color::Magenta)),
        Span::styled(format!("  {}", run.event), dim),
    ];
    if !duration.is_empty() {
        spans.push(Span::styled(
            format!("  {duration}"),
            Style::default().fg(Color::Blue),
        ));
    }
    if !age.is_empty() {
        spans.push(Span::styled(format!("  {age}"), dim));
    }
    spans
}

impl GhListItem for WorkflowRun {
    fn pane_title() -> &'static str {
        "Workflow Runs"
    }

    fn empty_message() -> &'static str {
        "No workflow runs"
    }

    fn render_item(&self, _tree: &TreePos) -> ListItem<'static> {
        render_run_row(self, now_secs())
    }

    /// Runs are identified by their database id: the run *number* only
    /// counts within one workflow and repeats across workflows.
    #[allow(clippy::misnamed_getters)]
    fn number(&self) -> u64 {
        self.id
    }

    fn search_text(&self) -> String {
        format!(
            "{} #{} {} {}",
            self.title(),
            self.number,
            self.head_branch,
            self.event
        )
    }

    fn browser_event(&self) -> PaneEvent {
        PaneEvent::OpenUrl(self.url.clone())
    }

    fn load_disk_cache() -> Option<Vec<Self>> {
        client::load_run_list()
    }

    fn save_disk_cache(items: &[Self]) {
        client::save_run_list(items);
    }

    fn fetch_list() -> Result<Vec<Self>, String> {
        client::list_runs(client::RUN_LIST_LIMIT)
    }

    fn wrap_bg_message(result: Result<Vec<Self>, String>) -> GhBgMessage {
        GhBgMessage::RunList(result)
    }
}

pub type GhRunListPane = GhListPane<WorkflowRun>;

pub fn new_pane(pane_id: usize, detail_id: usize, switch_target: usize) -> GhRunListPane {
    GhListPane::new(pane_id, detail_id, KeyCode::BackTab, switch_target)
}

impl GhRunListPane {
    /// Whether any listed run is still queued or in progress.
    pub fn has_active(&self) -> bool {
        self.items.iter().any(|r| r.state().is_active())
    }

    /// `(runs, still queued or running)` for the status bar.
    pub fn counts(&self) -> (usize, usize) {
        let active = self.items.iter().filter(|r| r.state().is_active()).count();
        (self.items.len(), active)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::core::pane::{Pane, PaneShared};

    pub(crate) fn run(id: u64, status: &str, conclusion: &str) -> WorkflowRun {
        WorkflowRun {
            id,
            number: id,
            name: "CI".into(),
            workflow_name: "CI".into(),
            status: status.into(),
            conclusion: conclusion.into(),
            head_branch: "main".into(),
            event: "push".into(),
            created_at: "2026-08-28T08:17:23Z".into(),
            updated_at: "2026-08-28T08:18:44Z".into(),
            url: format!("https://github.com/td72/vig/actions/runs/{id}"),
        }
    }

    fn shared() -> PaneShared {
        PaneShared {
            focused_pane: 0,
            previous_pane: 0,
            search: crate::core::search::SearchState::new(),
        }
    }

    fn row_text(run: &WorkflowRun) -> String {
        let now = parse_iso8601("2026-08-28T08:20:23Z").unwrap();
        run_row_spans(run, now)
            .iter()
            .map(|s| s.content.as_ref())
            .collect()
    }

    #[test]
    fn row_shows_icon_workflow_number_branch_event_duration_and_age() {
        let text = row_text(&run(7, "completed", "success"));
        assert_eq!(text, " ✓ CI #7  main  push  1m21s  3m ago");
        let text = row_text(&run(8, "in_progress", ""));
        assert_eq!(text, " ◐ CI #8  main  push  3m00s  3m ago");
        let text = row_text(&run(9, "queued", ""));
        assert_eq!(text, " ◯ CI #9  main  push  3m ago");
        let text = row_text(&run(10, "completed", "failure"));
        assert!(text.starts_with(" ✗ CI #10"));
    }

    #[test]
    fn identity_is_the_database_id_not_the_run_number() {
        let mut r = run(33154751728, "completed", "success");
        r.number = 188;
        assert_eq!(r.number(), 33154751728);
        assert_eq!(r.search_text(), "CI #188 main push");
        assert!(matches!(
            r.browser_event(),
            PaneEvent::OpenUrl(u) if u.ends_with("/runs/33154751728")
        ));
    }

    #[test]
    fn search_covers_workflow_branch_and_event() {
        let mut pane = new_pane(0, 1, 2);
        let mut r = run(1, "completed", "success");
        r.head_branch = "feat/actions-view".into();
        pane.set_items(vec![r, run(2, "completed", "success")]);
        assert_eq!(pane.collect_search_matches(&shared(), "actions").len(), 1);
        assert_eq!(pane.collect_search_matches(&shared(), "push").len(), 2);
        assert_eq!(pane.collect_search_matches(&shared(), "#2").len(), 1);
    }

    #[test]
    fn active_runs_are_counted() {
        let mut pane = new_pane(0, 1, 2);
        assert!(!pane.has_active());
        pane.set_items(vec![
            run(4, "queued", ""),
            run(3, "completed", "success"),
            run(2, "in_progress", ""),
        ]);
        assert_eq!(pane.counts(), (3, 2));
        assert!(pane.has_active());
    }

    #[test]
    fn refresh_keeps_the_selection_on_the_same_run() {
        let mut pane = new_pane(0, 1, 2);
        pane.set_items(vec![
            run(3, "completed", "success"),
            run(2, "in_progress", ""),
        ]);
        pane.selected_idx = 1;
        // A new run is prepended: the selected run moves down one row.
        pane.set_items(vec![
            run(4, "queued", ""),
            run(3, "completed", "success"),
            run(2, "completed", "failure"),
        ]);
        assert_eq!(pane.selected_number(), Some(2));
        assert_eq!(pane.selected_idx, 2);
        // A run that disappeared: the index is clamped instead.
        pane.set_items(vec![run(4, "queued", "")]);
        assert_eq!(pane.selected_idx, 0);
    }
}
