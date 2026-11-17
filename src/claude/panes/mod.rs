pub mod detail;
pub mod files;
pub mod interactions;
pub mod trace;

use crate::claude::domain::types::{SessionEntry, TextLevel, WholeKey};
use crate::github::domain::actions::time::{format_duration, format_relative};
use iktinos_core::record::Timestamp;
use iktinos_core::session::{Cost, CostSource, LiveState, ModelUsage, Session, Subagent, ToolCall};
use iktinos_core::trace::{between, Interaction, Item, LlmRequest, SpanKind};
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};
use std::time::{SystemTime, UNIX_EPOCH};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// A session whose transcript was written to within this is `active`.
pub const ACTIVE_WITHIN_MS: i64 = 60_000;

/// Milliseconds since the Unix epoch.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn ms(at: Option<Timestamp>) -> Option<i64> {
    at.map(Timestamp::as_millis)
}

/// `5m ago` for a time in milliseconds, empty when unknown.
pub fn ago(at_ms: Option<i64>, now_ms: i64) -> String {
    at_ms.map_or_else(String::new, |at| format_relative((now_ms - at) / 1000))
}

/// `0.4s`, `4.2s`, `1m05s`.
pub fn duration(ms: u64) -> String {
    if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        format_duration((ms / 1000) as i64)
    }
}

/// `$1.23`, or `$0.004` for less than a cent.
pub fn usd(amount: f64) -> String {
    if amount > 0.0 && amount < 0.01 {
        format!("${amount:.3}")
    } else {
        format!("${amount:.2}")
    }
}

/// `123k` / `1.2M` tokens.
pub fn tokens(n: u64) -> String {
    match n {
        0..=999 => n.to_string(),
        1_000..=999_999 => format!("{}k", n / 1000),
        _ => format!("{:.1}M", n as f64 / 1e6),
    }
}

/// `3 tool calls`, `1 tool call`.
pub fn count(n: u64, what: &str) -> String {
    format!("{n} {what}{}", if n == 1 { "" } else { "s" })
}

/// The style of secondary text. The selection bar is dark gray itself,
/// so on the selected row it is drawn a shade lighter to stay readable.
pub fn dim(selected: bool) -> Style {
    Style::default().fg(if selected {
        Color::Gray
    } else {
        Color::DarkGray
    })
}

/// `text` in exactly `width` columns: cut with an ellipsis, or padded.
pub fn fit(text: &str, width: usize) -> String {
    if text.width() <= width {
        return format!("{text}{}", " ".repeat(width - text.width()));
    }
    let mut out = String::new();
    let mut used = 0;
    for c in text.chars() {
        let w = c.width().unwrap_or(0);
        if used + w + 1 > width {
            break;
        }
        out.push(c);
        used += w;
    }
    if width > 0 {
        out.push('…');
        used += 1;
    }
    format!("{out}{}", " ".repeat(width.saturating_sub(used)))
}

/// `text` right-aligned in `width` columns.
pub fn right(text: &str, width: usize) -> String {
    format!("{}{text}", " ".repeat(width.saturating_sub(text.width())))
}

/// `text` broken into lines of at most `width` columns, at spaces where
/// there are any.
pub fn wrap(text: &str, width: usize) -> Vec<Line<'static>> {
    let width = width.max(1);
    let mut lines = Vec::new();
    for paragraph in text.lines() {
        let mut line = String::new();
        let mut used = 0;
        for word in paragraph.split_inclusive(' ') {
            let mut word = word;
            // A word wider than a line is cut wherever it has to be.
            while word.width() > width.saturating_sub(used) {
                if used > 0 {
                    lines.push(Line::raw(std::mem::take(&mut line)));
                    used = 0;
                    if word.width() <= width {
                        break;
                    }
                }
                let mut cut = 0;
                let mut columns = 0;
                for (at, c) in word.char_indices() {
                    let w = c.width().unwrap_or(0);
                    if columns + w > width {
                        break;
                    }
                    columns += w;
                    cut = at + c.len_utf8();
                }
                if cut == 0 {
                    break;
                }
                lines.push(Line::raw(word[..cut].to_owned()));
                word = &word[cut..];
            }
            line.push_str(word);
            used += word.width();
        }
        lines.push(Line::raw(line));
    }
    lines
}

