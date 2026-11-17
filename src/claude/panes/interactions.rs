//! Interactions pane: the repository's sessions, the most recently
//! updated first, each with its latest interactions (a prompt and
//! everything done to answer it) under it, the latest first.
//!
//! The latest few sessions are read and unfolded to begin with; another
//! is read when it is opened. `t` lists the interactions of the sessions
//! that are read all together instead. The selection stays on the latest
//! interaction while it sits on the first one.

use super::{
    ago, dim, duration, find_interaction, fit, interaction_summary, ms, now_ms, right,
    session_summary, span_counts, state_of, status, took_ms, tools_used, Chosen, InteractionKey,
};
use crate::claude::domain::types::{SessionEntry, TextLevel};
use crate::core::app::AppContext;
use crate::core::keymap::{ActionHelp, Keymap, NavAction, SearchAction};
use crate::core::pane::{self, Pane, PaneEvent, PaneShared};
use crate::core::search::SearchMatch;
use crate::core::theme;
use iktinos::session::Session;
use iktinos::trace::Interaction;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::ListItem,
    Frame,
};
use std::collections::HashSet;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum InteractionsAction {
    Nav(NavAction),
    /// An interaction: into its tree. A session: read it, fold / unfold it.
    Open,
    /// Fold / unfold the session the selection is in.
    ToggleFold,
    /// By session, or all interactions together.
    ToggleFlat,
    Search(SearchAction),
    Esc,
}

crate::impl_pane_action_from_str!(
    InteractionsAction,
    nav: Nav,
    search: Search,
    esc: Esc,
    Open,
    ToggleFold,
    ToggleFlat
);

impl ActionHelp for InteractionsAction {
    fn label(&self) -> Option<&'static str> {
        match self {
            InteractionsAction::Nav(nav) => nav.label(),
            InteractionsAction::Open => Some("Into the interaction's tree / open the session"),
            InteractionsAction::ToggleFold => Some("Fold / unfold the session"),
            InteractionsAction::ToggleFlat => Some("By session / all interactions together"),
            InteractionsAction::Search(sa) => sa.label(),
            InteractionsAction::Esc => Some("Clear search"),
        }
    }
}

/// Columns right of the text: llm, tools, took, state.
const COUNTS_WIDTH: usize = 4 + 1 + 5 + 1 + 7 + 1 + 8;
const WHEN_WIDTH: usize = 8;

/// One row of the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    /// `sessions[i]`.
    Session(usize),
    /// `sessions[i]`'s interaction `j` of its trace.
    Interaction(usize, usize),
}

/// What a row is, independent of its position: survives an update.
#[derive(Debug, Clone, PartialEq, Eq)]
enum RowKey {
    Session(String),
    Interaction(String, InteractionKey),
}

pub struct InteractionsPane {
    sessions: Vec<SessionEntry>,
    /// Sessions that are read and folded, by id.
    folded: HashSet<String>,
    /// Sessions that were opened and are not read yet, by id.
    opening: HashSet<String>,
    /// All interactions together, not under their sessions.
    flat: bool,
    rows: Vec<Row>,
    pub selected_idx: usize,
    level: TextLevel,
    keymap: Keymap<InteractionsAction>,
    pane_id: usize,
    trace_pane: usize,
    /// First visible row, kept across frames (see `theme::render_search_list`).
    list_scroll: usize,
    view_height: u16,
}

impl InteractionsPane {
    pub fn new(pane_id: usize, trace_pane: usize, level: TextLevel) -> Self {
        Self {
            sessions: Vec::new(),
            folded: HashSet::new(),
            opening: HashSet::new(),
            flat: false,
            rows: Vec::new(),
            selected_idx: 0,
            level,
            keymap: Keymap::new(),
            pane_id,
            trace_pane,
            list_scroll: 0,
            view_height: 20,
        }
    }

    pub fn set_keymap(&mut self, km: Keymap<InteractionsAction>) {
        self.keymap = km;
    }

    pub fn keymap(&self) -> &Keymap<InteractionsAction> {
        &self.keymap
    }

    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    fn interaction(&self, s: usize, i: usize) -> Option<(&Arc<Session>, &Interaction)> {
        let session = self.sessions.get(s)?.session.as_ref()?;
        Some((session, session.trace.get(i)?))
    }

    fn key_of(&self, row: Row) -> Option<RowKey> {
        match row {
            Row::Session(s) => self.sessions.get(s).map(|e| RowKey::Session(e.id.clone())),
            Row::Interaction(s, i) => {
                let (session, interaction) = self.interaction(s, i)?;
                Some(RowKey::Interaction(
                    session.id.clone(),
                    InteractionKey::of(interaction),
                ))
            }
        }
    }

