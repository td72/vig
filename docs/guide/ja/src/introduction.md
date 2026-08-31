# vig ユーザーガイド

vig はリポジトリと、その周りで動いているものを見張るための**閲覧専用 TUI コックピット**です: git (サイドバイサイドの差分・ログ・reflog)、GitHub の issue / PR / Actions 実行 / Projects ボード、ファイルブラウザ、Docker コンテナ、プロセス、worktree / stash。全体を vim スタイルのキーで操作できます。AI エージェントが作業するリポジトリを含め、busy なリポジトリの監視を想定して作られています。

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
- **[設定の基本](configuration-basics.md)** — 設定ファイルの場所、3 つの
  レイヤ（組み込み → ユーザー → リポジトリローカル）、`vig config`
  サブコマンド、最低限の KDL、マージ規則。
- **[設定レシピ](config-recipes.md)** — コピーしてそのまま使える実例集:
  テーマ、キーバインド、タブ、レイアウト、slot、ボードの固定、
  リポジトリごとの設定、ポーリング。すべての例は CI で検証されています。
- **設定リファレンス / トラブルシューティング** — 今後の PR で追加予定です。
  それまでは [docs/config.md](https://github.com/td72/vig/blob/main/docs/config.md)
  を参照してください。

## English version

このガイドの英語版はこちら:
**[vig User Guide](https://td72.github.io/vig/)**
（リポジトリ内: [docs/guide](https://github.com/td72/vig/tree/main/docs/guide/src)）。
