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
- **Files View** — yazi 風の 3 カラムファイルブラウザ（親 / 現在 / プレビュー）。シンタックスハイライト付きプレビュー
- `~/.config/vig/config.kdl` でレイアウト・キーバインド・ハイライトテーマをカスタマイズ可能

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

## 設定

設定なしでそのまま使えます。レイアウト・キーバインド・ハイライトテーマを変えたい場合は
`~/.config/vig/config.kdl`（または `--config <path>` / `$VIG_CONFIG`）に
KDL ファイルを置きます。書いた部分だけが上書きされ、それ以外はデフォルトのままです。

![config demo](../assets/demo-config.gif)

```kdl
// ~/.config/vig/config.kdl
theme "Solarized (dark)"
page "git" {
    pane "file_tree" {
        keys {
            "o" "ExpandOrOpen"   // バインドを追加
            "Space" "None"       // バインドを解除
        }
    }
}
```

```bash
vig config path     # 使用される設定ファイルのパスを表示
vig config dump     # 組み込みデフォルトを出力（これをコピーして編集）
vig config themes   # 利用可能なハイライトテーマを一覧表示
```

レイアウトの入れ替え（サイドバーを右側にする等）も可能です。設定に誤りがあると
デフォルトへ黙ってフォールバックせず、ファイルパスと行番号付きで起動時にエラーになります。
スキーマの全体は [config.md](config.md) を参照してください。

## キーバインド

### View 切り替え

| キー | 操作 |
|------|------|
| `1` | Git View に切り替え |
| `2` | GitHub View に切り替え |
| `3` | Files View に切り替え |

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
本文とコメントは Markdown としてレンダリングされる (見出し・リスト・タスクリスト・コード・可能な範囲でペイン幅に収まるよう縮めたテーブル)。

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

### Files View

![files demo](../assets/demo-files.gif)

リポジトリをルートとした読み取り専用のファイルブラウザです。左カラムが親ディレクトリ、
中央が現在のディレクトリ、右が選択中エントリのプレビュー（テキストはシンタックスハイライト、
ディレクトリは一覧）です。エントリにはファイル種別ごとの Nerd Font アイコンが付きます。
端末のフォントが [Nerd Font](https://www.nerdfonts.com/) でない場合は設定に `icons "none"` を書いてください。

| キー | 操作 |
|------|------|
| `j` / `k` | 選択移動（プレビューが追従） |
| `l` / `→` / `Enter` | ディレクトリに入る / プレビューにフォーカス |
| `h` / `←` / `Backspace` | 親ディレクトリへ |
| `i` | プレビューにフォーカス |
| `j` / `k` / `Ctrl+d` / `Ctrl+u`（プレビュー） | スクロール |
| `h` / `Esc`（プレビュー） | ファイル一覧に戻る |
| `/` `n` `N` | ファイル名検索 |
| `e` | 選択中のファイルを外部エディタで開く |
| `o` | 選択中のファイル / ディレクトリを OS の既定アプリで開く (`open` / `xdg-open` / `explorer`) |
| `O` | アプリ名を入力して選択中の項目を開く (macOS では `open -a <app>`) |
| `r` | 現在のディレクトリを再読込 |

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
