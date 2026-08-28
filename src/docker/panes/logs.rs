//! Logs pane: `docker logs --tail N --timestamps` for the selected container,
//! then `--since <last timestamp>` appends on every poll. Scrollback, follow
//! mode and search come from the shared `TailState`.

use crate::core::app::AppContext;
use crate::core::keymap::{
    nav_bindings, search_bindings, ActionHelp, Keymap, NavAction, SearchAction,
};
use crate::core::pane::{self, HasSearchEsc, Pane, PaneEvent, PaneShared};
use crate::core::search::SearchMatch;
use crate::core::theme;
use crate::core::ui::tail_pane::{TailPane, TailState};
use crate::docker::domain::client::{self, split_timestamp};
use crate::docker::domain::types::Container;
use crate::docker::state::DockerBgMessage;
use crossterm::event::KeyCode;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    Frame,
};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Lines fetched when a container is first selected.
pub const LOG_TAIL: usize = 200;
/// Scrollback cap.
pub const LOG_CAP: usize = 5000;
/// How often the pane asks for new lines while a container is selected.
const POLL_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Debug, Clone)]
pub enum LogsAction {
    Nav(NavAction),
    Search(SearchAction),
    Back,
    Esc,
}

crate::impl_pane_action_from_str!(LogsAction, nav: Nav, search: Search, Back, Esc);

impl HasSearchEsc for LogsAction {
    fn as_search(&self) -> Option<&SearchAction> {
        match self {
            LogsAction::Search(sa) => Some(sa),
            _ => None,
        }
    }
    fn is_esc(&self) -> bool {
        matches!(self, LogsAction::Esc)
    }
}

impl ActionHelp for LogsAction {
    fn label(&self) -> Option<&'static str> {
        match self {
            LogsAction::Nav(NavAction::MoveDown) => Some("Scroll down (pauses follow)"),
            LogsAction::Nav(NavAction::MoveUp) => Some("Scroll up (pauses follow)"),
            LogsAction::Nav(NavAction::JumpBottom) => Some("Bottom / resume follow"),
            LogsAction::Nav(nav) => nav.label(),
            LogsAction::Search(sa) => sa.label(),
            LogsAction::Back => Some("Back to list"),
            LogsAction::Esc => Some("Clear search / back"),
        }
    }
}

pub fn default_keymap() -> Keymap<LogsAction> {
    Keymap::new()
        .bindings(nav_bindings(LogsAction::Nav))
        .bindings(search_bindings(LogsAction::Search))
        .key(KeyCode::Char('h'), LogsAction::Back)
        .key(KeyCode::Esc, LogsAction::Esc)
}

pub struct LogsPane {
    pane_id: usize,
    keymap: Keymap<LogsAction>,
    pub tail: TailState,
    /// `(id, name)` of the container whose logs are shown.
    target: Option<(String, String)>,
    /// Correlates fetch results with the current target; bumped on every reload.
    request_id: u64,
    /// Timestamp of the newest line in the buffer (`--since` for the next poll).
    last_ts: Option<String>,
    in_flight: bool,
    last_poll: Option<Instant>,
    error: Option<String>,
}

impl LogsPane {
    pub fn new(pane_id: usize) -> Self {
        Self {
            pane_id,
            keymap: default_keymap(),
            tail: TailState::new(LOG_CAP),
            target: None,
            request_id: 0,
            last_ts: None,
            in_flight: false,
            last_poll: None,
            error: None,
        }
    }

    pub fn set_keymap(&mut self, km: Keymap<LogsAction>) {
        self.keymap = km;
    }

    pub fn keymap(&self) -> &Keymap<LogsAction> {
        &self.keymap
    }

    /// Follow `container` (or nothing). Re-selecting the same container keeps
    /// the buffer; anything else starts over with a fresh tail.
    pub fn load(&mut self, container: Option<&Container>, tx: &mpsc::Sender<DockerBgMessage>) {
        let same = match (&self.target, container) {
            (Some((id, _)), Some(c)) => *id == c.id,
            (None, None) => true,
            _ => false,
        };
        if same {
            return;
        }
        self.target = container.map(|c| (c.id.clone(), c.name.clone()));
        self.restart(tx);
    }

