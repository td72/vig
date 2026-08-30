//! Log pane: the selected job's log (`gh run view --log --job`) in the
//! shared `TailPane`. Step boundaries and `##[group]` markers are section
//! lines; `]` / `[` jump between failed steps. Jobs still running are
//! polled every few seconds and the new lines appended.

use crate::actions::domain::client;
use crate::actions::domain::log::{
    decode, failed_step_lines, new_tail, parse_job_log, step_line, LogLine,
};
use crate::actions::panes::jobs::LogTarget;
use crate::actions::state::ActionsBgMessage;
use crate::core::app::AppContext;
use crate::core::keymap::{
    nav_bindings, search_bindings, ActionHelp, Keymap, NavAction, SearchAction,
};
use crate::core::pane::{self, Pane, PaneEvent, PaneShared};
use crate::core::search::SearchMatch;
use crate::core::theme;
use crate::core::ui::tail_pane::{TailPane, TailState};
use crossterm::event::KeyCode;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    Frame,
};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Scrollback cap (GitHub logs can run to tens of thousands of lines).
pub const LOG_CAP: usize = 20_000;
/// How often a running job's log is re-fetched.
const POLL_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub enum LogAction {
    Nav(NavAction),
    Search(SearchAction),
    NextFailed,
    PrevFailed,
    Back,
    Esc,
}

crate::impl_pane_action_from_str!(
    LogAction, nav: Nav, search: Search, esc: Esc,
    NextFailed, PrevFailed, Back
);

impl ActionHelp for LogAction {
    fn label(&self) -> Option<&'static str> {
        match self {
            LogAction::Nav(NavAction::MoveDown) => Some("Scroll down (pauses follow)"),
            LogAction::Nav(NavAction::MoveUp) => Some("Scroll up (pauses follow)"),
            LogAction::Nav(NavAction::JumpBottom) => Some("Bottom / resume follow"),
            LogAction::Nav(nav) => nav.label(),
            LogAction::Search(sa) => sa.label(),
            LogAction::NextFailed => Some("Next failed step"),
            LogAction::PrevFailed => Some("Prev failed step"),
            LogAction::Back => Some("Back to jobs"),
            LogAction::Esc => Some("Clear search / back"),
        }
    }
}

pub fn default_keymap() -> Keymap<LogAction> {
    Keymap::new()
        .bindings(nav_bindings(LogAction::Nav))
        .bindings(search_bindings(LogAction::Search))
        .key(KeyCode::Char(']'), LogAction::NextFailed)
        .key(KeyCode::Char('['), LogAction::PrevFailed)
        .key(KeyCode::Char('h'), LogAction::Back)
        .key(KeyCode::Esc, LogAction::Esc)
}

pub struct LogPane {
    pane_id: usize,
    keymap: Keymap<LogAction>,
    pub tail: TailState,
    target: Option<LogTarget>,
    /// Correlates fetch results with the current target; bumped on reload.
    request_id: u64,
    in_flight: bool,
    last_poll: Option<Instant>,
    error: Option<String>,
    /// Step to scroll to once the log has arrived.
    pending_jump: Option<String>,
}

impl LogPane {
    pub fn new(pane_id: usize) -> Self {
        Self {
            pane_id,
            keymap: default_keymap(),
            tail: TailState::new(LOG_CAP),
            target: None,
            request_id: 0,
            in_flight: false,
            last_poll: None,
            error: None,
            pending_jump: None,
        }
    }

    pub fn set_keymap(&mut self, km: Keymap<LogAction>) {
        self.keymap = km;
    }

    pub fn keymap(&self) -> &Keymap<LogAction> {
        &self.keymap
    }

    pub fn target(&self) -> Option<&LogTarget> {
        self.target.as_ref()
    }

    pub fn is_loading(&self) -> bool {
        self.in_flight && self.tail.is_empty()
    }

    /// Show `target`'s log, scrolling to `step` once loaded. Re-opening the
    /// job that is already shown keeps the buffer and only performs the jump.
    pub fn load(
        &mut self,
        target: LogTarget,
        step: Option<String>,
        tx: &mpsc::Sender<ActionsBgMessage>,
    ) {
        let same = self
            .target
            .as_ref()
            .is_some_and(|t| t.job_id == target.job_id);
        if same {
            self.target = Some(target);
            match step {
                Some(name) if !self.tail.is_empty() => self.jump_to_step(&name),
                Some(name) => self.pending_jump = Some(name),
                None => {}
            }
            return;
        }
        self.target = Some(target);
        self.pending_jump = step;
        self.restart(tx);
    }

    /// Forget the current job (its run is gone from the list).
    pub fn clear(&mut self) {
        self.target = None;
        self.pending_jump = None;
        self.error = None;
        self.in_flight = false;
        self.request_id += 1;
        self.tail.clear();
    }

