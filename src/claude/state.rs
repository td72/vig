//! The Claude page: the Claude Code session working in this repository —
//! its latest interactions, each as a tree of LLM requests, tool calls and
//! subagents on a time axis, and the files it touched.
//!
//! There is nothing to select: the page shows the most recently updated
//! session whose working directory is under the repository or one of its
//! worktrees. Read-only: transcripts under the projects directory are
//! read by `iktinos`; nothing is written and nothing is sent to Claude
//! Code. `claude-text` decides how much text is drawn; what a response
//! said and what a tool returned are read only at `"full"`.

use crate::claude::domain::types::{LiveStatus, Snapshot, TextLevel};
use crate::claude::domain::watch::{self, ClaudeBgMessage, Command, Listen};
use crate::claude::panes::detail::{DetailAction, DetailPane, WholeRequest};
use crate::claude::panes::files::{FilesAction, FilesPane};
use crate::claude::panes::interactions::{InteractionsAction, InteractionsPane};
use crate::claude::panes::trace::{TraceAction, TracePane};
use crate::claude::panes::{cost_text, ms, now_ms, status, tokens, StatusKind};
use crate::core::app::{AppContext, PageState};
use crate::core::config::{Config, LoadedPageConfig};
use crate::core::keymap::{Keymap, ViewAction};
use crate::core::layout::{split_page_frame, PageLayoutConfig};
use crate::core::page::PageAction;
use crate::core::pane::{self, Pane, PaneEvent, PaneSet, PaneShared};
use crate::core::search::SearchState;
use crate::core::ui::status_bar;
use crate::github::domain::actions::time::format_duration;
use anyhow::Result;
use crossterm::event::KeyEvent;
use iktinos_core::session::Session;
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

/// How often the transcripts are read for appends while the page is shown.
pub const POLL_INTERVAL: Duration = Duration::from_secs(1);

/// Cells of the context gauge.
const GAUGE_CELLS: usize = 8;

/// Pane IDs resolved from the KDL config at construction time.
#[derive(Debug, Clone, Copy)]
pub struct ClaudePaneIds {
    pub interactions: usize,
    pub trace: usize,
    pub detail: usize,
    pub files: usize,
}

impl ClaudePaneIds {
    fn from_config(cfg: &LoadedPageConfig) -> Self {
        Self {
            interactions: cfg.resolve_id_expect("interactions"),
            trace: cfg.resolve_id_expect("trace"),
            detail: cfg.resolve_id_expect("detail"),
            files: cfg.resolve_id_expect("files"),
        }
    }
}

pub struct ClaudePanes {
    pub interactions: InteractionsPane,
    pub trace: TracePane,
    pub detail: DetailPane,
    pub files: FilesPane,
    pub ids: ClaudePaneIds,
}

impl PaneSet for ClaudePanes {
    fn get_mut(&mut self, idx: usize) -> Option<&mut dyn Pane<PaneEvent>> {
        if idx == self.ids.interactions {
            Some(&mut self.interactions)
        } else if idx == self.ids.trace {
            Some(&mut self.trace)
        } else if idx == self.ids.detail {
            Some(&mut self.detail)
        } else if idx == self.ids.files {
            Some(&mut self.files)
        } else {
            None
        }
    }
}

/// Counts for the status bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    pub tool_calls: u64,
    pub files: usize,
    pub subagents: usize,
}

pub struct ClaudeState {
    pub pane: PaneShared,
    pub panes: ClaudePanes,
    workdir: PathBuf,
    level: TextLevel,
    /// `claude-projects-dir`, when set.
    configured_dir: Option<PathBuf>,
    /// The session the selection is in, when it is read: what the line
    /// above the panes and the status bar describe.
    session: Option<Arc<Session>>,
    /// The id of the session the selection is in, read or not, and whose
    /// files the Files pane lists.
    selected_id: Option<String>,
    /// The Files pane is to be filled again: a snapshot arrived.
    files_stale: bool,
    /// `claude-live`: listen for live events from the bridge mod.
    listen: bool,
    /// What the live socket has heard.
    pub live: LiveStatus,
    /// A session that is read is busy: the socket should not be silent.
    pub busy: bool,
    /// The Detail pane shows the row chosen in the tree, not the
    /// interaction: the tree was the last of the two to have the focus.
    detail_of_trace: bool,
    /// `false` until the reader has answered once.
    loaded: bool,
    /// Why the projects directory cannot be read, when it cannot.
    pub unavailable: Option<String>,
    cmd_tx: Option<mpsc::Sender<Command>>,
    bg_rx: Option<mpsc::Receiver<ClaudeBgMessage>>,
    /// A poll is with the reader and not answered yet.
    pending: bool,
    last_poll: Option<Instant>,
    layout_config: PageLayoutConfig,
    view_keymap: Keymap<ViewAction>,
}