    fn rebuild_rows(&mut self) {
        let mut rows = Vec::new();
        for (s, entry) in self.sessions.iter().enumerate() {
            if !self.flat {
                rows.push(Row::Session(s));
            }
            let Some(session) = &entry.session else {
                continue;
            };
            if self.flat || !self.folded.contains(&entry.id) {
                // The latest first.
                rows.extend(
                    (0..session.trace.len())
                        .rev()
                        .map(|i| Row::Interaction(s, i)),
                );
            }
        }
        if self.flat {
            let started = |row: &Row| match row {
                Row::Interaction(s, i) => self
                    .interaction(*s, *i)
                    .and_then(|(_, interaction)| ms(interaction.started_at)),
                Row::Session(_) => None,
            };
            rows.sort_by_key(|row| std::cmp::Reverse(started(row)));
        }
        self.rows = rows;
    }

    /// The first interaction listed: the latest of the latest session.
    fn head(&self) -> Option<usize> {
        self.rows
            .iter()
            .position(|row| matches!(row, Row::Interaction(..)))
    }

    /// Rebuild the rows around what is selected: the selection stays on
    /// its row, or, while it is on the first interaction (or on nothing),
    /// on whatever is the first interaction now.
    fn relist(&mut self) {
        let at_head = self.rows.is_empty() || self.head() == Some(self.selected_idx);
        let keep = self
            .rows
            .get(self.selected_idx)
            .and_then(|row| self.key_of(*row));
        self.rebuild_rows();
        let last = self.rows.len().saturating_sub(1);
        let found = keep.and_then(|key| {
            self.rows
                .iter()
                .position(|row| self.key_of(*row).as_ref() == Some(&key))
        });
        self.selected_idx = match (at_head, self.head(), found) {
            (true, Some(head), _) => head,
            (_, _, Some(found)) => found,
            _ => self.selected_idx.min(last),
        };
    }

    /// Replace the sessions.
    pub fn set_sessions(&mut self, sessions: Vec<SessionEntry>) {
        self.opening
            .retain(|id| !sessions.iter().any(|e| &e.id == id && e.session.is_some()));
        self.sessions = sessions;
        self.relist();
    }

    /// The session the selection is on or in.
    pub fn selected_entry(&self) -> Option<&SessionEntry> {
        match self.rows.get(self.selected_idx)? {
            Row::Session(s) | Row::Interaction(s, _) => self.sessions.get(*s),
        }
    }

    /// The selected interaction and its session, for the Trace pane.
    pub fn selected_interaction(&self) -> Option<(Arc<Session>, InteractionKey)> {
        let Row::Interaction(s, i) = *self.rows.get(self.selected_idx)? else {
            return None;
        };
        let (session, interaction) = self.interaction(s, i)?;
        Some((session.clone(), InteractionKey::of(interaction)))
    }

    /// The selected row, for the Detail pane.
    pub fn chosen(&self, now: i64) -> Chosen {
        match self.rows.get(self.selected_idx) {
            Some(Row::Session(s)) => match self.sessions.get(*s) {
                Some(entry) => Chosen::Session(Box::new(session_summary(
                    entry,
                    self.opening.contains(&entry.id),
                    now,
                ))),
                None => Chosen::Nothing,
            },
            Some(Row::Interaction(s, i)) => match self.interaction(*s, *i) {
                Some((session, interaction)) => {
                    // Through the session, so "is it the latest" compares
                    // the same allocation.
                    let key = InteractionKey::of(interaction);
                    let interaction = find_interaction(session, &key).unwrap_or(interaction);
                    Chosen::Interaction(Box::new(interaction_summary(session, interaction, now)))
                }
                None => Chosen::Nothing,
            },
            None => Chosen::Nothing,
        }
    }

    /// Fold or unfold session `s`; one that is not read is asked for.
    fn toggle_session(&mut self, s: usize) -> Vec<PaneEvent> {
        let Some(entry) = self.sessions.get(s) else {
            return vec![];
        };
        let id = entry.id.clone();
        if entry.session.is_none() {
            self.opening.insert(id.clone());
            return vec![PaneEvent::LoadSession(id)];
        }
        if !self.folded.remove(&id) {
            self.folded.insert(id);
        }
        self.rebuild_rows();
        self.selected_idx = self
            .rows
            .iter()
            .position(|row| *row == Row::Session(s))
            .unwrap_or(0);
        vec![PaneEvent::SelectionChanged]
    }

