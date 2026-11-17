//! Detail pane: what is chosen in the other two — the interaction under
//! the cursor (how long it took and where the time went), or the row of
//! its tree: a request's tokens and cost, a tool call's command and, at
//! `claude-text "full"`, what it returned.
//!
//! A session keeps only the start of an output and the first line of a
//! response. `o` reads all of it from the transcript; it is held here
//! while it is shown and dropped when something else is chosen.

use super::{
    ago, count, duration, field, now_ms, outcome, tokens, usd, wrap, AgentSummary, Chosen,
    InteractionSummary, RequestDetail, SessionSummary,
};
use crate::claude::domain::types::{TextLevel, Whole, WholeKey, WholeText};
use crate::core::app::AppContext;
use crate::core::keymap::{half_page_step, ActionHelp, Keymap, NavAction};
use crate::core::pane::{Pane, PaneEvent, PaneShared};
use crate::core::theme;
use iktinos_core::session::ToolCall;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

#[derive(Debug, Clone)]
pub enum DetailAction {
    Nav(NavAction),
    /// Read the whole output / response, or go back to its start.
    ReadWhole,
    Back,
    Esc,
}

crate::impl_pane_action_from_str!(DetailAction, nav: Nav, ReadWhole, Back, Esc);

impl ActionHelp for DetailAction {
    fn label(&self) -> Option<&'static str> {
        match self {
            DetailAction::Nav(NavAction::MoveDown) => Some("Scroll down"),
            DetailAction::Nav(NavAction::MoveUp) => Some("Scroll up"),
            DetailAction::Nav(nav) => nav.label(),
            DetailAction::ReadWhole => Some("Read the whole output / response"),
            DetailAction::Back => Some("Back to the list"),
            DetailAction::Esc => Some("Back to the list"),
        }
    }
}

/// The most of a whole text that is laid out, in characters: more than
/// anyone scrolls through, and cheap to wrap again on every draw.
const WHOLE_SHOWN: usize = 200_000;

enum WholeState {
    Loading,
    Loaded(Result<Option<WholeText>, String>),
}

/// What `o` leads to.
#[derive(Debug, PartialEq, Eq)]
pub enum WholeRequest {
    /// Ask the reader for this.
    Read(WholeKey),
    /// The whole text was shown and is dropped: back to the excerpt.
    Dropped,
    /// Outputs and responses are not read at this `claude-text` level.
    NotAtThisLevel,
    /// What is chosen has no text to read.
    Nothing,
}

pub struct DetailPane {
    chosen: Chosen,
    whole: Option<(WholeKey, WholeState)>,
    scroll: usize,
    /// Lines and rows of the last draw: what scrolling is bounded by.
    lines: usize,
    view_height: u16,
    level: TextLevel,
    keymap: Keymap<DetailAction>,
    pane_id: usize,
}

impl DetailPane {
    pub fn new(pane_id: usize, level: TextLevel) -> Self {
        Self {
            chosen: Chosen::Nothing,
            whole: None,
            scroll: 0,
            lines: 0,
            view_height: 20,
            level,
            keymap: Keymap::new(),
            pane_id,
        }
    }

    pub fn set_keymap(&mut self, km: Keymap<DetailAction>) {
        self.keymap = km;
    }

    pub fn keymap(&self) -> &Keymap<DetailAction> {
        &self.keymap
    }

    /// Show `chosen`. Something else than before starts at the top, and a
    /// whole text that is not its own is dropped.
    pub fn show(&mut self, chosen: Chosen) {
        if ident(&chosen) != ident(&self.chosen) {
            self.scroll = 0;
        }
        let key = chosen.whole_key();
        if self
            .whole
            .as_ref()
            .is_some_and(|(held, _)| Some(held) != key.as_ref())
        {
            self.whole = None;
        }
        self.chosen = chosen;
    }

