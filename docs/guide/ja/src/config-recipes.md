# 設定レシピ

実際によく変える設定の実例集です。各レシピは、困りごと +
`~/.config/vig/config.kdl` にそのまま貼れる完全な設定 + 画面がどう
変わるか、で構成されています。このページの `kdl` ブロックはすべて、
vig のテストスイートがユーザー設定とまったく同じ経路で読み込んで
検証しています — 壊れた例は出荷されません。

[設定の基本](configuration-basics.md) をまだ読んでいない場合の 1 行
まとめ: あなたのファイルは
[デフォルト](https://github.com/td72/vig/blob/main/assets/default.kdl)
の部分上書きで、キーのブロックはキー単位でマージ、レイアウトは丸ごと
置換です。

## 見た目

### シンタックスハイライトのテーマを変える

*diff の配色が端末に合わない。*

```bash
vig config themes    # 選択肢を一覧。`*` が現在有効なもの
```

```kdl
theme "Solarized (dark)"
```

次回起動から diff ビュー（Git と Worktrees）と Files のプレビューの
配色が変わります。テーマから使われるのは前景色だけなので、ライト系
テーマ（`InspiredGitHub`、`Solarized (light)`、`base16-ocean.light`）が
読みやすいのは主にライト背景の端末です。

### ファイルアイコンを消す

*Files ビューのアイコンが豆腐 / 文字化けになる。*

あれは Nerd Font のグリフで、端末のフォントに入っていないのが原因
です。[Nerd Font](https://www.nerdfonts.com/) を入れるか、こうします:

```kdl
icons "none"
```

Files ビューはプレーンなファイル名だけを表示します。

### 画像プレビューを抑える

*SSH 越し / この端末だと画像プレビューが崩れる。*

デフォルト（`"auto"`）では Files ビューが端末のグラフィック
プロトコル（Kitty、iTerm2、Sixel）を検出し、なければユニコードの
ハーフブロックにフォールバックします。上書きは 2 通り:

```kdl
image-preview "halfblocks"   // 検出せず、常にハーフブロック
```

```kdl
image-preview "none"         // 画像は一切描画しない
```

## キーバインド

### キーを足す・付け替える

*ファイルマネージャと同じく、Git のファイルツリーでも `o` で開きたい。*

```kdl
page "git" {
    pane "file_tree" {
        keys {
            "o" "ExpandOrOpen"
        }
    }
}
```

キーはキー単位でマージされます: これはバインドを 1 つ足す（`o` に
既存のバインドがあれば上書きする）だけで、`file_tree` の他のキーは
デフォルトのままです。ヘルプオーバーレイ（`?`）は有効な設定から生成
されるので、自動で反映されます。

キーは文字列で書きます: 1 文字（`"j"`、`"G"`、`"/"`）、名前付きキー
（`"Enter"`、`"Esc"`、`"Tab"`、`"BackTab"`、`"Space"`、`"Backspace"`、
`"Delete"`、`"Up"`、`"Down"`、`"Left"`、`"Right"`、`"Home"`、`"End"`、
`"PageUp"`、`"PageDown"`）、または `Ctrl+` の組み合わせ
（`"Ctrl+d"`）。アクション名はペインごとに決まっています —
`vig config dump` が全ペインのデフォルトを示し、
[設定リファレンス](config-reference.md) がすべてを一覧する予定です。

グローバルなキーは `app` ブロックに書き、全ページで効きます:

```kdl
app {
    "q" "Quit"            // ペインからでなく、どこからでも終了
    "Ctrl+g" "page:git"   // Git ビューへジャンプ
}
```

`app` のアクションは `"Quit"` と `"page:<name>"` です — ページ切り替え
は*名前*で指すので、タブを並べ替えてもバインドはそのまま効きます。

### バインドを消す

*`Space` でディレクトリが開閉するのが誤爆する。*

予約アクション `"None"` に割り当てます:

```kdl
page "git" {
    pane "file_tree" {
        keys {
            "Space" "None"
        }
    }
}
```

そのペインでキーは何もしなくなり、ヘルプオーバーレイからも消えます。
preset 由来のキーにも効きます — ペインで `"n" "None"` と書けば、
そのペインの検索ネクストが消えます。

### preset とは何か

`vig config dump` を見ると、ほぼすべてのペインの `keys` に
`preset "nav"` と `preset "search"` があります。preset は標準バインドの
名前付きセットで、その場に展開されます:

| preset | 展開結果 |
|---|---|
| `nav` | `j`/`Down` → `Nav.MoveDown`、`k`/`Up` → `Nav.MoveUp`、`Ctrl+d` → `Nav.HalfPageDown`、`Ctrl+u` → `Nav.HalfPageUp`、`g` → `Nav.JumpTop`、`G` → `Nav.JumpBottom` |
| `search` | `/` → `Search.Start`、`n` → `Search.Next`、`N` → `Search.Prev` |

規則は 2 つです:

- **明示が preset に勝つ。** preset が先に展開され、同じペイン内の
  明示バインド — デフォルト設定のものでもあなたのものでも — が同じ
  キーについて勝ちます。上のレシピで `preset "search"` 由来の `n` を
  `"None"` で消せたのはこのためです。
- **preset は置換されず、常に追加される。** キーのマージ時、あなたの
  `preset` 行は既存の行に並んで追加されます。もし `search` を持たない
  ペインがあれば、`preset "search"` の 1 行で検索キー 3 つを足せます。

## タブ

### タブを絞る・並べ替える

*Git と Files と Worktrees しか使わない。*

```kdl
pages "git" "files" "worktrees"
```

ヘッダは `1:Git 2:Files 3:Worktrees` になります。`pages` はデフォルトの
リストを**丸ごと**置き換えます: リスト内の位置がタブ番号で、書かなかった
ページは完全に無効です — 起動されず、タブもバックグラウンドの
ポーリングもありません。

並べ替えも同じ書き方です:

```kdl
pages "github" "git" "files" "docker" "procs" "worktrees" "projects"
```

数字キーは*ページに名前で*結び付いたバインドです — どちらの設定でも、
組み込みの `page:git` バインドは新しい位置の Git ビューにちゃんと
届きます。無効化したページの組み込みキーは黙って外されますが、
*あなた自身の* `app` ブロックから `pages` に無いページへのバインドは
エラーです。決して動きようがないからです:

```kdl,ignore
pages "git" "files"
app {
    "d" "page:docker"    // → エラー: page "docker" is not listed in `pages`
}
```

## レイアウト

### レイアウトの木を読む

各ページの配置は、`layout { }` の中の 3 種類の要素からなる木です:

- `split direction="horizontal" { … }` は子を左右に並べ、
  `direction="vertical"` は上下に積みます。各子は `size=` を取れます。
- `place "<pane>"` はペインを表示します。
- `slot "<name>" … { … }` は*時によって別のペイン*を表示する 1 つの
  領域です — [後述](#slot-1-つの領域に複数のペイン)。

サイズは `"30"`（ちょうど 30 セル）、`"40%"`、`"min:20"`（最低 20
セル、残りを取る）。省略は `min:0` です。デフォルトの Git ビューの
レイアウトに注釈を付けると:

```kdl
page "git" {
    layout {
        split direction="vertical" {                    // 2 段
            split direction="horizontal" size="40%" {   // 上段: 高さ40%、3 列
                place "file_tree" size="30"             //   幅ちょうど 30 セル
                place "branch_list" size="35%"          //   幅の 35%
                place "reflog" size="min:20"            //   残り全部、最低 20
            }
            slot "main" size="min:3" then="git_log" default="diff_view" {
                triggers "branch_list" "reflog" "git_log"
            }                                           // 下段: log か diff
        }
    }
}
```

このブロック自体が有効な設定です — ページのデフォルトレイアウトを
そのまま書き直しても何も変わりません。そしてそれがすべてのレイアウト
編集の始め方です: `vig config dump` からそのページの `layout` を
コピーして、調整する。あなたが書いた `layout` はページの
**レイアウト全体**を置き換えます。部分マージはありません。

制約は 2 つで、どちらも起動時に検査されます: レイアウトは各ペインを
高々 1 回しか置けず、最低 1 つのペインを置かなければなりません。

### ペインを広げる

*Files のプレビューが狭い。*

dump から Files のレイアウトをコピーして数字をずらします:

```kdl
page "files" {
    layout {
        split direction="horizontal" {
            place "parent_dir" size="15%"   // デフォルト: 20%
            place "dir_list" size="25%"     // デフォルト: 30%
            place "preview" size="min:20"   // 空いた分を取る
        }
    }
}
```

プレビューの幅が約 50% から約 60% になります。

### ペインを置かない

*reflog は見ない。その場所をブランチにあげたい。*

レイアウトに書かなかったペインは**非アクティブ**になります: 領域を
持たず、`Tab` 巡回もフォーカスもスキップし、そのペインを指す `bind`
行は無視されます。`tabs` やキーを直す必要はありません — 勝手に
適応します。

```kdl
page "git" {
    layout {
        split direction="vertical" {
            split direction="horizontal" size="40%" {
                place "file_tree" size="30"
                place "branch_list" size="min:20"     // reflog の場所はあなたのもの
            }
            slot "main" size="min:3" then="git_log" default="diff_view" {
                triggers "branch_list" "git_log"
            }
        }
    }
}
```

### Projects のリストペインを復活させる

*`p` で巡回ではなく、リンクされたボードを一覧で見たい。*

Projects ページには意図的に置かれていないペインがあります:
リポジトリにリンクされたボードの一覧 `projects` です。デフォルトの
レイアウトはボードとアイテム詳細だけを表示します（ボードは `p` / `P`
で巡回）。リストを置けば生き返ります — 組み込みの
`bind select="projects" detail="board"` も、置いた瞬間から自動で
効き始めます:

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

リストでプロジェクトを選ぶと右にそのボードが読み込まれます。`Enter`
で中へ、ボードから `Esc` でリストへ戻ります。（このレイアウトは
[assets/default.kdl](https://github.com/td72/vig/blob/main/assets/default.kdl)
にコメントとして載っているものと同じです。）

### slot: 1 つの領域に複数のペイン

*`slot`、`when`、`then` とは？*

`slot` は、フォーカスの位置によって別のペインを表示するレイアウト
領域です。実例は GitHub ビューの詳細領域 — 下部の 1 領域に、
3 つの候補が入ります:

```kdl
page "github" {
    layout {
        split direction="vertical" {
            split direction="horizontal" size="40%" {
                place "issue_list" size="33%"
                place "pr_list" size="34%"
                place "run_list" size="33%"
            }
            slot "detail" size="min:3" default="issue_detail" {
                when "pr_list" "pr_detail" then="pr_detail"
                when "run_list" "run_detail" then="run_detail"
            }
        }
    }
}
```

slot の読み方: 各 `when` はトリガーになるペインを列挙し、表示する
ペインを `then=` で指名します。フォーカス中のペインを含む最初の
`when` が勝ち、どれにも当たらなければ `default=` が表示されます。
つまり: PR 列（あるいは PR 詳細の中 — `pr_detail` 自身がトリガーに
入っているのはそのためです）にフォーカス → 領域は `pr_detail`。
実行列にフォーカス → `run_detail`。それ以外はどこでも
`issue_detail`。各 `when` が自分の `then` ペインを自分のトリガーに
含めている点に注目してください — そうしないと、詳細の*中へ*
フォーカスを移した瞬間に領域が別のペインへ切り替わってしまいます。

単一ケースの省略形 — slot 自体に `then=` を付け、子に `triggers` を
書く形 — もあり、Git ビューが使っています: `branch_list`・`reflog`・
`git_log` のどれかにフォーカスがある間は `git_log`、それ以外は
`diff_view`（上の [レイアウトの木を読む](#レイアウトの木を読む) を
参照）。両方の形は 1 つの slot で併用でき、slot の名前（`"detail"`、
`"main"`）はただのラベルです。

slot を自分のものにする変種 — PR で暮らしているなら、`pr_detail` を
定位置にします:

```kdl
page "github" {
    layout {
        split direction="vertical" {
            split direction="horizontal" size="40%" {
                place "issue_list" size="33%"
                place "pr_list" size="34%"
                place "run_list" size="33%"
            }
            slot "detail" size="min:3" default="pr_detail" {
                when "issue_list" "issue_detail" then="issue_detail"
                when "run_list" "run_detail" then="run_detail"
            }
        }
    }
}
```

ペイン配置の数え方として、slot は表示しうる各ペインを 1 回ずつ
「置いた」ことになります — なので他の `place` が `pr_detail` を重ねて
置くことはできず、「高々 1 回」規則は木全体に効きます。

## Projects

### ボードを 1 つに固定する

*リポジトリに 5 つボードがリンクされているが、見るのは 1 つだけ。*

タイトルで（リンクされたプロジェクトに対して大文字小文字を無視して
照合）:

```kdl
projects-board "Roadmap"
```

またはプロジェクト番号で:

```kdl
projects-board 2
```

Projects ページはそのボードだけを表示します。`p` / `P` は巡回しなく
なり（押すとステータスバーに
`board pinned by config (projects-board)` と出ます）、ヘッダから
`(i/n)` カウンタが消えます。一致するプロジェクトが無ければ、ボード
ペインが指定名を挙げてそう伝えます。番号の形は、設定の中で唯一
クォート無しの整数を書く場所です。

## リポジトリごとの設定

### 1 つのリポジトリのための `.vig.kdl`

*このリポジトリだけタブ構成を変えて、ボードも固定したい。*

worktree のルートに `.vig.kdl` を置きます（そして gitignore して
ください — 個人用です）:

```kdl
// .vig.kdl — このリポジトリだけ
pages "git" "github" "projects"
projects-board "Roadmap"
github-poll-interval "10s"
```

ユーザー設定の上に同じ規則でマージされます（組み込み → ユーザー →
リポジトリローカル、でリポジトリローカルが勝ち）。そのリポジトリで
だけ真になることはここに書きます: 固定するボード、絞ったタブ、忙しさ
に合わせたポーリング間隔、そのプロジェクトの端末プロファイルに合う
テーマ。*あなた*に付いて回る好み — キーバインドやアイコン — は
ユーザー設定へ。

ここでのエラーが vig を止めることはありません: 壊れた `.vig.kdl` は
ステータスバーで報告され（`ignored .vig.kdl: …`）、vig は組み込み +
ユーザーで起動します。

### 信頼ダイアログ

`.vig.kdl` が git に**追跡されている**場合、それはリポジトリと一緒に
やって来たものなので、vig は読み込む前に確認します — 設定はどの
ページ・どのキーバインドが存在するかを決めるものなので、黙って読み
込みはしません。ダイアログは UI が始まる前に表示されます:

- `y` — 読み込み、この内容のファイルについて回答を記憶
- `n` — 無視して、記憶
- `v` — まずファイルを見てから決める
- `Esc` — 今回だけ無視。次回の起動でまた確認

記憶された決定は worktree *と*内容ハッシュがキーなので、ファイルが
変わると（pull の後など）もう一度確認されます。CLI から管理できます:

```bash
vig config trust                     # 記憶済みの決定を一覧
vig config trust --forget ~/src/foo  # その worktree で次回また確認させる
```

自分の**未追跡**の `.vig.kdl` でダイアログが出ることはありません —
ステータスバーに `loaded .vig.kdl` と出て、黙って読み込まれます。

### リポジトリレイヤを切る

*リポジトリに自分の vig を触らせたくない。*

**ユーザー**設定に:

```kdl
repo-config "off"
```

`.vig.kdl` は一切読み込まれず、ダイアログも出ません。有効なのは
ユーザー設定の値だけです — `.vig.kdl` は `repo-config` をそもそも
書けないので、リポジトリ側からスイッチを戻すことはできません。

## ポーリングと履歴

### GitHub のポーリングを落ち着かせる（or 速める）

*実行中のジョブを眺めている間、vig のポーリングが多すぎる。*

```kdl
github-poll-interval "10s"
```

これは GitHub ビューが**何かが動いている間**にポーリングする間隔です
— 実行中の Workflow Runs 列、ウォッチモード（`w`）の PR チェック、
実行中ジョブのログ。デフォルト `"5s"`、最小 `"2s"`（設定で API
クォータを溶かせないようにするため）。別のビューを表示している間、
ポーリングは完全に止まります。レート制限への対処はこの設定とは別に
組み込まれています: GitHub がリクエストを拒否すると vig は指数
バックオフし、リセット時刻をステータスバーに表示します。

### Procs のサンプリング間隔と履歴の深さ

*グラフを滑らかにして、履歴も長く取りたい。*

```kdl
procs-refresh-interval "1s"
procs-history "600"
```

`procs-refresh-interval` は Procs ビューが表示中にプロセスとポートを
読み直す間隔です（デフォルト `"2s"`、最小 `"250ms"`、`"1.5s"` /
`"500ms"` のような値も可。他のビューではサンプリングは止まります）。

`procs-history` は履歴グラフ — システムの CPU / メモリのチャートと
プロセスごとのスパークライン — が保持するサンプル数です。1 回の
リフレッシュにつき 1 サンプルなので、2 つの設定は掛け算になります:
上の例は 600 × 1s = 10 分の履歴です。デフォルト `"120"`（`"2s"` で
4 分）、許容範囲は `"10"` 〜 `"10000"`。