    /// Drop the buffer and fetch the tail again.
    pub fn refresh(&mut self, tx: &mpsc::Sender<DockerBgMessage>) {
        self.restart(tx);
    }

    fn restart(&mut self, tx: &mpsc::Sender<DockerBgMessage>) {
        self.tail.clear();
        self.last_ts = None;
        self.error = None;
        self.in_flight = false;
        self.request_id += 1;
        if self.target.is_some() {
            self.spawn(tx, false);
        }
    }

    /// Poll for new lines once per [`POLL_INTERVAL`].
    pub fn on_tick(&mut self, tx: &mpsc::Sender<DockerBgMessage>) {
        if self.target.is_none() || self.in_flight {
            return;
        }
        let due = self.last_poll.is_none_or(|t| t.elapsed() >= POLL_INTERVAL);
        if due {
            self.spawn(tx, true);
        }
    }

    fn spawn(&mut self, tx: &mpsc::Sender<DockerBgMessage>, append: bool) {
        let Some((id, _)) = &self.target else {
            return;
        };
        let id = id.clone();
        let since = if append { self.last_ts.clone() } else { None };
        let request_id = self.request_id;
        self.in_flight = true;
        self.last_poll = Some(Instant::now());
        let tx = tx.clone();
        std::thread::spawn(move || {
            let result = client::fetch_logs(&id, LOG_TAIL, since.as_deref());
            let _ = tx.send(DockerBgMessage::Logs {
                request_id,
                append,
                result,
            });
        });
    }

    /// Apply a fetch result; results from a previous target are dropped.
    pub fn apply(&mut self, request_id: u64, append: bool, result: Result<Vec<String>, String>) {
        if request_id != self.request_id {
            return;
        }
        self.in_flight = false;
        match result {
            Ok(lines) => {
                self.error = None;
                let lines = if append && self.last_ts.is_some() {
                    let last_ts = self.last_ts.clone().unwrap_or_default();
                    let tail: Vec<String> = self.tail.last_lines(64).map(str::to_string).collect();
                    new_lines_since(&tail, &last_ts, lines)
                } else {
                    lines
                };
                if let Some(ts) = lines.iter().rev().find_map(|l| split_timestamp(l).0) {
                    self.last_ts = Some(ts.to_string());
                }
                if append {
                    self.tail.extend(lines);
                } else {
                    self.tail.set_lines(lines);
                }
            }
            Err(e) => self.error = Some(e),
        }
    }

    fn execute(&mut self, shared: &PaneShared, action: LogsAction) -> Vec<PaneEvent> {
        let esc_fallback = vec![PaneEvent::SetFocus(shared.previous_pane)];
        if let Some(events) =
            pane::try_dispatch_search_esc(&action, shared, self.pane_id, esc_fallback)
        {
            return events;
        }
        match action {
            LogsAction::Nav(nav) => {
                self.tail.apply_nav(nav);
                vec![]
            }
            LogsAction::Back => vec![PaneEvent::SetFocus(shared.previous_pane)],
            LogsAction::Search(_) | LogsAction::Esc => vec![],
        }
    }
}

/// Lines from a `--since <last_ts>` fetch that are not already in the buffer:
/// everything older than `last_ts` is dropped, and lines stamped exactly
/// `last_ts` are kept only if the buffer's tail does not already hold them.
pub fn new_lines_since(
    buffer_tail: &[String],
    last_ts: &str,
    incoming: Vec<String>,
) -> Vec<String> {
    incoming
        .into_iter()
        .filter(|line| {
            let ts = split_timestamp(line).0.unwrap_or("");
            match ts.cmp(last_ts) {
                std::cmp::Ordering::Less => false,
                std::cmp::Ordering::Greater => true,
                std::cmp::Ordering::Equal => !buffer_tail.iter().any(|b| b == line),
            }
        })
        .collect()
}