    /// `o`: read all of what is chosen, or, where it is shown, drop it.
    pub fn toggle_whole(&mut self) -> WholeRequest {
        let Some(key) = self.chosen.whole_key() else {
            return WholeRequest::Nothing;
        };
        if !self.level.shows_output() {
            return WholeRequest::NotAtThisLevel;
        }
        if self.whole.take().is_some() {
            self.scroll = 0;
            return WholeRequest::Dropped;
        }
        self.whole = Some((key.clone(), WholeState::Loading));
        WholeRequest::Read(key)
    }

    /// The reader's answer. One that is not waited for any more is dropped.
    pub fn loaded(&mut self, whole: Whole) {
        if let Some((key, state)) = &mut self.whole {
            if *key == whole.key {
                *state = WholeState::Loaded(whole.content);
            }
        }
    }

    fn execute(&mut self, shared: &PaneShared, action: DetailAction) -> Vec<PaneEvent> {
        let page = half_page_step(self.view_height) as usize;
        let bottom = self.lines.saturating_sub(self.view_height as usize);
        match action {
            DetailAction::Nav(nav) => {
                self.scroll = match nav {
                    NavAction::MoveDown => self.scroll + 1,
                    NavAction::MoveUp => self.scroll.saturating_sub(1),
                    NavAction::HalfPageDown => self.scroll + page,
                    NavAction::HalfPageUp => self.scroll.saturating_sub(page),
                    NavAction::JumpTop => 0,
                    NavAction::JumpBottom => bottom,
                }
                .min(bottom);
                vec![]
            }
            DetailAction::ReadWhole => vec![PaneEvent::ReadWhole],
            DetailAction::Back | DetailAction::Esc => {
                vec![PaneEvent::SetFocus(shared.previous_pane)]
            }
        }
    }

