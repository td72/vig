# Configuration Basics

vig starts with a complete built-in configuration — you never *have* to write
a config file. When you want to change something (the theme, a key, the tabs,
a layout), a single KDL file describes the change and vig merges it on top of
the defaults. You only write the parts you want different.

This chapter covers where config files live, how the layers stack, the
`vig config` subcommands, just enough KDL syntax to read and write the file,
and the merge rules. The next chapter, [Config Recipes](config-recipes.md),
is a cookbook of worked examples — every snippet there is a complete config
you can paste and run.

## The three layers

vig builds its effective configuration from up to three layers, each merged
on top of the previous one:

```text
3. repo-local   .vig.kdl at the worktree root   (personal, per-repository)
2. user         ~/.config/vig/config.kdl        (yours, for every repository)
1. builtin      the embedded defaults           (always present)
```

Run `vig config path` at any time to see all three layers, their paths, and
whether each one was found and loaded.

### 1 — builtin

The defaults are compiled into the binary. They are a complete KDL config
themselves — the same
[assets/default.kdl](https://github.com/td72/vig/blob/main/assets/default.kdl)
that `vig config dump` prints — so everything that *can* be configured is
visible in one place, with comments.

### 2 — user

Your own config file. vig looks for it in this order:

1. `--config <path>` command-line flag
2. `$VIG_CONFIG` environment variable
3. `$XDG_CONFIG_HOME/vig/config.kdl`, or `~/.config/vig/config.kdl` if
   `XDG_CONFIG_HOME` is unset — on every OS, **including macOS** (vig
   deliberately does not use `~/Library/Application Support`; `~/.config/vig`
   is what users of zellij / helix / etc. expect everywhere)

A missing file at the default location simply means "use the defaults", but a
path given explicitly via `--config` or `$VIG_CONFIG` must exist — vig
refuses to start otherwise, so a typo in the path cannot silently give you
the wrong config. Note that `--config` / `$VIG_CONFIG` *replace* the user
layer; they do not add a fourth one.

### 3 — repo-local (`.vig.kdl`)

vig also reads a personal `.vig.kdl` from the root of the current worktree
and merges it on top of your user config, so one repository can get its own
theme, pages, or keybindings. It uses the exact same schema as the user
config and is meant to be **gitignored** — it is your file, not the
project's. The [per-repository recipes](config-recipes.md#per-repository-config)
show what typically goes in it.

Because a cloned repository may *ship* a committed `.vig.kdl`, trust is
decided by git tracking:

- An **untracked** `.vig.kdl` is your own file: it loads silently, and the
  status bar shows `loaded .vig.kdl` once at startup.
- A **tracked** `.vig.kdl` is repo-provided: a **trust dialog** appears
  before the app is built (the answer decides which pages and keybindings
  even exist). `y` loads it and remembers the decision, `n` ignores it and
  remembers, `v` shows the file so you can decide, and `Esc` ignores it this
  one time without remembering anything.

Decisions are stored in `$XDG_STATE_HOME/vig/trust.json`
(`~/.local/state/vig/trust.json`), keyed by the worktree path **and a hash of
the file content** — when the file changes (say, after a pull), the old
decision no longer applies and vig asks again. `vig config trust` lists the
remembered decisions; `vig config trust --forget <path>` drops one.

Two more properties of this layer, both deliberate:

- **It degrades instead of aborting.** An error in `.vig.kdl` never prevents
  vig from starting: you get builtin + user, plus an
  `ignored .vig.kdl: <reason>` note in the status bar (and one line on
  stderr).
- **It cannot control its own switch.** Putting `repo-config "off"` in your
  *user* config disables the layer entirely — no loading, no dialog. Only
  the user config's value counts; a `.vig.kdl` that contains `repo-config`
  itself (even `"on"`) is rejected.

## The `vig config` subcommands

| Command | What it does |
|---|---|
| `vig config path` | One line per layer — builtin / user / repo-local — with its path and status (`loaded`, `not found`, `ignored (…)`, `pending trust decision`). |
| `vig config dump` | Print the built-in default config. This is the complete schema, commented — the best starting point for your own file. |
| `vig config themes` | List the available syntax highlighting themes; `*` marks the active one. |
| `vig config trust` | List the remembered `.vig.kdl` trust decisions (worktree, decision, date). |
| `vig config trust --forget <path>` | Forget the decision for one worktree, so the dialog asks again. |

All of them respect `--config` / `$VIG_CONFIG`, so
`vig --config ./try.kdl config path` tells you what that file would do.

## Copy the dump, then trim

The comfortable way to write your first config:

```bash
mkdir -p ~/.config/vig
vig config dump > ~/.config/vig/config.kdl
```

Now open the file, change what you want changed — and then **delete
everything you did not change**. Your file is a *partial override*: whatever
it does not mention keeps its default. Trimming matters for a second reason
too: a full copy of the dump freezes every default at today's values, so
when a future vig release improves a default binding or layout, your
untouched-but-copied version would silently override it. A trimmed config
states exactly your opinions, and nothing else.

A trimmed file often ends up this small:

```kdl
theme "Solarized (dark)"
icons "none"
pages "git" "github" "worktrees"
```

vig validates the file at startup, so you can iterate quickly: edit, run
`vig`, read the error if any, repeat. To experiment without touching your
real config, point at a scratch file: `vig --config ./try.kdl`.

## Just enough KDL

The config is a [KDL](https://kdl.dev/) document. You need five ideas to read
and write it:

- A **node** is a name followed by arguments: `theme "Solarized (dark)"`.
- **Arguments are strings** in double quotes; a node can take several:
  `pages "git" "files" "worktrees"`. (One exception takes a bare integer:
  `projects-board 2`.)
- A **property** is a named value on a node: `split direction="horizontal"`.
- A node can have **children** in `{ … }`, nested to any depth.
- **Comments**: `//` to end of line, `/* … */` for a span, and the
  KDL-specific *slashdash* `/-` which comments out the entire node that
  follows it — children and all.

All five in six lines:

```kdl
theme "Solarized (dark)"        // node with one string argument
/- icons "none"                 // slashdash: this node is ignored
page "git" {                    // children block
    layout {
        split direction="horizontal" {      // property
            place "file_tree" size="30"
            place "diff_view" size="min:20"
        }
    }
}
```

(That example really loads — it also happens to replace the Git view's layout
with just the file tree and the diff, which the
[layout recipes](config-recipes.md#layouts) explain.)

## How merging works

Your file merges into the defaults node by node, and different nodes merge
differently. There are three classes:

| Class | Nodes | Rule |
|---|---|---|
| Replace wholesale | `theme`, `icons`, `image-preview`, `procs-refresh-interval`, `procs-history`, `github-poll-interval`, `projects-board`, `pages`, `repo-config` | Your node replaces the default node entirely. |
| Merge per key | `app { }`, `page "…" { pane "…" { keys { } } }` | Each key you mention replaces the default binding for that key; keys you do not mention keep their default. `preset` lines are appended. |
| Replace wholesale, per page | `page "…" { layout { } }`, `tabs`, `bind` | If your page block contains a `layout`, it replaces that page's whole layout. Same for `tabs`, and for the set of `bind` lines. |

The consequences, spelled out:

- **Top-level values are all-or-nothing** — which is natural, since each is a
  single value. Your `pages` list replaces the default list *wholesale*:
  pages you leave out are disabled, not merely moved.
- **Key blocks are additive.** Writing
  `page "git" { pane "file_tree" { keys { "o" "ExpandOrOpen" } } }` adds one
  binding; every other `file_tree` key keeps its default. Binding a key to
  the special action `"None"` removes it. This is why a keybinding tweak is
  two lines, not a restatement of forty defaults.
- **Layouts are not additive.** If you write a `layout` for a page, you are
  writing the *whole* layout of that page — there is no way to nudge one
  pane's size without restating the tree. Start from the corresponding block
  in `vig config dump` and edit. The same applies to `tabs` (the pane cycle
  order) and `bind` (the select→detail wiring): one user `bind` line
  replaces *all* of the page's default `bind` lines.

Page and pane names are fixed — you can rearrange, resize, and rebind them,
but not invent new ones. The valid pages are `git`, `github`, `files`,
`docker`, `procs`, `worktrees`, and `projects`; run `vig config dump` to see
each page's panes.

The repo-local `.vig.kdl` merges with exactly the same rules, one layer
later: builtin → user → repo-local, repo-local winning.

## Errors are loud

Any problem in the user config — a syntax error, an unknown node, a bad
theme name, a layout that places a pane twice or places nothing — stops vig
from starting and prints a message naming the file (and line:column for
syntax errors). vig never silently falls back to the defaults when a config
file is present, so a typo cannot go unnoticed:

```kdl,ignore
theem "Solarized (dark)"
// → invalid config file ~/.config/vig/config.kdl:
//   unknown top-level block "theem" (expected `theme`, `icons`, ...)
```

The one exception is the repo-local layer, which degrades instead of
aborting, as described [above](#3--repo-local-vigkdl).
