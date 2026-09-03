# Projects View

![projects demo](../../../assets/demo-projects.gif)

A read-only board for the GitHub Projects (v2) linked to the current
repository (`gh repo view --json projectsV2`), built on `gh project
field-list` and `gh project item-list --format json`.

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

## Key bindings

| Key | Action |
|-----|--------|
| `p` / `P` | Next / previous linked project |
| `h` / `l`, `←` / `→` (board) | Previous / next column (table mode: sort column) |
| `j` / `k` (board) | Move between cards in a column (table mode: rows) |
| `t` (board) | Toggle table mode |
| `s` (board, table mode) | Cycle the sort column |
| `Enter` / `i` (board) | Focus the detail |
| `o` | Open the project / item in the browser |
| `y` | Copy the project / item URL |
| `j` / `k` / `Ctrl+d` / `Ctrl+u` (detail) | Scroll |
| `h` / `Esc` (detail) | Back to the board |
| `Tab` / `Shift+Tab` | Cycle panes: Board → Detail |
| `/` `n` `N` | Search item titles / numbers across columns |
| `r` | Re-read the linked projects, the board and the shown item |

## Constraints

- `gh project` needs the `project` token scope. When it is missing the view
  shows a notice instead of the panes: run `gh auth refresh -s project`, then
  press `r`.
- Boards are fetched with `--limit 500`; the status bar says `(truncated)`
  when a project has more items.
- Nothing in this view adds, moves, edits or deletes anything.