    fn execute(&mut self, shared: &PaneShared, action: InteractionsAction) -> Vec<PaneEvent> {
        if let Some(events) = pane::try_dispatch_search_esc(&action, shared, self.pane_id, vec![]) {
            return events;
        }
        let row = self.rows.get(self.selected_idx).copied();
        match (action, row) {
            (InteractionsAction::Nav(nav), _) => {
                let len = self.rows.len();
                pane::execute_list_nav(nav, &mut self.selected_idx, len, Some(self.view_height))
            }
            (InteractionsAction::Open, Some(Row::Interaction(..))) => {
                vec![PaneEvent::SetFocus(self.trace_pane)]
            }
            (InteractionsAction::Open, Some(Row::Session(s)))
            | (InteractionsAction::ToggleFold, Some(Row::Session(s))) => self.toggle_session(s),
            // Folding from inside a session: only where sessions are rows.
            (InteractionsAction::ToggleFold, Some(Row::Interaction(s, _))) if !self.flat => {
                self.toggle_session(s)
            }
            (InteractionsAction::ToggleFlat, _) => {
                self.flat = !self.flat;
                self.relist();
                self.list_scroll = 0;
                vec![PaneEvent::SelectionChanged]
            }
            _ => vec![],
        }
    }

    /// What a row says the interaction was: what was asked (from
    /// `claude-text "summary"`), else the tools it used.
    fn text(&self, interaction: &Interaction) -> (String, Style) {
        match interaction
            .prompt
            .as_ref()
            .filter(|_| self.level.shows_summaries())
        {
            Some(prompt) => (prompt.clone(), Style::default()),
            None => (tools_used(interaction), Style::default().fg(Color::Cyan)),
        }
    }

    /// A session's name: its title, else the start of its id.
    fn name(entry: &SessionEntry) -> String {
        entry
            .session
            .as_ref()
            .and_then(|s| s.title.clone())
            .unwrap_or_else(|| entry.id.chars().take(8).collect())
    }

    fn search_text(&self, row: Row) -> String {
        match row {
            Row::Session(s) => self
                .sessions
                .get(s)
                .map(|entry| format!("{} {}", entry.place, Self::name(entry)))
                .unwrap_or_default(),
            Row::Interaction(s, i) => self
                .interaction(s, i)
                .map(|(_, interaction)| self.text(interaction).0)
                .unwrap_or_default(),
        }
    }

    fn text_width(width: usize) -> usize {
        width.saturating_sub(1 + WHEN_WIDTH + 2 + 1 + COUNTS_WIDTH)
    }

