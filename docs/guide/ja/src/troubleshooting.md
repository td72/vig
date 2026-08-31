# トラブルシューティング / FAQ

実際に踏みがちな問題を、vig が表示するそのままのメッセージと、それぞれの
抜け道つきでまとめます。設定変更で直るものは
[設定リファレンス](config-reference.md) にリンクしています。

## vig が起動しない（設定エラー）

vig は起動時にユーザー設定を検証し、**フェイルファスト**です: 構文
エラー・未知のノード・不正な値など、どんな問題でもファイル名を含む
メッセージとともに vig は止まります。黙ってデフォルトにフォールバック
することはありません。実物:

```text
Error: invalid config file /home/you/.config/vig/config.kdl

Caused by:
    unknown top-level block "theem" (expected `theme`, `icons`, `image-preview`,
    `procs-refresh-interval`, `procs-history`, `github-poll-interval`,
    `projects-board`, `pages`, `repo-config`, `app`, or `page`)
```

読み方: 1 行目がファイル名、Caused by が vig の受け付けなかったものと、
期待される候補です — ここでは `theme` のタイポ。構文エラーにはさらに
`ファイル:行:桁` が付きます。カテゴリの一覧は
[起動時エラー](config-reference.md#起動時エラー) にあります。

素早く直すための道具が 3 つ:

- `vig config path` — 各レイヤの状態を表示。壊れたユーザー設定は TUI を
  起動しようとせず、同じメッセージ付きで `invalid (…)` と出ます。
- `vig --config ./try.kdl` — 本物の設定に触らず、別ファイルで実験。
- `vig config dump` — 常に正しい形を写せるリファレンス。

リポジトリローカルの `.vig.kdl` だけは起動を止めません: 壊れていると
vig は組み込み + ユーザーで起動し、ステータスバーに
`ignored .vig.kdl: <理由>`（stderr にも 1 行）が出ます。

## GitHub ビューがペインの代わりにエラーを表示する

GitHub・Projects ビュー（とそのポーリング）は
[GitHub CLI（`gh`）](https://cli.github.com/) を経由します。よくある
原因は 2 つ:

- **`gh` が入っていない** — ステータスバーに起動エラー（例:
  `gh not found: No such file or directory`）が出ます。GitHub CLI を
  インストールして `PATH` に置いてください。
- **`gh` が未認証** — `gh` のエラーがそのまま表示されます。
  `gh auth login` を実行してから、ビューで `r` を押してリトライして
  ください。

vig 自身があなたのトークンを読むことはありません — 認証は完全に `gh`
のものです。

## Projects ビュー: `gh needs the project scope`

Projects ビューは `gh project …` を使い、これにはトークンの `project`
スコープが必要です — `gh auth login` がデフォルトでは付けないスコープ
です。無い場合、ビューはペインの代わりに案内を表示し、ステータスバーに
こう出ます:

```text
gh needs the project scope: run `gh auth refresh -s project`
```

そのまま実行してください:

```bash
gh auth refresh -s project
```

その後ビューで `r` を押します。（GitHub ビューはこのスコープ無しで
動きます。必要なのは Projects だけです。）

## `⚠ GitHub rate limited`

GitHub がリクエストをレート制限で拒否すると、GitHub ページはすべての
ポーリングを止めて指数バックオフします — 30 秒、60 秒、…最大 10 分 —
そしてステータスバーに `⚠ GitHub rate limited (resets in Nm)` を表示
します。リセット時刻は 1 回の `gh api rate_limit` 呼び出しから取得
します（このエンドポイント自体はレート制限されません）。`r` で即時
リトライ、最初に成功したフェッチでバックオフは解除されます。

頻繁に当たるならポーリングを遅くしてください — vig がポーリングするのは
何かが動いている間だけ（実行中のワークフロー、ウォッチモード、実行中
ジョブのログ）で、間隔は
[`github-poll-interval`](config-reference.md#github-poll-interval)
（デフォルト `"5s"`、最小 `"2s"`）です:

```kdl
github-poll-interval "15s"
```

クォータはあなたのトークンを使うすべてと共有です — 同じアカウントを
ポーリングする他のツールも同じ上限を消費します。

## Files ビューのアイコンが豆腐 / 文字化けになる

あれは Nerd Font のグリフで、端末のフォントに入っていないのが原因です。
[Nerd Font](https://www.nerdfonts.com/) を入れるか、アイコンを切って
ください（[`icons`](config-reference.md#icons)）:

```kdl
icons "none"
```

## 画像プレビューがおかしい（低解像度なのはなぜ？）

Files ビューが画像を元の解像度でプレビューできるのは、グラフィック
プロトコルを持つ端末 — Kitty、WezTerm、Ghostty、iTerm2、foot などの
Sixel 対応端末 — だけです。それ以外（および多くの SSH / マルチプレクサ
構成）ではユニコードのハーフブロックにフォールバックし、これは意図的に
粗い表示です。プレビューの 1 行目に使用中のレンダラが出るので、どちらの
経路になったか確認できます。

自動検出があなたの端末で誤動作する場合は上書きしてください
（[`image-preview`](config-reference.md#image-preview)）:

```kdl
image-preview "halfblocks"   // 検出せず、常にハーフブロック
```

```kdl
image-preview "none"         // 画像は描画しない。メタデータのみ
```

20 MB を超える画像はデコードしません。

## `.vig.kdl` の信頼ダイアログが何度も出る

ダイアログが出るのは git に**追跡されている** `.vig.kdl`（リポジトリと
一緒に来たもの）で、記憶される回答は worktree パス**とファイル内容の
ハッシュ**がキーです — つまりファイルが変わると（pull の後など）意図的
にもう一度確認され、`Esc` は何も記憶しません（記憶するのは `y` / `n`
です）。記憶は CLI から管理できます:

```bash
vig config trust                     # 記憶済みの決定を一覧
vig config trust --forget <path>     # その worktree で次回また確認させる
```

リポジトリローカルレイヤ自体が不要なら、**ユーザー**設定でスイッチを
切ってください（[`repo-config`](config-reference.md#repo-config)）—
読み込みもダイアログも無くなります:

```kdl
repo-config "off"
```

あなた自身の**未追跡**の `.vig.kdl` でダイアログが出ることはありません。

## vig がファイルを置く場所（と消し方）

vig が書き込むのは 3 か所で、どれも消して安全です:

| 何 | どこ | 備考 |
|---|---|---|
| 設定 | `~/.config/vig/config.kdl`（または `$XDG_CONFIG_HOME/vig/config.kdl`） | あなたのファイル。vig は読むだけです。 |
| GitHub ディスクキャッシュ | `<cache>/vig/v1/<owner>/<repo>/` — `<cache>` は Linux では `~/.cache`（`$XDG_CACHE_HOME`）、macOS では `~/Library/Caches` | issue / PR の一覧と詳細のキャッシュ。GitHub ビューを開いた瞬間に中身が出るためのものです。消しても再フェッチ 1 回分のコストだけ。 |
| 信頼ストア | `$XDG_STATE_HOME/vig/trust.json`（`~/.local/state/vig/trust.json`） | 記憶済みの `.vig.kdl` 信頼決定。個別には `vig config trust --forget` を推奨。ファイルごと消してもまた確認されるだけです。 |

vig が認証情報を保存することはありません — GitHub アクセスは `gh` 経由
で、トークンは `gh` が管理します。

## FAQ

### vig がリポジトリを変更することはある？

設計上ありません。vig が行うのは読み取り操作と、ちょうど 2 つの安全な
git コマンド — `git switch` と `git branch -d`（マージ済みでない
ブランチを拒否する安全な削除）— だけで、どちらもブランチのアクション
メニューから、確認のうえでのみ実行されます。merge、rebase、force
delete、push、stash の変更、コンテナ操作、プロセスへのシグナルは一切
ありません。

### キーが効かない / 変な動きをする — どこを見る？

`?` を押してください: ヘルプオーバーレイは*マージ後*の設定から生成
されるので、あなたのリバインドを含めて、いま何がバインドされているかが
正確に出ます。そこに無いキーは `"None"` で外されたか、レイアウトで
ペインが非アクティブになっています。どのレイヤが読まれているかは
`vig config path` で分かります — リポジトリローカルの `.vig.kdl` も
キーをリバインドできることを忘れずに。

### ペインが画面から消えたのはなぜ？

設定（または `.vig.kdl`）のレイアウトがペインを書かないと、そのペインは
*非アクティブ*になります — 領域なし、`Tab` からもスキップ。これは機能
です（[レシピ](config-recipes.md#ペインを置かない)）。Projects ページに
は意図的に置かれていないペインすらあります
（[ページ `projects`](config-reference.md#ページ-projects)）。ペインを
置いた `layout` を書き直せば戻ります。

### vig のアップデート方法は？

`vig update` が最新リリースをダウンロードし、署名を検証してバイナリを
置き換えます — ビルド済みバイナリからのインストール向けです。Homebrew
や cargo で入れた場合は `brew upgrade vig` / `cargo install vig` を
使ってください。

### `gh` や Docker が無くても動く？

動きます。ビューは独立に劣化します: `gh` が無ければ GitHub と Projects
ビューが案内を表示し、Docker デーモンが無ければ Docker ビューが表示
します。Git・Files・Procs・Worktrees ビューはどちらも必要としません。
使わないビューは [`pages`](config-reference.md#pages) で丸ごと無効化
できます。
