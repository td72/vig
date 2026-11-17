#!/usr/bin/env python3
"""Write a synthetic Claude Code session for the Claude page's demo tape.

    claude-fixture.py <projects-dir> <repo-dir> [live]

Everything here is invented: the record shapes follow Claude Code's
transcripts, no value comes from a real session. The session's `cwd` is
<repo-dir>, so vig started there picks it up, and its timestamps count
back from now. With `live`, a few more tool calls are appended afterwards,
one every couple of seconds, so the recording shows the page following a
session that is still working; each is also announced as a live event, the
way the iktinos-bridge mod would, to the sockets in VIG_CLAUDE_SOCKET_DIR.
"""

import json
import os
import socket
import sys
import time
from datetime import datetime, timedelta, timezone
from pathlib import Path

SESSION = "5e55104e-0000-4000-8000-00000000d3e0"
EARLIER = "ea411e40-0000-4000-8000-00000000d3e0"
EXPLORE = "a1b2c3d4e5f60718"
NESTED = "b2c3d4e5f6071829"
MAIN_MODEL = "claude-opus-5-5"
AGENT_MODEL = "claude-haiku-5-5"


def post(session: str, kind: str, **fields):
    """Send one live event the way the iktinos-bridge mod does: to every
    socket in the directory vig listens in (VIG_CLAUDE_SOCKET_DIR)."""
    directory = os.environ.get("VIG_CLAUDE_SOCKET_DIR")
    if not directory:
        return
    event = {"v": 1, "type": kind, "session_id": session, "ts": int(time.time() * 1000)}
    event.update(fields)
    body = json.dumps(event).encode()
    head = f"POST /events HTTP/1.1\r\nHost: iktinos\r\nContent-Length: {len(body)}\r\n\r\n"
    for path in Path(directory).glob("*.sock"):
        try:
            with socket.socket(socket.AF_UNIX) as client:
                client.settimeout(1)
                client.connect(str(path))
                client.sendall(head.encode() + body)
                client.recv(64)
        except OSError:
            pass


