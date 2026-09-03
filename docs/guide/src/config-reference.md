# Config Reference

The complete reference for vig's KDL configuration: every node the config
file accepts, with its form, default, merge rule and errors. It is organized
for lookup — one section per top-level node, then the `page` block elements,
then every page with its panes and actions.

If you are reading about the config for the first time, start with
[Configuration Basics](configuration-basics.md) (locations, layers, merge
model) and [Config Recipes](config-recipes.md) (worked examples). This
chapter assumes those and aims for completeness.

Conventions used below:

- **Form** — what the node looks like. All values are quoted strings unless
  noted; the one exception is `projects-board 2` (a bare integer).
- **Default** — the built-in value, as shipped in
  [assets/default.kdl](https://github.com/td72/vig/blob/main/assets/default.kdl)
  (`vig config dump` prints it).
- **Merge** — what happens when your config states the node.
- Complete, loadable examples are shown as `kdl` blocks — vig's test suite
  loads each one exactly the way a user config is loaded. Fragments and
  error demonstrations are marked as ignored and annotated with the error
  they produce.

## Top level at a glance

| Node | Default | Merge rule |
|---|---|---|
| [`theme`](#theme) | `"base16-eighties.dark"` | replaced |
| [`icons`](#icons) | `"nerd"` | replaced |
| [`image-preview`](#image-preview) | `"auto"` | replaced |
| [`markdown-preview`](#markdown-preview) | `"render"` | replaced |
| [`procs-refresh-interval`](#procs-refresh-interval) | `"2s"` | replaced |
| [`procs-history`](#procs-history) | `"120"` | replaced |
| [`github-poll-interval`](#github-poll-interval) | `"5s"` | replaced |
| [`projects-board`](#projects-board) | absent (all linked boards) | replaced |
| [`pages`](#pages) | all seven pages | replaced wholesale |
| [`repo-config`](#repo-config) | `"on"` | replaced (user config only) |
| [`app`](#app) | `Ctrl+c` quit, `1`…`7` page switch | merged per key |
| [`page`](#the-page-block) | see [Pages and panes](#pages-and-panes) | per element, see below |

A config stating every top-level node (each at its default here, so this
loads and changes nothing):

```kdl
theme "base16-eighties.dark"
icons "nerd"
image-preview "auto"
markdown-preview "render"
procs-refresh-interval "2s"
procs-history "120"
github-poll-interval "5s"
pages "git" "github" "files" "docker" "procs" "worktrees" "projects"
repo-config "on"
app {
    "Ctrl+c" "Quit"
}
```

Anything else at the top level is an error:

```kdl,ignore
colors "red"
// → unknown top-level block "colors" (expected `theme`, `icons`,
//   `image-preview`, `markdown-preview`, `procs-refresh-interval`,
//   `procs-history`, `github-poll-interval`, `projects-board`, `pages`,
//   `repo-config`, `app`, or `page`)
```

## Top-level nodes

### `theme`

The syntax highlighting theme used by the diff views (Git and Worktrees) and
the Files preview.

- **Form** — `theme "<name>"`
- **Default** — `"base16-eighties.dark"`
- **Merge** — replaces the default.

Only the themes bundled with
[syntect](https://github.com/trishume/syntect) are available; run
`vig config themes` to list them (`*` marks the active one):
`InspiredGitHub`, `Solarized (dark)`, `Solarized (light)`,
`base16-eighties.dark`, `base16-mocha.dark`, `base16-ocean.dark`,
`base16-ocean.light`. Only foreground colors are taken from the theme, so
the light themes are readable mainly on a light terminal background.

```kdl
theme "Solarized (dark)"
```

```kdl,ignore
theme "Solarised (dark)"
// → unknown theme "Solarised (dark)"; available: InspiredGitHub, ...
```

### `icons`

File-type icons in the Files view.

- **Form** — `icons "<mode>"` — `"nerd"` or `"none"`
- **Default** — `"nerd"`
- **Merge** — replaces the default.

`"nerd"` shows Nerd Font glyphs by file type and needs a
[Nerd Font](https://www.nerdfonts.com/) in your terminal; `"none"` shows
plain names ([recipe](config-recipes.md#turn-off-file-icons)).

```kdl
icons "none"
```

### `image-preview`

How the Files view renders image previews (PNG / JPEG / GIF / WebP).

- **Form** — `image-preview "<mode>"` — `"auto"`, `"halfblocks"` or `"none"`
- **Default** — `"auto"`
- **Merge** — replaces the default.

`"auto"` probes the terminal for a graphics protocol (Kitty, iTerm2, Sixel)
and falls back to unicode halfblocks; `"halfblocks"` skips the detection and
always uses halfblocks; `"none"` renders no image at all (the preview still
shows the image's metadata line).

```kdl
image-preview "halfblocks"
```

### `markdown-preview`

How the Files view previews Markdown files (`.md` / `.markdown`).

- **Form** — `markdown-preview "<mode>"` — `"render"` or `"raw"`
- **Default** — `"render"`
- **Merge** — replaces the default.

`"render"` shows the rendered form (headings, emphasis, lists, code, GFM
tables fitted to the pane width); `"raw"` shows the plain
syntax-highlighted text. `m` toggles between the two for the session either
way.

```kdl
markdown-preview "raw"
```

### `procs-refresh-interval`

How often the Procs view re-reads the process list and the listening ports
while it is shown. Sampling pauses on the other views.

- **Form** — `procs-refresh-interval "<duration>"` — a number with `s` or
  `ms` (`"2s"`, `"1.5s"`, `"500ms"`), quoted; at least `"250ms"`
- **Default** — `"2s"`
- **Merge** — replaces the default.

```kdl
procs-refresh-interval "5s"
```

```kdl,ignore
procs-refresh-interval "100ms"
// → bad procs-refresh-interval "100ms"; expected a duration such as
//   "2s" or "500ms" (at least 250ms)
```

### `procs-history`

How many samples the Procs view's history graphs keep — the system CPU /
memory area charts in the `graphs` pane and the per-process history charts
in the detail pane. One sample is taken per refresh interval, so this and
[`procs-refresh-interval`](#procs-refresh-interval) multiply into a time
span.

- **Form** — `procs-history "<n>"` — a quoted number between `"10"` and
  `"10000"`
- **Default** — `"120"` (4 minutes of history at the default `"2s"`)
- **Merge** — replaces the default.

```kdl
procs-history "300"
```

```kdl,ignore
procs-history "5"
// → bad procs-history "5"; expected a sample count between 10 and 10000
```

### `github-poll-interval`

How often the GitHub page polls while something is active — the Workflow
Runs column while a run is queued or in progress, a PR's checks in watch
mode (`w`), and the log of a running job. Polling pauses while another page
is shown.

- **Form** — `github-poll-interval "<duration>"` — a number with `s` or
  `ms`, quoted; at least `"2s"`, so a config cannot burn through the API
  quota
- **Default** — `"5s"`
- **Merge** — replaces the default.

Rate-limit handling is built in on top of this setting: when GitHub rejects
a request as rate-limited, the page suspends all its polling with an
exponential backoff (30s, 60s, … capped at 10 minutes) and shows
`⚠ GitHub rate limited (resets in Nm)` in the status bar. The reset time
comes from one `gh api rate_limit` call (that endpoint is not
rate-limited). `r` retries immediately; a successful fetch clears the
backoff. See [Troubleshooting](troubleshooting.md#-github-rate-limited)
for the full story.

```kdl
github-poll-interval "10s"
```

### `projects-board`

Pins the Projects page to one board. The single argument is either a board
title (a string, matched case-insensitively against the projects linked to
the repository) or a project number — the config's one bare integer:

- **Form** — `projects-board "<title>"` or `projects-board <number>`
- **Default** — absent: every linked project is available and `p` / `P`
  cycle through them
- **Merge** — replaces the default.

```kdl
projects-board "Roadmap"
```

```kdl
projects-board 2
```

When set, the page shows only that board: `p` / `P` no longer cycle
(pressing them shows `board pinned by config (projects-board)` in the
status bar) and the header shows the title without the `(i/n)` counter.
When no linked project matches the pin, the board pane shows a notice
naming it.

```kdl,ignore
projects-board "Roadmap" 2
// → bad projects-board (one argument required); expected exactly one
//   argument, a board title (`projects-board "Roadmap"`) or a project
//   number (`projects-board 2`)
```

### `pages`

Which pages are enabled, in tab order. The position in the list is the
page's *slot* — the number shown in the header (`1:Git`, `2:GitHub`, …) and
the position `Tab` cycling reaches.

- **Form** — `pages "<name>" "<name>" ...` — names from `git`, `github`,
  `files`, `docker`, `procs`, `worktrees`, `projects`; at least one, no
  repeats
- **Default** — all seven, in that order
- **Merge** — replaces the default list **wholesale**.

Pages you leave out are disabled — not started, no tab, no background
polling:

```kdl
pages "git" "files" "worktrees"
```

gives a three-tab vig with `1:Git 2:Files 3:Worktrees`.

Keys are bindings *onto* pages, not slots: `app { "<key>" "page:<name>" }`
keeps addressing a page by name wherever it sits, so the built-in `1` … `7`
still switch to the same pages after reordering. Built-in keys of disabled
pages are dropped; a binding in *your* `app { }` block to a page that is
not listed in `pages` is an error (see [`app`](#app)).

```kdl,ignore
pages "git" "filez"
// → pages: unknown page "filez"; expected one of: git, github, files,
//   docker, procs, worktrees, projects

pages "git" "git"
// → pages: page "git" listed twice

pages
// → pages must list at least one page
```

The `actions` page of v0.7.0 was folded into the `github` page (its
Workflow Runs column) in v0.8.0. A config that still lists it is rejected
with a message saying so:

```kdl,ignore
pages "git" "actions"
// → pages: page "actions" was folded into the "github" page (v0.8.0);
//   remove it from pages / app bindings
```

### `repo-config`

Whether the repository-local `.vig.kdl` layer
([Configuration Basics](configuration-basics.md#3--repo-local-vigkdl)) is
read at all.

- **Form** — `repo-config "on"` or `repo-config "off"`
- **Default** — `"on"`
- **Merge** — replaces the default; **only the user config's value
  counts**.

With `"off"` the `.vig.kdl` file is never loaded and the trust dialog never
appears. The switch is read before the repo layer is merged, so a
`.vig.kdl` cannot turn itself on or off — a `.vig.kdl` that contains
`repo-config` at all (even `"on"`) is rejected:
`repo-config can only be set in the user config, not in .vig.kdl`.

```kdl
repo-config "off"
```

### `app`

Global key bindings that work on every page.

- **Form** — `app { "<key>" "<action>" ... }`
- **Default** — `"Ctrl+c" "Quit"` and `"1"` … `"7"` bound to the seven
  pages by name
- **Merge** — merged per key: a key you set replaces the default binding
  for that key; keys you do not mention keep theirs.

| Action | Meaning |
|---|---|
| `"Quit"` | Quit vig |
| `"page:<name>"` | Switch to that page — `page:git`, `page:github`, `page:files`, `page:docker`, `page:procs`, `page:worktrees`, `page:projects`. The page must be listed in [`pages`](#pages). |
| `"None"` | Remove the binding for that key |

```kdl
app {
    "q" "Quit"            // quit from anywhere, not only from a pane
    "Ctrl+g" "page:git"   // jump to the Git view
    "7" "None"            // unbind the built-in Projects switch
}
```

A `page:` binding of your own naming a page that exists but is not enabled
is an error (built-in bindings of disabled pages are silently dropped
instead):

```kdl,ignore
pages "git" "files"
app {
    "d" "page:docker"
    // → app block: "d" "page:docker": page "docker" is not listed in
    //   `pages` (git, files)
}
```

## Keys

A key is written as a string, in one of three forms:

- **A single character** — `"j"`, `"G"`, `"/"`, `"]"`. Case matters:
  `"g"` and `"G"` are different keys.
- **A named key** — `"Enter"`, `"Esc"`, `"Tab"`, `"BackTab"`, `"Space"`,
  `"Backspace"`, `"Delete"`, `"Up"`, `"Down"`, `"Left"`, `"Right"`,
  `"Home"`, `"End"`, `"PageUp"`, `"PageDown"`. A few aliases are accepted:
  `"Return"` / `"CR"` for Enter, `"Escape"` for Esc, `"S-Tab"` for BackTab,
  `"BS"` for Backspace, `"Del"` for Delete.
- **A `Ctrl+` combination** — `"Ctrl+d"`, `"Ctrl+u"`, `"Ctrl+c"`. `Ctrl` is
  the only supported modifier; there are no `Alt+` or function keys.

Binding a key to the reserved action `"None"` removes it — in `app { }` and
in any pane's `keys { }` block alike.

## Presets

A preset is a named bundle of standard bindings, expanded in place inside a
pane's `keys { }` block. Two exist:

| Preset | Expands to |
|---|---|
| `nav` | `j`/`Down` → `Nav.MoveDown`, `k`/`Up` → `Nav.MoveUp`, `Ctrl+d` → `Nav.HalfPageDown`, `Ctrl+u` → `Nav.HalfPageUp`, `g` → `Nav.JumpTop`, `G` → `Nav.JumpBottom` |
| `search` | `/` → `Search.Start`, `n` → `Search.Next`, `N` → `Search.Prev` |

The `Nav.*` and `Search.*` actions can also be bound individually, in any
pane that has the corresponding preset in its defaults. Two rules govern
presets ([why](config-recipes.md#what-presets-are)): presets expand first
and explicit bindings in the same pane win for the same key; and when your
keys merge into a pane, `preset` lines are appended, never replaced.

```kdl
page "git" {
    pane "diff_view" {
        keys {
            "J" "Nav.HalfPageDown"   // bind a preset action explicitly
            "n" "None"               // remove a preset-provided binding
        }
    }
}
```

## The `page` block

```kdl,ignore
page "<name>" {
    layout { <split | place | slot> }     // replaced wholesale
    tabs "<pane>" "<pane>" ...            // replaced wholesale
    bind select="<pane>" detail="<pane>"  // all bind lines replaced together
    pane "<name>" { keys { ... } }        // keys merged per key
}
```

Page names and pane names are fixed — you can rearrange, resize and rebind
them, but not add new ones. Every block is optional; a `page` block only
changes what it states.

### `layout`

Exactly one root element, of three kinds, nested to any depth:

- `split direction="horizontal" { <children> }` — lays its children side by
  side; `direction="vertical"` stacks them. Each child may take
  `size="..."`.
- `place "<pane>"` — shows a pane.
- `slot "<name>" ...` — one area that shows different panes depending on
  focus; see [Slots](#slots) below.

**Sizes** — `"30"` (exactly 30 cells), `"40%"` (percentage), `"min:20"` (at
least 20 cells, take the leftovers). Omitted means `"min:0"`.

**Merge** — a `layout` you write replaces the page's **whole** layout;
there is no partial layout merge. Start from the page's block in
`vig config dump` and edit
([recipes](config-recipes.md#reading-a-layout-tree)).

**Rules**, both checked at startup:

- Each pane may be placed **at most once** — counting `place` lines and,
  for a slot, each distinct pane the slot can show.
- At least one pane must be placed.
- A pane the layout leaves out is **inactive**: it gets no area, `Tab`
  cycling and focus skip it, and `bind` lines naming it are ignored. The
  built-in Projects page ships this way — see
  [Page `projects`](#page-projects).

```kdl,ignore
page "git" {
    layout {
        split direction="vertical" {
            place "diff_view"
            place "diff_view"
        }
    }
}
// → page "git": layout places pane "diff_view" more than once

page "git" { layout { } }
// → page "git" layout is empty
```

#### Slots

A `slot` is a layout area that shows different panes at different times:
whichever case matches the currently focused pane wins. Two forms, which
can be combined in one slot:

- **Single case** — `then=` on the slot plus a `triggers` child:

  ```kdl,ignore
  slot "main" size="min:3" then="git_log" default="diff_view" {
      triggers "branch_list" "reflog" "git_log"
  }
  ```

  Shows `git_log` while `branch_list`, `reflog` or `git_log` has focus,
  and `diff_view` otherwise (the Git view's bottom area).

- **Multi case** — `when` children, each listing trigger panes and naming
  the pane to show; the first `when` whose triggers include the focused
  pane wins, `default=` shows when none does:

  ```kdl,ignore
  slot "detail" size="min:3" default="issue_detail" {
      when "pr_list" "pr_detail" then="pr_detail"
      when "run_list" "run_detail" then="run_detail"
  }
  ```

  The GitHub view's detail area. Note each `when` names its `then` pane
  among its own triggers, so moving focus *into* the detail does not
  switch the area away from it.

The slot's name (`"main"`, `"detail"`) is just a label. For the
at-most-once placement rule, a slot counts each distinct pane it can show
as placed once. A worked walk-through is in the
[slot recipes](config-recipes.md#slots-one-area-several-panes).

### `tabs`

The panes cycled by `Tab` / `BackTab`, in order.

- **Form** — `tabs "<pane>" "<pane>" ...`
- **Merge** — replaced wholesale when present.

Panes the layout does not place are skipped, so the default `tabs` stays
valid under your layout — you only need to restate `tabs` to change the
cycle order or to include a pane the default order lacks.

### `bind`

Which detail pane a selection pane drives — e.g. selecting a file in
`file_tree` loads it into `diff_view`.

- **Form** — `bind select="<pane>" detail="<pane>"`, repeatable
- **Merge** — one user `bind` line replaces **all** of the page's default
  `bind` lines.

A `bind` naming an unplaced pane is ignored — and starts applying on its
own once a layout places the pane (this is how the Projects page's list
pane comes alive; see [Page `projects`](#page-projects)).

### `pane` and `keys`

```kdl
page "git" {
    pane "file_tree" {
        keys {
            "o" "ExpandOrOpen"   // add or override a binding
            "Space" "None"       // remove one
        }
    }
}
```

- **Merge** — per key, on top of the pane's default keys (including
  expanded presets). `preset` lines are appended.

Each pane accepts its own set of actions, listed per page below. The pane
named `view` is special: it is not a real pane but the holder of the
page-wide keys (quit, help, refresh, tab and pane cycling), and it can
never be placed in a layout. The help overlay (`?`) is generated from the
merged keymap, so it always reflects your bindings.

## Pages and panes

For each page: its panes, each pane's bindable actions, and the built-in
key for each action. `Nav.*` and `Search.*` (see [Presets](#presets)) are
additionally available in every pane whose defaults include the
corresponding preset — below, panes with `nav` and `search` presets are
marked. `Esc` is an action of every interactive pane (leave the pane /
clear the search). Run `vig config dump` to see every default binding in
its KDL form.

### Page `git`

Panes: `file_tree`, `branch_list`, `git_log`, `reflog`, `diff_view` — all
placed by the default layout (`git_log` and `diff_view` share the `main`
slot).

| Pane | Action | Default key | Meaning |
|---|---|---|---|
| `view` | `Quit` / `Help` / `Refresh` | `q` / `?` / `r` | page-wide |
| | `PrevTab` / `NextTab` | `h` / `l` | move between the sidebar panes |
| | `CyclePaneForward` / `CyclePaneBackward` | `Tab` / `BackTab` | cycle the `tabs` panes |
| | `OpenEditor` | `e` | open the selected file in `$EDITOR` |
| `file_tree` (nav, search) | `ToggleDir` | `Space` | expand / collapse a directory |
| | `ExpandOrOpen` | `Enter`, `Right` | expand a directory / open a file's diff |
| | `FocusDiff` | `i` | focus the diff view |
| `branch_list` (nav, search) | `OpenActionMenu` | `Enter` | switch / safe-delete / set as diff base |
| | `FocusLog` | `i` | focus the git log |
| `git_log` (nav, search) | `YankHash` | `y` | copy the commit hash |
| | `YankUrl` | `Y` | copy the commit URL |
| | `OpenGitHub` | `o` | open the commit on GitHub |
| | `FocusReflog` | `h` | focus the reflog |
| `reflog` (nav, search) | `SetDiffBase` | `Enter` | diff the working tree against this entry |
| | `FocusLog` | `i` | focus the git log |
| `diff_view` (nav, search) | `ScrollLeft` / `ScrollRight` | `h`, `Left` / `l`, `Right` | horizontal scroll |
| | `EnterNormalMode` | `i` | vim-style Normal mode (cursor, yank, visual) |

### Page `github`

Panes: `issue_list`, `pr_list`, `run_list` (the three columns) and
`issue_detail`, `pr_detail`, `run_detail` (sharing the `detail` slot).

| Pane | Action | Default key | Meaning |
|---|---|---|---|
| `view` | `Quit` / `Help` / `Refresh` | `q` / `?` / `r` | page-wide |
| | `PrevTab` / `NextTab` | `h` / `l` | move between the columns |
| | `CyclePaneForward` / `CyclePaneBackward` | `Tab` / `BackTab` | cycle columns and detail |
| `issue_list`, `pr_list`, `run_list` (nav, search) | `OpenDetail` | `i`, `Enter` | open the detail view |
| | `SwitchTab` | `Tab` (issues) / `BackTab` (PRs, runs) | column-local tab switch |
| | `OpenBrowser` | `o` | open the item in the browser |
| | `CopyUrl` | `y` | copy the item URL |
| `issue_detail`, `pr_detail` (nav) | `FocusBody` / `FocusRight` | `h` / `l` | body ↔ right-hand sub-panes |
| | `CycleForward` / `CycleBackward` | `Tab` / `BackTab` | cycle the sub-panes |
| | `ToggleWatch` | `w` | watch mode: auto-refresh the open item |
| | `OpenItem` | `o` | open in the browser |
| | `CopyUrl` | `y` | copy the item URL |
| `run_detail` (nav, search) | `FocusBody` / `FocusRight` | `h` / `l` | Jobs ↔ Log sub-panes |
| | `CycleForward` / `CycleBackward` | `Tab` / `BackTab` | cycle the sub-panes |
| | `OpenLog` | `i`, `Enter` | show the selected job's log |
| | `NextFailed` / `PrevFailed` | `]` / `[` | jump between failed steps |
| | `OpenItem` | `o` | open the run / job in the browser |
| | `CopyUrl` | `y` | copy the run URL |

In `run_detail`, `Nav.JumpBottom` (`G`) also resumes following a running
job's log.

### Page `files`

Panes: `parent_dir`, `dir_list`, `preview` — all placed. `parent_dir` is
**display-only**: it has a `pane` block with no keys and accepts none.

| Pane | Action | Default key | Meaning |
|---|---|---|---|
| `view` | `Quit` / `Help` / `Refresh` | `q` / `?` / `r` | page-wide |
| | `CyclePaneForward` / `CyclePaneBackward` | `Tab` / `BackTab` | cycle `dir_list` and `preview` |
| | `OpenEditor` | `e` | open the selected file in `$EDITOR` |
| | `OpenDefault` | `o` | open with the OS default app |
| | `OpenWith` | `O` | open with an app you name |
| `dir_list` (nav, search) | `Enter` | `l`, `Right`, `Enter` | enter directory / focus preview |
| | `Parent` | `h`, `Left`, `Backspace` | go to the parent directory |
| | `FocusPreview` | `i` | focus the preview |
| `preview` (nav) | `Back` | `h`, `Left` | back to the file list |

### Page `docker`

Panes: `containers`, `images`, `detail`, `logs` — all placed.

| Pane | Action | Default key | Meaning |
|---|---|---|---|
| `view` | `Quit` / `Help` / `Refresh` | `q` / `?` / `r` | page-wide |
| | `CyclePaneForward` / `CyclePaneBackward` | `Tab` / `BackTab` | cycle the panes |
| `containers` (nav, search) | `OpenDetail` | `i`, `Enter` | focus the inspect summary |
| | `FocusLogs` | `l` | focus the log tail |
| `images` (nav, search) | `OpenDetail` | `i`, `Enter` | focus the inspect summary |
| `detail` (nav) | `Back` | `h` | back to the list |
| `logs` (nav, search) | `Back` | `h` | back to the list |

In `logs`, `Nav.JumpBottom` (`G`) also resumes following the tail.

### Page `procs`

Panes: `processes`, `ports`, `detail`, `graphs` — all placed.

| Pane | Action | Default key | Meaning |
|---|---|---|---|
| `view` | `Quit` / `Help` / `Refresh` | `q` / `?` / `r` | page-wide |
| | `CyclePaneForward` / `CyclePaneBackward` | `Tab` / `BackTab` | cycle the panes |
| `processes` (nav, search) | `FocusDetail` | `i`, `l`, `Enter` | focus the process detail |
| | `CycleSort` | `s` | sort by CPU → MEM → PID |
| | `TogglePerCore` | `c` | CPU graph: history ⇄ per-core bars |
| `ports` (nav, search) | `JumpToProcess` | `Enter` | jump to the owning process |
| `detail` (nav) | `Back` | `h`, `Left` | back to the process list |
| `graphs` | `TogglePerCore` | `c` | CPU graph: history ⇄ per-core bars |

### Page `worktrees`

Panes: `worktrees`, `stashes`, `preview` — all placed.

| Pane | Action | Default key | Meaning |
|---|---|---|---|
| `view` | `Quit` / `Help` / `Refresh` | `q` / `?` / `r` | page-wide |
| | `CyclePaneForward` / `CyclePaneBackward` | `Tab` / `BackTab` | cycle the panes |
| `worktrees`, `stashes` (nav, search) | `FocusPreview` | `i`, `l`, `Enter` | focus the preview |
| `preview` (nav, search) | `ScrollLeft` / `ScrollRight` | `h`, `Left` / `l`, `Right` | horizontal scroll |
| | `EnterNormalMode` | `i` | Normal mode in the stash diff |
| | `NextFile` / `PrevFile` | `]` / `[` | next / previous file in a stash |
| | `Back` | `Backspace` | back to the list |

### Page `projects`

Panes: `projects`, `board`, `detail`. The built-in layout places only
`board` and `detail`; the `projects` list pane is defined but **not
placed** — `p` / `P` cycle the linked projects instead, and a top-level
[`projects-board`](#projects-board) pins one board and disables them. To
get the list back, place it — the built-in
`bind select="projects" detail="board"` then applies on its own
([recipe](config-recipes.md#bring-the-projects-list-pane-back)):

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

| Pane | Action | Default key | Meaning |
|---|---|---|---|
| `view` | `Quit` / `Help` / `Refresh` | `q` / `?` / `r` | page-wide |
| | `NextProject` / `PrevProject` | `p` / `P` | cycle the linked projects |
| | `CyclePaneForward` / `CyclePaneBackward` | `Tab` / `BackTab` | cycle the panes |
| `projects` (nav, search) | `OpenBoard` | `i`, `l`, `Enter` | show the selected project's board |
| | `OpenBrowser` | `o` | open the project in the browser |
| | `CopyUrl` | `y` | copy the project URL |
| `board` (nav, search) | `PrevColumn` / `NextColumn` | `h`, `Left` / `l`, `Right` | move between columns (table mode: sort column) |
| | `ToggleTable` | `t` | board ⇄ table mode |
| | `CycleSort` | `s` | cycle the sort column (table mode) |
| | `NextView` / `PrevView` | `v` / `V` | cycle the project's saved views |
| | `OpenDetail` | `i`, `Enter` | focus the item detail |
| | `OpenBrowser` | `o` | open the item in the browser |
| | `CopyUrl` | `y` | copy the item URL |
| `detail` (nav) | `Back` | `h`, `Left` | back to the board |
| | `OpenBrowser` | `o` | open the item in the browser |

On the board, `Esc` returns to the project list when it is placed.

## Startup errors

Any problem in the user config stops vig from starting and prints a message
naming the file — vig never silently falls back to the defaults when a
config file is present. The categories:

- **Syntax errors** — reported with `file:line:column` and the parser's
  message, e.g. for a `page "git" {` left unclosed:

  ```text
  Error: failed to parse config file /home/you/.config/vig/config.kdl
    /home/you/.config/vig/config.kdl:1:12: No closing '}' for child block
  ```

- **Unknown names** — top-level blocks, pages, panes, themes, icon /
  image-preview modes, preset names, keys and actions are all validated;
  each error lists what was expected. Typos cannot go unnoticed.
- **Structural errors** — a layout that places a pane twice or places
  nothing, a slot without `then=` / `when` cases, a `bind` without
  `select=` / `detail=`.
- **Value errors** — an interval below its minimum or without a unit, a
  `procs-history` out of range, a `projects-board` with the wrong argument
  shape, a `repo-config` other than `"on"` / `"off"`.
- **Cross-references** — your `app` binding to a page not listed in
  `pages`, or to the removed `actions` page.

The repository-local `.vig.kdl` layer is the one exception: it degrades
instead of aborting (`ignored .vig.kdl: <reason>` in the status bar). See
[Troubleshooting](troubleshooting.md#vig-refuses-to-start-config-error)
for reading the messages in practice.
