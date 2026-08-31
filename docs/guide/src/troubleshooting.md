# Troubleshooting / FAQ

The problems people actually hit, with the exact messages vig shows and the
way out of each. The fixes that are config changes link into the
[Config Reference](config-reference.md).

## vig refuses to start (config error)

vig validates the user config at startup and **fails fast**: any problem —
a syntax error, an unknown node, a bad value — stops vig with a message
naming the file, instead of silently falling back to the defaults. A real
one:

```text
Error: invalid config file /home/you/.config/vig/config.kdl

Caused by:
    unknown top-level block "theem" (expected `theme`, `icons`, `image-preview`,
    `procs-refresh-interval`, `procs-history`, `github-poll-interval`,
    `projects-board`, `pages`, `repo-config`, `app`, or `page`)
```

Reading it: the first line names the file; the cause names the thing vig
did not accept and lists what it expected — here a typo of `theme`. Syntax
errors additionally carry `file:line:column`. The
[Startup errors](config-reference.md#startup-errors) section lists every
category.

Three tools for iterating quickly:

- `vig config path` — shows each layer's status; a broken user config shows
  as `invalid (…)` with the same message, without trying to start the TUI.
- `vig --config ./try.kdl` — experiment on a scratch file without touching
  your real config.
- `vig config dump` — the always-valid reference to copy correct forms
  from.

The repository-local `.vig.kdl` is the one layer that never blocks startup:
when it is broken, vig starts with builtin + user and shows
`ignored .vig.kdl: <reason>` in the status bar (plus one line on stderr).

## The GitHub view shows an error instead of the panes

The GitHub, Projects (and their polling) go through the
[GitHub CLI (`gh`)](https://cli.github.com/). Two usual causes:

- **`gh` is not installed** — the status bar shows the launch error (e.g.
  `gh not found: No such file or directory`). Install the GitHub CLI so it
  is on your `PATH`.
- **`gh` is not authenticated** — the `gh` error is shown as-is. Run
  `gh auth login`, then press `r` in the view to retry.

vig never reads your token itself; authentication is entirely `gh`'s.

## Projects view: `gh needs the project scope`

The Projects view uses `gh project …`, which needs the `project` token
scope — a scope `gh auth login` does not grant by default. When it is
missing, the view shows a notice instead of the panes and the status bar
says:

```text
gh needs the project scope: run `gh auth refresh -s project`
```

Run exactly that:

```bash
gh auth refresh -s project
```

then press `r` in the view. (The GitHub view works without this scope; only
Projects needs it.)

## `⚠ GitHub rate limited`

When GitHub rejects a request as rate-limited, the GitHub page stops all
its polling and backs off exponentially — 30s, 60s, … capped at 10
minutes — showing `⚠ GitHub rate limited (resets in Nm)` in the status
bar. The reset time comes from a single `gh api rate_limit` call (that
endpoint is itself not rate-limited). `r` retries immediately; the first
successful fetch clears the backoff.

If you hit this regularly, slow the polling down — vig polls only while
something is active (a running workflow, watch mode, a running job's log),
at [`github-poll-interval`](config-reference.md#github-poll-interval)
(default `"5s"`, minimum `"2s"`):

```kdl
github-poll-interval "15s"
```

Remember the quota is shared with everything else using your token —
other tools polling the same account count against the same limit.

## The Files view shows boxes / garbage instead of icons

Those are Nerd Font glyphs and your terminal font does not have them.
Either install a [Nerd Font](https://www.nerdfonts.com/), or turn icons
off ([`icons`](config-reference.md#icons)):

```kdl
icons "none"
```

## Image previews look wrong (or: why is my image low-res?)

The Files view previews images at full resolution only in terminals with a
graphics protocol — Kitty, WezTerm, Ghostty, iTerm2, or Sixel-capable ones
such as foot. Elsewhere (and over most SSH / multiplexer setups) it falls
back to unicode halfblocks, which are deliberately coarse. The preview's
first line names the renderer in use, so you can see which path you got.

If the auto-detection misbehaves in your terminal, override it
([`image-preview`](config-reference.md#image-preview)):

```kdl
image-preview "halfblocks"   // skip detection, always use halfblocks
```

```kdl
image-preview "none"         // no image rendering, metadata only
```

Images over 20 MB are not decoded.

## The trust dialog keeps asking about `.vig.kdl`

The dialog appears for a git-**tracked** `.vig.kdl` (one that came with the
repository), and the remembered answer is keyed by the worktree path **and
a hash of the file content** — so a changed file (e.g. after a pull) asks
again by design, and `Esc` never remembers anything (use `y` / `n` for
that). Manage the memory from the CLI:

```bash
vig config trust                     # list remembered decisions
vig config trust --forget <path>     # ask again next time in that worktree
```

If you never want the repo-local layer at all, switch it off in your
**user** config ([`repo-config`](config-reference.md#repo-config)) — no
loading, no dialog:

```kdl
repo-config "off"
```

Your own **untracked** `.vig.kdl` never triggers the dialog.

## Where vig keeps its files (and how to clear them)

vig writes to three places, all safe to delete:

| What | Where | Notes |
|---|---|---|
| Config | `~/.config/vig/config.kdl` (or `$XDG_CONFIG_HOME/vig/config.kdl`) | Yours; vig only reads it. |
| GitHub disk cache | `<cache>/vig/v1/<owner>/<repo>/` where `<cache>` is `~/.cache` (`$XDG_CACHE_HOME`) on Linux, `~/Library/Caches` on macOS | Cached issue / PR lists and details, so the GitHub view has content the moment it opens. Deleting it costs one re-fetch. |
| Trust store | `$XDG_STATE_HOME/vig/trust.json` (`~/.local/state/vig/trust.json`) | The remembered `.vig.kdl` decisions. Prefer `vig config trust --forget` for single entries; deleting the file just re-asks everywhere. |

vig stores no credentials anywhere — GitHub access goes through `gh`, which
manages its own token.

## FAQ

### Can vig change my repository?

No, by design. vig performs read operations and exactly two safe git
commands — `git switch` and `git branch -d` (the safe delete, which
refuses unmerged branches) — both only from the branch action menu, after
you confirm. No merge, rebase, force delete, push, stash mutation,
container operation or process signal, ever.

### A key does nothing / does the wrong thing — where do I look?

Press `?`: the help overlay is generated from the *merged* config, so it
shows exactly what is bound right now — your rebindings included. If a key
is missing there, it was unbound (`"None"`) or your layout made its pane
inactive. `vig config path` tells you which layers are loaded; remember a
repo-local `.vig.kdl` can rebind keys too.

### Why is a pane missing from the screen?

A layout in your config (or in `.vig.kdl`) that leaves a pane out makes it
*inactive* — no area, skipped by `Tab`. That is a feature
([recipe](config-recipes.md#leave-a-pane-out)), and the Projects page even
ships one intentionally unplaced pane
([Page `projects`](config-reference.md#page-projects)). Restate the page's
`layout` with the pane placed to bring it back.

### How do I update vig?

`vig update` downloads the latest release, verifies its signature and
replaces the binary — meant for installs from the pre-built binaries. If
you installed via Homebrew or cargo, use `brew upgrade vig` /
`cargo install vig` instead.

### Does vig work without `gh` / Docker?

Yes. Views degrade independently: without `gh` the GitHub and Projects
views show a notice; without a Docker daemon the Docker view does; the
Git, Files, Procs and Worktrees views never need them. Disable unused
views entirely with [`pages`](config-reference.md#pages).
