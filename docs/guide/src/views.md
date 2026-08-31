# Views

vig is organized as a row of views — tabs in the header, numbered `1:Git`
through `7:Projects`. Each view is a self-contained page with its own panes
and key bindings; the following chapters cover them one by one.

## Read-only by design

Before the keys, the philosophy — because it shapes every view:

**vig inspects; it does not mutate.** The only write operations in the entire
program are the two safe git commands offered by the branch action menu:
`git switch` and `git branch -d` (safe delete, which git refuses if the branch
is unmerged). Everything else — GitHub, Docker, processes, worktrees, stashes,
project boards — is strictly read-only:

- The GitHub view never reruns, cancels, comments, or edits anything.
- The Docker view never starts, stops, or removes containers or images, and
  never displays environment variables.
- The Procs view never sends a signal, and never reads or displays
  environment variables.
- The Worktrees view never applies, drops, or prunes anything.
- The Projects view never adds, moves, edits, or deletes items.

You can leave vig open all day and it will not change your system. Destructive
operations (merge, rebase, force delete, push) are not "hidden behind a
setting" — they do not exist in the code.

## Switching views

| Key | Action |
|-----|--------|
| `1` | Switch to Git View |
| `2` | Switch to GitHub View |
| `3` | Switch to Files View |
| `4` | Switch to Docker View |
| `5` | Switch to Procs View |
| `6` | Switch to Worktrees View |
| `7` | Switch to Projects View |

The numbers are the position of the view in the header, which follows the
`pages` list in the configuration — trim or reorder that list and the numbers
follow.

## Conventions shared by every view

The views deliberately feel the same:

- `j` / `k` move, `gg` / `G` jump to top / bottom, `Ctrl+d` / `Ctrl+u` scroll
  half a page.
- `Tab` / `Shift+Tab` cycle the panes of the view.
- Selecting an item in a list updates its detail / preview pane immediately;
  `i` or `Enter` moves focus into it, `h` or `Esc` comes back.
- `/` searches within the focused pane, `n` / `N` step through matches
  (case-insensitive).
- `r` refreshes, `?` shows help, `q` / `Ctrl+c` quits.

> The key tables in these chapters show the **default** bindings, taken from
> the built-in configuration
> ([assets/default.kdl](https://github.com/td72/vig/blob/main/assets/default.kdl)),
> which is the source of truth. The
> [README](https://github.com/td72/vig/blob/main/README.md) carries a
> condensed summary and may omit minor keys. Every key can be rebound or unbound; see the
> configuration chapters.
