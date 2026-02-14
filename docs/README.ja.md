# vig

[English](../README.md)

Git の差分をサイドバイサイドで表示する TUI ビューア。vim スタイルのキーバインドで操作できます。

> **安全設計** — vig は読み取り操作と安全な git コマンド（`git switch`、`git branch -d`）のみを実行します。merge、rebase、force delete などの破壊的操作は意図的に除外しています。

![demo](../assets/demo.gif)

## 特徴

- サイドバイサイド diff ビュー（シンタックスハイライト付き）
- ブランチセレクタ（git log プレビュー付き）
- ワーキングディレクトリを任意のローカルブランチと比較可能
- Vim スタイルのモード: Scroll, Normal, Visual, Visual-Line
- ファイルツリー（ステータス表示: A/D/M/R/?）
- Vim モーションによるヤンク（コピー）、システムクリップボード対応
- ファイル監視による自動リフレッシュ
- 外部エディタでファイルを開く（`$EDITOR`）
- **GitHub View** — Issue と Pull Request を閲覧（本文、コメント、レビュー、CI ステータス）。`gh` CLI 使用

## インストール

### Homebrew

```bash
brew install td72/tap/vig
```

### ビルド済みバイナリ

[GitHub Releases](https://github.com/td72/vig/releases) ページからビルド済みバイナリをダウンロードできます:

```bash
# Linux x86_64
curl -sL https://github.com/td72/vig/releases/latest/download/vig-x86_64-unknown-linux-gnu.tar.gz | tar xz -C ~/.local/bin vig

# Linux aarch64
curl -sL https://github.com/td72/vig/releases/latest/download/vig-aarch64-unknown-linux-gnu.tar.gz | tar xz -C ~/.local/bin vig

# macOS Apple Silicon
curl -sL https://github.com/td72/vig/releases/latest/download/vig-aarch64-apple-darwin.tar.gz | tar xz -C ~/.local/bin vig

# macOS Intel
curl -sL https://github.com/td72/vig/releases/latest/download/vig-x86_64-apple-darwin.tar.gz | tar xz -C ~/.local/bin vig
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

## 使い方

Git リポジトリ内で実行:

```bash
vig
```

## キーバインド

### View 切り替え

| キー | 操作 |
|------|------|
| `1` | Git View に切り替え |
| `2` | GitHub View に切り替え |

### ペイン操作

| キー | 操作 |
|------|------|
| `Tab` / `Shift+Tab` | ペイン切り替え: Files → Branches → Reflog → GitLog → Diff |
| `h` / `l` | 上部ペイン間の移動（Files, Branches, Reflog） |
| `i` | 上部ペインからメインペインへ移動（GitLog / Diff） |
| `Esc` | メインペインから直前の上部ペインへ戻る |

### ナビゲーション

| キー | 操作 |
|------|------|
| `j` / `k` | 下 / 上にスクロール |
| `h` / `l` | 左 / 右にスクロール（Diff ビュー内） |
| `gg` | 先頭にジャンプ |
| `G` | 末尾にジャンプ |
| `Ctrl+d` / `Ctrl+u` | 半ページ下 / 上 |

### ブランチリスト

![branch demo](../assets/demo-branch.gif)

| キー | 操作 |
|------|------|
| `j` / `k` | ブランチ移動（git log プレビューが更新） |
| `Enter` | アクションメニュー（switch / delete / diff base 設定） |
| `/` | ブランチ検索 |
| `Esc` | 検索クリア / 比較対象を HEAD にリセット |

### Git Log

| キー | 操作 |
|------|------|
| `j` / `k` | コミット移動 |
| `Ctrl+d` / `Ctrl+u` | 半ページスクロール |
| `g` / `G` | 先頭 / 末尾 |
| `y` | コミットハッシュをコピー |
| `o` | GitHubで開く |
| `/` | コミット検索 |
| `Esc` | 検索クリア / ブランチリストへ戻る |

### Reflog

| キー | 操作 |
|------|------|
| `j` / `k` | エントリ移動 |
| `Ctrl+d` / `Ctrl+u` | 半ページスクロール |
| `g` / `G` | 先頭 / 末尾 |
| `Enter` | diff base として設定 |
| `/` | reflog 検索 |
| `Esc` | 検索クリア / Branches へ戻る |

### モード

| キー | 操作 |
|------|------|
| `i` | Normal モードに入る |
| `v` | Visual モード（文字単位） |
| `V` | Visual-Line モード（行単位） |
| `Esc` | Scroll モードに戻る |

### ヤンク（コピー）

![yank demo](../assets/demo-yank.gif)

| キー | 操作 |
|------|------|
| `yy` | 行をヤンク |
| `yw` / `ye` / `yb` | 単語 / 単語末尾 / 単語先頭までヤンク |
| `y$` / `y0` | 行末 / 行頭までヤンク |
| `y`（Visual モード） | 選択範囲をヤンク |

テキストオブジェクトも対応: `iw`, `aw`, `i"`, `a"`, `i(`, `a(`, `i{`, `a{`

### 検索

| キー | 操作 |
|------|------|
| `/` | 検索を開始 |
| `n` | 次のマッチへ |
| `N` | 前のマッチへ |

全ペイン（DiffView、FileTree、CommitLog、Reflog）で検索可能。大文字小文字を区別しない。

### GitHub View

GitHub の Issue と Pull Request を vig 内で閲覧可能。[GitHub CLI (`gh`)](https://cli.github.com/) のインストールと認証が必要。

| キー | 操作 |
|------|------|
| `h` / `l` | Issue 一覧 ↔ PR 一覧 |
| `j` / `k` | リスト内ナビゲーション |
| `i` / `Enter` | 詳細ビューを開く |
| `o` | ブラウザで開く |
| `Esc` | 一覧に戻る |
| `Ctrl+d` / `Ctrl+u` | 半ページスクロール（詳細ビュー） |
| `g` / `G` | 先頭 / 末尾 |
| `r` | データ再取得 |

### その他

| キー | 操作 |
|------|------|
| `Enter` / `Space` | ファイルを開く / ディレクトリの展開・折りたたみ |
| `e` | 外部エディタで開く |
| `r` | 差分とブランチを更新 |
| `?` | ヘルプを表示 |
| `q` / `Ctrl+c` | 終了 |

## 開発

### セットアップ

```bash
mise install   # prek をインストール
mise exec -- prek install   # pre-commit フックをインストール
```

### Pre-commit フック

[prek](https://github.com/j178/prek) で管理:

- `cargo fmt --check`
- `cargo clippy`
- 末尾空白、EOF 修正、TOML/YAML チェック、マージコンフリクト検出、大容量ファイルチェック
- GIF 鮮度チェック（tape 変更時は GIF の再生成が必須）

### CI

GitHub Actions が `main` への push と PR で実行:

- prek フック（fmt + clippy）
- `cargo test`
- `cargo build`

## ライセンス

MIT
