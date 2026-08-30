//! Runs pane: the latest workflow runs from `gh run list`, newest first.
//! Status icon, workflow, run number, branch, event, duration (elapsed
//! while running) and how long ago the run was created.

use crate::actions::domain::time::{duration_between, format_relative, now_secs, parse_iso8601};
use crate::actions::domain::types::{RunState, WorkflowRun};
use crate::core::app::AppContext;
use crate::core::keymap::{
    nav_bindings, search_bindings, ActionHelp, Keymap, NavAction, SearchAction,
};
use crate::core::pane::{self, Pane, PaneEvent, PaneShared};
use crate::core::search::SearchMatch;
use crate::core::theme;
use crossterm::event::KeyCode;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::ListItem,
    Frame,
};

#[derive(Debug, Clone)]
pub enum RunsAction {
    Nav(NavAction),
    Search(SearchAction),
    OpenDetail,
    OpenBrowser,
    Esc,
}

crate::impl_pane_action_from_str!(
    RunsAction, nav: Nav, search: Search, esc: Esc,
    OpenDetail, OpenBrowser
);

impl ActionHelp for RunsAction {
    fn label(&self) -> Option<&'static str> {
        match self {
            RunsAction::Nav(nav) => nav.label(),
            RunsAction::Search(sa) => sa.label(),
            RunsAction::OpenDetail => Some("Focus jobs"),
            RunsAction::OpenBrowser => Some("Open run in browser"),
            RunsAction::Esc => Some("Clear search"),
        }
    }
}

pub fn default_keymap() -> Keymap<RunsAction> {
    Keymap::new()
        .bindings(nav_bindings(RunsAction::Nav))
        .bindings(search_bindings(RunsAction::Search))
        .key(KeyCode::Char('i'), RunsAction::OpenDetail)
        .key(KeyCode::Enter, RunsAction::OpenDetail)
        .key(KeyCode::Char('o'), RunsAction::OpenBrowser)
        .key(KeyCode::Esc, RunsAction::Esc)
}

pub fn state_color(state: RunState) -> Color {
    match state {
        RunState::Queued => Color::DarkGray,
        RunState::InProgress => Color::Yellow,
        RunState::Success => Color::Green,
        RunState::Failure => Color::Red,
        RunState::Cancelled | RunState::Skipped | RunState::Other => Color::DarkGray,
    }
}

pub struct RunsPane {
    pub runs: Vec<WorkflowRun>,
    pub selected_idx: usize,
    loading: bool,
    keymap: Keymap<RunsAction>,
    pane_id: usize,
    jobs_pane_id: usize,
    log_pane_id: usize,
    view_height: u16,
}

impl RunsPane {
    pub fn new(pane_id: usize, jobs_pane_id: usize, log_pane_id: usize) -> Self {
        Self {
            runs: Vec::new(),
            selected_idx: 0,
            loading: false,
            keymap: default_keymap(),
            pane_id,
            jobs_pane_id,
            log_pane_id,
            view_height: 20,
        }
    }

    pub fn set_keymap(&mut self, km: Keymap<RunsAction>) {
        self.keymap = km;
    }

    pub fn keymap(&self) -> &Keymap<RunsAction> {
        &self.keymap
    }

    pub fn set_loading(&mut self, loading: bool) {
        self.loading = loading;
    }

    pub fn is_loading(&self) -> bool {
        self.loading
    }

    pub fn selected(&self) -> Option<&WorkflowRun> {
        self.runs.get(self.selected_idx)
    }

    /// `(runs, still queued or running)`.
    pub fn counts(&self) -> (usize, usize) {
        let active = self.runs.iter().filter(|r| r.state().is_active()).count();
        (self.runs.len(), active)
    }

    /// Whether any listed run is still queued or in progress.
    pub fn has_active(&self) -> bool {
        self.counts().1 > 0
    }

    /// Replace the list, keeping the selection on the same run when possible.
    pub fn set_runs(&mut self, runs: Vec<WorkflowRun>) {
        let keep = self.selected().map(|r| r.id);
        self.runs = runs;
        self.selected_idx = keep
            .and_then(|id| self.runs.iter().position(|r| r.id == id))
            .unwrap_or(self.selected_idx)
            .min(self.runs.len().saturating_sub(1));
    }