    fn lines(&self, now: i64, width: usize) -> Vec<Line<'static>> {
        match &self.chosen {
            Chosen::Nothing => vec![Line::styled(
                "Nothing selected",
                Style::default().fg(Color::DarkGray),
            )],
            Chosen::Session(summary) => session_lines(summary, now),
            Chosen::Interaction(summary) => interaction_lines(summary, now, width, self.level),
            Chosen::Tool(call) => {
                let mut lines = call_lines(call, now, width, self.level);
                lines.extend(self.output_lines(call, width));
                lines
            }
            Chosen::Request(detail) => {
                let mut lines = request_lines(detail, now);
                lines.extend(self.said_lines(detail, width));
                lines
            }
            Chosen::Agent(agent) => agent_lines(agent, now, width, self.level),
        }
    }

    /// What a tool call returned: the start the session keeps, or all of
    /// it once it was read.
    fn output_lines(&self, call: &ToolCall, width: usize) -> Vec<Line<'static>> {
        let mut lines = vec![Line::raw("")];
        if !self.level.shows_output() {
            lines.push(note("output    not read: `claude-text \"full\"` shows it"));
            return lines;
        }
        match self.whole.as_ref().map(|(_, state)| state) {
            Some(WholeState::Loading) => lines.push(note("output    reading…")),
            Some(WholeState::Loaded(Err(why))) => lines.push(Line::styled(
                format!("output    could not be read: {why}"),
                Style::default().fg(Color::Red),
            )),
            Some(WholeState::Loaded(Ok(None))) => lines.push(note(
                "output    the session's files hold no result of this call",
            )),
            Some(WholeState::Loaded(Ok(Some(whole)))) => {
                // The file it was moved to, by its place in the session's
                // directory.
                let from = whole.file.as_ref().map_or(String::new(), |path| {
                    let tail: Vec<_> = path.iter().rev().take(2).collect();
                    let tail: std::path::PathBuf = tail.into_iter().rev().collect();
                    format!(", from {}", tail.display())
                });
                lines.push(heading(format!(
                    "output    all of it: {}, {}{from}  (o: the start)",
                    count(whole.text.lines().count() as u64, "line"),
                    count(whole.text.chars().count() as u64, "character"),
                )));
                lines.extend(whole_text(&whole.text, width));
                if whole.truncated {
                    lines.push(note("… cut where reading stopped"));
                }
                lines.extend(tail_lines(
                    whole.stderr.as_deref(),
                    whole.interrupted,
                    whole.other_blocks,
                    width,
                ));
            }
            None => match &call.output {
                None if call.success.is_none() => lines.push(note("output    no result yet")),
                None => lines.push(note("output    none")),
                Some(output) => {
                    let size = format!(
                        "{}, {}",
                        count(output.lines as u64, "line"),
                        count(output.chars as u64, "character")
                    );
                    lines.push(heading(if output.truncated {
                        format!("output    {size}; the start is shown  (o: all of it)")
                    } else {
                        format!("output    {size}")
                    }));
                    lines.extend(wrap(&output.text, width));
                    if output.truncated {
                        lines.push(note("…"));
                    }
                    lines.extend(tail_lines(
                        output.stderr.as_deref(),
                        output.interrupted,
                        output.other_blocks,
                        width,
                    ));
                }
            },
        }
        lines
    }

    /// What a response did: the tools it called and, at `full`, what it
    /// said — its first line, or all of it once it was read.
    fn said_lines(&self, detail: &RequestDetail, width: usize) -> Vec<Line<'static>> {
        let mut lines = vec![Line::raw("")];
        if !detail.called.is_empty() {
            lines.push(field("called", detail.called.join(", ")));
        }
        if !self.level.shows_output() {
            lines.push(note("said      not read: `claude-text \"full\"` shows it"));
            return lines;
        }
        match self.whole.as_ref().map(|(_, state)| state) {
            Some(WholeState::Loading) => lines.push(note("said      reading…")),
            Some(WholeState::Loaded(Err(why))) => lines.push(Line::styled(
                format!("said      could not be read: {why}"),
                Style::default().fg(Color::Red),
            )),
            Some(WholeState::Loaded(Ok(None))) => lines.push(note(
                "said      nothing: the session's files hold no text of this response",
            )),
            Some(WholeState::Loaded(Ok(Some(whole)))) => {
                lines.push(heading(format!(
                    "said      all of it: {}, {}  (o: the first line)",
                    count(whole.text.lines().count() as u64, "line"),
                    count(whole.text.chars().count() as u64, "character"),
                )));
                lines.extend(whole_text(&whole.text, width));
                if whole.truncated {
                    lines.push(note("… cut where reading stopped"));
                }
            }
            None => match &detail.request.text {
                Some(text) => {
                    lines.push(heading(
                        "said      the first line  (o: all of it)".to_owned(),
                    ));
                    lines.extend(wrap(text, width));
                }
                None => lines.push(note("said      nothing")),
            },
        }
        lines
    }
}

/// What tells one chosen thing from another: a change scrolls to the top.
fn ident(chosen: &Chosen) -> String {
    match chosen {
        Chosen::Nothing => String::new(),
        Chosen::Session(s) => format!("s:{}", s.id),
        Chosen::Interaction(i) => format!("i:{:?}:{:?}", i.started_ms, i.prompt),
        Chosen::Tool(call) => format!("t:{}", call.id),
        Chosen::Request(d) => format!("r:{:?}:{:?}", d.request.message_id, d.started_ms),
        Chosen::Agent(a) => format!("a:{:?}:{:?}", a.started_ms, a.agent_type),
    }
}

fn heading(text: String) -> Line<'static> {
    Line::styled(text, Style::default().fg(Color::Yellow))
}

fn note(text: &str) -> Line<'static> {
    Line::styled(text.to_owned(), Style::default().fg(Color::DarkGray))
}