impl pane::PageLayout for ClaudeState {
    type Panes = ClaudePanes;
    fn page_parts_mut(
        &mut self,
    ) -> (
        &mut PaneShared,
        &mut Self::Panes,
        &Keymap<ViewAction>,
        &PageLayoutConfig,
    ) {
        (
            &mut self.pane,
            &mut self.panes,
            &self.view_keymap,
            &self.layout_config,
        )
    }
}

impl ClaudeState {
    pub fn new(workdir: &Path, cfg: &Config) -> Result<Self> {
        let page_cfg = cfg.claude_page()?;
        let ids = ClaudePaneIds::from_config(&page_cfg);
        let level = cfg.claude_text()?;

        let interactions_km = page_cfg.keymap::<InteractionsAction>("interactions")?;
        let trace_km = page_cfg.keymap::<TraceAction>("trace")?;
        let detail_km = page_cfg.keymap::<DetailAction>("detail")?;
        let files_km = page_cfg.keymap::<FilesAction>("files")?;
        let view_km = page_cfg.keymap::<ViewAction>("view")?;

        let mut interactions = InteractionsPane::new(ids.interactions, ids.trace, level);
        interactions.set_keymap(interactions_km);
        let mut trace = TracePane::new(ids.trace, ids.interactions, ids.detail, level);
        trace.set_keymap(trace_km);
        let mut detail = DetailPane::new(ids.detail, level);
        detail.set_keymap(detail_km);
        let mut files = FilesPane::new(ids.files);
        files.set_keymap(files_km);

        Ok(Self {
            pane: PaneShared {
                focused_pane: ids.interactions,
                previous_pane: ids.interactions,
                search: SearchState::new(),
            },
            panes: ClaudePanes {
                interactions,
                trace,
                detail,
                files,
                ids,
            },
            workdir: workdir.to_path_buf(),
            level,
            configured_dir: cfg.claude_projects_dir()?,
            session: None,
            selected_id: None,
            files_stale: false,
            listen: cfg.claude_live()?,
            live: LiveStatus::Off,
            busy: false,
            detail_of_trace: false,
            loaded: false,
            unavailable: None,
            cmd_tx: None,
            bg_rx: None,
            pending: false,
            last_poll: None,
            layout_config: page_cfg.layout,
            view_keymap: view_km,
        })
    }

    /// First switch to the page: start the reader and ask for a first read.
    fn initialize(&mut self) {
        if self.cmd_tx.is_some() {
            return;
        }
        let dir = watch::projects_dir(self.configured_dir.clone());
        self.panes.trace.set_roots(watch::repo_roots(&self.workdir));
        let listen = if self.listen {
            Listen::Shared
        } else {
            Listen::Off
        };
        let (cmd_tx, bg_rx) = watch::spawn(dir, self.workdir.clone(), self.level, listen);
        self.cmd_tx = Some(cmd_tx);
        self.bg_rx = Some(bg_rx);
        self.poll(Command::Poll);
    }

    /// Ask the reader to poll or refresh; it answers once.
    fn poll(&mut self, command: Command) {
        if let Some(tx) = &self.cmd_tx {
            if tx.send(command).is_ok() {
                self.pending = true;
                self.last_poll = Some(Instant::now());
            }
        }
    }

