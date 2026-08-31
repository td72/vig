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
- **GitHub View** — Issue と Pull Request（本文、コメント、レビュー、CI ステータス）、Actions のワークフロー実行（ジョブ / ステップ、ジョブログ）を閲覧。`gh` CLI 使用（読み取り専用）
- **Files View** — yazi 風の 3 カラムファイルブラウザ（親 / 現在 / プレビュー）。シンタックスハイライト付きプレビュー
- **Docker View** — compose プロジェクトごとにまとめたコンテナ一覧、イメージ一覧、inspect サマリ、ログのライブ tail。`docker` CLI 使用（読み取り専用）
- **Procs View** — 読み取り専用のプロセスツリー（CPU / メモリ）、LISTEN 中のポートとその所有プロセス、システム CPU / メモリ履歴グラフ、CPU / RSS スパークライン付きプロセス詳細
- **Worktrees View** — git worktree と stash を一覧し、HEAD コミットや stash の差分（サイドバイサイド）をプレビュー
- **Projects View** — リポジトリにリンクされた GitHub Projects (v2) のボード。`Status` ごとのカンバン列、ソートできるテーブルモード、プロジェクトフィールドをすべて表示するアイテム詳細。`gh` CLI 使用（読み取り専用）
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
`pages` 行で表示するビューとタブの順番を選べます。書かなかったページは無効になります。

![config demo](../assets/demo-config.gif)

```kdl
// ~/.config/vig/config.kdl
theme "Solarized (dark)"
pages "git" "files" "worktrees"   // この 3 タブだけを、この順で表示
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
| `4` | Docker View に切り替え |
| `5` | Procs View に切り替え |
| `6` | Worktrees View に切り替え |
| `7` | Projects View に切り替え |

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

![github demo](../assets/demo-github-pr.gif)

GitHub の Issue・Pull Request・Actions のワークフロー実行を vig 内で閲覧可能。[GitHub CLI (`gh`)](https://cli.github.com/) のインストールと認証が必要。
本文とコメントは Markdown としてレンダリングされる (見出し・リスト・タスクリスト・コード・可能な範囲でペイン幅に収まるよう縮めたテーブル)。
sub-issue は親 issue の下に、GitHub Stack ([`gh stack`](https://github.com/github/gh-stack) で作るスタック PR) は下から順に土台の PR の下にツリー表示される。

3 列目には最新 50 件のワークフロー実行（`gh run list`）がステータス・ワークフロー名・実行番号・
ブランチ・イベント・所要時間（実行中は経過時間）・経過日時とともに並び、queued / in progress の
実行がある間は 5 秒ごと（設定の `github-poll-interval`）に更新されます。実行を選ぶと詳細エリアの **Jobs** サブペインにジョブが
ステップをぶら下げたツリーで表示され（失敗したステップは赤）、ジョブまたはステップで `Enter` を
押すとそのジョブのログが **Log** サブペインに表示されます（ステップ境界と `##[group]` はセクション行）。
実行中ジョブのログは同じ間隔でポーリングして tail のように追記されます。
GitHub にレート制限された場合はページ内のポーリングを指数バックオフ（30 秒〜最大 10 分）で停止し、
ステータスバーに `⚠ GitHub rate limited (resets in Nm)` を表示します（`r` で即時リトライ、成功で解除）。
このビューが実行を再実行・キャンセル・削除することはありません。

| キー | 操作 |
|------|------|
| `h` / `l` | Issues / Pull Requests / Workflow Runs の列を切り替え |
| `Tab` / `Shift+Tab` | 列を順に切り替え（詳細ビュー内ではサブペインを切り替え） |
| `j` / `k` | リスト内ナビゲーション（詳細は選択に追従） |
| `i` / `Enter` | 詳細ビューを開く |
| `o` | ブラウザで開く（issue / PR / 実行、または選択中のジョブ） |
| `Esc` | 一覧に戻る |
| `h` / `l`（詳細） | 本文 ↔ 右側のサブペイン。実行の場合は Jobs ↔ Log |
| `i` / `Enter`（実行の詳細・Jobs） | ジョブのログを表示（ステップ行ならそのステップへスクロール） |
| `]` / `[`（実行の詳細） | ログ内の次 / 前の失敗ステップへ |
| `G`（実行の詳細・Log） | 末尾へ移動して follow を再開 |
| `/` `n` `N` | 検索: `#番号` / タイトル、ワークフロー / ブランチ / イベント、実行の詳細ではジョブ・ステップ名 / ログ行 |
| `Ctrl+d` / `Ctrl+u` | 半ページスクロール（詳細ビュー） |
| `g` / `G` | 先頭 / 末尾 |
| `r` | データ再取得（詳細ビューではその項目のみ。実行はジョブとログを再取得） |

### Files View

![files demo](../assets/demo-files.gif)

