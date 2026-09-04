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
# The page shows the projects linked to this remote
# (`gh repo view --json projectsV2`): td72/vig has exactly the demo board
# (#2) linked, so the recording runs against the real gh CLI with no shims —
# the board, fields and issue bodies are the real thing.
Type@1ms "git remote add origin https://github.com/td72/vig.git"
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

# Switch to the Projects View with '7'. The project's saved view (a Table
# view on this board) drives the page: its visible fields are the columns.
Type "7"
Sleep 6s

# Move between the rows with j / k (the detail follows), `s` cycles the
# sort column
Type "j"
Sleep 1.5s
Type "j"
Sleep 1.5s
Type "s"
Sleep 1.5s
Type "s"
Sleep 1.5s

# `t` toggles the kanban for the same items
Type "t"
Sleep 2.5s

# Move between the Status columns with l / h, j / k moves within a column
Type "l"
Sleep 1.2s
Type "l"
Sleep 1.2s
Type "h"
Sleep 1s
Type "h"
Sleep 1s
Type "j"
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

# Back to the board
Escape
Sleep 1.5s

# Quit
Type "q"
Sleep 500ms
