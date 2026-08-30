Output assets/demo-projects.gif

Require vig
Require gh

Set Shell "bash"
Set FontSize 16
Set Width 1200
Set Height 700
Set Theme "Catppuccin Frappe"
Set WindowBar Colorful
Set CursorBlink false
Set TypingSpeed 50ms

# --- Hidden setup: a temporary git repo whose origin is td72/vig ---
Hide

Type@1ms "cd $(mktemp -d)"
Enter
Sleep 300ms
Type@1ms "git init -q && git config user.email a@b.c && git config user.name demo"
Enter
Sleep 300ms
Type@1ms "echo 'hello' > README.md && git add -A && git commit -q -m 'init'"
Enter
Sleep 500ms
# The owner (td72) and the linked projects are resolved from this remote.
Type@1ms "git remote add origin https://github.com/td72/vig.git"
Enter
Sleep 300ms

# The recording runs against the real gh CLI, but a PATH shim restricts
# `gh project list` to the demo board (#2, linked to td72/vig) so no other
# project of the recording account appears. Everything else is forwarded
# untouched: the board, fields and issue bodies are the real thing.
Type@1ms "mkdir -p shim && cat > shim/gh << 'EOF'"
Enter
Type@1ms "#!/bin/sh"
Enter
Type@1ms "real=$(PATH=${PATH#*:} command -v gh)"
Enter
Type@1ms 'if [ "$1" = project ] && [ "$2" = list ]; then'
Enter
Type@1ms `  exec "$real" "$@" --jq '{projects: [.projects[] | select(.number == 2)], totalCount: 1}'`
Enter
Type@1ms "fi"
Enter
Type@1ms `exec "$real" "$@"`
Enter
Type@1ms "EOF"
Enter
Type@1ms "chmod +x shim/gh && export PATH=$PWD/shim:$PATH"
Enter
Sleep 300ms
# Drop the disk cache so the recording shows a fresh load.
Type@1ms "rm -rf ~/Library/Caches/vig/v1/td72/vig/projects ~/.cache/vig/v1/td72/vig/projects"
Enter
Type@1ms "clear"
Enter
Sleep 300ms

Show

# --- Demo: Projects View ---

Type "vig"
Enter
Sleep 2s

# Switch to the Projects View with '7': the project list and its board load
Type "7"
Sleep 6s

# Focus the board
Enter
Sleep 1.5s

# Move between the Status columns with l / h
Type "l"
Sleep 1.2s
Type "l"
Sleep 1.2s
Type "h"
Sleep 1s
Type "h"
Sleep 1s

# Move between the cards of a column with j / k (the detail follows)
Type "j"
Sleep 1.5s
Type "j"
Sleep 1.5s

# Table mode: one row per item, `s` cycles the sort column
Type "t"
Sleep 2.5s
Type "s"
Sleep 1.5s
Type "s"
Sleep 1.5s
Type "s"
Sleep 1.5s

# Back to the board
Type "t"
Sleep 1.5s

# Open the item detail: project fields, then the issue body
Enter
Sleep 2.5s
Type "j"
Sleep 500ms
Type "j"
Sleep 500ms
Type "j"
Sleep 1.5s

# Back to the board, then to the project list
Escape
Sleep 1.2s
Escape
Sleep 1.5s

# Quit
Type "q"
Sleep 500ms
