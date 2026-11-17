//! Trace pane: the selected interaction as a tree — its LLM requests,
//! the tool calls each made, the subagents the calls spawned and what
//! they did — each row with a bar on a time axis as long as the
//! interaction.

use super::{
    agent_summary, bar, called, dim, duration, end_ms, find_interaction, fit, ms, now_ms, outcome,
    request_marks, request_text, right, runs, span_counts, Chosen, InteractionKey, RequestDetail,
};
use crate::claude::domain::files::display_path;
use crate::claude::domain::types::{TextLevel, Touch};
use crate::core::app::AppContext;
use crate::core::keymap::{ActionHelp, Keymap, NavAction, SearchAction};
use crate::core::pane::{self, Pane, PaneEvent, PaneShared};
use crate::core::search::SearchMatch;
use crate::core::theme;
use iktinos::session::{Session, Subagent, ToolCall};
use iktinos::trace::{Interaction, Item, Node, SpanKind};
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::ListItem,
    Frame,
};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum TraceAction {
    Nav(NavAction),
    /// A subagent's own spans: shown or hidden.
    ToggleFold,
    /// Into the Detail pane.
    Open,
    /// Back to the interactions.
    Back,
    /// Read the whole output / response of the row.
    ReadWhole,
    Search(SearchAction),
    Esc,
}

crate::impl_pane_action_from_str!(
    TraceAction,
    nav: Nav,
    search: Search,
    esc: Esc,
    ToggleFold,
    Open,
    Back,
    ReadWhole
);

impl ActionHelp for TraceAction {
    fn label(&self) -> Option<&'static str> {
        match self {
            TraceAction::Nav(nav) => nav.label(),
            TraceAction::ToggleFold => Some("Show / hide a subagent's spans"),
            TraceAction::Open => Some("Into the detail"),
            TraceAction::Back => Some("Back to the interactions"),
            TraceAction::ReadWhole => Some("Read the whole output / response"),
            TraceAction::Search(sa) => sa.label(),
            TraceAction::Esc => Some("Clear search / back"),
        }
    }
}

/// What a row is, independent of its position: survives an update.
#[derive(Debug, Clone, PartialEq, Eq)]
enum RowKey {
    Tool(String),
    Request(Option<String>, Option<i64>),
    Agent(String),
}

fn key_of(node: &Node) -> RowKey {
    match node.item {
        Item::Subagent(agent) => RowKey::Agent(agent.agent_id.clone()),
        Item::Span(span) => match &span.kind {
            SpanKind::Tool(call) => RowKey::Tool(call.id.clone()),
            SpanKind::LlmRequest(request) => {
                RowKey::Request(request.message_id.clone(), ms(span.started_at))
            }
            _ => RowKey::Request(None, ms(span.started_at)),
        },
    }
}

const TOOK_WIDTH: usize = 7;
const MARK_WIDTH: usize = 10;

pub struct TracePane {
    session: Option<Arc<Session>>,
    /// The interaction whose tree this is.
    interaction: Option<InteractionKey>,
    /// Subagents whose spans are listed, by agent id.
    unfolded: HashSet<String>,
    pub selected_idx: usize,
    level: TextLevel,
    /// The repository's roots: a file tool's path is shown from them.
    roots: Vec<PathBuf>,
    keymap: Keymap<TraceAction>,
    pane_id: usize,
    interactions_pane: usize,
    detail_pane: usize,
    /// First visible row, kept across frames (see `theme::render_search_list`).
    list_scroll: usize,
    view_height: u16,
}

impl TracePane {
    pub fn new(
        pane_id: usize,
        interactions_pane: usize,
        detail_pane: usize,
        level: TextLevel,
    ) -> Self {
        Self {
            session: None,
            interaction: None,
            unfolded: HashSet::new(),
            selected_idx: 0,
            level,
            roots: Vec::new(),
            keymap: Keymap::new(),
            pane_id,
            interactions_pane,
            detail_pane,
            list_scroll: 0,
            view_height: 20,
        }
    }

    pub fn set_keymap(&mut self, km: Keymap<TraceAction>) {
        self.keymap = km;
    }

    pub fn keymap(&self) -> &Keymap<TraceAction> {
        &self.keymap
    }

    pub fn set_roots(&mut self, roots: Vec<PathBuf>) {
        self.roots = roots;
    }

