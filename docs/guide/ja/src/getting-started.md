# Getting Started

vig はすべて端末内で動きます。インストールして Git リポジトリに `cd` し、
`vig` を実行するだけ — 設定なしでそのまま使えます。

## インストール

### Homebrew

```bash
brew install td72/tap/vig
```

### ビルド済みバイナリ

[GitHub Releases](https://github.com/td72/vig/releases) ページから
ビルド済みバイナリをダウンロードできます:

```bash
# Linux x86_64
curl -sL https://github.com/td72/vig/releases/latest/download/vig-x86_64-unknown-linux-gnu.tar.gz | tar xz -C ~/.local/bin vig

# Linux aarch64
curl -sL https://github.com/td72/vig/releases/latest/download/vig-aarch64-unknown-linux-gnu.tar.gz | tar xz -C ~/.local/bin vig

# macOS Apple Silicon
curl -sL https://github.com/td72/vig/releases/latest/download/vig-aarch64-apple-darwin.tar.gz | tar xz -C ~/.local/bin vig
```

### crates.io

```bash
cargo install vig
```

### ソースからビルド

必要なもの: Rust ツールチェイン, libgit2, libssl, pkg-config

```bash
cargo install --path .
```

## 最初の起動

Git リポジトリ内で vig を実行します:

```bash
cd your-repo
vig
```

起動すると **Git ビュー** が表示されます。左に変更ファイル、その隣に
ブランチと reflog、残りの画面いっぱいにサイドバイサイドの diff です。
ヘッダにはビューが番号付きタブ（`1:Git`、`2:GitHub`、…）で並び、数字キーで
切り替えられます。下部のステータスバーには現在のモードと、フォーカス中の
ペインでよく使うキーが表示されます。

初日に知っておくと良いこと:

- `q` または `Ctrl+c` で終了します。
- `?` で現在のビューの全キーバインドを一覧するヘルプオーバーレイが開きます。
- `r` で現在のビューを再読込します。Git ビューはファイルの変更を監視して
  自動でも更新されます。
- vig は設定なしで動きます。変えたくなったら KDL ファイル 1 つで設定できます
  （`--config <path>`、`$VIG_CONFIG`、または `~/.config/vig/config.kdl`）。
  詳しくは設定の章を参照してください。

## 7 つのビューのツアー

vig には 7 つのビューがあります。それぞれ [ビュー](views.md) の章で詳しく
説明します。ここでは 30 秒版のツアーです。

### 1 — Git

![git demo](../../../../assets/demo.gif)

vig の中心です。シンタックスハイライト付きのサイドバイサイド diff、
ステータス表示付きのファイルツリー、git log プレビュー付きのブランチ
セレクタ、そして reflog。任意のブランチや reflog エントリと比較でき、
vim モーションでヤンクし、`$EDITOR` でファイルを開けます。

### 2 — GitHub

![github demo](../../../../assets/demo-github-pr.gif)

Issue、Pull Request（本文、コメント、レビュー、CI ステータス）、Actions の
ワークフロー実行（ジョブ、ステップ、ジョブログ）を `gh` CLI 経由で
読み取り専用で閲覧できます。本文は Markdown としてレンダリングされます。

### 3 — Files

![files demo](../../../../assets/demo-files.gif)

リポジトリをルートとした yazi 風の 3 カラムファイルブラウザです。
親ディレクトリ、現在のディレクトリ、シンタックスハイライト付きプレビュー。
画像もプレビューでき、グラフィックプロトコル対応端末では元の解像度で
描画されます。

### 4 — Docker

![docker demo](../../../../assets/demo-docker.gif)

compose プロジェクトごとにまとめたコンテナ一覧、イメージ一覧、inspect
サマリ、ログのライブ tail。`docker` CLI 経由の読み取り専用ビューです。

### 5 — Procs

![procs demo](../../../../assets/demo-procs.gif)

CPU / メモリ付きのプロセスツリー、LISTEN 中のポートとその所有プロセス、
btop 風のシステム履歴グラフ、CPU / RSS スパークライン付きのプロセス詳細。
見るだけのビューで、シグナルを送ることはありません。

### 6 — Worktrees

![worktrees demo](../../../../assets/demo-worktrees.gif)

worktree と stash を一覧し、HEAD コミットや stash のパッチを Git ビューと
同じサイドバイサイド diff ビューで表示します。

### 7 — Projects

![projects demo](../../../../assets/demo-projects.gif)

リポジトリにリンクされた GitHub Projects (v2) のボード。`Status` ごとの
カンバン列、ソートできるテーブルモード、全プロジェクトフィールドを表示する
アイテム詳細。

## vig 内でヘルプを見る

どのビューでも `?` でヘルプオーバーレイが開き、現在のビューの全キー
バインドが一覧されます。有効な設定から生成されるので、自分でリバインドした
キーもそのまま反映されます。`?` または `Esc` で閉じます。

## vig を最新に保つ

```bash
vig update
```

`vig update` は GitHub から最新リリースをダウンロードし、署名を検証して
現在のバイナリを置き換えます。ビルド済みリリースバイナリでインストールした
場合向けの機能です。Homebrew や cargo でインストールした場合は、パッケージ
マネージャに任せるため `brew upgrade vig` / `cargo install vig` を使って
ください。

## 必要な環境

vig 本体は Git リポジトリさえあれば動きます。一部のビューは外部ツールが
あるときに使えます:

| ビュー | 必要なもの | 無い場合 |
|--------|-----------|----------|
| Git, Worktrees | 追加不要 | — |
| GitHub | [GitHub CLI (`gh`)](https://cli.github.com/) のインストールと認証（`gh auth login`） | ペインの代わりに案内を表示 |
| Projects | `gh` のトークンに `project` スコープ — `gh auth refresh -s project` を実行 | スコープ不足の案内を表示 |
| Docker | `docker` CLI と起動中のデーモン | ペインの代わりに案内を表示 |
| Procs | 追加不要（ポート取得に macOS は `lsof`、Linux は `ss`） | ポート情報が空になることがある |

その他のメモ:

- **Nerd Font** — Files ビューのファイル種別アイコンには
  [Nerd Font](https://www.nerdfonts.com/) が必要です。端末のフォントが
  Nerd Font でない場合は設定に `icons "none"` を書いてください。
- **`$EDITOR`** — `e` で選択中のファイルを外部エディタで開きます。