/// `2026-08-27T09:05:29.121249516Z message` → dim `09:05:29` + message.
fn log_line(raw: &str) -> Line<'static> {
    match split_timestamp(raw) {
        (Some(ts), rest) => {
            let clock: String = ts.chars().skip(11).take(8).collect();
            Line::from(vec![
                Span::styled(format!(" {clock} "), Style::default().fg(Color::DarkGray)),
                Span::raw(rest.to_string()),
            ])
        }
        (None, rest) => Line::from(Span::raw(format!(" {rest}"))),
    }
}

impl Pane<PaneEvent> for LogsPane {
    crate::impl_handle_key!(keymap);

    fn render(&mut self, f: &mut Frame, _ctx: &AppContext, shared: &PaneShared, area: Rect) {
        let title = match &self.target {
            Some((_, name)) => format!("Logs: {name} [{}]", self.tail.mode_label()),
            None => "Logs".to_string(),
        };
        let block = theme::pane_block(&title, shared.focused_pane == self.pane_id);
        let empty = match (&self.error, &self.target) {
            (Some(e), _) => format!("Error: {e}"),
            (None, None) => "Select a container to tail its logs".to_string(),
            (None, Some(_)) if self.in_flight => "Loading...".to_string(),
            (None, Some(_)) => "(no output yet)".to_string(),
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
    use std::collections::BTreeMap;

    fn container(id: &str) -> Container {
        Container {
            id: id.into(),
            name: format!("name-{id}"),
            image: "img".into(),
            state: "running".into(),
            status: String::new(),
            ports: String::new(),
            labels: BTreeMap::new(),
        }
    }

    #[test]
    fn since_filter_drops_old_and_duplicate_lines() {
        let tail = vec![
            "2026-01-01T00:00:01Z a".to_string(),
            "2026-01-01T00:00:02Z b".to_string(),
            "2026-01-01T00:00:02Z c".to_string(),
        ];
        let incoming = vec![
            "2026-01-01T00:00:01Z a".to_string(),
            "2026-01-01T00:00:02Z b".to_string(),
            "2026-01-01T00:00:02Z c".to_string(),
            "2026-01-01T00:00:02Z d".to_string(),
            "2026-01-01T00:00:03Z e".to_string(),
        ];
        assert_eq!(
            new_lines_since(&tail, "2026-01-01T00:00:02Z", incoming),
            ["2026-01-01T00:00:02Z d", "2026-01-01T00:00:03Z e"]
        );
    }

    #[test]
    fn apply_tracks_last_timestamp_and_ignores_stale_requests() {
        let (tx, _rx) = mpsc::channel();
        let mut pane = LogsPane::new(0);
        pane.load(Some(&container("x")), &tx);
        let rid = pane.request_id;
        assert!(pane.in_flight);
        pane.apply(
            rid,
            false,
            Ok(vec![
                "2026-01-01T00:00:01Z one".into(),
                "2026-01-01T00:00:02Z two".into(),
            ]),
        );
        assert_eq!(pane.tail.len(), 2);
        assert_eq!(pane.last_ts.as_deref(), Some("2026-01-01T00:00:02Z"));
        assert!(!pane.in_flight);
        // Append with the overlap docker re-sends for `--since`.
        pane.apply(
            rid,
            true,
            Ok(vec![
                "2026-01-01T00:00:02Z two".into(),
                "2026-01-01T00:00:03Z three".into(),
            ]),
        );
        assert_eq!(pane.tail.len(), 3);
        assert_eq!(pane.last_ts.as_deref(), Some("2026-01-01T00:00:03Z"));
        // Switching containers invalidates in-flight results.
        pane.load(Some(&container("y")), &tx);
        assert!(pane.tail.is_empty());
        pane.apply(rid, true, Ok(vec!["2026-01-01T00:00:09Z late".into()]));
        assert!(pane.tail.is_empty());
        // Re-selecting the same container keeps the current request.
        let before = pane.request_id;
        pane.load(Some(&container("y")), &tx);
        assert_eq!(pane.request_id, before);
        pane.load(None, &tx);
        assert!(pane.target.is_none());
    }
}
