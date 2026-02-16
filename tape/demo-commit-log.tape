Output assets/demo-commit-log.gif

Require vig

Set Shell "bash"
Set FontSize 16
Set Width 1200
Set Height 700
Set Theme "Catppuccin Frappe"
Set WindowBar Colorful
Set CursorBlink false
Set TypingSpeed 50ms

# --- Hidden setup: create a temporary git repo with merge history ---
Hide

Type@1ms "cd $(mktemp -d)"
Enter
Sleep 300ms
Type@1ms "git init -q && git config user.email a@b.c && git config user.name demo"
Enter
Sleep 300ms

# --- Initial commit on main ---

Type@1ms "cat > main.rs << 'EOF'"
Enter
Type@1ms "fn main() {"
Enter
Type@1ms `    println!("Hello, world!");`
Enter
Type@1ms "}"
Enter
Type@1ms "EOF"
Enter
Sleep 200ms

Type@1ms "git add -A && git commit -q -m 'Initial commit'"
Enter
Sleep 500ms

# --- Create feature branch ---

Type@1ms "git checkout -q -b feature/auth"
Enter
Sleep 300ms

Type@1ms "cat > auth.rs << 'EOF'"
Enter
Type@1ms "pub fn login(user: &str, pass: &str) -> bool {"
Enter
Type@1ms `    user == "admin" && pass == "secret"`
Enter
Type@1ms "}"
Enter
Type@1ms "EOF"
Enter
Sleep 200ms

Type@1ms "git add -A && git commit -q -m 'Add auth module'"
Enter
Sleep 500ms

Type@1ms "cat > auth.rs << 'EOF'"
Enter
Type@1ms "pub fn login(user: &str, pass: &str) -> bool {"
Enter
Type@1ms `    user == "admin" && pass == "secret"`
Enter
Type@1ms "}"
Enter
Type@1ms ""
Enter
Type@1ms "pub fn logout() {"
Enter
Type@1ms `    println!("Logged out");`
Enter
Type@1ms "}"
Enter
Type@1ms "EOF"
Enter
Sleep 200ms

Type@1ms "git add -A && git commit -q -m 'Add logout function'"
Enter
Sleep 500ms

# --- Back to main, add another commit ---

Type@1ms "git checkout -q main"
Enter
Sleep 300ms

Type@1ms "cat > config.toml << 'EOF'"
Enter
Type@1ms "[package]"
Enter
Type@1ms `name = "myapp"`
Enter
Type@1ms `version = "0.1.0"`
Enter
Type@1ms "EOF"
Enter
Sleep 200ms

Type@1ms "git add -A && git commit -q -m 'Add config file'"
Enter
Sleep 500ms

# --- Merge feature branch ---

Type@1ms "git merge -q --no-ff feature/auth -m 'Merge feature/auth into main'"
Enter
Sleep 500ms

# --- One more commit after merge ---

Type@1ms "cat > main.rs << 'EOF'"
Enter
Type@1ms "mod auth;"
Enter
Type@1ms ""
Enter
Type@1ms "fn main() {"
Enter
Type@1ms `    println!("Hello, world!");`
Enter
Type@1ms `    if auth::login("admin", "secret") {`
Enter
Type@1ms `        println!("Welcome!");`
Enter
Type@1ms "    }"
Enter
Type@1ms "}"
Enter
Type@1ms "EOF"
Enter
Sleep 200ms

Type@1ms "git add -A && git commit -q -m 'Use auth module in main'"
Enter
Sleep 500ms

# --- Working directory changes for diff view ---

Type@1ms "cat > main.rs << 'EOF'"
Enter
Type@1ms "mod auth;"
Enter
Type@1ms ""
Enter
Type@1ms "fn main() {"
Enter
Type@1ms `    println!("Hello, world!");`
Enter
Type@1ms `    if auth::login("admin", "secret") {`
Enter
Type@1ms `        println!("Welcome, admin!");`
Enter
Type@1ms "        auth::logout();"
Enter
Type@1ms "    }"
Enter
Type@1ms "}"
Enter
Type@1ms "EOF"
Enter
Sleep 200ms

Type@1ms "clear"
Enter
Sleep 300ms

Show

# --- Demo: commit log with branch graph and detail ---

Type "vig"
Enter
Sleep 3s

# Switch to BranchList
Tab
Sleep 1s

# Switch to Reflog
Tab
Sleep 1s

# Switch to GitLog pane (shows branch graph + detail)
Tab
Sleep 2s

# Navigate commits — watch pipe highlighting change
Type "j"
Sleep 1.5s

Type "j"
Sleep 1.5s

# Merge commit — pipes fork to feature branch
Type "j"
Sleep 2s

# Continue down
Type "j"
Sleep 1.5s

Type "j"
Sleep 1.5s

# Jump to top
Type "gg"
Sleep 1.5s

# Jump to bottom
Type "G"
Sleep 1.5s

# Back to top
Type "gg"
Sleep 1.5s

# Quit
Type "q"
Sleep 500ms