    /// The jobs list was refreshed: pick up the job's new state. A job that
    /// just finished gets one last full fetch so the buffer is complete.
    pub fn update_target(&mut self, latest: LogTarget, tx: &mpsc::Sender<ActionsBgMessage>) {
        let Some(current) = &self.target else {
            return;
        };
        if current.job_id != latest.job_id {
            return;
        }
        let finished = current.in_progress && !latest.in_progress;
        self.target = Some(latest);
        if finished {
            self.spawn(tx, true);
        }
    }

    /// `r`: drop the buffer and fetch again.
    pub fn refresh(&mut self, tx: &mpsc::Sender<ActionsBgMessage>) {
        self.restart(tx);
    }

    fn restart(&mut self, tx: &mpsc::Sender<ActionsBgMessage>) {
        self.tail.clear();
        self.error = None;
        self.in_flight = false;
        self.request_id += 1;
        if self.target.is_some() {
            self.spawn(tx, false);
        }
    }

    /// Poll a running job once per [`POLL_INTERVAL`].
    pub fn on_tick(&mut self, tx: &mpsc::Sender<ActionsBgMessage>) {
        let running = self.target.as_ref().is_some_and(|t| t.in_progress);
        if !running || self.in_flight {
            return;
        }
        let due = self.last_poll.is_none_or(|t| t.elapsed() >= POLL_INTERVAL);
        if due {
            self.spawn(tx, true);
        }
    }

    fn spawn(&mut self, tx: &mpsc::Sender<ActionsBgMessage>, append: bool) {
        let Some(target) = &self.target else {
            return;
        };
        let (run_id, job_id, in_progress) = (target.run_id, target.job_id, target.in_progress);
        let request_id = self.request_id;
        self.in_flight = true;
        self.last_poll = Some(Instant::now());
        let tx = tx.clone();
        std::thread::spawn(move || {
            let result =
                client::fetch_job_log(run_id, job_id, in_progress).map(|raw| parse_job_log(&raw));
            let _ = tx.send(ActionsBgMessage::Log {
                request_id,
                append,
                result,
            });
        });
    }

    /// Apply a fetch result; results for a previous target are dropped.
    pub fn apply(&mut self, request_id: u64, append: bool, result: Result<Vec<String>, String>) {
        if request_id != self.request_id {
            return;
        }
        self.in_flight = false;
        match result {
            Ok(lines) => {
                self.error = None;
                if append {
                    match new_tail(self.tail.len(), self.tail.lines(), lines) {
                        Ok(tail) => self.tail.extend(tail),
                        Err(all) => self.tail.set_lines(all),
                    }
                } else {
                    self.tail.set_lines(lines);
                }
                if let Some(name) = self.pending_jump.take() {
                    self.jump_to_step(&name);
                }
            }
            Err(e) => {
                // A job that has not written anything yet is not an error.
                if self.tail.is_empty() && self.target.as_ref().is_some_and(|t| t.in_progress) {
                    self.error = None;
                } else {
                    self.error = Some(e);
                }
            }
        }
    }

    fn jump_to_step(&mut self, name: &str) {
        if let Some(idx) = step_line(self.tail.lines(), name) {
            self.tail.scroll_to(idx);
        }
    }

    /// Header lines of the failed steps, in buffer order.
    fn failed_lines(&self) -> Vec<usize> {
        match &self.target {
            Some(t) => failed_step_lines(self.tail.lines(), &t.failed_steps),
            None => vec![],
        }
    }

    /// `]` / `[`: the next / previous failed step relative to the top of
    /// the view, wrapping around. `]` from the end of the buffer (the
    /// initial follow position) therefore lands on the first failed step.
    fn jump_failed(&mut self, forward: bool) -> Vec<PaneEvent> {
        let lines = self.failed_lines();
        if lines.is_empty() {
            return vec![PaneEvent::StatusMessage("No failed steps".to_string())];
        }
        let top = self.tail.top();
        let target = if forward {
            lines.iter().copied().find(|&l| l > top).unwrap_or(lines[0])
        } else {
            lines
                .iter()
                .rev()
                .copied()
                .find(|&l| l < top)
                .unwrap_or(lines[lines.len() - 1])
        };
        self.tail.scroll_to(target);
        let pos = lines.iter().position(|&l| l == target).unwrap_or(0);
        vec![PaneEvent::StatusMessage(format!(
            "Failed step [{}/{}]",
            pos + 1,
            lines.len()
        ))]
    }

