# Worktrees View

![worktrees demo](../../../assets/demo-worktrees.gif)

A read-only overview of the repository's worktrees and stashes. The top-left
pane lists the worktrees (`git worktree list`) with their path — relative to
the main worktree where possible — the checked-out branch (or a detached
HEAD), and flags such as `[main]`, `[locked]`, `[prunable]` or `[bare]`; the
worktree vig is running in is marked with `*`. The bottom-left pane lists the
stashes (`stash@{n}`, message, the branch they were made on and how long ago).

## The preview pane

The preview on the right follows the selection:

- For a **worktree** it shows the HEAD commit (hash, author, date, subject)
  and its changed files.
- For a **stash** it shows the stash's patch, including untracked files it
  carries, in the same side-by-side diff view as the Git view — syntax
  highlighting, search, and Normal / Visual mode with yank all work there.
  `[` / `]` step through the files of a multi-file stash.

## Key bindings

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

## Constraints

- Nothing is ever applied, dropped, added, removed, locked, or pruned from
  this view — it lists and previews, full stop. Managing worktrees and stashes
  stays in your shell.
