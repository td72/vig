# Git View

The heart of vig: a side-by-side diff of your working directory against a
base of your choosing. Four panes across the top and main area:

- **Files** — the changed files as a tree, with status indicators
  (`A` added, `D` deleted, `M` modified, `R` renamed, `?` untracked).
- **Branches** — every local branch, with a git log preview of the selected
  one in the main area.
- **Reflog** — the repository's reflog entries.
- **Diff / Git Log** — the main area: the side-by-side diff with syntax
  highlighting, or the log preview while you browse branches or the reflog.

The view watches the working directory and refreshes the diff automatically
when files change; `r` refreshes by hand. `e` opens the selected file in
`$EDITOR`.

## Moving between panes

| Key | Action |
|-----|--------|
| `Tab` / `Shift+Tab` | Cycle panes: Files → Branches → Reflog → GitLog → Diff |
| `h` / `l` | Move between adjacent upper panes (Files, Branches, Reflog) |
| `i` | Jump from upper pane to main pane (GitLog / Diff) |
| `Esc` | Return from main pane to previous upper pane |

And within a pane:

| Key | Action |
|-----|--------|
| `j` / `k` | Scroll down / up |
| `h` / `l` | Scroll left / right (in Diff view) |
| `gg` | Jump to top |
| `G` | Jump to bottom |
| `Ctrl+d` / `Ctrl+u` | Half page down / up |

## Branch list

![branch demo](../../../assets/demo-branch.gif)

Selecting a branch previews its git log in the main area. `Enter` opens the
action menu — the only place in vig that writes to the repository, and it
offers exactly three things: switch to the branch (`git switch`), delete it
safely (`git branch -d`; git refuses if it is unmerged — vig never uses
`-D`), or set it as the diff base so the diff pane compares your working
directory against it.

| Key | Action |
|-----|--------|
| `j` / `k` | Navigate branches (git log preview updates) |
| `Enter` | Action menu (switch / delete / set as diff base) |
| `/` | Search branches |
| `Esc` | Clear search / Reset comparison to HEAD |

## Git log

![commit log demo](../../../assets/demo-commit-log.gif)

The log preview in the main area is itself navigable: walk the commits, yank
a hash, or jump to the commit on GitHub.

| Key | Action |
|-----|--------|
| `j` / `k` | Navigate commits |
| `Ctrl+d` / `Ctrl+u` | Half page scroll |
| `g` / `G` | Top / Bottom |
| `y` | Copy commit hash |
| `o` | Open in GitHub |
| `/` | Search commits |
| `Esc` | Clear search / Back to Branch List |

## Reflog

![reflog demo](../../../assets/demo-reflog.gif)

The reflog pane makes "what did I just do?" diffable: select any reflog entry
and `Enter` sets it as the diff base, so the main pane shows what changed
since that state.

| Key | Action |
|-----|--------|
| `j` / `k` | Navigate entries |
| `Ctrl+d` / `Ctrl+u` | Half page scroll |
| `g` / `G` | Top / Bottom |
| `Enter` | Set as diff base |
| `/` | Search reflog |
| `Esc` | Clear search / Back to Branches |

## The diff pane: modes

The diff pane starts in **Scroll** mode, where `j` / `k` / `h` / `l` scroll.
Press `i` for **Normal** mode, which gives you a character cursor and vim
motions — from there `v` and `V` start selections, exactly like in a vim
buffer.

| Key | Action |
|-----|--------|
| `i` | Enter Normal mode |
| `v` | Visual mode (character) |
| `V` | Visual-Line mode |
| `Esc` | Back to Scroll mode |

## Yank (copy)

![yank demo](../../../assets/demo-yank.gif)

In Normal / Visual mode, yanks go to the system clipboard:

| Key | Action |
|-----|--------|
| `yy` | Yank line |
| `yw` / `ye` / `yb` | Yank word / end of word / word back |
| `y$` / `y0` | Yank to end / start of line |
| `y` (in Visual) | Yank selection |

Text objects are also supported: `iw`, `aw`, `i"`, `a"`, `i(`, `a(`, `i{`, `a{`

## Search

![search demo](../../../assets/demo-search.gif)

| Key | Action |
|-----|--------|
| `/` | Start search |
| `n` | Next match |
| `N` | Previous match |

Search works in all panes (DiffView, FileTree, CommitLog, Reflog) and is
case-insensitive.

## Constraints

- The action menu's `git switch` and `git branch -d` are the **only** write
  operations in all of vig. There is no staging, committing, merging,
  rebasing, pushing, or force-deleting — by design, not by omission.
- The diff always has your working directory on one side; the base (HEAD, a
  branch, or a reflog entry) is what you choose on the other.
