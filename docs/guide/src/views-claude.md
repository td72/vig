# Claude View

![claude demo](../../../assets/demo-claude.gif)

What Claude Code is doing in this repository, read from its transcripts. The
view lists **every session whose working directory is under the repository or
one of its worktrees** (subdirectories included), the most recently updated
first, and follows them as they work. Starting `claude`, `/clear` and
`--fork-session` begin a new session; `--continue` / `--resume`, compaction
and subagents stay in the same one.

Reading a session means reading its whole transcript, so not every session is
read: the three most recently updated are read and unfolded to begin with, and
any other when you open its row (`Enter`). A session that is read stays read
and is followed from then on.

The line above the panes describes the session the selection is in:

```text
Add retry with backoff to the fetcher · opus-5-5 · ● active · ctx ▓▓▓░░░░░ 33% · ~$1.75 · feat/retry-backoff
```

| Item | Meaning |
|---|---|
| title | The session's title (Claude Code's own, or the one you set) |
| model | The model of the latest response |
| state | With [live events](#live-events): `● working`, `● running Bash`, `▲ approve Bash?` (a tool call waits for approval), `○ waiting for input`. Without them: `● active` — the transcript was written to within the last minute; `○ quiet 5m00s` — how long since it was. `■ exited` — Claude Code exited. The transcript alone cannot tell a long-running tool from waiting for input |
| `ctx` | How full the context was at the latest response, against the model's window |
| cost | Claude Code's own total where it has written one; `~` marks a figure that contains a price-table estimate; `$—` when the model's price is unknown |
| branch | The branch the session runs on |

## The panes

- **Interactions** — the sessions, each with its latest interactions under
  it, the latest first. A session's row says when it was last written to,
  where it runs when that is not the repository's root (a subdirectory, or
  another worktree by its directory's name), its title, how many LLM requests
  and tool calls its interactions made and whether it is `active`, `quiet`
  or `exited`; one that is not read says `not read`. `Enter` on a session's
  row reads it, or folds / unfolds it (`Space` does the same from any row of
  the session). An interaction is a prompt and everything done to answer it.
  Its row says when it started, what was asked (the prompt's first line), how
  many LLM requests and tool calls its main thread made, how long it took and
  how it ended: `done` (Claude Code recorded its duration), `running` (no end
  yet, and the session is being written to) or `no end` (cut off, or not read
  to its end). `Enter` / `i` on an interaction goes into its tree. `t` lists
  the interactions of the sessions that are read all together, the latest
  first, and back. While the selection sits on the first interaction it stays
  on the latest one; move it and it stays where you are.
- **Trace** — the selected interaction as a tree, in the order things
  happened. An LLM request's row says what its response did
  (`thinking 12.0s → Read, Bash`; with `claude-text "full"`, the first line
  of what it said) and how it stopped (`tool_use`, `end_turn`; anything else
  is red). The tool calls it made hang under it: `✓` / `✗` (`·` while there
  is no result), the tool, what it ran, how long it took. A subagent hangs
  under the call that spawned it (`↳ Explore  <its task>`); `Space` shows or
  hides what it did. `context rebuilt` and `shape changed` mark a request
  that did not continue the one before it (as after a compaction) or whose
  system prompt or tools differ. On the right, each row is a bar on a time
  axis as long as the interaction (left out in a narrow pane).
- **Detail** — what is chosen. On a session's row: its id, where it runs,
  branch, model, state, context and cost, then what it used by model and by
  source (who asked: `repl_main_thread`, `agent:builtin:Explore`, …) — the
  requests, the estimated cost and the input / output / cache tokens of
  each, the most expensive first. A model the price table does not know
  reads `cost unknown`, never zero. On an interaction: how long it
  took and where the time went (LLM requests, tool calls, the
  rest), its subagents, tokens and estimated cost. With the focus in Trace:
  the row — a request's model, stop reason, tokens and cost; a tool call's
  whole command (every line of it) and, with `claude-text "full"`, what it
  returned; a subagent's task, state, tokens and cost. `j` / `k` scroll.
- **Files** — the files the selected session read (`R`), edited (`E`) or wrote (`W`),
  the latest touch first, with the strongest kind of touch, how many calls
  touched the file and how long ago. Paths are relative to the repository
  (`~/…` outside it).
  Only the `Read`, `Edit`, `MultiEdit`, `NotebookEdit` and `Write` tools
  count: what a shell command touched is not known.

A session keeps its latest 100 interactions (at most 500 spans on each
thread) and its latest 2000 tool calls, which the Files pane is built from.
An LLM request's duration is an approximation: the transcript records when
blocks were written, not when the API answered.

## How much text is shown

Transcripts hold your conversation, so how much of it reaches the screen is a
setting. [`claude-text`](config-reference.md#claude-text) sets the level:

| Level | Shown |
|---|---|
| `"summary"` (default) | The session title, the first line of each prompt, what each tool call ran (one line in the tree, the whole command in Detail), file paths and each subagent's task |
| `"title"` | Only the session title, tool names and file paths — for a shared screen |
| `"full"` | Also the first line of each response and the start of what each tool call returned (up to 4000 characters, with its stderr); `o` reads all of either |

Below `"full"`, responses and tool outputs are not read into the view at all.
At `"full"`, `o` on a tool call or a request reads the whole text from the
transcript (an output Claude Code moved to its own file included), shows it in
Detail, and drops it when something else is chosen; `o` again goes back to
the start. A tool's output is shown as it was printed: if a command wrote a
secret or the environment, it is on screen. `/` searches only what is drawn in
the lists, not the Detail pane.

## Key bindings

| Key | Action |
|-----|--------|
| `Tab` / `Shift+Tab` | Cycle panes: Interactions → Trace → Detail → Files |
| `j` / `k`, `gg` / `G`, `Ctrl+d` / `Ctrl+u` | Move selection; scroll in Detail |
| `Enter` / `i` (interactions) | On an interaction: into its tree. On a session: read it, fold / unfold it |
| `Space` (interactions) | Fold / unfold the session |
| `t` (interactions) | List the interactions by session / all together |
| `Space` (trace) | Show / hide a subagent's spans |
| `Enter` / `i` / `l` (trace) | Into the detail |
| `h` / `Esc` (trace, detail) | Back |
| `o` (trace, detail) | Read the whole output / response; again, its start (`claude-text "full"`) |
| `y` (files) | Copy the path |
| `/` `n` `N` | Search the focused pane |
| `r` | Start over: re-list the sessions and re-read the transcripts |

## Live events

The transcripts say what happened; what a session is doing *right now* comes
from the `iktinos-bridge` Claude Code mod, when it is installed:

```bash
claude plugin install iktinos-bridge --marketplace td72/iktinos
```

While the Claude view has been opened, vig listens on a Unix socket of its own
(`~/.local/state/iktinos/sockets/<pid>.sock`, removed when vig exits), and the
mod posts each event to every socket in that directory — so vig and the
standalone `iktinos` viewer hear the same events. vig only listens. The events
carry names, ids, numbers and a tool call's one-line summary, never a prompt,
a response or an output.

With them the view knows whether a session is working, running a tool, waiting
for approval or waiting for your input, how full its context is against the
window in effect, and its rate limits (on the session's row in Detail). The
end of the status bar says what the socket has heard, since an open socket
says nothing about a mod that sends:

| Status bar | Meaning |
|---|---|
| `live off` | Not listening ([`claude-live "off"`](config-reference.md#claude-live)) |
| `live failed` | The socket could not be opened |
| `live on, no events yet` | Listening, and nothing has arrived: the mod is not installed, is disabled, or no session has done anything since |
| `live 128 events, last 3s ago` | Receiving. Yellow when a session is busy and nothing has arrived for a minute |

An event is shown when it arrives, not at the next read of the transcripts.
The count is of every session on the machine; events of sessions of other
repositories, and of sessions that are not read, are counted and not used.
Without the mod the view works from the transcripts alone.

## Where the transcripts are

Claude Code keeps one JSON Lines file per session under
`~/.claude/projects/` (`$CLAUDE_CONFIG_DIR/projects` when that is set).
[`claude-projects-dir`](config-reference.md#claude-projects-dir) points the
view elsewhere. The files are read once a second while the view is shown —
only what was appended — and the directory is listed again every few seconds.
Whether a session belongs to the repository is told from its working
directory: below the repository, or in a worktree that shares its `.git`.
The reading is done by the `iktinos-core` crate of
[iktinos](https://github.com/td72/iktinos).

When the directory does not exist, or the repository has no session yet, the
view says so instead of showing empty panes.

## Constraints

- Read-only: nothing is written to Claude Code's directories and nothing is
  sent to Claude Code.
- Responses and tool outputs are read only with `claude-text "full"`. vig
  itself never reads the environment; what a tool printed is shown as it is.
- Without the `iktinos-bridge` mod the state is what the transcript says:
  whether the session waits for your input, which tool is running right now
  and the rate limits need [live events](#live-events).
- The one thing vig creates for this view is its own socket file, outside
  Claude Code's directories, removed on exit.
- Sessions of other repositories are not listed.