    fn current(&self) -> Option<(&Session, &Interaction)> {
        let session = self.session.as_deref()?;
        let interaction = find_interaction(session, self.interaction.as_ref()?)?;
        Some((session, interaction))
    }

    /// The tree's rows: the interaction walked depth first, without what
    /// is under a subagent that is not unfolded.
    fn rows(&self) -> Vec<Node<'_>> {
        let Some((session, interaction)) = self.current() else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        let mut hidden_below: Option<usize> = None;
        for node in session.walk(interaction) {
            if hidden_below.is_some_and(|depth| node.depth > depth) {
                continue;
            }
            hidden_below = match node.item {
                Item::Subagent(agent) if !self.unfolded.contains(&agent.agent_id) => {
                    Some(node.depth)
                }
                _ => None,
            };
            rows.push(node);
        }
        rows
    }

    /// Show the tree of an interaction of a session (or of none). The
    /// same one as before keeps the selection on its row; another starts
    /// at the top.
    pub fn show(&mut self, target: Option<(Arc<Session>, InteractionKey)>) {
        let (session, interaction) = target.unzip();
        let same_session = match (&self.session, &session) {
            (Some(old), Some(new)) => old.id == new.id,
            (None, None) => true,
            _ => false,
        };
        if same_session && self.interaction == interaction {
            let unchanged = match (&self.session, &session) {
                (Some(old), Some(new)) => Arc::ptr_eq(old, new),
                _ => true,
            };
            if !unchanged {
                let keep = self.rows().get(self.selected_idx).map(key_of);
                self.session = session;
                self.reselect(keep);
            }
            return;
        }
        self.session = session;
        self.interaction = interaction;
        self.selected_idx = 0;
        self.list_scroll = 0;
    }

    fn reselect(&mut self, keep: Option<RowKey>) {
        let rows = self.rows();
        let found = keep.and_then(|key| rows.iter().position(|node| key_of(node) == key));
        let last = rows.len().saturating_sub(1);
        self.selected_idx = found.unwrap_or(self.selected_idx).min(last);
    }

    /// Show or hide the spans of the subagent the selection is on.
    fn toggle_fold(&mut self) {
        let rows = self.rows();
        let Some(node) = rows.get(self.selected_idx) else {
            return;
        };
        let Item::Subagent(agent) = node.item else {
            return;
        };
        let id = agent.agent_id.clone();
        let keep = Some(key_of(node));
        if !self.unfolded.remove(&id) {
            self.unfolded.insert(id);
        }
        self.reselect(keep);
    }

    /// The selected row, for the Detail pane.
    pub fn chosen(&self) -> Chosen {
        let rows = self.rows();
        let (Some(node), Some((session, interaction))) =
            (rows.get(self.selected_idx), self.current())
        else {
            return Chosen::Nothing;
        };
        match node.item {
            Item::Subagent(agent) => Chosen::Agent(Box::new(agent_summary(agent))),
            Item::Span(span) => match &span.kind {
                SpanKind::Tool(call) => Chosen::Tool(Box::new(self.shown_call(call))),
                SpanKind::LlmRequest(request) => Chosen::Request(Box::new(RequestDetail {
                    request: request.clone(),
                    started_ms: ms(span.started_at),
                    took_ms: span.duration_ms(),
                    called: called(session, interaction, request),
                })),
                _ => Chosen::Nothing,
            },
        }
    }

    /// `call` with a file tool's path the way the Files pane shows it.
    fn shown_call(&self, call: &ToolCall) -> ToolCall {
        let mut call = call.clone();
        if Touch::of_tool(&call.name).is_some() {
            call.summary = call.summary.map(|path| display_path(&path, &self.roots));
        }
        call
    }

    fn execute(&mut self, shared: &PaneShared, action: TraceAction) -> Vec<PaneEvent> {
        let back = || vec![PaneEvent::SetFocus(self.interactions_pane)];
        if let Some(events) = pane::try_dispatch_search_esc(&action, shared, self.pane_id, back()) {
            return events;
        }
        match action {
            TraceAction::Nav(nav) => {
                let len = self.rows().len();
                pane::execute_list_nav(nav, &mut self.selected_idx, len, Some(self.view_height))
            }
            TraceAction::ToggleFold => {
                self.toggle_fold();
                vec![PaneEvent::SelectionChanged]
            }
            TraceAction::Open => vec![PaneEvent::SetFocus(self.detail_pane)],
            TraceAction::Back => back(),
            TraceAction::ReadWhole => vec![PaneEvent::ReadWhole],
            _ => vec![],
        }
    }

    /// The name of a row and what is said beside it, as far as
    /// `claude-text` lets it be drawn.
    fn label(
        &self,
        node: &Node,
        session: &Session,
        interaction: &Interaction,
    ) -> (String, Style, String) {
        match node.item {
            Item::Subagent(agent) => (
                format!("↳ {}", agent.agent_type.as_deref().unwrap_or("agent")),
                Style::default().fg(Color::Magenta),
                self.shown(agent_task(agent)),
            ),
            Item::Span(span) => match &span.kind {
                SpanKind::Tool(call) => (
                    call.name.clone(),
                    Style::default().fg(Color::Cyan),
                    self.shown(self.shown_call(call).summary.as_deref()),
                ),
                SpanKind::LlmRequest(request) => (
                    request_text(request, &called(session, interaction, request), self.level),
                    Style::default().fg(Color::LightBlue),
                    request_marks(request).join(", "),
                ),
                _ => (String::new(), Style::default(), String::new()),
            },
        }
    }

    fn shown(&self, text: Option<&str>) -> String {
        text.filter(|_| self.level.shows_summaries())
            .unwrap_or_default()
            .to_owned()
    }

    fn search_text(&self, node: &Node, session: &Session, interaction: &Interaction) -> String {
        let (name, _, beside) = self.label(node, session, interaction);
        format!("{name} {beside}")
    }

    #[allow(clippy::too_many_arguments)]
    fn row(
        &self,
        node: &Node,
        session: &Session,
        interaction: &Interaction,
        window: Option<(i64, i64)>,
        running: bool,
        now: i64,
        (width, axis): (usize, usize),
        selected: bool,
    ) -> Line<'static> {
        let dim = dim(selected);
        let start = ms(node.started_at());
        // What has not ended reaches to now while the interaction runs.
        let end = ms(node.ended_at()).or(running.then_some(now));
        let took = match (start, end) {
            (Some(start), Some(end)) => Some((end - start).max(0) as u64),
            _ => None,
        };
        let (mark, mark_style, colour) = match node.item {
            Item::Subagent(agent) => match agent.finished {
                Some(true) => (
                    "done".to_owned(),
                    Style::default().fg(Color::Green),
                    Color::Magenta,
                ),
                _ => (
                    "running".to_owned(),
                    Style::default().fg(Color::Yellow),
                    Color::Magenta,
                ),
            },
            Item::Span(span) => match &span.kind {
                SpanKind::Tool(call) => {
                    let (mark, style) = outcome(call, selected);
                    let colour = if call.success == Some(false) {
                        Color::Red
                    } else {
                        Color::Green
                    };
                    (mark.to_owned(), style, colour)
                }
                SpanKind::LlmRequest(request) => {
                    let stop = request.stop_reason.clone().unwrap_or_default();
                    // The end of the answer, more to do, or cut short.
                    let style = match stop.as_str() {
                        "end_turn" => Style::default().fg(Color::Green),
                        "tool_use" | "" => dim,
                        _ => Style::default().fg(Color::Red),
                    };
                    (stop, style, Color::LightBlue)
                }
                _ => (String::new(), dim, Color::LightBlue),
            },
        };
        let (name, style, beside) = self.label(node, session, interaction);
        let fold = match node.item {
            Item::Subagent(agent) if self.unfolded.contains(&agent.agent_id) => "▾ ",
            Item::Subagent(_) => "▸ ",
            Item::Span(_) => "  ",
        };
        let is_request = matches!(
            node.item,
            Item::Span(span) if matches!(span.kind, SpanKind::LlmRequest(_))
        );
        let indent = format!(" {}{fold}", "  ".repeat(node.depth));
        let fixed = TOOK_WIDTH + 1 + MARK_WIDTH + if axis > 0 { axis + 1 } else { 0 };
        let text_width = width.saturating_sub(indent.chars().count() + 1 + fixed);
        let mut spans = vec![Span::raw(indent)];
        if is_request {
            // What marks a response comes first, where it is not cut.
            let marks = if beside.is_empty() {
                String::new()
            } else {
                format!("{beside}  ")
            };
            let rest = text_width.saturating_sub(marks.chars().count());
            spans.push(Span::styled(marks, Style::default().fg(Color::Yellow)));
            spans.push(Span::styled(fit(&name, rest), style));
        } else {
            let name_width = name.chars().count().min(text_width);
            let rest = text_width.saturating_sub(name_width + 2);
            spans.push(Span::styled(fit(&name, name_width), style));
            spans.push(Span::raw(format!("  {}", fit(&beside, rest))));
        }
        spans.push(Span::raw(format!(
            " {}",
            right(&took.map(duration).unwrap_or_default(), TOOK_WIDTH)
        )));
        spans.push(Span::styled(
            format!(" {}", fit(&mark, MARK_WIDTH - 1)),
            mark_style,
        ));
        if axis > 0 {
            let drawn = window.map_or(String::new(), |window| bar(start, end, window, axis));
            spans.push(Span::styled(
                format!(" {}", fit(&drawn, axis)),
                Style::default().fg(colour),
            ));
        }
        Line::from(spans)
    }
}

