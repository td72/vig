# vig User Guide

vig is a Git TUI with a side-by-side diff view, vim-style keybindings, and a
set of read-only companion views for the things you look at next to your
repository: GitHub issues / PRs / Actions runs, the working tree as a file
browser, Docker containers, running processes, worktrees and stashes, and
GitHub Projects boards.

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
- **Configuration Basics / Config Recipes / Config Reference /
  Troubleshooting** — coming in later PRs (vig is fully configurable through a
  single KDL file; until those chapters land, see
  [docs/config.md](https://github.com/td72/vig/blob/main/docs/config.md)).

## 日本語版

This guide is also available in Japanese:
**[vig ユーザーガイド](https://td72.github.io/vig/ja/)**
(in-repo: [docs/guide/ja](https://github.com/td72/vig/tree/main/docs/guide/ja/src)).