/// How a tool call ended: the mark and its style.
pub fn outcome(call: &ToolCall, selected: bool) -> (&'static str, Style) {
    match call.success {
        Some(true) => ("✓", Style::default().fg(Color::Green)),
        Some(false) => ("✗", Style::default().fg(Color::Red)),
        // No result in the transcript (yet).
        None => ("·", dim(selected)),
    }
}

/// What tells one interaction of a session from another across updates:
/// its id, or its start where the prompt was not read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InteractionKey {
    id: String,
    started_ms: Option<i64>,
}

impl InteractionKey {
    pub fn of(interaction: &Interaction) -> Self {
        Self {
            id: interaction.id.clone(),
            started_ms: ms(interaction.started_at),
        }
    }
}

pub fn find_interaction<'a>(session: &'a Session, key: &InteractionKey) -> Option<&'a Interaction> {
    session
        .trace
        .iter()
        .find(|interaction| InteractionKey::of(interaction) == *key)
}

/// Whether the transcript was written to a moment ago.
pub fn session_active(session: &Session, now_ms: i64) -> bool {
    !session.exited && ms(session.last_activity).is_some_and(|at| now_ms - at <= ACTIVE_WITHIN_MS)
}

/// What a session is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    /// Working, or running a tool.
    Busy,
    /// A tool call waits for approval.
    NeedsApproval,
    /// The prompt is the user's.
    Waiting,
    /// Nothing was written for a while, and no live event says more.
    Quiet,
    Exited,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub kind: StatusKind,
    /// `running Bash`, `approve Bash?`, `waiting for input`, …
    pub text: String,
    /// One word, for a column.
    pub word: &'static str,
    /// Live events said so, not the transcript's age.
    pub live: bool,
}

impl StatusKind {
    pub fn style(self) -> Style {
        match self {
            StatusKind::Busy => Style::default().fg(Color::Green),
            StatusKind::NeedsApproval => Style::default().fg(Color::Red),
            StatusKind::Waiting => Style::default().fg(Color::Cyan),
            StatusKind::Quiet => Style::default().fg(Color::Yellow),
            StatusKind::Exited => Style::default().fg(Color::DarkGray),
        }
    }
}

/// What a session is doing: from live events where they say, from the
/// transcript otherwise — which only knows when it was last written to,
/// and cannot tell a long-running tool from waiting for input.
pub fn status(session: &Session, now_ms: i64) -> Status {
    let said = |kind, text: String, word, live| Status {
        kind,
        text,
        word,
        live,
    };
    // A transcript that ends with the exit bookkeeping outranks older
    // events: the mod's `session.end` may never have been delivered.
    let live = session
        .live
        .as_ref()
        .filter(|live| !(session.exited && live.state_at <= session.last_activity));
    if let Some(live) = live {
        match live.state() {
            LiveState::Ended => return said(StatusKind::Exited, "exited".into(), "exited", true),
            LiveState::AwaitingApproval(pending) => {
                return said(
                    StatusKind::NeedsApproval,
                    format!("approve {}?", pending.name),
                    "approve?",
                    true,
                )
            }
            LiveState::RunningTool(tool) => {
                return said(
                    StatusKind::Busy,
                    format!("running {}", tool.name),
                    "tool",
                    true,
                )
            }
            LiveState::Working => return said(StatusKind::Busy, "working".into(), "working", true),
            LiveState::WaitingForInput => {
                return said(
                    StatusKind::Waiting,
                    "waiting for input".into(),
                    "waiting",
                    true,
                )
            }
            LiveState::Unknown => {}
        }
    }
    if session.exited {
        said(StatusKind::Exited, "exited".into(), "exited", false)
    } else if session_active(session, now_ms) {
        said(StatusKind::Busy, "active".into(), "active", false)
    } else {
        said(StatusKind::Quiet, "quiet".into(), "quiet", false)
    }
}

/// Whether `interaction` is still going: it has no end, it is the
/// session's latest and the session is busy.
pub fn runs(session: &Session, interaction: &Interaction, now_ms: i64) -> bool {
    interaction.ended_at.is_none()
        && session
            .trace
            .back()
            .is_some_and(|last| std::ptr::eq(last, interaction))
        && matches!(
            status(session, now_ms).kind,
            StatusKind::Busy | StatusKind::NeedsApproval
        )
}

