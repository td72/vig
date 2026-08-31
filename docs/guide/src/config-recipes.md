# Config Recipes

A cookbook for the parts of the config people actually change. Each recipe
is a problem, a complete config you can paste into
`~/.config/vig/config.kdl`, and what changes on screen. Every `kdl` block on
this page is loaded by vig's test suite exactly the way a user config is —
a broken example cannot ship.

If you have not read [Configuration Basics](configuration-basics.md), the
one-line summary: your file is a partial override of
[the defaults](https://github.com/td72/vig/blob/main/assets/default.kdl) —
key blocks merge per key, layouts replace wholesale.

## Appearance

### Change the syntax highlighting theme

*The diff colors don't fit my terminal.*

```bash
vig config themes    # list the choices; `*` marks the active one
```

```kdl
theme "Solarized (dark)"
```

The diff view (Git and Worktrees) and the Files preview re-color
immediately on next start. Only foreground colors come from the theme, so
the light themes (`InspiredGitHub`, `Solarized (light)`,
`base16-ocean.light`) are readable mainly on a light terminal background.

### Turn off file icons

*The Files view shows boxes / garbage instead of icons.*

Those are Nerd Font glyphs and your terminal font doesn't have them. Either
install a [Nerd Font](https://www.nerdfonts.com/), or:

```kdl
icons "none"
```

The Files view shows plain names.

### Tame image previews

*Image previews look wrong over SSH / in my terminal.*

By default (`"auto"`) the Files view probes the terminal for a graphics
protocol (Kitty, iTerm2, Sixel) and falls back to unicode halfblocks. Two
overrides:

```kdl
image-preview "halfblocks"   // skip detection, always use halfblocks
```

```kdl
image-preview "none"         // no image rendering at all
```

## Keybindings

### Rebind or add a key

*I want `o` to open things in the Git file tree, like in my file manager.*

```kdl
page "git" {
    pane "file_tree" {
        keys {
            "o" "ExpandOrOpen"
        }
    }
}
```

Keys merge per key: this adds one binding (or overrides `o` if it had one)
and leaves every other `file_tree` key at its default. The help overlay
(`?`) picks it up automatically — it is generated from the active config.

A key is written as a string: a single character (`"j"`, `"G"`, `"/"`), a
named key (`"Enter"`, `"Esc"`, `"Tab"`, `"BackTab"`, `"Space"`,
`"Backspace"`, `"Delete"`, `"Up"`, `"Down"`, `"Left"`, `"Right"`, `"Home"`,
`"End"`, `"PageUp"`, `"PageDown"`), or a `Ctrl+` combination (`"Ctrl+d"`).
Action names are per pane — `vig config dump` shows every pane with its
defaults, and the [Config Reference](config-reference.md) will list them all.

Global keys live in the `app` block and work on every page:

```kdl
app {
    "q" "Quit"            // quit from anywhere, not only from a pane
    "Ctrl+g" "page:git"   // jump to the Git view
}
```

`app` actions are `"Quit"` and `"page:<name>"` — switching to a page works
by *name*, so the binding keeps working when you reorder the tabs.

### Remove a binding

*`Space` toggling directories keeps surprising me.*

Bind the key to the reserved action `"None"`:

```kdl
page "git" {
    pane "file_tree" {
        keys {
            "Space" "None"
        }
    }
}
```

The key does nothing in that pane anymore and disappears from the help
overlay. This works for preset-provided keys too — `"n" "None"` in a pane
removes the search-next key there.

### What presets are

In `vig config dump` you'll see `preset "nav"` and `preset "search"` inside
almost every pane's `keys` block. A preset is a named bundle of standard
bindings, expanded in place:

| Preset | Expands to |
|---|---|
| `nav` | `j`/`Down` → `Nav.MoveDown`, `k`/`Up` → `Nav.MoveUp`, `Ctrl+d` → `Nav.HalfPageDown`, `Ctrl+u` → `Nav.HalfPageUp`, `g` → `Nav.JumpTop`, `G` → `Nav.JumpBottom` |
| `search` | `/` → `Search.Start`, `n` → `Search.Next`, `N` → `Search.Prev` |

Two rules govern them:

- **Explicit beats preset.** Presets expand first; an explicit binding in
  the same pane — the default config's or yours — wins for that key. That is
  how the recipe above could unbind `n` even though `preset "search"`
  provides it.
- **Presets are appended, never replaced.** When your keys merge into a
  pane, a `preset` line of yours is added alongside the existing ones. So if
  a pane somehow lacked `search`, `preset "search"` in your config adds the
  three search keys in one line.

## Tabs

### Trim or reorder the tabs

*I only use the Git, Files and Worktrees views.*

```kdl
pages "git" "files" "worktrees"
```

The header becomes `1:Git 2:Files 3:Worktrees`. The `pages` list replaces
the default list **wholesale**: the position in the list is the tab's number,
and pages you leave out are disabled entirely — not started, no tab, no
background polling.

Reordering works the same way:

```kdl
pages "github" "git" "files" "docker" "procs" "worktrees" "projects"
```

Number keys are bindings *onto pages, by name* — after either config, the
built-in `page:git` binding still reaches the Git view from its new
position. Built-in keys of disabled pages are silently dropped; but a
binding in *your own* `app` block to a page you did not list is an error,
because it can never work:

```kdl,ignore
pages "git" "files"
app {
    "d" "page:docker"    // → error: page "docker" is not listed in `pages`
}
```

## Layouts

### Reading a layout tree

Every page's arrangement is a tree of three elements inside `layout { }`:

- `split direction="horizontal" { … }` lays its children side by side;
  `direction="vertical"` stacks them. Each child may take a `size=`.
- `place "<pane>"` shows a pane.
- `slot "<name>" … { … }` is one area that shows *different panes at
  different times* — covered [below](#slots-one-area-several-panes).

Sizes are `"30"` (exactly 30 cells), `"40%"`, or `"min:20"` (at least 20
cells, grab the leftovers). Omitted means `min:0`. Here is the default Git
view layout, annotated:

```kdl
page "git" {
    layout {
        split direction="vertical" {                    // two rows
            split direction="horizontal" size="40%" {   // top row: 40% tall, three columns
                place "file_tree" size="30"             //   exactly 30 cells wide
                place "branch_list" size="35%"          //   35% of the width
                place "reflog" size="min:20"            //   the rest, at least 20
            }
            slot "main" size="min:3" then="git_log" default="diff_view" {
                triggers "branch_list" "reflog" "git_log"
            }                                           // bottom row: log or diff
        }
    }
}
```

That block *is* a valid config — restating a page's default layout changes
nothing, and is exactly how every layout edit starts: copy the page's
`layout` from `vig config dump`, then adjust. A `layout` you write replaces
the page's **whole** layout; there is no partial layout merge.

Two constraints, both enforced at startup: a layout may place each pane at
most once, and it must place at least one pane.

### Widen a pane

*The Files preview is too narrow.*

Copy the Files layout from the dump and shift the numbers:

```kdl
page "files" {
    layout {
        split direction="horizontal" {
            place "parent_dir" size="15%"   // default: 20%
            place "dir_list" size="25%"     // default: 30%
            place "preview" size="min:20"   // takes what the others freed
        }
    }
}
```

The preview now gets ~60% of the width instead of ~50%.

### Leave a pane out

*I never look at the reflog; give its space to the branches.*

A pane your layout does not mention becomes **inactive**: it gets no area,
`Tab` cycling and focus skip it, and `bind` lines naming it are ignored.
You don't have to touch `tabs` or keys — they adapt.

```kdl
page "git" {
    layout {
        split direction="vertical" {
            split direction="horizontal" size="40%" {
                place "file_tree" size="30"
                place "branch_list" size="min:20"     // reflog's space is yours
            }
            slot "main" size="min:3" then="git_log" default="diff_view" {
                triggers "branch_list" "git_log"
            }
        }
    }
}
```

### Bring the Projects list pane back

*I want to see all linked project boards as a list, not cycle with `p`.*

The Projects page ships with an intentionally unplaced pane: `projects`, the
list of boards linked to the repository. The default layout shows only the
board and the item detail (`p` / `P` cycle between boards). Place the list
and it comes alive — including its built-in
`bind select="projects" detail="board"`, which starts applying on its own:

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

Selecting a project in the list loads its board on the right; `Enter` moves
into it, `Esc` from the board returns to the list. (This very layout sits as
a comment in
[assets/default.kdl](https://github.com/td72/vig/blob/main/assets/default.kdl).)

### Slots: one area, several panes

*What are `slot`, `when` and `then`?*

A `slot` is a layout area that shows different panes depending on where your
focus is. The GitHub view's detail area is the worked example — one bottom
area, three possible occupants:

```kdl
page "github" {
    layout {
        split direction="vertical" {
            split direction="horizontal" size="40%" {
                place "issue_list" size="33%"
                place "pr_list" size="34%"
                place "run_list" size="33%"
            }
            slot "detail" size="min:3" default="issue_detail" {
                when "pr_list" "pr_detail" then="pr_detail"
                when "run_list" "run_detail" then="run_detail"
            }
        }
    }
}
```

Reading the slot: each `when` lists trigger panes and names the pane to show
(`then=`). The first `when` whose trigger list contains the focused pane
wins; when none matches, `default=` shows. So: focus on the PR column (or
inside the PR detail itself — that's why `pr_detail` is its own trigger) →
the area shows `pr_detail`; focus on the runs column → `run_detail`;
anywhere else, `issue_detail`. Note that each `when` names its `then` pane
among its own triggers — otherwise moving focus *into* the detail would
switch the area away from it.

There is also a single-case shorthand — `then=` on the slot itself plus a
`triggers` child — which the Git view uses: show `git_log` while
`branch_list`, `reflog` or `git_log` has focus, `diff_view` otherwise (see
[Reading a layout tree](#reading-a-layout-tree) above). Both forms can be
combined in one slot; the slot's name (`"detail"`, `"main"`) is just a label.

A variation to make the slot yours — you live in PRs, so make `pr_detail`
the resting state:

```kdl
page "github" {
    layout {
        split direction="vertical" {
            split direction="horizontal" size="40%" {
                place "issue_list" size="33%"
                place "pr_list" size="34%"
                place "run_list" size="33%"
            }
            slot "detail" size="min:3" default="pr_detail" {
                when "issue_list" "issue_detail" then="issue_detail"
                when "run_list" "run_detail" then="run_detail"
            }
        }
    }
}
```

For pane-placement purposes a slot counts each pane it can show as placed
once — so no other `place` may show `pr_detail` again, and the
at-most-once rule applies across the whole tree.

## Projects

### Pin one board

*My repository links five boards; I only ever look at one.*

By title (matched case-insensitively against the linked projects):

```kdl
projects-board "Roadmap"
```

Or by project number:

```kdl
projects-board 2
```

The Projects page shows only that board. `p` / `P` stop cycling (they show
`board pinned by config (projects-board)` in the status bar) and the header
drops the `(i/n)` counter. If no linked project matches, the board pane says
so, naming your pin. Note the number form is the one place the config takes
a bare integer instead of a quoted string.

## Per-repository config

### A `.vig.kdl` for one repository

*This one repo needs different tabs and a pinned board — but only this repo.*

Put a `.vig.kdl` at the worktree root (and gitignore it — it is personal):

```kdl
// .vig.kdl — this repository only
pages "git" "github" "projects"
projects-board "Roadmap"
github-poll-interval "10s"
```

It merges on top of your user config with the same rules (builtin → user →
repo-local, repo-local wins). Anything that is only true for one repository
belongs here: its pinned board, a trimmed page list, a busier or calmer poll
interval, a theme that matches that project's terminal profile. Preferences
that follow *you* — your keybindings, your icons — belong in the user
config.

Errors here never stop vig: a broken `.vig.kdl` is reported in the status
bar (`ignored .vig.kdl: …`) and vig starts with builtin + user.

### The trust dialog

If a `.vig.kdl` is **tracked** by git, it came with the repository, and vig
asks before loading it — a config decides which pages and keybindings exist,
so it is not loaded silently. The dialog appears before the UI starts:

- `y` — load it, and remember that answer for this exact file content
- `n` — ignore it, and remember
- `v` — view the file first, then decide
- `Esc` — ignore it this one time; ask again next start

Remembered decisions are keyed by worktree *and* content hash, so a changed
file (after a pull, say) asks again. Manage them from the CLI:

```bash
vig config trust                     # list remembered decisions
vig config trust --forget ~/src/foo  # ask again next time in that worktree
```

Your own **untracked** `.vig.kdl` never triggers the dialog — it loads
silently with a `loaded .vig.kdl` note in the status bar.

### Turn the repo layer off

*I never want a repository influencing my vig.*

In your **user** config:

```kdl
repo-config "off"
```

No `.vig.kdl` is loaded and no dialog ever appears. Only the user config's
value counts — a `.vig.kdl` cannot contain `repo-config` at all, so a
repository can never flip the switch back.

## Polling and history

### Calm down (or speed up) GitHub polling

*vig polls too often while I watch a running job.*

```kdl
github-poll-interval "10s"
```

This is how often the GitHub view polls **while something is active** — the
Workflow Runs column with a run in progress, a PR's checks in watch mode
(`w`), a running job's log. Default `"5s"`, minimum `"2s"` (so a config
cannot burn through your API quota); polling pauses entirely while another
view is shown. Rate-limit handling is built in on top: when GitHub rejects a
request, vig backs off exponentially and shows the reset time in the status
bar, regardless of this setting.

### Procs sampling rate and history depth

*I want smoother graphs and a longer history.*

```kdl
procs-refresh-interval "1s"
procs-history "600"
```

`procs-refresh-interval` is how often the Procs view re-reads processes and
ports while it is shown (default `"2s"`, minimum `"250ms"`, also `"1.5s"` /
`"500ms"` style values; sampling pauses on other views).

`procs-history` is how many samples the history graphs keep — the system
CPU / memory charts and the per-process sparklines. One sample lands per
refresh, so the two settings multiply: the example keeps
600 × 1s = 10 minutes of history. Default `"120"` (4 minutes at `"2s"`);
allowed range `"10"` to `"10000"`.
