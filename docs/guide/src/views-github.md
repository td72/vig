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
| `Tab` / `Shift+Tab` | Cycle the columns (in a detail view: its sub-panes) |
| `j` / `k` | Navigate list (the detail follows the selection) |
| `i` / `Enter` | Open detail view |
| `o` | Open in browser (issue, PR, run or the selected job) |
| `Esc` | Back to list |
| `h` / `l` (detail) | Body ↔ right-hand sub-panes; for a run: Jobs ↔ Log |
| `w` (issue / PR detail) | Toggle watch mode (auto-refresh the open item) |
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
