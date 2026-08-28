# CLAUDE.md

## Concept

vig is a **read-only / safe-operations-only** Git TUI viewer:
- Allowed: `git switch`, `git branch -d` (safe delete), read operations
- **Not** allowed: merge, rebase, force delete (`-D`), push, or any destructive operation

This is a deliberate design choice — vig helps you *inspect*, not *mutate* your repository in dangerous ways.

## Build & Test

```bash
cargo build           # dev build
cargo clippy          # lint
mise run demo:all     # record all demo GIFs (requires vhs)
mise run demo:branch  # record branch selector demo only
```

## Branch Workflow

Always create a feature branch before making changes. Never commit directly to `main`.
When starting work on an issue, always pull the latest `main` first, then create the branch from it.

```bash
git checkout main && git pull        # update main first
git checkout -b feat/<feature-name>  # create a branch and start working
```

## Conventions

### Demo Tapes as Documentation & Tests

When adding or modifying a user-visible feature:

1. Update `README.md` and `docs/README.ja.md` (features, keybindings, etc.)
2. Create or update a VHS tape file in `tape/` that demonstrates the feature
3. Run `mise run demo:all` to re-record the GIFs
4. Commit both the `.tape` and `.gif` together — the `check-gif-freshness` pre-commit hook enforces this

**Important:** Feature PRs must include tape updates. If the feature changes UI or keybindings, update the relevant tape's key sequences. Even if no key sequences change, re-record the GIFs so they reflect the current UI. Do not defer tape updates to a separate PR.

Tape files serve as both **visual documentation** (the generated GIFs are embedded in PRs/README) and **integration tests** (VHS replays the exact key sequences against a real vig instance, so a broken feature will produce a visibly wrong GIF or crash during recording).

### Issue / Pull Request

When creating an issue or PR, first present the title and body in Japanese for user review. After approval, translate to English and create via `gh` command.

Always assign appropriate labels when creating issues (e.g., `enhancement`, `bug`, `documentation`).

### Copilot Review

After creating a PR or pushing changes (except when pushing fixes for Copilot review comments), request a Copilot review.

**Note:** Copilot cannot be added as a reviewer via CLI/API. The user must add it manually from the GitHub Web UI (PR → Reviewers → Copilot), or configure automatic Copilot review in the repository's Rulesets settings.

After the review completes, use `/review-copilot-comments` to check and address the review comments.

### Commit Messages

Use gitmoji prefix: `✨` new feature, `🐛` bug fix, `🩹` minor fix, `♻️` refactor, `🔧` config, `📝` docs, etc.

### Key Architecture

- `src/main.rs` — CLI, event loop, page registration (`pages = vec![git, github, files, docker, procs]`)
- `src/core/` — Page-agnostic framework: `app.rs` (`App`, `AppContext`, `PageState` trait), `pane.rs` (`Pane`, `PaneSet`, `PaneShared`, event dispatch), `layout.rs`, `keymap.rs`, `search.rs`, `tab.rs`, `tree.rs` (`nest_by` tree layout), `config/` (KDL loader / merge), `ui/` (status bar, help overlay, confirm dialog, `tail_pane.rs` log tail component)
- `src/git/` — Git page: `domain/` (repository, diff, watcher), `panes/` (file tree, branch list, reflog, git log, diff view), `state.rs`
- `src/github/` — GitHub page (`gh` CLI): issue / PR lists with detail views, disk cache
- `src/files/` — Files page: yazi-like parent / current / preview columns
- `src/docker/` — Docker page (`docker` CLI, read-only): containers grouped by compose project, images, inspect summary, log tail
- `src/procs/` — Procs page (`sysinfo` + `lsof` / `ss`, read-only): process tree, listening ports, process detail
- `assets/default.kdl` — Built-in config: every page's layout, tabs, bindings and keys live here

Each page follows the same shape: `page.rs` (`new_page(...) -> Result<Page>`), `state.rs` (the `PageState` impl owning a `PaneShared` and a `PaneSet` of panes), `panes/` (one `Pane` impl per pane), `domain/` (data fetching / parsing, no UI).

### How to add a page

Use the Docker page (`src/docker/`) as the template. Checklist:

1. **Module layout** — `src/<page>/{mod.rs, page.rs, state.rs, domain/{mod.rs, client.rs, types.rs}, panes/{mod.rs, <pane>.rs…}}`. `state.rs` implements `PageState` (`id()` is the KDL page name, `label()` the tab text) and `pane::PageLayout`; each pane implements `Pane<PaneEvent>` with its own action enum built via `impl_pane_action_from_str!` and `ActionHelp`.
2. **Register it** — `mod <page>;` in `src/main.rs`, create it in `run_tui` and append it to `pages`. Add it to the `app_keymap_resolves_page_switch_bindings` test in `src/core/app.rs` (and any test that lists page names, e.g. `user_app_keys_merge` in `loader.rs`).
3. **KDL** — add `page "<page>" { layout { … } tabs … bind … pane "view" { keys { … } } pane "<pane>" { keys { … } } }` to `assets/default.kdl` (every pane placed exactly once; `tabs` / `bind` only reference placed panes; `"q" "Quit"`, `"?" "Help"`, `"r" "Refresh"`, `Tab` / `BackTab` in `view`) and `"<n>" "page:<page>"` to the `app { }` block.
4. **Loader** — `pub fn <page>_page(&self) -> Result<LoadedPageConfig>` in `src/core/config/loader.rs`, call it from `Config::with_user` validation, and add a `<page>_page_ids_keys_and_bindings` test next to the existing ones.
5. **Header / status bar** — `render_<page>_header` and `render_<page>_status_bar` in `src/core/ui/status_bar.rs`.
6. **Background work** — an `mpsc` channel of a `<Page>BgMessage` enum, fetched on worker threads and drained in `drain_background()`; lazy-initialize on the first `on_activate`. External CLIs are detected on first use and a "not available" notice replaces the panes when missing.
7. **Help** — `help_bindings()` starts with the `view` keymap's `help_entries()` followed by one `help_section` per pane. Do not add a view-switch entry: `App::active_help_bindings()` prepends `"1 / 2 / …"` / "Switch view" from the `app { }` keymap via `AppContext::page_keys`. The header shows each page's slot number (its position in `pages`), never a key.
8. **Docs** — a Features bullet, a `| n |` row in View Switching and a `### <Page> View` key table in both `README.md` and `docs/README.ja.md`; the page and its actions in `docs/config.md`.
9. **Demo** — `tape/demo-<page>.tape` + `assets/demo-<page>.gif`, a `[tasks."demo:<page>"]` entry in `mise.toml` added to `demo:all`, recorded with `mise run demo:<page>`. Recordings must not show the recording machine (no hostname, real processes or private paths); the Procs tape records with `VIG_PROCS_ROOT_PID` so only synthetic demo processes appear.
10. **Read-only** — only inspecting commands; nothing that mutates the system.