    /// Show `snapshot`: the selections and folds stay where they are.
    fn apply(&mut self, snapshot: Snapshot) {
        self.loaded = true;
        self.unavailable = None;
        let now = now_ms();
        self.busy = snapshot
            .sessions
            .iter()
            .filter_map(|entry| entry.session.as_deref())
            .any(|session| status(session, now).kind == StatusKind::Busy);
        self.live = snapshot.live;
        self.panes.interactions.set_sessions(snapshot.sessions);
        self.files_stale = true;
        self.sync();
    }

    /// Carry what is selected from pane to pane: the line above the panes
    /// and the files are of the selected session, the tree is of the
    /// selected interaction, the detail of whichever of the two lists was
    /// focused last.
    fn sync(&mut self) {
        let ids = self.panes.ids;
        if self.pane.focused_pane == ids.interactions {
            self.detail_of_trace = false;
        } else if self.pane.focused_pane == ids.trace {
            self.detail_of_trace = true;
        }
        let entry = self.panes.interactions.selected_entry();
        let id = entry.map(|e| e.id.clone());
        self.session = entry.and_then(|e| e.session.clone());
        if id != self.selected_id {
            self.panes.files.reset();
            self.files_stale = true;
        }
        if self.files_stale {
            let files = entry.map(|e| e.files.clone()).unwrap_or_default();
            self.panes.files.set_files(files);
            self.files_stale = false;
        }
        self.selected_id = id;

        let target = self.panes.interactions.selected_interaction();
        // On a session's row there is no tree to choose a row of.
        if target.is_none() {
            self.detail_of_trace = false;
        }
        self.panes.trace.show(target);
        let chosen = if self.detail_of_trace {
            self.panes.trace.chosen()
        } else {
            self.panes.interactions.chosen(now_ms())
        };
        self.panes.detail.show(chosen);
    }

    fn drain_bg_messages(&mut self) {
        let messages: Vec<_> = match &self.bg_rx {
            Some(rx) => rx.try_iter().collect(),
            None => return,
        };
        for message in messages {
            match message {
                ClaudeBgMessage::Whole(whole) => {
                    self.panes.detail.loaded(*whole);
                    continue;
                }
                ClaudeBgMessage::Update(snapshot) => self.apply(*snapshot),
                ClaudeBgMessage::Unchanged => {}
                ClaudeBgMessage::Unavailable(why) => {
                    self.apply(Snapshot::default());
                    self.unavailable = Some(why);
                }
            }
            self.pending = false;
        }
    }

    /// `o`: all of what the Detail pane shows, read from the transcript.
    fn read_whole(&mut self, ctx: &mut AppContext) {
        match self.panes.detail.toggle_whole() {
            WholeRequest::Read(key) => {
                if let (Some(tx), Some(id)) = (&self.cmd_tx, &self.selected_id) {
                    let _ = tx.send(Command::ReadWhole(id.clone(), key));
                }
            }
            WholeRequest::NotAtThisLevel => {
                ctx.status_message = Some(
                    "Outputs and responses are not read: set `claude-text \"full\"`".to_string(),
                );
            }
            WholeRequest::Dropped | WholeRequest::Nothing => {}
        }
    }

    fn process_events(
        &mut self,
        ctx: &mut AppContext,
        events: Vec<PaneEvent>,
    ) -> Result<PageAction> {
        for event in events {
            if pane::process_common_event(&mut self.pane, ctx, &event) {
                continue;
            }
            match event {
                PaneEvent::JumpToMatch(forward) => {
                    self.pane
                        .jump_to_search_match(&mut self.panes, ctx, forward);
                }
                PaneEvent::LoadSession(id) => {
                    // Answered with a snapshot, like a poll.
                    self.poll(Command::Open(id));
                }
                PaneEvent::ReadWhole => {
                    // From the tree, `o` is about the row under the cursor.
                    self.sync();
                    self.read_whole(ctx);
                }
                _ => {}
            }
        }
        self.sync();
        Ok(PageAction::None)
    }

    fn handle_view_key(&mut self, ctx: &mut AppContext, key: KeyEvent) -> Result<PageAction> {
        if let Some(action) = self.view_keymap.lookup(key) {
            if let Some(page_action) = pane::execute_common_view_action(ctx, *action) {
                return Ok(page_action);
            }
            if *action == ViewAction::Refresh {
                self.poll(Command::Refresh);
                return Ok(PageAction::None);
            }
        }
        let events = pane::dispatch_page_key(self, key);
        self.process_events(ctx, events)
    }

