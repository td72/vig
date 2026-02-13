Output assets/demo-github.gif

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
# Set remote to vig repo for gh commands
Type@1ms "git remote add origin https://github.com/td72/vig.git"
Enter
Sleep 300ms
Type@1ms "clear"
Enter
Sleep 300ms

Show

# --- Demo: GitHub View ---

Type "vig"
Enter
Sleep 3s

# Switch to GitHub View with '2'
Type "2"
Sleep 4s

# Navigate issue list with j/k
Type "j"
Sleep 800ms
Type "j"
Sleep 800ms

# Open issue detail
Enter
Sleep 3s

# Scroll detail with j
Type "j"
Sleep 500ms
Type "j"
Sleep 500ms
Type "j"
Sleep 500ms

# Back to list
Escape
Sleep 1.5s

# Move to PR list with l
Type "l"
Sleep 1.5s

# Navigate PR list
Type "j"
Sleep 800ms

# Open PR detail
Enter
Sleep 3s

# Scroll through PR detail
Ctrl+d
Sleep 1.5s

# Back to list
Escape
Sleep 1s

# Switch back to Git View with '1'
Type "1"
Sleep 2s

# Quit
Type "q"
Sleep 500ms
