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

コードを修正する時は必ずフィーチャーブランチを作成して作業する。`main` ブランチに直接コミットしないこと。

```bash
git checkout -b feat/<feature-name>  # ブランチを切って作業開始
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

Issue や PR を作成する際は、まず日本語でタイトル・本文をユーザーに提示して確認を取る。承認後、英語に翻訳して `gh` コマンドで作成する。

Always assign appropriate labels when creating issues (e.g., `enhancement`, `bug`, `documentation`).

### Commit Messages

Use gitmoji prefix: `✨` new feature, `🐛` bug fix, `🩹` minor fix, `♻️` refactor, `🔧` config, `📝` docs, etc.

### Key Architecture

- `src/app.rs` — App state, key handling, all pane logic
- `src/git/` — Git operations (repository, diff, watcher)
- `src/ui/` — Rendering modules (layout, file_tree, branch_selector, diff_view, commit_log, status_bar)
- `src/main.rs` — Event loop, draw dispatch
