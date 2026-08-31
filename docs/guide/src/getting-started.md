# Getting Started

vig runs entirely in your terminal. Install it, `cd` into a Git repository,
and run `vig` — no configuration is required to get a working setup.

## Installation

### Homebrew

```bash
brew install td72/tap/vig
```

### Pre-built binaries

Download a pre-built binary from the
[GitHub Releases](https://github.com/td72/vig/releases) page:

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

## First run

Run vig inside a Git repository:

```bash
cd your-repo
vig
```

You land in the **Git view**: the changed files on the left, branches and the
reflog next to them, and the side-by-side diff filling the rest of the screen.
The header lists the views as numbered tabs (`1:Git`, `2:GitHub`, …); press the
number to switch. The status bar at the bottom shows the current mode and the
most useful keys for the focused pane.

A few things worth knowing on day one:

- `q` or `Ctrl+c` quits.
- `?` opens a help overlay listing every binding of the current view.
- `r` refreshes the current view; the Git view also refreshes automatically
  when files change on disk.
- vig works without any configuration. When you want to change something,
  a single KDL file does it (`--config <path>`, `$VIG_CONFIG`, or
  `~/.config/vig/config.kdl`) — see the configuration chapters.

## A tour of the seven views

vig ships with seven views. Each gets its own chapter in [Views](views.md);
this is the thirty-second version.

### 1 — Git

![git demo](../../../assets/demo.gif)

The heart of vig: a side-by-side diff of your working directory with syntax
highlighting, a file tree with status indicators, a branch selector with a git
log preview, and the reflog. Compare against any branch or reflog entry, yank
text with vim motions, and open files in `$EDITOR`.

### 2 — GitHub

![github demo](../../../assets/demo-github-pr.gif)

Issues, pull requests (body, comments, reviews, CI status) and Actions
workflow runs (jobs, steps, job logs) — browsed read-only through the `gh`
CLI, with bodies rendered as Markdown.

### 3 — Files

![files demo](../../../assets/demo-files.gif)

A yazi-like three-column file browser rooted at the repository: parent
directory, current directory, and a preview with syntax highlighting — images
included, drawn at full resolution in terminals with a graphics protocol.

### 4 — Docker

![docker demo](../../../assets/demo-docker.gif)

Containers grouped by compose project, images, an inspect summary and a live
log tail — read-only, via the `docker` CLI.

### 5 — Procs

![procs demo](../../../assets/demo-procs.gif)

A process tree with CPU and memory, listening ports and their owners,
btop-style system history graphs, and a per-process detail with CPU / RSS
sparklines. Inspect-only: vig never sends a signal.

### 6 — Worktrees

![worktrees demo](../../../assets/demo-worktrees.gif)

Your worktrees and stashes at a glance, with the HEAD commit or the stash's
patch shown in the same side-by-side diff view as the Git view.

### 7 — Projects

![projects demo](../../../assets/demo-projects.gif)

The GitHub Projects (v2) boards linked to the repository: kanban columns by
`Status`, a sortable table mode, and an item detail with every project field.

## Getting help inside vig

Press `?` in any view to open the help overlay. It lists every key binding of
the current view — including your own rebindings, since it is generated from
the active configuration. Press `?` or `Esc` to close it.

## Keeping vig up to date

```bash
vig update
```

`vig update` downloads the latest release from GitHub, verifies its signature,
and replaces the current binary. It is meant for installs from the pre-built
release binaries; if you installed through Homebrew or cargo, prefer
`brew upgrade vig` or `cargo install vig` so your package manager stays in
charge.

## Requirements

vig itself only needs a Git repository to run in. Some views use external
tools when present:

| View | Needs | Without it |
|------|-------|------------|
| Git, Worktrees | nothing extra | — |
| GitHub | [GitHub CLI (`gh`)](https://cli.github.com/) installed and authenticated (`gh auth login`) | the view shows a notice |
| Projects | `gh` with the `project` token scope — run `gh auth refresh -s project` | the view shows a notice explaining the missing scope |
| Docker | `docker` CLI and a running daemon | the view shows a notice |
| Procs | nothing extra (`lsof` on macOS / `ss` on Linux for ports) | port info may be empty |

Two smaller notes:

- **Nerd Font** — the Files view shows file-type icons that need a
  [Nerd Font](https://www.nerdfonts.com/). If your terminal font is not one,
  put `icons "none"` in your config.
- **`$EDITOR`** — `e` opens the selected file in your external editor.
