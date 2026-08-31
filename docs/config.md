# Configuration

vig reads an optional KDL config file and merges it on top of its built-in
defaults. You only write the parts you want to change.

## Location

vig looks for a config file in this order:

1. `--config <path>` command-line flag
2. `$VIG_CONFIG` environment variable
3. `$XDG_CONFIG_HOME/vig/config.kdl`, or `~/.config/vig/config.kdl` if
   `XDG_CONFIG_HOME` is unset (on every OS, including macOS)

If the file at the default location does not exist, the built-in defaults are
used. A path given via `--config` or `$VIG_CONFIG` must exist.

```bash
vig config path          # print the path that would be used
vig config dump          # print the built-in defaults (a good starting point)
vig config dump > ~/.config/vig/config.kdl
```

## Errors

Any problem in the config file — a syntax error, an unknown page / pane /
action name, a layout that places a pane twice or places nothing — stops vig
from starting and prints a message with the file path (and line:column for
syntax errors). vig never silently falls back to the defaults when a config
file is present, so a typo cannot go unnoticed.

## Merge rules

Your file is a **partial override** of the defaults:

| Block | Rule |
|---|---|
| `theme "<name>"` | Replaces the syntax highlighting theme. |
| `icons "<mode>"` | Replaces the Files view icon mode. |
| `procs-refresh-interval "<duration>"` | Replaces how often the Procs view re-reads processes and ports. |
| `procs-history "<n>"` | Replaces how many samples the Procs history graphs keep. |
| `pages "<name>" ...` | Replaces the whole list of enabled pages and their tab order. |
| `app { }` | Merged per key. A key you set replaces the default binding for that key. |
| `page "<name>" { pane "<pane>" { keys { } } }` | Merged per key, on top of the default keys (including expanded presets). |
| `"<key>" "None"` | Removes the binding for that key. |
| `page "<name>" { layout { } }` | Replaces the whole default layout of that page. |
| `page "<name>" { tabs ... }` | Replaces the tab order. |
| `page "<name>" { bind ... }` | Replaces all select→detail bindings of that page. |

Page names (`git`, `github`, `files`, `docker`, `procs`, `worktrees`, `projects`) and pane names are fixed — you can rearrange
and rebind them, but not add new ones. Pages can be reordered or disabled
with `pages`. A replaced layout may place each pane **at most once**; a pane
it leaves out is *inactive* — it gets no area, is skipped by `tabs` cycling
and focus, and `bind` lines naming it are ignored (the Projects page ships
this way: its `projects` list pane is defined but not placed). At least one
pane must be placed.

## Example

```kdl
// ~/.config/vig/config.kdl

theme "Solarized (dark)"   // `vig config themes` lists the choices

pages "git" "files" "worktrees"   // three tabs, in this order

app {
    "q" "Quit"          // quit from anywhere, not only from a pane
}

page "git" {
    // Sidebar on the right, diff / log on the left.
    layout {
        split direction="horizontal" {
            slot "main" size="min:3" then="git_log" default="diff_view" {
                triggers "branch_list" "reflog" "git_log"
            }
            split direction="vertical" size="35%" {
                place "file_tree" size="50%"
                place "branch_list" size="30%"
                place "reflog" size="min:5"
            }
        }
    }

    pane "file_tree" {
        keys {
            "o" "ExpandOrOpen"   // add a binding
            "Space" "None"       // remove a binding
        }
    }
}
```

## Schema

### Top level

```kdl
theme "<name>"
icons "<mode>"
procs-refresh-interval "<duration>"
procs-history "<n>"
pages "<name>" "<name>" ...
app { <key> <action> ... }
page "git" { ... }
page "github" { ... }
page "files" { ... }
page "docker" { ... }
page "procs" { ... }
page "worktrees" { ... }
page "projects" { ... }
```

### `theme`