    /// The first 8 characters of the selected session's id.
    pub fn session_id(&self) -> Option<&str> {
        self.selected_id.as_deref().map(|id| &id[..id.len().min(8)])
    }

    pub fn session_count(&self) -> usize {
        self.panes.interactions.session_count()
    }

    pub fn counts(&self) -> Option<Counts> {
        self.session.as_ref().map(|s| Counts {
            tool_calls: s.tool_calls,
            files: self.panes.files.items.len(),
            subagents: s.subagents.len(),
        })
    }

    pub fn is_loading(&self) -> bool {
        !self.loaded
    }

    fn render_notice(&self, f: &mut Frame, area: Rect, lines: &[String]) {
        let dim = Style::default().fg(Color::DarkGray);
        let mut out = vec![Line::default()];
        out.extend(
            lines
                .iter()
                .map(|l| Line::from(Span::styled(format!("  {l}"), dim))),
        );
        f.render_widget(Paragraph::new(out), area);
    }
}

/// `opus-5-5` for `claude-opus-5-5`: the family prefix says nothing here.
fn short_model(model: &str) -> &str {
    model.strip_prefix("claude-").unwrap_or(model)
}

/// `▓▓▓▓▓░░░` for a fill of 0.62.
fn gauge(fill: f64) -> String {
    let filled = ((fill.clamp(0.0, 1.0) * GAUGE_CELLS as f64).round() as usize).min(GAUGE_CELLS);
    format!("{}{}", "▓".repeat(filled), "░".repeat(GAUGE_CELLS - filled))
}

