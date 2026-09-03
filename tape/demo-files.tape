Output assets/demo-files.gif

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

# --- Hidden setup: a small project with nested directories ---
Hide

Type@1ms "cd $(mktemp -d)"
Enter
Sleep 300ms
# VHS's xterm.js claims iTerm2 image support but does not draw it, so force
# the unicode half-block renderer for the recording.
Type@1ms `echo 'image-preview "halfblocks"' > ../vig-demo.kdl && export VIG_CONFIG=$PWD/../vig-demo.kdl`
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
Type@1ms "A tiny project used to show the **Files** view."
Enter
Type@1ms ""
Enter
Type@1ms "## Features"
Enter
Type@1ms ""
Enter
Type@1ms "- three columns, like yazi"
Enter
Type@1ms "- syntax-highlighted previews"
Enter
Type@1ms "- rendered *markdown*"
Enter
Type@1ms ""
Enter
Type@1ms "| Key | Action |"
Enter
Type@1ms "|-----|--------|"
Enter
Type@1ms "| `m` | toggle markdown |"
Enter
Type@1ms "| `o` | open with default app |"
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
Type@1ms "echo 'iVBORw0KGgoAAAANSUhEUgAAAUAAAACgCAIAAADywSLLAAACa0lEQVR42u3TMRWAMBBEwQhDRGqUIAc5KElNQ48FihTcZvZ9CTtt9K7g+jkUXHNxgAWwABbAAlgAC2CABbAAFsACWAALYIAFsAAWwAJYAAtggAWwABbAAlgAC2ABDLAAFsACWAALYAEMsAAWwAJYAAtgAQywABbAAlgAC2ABDLAAFsACWAALYAEsgAEWwAJYAAtgASyAARbAAlgAC2ABLIABFsACWAALYAEsgAH2coAFsAAWwAJYAAtggAWwABbAAlgAC2CABbAAFsACWAALYIAFsAAWwAJYAAtguTjA+mPP7MEAsMpwRRpgBaKFGWCFoIUZYEXRxRhg1XZLMsAKoYsxwOhiLIDRxRhg0cswwFqaLsYA08uwAKaXYYBFL8MA08swbwDTy7AAppdhgAUwwADTy7AABhhggEUvwwADbAADDDDAAhhggHMB79ehWZH5cfe1aUoAAwwwwAIYYIABBlgAAwwwwGKYXoABNoABZpheAQwwwACLYXoBZpheAcwwvQCLYXoBFsP0AswwvQIYY3QBFsP0Aqw1GBMFMMboCmCM0QVYJHMLsKIYYwOwimFGBWAVw4wHwCpDGgaA9btcHGABLIAFsAAWwAIYYAEsgAWwABbAAhhgASyABbAAFsACGGABLIAFsAAWwAJYAAMsgAWwABbAAlgAAyyABbAAFsACWAADLIAFsAAWwAJYAAMsgAWwABbAAlgAC2CABbAAFsACWAALYIAFsAAWwAJYAAtggAWwABbAAlgAC2CAvRxgASyABbAAFsACGGABLIAFsAAWwAIYYAEsgAWwABbAAhhgASyABbAAFsACWC+uQR6jr4JbpwAAAABJRU5ErkJggg==' | base64 -d > logo.png"
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
# logo.png: images are previewed too
Type "j"
Sleep 2.5s

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

# README.md is rendered as markdown; `m` toggles the raw text
Type "j"
Sleep 600ms
Type "j"
Sleep 600ms
Type "j"
Sleep 2.5s
Type "m"
Sleep 2s
Type "m"
Sleep 2s

# Focus the preview, then back to the list
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

# Open-with prompt (O): type an app name, then cancel
Type "O"
Sleep 1s
Type "Preview"
Sleep 1.5s
Escape
Sleep 800ms

# Quit
Type "q"
Sleep 500ms
