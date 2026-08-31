# vig User Guide

vig is a **read-only TUI cockpit** for a repository and everything working
around it: git (side-by-side diffs, log, reflog), GitHub issues / pull
requests / Actions runs / Projects boards, a file browser, Docker containers,
running processes, and worktrees / stashes — with vim-style keybindings
throughout. It is built for keeping an eye on busy repositories, including
ones where AI agents do the work.

> **Safe by design** — vig only performs read operations and safe git commands
> (`git switch`, `git branch -d`). Destructive operations like merge, rebase,
> force delete, or push are intentionally excluded. vig helps you *inspect*
> your repository, not mutate it.

![demo](../../../assets/demo.gif)

## What's in this guide

- **[Getting Started](getting-started.md)** — installation, your first run, a
  tour of the seven views, and what vig needs from your environment.
- **[Views](views.md)** — one chapter per view: what it shows, every key
  binding, and each view's constraints.
- **[Configuration Basics](configuration-basics.md)** — where config files
  live, the three layers (builtin → user → repo-local), the `vig config`
  subcommands, just enough KDL, and the merge rules.
- **[Config Recipes](config-recipes.md)** — worked, copy-pasteable examples:
  themes, keybindings, tabs, layouts, slots, board pinning, per-repository
  config, polling. Every example is CI-verified.
- **Config Reference / Troubleshooting** — coming in a later PR; until then,
  see [docs/config.md](https://github.com/td72/vig/blob/main/docs/config.md).

## 日本語版

This guide is also available in Japanese:
**[vig ユーザーガイド](https://td72.github.io/vig/ja/)**
(in-repo: [docs/guide/ja](https://github.com/td72/vig/tree/main/docs/guide/ja/src)).