    fn session_row(
        &self,
        entry: &SessionEntry,
        now: i64,
        width: usize,
        selected: bool,
    ) -> Line<'static> {
        let dim = dim(selected);
        let when = Span::styled(
            format!(
                " {}  ",
                right(&short_ago(entry.modified_ms, now), WHEN_WIDTH)
            ),
            dim,
        );
        // Where it runs, unless that is the repository's root.
        let place_text = match entry.place.as_str() {
            "." => String::new(),
            place => format!("{place}  "),
        };
        let place_width = place_text.chars().count();
        let place = Span::styled(place_text, Style::default().fg(Color::Magenta));
        let text_width = Self::text_width(width);
        let Some(session) = &entry.session else {
            let note = if self.opening.contains(&entry.id) {
                "reading…"
            } else {
                "not read"
            };
            let rest = text_width.saturating_sub(2 + place_width);
            return Line::from(vec![
                when,
                Span::styled("▸ ", dim),
                place,
                Span::styled(fit(&Self::name(entry), rest), dim),
                Span::raw(" ".repeat(1 + 4 + 1 + 5 + 1 + 7)),
                Span::styled(format!(" {note:<8}"), dim),
            ]);
        };
        let fold = if self.folded.contains(&entry.id) {
            "▸ "
        } else {
            "▾ "
        };
        let (requests, calls) = session
            .trace
            .iter()
            .map(span_counts)
            .fold((0, 0), |(r, c), (dr, dc)| (r + dr, c + dc));
        let status = status(session, now);
        let (state, state_style) = (status.word, status.kind.style());
        let rest = text_width.saturating_sub(2 + place_width);
        Line::from(vec![
            when,
            Span::styled(fold, dim),
            place,
            Span::styled(
                fit(&Self::name(entry), rest),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!(" {}", right(&requests.to_string(), 4)), dim),
            Span::styled(format!(" {}", right(&calls.to_string(), 5)), dim),
            Span::raw(" ".repeat(1 + 7)),
            Span::styled(format!(" {state:<8}"), state_style),
        ])
    }

    fn interaction_row(
        &self,
        entry: &SessionEntry,
        session: &Session,
        interaction: &Interaction,
        now: i64,
        width: usize,
        selected: bool,
    ) -> Line<'static> {
        let dim = dim(selected);
        let (text, text_style) = self.text(interaction);
        let (requests, calls) = span_counts(interaction);
        let took = took_ms(session, interaction, now);
        let state = state_of(session, interaction, now);
        let state_style = match state {
            "done" => Style::default().fg(Color::Green),
            "running" => Style::default().fg(Color::Yellow),
            _ => dim,
        };
        let text_width = Self::text_width(width);
        let mut spans = vec![Span::styled(
            format!(
                " {}  ",
                right(&short_ago(ms(interaction.started_at), now), WHEN_WIDTH)
            ),
            dim,
        )];
        if self.flat {
            // What tells the sessions apart: where it runs, or, at the
            // repository's root, the session's name.
            let label = match entry.place.as_str() {
                "." => Self::name(entry),
                place => place.to_owned(),
            };
            let place = fit(&label, label.chars().count().min(14));
            let rest = text_width.saturating_sub(place.chars().count() + 2);
            spans.push(Span::styled(place, Style::default().fg(Color::Magenta)));
            spans.push(Span::styled(format!("  {}", fit(&text, rest)), text_style));
        } else {
            spans.push(Span::raw("    "));
            spans.push(Span::styled(
                fit(&text, text_width.saturating_sub(4)),
                text_style,
            ));
        }
        spans.extend([
            Span::styled(format!(" {}", right(&requests.to_string(), 4)), dim),
            Span::styled(format!(" {}", right(&calls.to_string(), 5)), dim),
            Span::raw(format!(
                " {}",
                right(&took.map(duration).unwrap_or_default(), 7)
            )),
            Span::styled(format!(" {state:<8}"), state_style),
        ]);
        Line::from(spans)
    }

    fn row_line(&self, row: Row, now: i64, width: usize, selected: bool) -> Line<'static> {
        match row {
            Row::Session(s) => match self.sessions.get(s) {
                Some(entry) => self.session_row(entry, now, width, selected),
                None => Line::default(),
            },
            Row::Interaction(s, i) => match (self.sessions.get(s), self.interaction(s, i)) {
                (Some(entry), Some((session, interaction))) => {
                    self.interaction_row(entry, session, interaction, now, width, selected)
                }
                _ => Line::default(),
            },
        }
    }
}

/// `5m ago`, or a dash where the time is not known.
fn short_ago(at: Option<i64>, now: i64) -> String {
    match at {
        Some(_) => ago(at, now),
        None => "—".to_owned(),
    }
}

impl Pane<PaneEvent> for InteractionsPane {
    crate::impl_handle_key!(keymap);

    fn render(&mut self, f: &mut Frame, _ctx: &AppContext, shared: &PaneShared, area: Rect) {
        self.view_height = area.height.saturating_sub(2);
        let empty = self.rows.is_empty().then_some("No sessions yet");
        let now = now_ms();
        let width = area.width.saturating_sub(2) as usize;
        let interactions = self
            .rows
            .iter()
            .filter(|row| matches!(row, Row::Interaction(..)))
            .count();
        let title = format!(
            "Interactions ({} · {})",
            super::count(self.sessions.len() as u64, "session"),
            if self.flat {
                format!("{interactions} together")
            } else {
                "by session".to_owned()
            }
        );
        // The selection is kept while unfocused: it says whose tree is shown.
        let selected = self.selected_idx;
        let scroll = theme::render_list_pane(
            f,
            area,
            shared,
            self.pane_id,
            &title,
            Some(selected),
            empty,
            self.list_scroll,
            |match_set, current_match_idx| {
                self.rows
                    .iter()
                    .enumerate()
                    .map(|(idx, row)| {
                        let mut li =
                            ListItem::new(self.row_line(*row, now, width, idx == selected));
                        let hl = theme::search_highlight_for(match_set, current_match_idx, idx);
                        if hl.is_active() {
                            li = li.style(hl.apply(Style::default()));
                        }
                        li
                    })
                    .collect()
            },
        );
        self.list_scroll = scroll;
        // The columns' names, on the pane's top border above them.
        if !self.rows.is_empty() {
            super::border_header(
                f,
                area,
                &title,
                1 + WHEN_WIDTH + 2 + Self::text_width(width),
                &format!(
                    " {} {} {} ",
                    right("llm", 4),
                    right("tools", 5),
                    right("took", 7)
                ),
            );
        }
    }

    fn collect_search_matches(&self, _shared: &PaneShared, query: &str) -> Vec<SearchMatch> {
        pane::collect_list_search_matches(&self.rows, query, |row| self.search_text(*row))
    }

    crate::impl_list_pane_selection!();
}
