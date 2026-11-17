//! The background reader: finds the repository's sessions under the
//! projects directory, follows the ones that are read and hands the page
//! a [`Snapshot`] whenever something changed.
//!
//! Read-only: `iktinos` lists directories and reads transcript files;
//! nothing is written and nothing is sent to Claude Code. Reading a
//! session means reading its whole transcript, so not every session is
//! read: the [`EAGER_SESSIONS`] most recently updated are, and any other
//! when the page opens it. The rest are listed from their file metadata.
//!
//! Unless `claude-live "off"`, the reader also listens for live events
//! from the `iktinos-bridge` Claude Code mod on a Unix socket of its own
//! (`~/.local/state/iktinos/sockets/<pid>.sock`, removed when vig exits).
//! It only listens: nothing is sent back but the HTTP status. Events say
//! what a session is doing right now; they are applied to the sessions
//! that are read. What a tool returned is kept (its start) only at
//! `claude-text "full"`; the whole of one output or response is read again
//! from the transcript when the page asks for it, and kept nowhere here.

use super::files::{display_path, touched};
use super::types::{
    LiveStatus, SessionEntry, Snapshot, TextLevel, TouchedFile, Whole, WholeKey, WholeText,
};
use crate::worktrees::domain::worktree::list_worktrees;
use iktinos_core::discover::{default_projects_dir, discover, SessionFiles};
use iktinos_core::output::{read_response, read_tool_output};
use iktinos_core::place::Place;
use iktinos_core::record::{Record, OUTPUT_CHARS};
use iktinos_core::session::{Session, SessionStore};
use iktinos_core::tail::Tail;
use iktinos_live::event::Event;
use iktinos_live::merge::apply_event;
use iktinos_live::server::Server;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};
use std::time::{SystemTime, UNIX_EPOCH};

/// How many of the most recently updated sessions are read without being
/// asked for.
pub const EAGER_SESSIONS: usize = 3;

/// How many recent tool calls of a session are kept: what the Files pane
/// is built from.
pub const RECENT_TOOLS: usize = 2000;

/// How many of its latest interactions a session keeps as a
/// trace: what the Interactions pane lists.
pub const TRACE_INTERACTIONS: usize = 100;

/// The projects directory is listed again every this many polls; the
/// polls in between only read what was appended to the known files.
const DISCOVER_EVERY: u32 = 5;

/// Overrides the projects directory. Not user-facing: the demo tape and
/// the tests point it at synthetic transcripts.
pub const PROJECTS_DIR_ENV: &str = "VIG_CLAUDE_PROJECTS_DIR";

/// Overrides the directory the live socket is opened in. Not user-facing:
/// the demo tape and the tests keep out of the directory the mod posts to.
pub const SOCKET_DIR_ENV: &str = "VIG_CLAUDE_SOCKET_DIR";

/// Whether and where to listen for live events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Listen {
    Off,
    /// In the directory every consumer shares, or the test override.
    Shared,
    #[cfg(test)]
    In(PathBuf),
}

/// What the page asks of the reader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Read what was appended.
    Poll,
    /// Start over: worktree roots, the listing and the transcripts.
    Refresh,
    /// Read this session too: the page opened it.
    Open(String),
    /// Read the whole of one output or response of this session.
    ReadWhole(String, WholeKey),
}

pub enum ClaudeBgMessage {
    Update(Box<Snapshot>),
    /// Polled; nothing changed.
    Unchanged,
    /// The projects directory cannot be read (Claude Code never ran here).
    Unavailable(String),
    /// The answer to [`Command::ReadWhole`].
    Whole(Box<Whole>),
}

/// Where the transcripts are: the demo / test override, then
/// `claude-projects-dir`, then Claude Code's own default.
pub fn projects_dir(configured: Option<PathBuf>) -> Option<PathBuf> {
    match std::env::var_os(PROJECTS_DIR_ENV) {
        Some(dir) if !dir.is_empty() => Some(PathBuf::from(dir)),
        _ => configured.or_else(default_projects_dir),
    }
}

