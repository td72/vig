# GitHub View

![github demo](../../../../assets/demo-github-pr.gif)

GitHub の Issue・Pull Request・Actions のワークフロー実行を vig 内で直接
閲覧できます。上部に 3 つの列が並び、それぞれに詳細ビューがあります。
[GitHub CLI (`gh`)](https://cli.github.com/) のインストールと認証が必要で、
無い場合はペインの代わりに案内を表示します。

本文とコメントは Markdown としてレンダリングされます（見出し・リスト・
タスクリスト・コード・可能な範囲でペイン幅に収まるよう縮めたテーブル）。
sub-issue は親 issue の下に、GitHub Stack
（[`gh stack`](https://github.com/github/gh-stack) で作るスタック PR）は
下から順に土台の PR の下にツリー表示されます。

## Issue と Pull Request

![github issue demo](../../../../assets/demo-github-issue.gif)

`j` / `k` でリストを移動すると詳細が追従します。`i` または `Enter` で詳細
ビューに入り、`h` / `l` で本文と右側のサブペイン（コメント、レビュー、PR の
CI ステータス）を切り替えます。`o` でブラウザで開きます。issue / PR の詳細
では `w` で **watch モード** を切り替えられます。約 10 秒ごとに表示中の
アイテムを再取得するので、返信や CI の結果を待つあいだ最新のまま眺めて
いられます。

Pull Request で `d` を押すと（一覧でも詳細でも）、その PR の diff をページ
の上に重なるウィンドウで開きます。左に変更ファイルのツリー、右にサイド
バイサイドの diff という、[Git view](views-git.md) が作業ツリーの差分を
見せるのと同じ形です（`Space` で折りたたみ、`/` で検索、`i` でヤンクや
ビジュアル選択ができるカーソルモード）。ヘッダに PR 番号・ブランチ・
タイトル、ステータスバーにファイル数と行数が出ます。diff は `gh pr diff`
で取得します。REST なので GraphQL ポイントを消費せず、fork からの PR や
fetch していないブランチでも開けます。最後に見た diff はディスクに
キャッシュされ、新しいものを取得するあいだ先に表示されます（`r` で
再取得）。`Esc` は diff → ツリー → 閉じる、と一段ずつ戻り、`q` はすぐ
閉じます。`b` で PR の Files タブをブラウザで開き、`y` でその URL を
コピーします。作業ツリーにあるファイルは `e` / `o` / `O` で開けます
（`$EDITOR`、OS の既定アプリ、名前を指定したアプリ。PR のファイルが
手元にあるとは限りません。PR そのものの checkout は後述の `W`）。
GitHub が描画を断るほど大きい diff はステータスバーにその旨が出ます。

Pull Request で `W` を押すと、もう一歩進んで PR をリポジトリの隣に
**git worktree** として checkout します。現在の checkout を触らずに、
動かす・テストする・直す、ができます。vig はまず実行する 2 つの
コマンドを見せて `y` を待ちます:

```text
git fetch origin refs/pull/<n>/head
git worktree add ../<repo>-pr-<n> -b pr/<n> FETCH_HEAD
```

`refs/pull/<n>/head` はすべての PR にあるので、fork からの PR も同じ手順
です。fetch はバックグラウンドで走り、worktree ができると
[Worktrees view](views-worktrees.md) に切り替わってその worktree が選択
されます（diff ウィンドウに戻れば `e` がその checkout のファイルを開き
ます）。その PR の worktree が既にある（そのブランチを checkout している
か、そのパスにある）場合は聞き直さずにそれを表示します。同名のディレクトリ
やブランチがあって PR の worktree ではない場合はエラーにして何も作りません。
場所と名前は [`worktree-dir`](config-reference.md#worktree-dir)・
[`worktree-name`](config-reference.md#worktree-name)・
[`worktree-branch`](config-reference.md#worktree-branch) で決まります。
vig が worktree を削除することはありません（`git worktree remove` は
あなたの手で）。

一覧に並ぶのは open なものです。`x` で closed な issue とマージ済み /
closed な PR も含めて表示し（薄い色と状態アイコン: `✓` closed、`⊕` merged。
ヘッダは `GitHub · closed`）、もう一度 `x` で外します。どちらも `gh` の
一覧コマンドなので GraphQL ポイントは使いません。
[`github-show-closed`](config-reference.md#github-show-closed) で最初から
含めた状態で始められます。

issue / PR の一覧も API ポイントを使わずに最新を保ちます。vig は
`github-poll-interval` ごとに、リポジトリで最後に更新された issue / PR を
1 件だけ *条件付き* リクエストで問い合わせ（`304 Not Modified` の応答は
無料）、その答えが変わったときだけ両方の一覧と表示中の詳細を再取得します。
Workflow Runs 列も、動いている実行がない間は同じチェック（最新の実行 1 件）
を独自に行うので、実行が始まれば `r` なしで一覧に現れます。
ステータスバーには一覧の経過時間（`lists 12s ago`）と、再取得直後は短く
`↻ updated` が出ます。これらのチェックも他のポーリングと同様に
`github-auto-refresh`・アイドル時の間隔延長・残量が少ないときの抑制に
従います。

## Workflow Runs

3 列目には最新 50 件のワークフロー実行（`gh run list`）がステータス・
ワークフロー名・実行番号・ブランチ・イベント・所要時間（実行中は経過時間）・
経過日時とともに並び、queued / in progress の実行がある間は 5 秒ごとに
更新されます。

実行を選ぶと詳細エリアの **Jobs** サブペインにジョブがステップをぶら下げた
ツリーで表示され（失敗したステップは赤）、ジョブまたはステップで `Enter` を
押すとそのジョブのログが **Log** サブペインに表示されます（ステップ境界と
`##[group]` はセクション行）。実行中ジョブのログは 5 秒ごとにポーリングして
tail のように追記されます。`]` / `[` で失敗ステップ間をジャンプ、`G` で
末尾へ移動して follow を再開します。

## キーバインド

| キー | 操作 |
|------|------|
| `h` / `l` | Issues / Pull Requests / Workflow Runs の列を切り替え |
| `Tab` / `Shift+Tab` | ページ内のペインを順に切り替え — 3 つの列と詳細（詳細ビュー内ではサブペイン） |
| `j` / `k` | リスト内ナビゲーション（詳細は選択に追従） |
| `i` / `Enter` | 詳細ビューを開く |
| `o` | ブラウザで開く（issue / PR / 実行、または選択中のジョブ） |
| `y` | アイテムの URL をコピー（ローカルで構築、API 不使用） |
| `Esc` | 一覧に戻る |
| `h` / `l`（詳細） | 本文 ↔ 右側のサブペイン。実行の場合は Jobs ↔ Log |
| `w`（issue / PR の詳細） | watch モードの切り替え（表示中アイテムの自動再取得） |
| `x`（issue / PR の一覧） | closed な issue / マージ済み・closed な PR を表示 / 非表示 |
| `d`（PR の一覧 / 詳細） | PR の diff をウィンドウで表示: 変更ファイル + サイドバイサイド diff（`q` / `Esc` で戻る、`b` Files タブ、`y` コピー、`e` / `o` / `O` でファイルを編集 / 起動、`r` 再取得） |
| `W`（PR の一覧 / 詳細） | PR を worktree として開く: 確認後に head を `git fetch` + `git worktree add`、Worktrees view でその worktree へ |
| `i` / `Enter`（実行の詳細・Jobs） | ジョブのログを表示（ステップ行ならそのステップへスクロール） |
| `]` / `[`（実行の詳細） | ログ内の次 / 前の失敗ステップへ |
| `G`（実行の詳細・Log） | 末尾へ移動して follow を再開 |
| `/` `n` `N` | 検索: `#番号` / タイトル、ワークフロー / ブランチ / イベント、実行の詳細ではジョブ・ステップ名 / ログ行 |
| `Ctrl+d` / `Ctrl+u` | 半ページスクロール（詳細ビュー） |
| `g` / `G` | 先頭 / 末尾 |
| `r` | データ再取得（詳細ビューではその項目のみ。実行はジョブとログを再取得） |

## 制約

- すべて `gh` CLI 経由なので、その認証とレートリミットの影響を受けます。
- このビューが実行を再実行・キャンセル・削除することはありません。
  コメントも編集もマージもできません。読むだけです。
