# CLAUDE.md

## Build & Test

```bash
cargo build           # dev build
cargo clippy          # lint
mise run demo:all     # record all demo GIFs (requires vhs)
mise run demo:branch  # record branch selector demo only
```

## Conventions

### Demo Tapes as Documentation & Tests

When adding or modifying a user-visible feature:

1. Create or update a VHS tape file in `tape/` that demonstrates the feature
2. Run `mise run demo:all` to re-record the GIFs
3. Commit both the `.tape` and `.gif` together — the `check-gif-freshness` pre-commit hook enforces this

Tape files serve as both **visual documentation** (the generated GIFs are embedded in PRs/README) and **integration tests** (VHS replays the exact key sequences against a real vig instance, so a broken feature will produce a visibly wrong GIF or crash during recording).

### Commit Messages

Use gitmoji prefix: `✨` new feature, `🐛` bug fix, `🩹` minor fix, `♻️` refactor, `🔧` config, `📝` docs, etc.

### Key Architecture

- `src/app.rs` — App state, key handling, all pane logic
- `src/git/` — Git operations (repository, diff, watcher)
- `src/ui/` — Rendering modules (layout, file_tree, branch_selector, diff_view, commit_log, status_bar)
- `src/main.rs` — Event loop, draw dispatch