/// Start the reader thread. It answers every poll or refresh with one
/// message, every [`Command::ReadWhole`] with one more, and ends when the
/// page drops its sender.
pub fn spawn(
    projects_dir: Option<PathBuf>,
    workdir: PathBuf,
    level: TextLevel,
    listen: Listen,
) -> (mpsc::Sender<Command>, mpsc::Receiver<ClaudeBgMessage>) {
    let (cmd_tx, cmd_rx) = mpsc::channel::<Command>();
    let (msg_tx, msg_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut watcher = Watcher::new(projects_dir, workdir, level, listen);
        while let Ok(first) = cmd_rx.recv() {
            // Polls queued while the last command ran collapse into one.
            let queued: Vec<Command> = std::iter::once(first).chain(cmd_rx.try_iter()).collect();
            let mut answers = Vec::new();
            for command in &queued {
                if let Command::Open(id) = command {
                    watcher.open(id);
                }
            }
            if queued.iter().any(|c| !matches!(c, Command::ReadWhole(..))) {
                answers.push(watcher.poll(queued.contains(&Command::Refresh)));
            }
            for command in queued {
                if let Command::ReadWhole(id, key) = command {
                    answers.push(ClaudeBgMessage::Whole(Box::new(
                        watcher.read_whole(&id, key),
                    )));
                }
            }
            if answers
                .into_iter()
                .any(|answer| msg_tx.send(answer).is_err())
            {
                break;
            }
        }
    });
    (cmd_tx, msg_rx)
}

