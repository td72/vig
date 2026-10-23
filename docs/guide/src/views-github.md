# GitHub View

![github demo](../../../assets/demo-github-pr.gif)

Browse GitHub Issues, Pull Requests and Actions workflow runs directly within
vig — three columns across the top, with a detail view for each. Requires the
[GitHub CLI (`gh`)](https://cli.github.com/) to be installed and
authenticated; without it the view shows a notice instead of the panes.

Bodies and comments are rendered as Markdown (headings, lists, task lists,
code, tables narrowed to fit the pane width where possible). Sub-issues are
listed under their parent issue as a tree, and PRs in a GitHub Stack (as
created by [`gh stack`](https://github.com/github/gh-stack)) are nested
bottom-to-top under the PR they build on.

## Issues and Pull Requests

![github issue demo](../../../assets/demo-github-issue.gif)

`j` / `k` walk the list while the detail follows; `i` or `Enter` moves into
the detail view, where `h` / `l` switch between the body and the right-hand
sub-panes (comments, reviews, CI status for a PR). `o` opens the item in the
browser. In an issue or PR detail, `w` toggles **watch mode**: vig re-fetches
the open item about every 10 seconds so a conversation or CI status you are
waiting on stays current.

`d` on a pull request — in the list or in its detail — opens the PR's diff
in a window on top of the page: the changed files as a tree on the left,
the side-by-side diff on the right, exactly as the [Git view](views-git.md)
shows a working-tree diff (folding with `Space`, `/` search, `i` for the
cursor mode with yank and visual selection). The header names the PR, its
branches and title; the status bar counts the files and lines. The diff
comes from `gh pr diff`, a REST call that costs no GraphQL points and works
for pull requests from forks and branches you never fetched; the last diff
seen is cached on disk and shown at once while a fresh one is fetched (`r`
fetches again). `Esc` steps back — diff → tree → close — and `q` closes
outright; `o` opens the PR's Files tab in the browser, `y` copies that URL,
and `e` opens the selected file in `$EDITOR` when the working tree has it
(a PR's file need not exist locally). A diff too large for GitHub to
render is reported in the status bar.

`W` on a pull request goes one step further and checks the PR out as a
**git worktree** next to the repository, so you can run it, test it and
edit it without touching your current checkout. vig first shows the two
commands it is about to run and waits for `y`:

```text
git fetch origin refs/pull/<n>/head
git worktree add ../<repo>-pr-<n> -b pr/<n> FETCH_HEAD
```

`refs/pull/<n>/head` exists for every pull request, forks included. The
fetch runs in the background; when the worktree is in place vig switches
to the [Worktrees view](views-worktrees.md) with it selected (and, back in
the diff window, `e` now opens files from that checkout). A PR that
already has a worktree — one checked out on that branch, or at that path —
is shown there instead of being asked about again; a directory or branch
of the same name that is *not* a worktree of the PR is reported as an
error and nothing is created. The location and names come from
[`worktree-dir`](config-reference.md#worktree-dir),
[`worktree-name`](config-reference.md#worktree-name) and
[`worktree-branch`](config-reference.md#worktree-branch). vig never
removes a worktree: `git worktree remove` stays with you.

The lists show open items. `x` brings in the closed issues and the merged /
closed pull requests as well (dimmed, with their state icon: `✓` closed,
`⊕` merged; the header reads `GitHub · closed`), and `x` again leaves them
out. Both are plain `gh` list calls — no GraphQL points.
[`github-show-closed`](config-reference.md#github-show-closed) starts the
page with them included.

The issue and PR lists also keep themselves current without spending API
points: every `github-poll-interval` vig sends one *conditional* request
for the repository's most recently updated issue or PR (a `304 Not
Modified` answer is free), and only when that answer changes does it
re-fetch both lists and the open detail. The Workflow Runs column has the
same check of its own (the newest run) while no run is active, so a run
that starts shows up without `r`. The status bar shows the lists' age
(`lists 12s ago`) and a brief `↻ updated` after such a refresh. The checks
follow `github-auto-refresh`, the idle slow-down and the low-quota
throttle like every other poll.

## Workflow Runs

The third column lists the latest 50 workflow runs (`gh run list`) with their
status, workflow, run number, branch, event, duration (elapsed while running)
and age; while any run is queued or in progress the list refreshes every 5
seconds.

Selecting a run fills the detail area with its jobs and their steps nested
underneath (failed steps in red) in the **Jobs** sub-pane; `Enter` on a job
or step loads that job's log into the **Log** sub-pane, with step boundaries
and `##[group]` markers rendered as section lines. Logs of jobs that are
still running are polled every 5 seconds and followed like a tail — `]` and
`[` jump between failed steps, and `G` jumps to the end and resumes
following.

## Key bindings

| Key | Action |
|-----|--------|
| `h` / `l` | Switch between the Issues, Pull Requests and Workflow Runs columns |
| `Tab` / `Shift+Tab` | Cycle through the page's panes — the three columns and the detail (inside a detail view: its sub-panes) |
| `j` / `k` | Navigate list (the detail follows the selection) |
| `i` / `Enter` | Open detail view |
| `o` | Open in browser (issue, PR, run or the selected job) |
| `y` | Copy the item URL (built locally, no API request) |
| `Esc` | Back to list |
| `h` / `l` (detail) | Body ↔ right-hand sub-panes; for a run: Jobs ↔ Log |
| `w` (issue / PR detail) | Toggle watch mode (auto-refresh the open item) |
| `x` (issue / PR list) | Show / hide closed issues and merged / closed PRs |
| `d` (PR list / detail) | Show the PR's diff in a window: changed files + side-by-side diff (`q` / `Esc` back, `o` Files tab, `y` copy, `e` edit, `r` re-fetch) |
| `W` (PR list / detail) | Open the PR as a worktree: `git fetch` its head + `git worktree add` after a confirmation, then jump to it on the Worktrees view |
| `i` / `Enter` (run detail, Jobs) | Show the job's log (a step row scrolls to that step) |
| `]` / `[` (run detail) | Next / previous failed step in the log |
| `G` (run detail, Log) | Jump to the end and resume following |
| `/` `n` `N` | Search: `#number` / title, workflow / branch / event, or in a run detail the job and step names / log lines |
| `Ctrl+d` / `Ctrl+u` | Half page scroll (detail view) |
| `g` / `G` | Top / Bottom |
| `r` | Refresh data (in a detail view: only that item; a run re-fetches its jobs and log) |

## Constraints

- Everything goes through the `gh` CLI, so its authentication and rate limits
  apply.
- Nothing in this view reruns, cancels or deletes anything — no commenting,
  no editing, no merging. It reads.
