Output assets/demo-repo-config.gif

Require vig

Set Shell "bash"
Set FontSize 16
Set Width 1200
Set Height 900
Set Theme "Catppuccin Frappe"
Set WindowBar Colorful
Set CursorBlink false
Set TypingSpeed 50ms

# --- Hidden setup: isolated config/state dirs, a repo with an untracked .vig.kdl ---
Hide

Type@1ms "export XDG_CONFIG_HOME=$(mktemp -d) XDG_STATE_HOME=$(mktemp -d)"
Enter
Type@1ms "cd $(mktemp -d)"
Enter
Sleep 300ms
Type@1ms "git init -q && git config user.email a@b.c && git config user.name demo"
Enter
Type@1ms "printf 'pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n' > lib.rs"
Enter
Type@1ms "printf 'fn main() {}\n' > main.rs"
Enter
Type@1ms "git add -A && git commit -q -m 'initial commit'"
Enter
Type@1ms "printf 'pub fn sub(a: i32, b: i32) -> i32 {\n    a - b\n}\n' >> lib.rs"
Enter
Sleep 300ms

# The personal, untracked repo-local config: theme change + fewer pages
Type@1ms "cat > .vig.kdl << 'EOF'"
Enter
Type@1ms `theme "Solarized (dark)"`
Enter
Type@1ms `pages "git" "files" "worktrees"`
Enter
Type@1ms "EOF"
Enter
Sleep 200ms

Type@1ms "clear"
Enter
Sleep 300ms

Show

# --- Untracked .vig.kdl: loaded silently ---

Type "cat .vig.kdl"
Enter
Sleep 3s

Type "vig config path"
Enter
Sleep 4s

Type "clear"
Enter
Sleep 300ms

# Status bar shows "loaded .vig.kdl"; only the three configured tabs exist
Type "vig"
Enter
Sleep 4s
Type "q"
Sleep 500ms

# --- Hidden setup: a second repo that SHIPS a committed .vig.kdl ---
Hide

Type@1ms "cd $(mktemp -d)"
Enter
Sleep 300ms
Type@1ms "git init -q && git config user.email a@b.c && git config user.name demo"
Enter
Type@1ms "printf 'fn main() {}\n' > main.rs"
Enter
Type@1ms "cat > .vig.kdl << 'EOF'"
Enter
Type@1ms `theme "Solarized (dark)"`
Enter
Type@1ms `pages "git" "files" "worktrees"`
Enter
Type@1ms "EOF"
Enter
Type@1ms "git add -A && git commit -q -m 'ship a .vig.kdl'"
Enter
Sleep 300ms
Type@1ms "clear"
Enter
Sleep 300ms

Show

# --- Tracked .vig.kdl: the trust dialog appears before the app starts ---

Type "git ls-files"
Enter
Sleep 2.5s

Type "vig"
Enter
Sleep 3.5s

# [v] view the file, then decide
Type "v"
Sleep 3s
Escape
Sleep 1.5s

# [y] load and remember
Type "y"
Sleep 4s
Type "q"
Sleep 500ms

# The decision is remembered
Type "vig config trust"
Enter
Sleep 4s