class Writer:
    def __init__(self, projects: Path, repo: Path, session: str = SESSION, minutes: float = 14):
        self.repo = repo
        self.session = session
        project = projects / "-demo-repo"
        self.main = project / f"{session}.jsonl"
        self.agents = project / session / "subagents"
        self.agents.mkdir(parents=True, exist_ok=True)
        self.start = datetime.now(timezone.utc) - timedelta(minutes=minutes)
        self.n = 0
        self.context = 180_000

    def at(self, seconds: float) -> str:
        stamp = self.start + timedelta(seconds=seconds)
        return stamp.strftime("%Y-%m-%dT%H:%M:%S.") + f"{stamp.microsecond // 1000:03d}Z"

    def uid(self) -> str:
        self.n += 1
        return f"rec-{self.n:04d}"

    def write(self, record: dict, agent: str | None = None):
        record.setdefault("sessionId", self.session)
        record.setdefault("cwd", str(self.repo))
        record.setdefault("gitBranch", "feat/retry-backoff")
        record.setdefault("version", "2.1.325")
        path = self.main
        if agent:
            record["agentId"] = agent
            record["isSidechain"] = True
            path = self.agents / f"agent-{agent}.jsonl"
        with path.open("a") as out:
            out.write(json.dumps(record) + "\n")

    def prompt(self, seconds: float, text: str, agent: str | None = None):
        self.write(
            {
                "type": "user",
                "uuid": self.uid(),
                "timestamp": self.at(seconds),
                "message": {"role": "user", "content": text},
            },
            agent,
        )

    def call(
        self,
        seconds: float,
        took: float | None,
        name: str,
        tool_input: dict,
        ok: bool = True,
        agent: str | None = None,
    ) -> str:
        """One tool call and, unless `took` is None, its result."""
        tool_id = f"toolu_{self.uid()}"
        if agent is None:
            self.context += 9_000
        self.write(
            {
                "type": "assistant",
                "uuid": self.uid(),
                "timestamp": self.at(seconds),
                "message": {
                    "id": f"msg_{self.uid()}",
                    "model": AGENT_MODEL if agent else MAIN_MODEL,
                    "role": "assistant",
                    "content": [
                        {
                            "type": "tool_use",
                            "id": tool_id,
                            "name": name,
                            "input": tool_input,
                        }
                    ],
                    "stop_reason": "tool_use",
                    "usage": {
                        "input_tokens": 40,
                        "output_tokens": 600,
                        "cache_creation_input_tokens": 9_000,
                        "cache_read_input_tokens": 20_000 if agent else self.context,
                    },
                },
            },
            agent,
        )
        if took is not None:
            self.write(
                {
                    "type": "user",
                    "uuid": self.uid(),
                    "timestamp": self.at(seconds + took),
                    "message": {
                        "role": "user",
                        "content": [
                            {
                                "type": "tool_result",
                                "tool_use_id": tool_id,
                                "content": "SYNTHETIC tool output",
                                "is_error": not ok,
                            }
                        ],
                    },
                },
                agent,
            )
        return tool_id

    def say(self, seconds: float, text: str, agent: str | None = None):
        """A response that only says something: the end of a turn."""
        self.write(
            {
                "type": "assistant",
                "uuid": self.uid(),
                "timestamp": self.at(seconds),
                "message": {
                    "id": f"msg_{self.uid()}",
                    "model": AGENT_MODEL if agent else MAIN_MODEL,
                    "role": "assistant",
                    "content": [{"type": "text", "text": text}],
                    "stop_reason": "end_turn",
                    "usage": {
                        "input_tokens": 40,
                        "output_tokens": 300,
                        "cache_creation_input_tokens": 2_000,
                        "cache_read_input_tokens": 20_000 if agent else self.context,
                    },
                },
            },
            agent,
        )

    def end(self, seconds: float, took: float):
        """Claude Code's own record of how long the turn took."""
        self.write(
            {
                "type": "system",
                "subtype": "turn_duration",
                "durationMs": int(took * 1000),
                "uuid": self.uid(),
                "timestamp": self.at(seconds),
            }
        )

    def meta(self, agent: str, kind: str, task: str, spawned_by: str, depth: int, done: bool):
        meta = {
            "agentType": kind,
            "description": task,
            "toolUseId": spawned_by,
            "spawnDepth": depth,
        }
        if done:
            meta["finished"] = True
        (self.agents / f"agent-{agent}.meta.json").write_text(json.dumps(meta))

    def path(self, relative: str) -> str:
        return str(self.repo / relative)