The syntax highlighting theme used in the diff view. Only the themes bundled
with [syntect](https://github.com/trishume/syntect) are available; run
`vig config themes` to list them (`*` marks the active one):

`InspiredGitHub`, `Solarized (dark)`, `Solarized (light)`,
`base16-eighties.dark` (default), `base16-mocha.dark`, `base16-ocean.dark`,
`base16-ocean.light`

Only foreground colors are taken from the theme, so the light themes are
readable mainly on a light terminal background.

### `icons`

File icons in the Files view. `"nerd"` (default) shows Nerd Font glyphs by
file type and needs a [Nerd Font](https://www.nerdfonts.com/) in your
terminal; `"none"` shows plain names.

```kdl
icons "none"
```

### `procs-refresh-interval`

How often the Procs view re-reads the process list and the listening ports
while it is shown. A number with `s` or `ms` (`"2s"` by default, `"1.5s"`,
`"500ms"`); at least `250ms`. Sampling pauses on the other views.

```kdl
procs-refresh-interval "5s"
```

### `procs-history`

How many samples the Procs view's history graphs keep — the system CPU /
memory area charts in the `graphs` pane and the per-process history charts
in the detail pane. One sample is taken per refresh interval; between `"10"` and
`"10000"`. The default `"120"` is 4 minutes of history at the default
`"2s"` interval.

```kdl
procs-history "300"
```

### `pages`

Which pages are enabled, in tab order. The position in the list is the
page's *slot* — the number shown in the header (`1:Git`, `2:GitHub`, …) and
the page reached by `Tab` cycling. The default lists every page:

```kdl
pages "git" "github" "files" "docker" "procs" "worktrees" "projects"
```

Your `pages` replaces this list wholesale. Pages you leave out are disabled
(not started, no tab), so

```kdl
pages "git" "files" "worktrees"
```

gives a three-tab vig with `1:Git 2:Files 3:Worktrees`. Unknown or repeated
names are errors.

Keys are bindings *onto* pages, not slots: `app { "<key>" "page:<name>" }`
keeps addressing a page by name wherever it sits, so the built-in `1` … `7`
still switch to the same pages after reordering (the help overlay lists them
as `1 / 3 / 6` in the example above). Built-in keys of disabled pages are
dropped; a binding in *your* `app { }` block to a page that is not listed in
`pages` is an error.

The `actions` page of v0.7.0 was folded into the `github` page (its Workflow
Runs column) in v0.8.0. A config that still lists `actions` in `pages` or
binds `page:actions` is rejected with a message saying so — remove it.

### `app`

Global bindings that work on every page.

| Action | Meaning |
|---|---|
| `"Quit"` | Quit vig |
| `"page:git"`, `"page:github"`, `"page:files"`, `"page:docker"`, `"page:procs"`, `"page:worktrees"`, `"page:projects"` | Switch to that page (it must be listed in `pages`) |
| `"None"` | Unbind the key |

### Keys

A key is written as a string: a single character (`"j"`, `"G"`, `"/"`), a
named key (`"Enter"`, `"Esc"`, `"Tab"`, `"BackTab"`, `"Space"`, `"Backspace"`,
`"Delete"`, `"Up"`, `"Down"`, `"Left"`, `"Right"`, `"Home"`, `"End"`,
`"PageUp"`, `"PageDown"`), or a `Ctrl+` combination (`"Ctrl+c"`, `"Ctrl+d"`).

### `page`

```kdl
page "git" {
    layout { <split | place | slot> }
    tabs "<pane>" "<pane>" ...
    bind select="<pane>" detail="<pane>"
    pane "<name>" { keys { ... } }
}
```

- `layout` — exactly one root element:
  - `split direction="horizontal|vertical" { <children> }` — `horizontal`
    lays children side by side, `vertical` stacks them. Each child may take
    `size="..."`.
  - `place "<pane>"` — show a pane.
  - `slot "<name>" then="<pane>" default="<pane>" { triggers "<pane>" ... }`
    — an area that shows `then` while one of the `triggers` panes has focus,
    and `default` otherwise (e.g. git log vs. diff view).
  - `slot "<name>" default="<pane>" { when "<pane>" ... then="<pane>"; ... }`
    — the same with several cases: the first `when` whose panes include the
    focused pane wins, `default` shows otherwise (the GitHub detail area:
    `pr_detail` for the PR column, `run_detail` for the runs column,
    `issue_detail` by default). Both forms may be combined.
  - Sizes: `"30"` (cells), `"40%"`, `"min:20"`. Defaults to `"min:0"`.
  - Each pane may be placed at most once; a pane the layout leaves out is
    inactive (no area, no focus).
- `tabs` — the panes cycled by `Tab` / `BackTab`, in order. Panes the layout
  does not place are skipped, so the default `tabs` stays valid under your
  layout.
- `bind` — which detail pane a selection pane drives (e.g. selecting a file
  in `file_tree` loads it into `diff_view`). A `bind` naming an unplaced
  pane is ignored, and starts applying once your layout places the pane.
- `pane "<name>" { keys { } }` — key bindings for that pane. `preset "nav"`
  and `preset "search"` expand to the standard navigation / search keys.

### Panes and actions

Run `vig config dump` to see every pane with its default keys. Action names
are listed per pane below; `Nav.*` and `Search.*` are available in every pane
that has the corresponding preset.

**Page `git`**

| Pane | Actions |
|---|---|
| `view` (page-wide) | `Quit`, `Help`, `Refresh`, `PrevTab`, `NextTab`, `CyclePaneForward`, `CyclePaneBackward`, `OpenEditor` |
| `file_tree` | `ToggleDir`, `ExpandOrOpen`, `FocusDiff`, `Esc` |
| `branch_list` | `OpenActionMenu`, `FocusLog`, `Esc` |
| `git_log` | `YankHash`, `OpenGitHub`, `FocusReflog`, `Esc` |
| `reflog` | `SetDiffBase`, `FocusLog`, `Esc` |
| `diff_view` | `ScrollLeft`, `ScrollRight`, `EnterNormalMode`, `Esc` |

**Page `github`**

| Pane | Actions |
|---|---|
| `view` (page-wide) | `Quit`, `Help`, `Refresh`, `PrevTab`, `NextTab`, `CyclePaneForward`, `CyclePaneBackward` |
| `issue_list`, `pr_list`, `run_list` | `OpenDetail`, `SwitchTab`, `OpenBrowser`, `Esc` |
| `issue_detail`, `pr_detail` | `FocusBody`, `FocusRight`, `CycleForward`, `CycleBackward`, `ToggleWatch`, `OpenItem`, `Esc` |
| `run_detail` | `FocusBody` (Jobs), `FocusRight` (Log), `CycleForward`, `CycleBackward`, `OpenLog`, `NextFailed`, `PrevFailed`, `OpenItem`, `Esc` (`Nav.JumpBottom` resumes following the log) |

**Page `files`**

| Pane | Actions |
|---|---|
| `view` (page-wide) | `Quit`, `Help`, `Refresh`, `CyclePaneForward`, `CyclePaneBackward`, `OpenEditor` |
| `parent_dir` | display-only (no keys) |
| `dir_list` | `Enter`, `Parent`, `FocusPreview`, `Esc` |
| `preview` | `Back`, `Esc` |

**Page `docker`**

| Pane | Actions |
|---|---|
| `view` (page-wide) | `Quit`, `Help`, `Refresh`, `CyclePaneForward`, `CyclePaneBackward` |
| `containers` | `OpenDetail`, `FocusLogs`, `Esc` |
| `images` | `OpenDetail`, `Esc` |
| `detail` | `Back`, `Esc` |
| `logs` | `Back`, `Esc` (`Nav.JumpBottom` resumes following) |

**Page `procs`**

| Pane | Actions |
|---|---|
| `view` (page-wide) | `Quit`, `Help`, `Refresh`, `CyclePaneForward`, `CyclePaneBackward` |
| `processes` | `FocusDetail`, `CycleSort`, `TogglePerCore`, `Esc` |
| `ports` | `JumpToProcess`, `Esc` |
| `detail` | `Back`, `Esc` |
| `graphs` | `TogglePerCore`, `Esc` |

**Page `worktrees`**

| Pane | Actions |
|---|---|
| `view` (page-wide) | `Quit`, `Help`, `Refresh`, `CyclePaneForward`, `CyclePaneBackward` |
| `worktrees`, `stashes` | `FocusPreview`, `Esc` |
| `preview` | `ScrollLeft`, `ScrollRight`, `EnterNormalMode`, `NextFile`, `PrevFile`, `Back`, `Esc` |

**Page `projects`**

The built-in layout places only the board and the detail; the `projects`
list pane is defined but **not placed** (`p` / `P` switch between the
linked projects instead). To get the list back, place it — the built-in
`bind select="projects" detail="board"` then applies on its own:

```kdl
page "projects" {
    layout {
        split direction="horizontal" {
            place "projects" size="22%"
            split direction="vertical" size="min:30" {
                place "board" size="60%"
                place "detail" size="min:5"
            }
        }
    }
    tabs "projects" "board" "detail"
}
```

| Pane | Actions |
|---|---|
| `view` (page-wide) | `Quit`, `Help`, `Refresh`, `NextProject` (`p`), `PrevProject` (`P`), `CyclePaneForward`, `CyclePaneBackward` |
| `projects` (optional) | `OpenBoard`, `OpenBrowser`, `Esc` |
| `board` | `PrevColumn`, `NextColumn` (table mode: sort column), `ToggleTable`, `CycleSort`, `OpenDetail`, `OpenBrowser`, `Esc` (back to the project list when it is placed) |
| `detail` | `Back`, `OpenBrowser`, `Esc` |

**Presets**

| Preset | Keys |
|---|---|
| `nav` | `j`/`Down` `Nav.MoveDown`, `k`/`Up` `Nav.MoveUp`, `Ctrl+d` `Nav.HalfPageDown`, `Ctrl+u` `Nav.HalfPageUp`, `g` `Nav.JumpTop`, `G` `Nav.JumpBottom` |
| `search` | `/` `Search.Start`, `n` `Search.Next`, `N` `Search.Prev` |