    fn execute(&mut self, shared: &PaneShared, action: LogAction) -> Vec<PaneEvent> {
        let esc_fallback = vec![PaneEvent::SetFocus(shared.previous_pane)];
        if let Some(events) =
            pane::try_dispatch_search_esc(&action, shared, self.pane_id, esc_fallback)
        {
            return events;
        }
        match action {
            LogAction::Nav(nav) => {
                self.tail.apply_nav(nav);
                vec![]
            }
            LogAction::NextFailed => self.jump_failed(true),
            LogAction::PrevFailed => self.jump_failed(false),
            LogAction::Back => vec![PaneEvent::SetFocus(shared.previous_pane)],
            LogAction::Search(_) | LogAction::Esc => vec![],
        }
    }
}

/// Style one buffer line: step and group headers as section lines, dim
/// clocks, red errors and yellow warnings.
fn log_line(raw: &str) -> Line<'static> {
    let dim = Style::default().fg(Color::DarkGray);
    match decode(raw) {
        LogLine::Step(name) => Line::from(vec![
            Span::styled(" ▶ ", Style::default().fg(Color::Cyan)),
            Span::styled(
                name.to_string(),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        LogLine::Group(title) => Line::from(vec![
            Span::styled("   ▸ ", Style::default().fg(Color::Blue)),
            Span::styled(title.to_string(), Style::default().fg(Color::Blue)),
        ]),
        LogLine::Error(msg) => Line::from(vec![
            Span::styled("   ✗ ", Style::default().fg(Color::Red)),
            Span::styled(msg.to_string(), Style::default().fg(Color::Red)),
        ]),
        LogLine::Warning(msg) => Line::from(vec![
            Span::styled("   ! ", Style::default().fg(Color::Yellow)),
            Span::styled(msg.to_string(), Style::default().fg(Color::Yellow)),
        ]),
        LogLine::Text { clock, text } => match clock {
            Some(clock) => Line::from(vec![
                Span::styled(format!(" {clock} "), dim),
                Span::raw(text.to_string()),
            ]),
            None => Line::from(Span::raw(format!(" {text}"))),
        },
    }
}

impl Pane<PaneEvent> for LogPane {
    crate::impl_handle_key!(keymap);

    fn render(&mut self, f: &mut Frame, _ctx: &AppContext, shared: &PaneShared, area: Rect) {
        let title = match &self.target {
            Some(t) if t.in_progress => {
                format!("Log: {} [{}]", t.job_name, self.tail.mode_label())
            }
            Some(t) => format!("Log: {}", t.job_name),
            None => "Log".to_string(),
        };
        let block = theme::pane_block(&title, shared.focused_pane == self.pane_id);
        let empty = match (&self.error, &self.target) {
            (Some(e), _) => format!("Error: {e}"),
            (None, None) => "Select a job and press Enter to show its log".to_string(),
            (None, Some(_)) if self.in_flight => "Loading...".to_string(),
            (None, Some(t)) if t.in_progress => "(no output yet)".to_string(),
            (None, Some(_)) => "(empty log)".to_string(),
        };
        let (match_set, current) = theme::list_search_highlights(shared, self.pane_id);
        TailPane::new(block)
            .empty_message(&empty)
            .highlights(&match_set, current)
            .formatter(log_line)
            .render(f, area, &mut self.tail);
    }

    fn collect_search_matches(&self, _shared: &PaneShared, query: &str) -> Vec<SearchMatch> {
        self.tail.search_matches(query)
    }

    fn jump_to_match(&mut self, _shared: &PaneShared, search_match: &SearchMatch) {
        self.tail.jump_to_match(search_match);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::domain::log::encode_step;

    fn target(job_id: u64, in_progress: bool, failed: &[&str]) -> LogTarget {
        LogTarget {
            run_id: 1,
            job_id,
            job_name: format!("job {job_id}"),
            in_progress,
            failed_steps: failed.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn shared() -> PaneShared {
        PaneShared {
            focused_pane: 2,
            previous_pane: 1,
            search: crate::core::search::SearchState::new(),
        }
    }

    fn lines() -> Vec<String> {
        vec![
            encode_step("Set up job"),
            "2026-08-28T08:17:27.0000000Z runner".into(),
            encode_step("cargo test"),
            "2026-08-28T08:17:46.0000000Z running".into(),
            "2026-08-28T08:18:11.0000000Z ##[error]exit 101".into(),
            encode_step("cargo build"),
            "2026-08-28T08:18:12.0000000Z ##[error]skipped".into(),
            encode_step("Complete job"),
            "2026-08-28T08:18:13.0000000Z done".into(),
        ]
    }

    #[test]
    fn load_apply_and_stale_requests() {
        let (tx, _rx) = mpsc::channel();
        let mut pane = LogPane::new(2);
        pane.load(target(10, false, &[]), None, &tx);
        let rid = pane.request_id;
        assert!(pane.in_flight);
        assert!(pane.is_loading());
        pane.apply(rid, false, Ok(lines()));
        assert_eq!(pane.tail.len(), 9);
        assert!(!pane.in_flight);
        // Re-opening the same job keeps the buffer.
        pane.load(target(10, false, &[]), None, &tx);
        assert_eq!(pane.request_id, rid);
        assert_eq!(pane.tail.len(), 9);
        // Another job starts over; the old result is dropped.
        pane.load(target(11, false, &[]), None, &tx);
        assert!(pane.tail.is_empty());
        pane.apply(rid, false, Ok(lines()));
        assert!(pane.tail.is_empty());
        pane.apply(pane.request_id, false, Err("boom".into()));
        assert_eq!(pane.error.as_deref(), Some("boom"));
        pane.clear();
        assert!(pane.target().is_none());
    }

    #[test]
    fn step_jump_waits_for_the_log() {
        let (tx, _rx) = mpsc::channel();
        let mut pane = LogPane::new(2);
        pane.tail.set_view_height(3);
        pane.load(
            target(10, false, &["cargo test"]),
            Some("cargo build".into()),
            &tx,
        );
        assert_eq!(pane.pending_jump.as_deref(), Some("cargo build"));
        pane.apply(pane.request_id, false, Ok(lines()));
        assert!(pane.pending_jump.is_none());
        assert_eq!(pane.tail.top(), 5);
        assert!(!pane.tail.is_following());
        // Same job, another step: immediate jump.
        pane.load(
            target(10, false, &["cargo test"]),
            Some("Set up job".into()),
            &tx,
        );
        assert_eq!(pane.tail.top(), 0);
    }

    #[test]
    fn failed_step_jumps_wrap_around() {
        let (tx, _rx) = mpsc::channel();
        let mut pane = LogPane::new(2);
        pane.tail.set_view_height(3);
        pane.load(target(10, false, &["cargo test", "cargo build"]), None, &tx);
        pane.apply(pane.request_id, false, Ok(lines()));
        // Following the end: `]` lands on the first failed step.
        assert!(pane.tail.is_following());
        let ev = pane.execute(&shared(), LogAction::NextFailed);
        assert!(matches!(ev.as_slice(), [PaneEvent::StatusMessage(m)] if m == "Failed step [1/2]"));
        assert_eq!(pane.tail.top(), 2);
        pane.execute(&shared(), LogAction::NextFailed);
        assert_eq!(pane.tail.top(), 5);
        // Past the last one: wraps to the first.
        pane.execute(&shared(), LogAction::NextFailed);
        assert_eq!(pane.tail.top(), 2);
        pane.execute(&shared(), LogAction::PrevFailed);
        assert_eq!(pane.tail.top(), 5);
        // No failures: a status message, no scroll.
        pane.load(target(11, false, &[]), None, &tx);
        pane.apply(pane.request_id, false, Ok(lines()));
        let ev = pane.execute(&shared(), LogAction::NextFailed);
        assert!(matches!(ev.as_slice(), [PaneEvent::StatusMessage(m)] if m == "No failed steps"));
    }

    #[test]
    fn running_jobs_append_and_finish_with_a_final_fetch() {
        let (tx, rx) = mpsc::channel();
        let mut pane = LogPane::new(2);
        pane.load(target(10, true, &[]), None, &tx);
        let rid = pane.request_id;
        // Nothing written yet is not an error while the job runs.
        pane.apply(rid, false, Err("HTTP 404".into()));
        assert!(pane.error.is_none());
        pane.apply(rid, true, Ok(lines()[..2].to_vec()));
        assert_eq!(pane.tail.len(), 2);
        pane.apply(rid, true, Ok(lines()[..4].to_vec()));
        assert_eq!(pane.tail.len(), 4);
        // A rewritten log replaces the buffer.
        pane.apply(rid, true, Ok(vec!["x".into(), "y".into()]));
        assert_eq!(pane.tail.len(), 2);
        // The jobs list reports completion: one more fetch is queued.
        drop(rx);
        pane.in_flight = false;
        pane.update_target(target(10, false, &["cargo test"]), &tx);
        assert!(pane.in_flight);
        assert_eq!(pane.target().unwrap().failed_steps, ["cargo test"]);
        // Updates for other jobs are ignored.
        pane.update_target(target(99, false, &[]), &tx);
        assert_eq!(pane.target().unwrap().job_id, 10);
    }

    #[test]
    fn back_and_esc_return_to_the_previous_pane() {
        let mut pane = LogPane::new(2);
        assert!(matches!(
            pane.execute(&shared(), LogAction::Back).as_slice(),
            [PaneEvent::SetFocus(1)]
        ));
        assert!(matches!(
            pane.execute(&shared(), LogAction::Esc).as_slice(),
            [PaneEvent::SetFocus(1)]
        ));
    }
}
