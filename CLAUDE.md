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

Tape files serve as both **visual documentation** (the generated GIFs are embedded in PRs/README) and **integration tests** (VHS replays the exact key sequences against a real vig instance, so a broken feature will produce a visibly wrong GIF or crash during recording).

### Issue / Pull Request

When creating an issue or PR, first present the title and body in Japanese for user review. After approval, translate to English and create via `gh` command.

Always assign appropriate labels when creating issues (e.g., `enhancement`, `bug`, `documentation`).

### Copilot Review

After creating a PR or pushing changes (except when pushing fixes for Copilot review comments), request a Copilot review:

```bash
gh pr edit <number> --add-reviewer "copilot"
```

After requesting a review, use `/review-copilot-comments` to check and address the review comments.

### Commit Messages

Use gitmoji prefix: `✨` new feature, `🐛` bug fix, `🩹` minor fix, `♻️` refactor, `🔧` config, `📝` docs, etc.

### Key Architecture

- `src/app.rs` — App state, key handling, all pane logic
- `src/git/` — Git operations (repository, diff, watcher)
- `src/ui/` — Rendering modules (layout, file_tree, branch_selector, diff_view, commit_log, status_bar)
- `src/main.rs` — Event loop, draw dispatch