/// How an interaction ended, as far as the transcript says.
pub fn state_of(session: &Session, interaction: &Interaction, now_ms: i64) -> &'static str {
    if interaction.ended_at.is_some() {
        "done"
    } else if runs(session, interaction, now_ms) {
        "running"
    } else {
        "no end"
    }
}

/// The LLM requests and the tool calls of an interaction's main thread.
pub fn span_counts(interaction: &Interaction) -> (usize, usize) {
    let spans = &interaction.thread.spans;
    let calls = spans
        .iter()
        .filter(|span| matches!(span.kind, SpanKind::Tool(_)))
        .count();
    (spans.len() - calls, calls)
}

/// When the last thing of an interaction ended, its subagents included:
/// the end of its time axis. `now` while it runs.
pub fn end_ms(session: &Session, interaction: &Interaction, now_ms: i64) -> Option<i64> {
    if runs(session, interaction, now_ms) {
        return Some(now_ms);
    }
    session
        .walk(interaction)
        .iter()
        .filter_map(|node| ms(node.ended_at().or(node.started_at())))
        .chain(ms(interaction.ended_at))
        .max()
}

/// How long an interaction took: what Claude Code measured, else from its
/// start to the last thing read of it.
pub fn took_ms(session: &Session, interaction: &Interaction, now_ms: i64) -> Option<u64> {
    interaction.duration_ms.or_else(|| {
        let start = ms(interaction.started_at)?;
        let end = end_ms(session, interaction, now_ms)?;
        Some((end - start).max(0) as u64)
    })
}