def main():
    projects, repo = Path(sys.argv[1]), Path(sys.argv[2]).resolve()
    live = len(sys.argv) > 3 and sys.argv[3] == "live"

    # An earlier session of the same repository, finished an hour ago.
    old = Writer(projects, repo, EARLIER, minutes=75)
    old.write({"type": "ai-title", "aiTitle": "Bump the dependencies"})
    old.prompt(0, "Update the dependencies and see what breaks")
    old.call(5, 21.4, "Bash", {"command": "cargo update"})
    old.call(40, 0.3, "Read", {"file_path": old.path("Cargo.toml")})
    old.call(52, 58.2, "Bash", {"command": "cargo test --workspace"})
    old.say(115, "SYNTHETIC: everything is up to date and the tests pass.")
    old.end(116, 116)
    stamp = time.time() - 73 * 60
    os.utime(old.main, (stamp, stamp))

    w = Writer(projects, repo)

    w.write({"type": "ai-title", "aiTitle": "Add retry with backoff to the fetcher"})

    # Turn 1: look around, with a subagent that spawns one of its own.
    w.prompt(0, "Where does fetch() get called, and is there any retry today?")
    w.call(4, 0.3, "Read", {"file_path": w.path("src/fetch.rs")})
    w.call(9, 0.2, "Read", {"file_path": w.path("Cargo.toml")})
    explore = w.call(
        15, 96, "Agent", {"description": "Find every caller of fetch()", "prompt": "SYNTHETIC"}
    )
    w.meta(EXPLORE, "Explore", "Find every caller of fetch()", explore, 1, True)
    w.prompt(16, "SYNTHETIC subagent prompt", EXPLORE)
    w.call(18, 0.4, "Grep", {"pattern": "fetch\\(", "path": str(repo)}, agent=EXPLORE)
    w.call(24, 0.2, "Read", {"file_path": w.path("src/main.rs")}, agent=EXPLORE)
    nested = w.call(
        30, 41, "Agent", {"description": "Check the tests for retries", "prompt": "SYNTHETIC"},
        agent=EXPLORE,
    )
    w.meta(NESTED, "general-purpose", "Check the tests for retries", nested, 2, True)
    w.prompt(31, "SYNTHETIC subagent prompt", NESTED)
    w.call(33, 0.3, "Glob", {"pattern": "tests/**/*.rs"}, agent=NESTED)
    w.call(38, 0.2, "Read", {"file_path": w.path("tests/fetch.rs")}, agent=NESTED)
    w.say(72, "SYNTHETIC: the tests never retry.", agent=NESTED)
    w.call(80, 0.3, "Read", {"file_path": w.path("src/util.rs")}, agent=EXPLORE)
    w.say(110, "SYNTHETIC: fetch() has two callers and no retry.", agent=EXPLORE)
    w.say(118, "SYNTHETIC: fetch() is called from main.rs and util.rs; nothing retries.")
    w.end(119, 119)

    # Turn 2: the change itself, with one failing test run.
    w.prompt(180, "Add retry with exponential backoff to the fetcher")
    w.call(186, 0.4, "Edit", {"file_path": w.path("src/fetch.rs")})
    w.call(204, 0.3, "Write", {"file_path": w.path("src/backoff.rs")})
    w.call(221, 0.3, "Edit", {"file_path": w.path("src/main.rs")})
    w.call(240, 38.4, "Bash", {"command": "cargo test --workspace"}, ok=False)
    w.call(300, 0.4, "Edit", {"file_path": w.path("src/backoff.rs")})
    w.call(318, 41.7, "Bash", {"command": "cargo test --workspace"})
    w.say(366, "SYNTHETIC: retries with exponential backoff are in; the tests pass.")
    w.end(367, 187)

    # Turn 3: polish; still going when the recording starts.
    w.prompt(14 * 60 - 55, "Document the backoff and run clippy")
    w.write({"type": "last-prompt", "lastPrompt": "Document the backoff and run clippy"})
    w.call(14 * 60 - 48, 0.3, "Edit", {"file_path": w.path("README.md")})
    w.call(14 * 60 - 30, 12.6, "Bash", {"command": "cargo clippy --all-targets -- -D warnings"})

    if not live:
        return
    # Keep working while vig watches.
    base = 14 * 60
    steps = [
        ("Edit", {"file_path": w.path("src/backoff.rs")}, 0.4),
        ("Bash", {"command": "cargo fmt --all --check"}, 1.2),
        ("Read", {"file_path": w.path("tests/fetch.rs")}, 0.2),
        ("Edit", {"file_path": w.path("tests/fetch.rs")}, 0.3),
        ("Bash", {"command": "cargo test backoff"}, 2.1),
    ]
    time.sleep(7)
    # What the mod would report: the turn in progress, the measurements,
    # and each tool call as it starts and ends.
    post(SESSION, "turn.start", turn_id="turn-3")
    post(
        SESSION,
        "session.measure",
        context={"tokens": w.context, "window": 1_000_000},
        rate_limits=[{"kind": "five_hour", "percent_used": 42.5}],
    )
    for n, (name, tool_input, took) in enumerate(steps):
        summary = tool_input.get("command") or Path(tool_input["file_path"]).name
        post(SESSION, "tool.call", phase="start", tool=name, tool_use_id=f"live-{n}", summary=summary)
        time.sleep(took)
        elapsed = (datetime.now(timezone.utc) - w.start).total_seconds()
        w.call(max(elapsed - took, base), took, name, tool_input)
        post(
            SESSION,
            "tool.call",
            phase="end",
            tool=name,
            tool_use_id=f"live-{n}",
            duration_ms=int(took * 1000),
            success=True,
        )
        time.sleep(max(2.5 - took, 0.2))


if __name__ == "__main__":
    main()
