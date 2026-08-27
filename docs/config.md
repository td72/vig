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
action name, a layout that does not place every pane — stops vig from
starting and prints a message with the file path (and line:column for syntax
errors). vig never silently falls back to the defaults when a config file is
present, so a typo cannot go unnoticed.

## Merge rules

Your file is a **partial override** of the defaults:

| Block | Rule |
|---|---|
| `theme "<name>"` | Replaces the syntax highlighting theme. |
| `app { }` | Merged per key. A key you set replaces the default binding for that key. |
| `page "<name>" { pane "<pane>" { keys { } } }` | Merged per key, on top of the default keys (including expanded presets). |
| `"<key>" "None"` | Removes the binding for that key. |
| `page "<name>" { layout { } }` | Replaces the whole default layout of that page. |
| `page "<name>" { tabs ... }` | Replaces the tab order. |
| `page "<name>" { bind ... }` | Replaces all select→detail bindings of that page. |

Page names (`git`, `github`, `files`) and pane names are fixed — you can rearrange
and rebind them, but not add or remove them. A replaced layout must place
every pane of the page.

## Example

```kdl
// ~/.config/vig/config.kdl

theme "Solarized (dark)"   // `vig config themes` lists the choices

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
app { <key> <action> ... }
page "git" { ... }
page "github" { ... }
page "files" { ... }
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

### `app`

Global bindings that work on every page.

| Action | Meaning |
|---|---|
| `"Quit"` | Quit vig |
| `"page:git"`, `"page:github"`, `"page:files"` | Switch to that page |
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
  - Sizes: `"30"` (cells), `"40%"`, `"min:20"`. Defaults to `"min:0"`.
- `tabs` — the panes cycled by `Tab` / `BackTab`, in order.
- `bind` — which detail pane a selection pane drives (e.g. selecting a file
  in `file_tree` loads it into `diff_view`).
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
| `issue_list`, `pr_list` | `OpenDetail`, `SwitchTab`, `OpenBrowser`, `Esc` |
| `issue_detail`, `pr_detail` | `FocusBody`, `FocusRight`, `CycleForward`, `CycleBackward`, `ToggleWatch`, `OpenItem`, `Esc` |

**Page `files`**

| Pane | Actions |
|---|---|
| `view` (page-wide) | `Quit`, `Help`, `Refresh`, `CyclePaneForward`, `CyclePaneBackward`, `OpenEditor` |
| `parent_dir` | display-only (no keys) |
| `dir_list` | `Enter`, `Parent`, `FocusPreview`, `Esc` |
| `preview` | `Back`, `Esc` |

**Presets**

| Preset | Keys |
|---|---|
| `nav` | `j`/`Down` `Nav.MoveDown`, `k`/`Up` `Nav.MoveUp`, `Ctrl+d` `Nav.HalfPageDown`, `Ctrl+u` `Nav.HalfPageUp`, `g` `Nav.JumpTop`, `G` `Nav.JumpBottom` |
| `search` | `/` `Search.Start`, `n` `Search.Next`, `N` `Search.Prev` |
