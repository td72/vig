# 設定リファレンス

vig の KDL 設定の完全なリファレンスです: 設定ファイルが受け付けるすべての
ノードを、書式・デフォルト・マージ規則・エラーとともに記載します。
引きやすさを優先して、トップレベルノードごとに 1 セクション、続いて
`page` ブロックの要素、最後に全ページのペインとアクション、の構成です。

設定にはじめて触れる場合は、[設定の基本](configuration-basics.md)
（場所・レイヤ・マージモデル）と [設定レシピ](config-recipes.md)
（実例集）から読んでください。この章はそれらを前提に、網羅性を目指します。

この章の表記:

- **書式** — ノードの書き方。特記がない限り値はクォートされた文字列です。
  唯一の例外は `projects-board 2`（裸の整数）。
- **デフォルト** — 組み込みの値。
  [assets/default.kdl](https://github.com/td72/vig/blob/main/assets/default.kdl)
  （`vig config dump` の出力）と同じです。
- **マージ** — あなたの設定がそのノードを書いたときに起きること。
- そのまま読み込める完全な例は `kdl` ブロックで示します — vig のテスト
  スイートが、ユーザー設定とまったく同じ経路で各例を読み込んで検証して
  います。断片やエラーの実演は ignore 指定で示し、出るエラーを注釈します。

## トップレベル一覧

| ノード | デフォルト | マージ規則 |
|---|---|---|
| [`theme`](#theme) | `"base16-eighties.dark"` | 置換 |
| [`icons`](#icons) | `"nerd"` | 置換 |
| [`image-preview`](#image-preview) | `"auto"` | 置換 |
| [`procs-refresh-interval`](#procs-refresh-interval) | `"2s"` | 置換 |
| [`procs-history`](#procs-history) | `"120"` | 置換 |
| [`github-poll-interval`](#github-poll-interval) | `"5s"` | 置換 |
| [`projects-board`](#projects-board) | なし（リンク済み全ボード） | 置換 |
| [`pages`](#pages) | 全 7 ページ | 丸ごと置換 |
| [`repo-config`](#repo-config) | `"on"` | 置換（ユーザー設定のみ） |
| [`app`](#app) | `Ctrl+c` 終了、`1`…`7` ページ切替 | キー単位マージ |
| [`page`](#page-ブロック) | [ページとペイン](#ページとペイン) 参照 | 要素ごと（後述） |

トップレベルノードをすべて書いた設定（各値はデフォルトなので、これは
読み込めて、かつ何も変えません）:

```kdl
theme "base16-eighties.dark"
icons "nerd"
image-preview "auto"
procs-refresh-interval "2s"
procs-history "120"
github-poll-interval "5s"
pages "git" "github" "files" "docker" "procs" "worktrees" "projects"
repo-config "on"
app {
    "Ctrl+c" "Quit"
}
```

これ以外のトップレベルノードはエラーです:

```kdl,ignore
colors "red"
// → unknown top-level block "colors" (expected `theme`, `icons`,
//   `image-preview`, `procs-refresh-interval`, `procs-history`,
//   `github-poll-interval`, `projects-board`, `pages`, `repo-config`,
//   `app`, or `page`)
```

## トップレベルノード

### `theme`

diff ビュー（Git と Worktrees）と Files のプレビューで使う
シンタックスハイライトのテーマ。

- **書式** — `theme "<name>"`
- **デフォルト** — `"base16-eighties.dark"`
- **マージ** — デフォルトを置換。

使えるのは [syntect](https://github.com/trishume/syntect) 同梱のテーマ
だけです。`vig config themes` で一覧できます（`*` が現在有効なもの）:
`InspiredGitHub`、`Solarized (dark)`、`Solarized (light)`、
`base16-eighties.dark`、`base16-mocha.dark`、`base16-ocean.dark`、
`base16-ocean.light`。テーマから使われるのは前景色だけなので、ライト系
テーマが読みやすいのは主にライト背景の端末です。

```kdl
theme "Solarized (dark)"
```

```kdl,ignore
theme "Solarised (dark)"
// → unknown theme "Solarised (dark)"; available: InspiredGitHub, ...
```

### `icons`

Files ビューのファイル種別アイコン。

- **書式** — `icons "<mode>"` — `"nerd"` または `"none"`
- **デフォルト** — `"nerd"`
- **マージ** — デフォルトを置換。

`"nerd"` はファイル種別ごとの Nerd Font グリフを表示し、端末に
[Nerd Font](https://www.nerdfonts.com/) が必要です。`"none"` は
プレーンな名前だけを表示します
（[レシピ](config-recipes.md#ファイルアイコンを消す)）。

```kdl
icons "none"
```

### `image-preview`

Files ビューの画像プレビュー（PNG / JPEG / GIF / WebP）の描画方法。

- **書式** — `image-preview "<mode>"` — `"auto"`・`"halfblocks"`・`"none"`
- **デフォルト** — `"auto"`
- **マージ** — デフォルトを置換。

`"auto"` は端末のグラフィックプロトコル（Kitty、iTerm2、Sixel）を検出し、
なければユニコードのハーフブロックへフォールバックします。
`"halfblocks"` は検出をスキップして常にハーフブロック、`"none"` は画像を
一切描画しません（プレビューにはメタデータ行だけが出ます）。

```kdl
image-preview "halfblocks"
```

### `procs-refresh-interval`

Procs ビューが表示中に、プロセス一覧と LISTEN ポートを読み直す間隔。
他のビューを表示している間、サンプリングは止まります。

- **書式** — `procs-refresh-interval "<duration>"` — `s` か `ms` 付きの
  数値（`"2s"`、`"1.5s"`、`"500ms"`）をクォートして書く。最小 `"250ms"`
- **デフォルト** — `"2s"`
- **マージ** — デフォルトを置換。

```kdl
procs-refresh-interval "5s"
```

```kdl,ignore
procs-refresh-interval "100ms"
// → bad procs-refresh-interval "100ms"; expected a duration such as
//   "2s" or "500ms" (at least 250ms)
```

### `procs-history`

Procs ビューの履歴グラフ — `graphs` ペインのシステム CPU / メモリの
エリアチャートと、詳細ペインのプロセスごとの履歴チャート — が保持する
サンプル数。1 回のリフレッシュにつき 1 サンプルなので、
[`procs-refresh-interval`](#procs-refresh-interval) との掛け算が
履歴の長さになります。

- **書式** — `procs-history "<n>"` — `"10"` 〜 `"10000"` の数値を
  クォートして書く
- **デフォルト** — `"120"`（デフォルトの `"2s"` で 4 分）
- **マージ** — デフォルトを置換。

```kdl
procs-history "300"
```

```kdl,ignore
procs-history "5"
// → bad procs-history "5"; expected a sample count between 10 and 10000
```

### `github-poll-interval`

GitHub ページが**何かが動いている間**にポーリングする間隔 — queued /
in progress の実行がある Workflow Runs 列、ウォッチモード（`w`）の
PR チェック、実行中ジョブのログ。別のページを表示している間、
ポーリングは止まります。

- **書式** — `github-poll-interval "<duration>"` — `s` か `ms` 付きの
  数値をクォートして書く。最小 `"2s"`（設定で API クォータを溶かせない
  ようにするため）
- **デフォルト** — `"5s"`
- **マージ** — デフォルトを置換。

レート制限への対処はこの設定の上に組み込まれています: GitHub が
リクエストをレート制限で拒否すると、ページはすべてのポーリングを指数
バックオフ（30 秒、60 秒、…最大 10 分）で停止し、ステータスバーに
`⚠ GitHub rate limited (resets in Nm)` を表示します。リセット時刻は
1 回の `gh api rate_limit` 呼び出しから取得します（このエンドポイント
自体はレート制限されません）。`r` で即時リトライ、成功すれば
バックオフは解除されます。詳しくは
[トラブルシューティング](troubleshooting.md#-github-rate-limited)。

```kdl
github-poll-interval "10s"
```

### `projects-board`

Projects ページを 1 つのボードに固定します。引数は 1 つで、ボードの
タイトル（文字列。リポジトリにリンクされたプロジェクトに対して大文字
小文字を無視して照合）か、プロジェクト番号 — 設定の中で唯一の裸の
整数 — です:

- **書式** — `projects-board "<title>"` または `projects-board <number>`
- **デフォルト** — なし: リンク済みプロジェクトすべてが対象で、
  `p` / `P` で巡回
- **マージ** — デフォルトを置換。

```kdl
projects-board "Roadmap"
```

```kdl
projects-board 2
```

設定するとページはそのボードだけを表示します: `p` / `P` は巡回しなく
なり（押すとステータスバーに `board pinned by config (projects-board)`
と出ます）、ヘッダから `(i/n)` カウンタが消えます。一致するプロジェクト
が無ければ、ボードペインが指定名を挙げてそう伝えます。

```kdl,ignore
projects-board "Roadmap" 2
// → bad projects-board (one argument required); expected exactly one
//   argument, a board title (`projects-board "Roadmap"`) or a project
//   number (`projects-board 2`)
```

### `pages`

有効にするページを、タブ順で並べます。リスト内の位置がページの
*スロット* — ヘッダに出る番号（`1:Git`、`2:GitHub`、…）であり、`Tab`
巡回の到達位置です。

- **書式** — `pages "<name>" "<name>" ...` — 名前は `git`、`github`、
  `files`、`docker`、`procs`、`worktrees`、`projects` から。最低 1 つ、
  重複なし
- **デフォルト** — この順で全 7 ページ
- **マージ** — デフォルトのリストを**丸ごと**置換。

書かなかったページは無効です — 起動されず、タブもバックグラウンドの
ポーリングもありません:

```kdl
pages "git" "files" "worktrees"
```

は `1:Git 2:Files 3:Worktrees` の 3 タブ構成になります。

キーはスロットではなく*ページに名前で*結び付いたバインドです:
`app { "<key>" "page:<name>" }` はページがどこにいても名前で指すので、
並べ替え後も組み込みの `1` … `7` は同じページに切り替わります。無効化
したページの組み込みキーは外されますが、*あなたの* `app { }` ブロック
から `pages` に無いページへのバインドはエラーです（[`app`](#app) 参照）。

```kdl,ignore
pages "git" "filez"
// → pages: unknown page "filez"; expected one of: git, github, files,
//   docker, procs, worktrees, projects

pages "git" "git"
// → pages: page "git" listed twice

pages
// → pages must list at least one page
```

v0.7.0 の `actions` ページは v0.8.0 で `github` ページ（Workflow Runs
列）に統合されました。まだ書いてある設定は、その旨のメッセージとともに
拒否されます:

```kdl,ignore
pages "git" "actions"
// → pages: page "actions" was folded into the "github" page (v0.8.0);
//   remove it from pages / app bindings
```

### `repo-config`

リポジトリローカルの `.vig.kdl` レイヤ
（[設定の基本](configuration-basics.md#3--リポジトリローカルvigkdl)）を
そもそも読むかどうか。

- **書式** — `repo-config "on"` または `repo-config "off"`
- **デフォルト** — `"on"`
- **マージ** — デフォルトを置換。**有効なのはユーザー設定の値だけ**。

`"off"` にすると `.vig.kdl` は一切読み込まれず、信頼ダイアログも出ません。
このスイッチはリポジトリレイヤをマージする前に読まれるので、`.vig.kdl`
が自分自身を on/off することはできません — `.vig.kdl` に `repo-config`
を書くこと自体が（`"on"` でも）拒否されます:
`repo-config can only be set in the user config, not in .vig.kdl`。

```kdl
repo-config "off"
```

### `app`

全ページで効くグローバルキーバインド。

- **書式** — `app { "<key>" "<action>" ... }`
- **デフォルト** — `"Ctrl+c" "Quit"` と、`"1"` … `"7"` の 7 ページへの
  名前バインド
- **マージ** — キー単位でマージ: 書いたキーだけがそのキーのデフォルトを
  置き換え、書かなかったキーはそのまま。

| アクション | 意味 |
|---|---|
| `"Quit"` | vig を終了 |
| `"page:<name>"` | そのページへ切り替え — `page:git`、`page:github`、`page:files`、`page:docker`、`page:procs`、`page:worktrees`、`page:projects`。[`pages`](#pages) に載っているページに限る。 |
| `"None"` | そのキーのバインドを削除 |

```kdl
app {
    "q" "Quit"            // ペインからでなく、どこからでも終了
    "Ctrl+g" "page:git"   // Git ビューへジャンプ
    "7" "None"            // 組み込みの Projects 切替を解除
}
```

存在はするが有効化されていないページへの、あなた自身の `page:` バインド
はエラーです（無効化したページの*組み込み*バインドは黙って外されます）:

```kdl,ignore
pages "git" "files"
app {
    "d" "page:docker"
    // → app block: "d" "page:docker": page "docker" is not listed in
    //   `pages` (git, files)
}
```

## キー

キーは文字列で、3 つの形のどれかで書きます:

- **1 文字** — `"j"`、`"G"`、`"/"`、`"]"`。大文字小文字は区別されます:
  `"g"` と `"G"` は別のキーです。
- **名前付きキー** — `"Enter"`、`"Esc"`、`"Tab"`、`"BackTab"`、
  `"Space"`、`"Backspace"`、`"Delete"`、`"Up"`、`"Down"`、`"Left"`、
  `"Right"`、`"Home"`、`"End"`、`"PageUp"`、`"PageDown"`。別名も
  いくつか使えます: Enter の `"Return"` / `"CR"`、Esc の `"Escape"`、
  BackTab の `"S-Tab"`、Backspace の `"BS"`、Delete の `"Del"`。
- **`Ctrl+` の組み合わせ** — `"Ctrl+d"`、`"Ctrl+u"`、`"Ctrl+c"`。
  修飾キーは `Ctrl` のみ対応です。`Alt+` やファンクションキーは
  ありません。

予約アクション `"None"` に割り当てるとキーは削除されます — `app { }`
でも、どのペインの `keys { }` でも同じです。

## preset

preset は標準バインドの名前付きセットで、ペインの `keys { }` ブロック内
にその場で展開されます。2 つあります:

| preset | 展開結果 |
|---|---|
| `nav` | `j`/`Down` → `Nav.MoveDown`、`k`/`Up` → `Nav.MoveUp`、`Ctrl+d` → `Nav.HalfPageDown`、`Ctrl+u` → `Nav.HalfPageUp`、`g` → `Nav.JumpTop`、`G` → `Nav.JumpBottom` |
| `search` | `/` → `Search.Start`、`n` → `Search.Next`、`N` → `Search.Prev` |

`Nav.*` と `Search.*` のアクションは、デフォルトに該当 preset を持つ
ペインでなら個別にもバインドできます。規則は 2 つ
（[背景](config-recipes.md#preset-とは何か)）: preset が先に展開され、
同じペイン内の明示バインドが同じキーについて勝つこと。そしてキーの
マージ時、`preset` 行は置換されず常に追加されることです。

```kdl
page "git" {
    pane "diff_view" {
        keys {
            "J" "Nav.HalfPageDown"   // preset のアクションを明示バインド
            "n" "None"               // preset 由来のバインドを削除
        }
    }
}
```

## `page` ブロック

```kdl,ignore
page "<name>" {
    layout { <split | place | slot> }     // 丸ごと置換
    tabs "<pane>" "<pane>" ...            // 丸ごと置換
    bind select="<pane>" detail="<pane>"  // bind 行はまとめて置換
    pane "<name>" { keys { ... } }        // keys はキー単位マージ
}
```

ページ名とペイン名は固定です — 並べ替え・リサイズ・リバインドはできます
が、新しく作ることはできません。どのブロックも省略可能で、`page`
ブロックは書いたものだけを変えます。

### `layout`

ルート要素はちょうど 1 つ。3 種類の要素を何段でもネストできます:

- `split direction="horizontal" { <children> }` — 子を左右に並べる。
  `direction="vertical"` は上下に積む。各子は `size="..."` を取れる。
- `place "<pane>"` — ペインを表示する。
- `slot "<name>" ...` — フォーカスに応じて別のペインを表示する 1 つの
  領域。後述の [slot](#slot) 参照。

**サイズ** — `"30"`（ちょうど 30 セル）、`"40%"`（割合）、`"min:20"`
（最低 20 セル、残りを取る）。省略は `"min:0"`。

**マージ** — `layout` を書くと、そのページの**レイアウト全体**を置き換え
ます。部分マージはありません。`vig config dump` の該当ブロックから
始めて編集してください（[レシピ](config-recipes.md#レイアウトの木を読む)）。

**規則**（どちらも起動時に検査されます）:

- 各ペインを置けるのは**高々 1 回** — `place` 行と、slot については
  その slot が表示しうる各ペインを 1 回と数えます。
- 最低 1 つのペインを置くこと。
- レイアウトに書かなかったペインは**非アクティブ**です: 領域を持たず、
  `Tab` 巡回とフォーカスにスキップされ、そのペインを指す `bind` 行は
  無視されます。組み込みの Projects ページ自体がこの形です —
  [ページ `projects`](#ページ-projects) 参照。

```kdl,ignore
page "git" {
    layout {
        split direction="vertical" {
            place "diff_view"
            place "diff_view"
        }
    }
}
// → page "git": layout places pane "diff_view" more than once

page "git" { layout { } }
// → page "git" layout is empty
```

#### slot

`slot` は、時によって別のペインを表示するレイアウト領域です: 現在
フォーカス中のペインにマッチするケースが勝ちます。形は 2 つあり、
1 つの slot で併用できます:

- **単一ケース** — slot 自体の `then=` と、子の `triggers`:

  ```kdl,ignore
  slot "main" size="min:3" then="git_log" default="diff_view" {
      triggers "branch_list" "reflog" "git_log"
  }
  ```

  `branch_list`・`reflog`・`git_log` のどれかにフォーカスがある間は
  `git_log`、それ以外は `diff_view`（Git ビューの下段）。

- **複数ケース** — `when` の子。それぞれトリガーになるペインを列挙し、
  表示するペインを指名します。フォーカス中のペインを含む最初の `when`
  が勝ち、どれにも当たらなければ `default=`:

  ```kdl,ignore
  slot "detail" size="min:3" default="issue_detail" {
      when "pr_list" "pr_detail" then="pr_detail"
      when "run_list" "run_detail" then="run_detail"
  }
  ```

  GitHub ビューの詳細領域です。各 `when` が自分の `then` ペインを自分の
  トリガーに含めている点に注意 — こうしないと、詳細の*中へ*フォーカスを
  移した瞬間に領域が切り替わってしまいます。

slot の名前（`"main"`、`"detail"`）はただのラベルです。配置の
「高々 1 回」規則では、slot は表示しうる各ペインを 1 回ずつ置いたことに
なります。読み解きは
[slot のレシピ](config-recipes.md#slot-1-つの領域に複数のペイン) に
あります。

### `tabs`

`Tab` / `BackTab` で巡回するペインを順に並べます。

- **書式** — `tabs "<pane>" "<pane>" ...`
- **マージ** — 書けば丸ごと置換。

レイアウトが置いていないペインはスキップされるので、デフォルトの
`tabs` はあなたのレイアウトの下でもそのまま有効です — `tabs` を書き直す
のは巡回順を変えたいときと、デフォルトの順に無いペインを入れたいとき
だけです。

### `bind`

選択ペインがどの詳細ペインを駆動するか — 例えば `file_tree` でファイル
を選ぶと `diff_view` に読み込まれる、の接続です。

- **書式** — `bind select="<pane>" detail="<pane>"`。複数書けます
- **マージ** — ユーザーの `bind` を 1 行でも書くと、そのページの
  デフォルトの `bind` 行は**すべて**置き換わります。

置かれていないペインを指す `bind` は無視されます — そしてレイアウトが
そのペインを置いた瞬間から自動で効き始めます（Projects ページのリスト
ペインが生き返る仕組みです。[ページ `projects`](#ページ-projects) 参照）。

### `pane` と `keys`

```kdl
page "git" {
    pane "file_tree" {
        keys {
            "o" "ExpandOrOpen"   // バインドを追加・上書き
            "Space" "None"       // バインドを削除
        }
    }
}
```

- **マージ** — キー単位で、そのペインのデフォルトキー（展開済み preset
  を含む）の上にマージ。`preset` 行は追加されます。

各ペインが受け付けるアクションは、下のページ別一覧の通りです。`view`
という名前のペインは特別で、実体のあるペインではなくページ全体のキー
（終了・ヘルプ・リフレッシュ・タブ / ペイン巡回）の置き場です。
レイアウトに置くことはできません。ヘルプオーバーレイ（`?`）はマージ後の
キーマップから生成されるので、常にあなたのバインドを反映します。

## ページとペイン

ページごとに: ペイン、各ペインのバインド可能なアクション、各アクションの
組み込みキー。`Nav.*` と `Search.*`（[preset](#preset) 参照）は、
デフォルトに該当 preset を持つすべてのペインで追加で使えます — 下の表
では nav / search preset を持つペインに印を付けています。`Esc` は
すべての操作可能なペインのアクションです（ペインを抜ける / 検索を
クリア）。デフォルトのバインド全体を KDL の形で見るには
`vig config dump` を実行してください。

### ページ `git`

ペイン: `file_tree`、`branch_list`、`git_log`、`reflog`、`diff_view` —
デフォルトレイアウトはすべて配置します（`git_log` と `diff_view` は
`main` slot を共有）。

| ペイン | アクション | デフォルトキー | 意味 |
|---|---|---|---|
| `view` | `Quit` / `Help` / `Refresh` | `q` / `?` / `r` | ページ全体 |
| | `PrevTab` / `NextTab` | `h` / `l` | サイドバーのペイン間を移動 |
| | `CyclePaneForward` / `CyclePaneBackward` | `Tab` / `BackTab` | `tabs` のペインを巡回 |
| | `OpenEditor` | `e` | 選択中ファイルを `$EDITOR` で開く |
| `file_tree` (nav, search) | `ToggleDir` | `Space` | ディレクトリの開閉 |
| | `ExpandOrOpen` | `Enter`、`Right` | ディレクトリを開く / ファイルの diff を開く |
| | `FocusDiff` | `i` | diff ビューにフォーカス |
| `branch_list` (nav, search) | `OpenActionMenu` | `Enter` | switch / 安全な削除 / diff base 設定 |
| | `FocusLog` | `i` | git log にフォーカス |
| `git_log` (nav, search) | `YankHash` | `y` | コミットハッシュをコピー |
| | `OpenGitHub` | `o` | コミットを GitHub で開く |
| | `FocusReflog` | `h` | reflog にフォーカス |
| `reflog` (nav, search) | `SetDiffBase` | `Enter` | このエントリと作業ツリーを比較 |
| | `FocusLog` | `i` | git log にフォーカス |
| `diff_view` (nav, search) | `ScrollLeft` / `ScrollRight` | `h`、`Left` / `l`、`Right` | 横スクロール |
| | `EnterNormalMode` | `i` | vim 風 Normal モード（カーソル・ヤンク・ビジュアル） |

### ページ `github`

ペイン: `issue_list`、`pr_list`、`run_list`（3 つの列）と
`issue_detail`、`pr_detail`、`run_detail`（`detail` slot を共有）。

| ペイン | アクション | デフォルトキー | 意味 |
|---|---|---|---|
| `view` | `Quit` / `Help` / `Refresh` | `q` / `?` / `r` | ページ全体 |
| | `PrevTab` / `NextTab` | `h` / `l` | 列間を移動 |
| | `CyclePaneForward` / `CyclePaneBackward` | `Tab` / `BackTab` | 列と詳細を巡回 |
| `issue_list`、`pr_list`、`run_list` (nav, search) | `OpenDetail` | `i`、`Enter` | 詳細ビューを開く |
| | `SwitchTab` | `Tab`（issues）/ `BackTab`（PR、runs） | 列ローカルのタブ切替 |
| | `OpenBrowser` | `o` | アイテムをブラウザで開く |
| `issue_detail`、`pr_detail` (nav) | `FocusBody` / `FocusRight` | `h` / `l` | 本文 ↔ 右側サブペイン |
| | `CycleForward` / `CycleBackward` | `Tab` / `BackTab` | サブペインを巡回 |
| | `ToggleWatch` | `w` | ウォッチモード: 開いたアイテムを自動更新 |
| | `OpenItem` | `o` | ブラウザで開く |
| `run_detail` (nav, search) | `FocusBody` / `FocusRight` | `h` / `l` | Jobs ↔ Log サブペイン |
| | `CycleForward` / `CycleBackward` | `Tab` / `BackTab` | サブペインを巡回 |
| | `OpenLog` | `i`、`Enter` | 選択中ジョブのログを表示 |
| | `NextFailed` / `PrevFailed` | `]` / `[` | 失敗ステップ間をジャンプ |
| | `OpenItem` | `o` | 実行 / ジョブをブラウザで開く |

`run_detail` では `Nav.JumpBottom`（`G`）が、実行中ジョブのログの
follow 再開も兼ねます。

### ページ `files`

ペイン: `parent_dir`、`dir_list`、`preview` — すべて配置。`parent_dir`
は**表示専用**です: キーの無い `pane` ブロックを持ち、アクションを受け
付けません。

| ペイン | アクション | デフォルトキー | 意味 |
|---|---|---|---|
| `view` | `Quit` / `Help` / `Refresh` | `q` / `?` / `r` | ページ全体 |
| | `CyclePaneForward` / `CyclePaneBackward` | `Tab` / `BackTab` | `dir_list` と `preview` を巡回 |
| | `OpenEditor` | `e` | 選択中ファイルを `$EDITOR` で開く |
| | `OpenDefault` | `o` | OS の既定アプリで開く |
| | `OpenWith` | `O` | アプリ名を指定して開く |
| `dir_list` (nav, search) | `Enter` | `l`、`Right`、`Enter` | ディレクトリに入る / プレビューにフォーカス |
| | `Parent` | `h`、`Left`、`Backspace` | 親ディレクトリへ |
| | `FocusPreview` | `i` | プレビューにフォーカス |
| `preview` (nav) | `Back` | `h`、`Left` | ファイル一覧に戻る |

### ページ `docker`

ペイン: `containers`、`images`、`detail`、`logs` — すべて配置。

| ペイン | アクション | デフォルトキー | 意味 |
|---|---|---|---|
| `view` | `Quit` / `Help` / `Refresh` | `q` / `?` / `r` | ページ全体 |
| | `CyclePaneForward` / `CyclePaneBackward` | `Tab` / `BackTab` | ペインを巡回 |
| `containers` (nav, search) | `OpenDetail` | `i`、`Enter` | inspect サマリにフォーカス |
| | `FocusLogs` | `l` | ログ tail にフォーカス |
| `images` (nav, search) | `OpenDetail` | `i`、`Enter` | inspect サマリにフォーカス |
| `detail` (nav) | `Back` | `h` | 一覧に戻る |
| `logs` (nav, search) | `Back` | `h` | 一覧に戻る |

`logs` では `Nav.JumpBottom`（`G`）が tail の follow 再開も兼ねます。

### ページ `procs`

ペイン: `processes`、`ports`、`detail`、`graphs` — すべて配置。

| ペイン | アクション | デフォルトキー | 意味 |
|---|---|---|---|
| `view` | `Quit` / `Help` / `Refresh` | `q` / `?` / `r` | ページ全体 |
| | `CyclePaneForward` / `CyclePaneBackward` | `Tab` / `BackTab` | ペインを巡回 |
| `processes` (nav, search) | `FocusDetail` | `i`、`l`、`Enter` | プロセス詳細にフォーカス |
| | `CycleSort` | `s` | ソート: CPU → MEM → PID |
| | `TogglePerCore` | `c` | CPU グラフ: 履歴 ⇄ コアごとのバー |
| `ports` (nav, search) | `JumpToProcess` | `Enter` | 所有プロセスへジャンプ |
| `detail` (nav) | `Back` | `h`、`Left` | プロセス一覧に戻る |
| `graphs` | `TogglePerCore` | `c` | CPU グラフ: 履歴 ⇄ コアごとのバー |

### ページ `worktrees`

ペイン: `worktrees`、`stashes`、`preview` — すべて配置。

| ペイン | アクション | デフォルトキー | 意味 |
|---|---|---|---|
| `view` | `Quit` / `Help` / `Refresh` | `q` / `?` / `r` | ページ全体 |
| | `CyclePaneForward` / `CyclePaneBackward` | `Tab` / `BackTab` | ペインを巡回 |
| `worktrees`、`stashes` (nav, search) | `FocusPreview` | `i`、`l`、`Enter` | プレビューにフォーカス |
| `preview` (nav, search) | `ScrollLeft` / `ScrollRight` | `h`、`Left` / `l`、`Right` | 横スクロール |
| | `EnterNormalMode` | `i` | stash diff の Normal モード |
| | `NextFile` / `PrevFile` | `]` / `[` | stash 内の次 / 前のファイル |
| | `Back` | `Backspace` | 一覧に戻る |

### ページ `projects`

ペイン: `projects`、`board`、`detail`。組み込みレイアウトが置くのは
`board` と `detail` だけで、`projects` 一覧ペインは定義されているのに
**置かれていません** — 代わりに `p` / `P` がリンク済みプロジェクトを
巡回し、トップレベルの [`projects-board`](#projects-board) でボードを
固定するとそれも止まります。一覧を戻すには置くだけです — 組み込みの
`bind select="projects" detail="board"` は置いた瞬間から自動で効きます
（[レシピ](config-recipes.md#projects-のリストペインを復活させる)）:

```kdl
page "projects" {
    layout {
        split direction="horizontal" {
            place "projects" size="22%"
            split direction="vertical" size="min:30" {
                place "board" size="60%"
                place "detail" size="min:5"
            }
        }
    }
    tabs "projects" "board" "detail"
}
```

| ペイン | アクション | デフォルトキー | 意味 |
|---|---|---|---|
| `view` | `Quit` / `Help` / `Refresh` | `q` / `?` / `r` | ページ全体 |
| | `NextProject` / `PrevProject` | `p` / `P` | リンク済みプロジェクトを巡回 |
| | `CyclePaneForward` / `CyclePaneBackward` | `Tab` / `BackTab` | ペインを巡回 |
| `projects` (nav, search) | `OpenBoard` | `i`、`l`、`Enter` | 選択中プロジェクトのボードを表示 |
| | `OpenBrowser` | `o` | プロジェクトをブラウザで開く |
| `board` (nav, search) | `PrevColumn` / `NextColumn` | `h`、`Left` / `l`、`Right` | 列間を移動（テーブルモード: ソート列） |
| | `ToggleTable` | `t` | ボード ⇄ テーブルモード |
| | `CycleSort` | `s` | ソート列を順に切替（テーブルモード） |
| | `OpenDetail` | `i`、`Enter` | アイテム詳細にフォーカス |
| | `OpenBrowser` | `o` | アイテムをブラウザで開く |
| `detail` (nav) | `Back` | `h`、`Left` | ボードに戻る |
| | `OpenBrowser` | `o` | アイテムをブラウザで開く |

ボードでの `Esc` は、プロジェクト一覧が置かれていればそこへ戻ります。

## 起動時エラー

ユーザー設定に問題があると、vig は起動せず、ファイル名を含むメッセージを
出します — 設定ファイルがあるのに黙ってデフォルトへフォールバックする
ことはありません。カテゴリ:

- **構文エラー** — `ファイル:行:桁` とパーサのメッセージ付き。例えば
  `page "git" {` を閉じ忘れると:

  ```text
  Error: failed to parse config file /home/you/.config/vig/config.kdl
    /home/you/.config/vig/config.kdl:1:12: No closing '}' for child block
  ```

- **未知の名前** — トップレベルブロック、ページ、ペイン、テーマ、
  icons / image-preview のモード、preset 名、キー、アクションはすべて
  検証され、エラーには期待される候補が並びます。タイポは見過ごされ
  ません。
- **構造エラー** — 同じペインを 2 回置く・何も置かないレイアウト、
  `then=` / `when` ケースの無い slot、`select=` / `detail=` の無い
  `bind`。
- **値エラー** — 最小値未満・単位無しの間隔、範囲外の `procs-history`、
  引数の形がおかしい `projects-board`、`"on"` / `"off"` 以外の
  `repo-config`。
- **相互参照** — `pages` に無いページ、または廃止された `actions`
  ページへの、あなたの `app` バインド。

唯一の例外はリポジトリローカルの `.vig.kdl` レイヤで、中断ではなく劣化
します（ステータスバーに `ignored .vig.kdl: <理由>`）。実際のメッセージ
の読み方は
[トラブルシューティング](troubleshooting.md#vig-が起動しない設定エラー)
を参照してください。
