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
- **Docker View** — Containers grouped by compose project, images, inspect summary and a live log tail via the `docker` CLI (read-only)
- **Procs View** — read-only process tree with CPU / memory, listening ports and their owners, and a per-process detail
- **Worktrees View** — git worktrees and stashes at a glance, with the HEAD commit or the stash diff (side-by-side) in a preview pane
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
| `4` | Switch to Docker View |
| `5` | Switch to Procs View |
| `7` | Switch to Worktrees View |

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
Sub-issues are listed under their parent issue as a tree, and PRs in a GitHub Stack (as created by [`gh stack`](https://github.com/github/gh-stack)) are nested bottom-to-top under the PR they build on.

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

Images (PNG / JPEG / GIF / WebP) are previewed in the pane, with their format,
dimensions, size and the renderer in use on the first line. In terminals with a graphics protocol
(Kitty, WezTerm, Ghostty, iTerm2, or Sixel-capable ones such as foot) the image
is drawn at full resolution; elsewhere it falls back to unicode half-blocks.
`image-preview "halfblocks"` skips the terminal detection and `"none"` shows
only the metadata. Images over 20 MB are not decoded.

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
| `o` | Open selected file or directory with the OS default app (`open` / `xdg-open` / `explorer`) |
| `O` | Open selected entry with an app you name (`open -a <app>` on macOS) |
| `r` | Re-read the current directory |

### Docker View

![docker demo](assets/demo-docker.gif)

A read-only view of the local Docker daemon, built on the `docker` CLI's JSON
output (`docker ps`, `docker images`, `docker inspect`, `docker logs`). If
`docker` is not installed or the daemon is not running, the view shows a notice
instead of the panes. Containers are grouped under their compose project
(running ones first), the detail pane shows an inspect summary for the selected
container or image, and the logs pane tails the selected container
(`--tail 200`, then `--since` appends every second while following). The lists
refresh every 5 seconds. Environment variables are never displayed, and nothing
in this view starts, stops or removes anything.

| Key | Action |
|-----|--------|
| `j` / `k` | Move selection (detail and logs follow) |
| `i` / `Enter` | Focus the detail pane |
| `l` (containers) | Focus the logs pane |
| `Tab` / `Shift+Tab` | Cycle panes: Containers → Images → Detail → Logs |
| `j` / `k` / `Ctrl+d` / `Ctrl+u` (detail, logs) | Scroll (scrolling the logs pauses following) |
| `G` (logs) | Jump to the end and resume following |
| `/` `n` `N` | Search container / image names, or log lines |
| `h` / `Esc` (detail, logs) | Back to the list |
| `r` | Re-fetch containers, images, detail and logs |

### Procs View

![procs demo](assets/demo-procs.gif)

A read-only view of what is running: the processes as a tree by parent pid
with CPU % and resident memory, the listening TCP / UDP ports with the
process that owns each one, and a detail of the selected process (pid, ppid,
user, state, uptime, CPU / memory, full command line, cwd, executable,
children, listening ports). Values that need privileges you do not have are
shown as `(no access)`; environment variables are never read or displayed.
The view only inspects — it never sends a signal.

Processes come from [sysinfo](https://crates.io/crates/sysinfo); ports from
`lsof` on macOS and `ss` on Linux. Both are re-read every 2 seconds while the
view is shown (`procs-refresh-interval` in the config) and on `r`.

| Key | Action |
|-----|--------|
| `j` / `k` / `Ctrl+d` / `Ctrl+u` / `g` / `G` | Move in the process tree (detail follows) |
| `s` | Cycle the sort: CPU → MEM → PID (shown in the pane title) |
| `Enter` / `i` / `l` | Focus the detail pane |
| `/` `n` `N` | Search command lines (processes) or address / port / name (ports) |
| `Tab` / `Shift+Tab` | Cycle panes: Processes → Ports → Detail |
| `Enter` (ports) | Jump to the process that owns the port |
| `j` / `k` / `Ctrl+d` / `Ctrl+u` (detail) | Scroll |
| `h` / `Esc` (detail) | Back to the process list |
| `r` | Refresh now |

### Worktrees View

![worktrees demo](assets/demo-worktrees.gif)

A read-only overview of the repository's worktrees and stashes. The top-left
pane lists the worktrees (`git worktree list`) with their path — relative to
the main worktree where possible — the checked-out branch (or a detached
HEAD), and flags such as `[main]`, `[locked]`, `[prunable]` or `[bare]`; the
worktree vig is running in is marked with `*`. The bottom-left pane lists the
stashes (`stash@{n}`, message, the branch they were made on and how long ago).

The preview on the right follows the selection: for a worktree it shows the
HEAD commit (hash, author, date, subject) and its changed files; for a stash
it shows the stash's patch, including untracked files it carries, in the same
side-by-side diff view as the Git view — syntax highlighting, search, and
Normal / Visual mode with yank work there. Nothing is ever applied,
dropped, added or removed from this view.

| Key | Action |
|-----|--------|
| `Tab` / `Shift+Tab` | Cycle panes: Worktrees → Stashes → Preview |
| `j` / `k` | Move selection (preview follows) |
| `i` / `l` / `Enter` | Focus the preview |
| `j` / `k` / `Ctrl+d` / `Ctrl+u` (preview) | Scroll |
| `h` / `l` (preview) | Scroll the diff horizontally |
| `[` / `]` (preview) | Previous / next file in a multi-file stash |
| `i` (preview) | Normal mode in the stash diff (`v` / `V` / `y` as in the Git view) |
| `Esc` / `Backspace` (preview) | Back to the list |
| `/` `n` `N` | Search paths / branches (worktrees), messages / branches (stashes), or the diff |
| `r` | Re-read worktrees and stashes |

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
