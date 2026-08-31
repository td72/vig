# Worktrees View

![worktrees demo](../../../../assets/demo-worktrees.gif)

リポジトリの worktree と stash を読み取り専用で一覧するビューです。左上の
ペインは worktree の一覧（`git worktree list`）で、パス（可能なら main
worktree からの相対パス）、チェックアウト中のブランチ（detached HEAD の
場合はそのハッシュ）、`[main]` `[locked]` `[prunable]` `[bare]` などの
フラグを表示します。vig を起動した worktree には `*` が付きます。左下の
ペインは stash の一覧（`stash@{n}`、メッセージ、作成元ブランチ、経過時間）
です。

## プレビューペイン

右のプレビューは選択に追従します:

- **worktree** を選ぶと HEAD コミット（ハッシュ、作者、日時、サブジェクト）
  と変更ファイルを表示します。
- **stash** を選ぶとその差分（untracked ファイルを含む）を Git ビューと
  同じサイドバイサイド diff ビューで表示します。シンタックスハイライト、
  検索、Normal / Visual モードとヤンクもそのまま使えます。複数ファイルの
  stash では `[` / `]` でファイルを移動できます。

## キーバインド

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

## 制約

- このビューから apply・drop・追加・削除・lock・prune を行うことは一切
  ありません。一覧とプレビューだけです。worktree や stash の管理は
  シェルで行ってください。
