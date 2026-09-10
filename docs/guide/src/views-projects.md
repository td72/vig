# Projects View

![projects demo](../../../assets/demo-projects.gif)

A read-only board for the GitHub Projects (v2) linked to the current
repository (`gh repo view --json projectsV2`), with the board itself fetched over GraphQL (a few points per board).

The board takes the full width and the first linked project shows up right
away: one column per `Status` option in GitHub's order, plus a `No status`
column for items without one. Cards show the item type (`●` issue, `⇅` pull
request, `✎` draft), number, title and assignees; a card whose item lives in
another repository carries a dimmed `owner/repo` prefix before its number.

## Several linked projects, and pinning one

With several linked projects the header reads `Board: <title> (i/n)` and
`p` / `P` cycle through them; with none the board explains how to link one
(the repository's Projects tab or `gh project link`). A top-level
`projects-board` config node pins the page to one board, by title or project
number — see
[`projects-board` in the Config Reference](config-reference.md#projects-board).

## Table mode and the detail pane

`t` switches to a table with one row per item and the project's fields
(Status, Priority, Estimate, Iteration, dates, custom text / number fields)
as sortable columns — `h` / `l` and `s` pick the sort column.

The detail pane lists every field value of the selected item, then the
issue / PR body and comments as in the GitHub view (drafts show their body).

## The optional projects list pane

A `projects` list pane also exists but is not placed by the built-in layout.
Placing it in your config gets a selectable list of the linked projects back —
see the [recipe](config-recipes.md#bring-the-projects-list-pane-back) for the
layout to paste.

## Saved views

The project's saved views (`ProjectV2.views`, fetched over GraphQL — `gh
project` does not expose them) are read together with the board. The header
shows the current view's name and layout (`Board: vig demo board · Sprint
[board] (2/3)`), and `v` / `V` cycle through them. A project without saved
views — or a views fetch that fails — falls back to the fixed `Status`
kanban. A **Table** view renders as the view defines it: its visible
fields become the columns (in the view's order, behind a `#` column),
its sort is the initial sort — descending sorts marked `▴` — and its
grouping renders one bold header row per group, `No <field>` last.

A **Board** view follows the view too: the columns come from its column
field (`verticalGroupByFields` — any single-select or iteration field, in
option order plus `No <field>`; `Status` when unset), its sort orders the
cards inside each column, and its horizontal grouping renders **swimlanes**
— one band per value with its own header line, `Space` collapses / expands
the selected lane and `j` / `k` cross between lanes at a column's edge.
A **Roadmap** view renders a timeline: item rows on the left, a time
scale on the right with one bar per item, a yellow today marker and
shaded iteration bands. Spans come from the project's date fields (a
name containing `start` / `begin` is the span start, `target` / `end` /
`due` / `finish` the end; a single date field is a point) or, for items
without dates, from the iteration field's start and duration. `+` / `-`
zoom between month, week and day scales, `h` / `l` scroll the timeline,
and `t` drops into the table and back. Items without a span are listed
without a bar. By default the timeline opens at the earliest item's start
at the week scale; [`projects-roadmap`](config-reference.md#projects-roadmap)
(`start "-7d"`, `zoom "month"`) changes where it opens and how zoomed,
per view through a local view's `roadmap { … }`.

A view's **filter** (`status:Todo -label:bug assignee:@me is:issue
no:milestone …`) is evaluated locally against the items already fetched —
no extra API call — before grouping and sorting, in every layout. Supported:
free-text title words, `field:value` with `,` lists and quoted values, `-`
negation, `is:issue|pr|draft`, `is:open|closed|merged` (a merged PR counts
as closed; an item whose state is unknown counts as open), `no:` / `has:`,
`assignee:` (`@me` is the signed-in login), `label:`, `milestone:`,
`repo:`. Ranges (`>`, `..`) and wildcards cannot be evaluated: the status
bar says `⚠ filter: unsupported "…"` and those tokens are ignored. The
status bar also counts what the filter hid (`(3 filtered out)`).

Two more filters stack on top of whichever view is shown, saved or local:
a [`projects-filter`](config-reference.md#projects-filter) expression from
the config (`projects-filter "-status:Done"` keeps done items out of every
view), and the **closed toggle** — `x` hides closed issues and merged /
closed pull requests (`· closed hidden` in the header; drafts have no state
and stay), and [`projects-hide-closed`](config-reference.md#projects-hide-closed)
starts the page that way.

## Local views

Views need not come from GitHub: a top-level
[`projects-view`](config-reference.md#projects-view) node in the config
defines one locally — its filter, grouping, sort, table columns and layout —
with no write access to the project. Local views follow the saved ones in
the `v` / `V` cycle, the header marks them `(local)`, and one can be the
view a board opens on (`default=#true`). A view can be limited to one
linked project (`board "<title>"` / `board <number>`); a field name the
board does not have is reported in the status bar and ignored rather than
being an error.

```kdl
projects-view "Mine" default=#true {
    filter "assignee:@me -status:Done"
    group-by "Status"
}
```

## Key bindings

| Key | Action |
|-----|--------|
| `p` / `P` | Next / previous linked project |
| `h` / `l`, `←` / `→` (board) | Previous / next column (table mode: sort column) |
| `j` / `k` (board) | Move between cards in a column (table mode: rows) |
| `t` (board) | Toggle table mode |
| `s` (board, table mode) | Cycle the sort column |
| `Enter` / `i` (board) | Focus the detail |
| `v` / `V` | Next / previous saved view of the project |
| `Space` | Collapse / expand the selected swimlane |
| `x` | Hide / show closed issues and merged / closed PRs |
| `+` / `-` | Zoom the roadmap time scale in / out |
| `o` | Open the project / item in the browser |
| `y` | Copy the project / item URL |
| `j` / `k` / `Ctrl+d` / `Ctrl+u` (detail) | Scroll |
| `h` / `Esc` (detail) | Back to the board |
| `Tab` / `Shift+Tab` | Cycle panes: Board → Detail |
| `/` `n` `N` | Search item titles / numbers across columns |
| `r` | Re-read the linked projects, the board and the shown item |

## Auto-refresh

While the page is shown, vig asks GitHub every
[`projects-poll-interval`](config-reference.md#projects-poll-interval)
(30 seconds by default) whether the board changed. The probe reads only the
project's `updatedAt` — one GraphQL point; moving a card or editing a
field bumps it — and re-fetches the board only when it moved, keeping the
selection, the view and the sort. The status bar shows the board's age
(`board 12s ago`) and a brief `↻ updated` after such a refresh. Coming back
to the page after five minutes still re-fetches a stale board. Both follow
`github-auto-refresh`, the idle slow-down and the low-quota throttle.

## Constraints

- `gh project` needs the `project` token scope. When it is missing the view
  shows a notice instead of the panes: run `gh auth refresh -s project`, then
  press `r`.
- Boards are fetched with two GraphQL requests — the fields, saved views
  and item count, then the items in pages sized to that count — asking
  only for what the page renders, which costs about a point per hundred
  item × field pairs (a small board: ~2 points). Past 500 items the status
  bar says `(truncated)`.
- The header warns `⚠ api N left` when fewer than 1,500 GraphQL points
  remain of the account's 5,000/hour, and automatic re-fetches slow down
  or stop on their own — see
  [Troubleshooting](troubleshooting.md#-github-rate-limited).
- Nothing in this view adds, moves, edits or deletes anything.
