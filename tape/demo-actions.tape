Output assets/demo-actions.gif

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

# --- Hidden setup: create a temporary git repo linked to a GitHub repo ---
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
# Set remote to vig repo for gh commands (it has real workflow runs)
Type@1ms "git remote add origin https://github.com/td72/vig.git"
Enter
Sleep 300ms
Type@1ms "clear"
Enter
Sleep 300ms

Show

# --- Demo: Actions View — runs, jobs, log ---

Type "vig"
Enter
Sleep 1.5s

# Switch to the Actions view: latest workflow runs
Type "6"
Sleep 4s

# Move down: the jobs pane follows the selected run
Type "j"
Sleep 2s
Type "j"
Sleep 2.5s

# Focus the jobs: jobs with their steps nested underneath
Enter
Sleep 2s
Type "j"
Sleep 800ms
Type "j"
Sleep 800ms
Type "j"
Sleep 1s

# Show this job's log (a step row scrolls to that step)
Enter
Sleep 4s

# Scroll the log
Type "j"
Sleep 400ms
Type "j"
Sleep 400ms
Type "j"
Sleep 1.5s
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

# Back to the jobs, then to the runs
Type "h"
Sleep 1.5s
Type "h"
Sleep 1.5s

# Search the runs by branch / workflow
Type "/release"
Enter
Sleep 2.5s
Escape
Sleep 1s

# Quit
Type "q"
Sleep 500ms