    fn execute(&mut self, shared: &PaneShared, action: RunsAction) -> Vec<PaneEvent> {
        if let Some(events) = pane::try_dispatch_search_esc(&action, shared, self.pane_id, vec![]) {
            return events;
        }
        match action {
            RunsAction::Nav(nav) => pane::execute_list_nav(
                nav,
                &mut self.selected_idx,
                self.runs.len(),
                Some(self.view_height),
            ),
            RunsAction::OpenDetail if !self.runs.is_empty() => {
                vec![PaneEvent::SetFocus(self.jobs_pane_id)]
            }
            RunsAction::OpenBrowser => match self.selected() {
                Some(run) if !run.url.is_empty() => vec![PaneEvent::OpenUrl(run.url.clone())],
                _ => vec![],
            },
            _ => vec![],
        }
    }

    fn search_text(run: &WorkflowRun) -> String {
        format!(
            "{} #{} {} {}",
            run.title(),
            run.number,
            run.head_branch,
            run.event
        )
    }

    fn render_row(run: &WorkflowRun, now: i64) -> ListItem<'static> {
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
        ListItem::new(Line::from(spans))
    }
}

impl Pane<PaneEvent> for RunsPane {
    crate::impl_handle_key!(keymap);

    fn render(&mut self, f: &mut Frame, _ctx: &AppContext, shared: &PaneShared, area: Rect) {
        self.view_height = area.height.saturating_sub(2);
        let empty = if self.loading && self.runs.is_empty() {
            Some("Loading...")
        } else if self.runs.is_empty() {
            Some("No workflow runs")
        } else {
            None
        };
        let is_focused = shared.focused_pane == self.pane_id;
        let show_selection = is_focused
            || shared.focused_pane == self.jobs_pane_id
            || shared.focused_pane == self.log_pane_id;
        let selected = show_selection.then_some(self.selected_idx);
        let now = now_secs();
        theme::render_list_pane(
            f,
            area,
            shared,
            self.pane_id,
            "Runs",
            selected,
            empty,
            |match_set, current_match_idx| {
                self.runs
                    .iter()
                    .enumerate()
                    .map(|(idx, run)| {
                        let mut li = Self::render_row(run, now);
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
        pane::collect_list_search_matches(&self.runs, query, Self::search_text)
    }

    crate::impl_list_pane_selection!();
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn set_runs_keeps_selection_by_id_and_counts_active() {
        let mut pane = RunsPane::new(0, 1, 2);
        pane.set_runs(vec![
            run(3, "completed", "success"),
            run(2, "in_progress", ""),
        ]);
        pane.selected_idx = 1;
        pane.set_runs(vec![
            run(4, "queued", ""),
            run(3, "completed", "success"),
            run(2, "completed", "failure"),
        ]);
        assert_eq!(pane.selected().unwrap().id, 2);
        assert_eq!(pane.counts(), (3, 1));
        assert!(pane.has_active());
        pane.set_runs(vec![]);
        assert_eq!(pane.selected_idx, 0);
        assert!(!pane.has_active());
    }

    #[test]
    fn search_covers_workflow_branch_and_event() {
        let mut pane = RunsPane::new(0, 1, 2);
        let mut r = run(1, "completed", "success");
        r.head_branch = "feat/actions-view".into();
        pane.set_runs(vec![r, run(2, "completed", "success")]);
        let shared = PaneShared {
            focused_pane: 0,
            previous_pane: 0,
            search: crate::core::search::SearchState::new(),
        };
        assert_eq!(pane.collect_search_matches(&shared, "actions").len(), 1);
        assert_eq!(pane.collect_search_matches(&shared, "push").len(), 2);
        assert_eq!(pane.collect_search_matches(&shared, "#2").len(), 1);
    }

    #[test]
    fn open_browser_emits_the_run_url() {
        let mut pane = RunsPane::new(0, 1, 2);
        pane.set_runs(vec![run(7, "completed", "success")]);
        let shared = PaneShared {
            focused_pane: 0,
            previous_pane: 0,
            search: crate::core::search::SearchState::new(),
        };
        let events = pane.execute(&shared, RunsAction::OpenBrowser);
        assert!(matches!(
            events.as_slice(),
            [PaneEvent::OpenUrl(u)] if u.ends_with("/runs/7")
        ));
        let events = pane.execute(&shared, RunsAction::OpenDetail);
        assert!(matches!(events.as_slice(), [PaneEvent::SetFocus(1)]));
    }
}
