Output assets/demo-procs.gif

Require vig

Set Shell "bash"
Set FontFamily "Hack Nerd Font Mono"
Set FontSize 16
Set Width 1200
Set Height 700
Set Theme "Catppuccin Frappe"
Set WindowBar Colorful
Set CursorBlink false
Set TypingSpeed 50ms

# --- Hidden setup: a repository plus a small server that listens on a port ---
Hide

Type@1ms "cd $(mktemp -d)"
Enter
Sleep 300ms
Type@1ms "git init -q && git config user.email a@b.c && git config user.name demo"
Enter
Sleep 300ms
Type@1ms "echo demo > README.md && git add -A && git commit -q -m 'initial commit'"
Enter
Sleep 300ms
# Something for the Ports pane to find: an HTTP server owned by this shell.
Type@1ms "python3 -m http.server 8765 >/dev/null 2>&1 &"
Enter
Sleep 1s

Type@1ms "clear"
Enter
Sleep 300ms

Show

# --- Demo: the Procs view ---

Type "vig"
Enter
Sleep 2s

# Switch to the Procs view: the process tree, listening ports and a detail
Type "5"
Sleep 3s

# Move down the tree; the detail pane follows the selection
Type "j"
Sleep 1s
Type "j"
Sleep 1s
Type "j"
Sleep 1.5s

# `s` cycles the sort: CPU -> MEM -> PID (shown in the pane title)
Type "s"
Sleep 2s
Type "s"
Sleep 2s

# Search the command lines
Type "/http.server"
Enter
Sleep 2.5s

# Tab to the Ports pane and find the server's port
Tab
Sleep 1.5s
Type "/8765"
Enter
Sleep 2s

# Enter jumps to the process that owns the port
Enter
Sleep 2.5s

# Focus the detail pane and scroll it, then back to the list
Type "i"
Sleep 1.5s
Type "j"
Sleep 800ms
Type "j"
Sleep 1.2s
Escape
Sleep 1s

# Help overlay
Type "?"
Sleep 2.5s
Escape
Sleep 800ms

# Quit
Type "q"
Sleep 500ms

# --- Hidden teardown: stop the server ---
Hide
Type@1ms "kill %1"
Enter
Sleep 300ms
