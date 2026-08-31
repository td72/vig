# Recorded with VIG_PROCS_ROOT_PID set to the tape's own shell so the GIF only
# shows the synthetic processes started below (plus the shell and vig), never
# the recording machine's real processes, ports or paths. The hostname is not
# rendered by the Procs header at all. The System graphs pane shows machine
# totals (CPU% / memory) by design — numbers only, nothing identifying.

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

# --- Hidden setup: a repository plus a few synthetic processes to look at ---
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
# Only this shell's subtree is shown (see the comment at the top).
Type@1ms "export VIG_PROCS_ROOT_PID=$$"
Enter
Sleep 200ms
# Two leaf processes, one parent / child pair, and an HTTP server that
# listens on a port for the Ports pane to find.
Type@1ms "sleep 600 & sleep 600 &"
Enter
Sleep 200ms
Type@1ms "sh -c 'sleep 600; true' &"
Enter
Sleep 200ms
# Busy workers so the CPU charts have movement and climb into the yellow /
# red bands of the load gradient (one spinning core barely moves the global
# CPU % on a many-core machine, so start a handful).
Type@1ms "for i in 1 2 3 4 5 6; do sh -c 'while :; do :; done' & done"
Enter
Sleep 200ms
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

# Tab to the Ports pane: the server's port, owned by python3
Tab
Sleep 2s

# Enter jumps to the process that owns the port
Enter
Sleep 2.5s

# Select one of the busy workers, then focus the detail pane: its CPU% /
# RSS history area charts sit under the CPU and MEM fields, columns colored
# by load like the System pane (scroll down to reveal them)
Type "/do :"
Enter
Sleep 1.5s
Type "i"
Sleep 1.5s
Type "j"
Sleep 800ms
Type "j"
Sleep 1.5s
Escape
Sleep 1s

# The System pane on top graphs the machine totals as colored area charts;
# let a few more samples accumulate so the curves grow from the right
Sleep 10s

# `c` toggles the CPU graph to one bar per core (works from the list too)
Type "c"
Sleep 2.5s
Type "c"
Sleep 2s

# Help overlay
Type "?"
Sleep 2.5s
Escape
Sleep 800ms

# Quit
Type "q"
Sleep 500ms

# --- Hidden teardown: stop the synthetic processes ---
Hide
Type@1ms "kill $(jobs -p) 2>/dev/null; wait 2>/dev/null"
Enter
Sleep 300ms