fn canon(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// The roots a file of this repository can be under: the working
/// directory and every worktree, as given and canonical. Touched files
/// are shown relative to them.
pub fn repo_roots(workdir: &Path) -> Vec<PathBuf> {
    let mut roots = vec![workdir.to_path_buf()];
    if let Ok(worktrees) = list_worktrees(workdir) {
        roots.extend(worktrees.into_iter().filter(|w| !w.bare).map(|w| w.path));
    }
    let canonical: Vec<PathBuf> = roots.iter().map(|r| canon(r)).collect();
    roots.extend(canonical);
    roots.sort();
    roots.dedup();
    roots
}

/// When the session was last written to, subagents included.
fn last_modified(files: &SessionFiles) -> Option<SystemTime> {
    files
        .subagents
        .iter()
        .filter_map(|s| s.transcript.modified)
        .chain(files.transcript.modified)
        .max()
}

/// Opens the live socket, where the page is to listen.
fn listen_for_events(listen: Listen) -> (Option<(Server, mpsc::Receiver<Event>)>, LiveStatus) {
    let started = match listen {
        Listen::Off => return (None, LiveStatus::Off),
        Listen::Shared => match std::env::var_os(SOCKET_DIR_ENV) {
            Some(dir) if !dir.is_empty() => Server::start_in(Path::new(&dir)),
            _ => Server::start(),
        },
        #[cfg(test)]
        Listen::In(dir) => Server::start_in(&dir),
    };
    match started {
        Ok((server, events)) => {
            let live = LiveStatus::Listening {
                socket: server.socket_path().to_path_buf(),
                events: 0,
                last_ms: None,
                rejected: 0,
            };
            (Some((server, events)), live)
        }
        Err(e) => (None, LiveStatus::Failed(e.to_string())),
    }
}

fn millis(at: Option<SystemTime>) -> Option<i64> {
    let since = at?.duration_since(UNIX_EPOCH).ok()?;
    Some(since.as_millis() as i64)
}

/// A session that is read.
struct Active {
    store: SessionStore,
    tails: HashMap<PathBuf, Tail>,
    /// What the page is given of it, made anew when the session grew.
    shown: Option<(Arc<Session>, Vec<TouchedFile>)>,
    /// A live event changed the session since `shown` was made.
    heard: bool,
}

/// What one read of a session's files found.
struct Read {
    /// `false` when a file was replaced: everything must be read again.
    intact: bool,
    /// Records were appended.
    grew: bool,
}

impl Active {
    fn new(level: TextLevel) -> Self {
        let mut store = SessionStore::new()
            .with_recent_tools(RECENT_TOOLS)
            .with_trace_interactions(TRACE_INTERACTIONS);
        // A tool's output is not read into the page below `full`.
        if level.shows_output() {
            store = store.with_tool_output(OUTPUT_CHARS);
        }
        Self {
            store,
            tails: HashMap::new(),
            shown: None,
            heard: false,
        }
    }

    /// Reads what was appended to the session's transcripts.
    fn read(&mut self, files: &SessionFiles) -> Read {
        self.store.discovered(files);
        let paths = std::iter::once(&files.transcript.path)
            .chain(files.subagents.iter().map(|s| &s.transcript.path));
        let mut read = Read {
            intact: true,
            grew: false,
        };
        for path in paths {
            if !self.tails.contains_key(path) {
                self.tails.insert(path.clone(), Tail::from_start(path));
                // A subagent that was just listed is news by itself.
                read.grew = true;
            }
        }
        let Self { store, tails, .. } = self;
        // A file that cannot be read now (removed, mid-rotation) is tried
        // again on the next poll.
        tails.retain(|_, tail| {
            match tail.poll_each(|line, _| store.apply(&files.id, &Record::parse_bytes(line))) {
                Ok(polled) => {
                    read.intact &= !polled.restarted;
                    read.grew |= polled.lines > 0;
                    true
                }
                Err(_) => false,
            }
        });
        read
    }
}

pub struct Watcher {
    projects_dir: Option<PathBuf>,
    workdir: PathBuf,
    level: TextLevel,
    /// Whether a session's directory is the repository or one of its
    /// worktrees.
    place: Place,
    roots: Vec<PathBuf>,
    /// The repository's sessions as last listed, the most recently
    /// updated first.
    found: Vec<SessionFiles>,
    polls: u32,
    /// The sessions that are read, by id.
    active: HashMap<String, Active>,
    /// Sessions the page opened: read as long as they are listed.
    opened: Vec<String>,
    /// What the page was last told, in short: each session, when it was
    /// written to and whether it is read. `None` before the first answer.
    told: Option<Vec<(String, Option<SystemTime>, bool)>>,
    /// The live socket and the events it hands over. Dropping it removes
    /// the socket.
    server: Option<(Server, mpsc::Receiver<Event>)>,
    live: LiveStatus,
}

impl Watcher {
    pub fn new(
        projects_dir: Option<PathBuf>,
        workdir: PathBuf,
        level: TextLevel,
        listen: Listen,
    ) -> Self {
        let (server, live) = listen_for_events(listen);
        Self {
            server,
            live,
            place: Place::new(&workdir),
            roots: repo_roots(&workdir),
            projects_dir,
            workdir,
            level,
            found: Vec::new(),
            polls: 0,
            active: HashMap::new(),
            opened: Vec::new(),
            told: None,
        }
    }

    /// The page opened this session: read it from now on.
    pub fn open(&mut self, id: &str) {
        if !self.opened.iter().any(|known| known == id) {
            self.opened.push(id.to_owned());
        }
    }

    pub fn poll(&mut self, refresh: bool) -> ClaudeBgMessage {
        let Some(dir) = self.projects_dir.clone() else {
            return ClaudeBgMessage::Unavailable(
                "cannot locate the Claude Code projects directory (no home directory)".into(),
            );
        };
        if refresh {
            self.place = Place::new(&self.workdir);
            self.roots = repo_roots(&self.workdir);
            self.active.clear();
            self.told = None;
        }
        if refresh || self.polls.is_multiple_of(DISCOVER_EVERY) {
            match discover(&dir) {
                Ok(all) => {
                    self.found = all
                        .into_iter()
                        .filter(|f| f.cwd.as_deref().is_some_and(|cwd| self.place.holds(cwd)))
                        .collect();
                    self.found.sort_by(|a, b| {
                        last_modified(b)
                            .cmp(&last_modified(a))
                            .then(a.id.cmp(&b.id))
                    });
                }
                Err(e) => {
                    self.found.clear();
                    self.active.clear();
                    self.told = None;
                    // Try again on the next poll: the directory appears
                    // the first time Claude Code runs.
                    self.polls = 0;
                    return ClaudeBgMessage::Unavailable(e.to_string());
                }
            }
        }
        self.polls = self.polls.wrapping_add(1);

        let heard = self.hear();
        match self.read(heard) {
            Some(snapshot) => ClaudeBgMessage::Update(Box::new(snapshot)),
            None => ClaudeBgMessage::Unchanged,
        }
    }

    /// Takes the live events that arrived and applies them to the sessions
    /// that are read. `true` when any arrived: the counts changed.
    fn hear(&mut self) -> bool {
        let Some((server, events)) = &self.server else {
            return false;
        };
        let mut arrived = 0;
        for event in events.try_iter() {
            arrived += 1;
            // An event of a session that is not read is counted, no more:
            // applying it would make a session of its own out of it.
            if let Some(active) = self.active.get_mut(&event.session_id) {
                apply_event(&mut active.store, &event);
                active.heard = true;
            }
        }
        let rejected_now = server.rejected();
        let LiveStatus::Listening {
            events,
            last_ms,
            rejected,
            ..
        } = &mut self.live
        else {
            return false;
        };
        let changed = arrived > 0 || *rejected != rejected_now;
        *rejected = rejected_now;
        if arrived > 0 {
            *events += arrived;
            *last_ms = millis(Some(SystemTime::now()));
        }
        changed
    }

    /// Reads the sessions that are read: the latest few and those the
    /// page opened. `None` when the page already has what there is.
    fn read(&mut self, heard: bool) -> Option<Snapshot> {
        let mut news = heard;
        // A session that is read stays read while it is listed.
        self.active
            .retain(|id, _| self.found.iter().any(|f| &f.id == id));
        for (rank, files) in self.found.iter().enumerate() {
            let wanted = rank < EAGER_SESSIONS
                || self.opened.contains(&files.id)
                || self.active.contains_key(&files.id);
            if !wanted {
                continue;
            }
            let level = self.level;
            let active = self
                .active
                .entry(files.id.clone())
                .or_insert_with(|| Active::new(level));
            let mut read = active.read(files);
            if !read.intact {
                // A transcript was replaced: forget what was folded and
                // read the session's files from the start.
                *active = Active::new(level);
                active.read(files);
                read.grew = true;
            }
            if read.grew || active.heard || active.shown.is_none() {
                active.heard = false;
                active.shown = active
                    .store
                    .get(&files.id)
                    .map(|session| (Arc::new(session.clone()), touched(session, &self.roots)));
                news = true;
            }
        }
        let telling: Vec<_> = self
            .found
            .iter()
            .map(|f| {
                (
                    f.id.clone(),
                    last_modified(f),
                    self.active.contains_key(&f.id),
                )
            })
            .collect();
        if !news && self.told.as_ref() == Some(&telling) {
            return None;
        }
        self.told = Some(telling);
        let sessions = self
            .found
            .iter()
            .map(|files| {
                let shown = self.active.get(&files.id).and_then(|a| a.shown.clone());
                let (session, touched) = shown.unzip();
                SessionEntry {
                    id: files.id.clone(),
                    place: self.place_name(files),
                    modified_ms: millis(last_modified(files)),
                    session,
                    files: touched.unwrap_or_default(),
                }
            })
            .collect();
        Some(Snapshot {
            sessions,
            live: self.live.clone(),
        })
    }

    /// Where a session runs, from the repository: `.` for its root, a
    /// worktree or a subdirectory by its path.
    fn place_name(&self, files: &SessionFiles) -> String {
        let Some(cwd) = &files.cwd else {
            return "?".into();
        };
        let canonical = canon(cwd);
        let here = canon(&self.workdir);
        if canonical == here {
            return ".".into();
        }
        match canonical.strip_prefix(&here) {
            Ok(below) => below.to_string_lossy().into_owned(),
            // Another worktree: by its directory's name.
            Err(_) => canonical.file_name().map_or_else(
                || display_path(&canonical.to_string_lossy(), &[]),
                |name| name.to_string_lossy().into_owned(),
            ),
        }
    }

    /// Reads all of one output or response of a session from its files.
    /// Nothing of it is kept here.
    pub fn read_whole(&self, session_id: &str, key: WholeKey) -> Whole {
        // Nothing a tool returned or a response said is read below `full`.
        if !self.level.shows_output() {
            return Whole {
                key,
                content: Err("not read at this `claude-text` level".into()),
            };
        }
        let Some(files) = self.found.iter().find(|f| f.id == session_id) else {
            return Whole {
                key,
                content: Ok(None),
            };
        };
        let content = match &key {
            WholeKey::Output(id) => read_tool_output(files, id).map(|found| {
                found.map(|output| WholeText {
                    text: output.text,
                    stderr: output.stderr,
                    truncated: output.truncated,
                    interrupted: output.interrupted,
                    other_blocks: output.other_blocks,
                    file: output.file,
                })
            }),
            WholeKey::Response(id) => read_response(files, id).map(|found| {
                found.map(|response| WholeText {
                    text: response.text,
                    truncated: response.truncated,
                    ..WholeText::default()
                })
            }),
        };
        Whole {
            key,
            content: content.map_err(|e| e.to_string()),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    /// A scratch directory removed when dropped.
    pub(crate) struct Scratch(pub PathBuf);

    impl Scratch {
        pub(crate) fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "vig-claude-{name}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            // Session cwds are compared canonically (`/tmp` vs `/private/tmp`).
            Self(fs::canonicalize(&dir).unwrap())
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    pub(crate) fn user(uuid: &str, at: &str, cwd: &Path) -> String {
        format!(
            r#"{{"type":"user","uuid":"{uuid}","timestamp":"{at}","cwd":"{}","gitBranch":"main","message":{{"role":"user","content":"hidden"}}}}"#,
            cwd.display()
        )
    }

    pub(crate) fn tool_use(uuid: &str, at: &str, id: &str, name: &str, input: &str) -> String {
        format!(
            r#"{{"type":"assistant","uuid":"{uuid}","timestamp":"{at}","message":{{"id":"m-{uuid}","model":"claude-fable-5-2","usage":{{"input_tokens":10,"output_tokens":20,"cache_read_input_tokens":4000}},"content":[{{"type":"tool_use","id":"{id}","name":"{name}","input":{input}}}]}}}}"#
        )
    }

    pub(crate) fn write_lines(path: &Path, lines: &[String]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        for line in lines {
            writeln!(file, "{line}").unwrap();
        }
    }

    pub(crate) fn tool_result(uuid: &str, at: &str, id: &str, output: &str) -> String {
        format!(
            r#"{{"type":"user","uuid":"{uuid}","timestamp":"{at}","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"{id}","content":"{output}","is_error":false}}]}}}}"#
        )
    }

    pub(crate) fn update(message: ClaudeBgMessage) -> Snapshot {
        match message {
            ClaudeBgMessage::Update(snapshot) => *snapshot,
            ClaudeBgMessage::Unchanged => panic!("expected an update, got Unchanged"),
            ClaudeBgMessage::Unavailable(e) => panic!("expected an update, got {e}"),
            ClaudeBgMessage::Whole(_) => panic!("expected an update, got a whole text"),
        }
    }

    /// The one session of a snapshot, read.
    pub(crate) fn only(snapshot: Snapshot) -> Arc<Session> {
        assert_eq!(snapshot.sessions.len(), 1);
        snapshot.sessions[0]
            .session
            .clone()
            .expect("a read session")
    }

    #[test]
    fn lists_the_sessions_of_the_repository_only() {
        let scratch = Scratch::new("watch");
        let repo = scratch.0.join("repo");
        let other = scratch.0.join("other");
        fs::create_dir_all(repo.join("sub")).unwrap();
        fs::create_dir_all(&other).unwrap();
        let projects = scratch.0.join("projects");

        let mut watcher = Watcher::new(
            Some(projects.clone()),
            repo.clone(),
            TextLevel::Summary,
            Listen::Off,
        );
        assert!(matches!(
            watcher.poll(false),
            ClaudeBgMessage::Unavailable(_)
        ));

        // A session elsewhere is not this repository's.
        write_lines(
            &projects.join("-other/aaaa.jsonl"),
            &[user("u1", "2026-11-02T10:00:00.000Z", &other)],
        );
        let snapshot = update(watcher.poll(false));
        assert!(snapshot.sessions.is_empty());
        // Still none: nothing to redraw.
        assert!(matches!(watcher.poll(false), ClaudeBgMessage::Unchanged));

        // A session in a subdirectory of the repository is.
        let transcript = projects.join("-repo-sub/bbbb.jsonl");
        write_lines(
            &transcript,
            &[
                user("u1", "2026-11-02T10:00:00.000Z", &repo.join("sub")),
                tool_use(
                    "a1",
                    "2026-11-02T10:00:01.000Z",
                    "t1",
                    "Edit",
                    &format!(r#"{{"file_path":"{}"}}"#, repo.join("src/a.rs").display()),
                ),
            ],
        );
        let snapshot = update(watcher.poll(true));
        assert_eq!(snapshot.sessions.len(), 1);
        let entry = &snapshot.sessions[0];
        assert_eq!(entry.place, "sub");
        let session = entry.session.clone().expect("the latest sessions are read");
        assert_eq!(session.id, "bbbb");
        assert_eq!(session.git_branch.as_deref(), Some("main"));
        assert_eq!(session.trace.len(), 1);
        assert_eq!(session.trace[0].prompt.as_deref(), Some("hidden"));
        assert_eq!(session.tool_calls, 1);
        assert_eq!(entry.files[0].path, "src/a.rs");

        // Nothing appended: nothing to redraw.
        assert!(matches!(watcher.poll(false), ClaudeBgMessage::Unchanged));

        // Appends are read without listing the directory again.
        write_lines(
            &transcript,
            &[tool_use(
                "a2",
                "2026-11-02T10:00:02.000Z",
                "t2",
                "Bash",
                r#"{"command":"cargo test"}"#,
            )],
        );
        assert_eq!(only(update(watcher.poll(false))).tool_calls, 2);
    }

    #[test]
    fn only_the_latest_sessions_are_read_until_another_is_opened() {
        let scratch = Scratch::new("lazy");
        let repo = scratch.0.join("repo");
        fs::create_dir_all(&repo).unwrap();
        let projects = scratch.0.join("projects");
        // Five sessions, s0 the oldest.
        for n in 0..5 {
            let path = projects.join(format!("-repo/s{n}.jsonl"));
            write_lines(&path, &[user("u1", "2026-11-02T10:00:00.000Z", &repo)]);
            let at = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000 + n);
            fs::File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(at)
                .unwrap();
        }
        let mut watcher = Watcher::new(Some(projects), repo, TextLevel::Summary, Listen::Off);
        let read = |snapshot: &Snapshot| -> Vec<(String, bool)> {
            snapshot
                .sessions
                .iter()
                .map(|e| (e.id.clone(), e.session.is_some()))
                .collect()
        };
        let listed = |ids: [(&str, bool); 5]| ids.map(|(id, read)| (id.to_owned(), read)).to_vec();
        // The most recently updated first; the latest three are read.
        let snapshot = update(watcher.poll(false));
        assert_eq!(
            read(&snapshot),
            listed([
                ("s4", true),
                ("s3", true),
                ("s2", true),
                ("s1", false),
                ("s0", false)
            ])
        );
        assert_eq!(snapshot.sessions[0].place, ".");
        assert!(matches!(watcher.poll(false), ClaudeBgMessage::Unchanged));

        // Opening one reads it, and it stays read.
        watcher.open("s0");
        let snapshot = update(watcher.poll(false));
        assert!(read(&snapshot).contains(&("s0".to_owned(), true)));
        assert!(read(&snapshot).contains(&("s1".to_owned(), false)));
        assert!(matches!(watcher.poll(false), ClaudeBgMessage::Unchanged));
    }

    #[test]
    fn a_replaced_transcript_is_read_again_from_the_start() {
        let scratch = Scratch::new("restart");
        let repo = scratch.0.join("repo");
        fs::create_dir_all(&repo).unwrap();
        let projects = scratch.0.join("projects");
        let transcript = projects.join("-repo/cccc.jsonl");
        write_lines(
            &transcript,
            &[
                user("u1", "2026-11-02T10:00:00.000Z", &repo),
                tool_use("a1", "2026-11-02T10:00:01.000Z", "t1", "Bash", "{}"),
                tool_use("a2", "2026-11-02T10:00:02.000Z", "t2", "Bash", "{}"),
            ],
        );
        let mut watcher = Watcher::new(
            Some(projects),
            repo.clone(),
            TextLevel::Summary,
            Listen::Off,
        );
        assert_eq!(only(update(watcher.poll(false))).tool_calls, 2);

        // Shorter than what was read: the file was replaced.
        fs::write(
            &transcript,
            format!("{}\n", user("u9", "2026-11-02T11:00:00.000Z", &repo)),
        )
        .unwrap();
        let session = only(update(watcher.poll(false)));
        assert_eq!(session.tool_calls, 0);
        assert_eq!(session.trace.len(), 1);
    }

    #[test]
    fn outputs_are_read_only_at_full() {
        let scratch = Scratch::new("output");
        let repo = scratch.0.join("repo");
        fs::create_dir_all(&repo).unwrap();
        let projects = scratch.0.join("projects");
        write_lines(
            &projects.join("-repo/eeee.jsonl"),
            &[
                user("u1", "2026-11-02T10:00:00.000Z", &repo),
                tool_use("a1", "2026-11-02T10:00:01.000Z", "t1", "Bash", "{}"),
                tool_result("r1", "2026-11-02T10:00:02.000Z", "t1", "SYNTHETIC output"),
            ],
        );
        let key = WholeKey::Output("t1".into());

        // Below `full` a call keeps nothing of what it returned, and the
        // whole of it is not read either.
        let mut watcher = Watcher::new(
            Some(projects.clone()),
            repo.clone(),
            TextLevel::Summary,
            Listen::Off,
        );
        let session = only(update(watcher.poll(false)));
        assert!(session.recent_tools[0].output.is_none());
        assert!(watcher.read_whole("eeee", key.clone()).content.is_err());

        let mut watcher = Watcher::new(Some(projects), repo, TextLevel::Full, Listen::Off);
        let session = only(update(watcher.poll(false)));
        let output = session.recent_tools[0].output.as_ref().expect("an excerpt");
        assert_eq!(output.text, "SYNTHETIC output");
        let whole = watcher.read_whole("eeee", key.clone());
        assert_eq!(
            whole.content.unwrap().expect("a result").text,
            "SYNTHETIC output"
        );
        // A call the session does not have, and a session that is not listed.
        let none = watcher.read_whole("eeee", WholeKey::Output("nope".into()));
        assert_eq!(none.content.unwrap(), None);
        assert_eq!(watcher.read_whole("nope", key).content.unwrap(), None);
    }

    /// Posts one event to the socket the way the bridge mod does.
    fn post(socket: &Path, event: &str) -> String {
        use std::io::Read;
        let mut stream = std::os::unix::net::UnixStream::connect(socket).unwrap();
        write!(
            stream,
            "POST /events HTTP/1.1\r\nHost: iktinos\r\nContent-Length: {}\r\n\r\n{event}",
            event.len()
        )
        .unwrap();
        let mut answer = String::new();
        stream.read_to_string(&mut answer).unwrap();
        answer
    }

    #[test]
    fn live_events_say_what_a_read_session_is_doing() {
        let scratch = Scratch::new("live");
        let repo = scratch.0.join("repo");
        fs::create_dir_all(&repo).unwrap();
        let projects = scratch.0.join("projects");
        write_lines(
            &projects.join("-repo/ffff.jsonl"),
            &[user("u1", "2026-11-02T10:00:00.000Z", &repo)],
        );
        // A short path: a socket's is limited to about a hundred bytes.
        let sockets = std::env::temp_dir().join(format!("vig-s{}", std::process::id()));
        let _ = fs::remove_dir_all(&sockets);
        let mut watcher = Watcher::new(
            Some(projects),
            repo,
            TextLevel::Summary,
            Listen::In(sockets.clone()),
        );
        let snapshot = update(watcher.poll(false));
        let LiveStatus::Listening {
            socket,
            events: 0,
            last_ms: None,
            rejected: 0,
        } = snapshot.live.clone()
        else {
            panic!("listening, nothing heard yet: {:?}", snapshot.live);
        };
        assert!(only(snapshot).live.is_none());
        assert!(matches!(watcher.poll(false), ClaudeBgMessage::Unchanged));

        let event = |session: &str, rest: &str| {
            format!(r#"{{"v":1,"session_id":"{session}","ts":1793613602000,{rest}}}"#)
        };
        let start = r#""type":"tool.call","phase":"start","tool":"Bash","tool_use_id":"t9","summary":"cargo test""#;
        assert!(post(&socket, &event("ffff", start)).starts_with("HTTP/1.1 200"));
        // Another repository's session: counted, and no session made of it.
        assert!(post(&socket, &event("elsewhere", start)).starts_with("HTTP/1.1 200"));
        // Not an event: answered with an error and counted as rejected.
        assert!(!post(&socket, "nonsense").starts_with("HTTP/1.1 200"));

        let snapshot = update(watcher.poll(false));
        assert!(matches!(
            snapshot.live,
            LiveStatus::Listening {
                events: 2,
                last_ms: Some(_),
                rejected: 1,
                ..
            }
        ));
        assert_eq!(snapshot.sessions.len(), 1);
        let session = only(snapshot);
        let live = session
            .live
            .as_ref()
            .expect("the event reached the session");
        assert_eq!(live.running_tools[0].name, "Bash");

        // Not listening: no socket, and the page is told so.
        drop(watcher);
        assert!(!socket.exists(), "the socket is removed with the reader");
        let _ = fs::remove_dir_all(&sockets);
    }
}