fn agent_task(agent: &Subagent) -> Option<&str> {
    agent.description.as_deref()
}

impl Pane<PaneEvent> for TracePane {
    crate::impl_handle_key!(keymap);

    fn render(&mut self, f: &mut Frame, _ctx: &AppContext, shared: &PaneShared, area: Rect) {
        self.view_height = area.height.saturating_sub(2);
        let width = area.width.saturating_sub(2) as usize;
        // The time axis takes a third of the pane, a quarter of a narrow
        // one, and is left out where there is no room for it.
        let axis = match width {
            90.. => width / 3,
            56.. => width / 4,
            _ => 0,
        };
        let now = now_ms();
        let focused = shared.focused_pane == self.pane_id;
        let current = self.current();
        let title = match current {
            Some((_, interaction)) => {
                let (requests, calls) = span_counts(interaction);
                format!(
                    "Trace ({}, {})",
                    super::count(requests as u64, "LLM request"),
                    super::count(calls as u64, "tool call")
                )
            }
            None => "Trace".to_owned(),
        };
        let rows = self.rows();
        let empty = match current {
            None => Some("No interaction selected"),
            Some(_) if rows.is_empty() => {
                Some("Nothing was read of this interaction but its prompt")
            }
            Some(_) => None,
        };
        // The cursor shows once the tree has been entered.
        let selected = focused.then_some(self.selected_idx.min(rows.len().saturating_sub(1)));
        let scroll = theme::render_list_pane(
            f,
            area,
            shared,
            self.pane_id,
            &title,
            selected,
            empty,
            self.list_scroll,
            |match_set, current_match_idx| {
                let Some((session, interaction)) = current else {
                    return Vec::new();
                };
                let running = runs(session, interaction, now);
                let window = ms(interaction.started_at).zip(end_ms(session, interaction, now));
                rows.iter()
                    .enumerate()
                    .map(|(idx, node)| {
                        let mut li = ListItem::new(self.row(
                            node,
                            session,
                            interaction,
                            window,
                            running,
                            now,
                            (width, axis),
                            selected == Some(idx),
                        ));
                        let hl = theme::search_highlight_for(match_set, current_match_idx, idx);
                        if hl.is_active() {
                            li = li.style(hl.apply(Style::default()));
                        }
                        li
                    })
                    .collect()
            },
        );
        let any = !rows.is_empty();
        drop(rows);
        self.list_scroll = scroll;
        // The columns' names, on the pane's top border above them.
        if any {
            let fixed = TOOK_WIDTH + 1 + MARK_WIDTH + if axis > 0 { axis + 1 } else { 0 };
            let at = width.saturating_sub(fixed + 1);
            super::border_header(
                f,
                area,
                &title,
                at,
                &format!(" {} ", right("took", TOOK_WIDTH)),
            );
            if axis > 0 {
                super::border_header(f, area, &title, width.saturating_sub(axis + 1), " time ");
            }
        }
    }

    fn collect_search_matches(&self, _shared: &PaneShared, query: &str) -> Vec<SearchMatch> {
        let Some((session, interaction)) = self.current() else {
            return Vec::new();
        };
        pane::collect_list_search_matches(&self.rows(), query, |node| {
            self.search_text(node, session, interaction)
        })
    }

    crate::impl_list_pane_selection!();
}