/// `lines` with those too long for `width` broken, keeping their style.
/// A field's value continues under itself, not under its name.
fn fitted(lines: Vec<Line<'static>>, width: usize) -> Vec<Line<'static>> {
    lines
        .into_iter()
        .flat_map(|line| {
            if line.width() <= width {
                return vec![line];
            }
            match line.spans.as_slice() {
                [only] => {
                    let style = line.style.patch(only.style);
                    wrap(&only.content, width)
                        .into_iter()
                        .map(|part| part.style(style))
                        .collect()
                }
                [name, value] if name.width() < width => {
                    let indent = name.width();
                    wrap(&value.content, width - indent)
                        .into_iter()
                        .enumerate()
                        .map(|(i, part)| {
                            let lead = match i {
                                0 => name.clone(),
                                _ => Span::raw(" ".repeat(indent)),
                            };
                            let text: String =
                                part.spans.iter().map(|s| s.content.as_ref()).collect();
                            Line::from(vec![lead, Span::styled(text, value.style)])
                        })
                        .collect()
                }
                _ => vec![line],
            }
        })
        .collect()
}

fn title(text: &str) -> Span<'static> {
    Span::styled(
        text.to_owned(),
        Style::default().add_modifier(Modifier::BOLD),
    )
}

/// A whole text wrapped, as much of it as is laid out.
fn whole_text(text: &str, width: usize) -> Vec<Line<'static>> {
    let (shown, cut) = match text.char_indices().nth(WHOLE_SHOWN) {
        Some((end, _)) => (&text[..end], true),
        None => (text, false),
    };
    let mut lines = wrap(shown, width);
    if cut {
        lines.push(note(&format!(
            "… the first {WHOLE_SHOWN} characters are shown"
        )));
    }
    lines
}

/// What follows an output's text: its stderr and what else is known.
fn tail_lines(
    stderr: Option<&str>,
    interrupted: bool,
    other_blocks: usize,
    width: usize,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if let Some(text) = stderr {
        lines.push(Line::raw(""));
        lines.push(Line::styled("stderr", Style::default().fg(Color::Red)));
        lines.extend(wrap(text, width));
    }
    if interrupted {
        lines.push(Line::styled(
            "interrupted",
            Style::default().fg(Color::Yellow),
        ));
    }
    if other_blocks > 0 {
        lines.push(note(&format!(
            "and {} that is not text",
            count(other_blocks as u64, "block")
        )));
    }
    lines
}

/// A session: where it runs and, once it is read, how it is doing.
fn session_lines(summary: &SessionSummary, now: i64) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(title("Session"))];
    if let Some(name) = summary.read.as_ref().and_then(|facts| facts.title.as_ref()) {
        lines.push(Line::raw(""));
        lines.push(Line::raw(name.clone()));
    }
    lines.push(Line::raw(""));
    lines.push(field("id", summary.id.clone()));
    lines.push(field("runs in", summary.place.clone()));
    if summary.modified_ms.is_some() {
        lines.push(field("updated", ago(summary.modified_ms, now)));
    }
    let Some(facts) = &summary.read else {
        lines.push(Line::raw(""));
        lines.push(note(if summary.opening {
            "reading its transcript…"
        } else {
            "not read: Enter reads its transcript"
        }));
        return lines;
    };
    if let Some(branch) = &facts.branch {
        lines.push(field("branch", branch.clone()));
    }
    if let Some(model) = &facts.model {
        lines.push(field("model", model.clone()));
    }
    lines.push(field("state", facts.state.clone()));
    lines.push(Line::raw(""));
    if let Some(context) = &facts.context {
        lines.push(field("context", context.clone()));
    }
    lines.push(field("cost", facts.cost.clone()));
    for (i, limit) in facts.rate_limits.iter().enumerate() {
        lines.push(field(if i == 0 { "limits" } else { "" }, limit.clone()));
    }
    lines.push(Line::raw(""));
    lines.push(field(
        "kept",
        count(facts.interactions as u64, "interaction"),
    ));
    lines.push(field("tools", count(facts.tool_calls, "call")));
    lines.push(field("subagents", facts.subagents.to_string()));
    lines.push(field("files", facts.files.to_string()));
    lines
}

