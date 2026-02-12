Output assets/demo-reflog.gif

Require vig

Set Shell "bash"
Set FontSize 16
Set Width 1200
Set Height 700
Set Theme "Catppuccin Frappe"
Set WindowBar Colorful
Set CursorBlink false
Set TypingSpeed 50ms

# --- Hidden setup: create a temporary git repo with reflog entries ---
Hide

Type@1ms "cd $(mktemp -d)"
Enter
Sleep 300ms
Type@1ms "git init -q && git config user.email a@b.c && git config user.name demo"
Enter
Sleep 300ms

# --- Create commits and branch switches to populate reflog ---

Type@1ms "echo 'first' > file.txt && git add -A && git commit -q -m 'initial commit'"
Enter
Sleep 500ms

Type@1ms "echo 'second' >> file.txt && git add -A && git commit -q -m 'add second line'"
Enter
Sleep 500ms

Type@1ms "git checkout -q -b feature/login"
Enter
Sleep 300ms

Type@1ms "echo 'login' > login.rs && git add -A && git commit -q -m 'add login module'"
Enter
Sleep 500ms

Type@1ms "git checkout -q main"
Enter
Sleep 300ms

Type@1ms "git checkout -q -b feature/signup"
Enter
Sleep 300ms

Type@1ms "echo 'signup' > signup.rs && git add -A && git commit -q -m 'add signup module'"
Enter
Sleep 500ms

Type@1ms "git checkout -q main"
Enter
Sleep 300ms

# Add working directory change so diff view has content
Type@1ms "echo 'modified' >> file.txt"
Enter
Sleep 200ms

Type@1ms "clear"
Enter
Sleep 300ms

Show

# --- Demo: reflog viewer ---

Type "vig"
Enter
Sleep 3s

# Navigate to Reflog pane with Tab (FileTree -> BranchList -> Reflog)
Tab
Sleep 1s
Tab
Sleep 1.5s

# Navigate reflog entries with j/k
Type "j"
Sleep 800ms
Type "j"
Sleep 800ms
Type "j"
Sleep 800ms

# Half page scroll
Ctrl+d
Sleep 1s

# Go to top
Type "g"
Sleep 1s

# Search reflog
Type "/checkout"
Enter
Sleep 2s

# Next match
Type "n"
Sleep 1.5s

# Clear search
Escape
Sleep 1s

# Select entry and set as diff base
Type "j"
Sleep 800ms
Enter
Sleep 2.5s

# Move to GitLog with i
Type "i"
Sleep 1.5s

# Back to Reflog with Esc
Escape
Sleep 1s

# Quit
Type "q"
Sleep 500ms
