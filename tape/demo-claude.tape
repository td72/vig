# Recorded from synthetic transcripts: tape/claude-fixture.py (its path is
# VIG_CLAUDE_FIXTURE, set by the mise task) writes an invented session for
# the demo repository into a scratch projects directory, and
# VIG_CLAUDE_PROJECTS_DIR points vig at it; VIG_CLAUDE_SOCKET_DIR does the same
# for the live socket. No real Claude Code session is read, heard or shown.
# The script keeps appending tool calls for a few seconds, each announced as
# a live event, so the page is seen following a session that is still working.

Output assets/demo-claude.gif

Require vig
Require python3

Set Shell "bash"
Set FontFamily "Hack Nerd Font Mono"
Set FontSize 16
Set Width 1200
Set Height 700
Set Theme "Catppuccin Frappe"
Set WindowBar Colorful
Set CursorBlink false
Set TypingSpeed 50ms

# --- Hidden setup: a repository and a synthetic session working in it ---
Hide

Type@1ms "cd $(mktemp -d) && mkdir repo && cd repo"
Enter
Sleep 300ms
# Ignore any user config so the recording shows the defaults.
Type@1ms `echo 'theme "base16-eighties.dark"' > ../vig-demo.kdl && export VIG_CONFIG=$PWD/../vig-demo.kdl`
Enter
Sleep 300ms
Type@1ms "git init -q -b main && git config user.email a@b.c && git config user.name demo"
Enter
Sleep 300ms
Type@1ms "echo demo > README.md && git add -A && git commit -q -m 'initial commit'"
Enter
Sleep 300ms
Type@1ms "export VIG_CLAUDE_PROJECTS_DIR=$PWD/../projects"
Enter
Sleep 200ms
# The live socket in a scratch directory too: the recording hears the
# fixture's synthetic events, not the sessions of this machine.
Type@1ms "export VIG_CLAUDE_SOCKET_DIR=$PWD/../s"
Enter
Sleep 200ms
# The session: written now, then appended to while vig is watching.
Type@1ms "(python3 $VIG_CLAUDE_FIXTURE $VIG_CLAUDE_PROJECTS_DIR $PWD live &) 2>/dev/null"
Enter
Sleep 1s

Type@1ms "clear"
Enter
Sleep 300ms

Show

# --- Demo: the Claude view ---

Type "vig"
Enter
Sleep 1.5s

# Switch to the Claude view (8): the repository's sessions, each with its
# interactions under it; the latest interaction's tree is under the list
Type "8"
Sleep 3s

# New tool calls appear in the tree as the session works
Sleep 9s

# Down to the first interaction: the one that used subagents
Type "j"
Sleep 1s
Type "j"
Sleep 2s

# Into its tree: LLM requests, the tool calls each made, a time axis
Enter
Sleep 1s
Type "j"
Sleep 400ms
Type "j"
Sleep 400ms
Type "j"
Sleep 400ms
Type "j"
Sleep 400ms
Type "j"
Sleep 1.5s

# The subagent the call spawned: unfold it to see what it did
Type "j"
Sleep 1s
Space
Sleep 2.5s
Type "j"
Sleep 400ms
Type "j"
Sleep 400ms
Type "j"
Sleep 2s

# Search the tree
Type "/tests"
Enter
Sleep 2s
Escape
Sleep 1s

# Back to the interactions; an earlier session of the repository is
# listed too: its row describes the session, Space folds it
Escape
Sleep 1s
Type "j"
Sleep 2s
Space
Sleep 1.5s

# Tab to the files the session touched
Tab
Tab
Tab
Sleep 1.5s
Type "j"
Sleep 500ms
Type "j"
Sleep 1.5s

# Help
Type "?"
Sleep 2.5s
Escape
Sleep 500ms

# Quit
Type "q"
Sleep 500ms