fn interaction_lines(
    summary: &InteractionSummary,
    now: i64,
    width: usize,
    level: TextLevel,
) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(title("Interaction"))];
    if let Some(prompt) = summary.prompt.as_ref().filter(|_| level.shows_summaries()) {
        lines.push(Line::raw(""));
        lines.extend(wrap(prompt, width));
    }
    lines.push(Line::raw(""));
    lines.push(field(
        "state",
        match summary.state {
            "no end" => "no end recorded: cut off, or not read to its end",
            state => state,
        },
    ));
    if summary.started_ms.is_some() {
        lines.push(field("started", ago(summary.started_ms, now)));
    }
    if let Some(took) = summary.took_ms {
        let by = if summary.measured {
            ""
        } else {
            "  so far, by the records read"
        };
        lines.push(field("took", format!("{}{by}", duration(took))));
    }
    lines.push(Line::raw(""));
    lines.push(field(
        "llm",
        format!(
            "{}, {} in all (approximate)",
            count(summary.requests, "request"),
            duration(summary.request_ms)
        ),
    ));
    lines.push(field(
        "tools",
        format!(
            "{}, {} in all",
            count(summary.calls, "call"),
            duration(summary.call_ms)
        ),
    ));
    if let Some(took) = summary.took_ms {
        let rest = took
            .saturating_sub(summary.request_ms)
            .saturating_sub(summary.call_ms);
        lines.push(field("the rest", duration(rest)));
    }
    lines.push(Line::raw(""));
    lines.push(field("subagents", summary.subagents.to_string()));
    lines.push(field("tokens", tokens(summary.tokens)));
    lines.push(field(
        "cost",
        format!(
            "~{}{}",
            usd(summary.usd),
            if summary.unpriced {
                "  without the requests of an unknown model"
            } else {
                ""
            }
        ),
    ));
    if summary.dropped > 0 {
        lines.push(note(&format!(
            "{} more spans were not kept",
            summary.dropped
        )));
    }
    lines
}

/// A tool call: how it ended, then what it ran, every line of it.
fn call_lines(call: &ToolCall, now: i64, width: usize, level: TextLevel) -> Vec<Line<'static>> {
    let (mark, style) = outcome(call, false);
    let mut head = vec![
        title(&call.name),
        Span::raw("  "),
        Span::styled(mark, style),
    ];
    if let Some(took) = call.duration_ms {
        head.push(Span::raw(format!("  took {}", duration(took))));
    }
    if let Some(at) = call.started_at {
        head.push(Span::styled(
            format!("  {}", ago(Some(at.as_millis()), now)),
            Style::default().fg(Color::DarkGray),
        ));
    }
    let mut lines = vec![Line::from(head)];
    if level.shows_summaries() {
        // The whole command where the one line is not all of it; a file
        // tool's path as the page shows it.
        let text = match (&call.input, &call.summary) {
            (Some(input), _) if call.name == "Bash" => Some(input),
            (_, Some(summary)) => Some(summary),
            (input, None) => input.as_ref(),
        };
        if let Some(text) = text {
            lines.push(Line::raw(""));
            lines.extend(wrap(text, width));
        }
    }
    lines
}

