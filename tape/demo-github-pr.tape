Output assets/demo-github-pr.gif

Require vig

Set Shell "bash"
Set FontSize 16
Set Width 1200
Set Height 700
Set Theme "Catppuccin Frappe"
Set WindowBar Colorful
Set CursorBlink false
Set TypingSpeed 50ms

# --- Hidden setup: create a temporary git repo linked to a GitHub repo ---
Hide

Type@1ms "mkdir -p $(mktemp -d)/vig && cd $_"
Enter
Sleep 300ms
Type@1ms "git init -q && git config user.email a@b.c && git config user.name demo"
Enter
Sleep 300ms
Type@1ms "echo 'hello' > README.md && git add -A && git commit -q -m 'init'"
Enter
Sleep 500ms
# Set remote to vig repo for gh commands
Type@1ms "git remote add origin https://github.com/td72/vig.git"
Enter
Sleep 300ms
Type@1ms "clear"
Enter
Sleep 300ms

Show

# --- Demo: GitHub View — Pull Requests and Workflow Runs ---

Type "vig"
Enter
Sleep 3s

# Switch to GitHub View with '2'
Type "2"
Sleep 4s

# Include merged / closed pull requests too (x)
Type "x"
Sleep 2s

# Move to PR list with l
Type "l"
Sleep 1.5s

# Navigate PR list
Type "j"
Sleep 800ms
Type "j"
Sleep 800ms

# Open PR detail
Enter
Sleep 3s

# Scroll body (markdown rendered)
Type "j"
Sleep 500ms
Type "j"
Sleep 500ms
Ctrl+d
Sleep 1.5s

# Show Status (Checks) pane
Type "l"
Sleep 2s

# Navigate checks with j
Type "j"
Sleep 800ms
Type "j"
Sleep 800ms

# Cycle to Reviews pane with Tab
Tab
Sleep 2s

# Cycle to Comments pane with Tab
Tab
Sleep 2s

# Navigate comments
Type "j"
Sleep 800ms

# Back to body pane with h
Type "h"
Sleep 1.5s

# Show the PR's diff: changed files + side-by-side diff in a window
Type "d"
Sleep 3s

# Walk the changed files; the diff follows
Type "j"
Sleep 1s
Type "j"
Sleep 1.5s

# Search the file tree by path
Type "/loader"
Enter
Sleep 2s

# Into the diff: scroll, then search it
Type "i"
Sleep 1s
Ctrl+d
Sleep 1s
Type "/fn "
Enter
Sleep 2s
Type "n"
Sleep 1.5s
Escape
Sleep 800ms

# Esc steps back to the file tree, then closes the window
Escape
Sleep 1s
Escape
Sleep 1.5s

# W would check the PR out as a worktree next to the repo: it asks first
Type "W"
Sleep 3s
Type "n"
Sleep 1s

# Back to list
Escape
Sleep 1s

# Move to the Workflow Runs column with l
Type "l"
Sleep 2s

# Navigate runs: the detail follows the selected run
Type "j"
Sleep 2s

# Open the run: its jobs with their steps nested underneath
Enter
Sleep 2s
Type "j"
Sleep 800ms
Type "j"
Sleep 1s

# Show this job's log in the Log sub-pane
Enter
Sleep 4s

# Scroll the log
Type "j"
Sleep 400ms
Type "j"
Sleep 400ms
Ctrl+d
Sleep 1.5s

# Search the log
Type "/cargo"
Enter
Sleep 2s
Type "n"
Sleep 1.5s
Escape
Sleep 800ms

# Back to the jobs with h, then to the runs
Type "h"
Sleep 1.5s
Escape
Sleep 1s

# Search the runs by workflow / branch
Type "/release"
Enter
Sleep 2.5s
Escape
Sleep 1s

# Switch back to Git View with '1'
Type "1"
Sleep 2s

# Quit
Type "q"
Sleep 500ms