/// The tools an interaction's main thread used and how often
/// (`Bash×3 Read×2`), in the order they were first used.
pub fn tools_used(interaction: &Interaction) -> String {
    let mut tools: Vec<(&str, usize)> = Vec::new();
    for span in &interaction.thread.spans {
        if let SpanKind::Tool(call) = &span.kind {
            match tools.iter_mut().find(|(known, _)| *known == call.name) {
                Some((_, n)) => *n += 1,
                None => tools.push((&call.name, 1)),
            }
        }
    }
    tools
        .iter()
        .map(|(name, n)| match n {
            1 => (*name).to_owned(),
            _ => format!("{name}×{n}"),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The tools a response called, by name, in order.
pub fn called(session: &Session, interaction: &Interaction, request: &LlmRequest) -> Vec<String> {
    let Some(id) = request.message_id.as_deref() else {
        return Vec::new();
    };
    std::iter::once(&interaction.thread)
        .chain(session.subagents.values().map(|agent| &agent.thread))
        .flat_map(|thread| &thread.spans)
        .filter_map(|span| match &span.kind {
            SpanKind::Tool(call) if call.message_id.as_deref() == Some(id) => {
                Some(call.name.clone())
            }
            _ => None,
        })
        .collect()
}

/// What a response did, in a line: the first line of what it said (from
/// `claude-text "full"`), else how long it thought and the tools it called.
pub fn request_text(request: &LlmRequest, called: &[String], level: TextLevel) -> String {
    if let Some(text) = request.text.as_ref().filter(|_| level.shows_output()) {
        return text.clone();
    }
    let mut parts = Vec::new();
    if let Some(ms) = request.thinking_ms {
        parts.push(format!("thinking {}", duration(ms)));
    }
    if !called.is_empty() {
        parts.push(format!("→ {}", called.join(", ")));
    }
    if parts.is_empty() {
        return if request.text.is_some() {
            "response".to_owned()
        } else {
            "llm request".to_owned()
        };
    }
    parts.join(" ")
}

/// What sets a request apart from the one before it, where anything does.
pub fn request_marks(request: &LlmRequest) -> Vec<&'static str> {
    let mut marks = Vec::new();
    if request.rebuilt {
        marks.push("context rebuilt");
    }
    if request.reshaped {
        marks.push("shape changed");
    }
    marks
}

/// What the Detail pane shows: owned, so the pane needs no session.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum Chosen {
    #[default]
    Nothing,
    Session(Box<SessionSummary>),
    Interaction(Box<InteractionSummary>),
    Tool(Box<ToolCall>),
    Request(Box<RequestDetail>),
    Agent(Box<AgentSummary>),
}

impl Chosen {
    /// What `o` reads whole for this.
    pub fn whole_key(&self) -> Option<WholeKey> {
        match self {
            Chosen::Tool(call) => Some(WholeKey::Output(call.id.clone())),
            Chosen::Request(detail) => detail.request.message_id.clone().map(WholeKey::Response),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RequestDetail {
    pub request: LlmRequest,
    pub started_ms: Option<i64>,
    pub took_ms: Option<u64>,
    pub called: Vec<String>,
}

/// `$1.23`; `~` marks a figure that contains a price-table estimate, and
/// `$—` stands for a cost that is not known.
pub fn cost_text(cost: &Cost) -> String {
    match cost.usd {
        Some(amount) if cost.source == CostSource::Reported => usd(amount),
        Some(amount) => format!("~{}", usd(amount)),
        None => "$—".to_owned(),
    }
}

/// A session's row, described: what is known of it without reading it,
/// and the rest once it is read.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionSummary {
    pub id: String,
    pub place: String,
    pub modified_ms: Option<i64>,
    /// It was opened and is being read.
    pub opening: bool,
    pub read: Option<SessionFacts>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionFacts {
    pub title: Option<String>,
    pub branch: Option<String>,
    pub model: Option<String>,
    pub state: String,
    /// `five_hour  42% used`, one for each rate limit live events reported.
    pub rate_limits: Vec<String>,
    pub context: Option<String>,
    pub cost: String,
    pub interactions: usize,
    pub tool_calls: u64,
    pub subagents: usize,
    pub files: usize,
    /// What the session used, by model, the most expensive first.
    pub by_model: Vec<UsageRow>,
    /// The same by who asked: the main thread, each kind of subagent, ….
    pub by_source: Vec<UsageRow>,
}

/// What one model, or one source of requests, used.
#[derive(Debug, Clone, PartialEq)]
pub struct UsageRow {
    pub name: String,
    pub requests: u64,
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_written: u64,
    /// The price-table estimate. `None` when nothing of it could be
    /// priced: an unknown model costs "unknown", never zero.
    pub usd: Option<f64>,
    /// Some of its requests were of a model the price table lacks: the
    /// estimate leaves them out.
    pub partly_unpriced: bool,
}

/// `usage` as rows, the most expensive first, then the most tokens.
fn usage_rows<'a>(usage: impl IntoIterator<Item = (&'a String, &'a ModelUsage)>) -> Vec<UsageRow> {
    let mut rows: Vec<(u64, UsageRow)> = usage
        .into_iter()
        .filter(|(_, usage)| usage.responses > 0)
        .map(|(name, usage)| {
            let usd = usage.estimated_cost();
            let row = UsageRow {
                name: name.strip_prefix("claude-").unwrap_or(name).to_owned(),
                requests: usage.responses,
                input: usage.input_tokens,
                output: usage.output_tokens,
                cache_read: usage.cache_read_tokens,
                cache_written: usage.cache_write_5m_tokens + usage.cache_write_1h_tokens,
                usd,
                partly_unpriced: usd.is_some() && usage.unpriced_responses > 0,
            };
            (usage.total_tokens(), row)
        })
        .collect();
    rows.sort_by(|(a_tokens, a), (b_tokens, b)| {
        b.usd
            .partial_cmp(&a.usd)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b_tokens.cmp(a_tokens))
            .then_with(|| a.name.cmp(&b.name))
    });
    rows.into_iter().map(|(_, row)| row).collect()
}

pub fn session_summary(entry: &SessionEntry, opening: bool, now_ms: i64) -> SessionSummary {
    SessionSummary {
        id: entry.id.clone(),
        place: entry.place.clone(),
        modified_ms: entry.modified_ms,
        opening,
        read: entry.session.as_deref().map(|session| SessionFacts {
            title: session.title.clone(),
            branch: session.git_branch.clone(),
            model: session.model.clone(),
            state: {
                let status = status(session, now_ms);
                if status.live {
                    format!("{}  (live)", status.text)
                } else {
                    status.text
                }
            },
            rate_limits: session
                .live
                .iter()
                .flat_map(|live| &live.rate_limits)
                .map(|limit| {
                    let resets = limit
                        .resets_at
                        .as_ref()
                        .map_or(String::new(), |at| format!(", resets {at}"));
                    format!("{}  {:.0}% used{resets}", limit.kind, limit.percent_used)
                })
                .collect(),
            context: session
                .current_context()
                .map(|context| match context.fill() {
                    Some(fill) => {
                        format!("{:.0}%  {} tokens", fill * 100.0, tokens(context.tokens))
                    }
                    None => format!("{} tokens", tokens(context.tokens)),
                }),
            cost: cost_text(&session.cost),
            interactions: session.trace.len(),
            tool_calls: session.tool_calls,
            subagents: session.subagents.len(),
            files: entry.files.len(),
            by_model: usage_rows(&session.usage.by_model),
            by_source: usage_rows(&session.usage.by_query_source),
        }),
    }
}

/// An interaction: how long it took and where the time went.
#[derive(Debug, Clone, PartialEq)]
pub struct InteractionSummary {
    pub prompt: Option<String>,
    pub state: &'static str,
    pub started_ms: Option<i64>,
    pub took_ms: Option<u64>,
    /// `took_ms` is Claude Code's own measurement.
    pub measured: bool,
    pub requests: u64,
    pub request_ms: u64,
    pub calls: u64,
    pub call_ms: u64,
    pub subagents: u64,
    /// Tokens and estimated cost, subagents included.
    pub tokens: u64,
    pub usd: f64,
    /// Some requests were of a model the price table does not know.
    pub unpriced: bool,
    /// Spans that were not kept because the thread was full.
    pub dropped: u64,
}

pub fn interaction_summary(
    session: &Session,
    interaction: &Interaction,
    now_ms: i64,
) -> InteractionSummary {
    // Where the time went, on the main thread; a subagent's time is
    // inside the call that spawned it.
    let (mut requests, mut request_ms, mut calls, mut call_ms) = (0, 0, 0, 0);
    for span in &interaction.thread.spans {
        let took = span.duration_ms().unwrap_or(0);
        match span.kind {
            SpanKind::Tool(_) => {
                calls += 1;
                call_ms += took;
            }
            _ => {
                requests += 1;
                request_ms += took;
            }
        }
    }
    let (mut subagents, mut tokens, mut usd, mut unpriced) = (0, 0, 0.0, false);
    for node in session.walk(interaction) {
        match node.item {
            Item::Subagent(_) => subagents += 1,
            Item::Span(span) => {
                if let SpanKind::LlmRequest(request) = &span.kind {
                    if let Some(used) = request.tokens {
                        tokens += used.input
                            + used.output
                            + used.cache_write_5m
                            + used.cache_write_1h
                            + used.cache_read;
                        match request.usd {
                            Some(cost) => usd += cost,
                            None => unpriced = true,
                        }
                    }
                }
            }
        }
    }
    InteractionSummary {
        prompt: interaction.prompt.clone(),
        state: state_of(session, interaction, now_ms),
        started_ms: ms(interaction.started_at),
        took_ms: took_ms(session, interaction, now_ms),
        measured: interaction.duration_ms.is_some(),
        requests,
        request_ms,
        calls,
        call_ms,
        subagents,
        tokens,
        usd,
        unpriced,
        dropped: interaction.thread.dropped,
    }
}

/// A subagent, without its spans.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentSummary {
    pub agent_type: Option<String>,
    pub description: Option<String>,
    pub model: Option<String>,
    pub finished: Option<bool>,
    pub started_ms: Option<i64>,
    pub took_ms: Option<u64>,
    pub tool_calls: u64,
    pub responses: u64,
    pub tokens: u64,
    /// The price-table estimate; `None` for a model the table lacks.
    pub usd: Option<f64>,
}

pub fn agent_summary(agent: &Subagent) -> AgentSummary {
    let usage = &agent.usage;
    AgentSummary {
        agent_type: agent.agent_type.clone(),
        description: agent.description.clone(),
        model: agent.model.clone(),
        finished: agent.finished,
        started_ms: ms(agent.started_at),
        took_ms: between(agent.started_at, agent.last_activity),
        tool_calls: agent.tool_calls,
        responses: usage.responses,
        tokens: usage.total_tokens(),
        usd: usage.estimated_cost().filter(|_| usage.responses > 0),
    }
}

/// A span as a bar on a time axis from `from` to `to`, `width` columns
/// wide. One without an end is a single column.
pub fn bar(start: Option<i64>, end: Option<i64>, (from, to): (i64, i64), width: usize) -> String {
    let (Some(start), true) = (start, width > 0) else {
        return String::new();
    };
    let total = (to - from).max(1) as f64;
    let column = |at: i64| ((at - from) as f64 / total).clamp(0.0, 1.0) * width as f64;
    let first = (column(start).floor() as usize).min(width - 1);
    let last = end
        .map_or(first + 1, |end| column(end).ceil() as usize)
        .clamp(first + 1, width);
    format!("{}{}", " ".repeat(first), "█".repeat(last - first))
}

/// Column names on a pane's top border: `text` starting `column` cells
/// into the pane's inner width. Left out where it would not fit, or
/// would run into the pane's `title`.
pub fn border_header(
    f: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    title: &str,
    column: usize,
    text: &str,
) {
    let x = area.x.saturating_add(1).saturating_add(column as u16);
    let room = area.right().saturating_sub(1).saturating_sub(x) as usize;
    // The title sits one cell in, with a space either side.
    let title_end = title.width() + 3;
    if area.height == 0 || text.width() > room || column < title_end {
        return;
    }
    f.buffer_mut()
        .set_string(x, area.y, text, Style::default().fg(Color::DarkGray));
}

/// `name   value` for the Detail pane.
pub fn field(name: &str, value: impl Into<String>) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{name:<10}"), Style::default().fg(Color::DarkGray)),
        Span::raw(value.into()),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_and_costs() {
        assert_eq!(duration(400), "0.4s");
        assert_eq!(duration(4200), "4.2s");
        assert_eq!(duration(65_000), "1m05s");
        assert_eq!(usd(1.234), "$1.23");
        assert_eq!(usd(0.004), "$0.004");
        assert_eq!(usd(0.0), "$0.00");
        assert_eq!(ago(None, 0), "");
        assert_eq!(ago(Some(0), 300_000), "5m ago");
        assert_eq!(tokens(45_000), "45k");
        assert_eq!(count(1, "tool call"), "1 tool call");
        assert_eq!(count(2, "tool call"), "2 tool calls");
    }

    #[test]
    fn text_is_fitted_by_columns() {
        assert_eq!(fit("abc", 5), "abc  ");
        assert_eq!(fit("abcdef", 4), "abc…");
        // Wide characters take two columns each.
        assert_eq!(fit("日本語です", 5), "日本…");
        assert_eq!(fit("日本語です", 6), "日本… ");
        assert_eq!(right("7", 3), "  7");
    }

    #[test]
    fn text_wraps_at_spaces_and_cuts_long_words() {
        let lines: Vec<String> = wrap("one two three\nabcdefghij", 7)
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        assert_eq!(lines, ["one ", "two ", "three", "abcdefg", "hij"]);
    }

    #[test]
    fn a_span_is_a_bar_on_the_interactions_axis() {
        assert_eq!(bar(Some(0), Some(50), (0, 100), 10), "█████");
        assert_eq!(bar(Some(50), Some(100), (0, 100), 10), "     █████");
        // No end: one column. No start: nothing.
        assert_eq!(bar(Some(90), None, (0, 100), 10), "         █");
        assert_eq!(bar(None, Some(5), (0, 100), 10), "");
    }

    #[test]
    fn a_requests_row_says_what_it_did() {
        let mut request = LlmRequest::default();
        assert_eq!(request_text(&request, &[], TextLevel::Full), "llm request");
        request.thinking_ms = Some(12_000);
        let called = ["Read".to_owned(), "Bash".to_owned()];
        assert_eq!(
            request_text(&request, &called, TextLevel::Summary),
            "thinking 12.0s → Read, Bash"
        );
        // What it said is drawn only at `full`.
        request.text = Some("Done.".into());
        assert_eq!(request_text(&request, &[], TextLevel::Full), "Done.");
        assert_eq!(
            request_text(&request, &[], TextLevel::Summary),
            "thinking 12.0s"
        );
    }
}