fn request_lines(detail: &RequestDetail, now: i64) -> Vec<Line<'static>> {
    let request = &detail.request;
    let warn = Style::default().fg(Color::Yellow);
    let mut lines = vec![Line::from(title("LLM request")), Line::raw("")];
    if let Some(model) = &request.model {
        lines.push(field("model", model.clone()));
    }
    if let Some(source) = &request.query_source {
        lines.push(field("source", source.clone()));
    }
    if let Some(stop) = &request.stop_reason {
        let mut line = field("stop", stop.clone());
        if request.unusual_stop().is_some() {
            line.spans[1].style = Style::default().fg(Color::Red);
        }
        lines.push(line);
    }
    if request.rebuilt {
        let mut line = field(
            "context",
            "rebuilt: sent anew, not as a continuation (as after a compaction)",
        );
        line.spans[1].style = warn;
        lines.push(line);
    }
    if request.reshaped {
        let mut line = field(
            "shape",
            "the system prompt or the tools differ from the request before",
        );
        line.spans[1].style = warn;
        lines.push(line);
    }
    if let Some(thinking) = request.thinking_ms {
        lines.push(field(
            "thinking",
            format!("{}  approximate", duration(thinking)),
        ));
    }
    if detail.started_ms.is_some() {
        lines.push(field("started", ago(detail.started_ms, now)));
    }
    if let Some(took) = detail.took_ms {
        lines.push(field(
            "took",
            format!(
                "{}  approximate: from the request to its last block",
                duration(took)
            ),
        ));
    }
    if let Some(used) = request.tokens {
        lines.push(Line::raw(""));
        lines.push(field("input", tokens(used.input)));
        lines.push(field("output", tokens(used.output)));
        lines.push(field(
            "cache",
            format!(
                "{} read, {} written",
                tokens(used.cache_read),
                tokens(used.cache_write_5m + used.cache_write_1h)
            ),
        ));
        lines.push(field(
            "cost",
            request
                .usd
                .map_or("unknown model".to_owned(), |cost| format!("~{}", usd(cost))),
        ));
    }
    lines
}

fn agent_lines(
    agent: &AgentSummary,
    now: i64,
    width: usize,
    level: TextLevel,
) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(vec![
        title("Subagent"),
        Span::raw("  "),
        Span::styled(
            agent.agent_type.clone().unwrap_or_else(|| "agent".into()),
            Style::default().fg(Color::Magenta),
        ),
    ])];
    if let Some(task) = agent
        .description
        .as_ref()
        .filter(|_| level.shows_summaries())
    {
        lines.push(Line::raw(""));
        lines.extend(wrap(task, width));
    }
    lines.push(Line::raw(""));
    lines.push(field(
        "state",
        match agent.finished {
            Some(true) => "finished",
            Some(false) => "not finished",
            None => "no verdict yet",
        },
    ));
    if let Some(model) = &agent.model {
        lines.push(field("model", model.clone()));
    }
    if agent.started_ms.is_some() {
        lines.push(field("started", ago(agent.started_ms, now)));
    }
    if let Some(took) = agent.took_ms {
        lines.push(field(
            "took",
            format!("{}  to its latest record", duration(took)),
        ));
    }
    lines.push(Line::raw(""));
    lines.push(field("llm", count(agent.responses, "request")));
    lines.push(field("tools", count(agent.tool_calls, "call")));
    lines.push(field("tokens", tokens(agent.tokens)));
    lines.push(field(
        "cost",
        agent
            .usd
            .map_or("unknown model".to_owned(), |cost| format!("~{}", usd(cost))),
    ));
    lines
}

impl Pane<PaneEvent> for DetailPane {
    crate::impl_handle_key!(keymap);

