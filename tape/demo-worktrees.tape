Output assets/demo-worktrees.gif

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

# --- Hidden setup: a repo with a second worktree and two stashes ---
Hide

Type@1ms "cd $(mktemp -d) && mkdir repo && cd repo"
Enter
Sleep 300ms
# Ignore any user config so the recording shows the defaults.
Type@1ms `echo 'theme "base16-eighties.dark"' > ../vig-demo.kdl && export VIG_CONFIG=$PWD/../vig-demo.kdl`
Enter
Sleep 300ms
Type@1ms "git init -q -b main && git config user.email a@b.c && git config user.name demo"
Enter
Sleep 300ms
Type@1ms "mkdir -p src"
Enter

Type@1ms "cat > src/main.rs << 'EOF'"
Enter
Type@1ms "mod util;"
Enter
Type@1ms ""
Enter
Type@1ms "fn main() {"
Enter
Type@1ms `    println!("Hello, {}!", util::name());`
Enter
Type@1ms "}"
Enter
Type@1ms "EOF"
Enter

Type@1ms "cat > src/util.rs << 'EOF'"
Enter
Type@1ms "/// The name to greet."
Enter
Type@1ms "pub fn name() -> &'static str {"
Enter
Type@1ms `    "vig"`
Enter
Type@1ms "}"
Enter
Type@1ms "EOF"
Enter

Type@1ms "printf '# demo\n\nA tiny project used to show the Worktrees view.\n' > README.md"
Enter
Type@1ms "git add -A && git commit -q -m 'initial commit'"
Enter
Sleep 300ms
Type@1ms `printf '[package]\nname = "demo"\n' > Cargo.toml && git add -A && git commit -q -m 'add Cargo.toml'`
Enter
Sleep 300ms

# A second worktree on its own branch, one commit ahead.
Type@1ms "git worktree add -q ../wt-feature -b feature"
Enter
Sleep 300ms
Type@1ms "(cd ../wt-feature && printf '\n- feature notes\n' >> README.md && git commit -q -am 'feature: add notes to README')"
Enter
Sleep 300ms

# Two stashes: a small edit, then an edit plus an untracked file.
Type@1ms "perl -pi -e 's/Hello/Howdy/' src/main.rs && git stash push -q -m 'wip: greeting tweak'"
Enter
Sleep 300ms
Type@1ms `perl -pi -e 's/"vig"/"worktrees"/' src/util.rs && printf 'Notes\n=====\n\n- stash preview\n' > NOTES.md && git stash push -q -u -m 'wip: rename and notes'`
Enter
Sleep 300ms

Type@1ms "clear"
Enter
Sleep 300ms

Show

# --- Demo: the Worktrees view ---

Type "vig"
Enter
Sleep 2s

# Switch to the Worktrees view (6)
Type "6"
Sleep 2.5s

# Select the feature worktree; the preview shows its HEAD commit
Type "j"
Sleep 2.5s

# Tab to the stash list; the preview shows the stash diff
Tab
Sleep 2.5s
Type "j"
Sleep 2.5s

# Focus the preview, scroll, and step to the next file of the stash
Type "i"
Sleep 1.5s
Type "j"
Sleep 500ms
Type "j"
Sleep 1.5s
Type "]"
Sleep 2.5s
Type "["
Sleep 1.5s

# Back to the stash list
Escape
Sleep 1.5s

# Search stashes by message
Type "/greet"
Enter
Sleep 2s
Escape
Sleep 1s

# Quit
Type "q"
Sleep 500ms
