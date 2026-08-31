# vig ユーザーガイド

vig は、サイドバイサイド diff ビューと vim スタイルのキーバインドを備えた
Git TUI です。リポジトリの周辺で普段見るもの — GitHub の Issue / PR /
Actions 実行、ファイルブラウザとしてのワーキングツリー、Docker コンテナ、
実行中のプロセス、worktree と stash、GitHub Projects のボード — を
読み取り専用のビューとしてまとめて眺められます。

> **安全設計** — vig は読み取り操作と安全な git コマンド（`git switch`、
> `git branch -d`）のみを実行します。merge、rebase、force delete、push
> などの破壊的操作は意図的に除外しています。vig はリポジトリを
> *眺める*ためのツールであり、変更するためのツールではありません。

![demo](../../../../assets/demo.gif)

## このガイドの構成

- **[Getting Started](getting-started.md)** — インストール、最初の起動、
  7 つのビューのツアー、必要な環境。
- **[ビュー](views.md)** — ビューごとに 1 章: 何が表示されるか、
  すべてのキーバインド、各ビューの制約。
- **設定の基本 / 設定レシピ / 設定リファレンス / トラブルシューティング** —
  今後の PR で追加予定です（vig は 1 つの KDL ファイルで細かく設定できます。
  それまでは [docs/config.md](https://github.com/td72/vig/blob/main/docs/config.md)
  を参照してください）。

## English version

このガイドの英語版はこちら:
**[vig User Guide](https://td72.github.io/vig/)**
（リポジトリ内: [docs/guide](https://github.com/td72/vig/tree/main/docs/guide/src)）。
