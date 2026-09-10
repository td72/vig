Output assets/demo-projects.gif

Require vig

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
# (`gh repo view --json projectsV2`). The recording replays recorded gh
# output (VIG_GH_FIXTURE, set by the mise task) — the board, fields and
# issue bodies are the real thing, captured with `mise run fixtures:refresh`.
Type@1ms "git remote add origin https://github.com/td72/vig.git"
Enter
Sleep 300ms
# A local view for the page: defined in the config rather than on GitHub
# (`projects-view`), passed with --config below.
Type@1ms "cat > vig.kdl <<'K'"
Enter
Type@1ms 'projects-view "Open work" {'
Enter
Type@1ms '  layout "table"'
Enter
Type@1ms '  filter "-status:Done"'
Enter
Type@1ms '  fields "Title" "Status" "Target date"'
Enter
Type@1ms '  sort "Target date" "asc"'
Enter
Type@1ms "}"
Enter
Type@1ms "K"
Enter
Sleep 300ms
Type@1ms "clear"
Enter
Sleep 300ms

Show

# --- Demo: Projects View ---

Type "vig --config vig.kdl"
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

# `v` switches to the project's next saved view (a Board-layout view here)
Type "v"
Sleep 2.5s

# `x` hides the closed issues and merged PRs (`· closed hidden` in the
# header, the count in the status bar); `x` again brings them back
Type "x"
Sleep 2.5s
Type "x"
Sleep 1.5s

# `v` again: the Timeline view — a roadmap with a bar per item, a today
# marker, month labels and week ticks
Type "v"
Sleep 3s

# `-` zooms out to the month scale, `+` back in
Type "-"
Sleep 2s
Type "+"
Sleep 1.5s

# j / k move between the items
Type "j"
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

# `v` once more: a view defined in the config (`projects-view`), not on
# GitHub — a table of the open items sorted by target date, `(local)` in
# the header
Type "v"
Sleep 3.5s

# Quit
Type "q"
Sleep 500ms
