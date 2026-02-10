# vig

[English](../README.md)

Git の差分をサイドバイサイドで表示する TUI ビューア。vim スタイルのキーバインドで操作できます。

![demo](../assets/demo.gif)

## 特徴

- サイドバイサイド diff ビュー（シンタックスハイライト付き）
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

### ナビゲーション

| キー | 操作 |
|------|------|
| `j` / `k` | 下 / 上にスクロール |
| `h` / `l` | 左 / 右にスクロール |
| `gg` | 先頭にジャンプ |
| `G` | 末尾にジャンプ |
| `Ctrl+d` / `Ctrl+u` | 半ページ下 / 上 |
| `Tab` / `Shift+Tab` | ペイン切り替え |

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

### その他

| キー | 操作 |
|------|------|
| `Enter` / `Space` | ファイルを開く / ディレクトリの展開・折りたたみ |
| `e` | 外部エディタで開く |
| `r` | 差分を更新 |
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

### CI

GitHub Actions が `main` への push と PR で実行:

- prek フック（fmt + clippy）
- `cargo test`
- `cargo build`

## ライセンス

MIT
