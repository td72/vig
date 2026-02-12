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

## Installation

### Requirements

- Rust toolchain
- libgit2, libssl, pkg-config

### Build from source

```bash
cargo install --path .
```

## Usage

Run in a Git repository:

```bash
vig
```

## Key Bindings

### Pane Navigation

| Key | Action |
|-----|--------|
| `Tab` / `Shift+Tab` | Cycle panes: Files → Branches → Reflog → GitLog → Diff |
| `h` / `l` | Move between adjacent panes |
| `i` | Enter next pane |

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
| `j` / `k` | Scroll log |
| `Ctrl+d` / `Ctrl+u` | Half page scroll |
| `g` / `G` | Top / Bottom |
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
| `Esc` | Clear search / Back to Git Log |

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
