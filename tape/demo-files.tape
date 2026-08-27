Output assets/demo-files.gif

Require vig

Set Shell "bash"
Set FontSize 16
Set Width 1200
Set Height 700
Set Theme "Catppuccin Frappe"
Set WindowBar Colorful
Set CursorBlink false
Set TypingSpeed 50ms

# --- Hidden setup: a small project with nested directories ---
Hide

Type@1ms "cd $(mktemp -d)"
Enter
Sleep 300ms
Type@1ms "git init -q && git config user.email a@b.c && git config user.name demo"
Enter
Sleep 300ms
Type@1ms "mkdir -p src/utils docs"
Enter

Type@1ms "cat > src/main.rs << 'EOF'"
Enter
Type@1ms "mod utils;"
Enter
Type@1ms ""
Enter
Type@1ms "fn main() {"
Enter
Type@1ms `    let name = utils::greet("vig");`
Enter
Type@1ms `    println!("{name}");`
Enter
Type@1ms "}"
Enter
Type@1ms "EOF"
Enter

Type@1ms "cat > src/utils/mod.rs << 'EOF'"
Enter
Type@1ms "/// Build a greeting for `name`."
Enter
Type@1ms "pub fn greet(name: &str) -> String {"
Enter
Type@1ms `    format!("Hello, {name}!")`
Enter
Type@1ms "}"
Enter
Type@1ms "EOF"
Enter

Type@1ms "cat > README.md << 'EOF'"
Enter
Type@1ms "# demo"
Enter
Type@1ms ""
Enter
Type@1ms "A tiny project used to show the Files view."
Enter
Type@1ms "EOF"
Enter

Type@1ms "cat > Cargo.toml << 'EOF'"
Enter
Type@1ms "[package]"
Enter
Type@1ms `name = "demo"`
Enter
Type@1ms `version = "0.1.0"`
Enter
Type@1ms "EOF"
Enter

Type@1ms "echo 'Notes go here.' > docs/notes.md"
Enter
Type@1ms "git add -A && git commit -q -m 'initial commit'"
Enter
Sleep 300ms

Type@1ms "clear"
Enter
Sleep 300ms

Show

# --- Demo: the Files view ---

Type "vig"
Enter
Sleep 2s

# Switch to the Files view
Type "3"
Sleep 2s

# Move down the listing; the preview follows the selection
Type "j"
Sleep 1.2s
Type "j"
Sleep 1.5s
Type "j"
Sleep 1.5s

# Enter src/ and look at main.rs
Type "k"
Sleep 600ms
Type "k"
Sleep 800ms
Type "l"
Sleep 1.5s
Type "j"
Sleep 2s

# Enter src/utils/
Type "k"
Sleep 600ms
Type "l"
Sleep 2s

# Back up twice (the directory we came from stays selected)
Type "h"
Sleep 1.2s
Type "h"
Sleep 1.5s

# Focus the preview of README.md, then back to the list
Type "j"
Sleep 600ms
Type "j"
Sleep 800ms
Type "i"
Sleep 1.5s
Escape
Sleep 1s

# Search by name
Type "/cargo"
Enter
Sleep 2s
Escape
Sleep 800ms

# Quit
Type "q"
Sleep 500ms
