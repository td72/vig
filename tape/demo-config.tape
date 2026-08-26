Output assets/demo-config.gif

Require vig

Set Shell "bash"
Set FontSize 16
Set Width 1200
Set Height 900
Set Theme "Catppuccin Frappe"
Set WindowBar Colorful
Set CursorBlink false
Set TypingSpeed 50ms

# --- Hidden setup: a small git repo with a working-tree change ---
Hide

Type@1ms "cd $(mktemp -d)"
Enter
Sleep 300ms
Type@1ms "git init -q && git config user.email a@b.c && git config user.name demo"
Enter
Sleep 300ms

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
Type@1ms "printf 'pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n' > lib.rs"
Enter
Type@1ms "git add -A && git commit -q -m 'initial commit'"
Enter
Sleep 300ms
Type@1ms "printf 'pub fn sub(a: i32, b: i32) -> i32 {\n    a - b\n}\n' >> lib.rs"
Enter
Type@1ms "git add -A && git commit -q -m 'add sub'"
Enter
Sleep 300ms

Type@1ms "cat > main.rs << 'EOF'"
Enter
Type@1ms "mod lib;"
Enter
Type@1ms ""
Enter
Type@1ms "fn main() {"
Enter
Type@1ms `    println!("2 + 3 = {}", lib::add(2, 3));`
Enter
Type@1ms "}"
Enter
Type@1ms "EOF"
Enter
Sleep 200ms

# --- The user config: sidebar on the right + a custom key ---

Type@1ms "cat > config.kdl << 'EOF'"
Enter
Type@1ms `page "git" {`
Enter
Type@1ms `    // Sidebar on the right, diff / log on the left`
Enter
Type@1ms `    layout {`
Enter
Type@1ms `        split direction="horizontal" {`
Enter
Type@1ms `            slot "main" size="min:3" then="git_log" default="diff_view" {`
Enter
Type@1ms `                triggers "branch_list" "reflog" "git_log"`
Enter
Type@1ms `            }`
Enter
Type@1ms `            split direction="vertical" size="35%" {`
Enter
Type@1ms `                place "file_tree" size="50%"`
Enter
Type@1ms `                place "branch_list" size="30%"`
Enter
Type@1ms `                place "reflog" size="min:5"`
Enter
Type@1ms `            }`
Enter
Type@1ms `        }`
Enter
Type@1ms `    }`
Enter
Type@1ms `    pane "file_tree" {`
Enter
Type@1ms `        keys {`
Enter
Type@1ms `            "o" "ExpandOrOpen"   // add a binding`
Enter
Type@1ms `            "Space" "None"       // remove a binding`
Enter
Type@1ms `        }`
Enter
Type@1ms `    }`
Enter
Type@1ms `}`
Enter
Type@1ms "EOF"
Enter
Sleep 200ms

Type@1ms "clear"
Enter
Sleep 300ms

Show

# --- Demo: show the config, then run vig with it ---

Type "cat config.kdl"
Enter
Sleep 4s

Type "clear"
Enter
Sleep 300ms

Type "vig --config config.kdl"
Enter
Sleep 3s

# Custom key: 'o' opens the file in the diff view
Type "o"
Sleep 2s

# Help overlay reflects the custom bindings
Type "?"
Sleep 3s
Escape
Sleep 500ms

# Cycle panes: the sidebar is on the right now
Tab
Sleep 1.5s
Tab
Sleep 1.5s

# Quit
Type "q"
Sleep 500ms