/// The line above the panes: what the shown session is and how it is
/// doing — title · model · state · context · cost · branch.
pub fn session_line(view: &Session, now_ms: i64) -> Line<'static> {
    let dim = Style::default().fg(Color::DarkGray);
    let sep = || Span::styled(" · ", dim);
    let mut spans = vec![Span::raw(" ")];
    match &view.title {
        Some(title) => spans.push(Span::styled(
            title.clone(),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        None => spans.push(Span::styled("(untitled)", dim)),
    }

    if let Some(model) = &view.model {
        spans.push(sep());
        spans.push(Span::styled(
            short_model(model).to_owned(),
            Style::default().fg(Color::Cyan),
        ));
    }

    // Live events say what it is doing; without them the transcript only
    // knows when it was last written to.
    spans.push(sep());
    let status = status(view, now_ms);
    let quiet = ms(view.last_activity).map(|at| format_duration((now_ms - at).max(0) / 1000));
    spans.push(match (status.kind, quiet) {
        (StatusKind::Exited, _) => Span::styled("■ exited", dim),
        (StatusKind::Quiet, Some(quiet)) => {
            Span::styled(format!("○ quiet {quiet}"), status.kind.style())
        }
        (StatusKind::Quiet, None) => Span::styled("○ quiet", dim),
        (StatusKind::Waiting, _) => Span::styled(format!("○ {}", status.text), status.kind.style()),
        (StatusKind::NeedsApproval, _) => {
            Span::styled(format!("▲ {}", status.text), status.kind.style())
        }
        (StatusKind::Busy, _) => Span::styled(format!("● {}", status.text), status.kind.style()),
    });

    if let Some(context) = view.current_context() {
        spans.push(sep());
        spans.push(Span::styled("ctx ", dim));
        match context.fill() {
            Some(fill) => {
                let color = match fill {
                    f if f >= 0.9 => Color::Red,
                    f if f >= 0.7 => Color::Yellow,
                    _ => Color::Green,
                };
                spans.push(Span::styled(gauge(fill), Style::default().fg(color)));
                spans.push(Span::raw(format!(" {:.0}%", fill * 100.0)));
            }
            None => spans.push(Span::raw(tokens(context.tokens))),
        }
    }

    spans.push(sep());
    spans.push(match view.cost.usd {
        Some(_) => Span::raw(cost_text(&view.cost)),
        None => Span::styled("$—", dim),
    });

    if let Some(branch) = &view.git_branch {
        spans.push(sep());
        spans.push(Span::styled(
            branch.clone(),
            Style::default().fg(Color::Magenta),
        ));
    }
    Line::from(spans)
}

impl PageState for ClaudeState {
    fn id(&self) -> &'static str {
        "claude"
    }

    fn label(&self) -> &'static str {
        "Claude"
    }

    fn help_bindings(&self) -> Vec<(String, String)> {
        use crate::core::keymap::help_section;
        let mut entries = self.view_keymap.help_entries();
        entries.extend(help_section("Interactions"));
        entries.extend(self.panes.interactions.keymap().help_entries());
        entries.extend(help_section("Trace"));
        entries.extend(self.panes.trace.keymap().help_entries());
        entries.extend(help_section("Detail"));
        entries.extend(self.panes.detail.keymap().help_entries());
        entries.extend(help_section("Files"));
        entries.extend(self.panes.files.keymap().help_entries());
        entries
    }

    fn handle_key(&mut self, ctx: &mut AppContext, key: KeyEvent) -> Result<PageAction> {
        if self.pane.handle_search_input(&mut self.panes, ctx, key) {
            return Ok(PageAction::None);
        }
        self.handle_view_key(ctx, key)
    }

    fn render(&mut self, f: &mut Frame, ctx: &AppContext, area: Rect) {
        let frame = split_page_frame(area);
        status_bar::render_claude_header(f, ctx, frame.header);
        if let Some(why) = &self.unavailable {
            self.render_notice(
                f,
                frame.content,
                &[
                    format!("Claude Code transcripts not available: {why}"),
                    "Run Claude Code in this repository, or set `claude-projects-dir`, then press r."
                        .to_string(),
                ],
            );
        } else if !self.loaded {
            self.render_notice(f, frame.content, &["Loading...".to_string()]);
        } else if self.session_count() > 0 {
            self.sync();
            let [bar, content] =
                Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(frame.content);
            let line = match &self.session {
                Some(session) => session_line(session, now_ms()),
                None => Line::styled(
                    " This session is not read: Enter on its row reads its transcript",
                    Style::default().fg(Color::DarkGray),
                ),
            };
            f.render_widget(Paragraph::new(line), bar);
            pane::render_page_content(self, f, ctx, content);
        } else {
            self.render_notice(
                f,
                frame.content,
                &[
                    "No Claude Code session in this repository or its worktrees yet.".to_string(),
                    "The page lists them once they exist.".to_string(),
                ],
            );
        }
        status_bar::render_claude_status_bar(f, ctx, self, frame.status_bar);
    }

    fn intercepts_all_keys(&self) -> bool {
        self.pane.search.active
    }

    fn on_tick(&mut self, _ctx: &mut AppContext) {
        let due = self.last_poll.is_none_or(|t| t.elapsed() >= POLL_INTERVAL);
        if due && !self.pending {
            self.poll(Command::Poll);
        }
    }

    fn on_activate(&mut self, _ctx: &mut AppContext) {
        self.initialize();
    }

    fn drain_background(&mut self) {
        self.drain_bg_messages();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude::domain::watch::tests::{
        tool_result, tool_use, update, user, write_lines, Scratch,
    };
    use crate::claude::domain::watch::{Listen, Watcher};
    use crossterm::event::{KeyCode, KeyModifiers};
    use iktinos_core::record::Record;
    use iktinos_core::session::SessionStore;
    use ratatui::{backend::TestBackend, Terminal};

    fn text(line: &Line) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    /// A session folded from synthetic records: last written to at
    /// 10:00:00, 620k tokens in context.
    fn session(extra: &[&str]) -> Session {
        let mut store = SessionStore::new();
        let mut lines = vec![
            r#"{"type":"ai-title","aiTitle":"Fix the flaky test"}"#,
            r#"{"type":"assistant","uuid":"a1","timestamp":"2026-11-02T10:00:00.000Z","gitBranch":"fix/flaky","message":{"id":"m1","model":"claude-fable-5-2","usage":{"input_tokens":0,"output_tokens":10,"cache_read_input_tokens":620000},"content":[{"type":"text","text":"SYNTHETIC"}]}}"#,
        ];
        lines.extend(extra);
        for line in lines {
            store.apply("0123456789abcdef", &Record::parse(line));
        }
        store.get("0123456789abcdef").unwrap().clone()
    }

    #[test]
    fn claude_page_builds_from_builtin_config() {
        let cfg = Config::builtin();
        let state = ClaudeState::new(Path::new("/repo"), &cfg).expect("claude page");
        assert_eq!(state.id(), "claude");
        assert_eq!(state.label(), "Claude");
        let ids = state.panes.ids;
        assert_eq!(state.pane.focused_pane, ids.interactions);
        assert_eq!(
            state.layout_config.tab_panes,
            vec![ids.interactions, ids.trace, ids.detail, ids.files]
        );
        let help = state.help_bindings();
        assert_eq!(help[0], ("q".to_string(), "Quit".to_string()));
        assert!(help.iter().all(|(_, d)| d != "Switch view"));
        for label in [
            "Into the interaction's tree / open the session",
            "By session / all interactions together",
            "Show / hide a subagent's spans",
            "Read the whole output / response",
            "Copy the path",
        ] {
            assert!(help.iter().any(|(_, d)| d == label), "{label}");
        }
    }

    #[test]
    fn session_line_shows_state_context_and_cost() {
        let at = iktinos_core::record::Timestamp::parse("2026-11-02T10:00:00.000Z")
            .unwrap()
            .as_millis();
        let line = text(&session_line(&session(&[]), at + 30_000));
        assert!(
            line.starts_with(" Fix the flaky test · fable-5-2 · ● active · ctx "),
            "{line}"
        );
        assert!(line.contains(" · ~$"), "{line}");
        assert!(line.ends_with(" · fix/flaky"), "{line}");

        // Quiet for a while.
        let line = text(&session_line(&session(&[]), at + 5 * 60_000));
        assert!(line.contains(" · ○ quiet 5m00s · "), "{line}");
        assert!(line.ends_with(" · fix/flaky"), "{line}");
    }

    fn ctx() -> AppContext {
        AppContext {
            should_quit: false,
            active_page: 0,
            page_labels: vec![],
            page_keys: vec![],
            show_help: false,
            status_message: None,
            error_dialog: None,
            workdir: PathBuf::from("."),
            needs_full_redraw: false,
            last_input: Instant::now(),
            auto_refresh: true,
            page_request: None,
            select_worktree: None,
        }
    }

    fn draw(state: &mut ClaudeState) -> String {
        let ctx = ctx();
        let mut terminal = Terminal::new(TestBackend::new(160, 30)).unwrap();
        terminal.draw(|f| state.render(f, &ctx, f.area())).unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn press(state: &mut ClaudeState, ctx: &mut AppContext, code: KeyCode) {
        state
            .handle_key(ctx, KeyEvent::new(code, KeyModifiers::NONE))
            .unwrap();
    }

    fn config(level: &str) -> Config {
        let doc: kdl::KdlDocument = format!(r#"claude-text "{level}""#).parse().unwrap();
        Config::with_user(&doc, PathBuf::from("/u/config.kdl")).unwrap()
    }

    /// Several sessions in one repository: the latest are unfolded, the
    /// selection sits on the latest interaction, another session is read
    /// when its row is opened.
    #[test]
    fn lists_every_session_and_reads_one_when_it_is_opened() {
        let scratch = Scratch::new("sessions");
        let repo = scratch.0.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let projects = scratch.0.join("projects");
        for n in 0..4 {
            let path = projects.join(format!("-repo/sess{n}.jsonl"));
            let lines = [
                format!(r#"{{"type":"ai-title","aiTitle":"Title {n}"}}"#),
                user("u1", "2026-11-02T10:00:00.000Z", &repo),
                tool_use("a1", "2026-11-02T10:00:01.000Z", "t1", "Bash", "{}"),
            ];
            write_lines(&path, &lines);
            let at = std::time::UNIX_EPOCH + Duration::from_secs(1_000_000 + n);
            std::fs::File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(at)
                .unwrap();
        }
        let mut watcher = Watcher::new(
            Some(projects),
            repo.clone(),
            TextLevel::Summary,
            Listen::Off,
        );
        let mut state = ClaudeState::new(&repo, &Config::builtin()).unwrap();
        state.apply(update(watcher.poll(false)));
        let screen = draw(&mut state);
        assert!(
            screen.contains("Interactions (4 sessions · by session)"),
            "{screen}"
        );
        // The latest three are read; the selection is on the latest
        // interaction, so the line above the panes is of its session.
        assert!(screen.contains("Title 3 · fable-5-2"), "{screen}");
        assert!(screen.contains("▾ Title 3"), "{screen}");
        assert!(screen.contains("▾ Title 1"), "{screen}");
        assert!(screen.contains("▸ sess0"), "{screen}");
        assert!(screen.contains("not read"), "{screen}");
        assert!(!screen.contains("Title 0"), "{screen}");
        assert_eq!(state.session_id(), Some("sess3"));

        // Down to the session that is not read, and open it.
        let mut ctx = ctx();
        press(&mut state, &mut ctx, KeyCode::Char('G'));
        assert_eq!(state.session_id(), Some("sess0"));
        assert!(draw(&mut state).contains("This session is not read"));
        let events = state.panes.interactions.handle_key(
            &state.pane,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        assert!(matches!(&events[..], [PaneEvent::LoadSession(id)] if id == "sess0"));
        assert!(draw(&mut state).contains("reading…"));
        watcher.open("sess0");
        state.apply(update(watcher.poll(false)));
        let screen = draw(&mut state);
        assert!(screen.contains("▾ Title 0"), "{screen}");
        assert!(screen.contains("Title 0 · fable-5-2"), "{screen}");

        // `t`: every interaction together, no session rows.
        press(&mut state, &mut ctx, KeyCode::Char('t'));
        let screen = draw(&mut state);
        assert!(screen.contains("4 together"), "{screen}");
        assert!(!screen.contains("▾"), "{screen}");
        // Space folds the session of a row when sessions are rows again.
        press(&mut state, &mut ctx, KeyCode::Char('t'));
        press(&mut state, &mut ctx, KeyCode::Char(' '));
        assert!(draw(&mut state).contains("▸ Title 0"));
    }

    /// The page end to end on synthetic transcripts: the reader's
    /// snapshot applied to the state and drawn.
    #[test]
    fn renders_the_repositorys_session_from_synthetic_transcripts() {
        let scratch = Scratch::new("page");
        let repo = scratch.0.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let projects = scratch.0.join("projects");
        let cfg = Config::builtin();
        let mut state = ClaudeState::new(&repo, &cfg).unwrap();
        state.panes.trace.set_roots(vec![repo.clone()]);

        assert!(draw(&mut state).contains("Loading..."));

        let mut watcher = Watcher::new(
            Some(projects.clone()),
            repo.clone(),
            TextLevel::Summary,
            Listen::Off,
        );
        let ClaudeBgMessage::Unavailable(why) = watcher.poll(false) else {
            panic!("the projects directory does not exist yet");
        };
        state.apply(Snapshot::default());
        state.unavailable = Some(why);
        assert!(draw(&mut state).contains("Claude Code transcripts not available"));

        std::fs::create_dir_all(&projects).unwrap();
        state.apply(update(watcher.poll(false)));
        assert!(draw(&mut state).contains("No Claude Code session in this repository"));

        let edited = repo.join("src/lib.rs");
        let lines = [
            r#"{"type":"ai-title","aiTitle":"Tidy the parser"}"#.to_string(),
            user("u1", "2026-11-02T10:00:00.000Z", &repo),
            tool_use(
                "a1",
                "2026-11-02T10:00:01.000Z",
                "t1",
                "Bash",
                r#"{"command":"cargo test --secret"}"#,
            ),
            tool_result("r1", "2026-11-02T10:00:03.000Z", "t1", "SYNTHETIC 3 passed"),
            tool_use(
                "a2",
                "2026-11-02T10:00:04.000Z",
                "t2",
                "Edit",
                &format!(r#"{{"file_path":"{}"}}"#, edited.display()),
            ),
        ];
        write_lines(&projects.join("-repo/dddd0000.jsonl"), &lines);
        state.apply(update(watcher.poll(true)));
        let screen = draw(&mut state);
        assert!(screen.contains("Tidy the parser"), "{screen}");
        assert!(screen.contains("fable-5-2"), "{screen}");
        // The default level says what happened: the prompt's first line
        // and what each call ran, the calls under the request that made
        // them.
        assert!(
            screen.contains("Interactions (1 session · by session)"),
            "{screen}"
        );
        assert!(screen.contains("▾ Tidy the parser"), "{screen}");
        assert!(screen.contains("hidden"), "{screen}");
        assert!(
            screen.contains("Trace (2 LLM requests, 2 tool calls)"),
            "{screen}"
        );
        assert!(screen.contains("→ Bash"), "{screen}");
        assert!(screen.contains("Bash  cargo test --secret"), "{screen}");
        assert!(screen.contains("Edit  src/lib.rs"), "{screen}");
        assert!(screen.contains("E src/lib.rs"), "{screen}");
        assert!(screen.contains("session dddd0000"), "{screen}");
        assert!(screen.contains("1 session here"), "{screen}");
        // The interaction is what the Detail pane is about.
        assert!(screen.contains("llm       2 requests"), "{screen}");
        // No output is read at this level.
        assert!(!screen.contains("SYNTHETIC"), "{screen}");

        // Into the tree, down to the Bash call: its command, not its output.
        let mut ctx = ctx();
        press(&mut state, &mut ctx, KeyCode::Enter);
        assert_eq!(state.pane.focused_pane, state.panes.ids.trace);
        press(&mut state, &mut ctx, KeyCode::Char('j'));
        let screen = draw(&mut state);
        assert!(screen.contains("Bash  ✓  took 2.0s"), "{screen}");
        assert!(screen.contains("output    not read"), "{screen}");
        press(&mut state, &mut ctx, KeyCode::Char('o'));
        assert!(ctx
            .status_message
            .as_deref()
            .unwrap()
            .contains("claude-text"));
        // Back out: the interaction again.
        press(&mut state, &mut ctx, KeyCode::Char('h'));
        assert_eq!(state.pane.focused_pane, state.panes.ids.interactions);
        assert!(draw(&mut state).contains("llm       2 requests"));

        // `claude-text "title"`: tool names and paths only.
        let mut state = ClaudeState::new(&repo, &config("title")).unwrap();
        state.panes.trace.set_roots(vec![repo.clone()]);
        state.apply(update(watcher.poll(true)));
        let screen = draw(&mut state);
        assert!(screen.contains("Bash Edit"), "{screen}");
        assert!(screen.contains("E src/lib.rs"), "{screen}");
        assert!(!screen.contains("--secret"), "{screen}");
        assert!(!screen.contains("hidden"), "{screen}");
        assert_eq!(
            state.counts(),
            Some(Counts {
                tool_calls: 2,
                files: 1,
                subagents: 0
            })
        );

        // `claude-text "full"`: what the call returned, and all of it on `o`.
        let mut watcher = Watcher::new(Some(projects), repo.clone(), TextLevel::Full, Listen::Off);
        let mut state = ClaudeState::new(&repo, &config("full")).unwrap();
        state.apply(update(watcher.poll(false)));
        let mut ctx = self::ctx();
        press(&mut state, &mut ctx, KeyCode::Enter);
        press(&mut state, &mut ctx, KeyCode::Char('j'));
        let screen = draw(&mut state);
        assert!(
            screen.contains("output    1 line, 18 characters"),
            "{screen}"
        );
        assert!(screen.contains("SYNTHETIC 3 passed"), "{screen}");
        let key = crate::claude::domain::types::WholeKey::Output("t1".into());
        assert_eq!(
            state.panes.detail.toggle_whole(),
            WholeRequest::Read(key.clone())
        );
        state
            .panes
            .detail
            .loaded(watcher.read_whole("dddd0000", key));
        let screen = draw(&mut state);
        assert!(screen.contains("output    all of it: 1 line"), "{screen}");
    }
}
