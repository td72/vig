# vig

[日本語](docs/README.ja.md)

A Git TUI side-by-side diff viewer with vim-style keybindings.

> **Safe by design** — vig only performs read operations and safe git commands (`git switch`, `git branch -d`). Destructive operations like merge, rebase, or force delete are intentionally excluded.

![demo](assets/demo.gif)

## Features

- Side-by-side diff view with syntax highlighting
- Branch selector with git log preview
- Compare working directory against any local branch
- Vim-style modes: Scroll, Normal, Visual, Visual-Line
- File tree with status indicators (A/D/M/R/?)
- Yank (copy) to system clipboard with vim motions
- Live file watching with auto-refresh
- Open files in external editor (`$EDITOR`)
- **GitHub View** — Browse Issues and Pull Requests (body, comments, reviews, CI status) via `gh` CLI
- **Files View** — yazi-like three-column file browser (parent / current / preview) with syntax-highlighted previews
- Configurable layout, key bindings, and highlighting theme via `~/.config/vig/config.kdl`

## Installation

### Homebrew

```bash
brew install td72/tap/vig
```

### Pre-built binaries

Download a pre-built binary from the [GitHub Releases](https://github.com/td72/vig/releases) page:

```bash
# Linux x86_64
curl -sL https://github.com/td72/vig/releases/latest/download/vig-x86_64-unknown-linux-gnu.tar.gz | tar xz -C ~/.local/bin vig

# Linux aarch64
curl -sL https://github.com/td72/vig/releases/latest/download/vig-aarch64-unknown-linux-gnu.tar.gz | tar xz -C ~/.local/bin vig

# macOS Apple Silicon
curl -sL https://github.com/td72/vig/releases/latest/download/vig-aarch64-apple-darwin.tar.gz | tar xz -C ~/.local/bin vig
```

### crates.io

```bash
cargo install vig
```

### Build from source

Requires: Rust toolchain, libgit2, libssl, pkg-config

```bash
cargo install --path .
```

## Usage

Run in a Git repository:

```bash
vig
```

## Configuration

vig works out of the box. To change the layout, key bindings, or highlighting theme, drop a KDL
file at `~/.config/vig/config.kdl` (or pass `--config <path>` / set
`$VIG_CONFIG`). Only the parts you write are overridden; everything else
keeps its default.

![config demo](assets/demo-config.gif)

```kdl
// ~/.config/vig/config.kdl
theme "Solarized (dark)"
page "git" {
    pane "file_tree" {
        keys {
            "o" "ExpandOrOpen"   // add a binding
            "Space" "None"       // remove a binding
        }
    }
}
```

```bash
vig config path     # show which file would be used
vig config dump     # print the built-in defaults as a starting point
vig config themes   # list the available highlighting themes
```

Layouts can be rearranged too (e.g. sidebar on the right). A broken config
fails fast with the file path and line number rather than silently falling
back to defaults. See [docs/config.md](docs/config.md) for the full schema.

## Key Bindings

### View Switching

| Key | Action |
|-----|--------|
| `1` | Switch to Git View |
| `2` | Switch to GitHub View |
| `3` | Switch to Files View |

### Pane Navigation

| Key | Action |
|-----|--------|
| `Tab` / `Shift+Tab` | Cycle panes: Files → Branches → Reflog → GitLog → Diff |
| `h` / `l` | Move between adjacent upper panes (Files, Branches, Reflog) |
| `i` | Jump from upper pane to main pane (GitLog / Diff) |
| `Esc` | Return from main pane to previous upper pane |

### Navigation

| Key | Action |
|-----|--------|
| `j` / `k` | Scroll down / up |
| `h` / `l` | Scroll left / right (in Diff view) |
| `gg` | Jump to top |
| `G` | Jump to bottom |
| `Ctrl+d` / `Ctrl+u` | Half page down / up |

### Branch List

![branch demo](assets/demo-branch.gif)

| Key | Action |
|-----|--------|
| `j` / `k` | Navigate branches (git log preview updates) |
| `Enter` | Action menu (switch / delete / set as diff base) |
| `/` | Search branches |
| `Esc` | Clear search / Reset comparison to HEAD |

### Git Log

| Key | Action |
|-----|--------|
| `j` / `k` | Navigate commits |
| `Ctrl+d` / `Ctrl+u` | Half page scroll |
| `g` / `G` | Top / Bottom |
| `y` | Copy commit hash |
| `o` | Open in GitHub |
| `/` | Search commits |
| `Esc` | Clear search / Back to Branch List |

### Reflog

| Key | Action |
|-----|--------|
| `j` / `k` | Navigate entries |
| `Ctrl+d` / `Ctrl+u` | Half page scroll |
| `g` / `G` | Top / Bottom |
| `Enter` | Set as diff base |
| `/` | Search reflog |
| `Esc` | Clear search / Back to Branches |

### Modes

| Key | Action |
|-----|--------|
| `i` | Enter Normal mode |
| `v` | Visual mode (character) |
| `V` | Visual-Line mode |
| `Esc` | Back to Scroll mode |

### Yank (copy)

![yank demo](assets/demo-yank.gif)

| Key | Action |
|-----|--------|
| `yy` | Yank line |
| `yw` / `ye` / `yb` | Yank word / end of word / word back |
| `y$` / `y0` | Yank to end / start of line |
| `y` (in Visual) | Yank selection |

Text objects are also supported: `iw`, `aw`, `i"`, `a"`, `i(`, `a(`, `i{`, `a{`

### Search

| Key | Action |
|-----|--------|
| `/` | Start search |
| `n` | Next match |
| `N` | Previous match |

Search works in all panes (DiffView, FileTree, CommitLog, Reflog). Case-insensitive.

### GitHub View

Browse GitHub Issues and Pull Requests directly within vig. Requires [GitHub CLI (`gh`)](https://cli.github.com/) to be installed and authenticated.
Bodies and comments are rendered as Markdown (headings, lists, task lists, code, tables narrowed to fit the pane width where possible).

| Key | Action |
|-----|--------|
| `h` / `l` | Switch between Issue List and PR List |
| `j` / `k` | Navigate list |
| `i` / `Enter` | Open detail view |
| `o` | Open in browser |
| `Esc` | Back to list |
| `Ctrl+d` / `Ctrl+u` | Half page scroll (detail view) |
| `g` / `G` | Top / Bottom |
| `r` | Refresh data |

### Files View

![files demo](assets/demo-files.gif)

A read-only file browser rooted at the repository. The left column shows the
parent directory, the middle the current one, and the right a preview of the
selected entry (syntax-highlighted text, or a listing for directories).
Entries get Nerd Font icons by file type; if your terminal font is not a
[Nerd Font](https://www.nerdfonts.com/), put `icons "none"` in your config.

| Key | Action |
|-----|--------|
| `j` / `k` | Move selection (preview follows) |
| `l` / `→` / `Enter` | Enter directory / focus preview |
| `h` / `←` / `Backspace` | Parent directory |
| `i` | Focus preview |
| `j` / `k` / `Ctrl+d` / `Ctrl+u` (preview) | Scroll |
| `h` / `Esc` (preview) | Back to file list |
| `/` `n` `N` | Search file names |
| `e` | Open selected file in external editor |
| `r` | Re-read the current directory |

### Other

| Key | Action |
|-----|--------|
| `Enter` / `Space` | Open file / Toggle directory |
| `e` | Open in external editor |
| `r` | Refresh diff and branches |
| `?` | Show help |
| `q` / `Ctrl+c` | Quit |

## Development

### Setup

```bash
mise install   # installs prek
mise exec -- prek install   # installs pre-commit hooks
```

### Pre-commit hooks

Managed by [prek](https://github.com/j178/prek):

- `cargo fmt --check`
- `cargo clippy`
- Trailing whitespace, EOF fixer, TOML/YAML check, merge conflict check, large file check
- GIF freshness check (tape modified → gif must be re-recorded)

### CI

GitHub Actions runs on push to `main` and pull requests:

- prek hooks (fmt + clippy)
- `cargo test`
- `cargo build`

## License

MIT
