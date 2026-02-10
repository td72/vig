Output assets/demo.gif

Require vig

Set Shell "bash"
Set FontSize 16
Set Width 1200
Set Height 600
Set Theme "Catppuccin Frappe"
Set WindowBar Colorful
Set CursorBlink false
Set TypingSpeed 50ms

# --- Hidden setup: create a temporary git repo with changes ---
Hide

Type@1ms "cd $(mktemp -d)"
Enter
Sleep 300ms
Type@1ms "git init -q && git config user.email a@b.c && git config user.name demo"
Enter
Sleep 300ms

# --- Initial files ---

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
Type@1ms ""
Enter
Type@1ms "fn subtract(a: i32, b: i32) -> i32 {"
Enter
Type@1ms "    a - b"
Enter
Type@1ms "}"
Enter
Type@1ms ""
Enter
Type@1ms "fn format_output(label: &str, value: i32) -> String {"
Enter
Type@1ms `    format!("{}: {}", label, value)`
Enter
Type@1ms "}"
Enter
Type@1ms ""
Enter
Type@1ms "#[cfg(test)]"
Enter
Type@1ms "mod tests {"
Enter
Type@1ms "    use super::*;"
Enter
Type@1ms ""
Enter
Type@1ms "    #[test]"
Enter
Type@1ms "    fn test_add() {"
Enter
Type@1ms "        assert_eq!(add(2, 3), 5);"
Enter
Type@1ms "    }"
Enter
Type@1ms ""
Enter
Type@1ms "    #[test]"
Enter
Type@1ms "    fn test_subtract() {"
Enter
Type@1ms "        assert_eq!(subtract(5, 3), 2);"
Enter
Type@1ms "    }"
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
Type@1ms `edition = "2021"`
Enter
Type@1ms "EOF"
Enter
Sleep 200ms

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

Type@1ms "git add -A && git commit -q -m init"
Enter
Sleep 500ms

# --- Modified files (unstaged working directory changes) ---

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
Type@1ms ""
Enter
Type@1ms "    let sum = add(2, 3);"
Enter
Type@1ms `    println!("{}", format_output("Sum", sum));`
Enter
Type@1ms ""
Enter
Type@1ms "    let product = multiply(4, 5);"
Enter
Type@1ms `    println!("{}", format_output("Product", product));`
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
Type@1ms ""
Enter
Type@1ms "fn multiply(a: i32, b: i32) -> i32 {"
Enter
Type@1ms "    a * b"
Enter
Type@1ms "}"
Enter
Type@1ms ""
Enter
Type@1ms "fn divide(a: i32, b: i32) -> Option<i32> {"
Enter
Type@1ms "    if b == 0 { None } else { Some(a / b) }"
Enter
Type@1ms "}"
Enter
Type@1ms ""
Enter
Type@1ms "fn format_output(label: &str, value: i32) -> String {"
Enter
Type@1ms `    format!("[{}] {}", label, value)`
Enter
Type@1ms "}"
Enter
Type@1ms ""
Enter
Type@1ms "#[cfg(test)]"
Enter
Type@1ms "mod tests {"
Enter
Type@1ms "    use super::*;"
Enter
Type@1ms ""
Enter
Type@1ms "    #[test]"
Enter
Type@1ms "    fn test_add() {"
Enter
Type@1ms "        assert_eq!(add(2, 3), 5);"
Enter
Type@1ms "    }"
Enter
Type@1ms ""
Enter
Type@1ms "    #[test]"
Enter
Type@1ms "    fn test_subtract() {"
Enter
Type@1ms "        assert_eq!(subtract(5, 3), 2);"
Enter
Type@1ms "    }"
Enter
Type@1ms ""
Enter
Type@1ms "    #[test]"
Enter
Type@1ms "    fn test_multiply() {"
Enter
Type@1ms "        assert_eq!(multiply(4, 5), 20);"
Enter
Type@1ms "    }"
Enter
Type@1ms ""
Enter
Type@1ms "    #[test]"
Enter
Type@1ms "    fn test_divide() {"
Enter
Type@1ms "        assert_eq!(divide(10, 2), Some(5));"
Enter
Type@1ms "        assert_eq!(divide(10, 0), None);"
Enter
Type@1ms "    }"
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
Type@1ms `version = "0.2.0"`
Enter
Type@1ms `edition = "2021"`
Enter
Type@1ms `description = "A sample application"`
Enter
Type@1ms ""
Enter
Type@1ms "[dependencies]"
Enter
Type@1ms `clap = "4.0"`
Enter
Type@1ms "EOF"
Enter
Sleep 200ms

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

Type@1ms "clear"
Enter
Sleep 300ms

Show

# --- Demo: vig navigation features ---

Type "vig"
Enter
Sleep 3s

# Navigate to main.rs in file tree (skip config.toml)
Type "j"
Sleep 500ms

# Select main.rs → switch to diff view
Enter
Sleep 2s

# Scroll down with j
Type "jjjjj"
Sleep 1.5s

# Scroll up with k
Type "kkk"
Sleep 1.5s

# Jump to bottom
Type "G"
Sleep 1.5s

# Jump to top
Type "gg"
Sleep 1.5s

# Half page down
Ctrl+d
Sleep 1.5s

# Half page up
Ctrl+u
Sleep 1.5s

# Switch to file tree
Tab
Sleep 1s

# Navigate to utils.rs
Type "j"
Sleep 500ms

# Select utils.rs
Enter
Sleep 2s

# Quit
Type "q"
Sleep 500ms