リポジトリをルートとした読み取り専用のファイルブラウザです。左カラムが親ディレクトリ、
中央が現在のディレクトリ、右が選択中エントリのプレビュー（テキストはシンタックスハイライト、
ディレクトリは一覧）です。エントリにはファイル種別ごとの Nerd Font アイコンが付きます。
端末のフォントが [Nerd Font](https://www.nerdfonts.com/) でない場合は設定に `icons "none"` を書いてください。

画像 (PNG / JPEG / GIF / WebP) はペイン内にプレビューされ、1行目に形式・寸法・サイズ・使用中のレンダラが表示されます。グラフィックプロトコル対応端末 (Kitty, WezTerm, Ghostty, iTerm2、foot などの Sixel 対応端末) では元の解像度で描画され、それ以外では Unicode 半ブロックにフォールバックします。`image-preview "halfblocks"` で端末検出をスキップ、`"none"` でメタデータのみの表示になります。20 MB を超える画像はデコードしません。

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

### Docker View

![docker demo](../assets/demo-docker.gif)

ローカルの Docker デーモンを読み取り専用で閲覧するビューです。`docker` CLI の JSON 出力
(`docker ps` / `docker images` / `docker inspect` / `docker logs`) だけを使います。`docker` が
インストールされていない、またはデーモンが起動していない場合はペインの代わりに通知を表示します。
コンテナは compose プロジェクトごとにまとめて表示され（実行中が先頭）、詳細ペインには選択中の
コンテナ / イメージの inspect サマリ、ログペインには選択中コンテナのログ (`--tail 200` のあと
follow 中は毎秒 `--since` で追記) が表示されます。一覧は 5 秒ごとに更新されます。
環境変数は決して表示されず、このビューがコンテナを起動・停止・削除することもありません。

| キー | 操作 |
|------|------|
| `j` / `k` | 選択移動（詳細とログが追従） |
| `i` / `Enter` | 詳細ペインにフォーカス |
| `l`（コンテナ一覧） | ログペインにフォーカス |
| `Tab` / `Shift+Tab` | ペイン切り替え: Containers → Images → Detail → Logs |
| `j` / `k` / `Ctrl+d` / `Ctrl+u`（詳細・ログ） | スクロール（ログをスクロールすると follow が一時停止） |
| `G`（ログ） | 末尾へ移動して follow を再開 |
| `/` `n` `N` | コンテナ / イメージ名、またはログ行を検索 |
| `h` / `Esc`（詳細・ログ） | 一覧に戻る |
| `r` | コンテナ・イメージ・詳細・ログを再取得 |

### Procs View

![procs demo](../assets/demo-procs.gif)

「いま何が動いているか」を読み取り専用で眺めるビューです。プロセスを親 pid ごとのツリーで CPU % と常駐メモリ付きで表示し、LISTEN 中の TCP / UDP ポートとその所有プロセス、選択中プロセスの詳細（pid、ppid、ユーザー、状態、経過時間、CPU / メモリ、完全なコマンドライン、cwd、実行ファイル、子プロセス、LISTEN ポート）を表示します。権限がなく読めない値は `(no access)` と表示され、環境変数は一切読み取り・表示しません。このビューは見るだけで、シグナルを送ることはありません。

プロセス情報は [sysinfo](https://crates.io/crates/sysinfo)、ポートは macOS では `lsof`、Linux では `ss` から取得します。ビュー表示中は 2 秒ごと（設定の `procs-refresh-interval`）と `r` で再読込します。

上部の System ペインはマシン全体の値を btop 風の塗りつぶしエリアチャートで表示します。グローバル
CPU %（直近ピーク付き）と使用メモリを直近 `procs-history` サンプル分（デフォルト 120 = 2 秒間隔で
4 分）描き、スワップがあれば `Swp` 行も表示します。各サンプル列は負荷で色分けされます — 50 % 未満は
緑、50 % 以上は黄、80 % 以上は赤 — パーセント表示のラベルも同じ色です。`c` で CPU チャートを
コアごとの小さなゲージ表示（同じグラデーション）に切り替えられます。チャートはバッファが埋まるまで
右詰めで伸び、ビュー表示中のみサンプリングされ、常にマシン全体が対象です（数値のみを描画します）。
詳細ペインには選択中プロセスの CPU %（色分けあり）/ 常駐メモリの履歴チャートが CPU / MEM 欄の下に
表示されます。

| キー | 操作 |
|------|------|
| `j` / `k` / `Ctrl+d` / `Ctrl+u` / `g` / `G` | プロセスツリー内の移動（詳細が追従） |
| `s` | ソート切り替え: CPU → MEM → PID（ペインタイトルに表示） |
| `c` | CPU グラフ切り替え: 履歴 ⇄ コアごとのバー |
| `Enter` / `i` / `l` | 詳細ペインにフォーカス |
| `/` `n` `N` | 検索（プロセス: コマンドライン / ポート: アドレス・ポート・名前） |
| `Tab` / `Shift+Tab` | ペイン切り替え: Processes → Ports → Detail → System |
| `Enter`（ポート） | ポートを所有するプロセスへジャンプ |
| `j` / `k` / `Ctrl+d` / `Ctrl+u`（詳細） | スクロール |
| `h` / `Esc`（詳細） | プロセス一覧に戻る |
| `r` | 今すぐ再読込 |

### Worktrees View

![worktrees demo](../assets/demo-worktrees.gif)

リポジトリの worktree と stash を読み取り専用で一覧するビューです。左上のペインは
worktree の一覧（`git worktree list`）で、パス（可能なら main worktree からの相対パス）、
チェックアウト中のブランチ（detached HEAD の場合はそのハッシュ）、`[main]` `[locked]`
`[prunable]` `[bare]` などのフラグを表示します。vig を起動した worktree には `*` が付きます。
左下のペインは stash の一覧（`stash@{n}`、メッセージ、作成元ブランチ、経過時間）です。

右のプレビューは選択に追従します。worktree を選ぶと HEAD コミット（ハッシュ、作者、日時、
サブジェクト）と変更ファイルを、stash を選ぶとその差分（untracked ファイルを含む）を
Git View と同じサイドバイサイド diff ビューで表示します。シンタックスハイライト、検索、
Normal / Visual モードとヤンクもそのまま使えます。apply / drop / add / remove などの
変更操作は一切行いません。

| キー | 操作 |
|------|------|
| `Tab` / `Shift+Tab` | ペイン切り替え: Worktrees → Stashes → Preview |
| `j` / `k` | 選択移動（プレビューが追従） |
| `i` / `l` / `Enter` | プレビューにフォーカス |
| `j` / `k` / `Ctrl+d` / `Ctrl+u`（プレビュー） | スクロール |
| `h` / `l`（プレビュー） | diff を左右にスクロール |
| `[` / `]`（プレビュー） | 複数ファイルの stash で前 / 次のファイルへ |
| `i`（プレビュー） | stash diff の Normal モード（`v` / `V` / `y` は Git View と同じ） |
| `Esc` / `Backspace`（プレビュー） | 一覧に戻る |
| `/` `n` `N` | 検索（worktree はパス / ブランチ、stash はメッセージ / ブランチ、プレビューは diff 内） |
| `r` | worktree と stash を再読込 |

### Projects View

![projects demo](../assets/demo-projects.gif)

現在のリポジトリにリンクされた GitHub Projects (v2) を読み取り専用のボードとして眺める
ビューです（リンクの取得は `gh repo view --json projectsV2`、ボードは
`gh project field-list` / `gh project item-list --format json`）。ボードは全幅で表示され、
最初のリンク済みプロジェクトがすぐに読み込まれます。列は `Status` の選択肢を GitHub 上の順に
1 列ずつ、加えてステータス未設定のアイテム用の `No status` 列です。リンクされたプロジェクトが
複数あるときはヘッダに `Board: <タイトル> (i/n)` と出て、`p` / `P` で順に切り替えられます。
1 つもリンクされていないときはリンク方法（リポジトリの Projects タブ、または
`gh project link`）を案内します。設定のトップレベルに `projects-board` を書くと、
タイトルまたはプロジェクト番号でボードを 1 つに固定できます（[config.md](config.md)）。カードにはアイテム種別（`●` issue、`⇅` pull request、
`✎` draft）、番号、タイトル、担当者を表示し、別リポジトリのアイテムには番号の前に薄い色で
`owner/repo` が付きます。`t` でテーブルモードに切り替わり、1 行 1 アイテムで
プロジェクトのフィールド（Status、Priority、Estimate、Iteration、日付、カスタムのテキスト /
数値フィールド）を列として表示・ソートできます。詳細ペインは選択中アイテムの全フィールド値に続けて、
GitHub View と同じ issue / PR の本文とコメントを表示します（draft は本文のみ）。ボードは
`--limit 500` で取得し、それを超えるプロジェクトではステータスバーに `(truncated)` と出ます。

`projects` 一覧ペインも実装されていますが、組み込みレイアウトには配置されていません。
設定でレイアウトに配置すると、リンク済みプロジェクトを選べる一覧が戻ってきます。
貼り付けられるレイアウト例は [config.md](config.md) を参照してください。

`gh project` にはトークンの `project` スコープが必要です。無い場合はペインの代わりに案内を
表示するので、`gh auth refresh -s project` を実行してから `r` を押してください。このビューから
アイテムの追加・移動・編集・削除は一切行いません。

| キー | 操作 |
|------|------|
| `p` / `P` | 次 / 前のリンク済みプロジェクトへ |
| `h` / `l`、`←` / `→`（ボード） | 前 / 次の列へ（テーブルモードではソート列の切り替え） |
| `j` / `k`（ボード） | 列内のカード移動（テーブルモードでは行移動） |
| `t`（ボード） | テーブルモードの切り替え |
| `s`（ボード、テーブルモード） | ソート列を順に切り替え |
| `Enter` / `i`（ボード） | 詳細ペインにフォーカス |
| `o` | プロジェクト / アイテムをブラウザで開く |
| `j` / `k` / `Ctrl+d` / `Ctrl+u`（詳細） | スクロール |
| `h` / `Esc`（詳細） | ボードに戻る |
| `Tab` / `Shift+Tab` | ペイン切り替え: Board → Detail |
| `/` `n` `N` | 検索（列をまたいだアイテムのタイトル / 番号） |
| `r` | リンク済みプロジェクト・ボード・表示中アイテムを再取得 |

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