    fn render(&mut self, f: &mut Frame, _ctx: &AppContext, shared: &PaneShared, area: Rect) {
        let rows = area.height.saturating_sub(2);
        let width = area.width.saturating_sub(2) as usize;
        let lines = fitted(self.lines(now_ms(), width), width);
        self.view_height = rows;
        self.lines = lines.len();
        self.scroll = self.scroll.min(lines.len().saturating_sub(rows as usize));
        let title = if lines.len() > rows as usize {
            format!(
                "Detail [{}-{}/{}]",
                self.scroll + 1,
                (self.scroll + rows as usize).min(lines.len()),
                lines.len()
            )
        } else {
            "Detail".to_owned()
        };
        let block = theme::pane_block(&title, shared.focused_pane == self.pane_id);
        let paragraph = Paragraph::new(lines)
            .scroll((u16::try_from(self.scroll).unwrap_or(u16::MAX), 0))
            .block(block);
        f.render_widget(paragraph, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iktinos_core::record::Record;
    use iktinos_core::session::SessionStore;

    fn text(lines: &[Line]) -> String {
        lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// A Bash call with a two-line command and a result, as the store
    /// keeps it at `level`.
    fn call(level: TextLevel) -> ToolCall {
        let mut store = SessionStore::new();
        if level.shows_output() {
            store = store.with_tool_output(4000);
        }
        for line in [
            r#"{"type":"assistant","uuid":"a1","timestamp":"2026-11-02T10:00:01.000Z","message":{"id":"m1","model":"claude-fable-5-2","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"set -e\ncargo test"}}]}}"#,
            r#"{"type":"user","uuid":"u2","timestamp":"2026-11-02T10:00:03.000Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"SYNTHETIC 3 passed","is_error":false}]}}"#,
        ] {
            store.apply("s1", &Record::parse(line));
        }
        store.get("s1").unwrap().recent_tools[0].clone()
    }

    fn pane(level: TextLevel) -> DetailPane {
        let mut pane = DetailPane::new(0, level);
        pane.show(Chosen::Tool(Box::new(call(level))));
        pane
    }

    #[test]
    fn a_calls_output_is_shown_only_at_full() {
        let shown = text(&pane(TextLevel::Full).lines(0, 60));
        assert!(shown.contains("set -e\ncargo test"), "{shown}");
        assert!(shown.contains("output    1 line, 18 characters"), "{shown}");
        assert!(shown.contains("SYNTHETIC 3 passed"), "{shown}");

        // `summary`: the whole command, and where the output is.
        let shown = text(&pane(TextLevel::Summary).lines(0, 60));
        assert!(shown.contains("set -e\ncargo test"), "{shown}");
        assert!(shown.contains("output    not read"), "{shown}");
        assert!(!shown.contains("SYNTHETIC"), "{shown}");

        // `title`: the tool and how it ended.
        let shown = text(&pane(TextLevel::Title).lines(0, 60));
        assert!(shown.starts_with("Bash  ✓  took 2.0s"), "{shown}");
        assert!(!shown.contains("cargo"), "{shown}");
    }

    #[test]
    fn the_whole_output_is_read_on_demand_and_dropped_again() {
        let mut pane = pane(TextLevel::Summary);
        assert_eq!(pane.toggle_whole(), WholeRequest::NotAtThisLevel);

        let mut pane = self::pane(TextLevel::Full);
        let key = WholeKey::Output("t1".into());
        assert_eq!(pane.toggle_whole(), WholeRequest::Read(key.clone()));
        assert!(text(&pane.lines(0, 60)).contains("reading…"));

        // An answer for something else is not this one's.
        pane.loaded(Whole {
            key: WholeKey::Output("other".into()),
            content: Ok(None),
        });
        assert!(text(&pane.lines(0, 60)).contains("reading…"));

        pane.loaded(Whole {
            key,
            content: Ok(Some(WholeText {
                text: "SYNTHETIC line one\nSYNTHETIC line two".into(),
                stderr: Some("SYNTHETIC warning".into()),
                ..WholeText::default()
            })),
        });
        let shown = text(&pane.lines(0, 60));
        assert!(
            shown.contains("all of it: 2 lines, 37 characters"),
            "{shown}"
        );
        assert!(shown.contains("SYNTHETIC line two"), "{shown}");
        assert!(shown.contains("stderr\nSYNTHETIC warning"), "{shown}");

        // `o` again: back to the start the session keeps.
        assert_eq!(pane.toggle_whole(), WholeRequest::Dropped);
        assert!(!text(&pane.lines(0, 60)).contains("line two"));

        // Choosing something else drops a whole text that is held.
        pane.toggle_whole();
        pane.show(Chosen::Nothing);
        assert!(pane.whole.is_none());
        assert_eq!(pane.toggle_whole(), WholeRequest::Nothing);
    }
}
