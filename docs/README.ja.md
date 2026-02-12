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

## インストール

### 必要なもの

- Rust ツールチェイン
- libgit2, libssl, pkg-config

### ソースからビルド

```bash
cargo install --path .
```

## 使い方

Git リポジトリ内で実行:

```bash
vig
```

## キーバインド

### ペイン操作

| キー | 操作 |
|------|------|
| `Tab` / `Shift+Tab` | ペイン切り替え: Files → Branches → Reflog → GitLog → Diff |
| `h` / `l` | 隣接ペインへ移動 |
| `i` | 次のペインへ移動 |

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
| `j` / `k` | ログをスクロール |
| `Ctrl+d` / `Ctrl+u` | 半ページスクロール |
| `g` / `G` | 先頭 / 末尾 |
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
| `Esc` | 検索クリア / Git Log へ戻る |

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
