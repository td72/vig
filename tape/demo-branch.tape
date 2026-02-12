Output assets/demo-branch.gif

Require vig

Set Shell "bash"
Set FontSize 16
Set Width 1200
Set Height 700
Set Theme "Catppuccin Frappe"
Set WindowBar Colorful
Set CursorBlink false
Set TypingSpeed 50ms

# --- Hidden setup: create a temporary git repo with multiple branches ---
Hide

Type@1ms "cd $(mktemp -d)"
Enter
Sleep 300ms
Type@1ms "git init -q && git config user.email a@b.c && git config user.name demo"
Enter
Sleep 300ms

# --- Initial files on main ---

Type@1ms "cat > main.rs << 'EOF'"
Enter
Type@1ms "fn main() {"
Enter
Type@1ms `    println!("Hello, world!");`
Enter
Type@1ms "}"
Enter
Type@1ms ""
Enter
Type@1ms "fn add(a: i32, b: i32) -> i32 {"
Enter
Type@1ms "    a + b"
Enter
Type@1ms "}"
Enter
Type@1ms "EOF"
Enter
Sleep 200ms

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

Type@1ms "git add -A && git commit -q -m 'initial commit'"
Enter
Sleep 500ms

# --- Second commit on main ---

Type@1ms "cat > utils.rs << 'EOF'"
Enter
Type@1ms "pub fn greet(name: &str) -> String {"
Enter
Type@1ms `    format!("Hello, {}!", name)`
Enter
Type@1ms "}"
Enter
Type@1ms "EOF"
Enter
Sleep 200ms

Type@1ms "git add -A && git commit -q -m 'add utils module'"
Enter
Sleep 500ms

# --- Create feature branch with changes ---

Type@1ms "git checkout -q -b feature/login"
Enter
Sleep 300ms

Type@1ms "cat > main.rs << 'EOF'"
Enter
Type@1ms "fn main() {"
Enter
Type@1ms `    println!("Hello, world!");`
Enter
Type@1ms "    let sum = add(2, 3);"
Enter
Type@1ms `    println!("Sum: {}", sum);`
Enter
Type@1ms "}"
Enter
Type@1ms ""
Enter
Type@1ms "fn add(a: i32, b: i32) -> i32 {"
Enter
Type@1ms "    a + b"
Enter
Type@1ms "}"
Enter
Type@1ms ""
Enter
Type@1ms "fn multiply(a: i32, b: i32) -> i32 {"
Enter
Type@1ms "    a * b"
Enter
Type@1ms "}"
Enter
Type@1ms "EOF"
Enter
Sleep 200ms

Type@1ms "git add -A && git commit -q -m 'add multiply and use add in main'"
Enter
Sleep 500ms

Type@1ms "cat > utils.rs << 'EOF'"
Enter
Type@1ms "pub fn greet(name: &str) -> String {"
Enter
Type@1ms `    format!("Welcome, {}!", name)`
Enter
Type@1ms "}"
Enter
Type@1ms ""
Enter
Type@1ms "pub fn farewell(name: &str) -> String {"
Enter
Type@1ms `    format!("Goodbye, {}!", name)`
Enter
Type@1ms "}"
Enter
Type@1ms "EOF"
Enter
Sleep 200ms

Type@1ms "git add -A && git commit -q -m 'update greet and add farewell'"
Enter
Sleep 500ms

# --- Create another feature branch ---

Type@1ms "git checkout -q main"
Enter
Sleep 300ms

Type@1ms "git checkout -q -b feature/signup"
Enter
Sleep 300ms
Type@1ms "git commit -q --allow-empty -m 'scaffold signup page'"
Enter
Sleep 500ms

# --- Back to main, create working directory changes ---

Type@1ms "git checkout -q main"
Enter
Sleep 300ms

Type@1ms "cat > main.rs << 'EOF'"
Enter
Type@1ms "use std::env;"
Enter
Type@1ms ""
Enter
Type@1ms "fn main() {"
Enter
Type@1ms "    let args: Vec<String> = env::args().collect();"
Enter
Type@1ms `    let name = args.get(1).map(|s| s.as_str()).unwrap_or("world");`
Enter
Type@1ms `    println!("Hello, {}!", name);`
Enter
Type@1ms "}"
Enter
Type@1ms ""
Enter
Type@1ms "fn add(a: i32, b: i32) -> i32 {"
Enter
Type@1ms "    a + b"
Enter
Type@1ms "}"
Enter
Type@1ms ""
Enter
Type@1ms "fn subtract(a: i32, b: i32) -> i32 {"
Enter
Type@1ms "    a - b"
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

# --- Demo: branch selector features ---

Type "vig"
Enter
Sleep 3s

# Switch to BranchList with Tab
Tab
Sleep 1.5s

# Navigate branches with j/k
Type "j"
Sleep 1s
Type "j"
Sleep 1s

# Search branches with /
Type "/login"
Enter
Sleep 2s

# Clear search
Escape
Sleep 1s

# Open action menu with Enter
Enter
Sleep 2s

# Select "View log" with l
Type "l"
Sleep 2.5s

# Move to GitLog pane with Tab
Tab
Sleep 1.5s

# Scroll log with j/k
Type "jjj"
Sleep 1s
Type "kk"
Sleep 1s

# Move to DiffView with Tab
Tab
Sleep 1.5s

# Scroll diff
Type "jjjjj"
Sleep 1.5s

# Go back to FileTree
Escape
Sleep 1s

# Switch to BranchList
Type "l"
Sleep 1s

# Reset to HEAD with Esc
Escape
Sleep 2s

# Quit
Type "q"
Sleep 500ms
