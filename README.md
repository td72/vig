# vig

[日本語](docs/README.ja.md)

A Git TUI side-by-side diff viewer with vim-style keybindings.

## Features

- Side-by-side diff view with syntax highlighting
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

### Navigation

| Key | Action |
|-----|--------|
| `j` / `k` | Scroll down / up |
| `h` / `l` | Scroll left / right |
| `gg` | Jump to top |
| `G` | Jump to bottom |
| `Ctrl+d` / `Ctrl+u` | Half page down / up |
| `Tab` / `Shift+Tab` | Switch pane |

### Modes

| Key | Action |
|-----|--------|
| `i` | Enter Normal mode |
| `v` | Visual mode (character) |
| `V` | Visual-Line mode |
| `Esc` | Back to Scroll mode |

### Yank (copy)

| Key | Action |
|-----|--------|
| `yy` | Yank line |
| `yw` / `ye` / `yb` | Yank word / end of word / word back |
| `y$` / `y0` | Yank to end / start of line |
| `y` (in Visual) | Yank selection |

Text objects are also supported: `iw`, `aw`, `i"`, `a"`, `i(`, `a(`, `i{`, `a{`

### Other

| Key | Action |
|-----|--------|
| `Enter` / `Space` | Open file / Toggle directory |
| `e` | Open in external editor |
| `r` | Refresh diff |
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

### CI

GitHub Actions runs on push to `main` and pull requests:

- prek hooks (fmt + clippy)
- `cargo test`
- `cargo build`

## License

MIT
