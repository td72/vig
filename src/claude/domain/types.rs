//! Plain data for the Claude page.
//!
//! The session itself is `iktinos`' model (`Session`, with its trace of
//! interactions): the panes read it as it is. What is added here is what
//! the page derives from it (the touched files) and what it reads on demand
//! (the whole of one output or response). [`TextLevel`] decides how much of
//! the text is drawn, and whether tool outputs are kept at all.

use iktinos_core::session::Session;
use std::path::PathBuf;
use std::sync::Arc;

/// How much text the page shows (`claude-text`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TextLevel {
    /// The session title, tool names and file paths only.
    Title,
    /// Also the first line of each prompt, what a tool call was about
    /// (its command, path or pattern) and a subagent's task. The default.
    Summary,
    /// Also what a response said and what a tool call returned.
    Full,
}

impl TextLevel {
    pub const NAMES: [&'static str; 3] = ["title", "summary", "full"];

    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "title" => Some(Self::Title),
            "summary" => Some(Self::Summary),
            "full" => Some(Self::Full),
            _ => None,
        }
    }

    /// Whether prompts, tool summaries and subagent tasks are drawn.
    pub fn shows_summaries(self) -> bool {
        self >= Self::Summary
    }

    /// Whether responses and tool outputs are read and drawn.
    pub fn shows_output(self) -> bool {
        self >= Self::Full
    }
}

/// How a session touched a file, weakest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Touch {
    Read,
    Edit,
    Write,
}

impl Touch {
    /// The tool that touches a file this way, if `tool` is one.
    pub fn of_tool(tool: &str) -> Option<Self> {
        match tool {
            "Read" => Some(Self::Read),
            "Edit" | "MultiEdit" | "NotebookEdit" => Some(Self::Edit),
            "Write" => Some(Self::Write),
            _ => None,
        }
    }

    pub fn letter(self) -> &'static str {
        match self {
            Self::Read => "R",
            Self::Edit => "E",
            Self::Write => "W",
        }
    }
}

/// A file the session read, edited or wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TouchedFile {
    /// Relative to the repository when the file is inside it.
    pub path: String,
    /// The strongest touch.
    pub touch: Touch,
    /// How many tool calls touched it.
    pub count: usize,
    pub last_ms: Option<i64>,
}

/// One of the repository's sessions, as the page lists it.
#[derive(Debug, Clone)]
pub struct SessionEntry {
    pub id: String,
    /// Where it runs, relative to the repository (`.` for its root) or as
    /// a path outside it.
    pub place: String,
    /// When its files were last written to.
    pub modified_ms: Option<i64>,
    /// The session, once it was read: the latest few are read to begin
    /// with, the others when they are opened.
    pub session: Option<Arc<Session>>,
    /// The files it touched, latest touch first. Empty until it is read.
    pub files: Vec<TouchedFile>,
}

/// What the background reader hands the page: the repository's sessions,
/// the most recently updated first, and what the live socket has heard.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub sessions: Vec<SessionEntry>,
    pub live: LiveStatus,
}

/// Whether the page listens for live events from the `iktinos-bridge` mod,
/// and what it has heard. An open socket alone says nothing about a mod
/// that sends, so the counts are shown.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum LiveStatus {
    /// Not listening (`claude-live "off"`).
    #[default]
    Off,
    /// The socket could not be opened.
    Failed(String),
    Listening {
        socket: PathBuf,
        /// Events received, of any session on the machine.
        events: u64,
        /// When the latest one arrived.
        last_ms: Option<i64>,
        /// Requests that were not events this build reads.
        rejected: u64,
    },
}

/// What is read whole on demand.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum WholeKey {
    /// A tool call's output, by its `tool_use` id.
    Output(String),
    /// A response's text, by its `message.id`.
    Response(String),
}

/// All of one output or response, as read from the transcript. Held only
/// while it is shown.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct WholeText {
    pub text: String,
    pub stderr: Option<String>,
    /// Longer than the reader's limit: cut.
    pub truncated: bool,
    pub interrupted: bool,
    /// Blocks that are not text (images, …).
    pub other_blocks: usize,
    /// The file the output was moved to, when it is not in the transcript.
    pub file: Option<PathBuf>,
}

/// `Debug` says how much there is, not what: an output is the user's data
/// and must not reach a log through a stray `{:?}`.
impl std::fmt::Debug for WholeText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WholeText")
            .field("chars", &self.text.chars().count())
            .field(
                "stderr_chars",
                &self.stderr.as_ref().map(|s| s.chars().count()),
            )
            .field("truncated", &self.truncated)
            .finish_non_exhaustive()
    }
}

/// The answer to a request for a whole text. `Ok(None)`: the session's
/// files hold none.
#[derive(Clone)]
pub struct Whole {
    pub key: WholeKey,
    pub content: Result<Option<WholeText>, String>,
}
