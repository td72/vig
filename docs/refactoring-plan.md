# リファクタリング計画: ペイン共通パターンの抽象化

## 前提

`feat/pane-abstraction` ブランチで以下が完了済み:

- `Action enum + Keymap<A>` によるキーバインド抽象化
- `Tab<L, D>` ジェネリック型
- `SearchAction enum + execute_search/execute_esc` ヘルパー
- `PaneSet::find_modal()` による modal ディスパッチ汎化

本計画はこれらの上に積む残りのリファクタリングを網羅する。

---

## Phase 1: カラーパレットの一元化

### 目的

マジックカラー値（`Color::Rgb(200,120,0)` 等）が 6+ ファイルに散在している。
`diff_view/view.rs` には既に定数があるが、他ペインでは使われていない。

### 変更内容

**新規ファイル: `src/core/theme.rs`**

```rust
use ratatui::style::Color;

// Borders
pub const BORDER_FOCUSED: Color = Color::Cyan;
pub const BORDER_UNFOCUSED: Color = Color::DarkGray;

// Search highlights (list panes)
pub const SEARCH_CURRENT_FG: Color = Color::Black;
pub const SEARCH_CURRENT_BG: Color = Color::Rgb(200, 120, 0);
pub const SEARCH_MATCH_BG: Color = Color::Rgb(60, 60, 0);

// List selection
pub const LIST_SELECTION_BG: Color = Color::DarkGray;

// Empty state text
pub const EMPTY_TEXT_FG: Color = Color::DarkGray;

// Modal overlay
pub const MODAL_BG: Color = Color::Rgb(30, 30, 30);
```

**`src/core/mod.rs`** に `pub mod theme;` 追加。

**`diff_view/view.rs`** の既存定数 → `theme::` を使う or 重複分を削除。

### 変更ファイル

| ファイル | 変更 |
|---------|------|
| `src/core/theme.rs` | **新規** |
| `src/core/mod.rs` | `pub mod theme` 追加 |
| `src/git/panes/file_tree.rs` | マジック色 → `theme::*` |
| `src/git/panes/branch_list.rs` | 同上 + `ACTION_MENU_BG` → `theme::MODAL_BG` |
| `src/git/panes/reflog.rs` | 同上 |
| `src/git/panes/git_log/view.rs` | 同上 |
| `src/git/panes/diff_view/view.rs` | ローカル定数 → `theme::*` に統合 |
| `src/github/panes/issue_list.rs` | ボーダー色 → `theme::*` |
| `src/github/panes/pr_list.rs` | 同上 |

### 検証

```bash
cargo clippy --all-targets -- -D warnings
cargo test
```

UI 動作確認: 各ペインの見た目が変わっていないこと。

---

## Phase 2: 検索ハイライト構築ロジックの共通化

### 目的

`match_set` / `current_match_idx` を構築する ~20 行のコードブロックが
4 ペイン（file_tree, branch_list, reflog, git_log/view）で完全に同一。

### 変更内容

**`src/core/pane.rs` にヘルパー追加:**

```rust
use std::collections::HashSet;

/// Extract list-entry search highlights for a given pane.
/// Returns (set of matched indices, current match index).
pub fn list_search_highlights(shared: &PaneShared, pane_id: usize) -> (HashSet<usize>, Option<usize>) {
    if shared.search.origin != pane_id {
        return (HashSet::new(), None);
    }
    let set: HashSet<usize> = shared
        .search
        .matches
        .iter()
        .filter_map(|m| match m {
            SearchMatch::ListEntry(idx) => Some(*idx),
            _ => None,
        })
        .collect();
    let current = shared.search.current_match_idx.and_then(|ci| {
        match shared.search.matches.get(ci) {
            Some(SearchMatch::ListEntry(idx)) => Some(*idx),
            _ => None,
        }
    });
    (set, current)
}
```

**`src/core/theme.rs` にスタイルヘルパー追加:**

```rust
use ratatui::style::{Modifier, Style};

/// Compute highlight style for the List widget's selected row.
/// If the selected row is a search match, use BOLD only (no bg override)
/// to preserve the match background. Otherwise use selection bg.
pub fn list_highlight_style(selected_is_match: bool) -> Style {
    if selected_is_match {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .bg(LIST_SELECTION_BG)
            .add_modifier(Modifier::BOLD)
    }
}
```

**各ペインの render():**

Before (~20 行):
```rust
let (match_set, current_match_idx) = if shared.search.origin == PANE_XXX {
    // ... 20 lines ...
};
```

After (1 行):
```rust
let (match_set, current_match_idx) = pane::list_search_highlights(shared, PANE_XXX);
```

`highlight_style` 計算も同様に 1 行に:
```rust
let highlight_style = theme::list_highlight_style(match_set.contains(&self.selected_idx));
```

### 変更ファイル

| ファイル | 変更 |
|---------|------|
| `src/core/pane.rs` | `list_search_highlights()` 追加 |
| `src/core/theme.rs` | `list_highlight_style()` 追加 |
| `src/git/panes/file_tree.rs` | render() 内の重複コード → ヘルパー呼び出し |
| `src/git/panes/branch_list.rs` | 同上 |
| `src/git/panes/reflog.rs` | 同上 |
| `src/git/panes/git_log/view.rs` | 同上 |

### 検証

```bash
cargo clippy --all-targets -- -D warnings
cargo test
```

動作確認: 検索ハイライト（`/`, `n`, `N`）が全ペインで従来通り動作すること。

---

## Phase 3: 空リスト表示の共通化

### 目的

「No branches」「Working tree clean」等の空状態レンダリングが 6 ペインで類似。

### 変更内容

**`src/core/theme.rs` にヘルパー追加:**

```rust
use ratatui::widgets::{Block, List, ListItem};
use ratatui::text::{Line, Span};

/// Render an empty-state list with a single placeholder message.
pub fn render_empty_list(f: &mut Frame, area: Rect, block: Block, message: &str) {
    let items = vec![ListItem::new(Line::from(Span::styled(
        format!("  {message}"),
        Style::default().fg(EMPTY_TEXT_FG),
    )))];
    let list = List::new(items).block(block);
    f.render_widget(list, area);
}
```

**各ペインの render():**

Before (~7 行):
```rust
if self.entries.is_empty() {
    let items = vec![ListItem::new(Line::from(Span::styled(
        "  No reflog entries",
        Style::default().fg(Color::DarkGray),
    )))];
    let list = List::new(items).block(block);
    f.render_widget(list, area);
    return;
}
```

After (4 行):
```rust
if self.entries.is_empty() {
    theme::render_empty_list(f, area, block, "No reflog entries");
    return;
}
```

GitHub ペインの Loading 状態も同様:
```rust
if self.loading && self.issues.is_empty() {
    theme::render_empty_list(f, area, block, "Loading...");
    return;
}
```

### 変更ファイル

| ファイル | 変更 |
|---------|------|
| `src/core/theme.rs` | `render_empty_list()` 追加 |
| `src/git/panes/file_tree.rs` | 空状態 → ヘルパー |
| `src/git/panes/branch_list.rs` | 同上 |
| `src/git/panes/reflog.rs` | 同上 |
| `src/git/panes/git_log/view.rs` | 同上 |
| `src/github/panes/issue_list.rs` | Loading + 空状態 → ヘルパー |
| `src/github/panes/pr_list.rs` | 同上 |

### 検証

```bash
cargo clippy --all-targets -- -D warnings
cargo test
```

---

## Phase 4: ボーダーブロック生成の共通化

### 目的

全リストペインで同一の「タイトル + ボーダー + フォーカス色」パターンが繰り返される。

### 変更内容

**`src/core/theme.rs` にヘルパー追加:**

```rust
/// Create a bordered block with focus-dependent border color.
pub fn pane_block(title: &str, is_focused: bool) -> Block<'_> {
    Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if is_focused {
            BORDER_FOCUSED
        } else {
            BORDER_UNFOCUSED
        }))
}
```

**各ペインの render():**

Before:
```rust
let border_color = if shared.focused_pane == PANE_REFLOG {
    Color::Cyan
} else {
    Color::DarkGray
};
let block = Block::default()
    .title(" Reflog ")
    .borders(Borders::ALL)
    .border_style(Style::default().fg(border_color));
```

After:
```rust
let block = theme::pane_block("Reflog", shared.focused_pane == PANE_REFLOG);
```

### 変更ファイル

| ファイル | 変更 |
|---------|------|
| `src/core/theme.rs` | `pane_block()` 追加 |
| `src/git/panes/file_tree.rs` | ボーダー → ヘルパー |
| `src/git/panes/branch_list.rs` | 同上 |
| `src/git/panes/reflog.rs` | 同上 |
| `src/git/panes/git_log/view.rs` | 同上 |
| `src/git/panes/diff_view/view.rs` | 同上 |
| `src/github/panes/issue_list.rs` | 同上 |
| `src/github/panes/pr_list.rs` | 同上 |
| `src/github/panes/detail_view.rs` | 同上（要確認） |

### 検証

```bash
cargo clippy --all-targets -- -D warnings
cargo test
```

---

## Phase 5: issue_list / pr_list の統合

### 目的

`issue_list.rs`（210 行）と `pr_list.rs`（232 行）の ~95% が同一構造。
差分はアイテム描画（PR の review badge / draft ラベル）とペイン切り替え方向のみ。

### 設計

**トレイト `GhListItem` を導入**して、リスト表示に必要な共通インターフェースを定義。
ジェネリクスではなく **トレイトベース + 個別 render_item メソッド** とする
（ratatui の `ListItem` 生成はアイテム型ごとに異なるため）。

```rust
// src/github/panes/gh_list.rs (新規)

/// Shared fields and logic for GitHub list panes (Issue / PR).
pub struct GhListPane<T> {
    pub items: Vec<T>,
    pub selected_idx: usize,
    pub loading: bool,
    keymap: Keymap<GhListAction>,
}

#[derive(Debug, Clone)]
pub enum GhListAction {
    Nav(NavAction),
    OpenDetail,
    SwitchTab,
    OpenBrowser,
}
```

**各アイテム型の描画:**

```rust
pub trait GhListRender {
    fn pane_title() -> &'static str;
    fn empty_message() -> &'static str;
    fn render_item(&self, idx: usize) -> ListItem<'static>;
    fn browser_event(&self) -> PaneEvent;
}
```

**issue_list.rs / pr_list.rs:**

`GhListRender` を `GhIssueListItem` / `GhPrListItem` に実装。
各ファイルは `GhListPane<T>` の type alias + `GhListRender` impl のみになる。

### 注意事項

- `SwitchTab` の方向（Tab vs BackTab）と遷移先 pane ID はジェネリックパラメータではなく、
  `GhListPane::new()` に設定値として渡す。
- `initialize()`, `spawn_fetch()`, `apply_list()` は型固有のロジック（API 呼び出し先、
  キャッシュキー）が異なるため、trait メソッドとして残す。
  → `GhListFetch` trait or 各ペインファイルに inherent impl として残す。

### 変更ファイル

| ファイル | 変更 |
|---------|------|
| `src/github/panes/gh_list.rs` | **新規**: `GhListPane<T>`, `GhListAction`, `GhListRender` trait |
| `src/github/panes/mod.rs` | `pub mod gh_list` 追加 |
| `src/github/panes/issue_list.rs` | `GhListPane<GhIssueListItem>` ベースに書き換え |
| `src/github/panes/pr_list.rs` | `GhListPane<GhPrListItem>` ベースに書き換え |
| `src/github/state.rs` | type alias 調整、keymap 設定の変更 |

### 検証

```bash
cargo clippy --all-targets -- -D warnings
cargo test
```

動作確認:
- Issues/PRs リストの表示・ナビゲーションが正常
- Tab/BackTab でリスト間切り替え
- i/Enter で詳細表示、o でブラウザ起動

---

## Phase 6: GitHub ペインへの検索機能追加

### 目的

Git ペインには `/`, `n`, `N` による検索があるが、GitHub ペインにはない。
Phase 5 の `GhListPane<T>` に検索を組み込むことで対称性を確保。

### 変更内容

**`GhListAction` に `Search(SearchAction)` + `Esc` 追加。**

**`GhListRender` に検索対象テキスト取得メソッド追加:**

```rust
pub trait GhListRender {
    // ... 既存 ...
    fn search_text(&self) -> String; // e.g. "#123 title"
}
```

**`GhListPane<T>` に `collect_search_matches`, `jump_to_match` 実装。**

**`Pane<PaneEvent>` impl を拡張:**

```rust
fn collect_search_matches(&self, shared: &PaneShared, query: &str) -> Vec<SearchMatch> {
    // items.iter() で search_text() を検索
}
fn jump_to_match(&mut self, _shared: &PaneShared, m: &SearchMatch) {
    if let SearchMatch::ListEntry(idx) = m { self.selected_idx = *idx; }
}
```

**`github/state.rs` の `process_events` に `JumpToMatch` ハンドラ追加:**

現状の `GitHubState::process_events` に `JumpToMatch` の分岐がない場合、追加する。

### 変更ファイル

| ファイル | 変更 |
|---------|------|
| `src/github/panes/gh_list.rs` | `Search(SearchAction)` + `Esc` 追加, search impl |
| `src/github/panes/issue_list.rs` | `GhListRender::search_text()` impl |
| `src/github/panes/pr_list.rs` | 同上 |
| `src/github/state.rs` | `JumpToMatch` イベント処理追加 |

### 検証

```bash
cargo clippy --all-targets -- -D warnings
cargo test
```

動作確認:
- Issues/PRs リストで `/` 検索、`n`/`N` でジャンプ
- `Esc` でクリア
- 検索ハイライトの表示

---

## Phase 7: Pane trait ラッパーメソッドの整理

### 目的

全ペインで `impl Pane<PaneEvent>` が inherent メソッドへの単純なラッパーになっている。
名前衝突を解消し、trait メソッドを直接実装する。

### 方針

inherent メソッドを **リネーム**して、trait impl 内で直接ロジックを書く。

例（BranchListPane）:

Before:
```rust
// inherent method
pub fn handle_key(&mut self, shared: &PaneShared, key: KeyEvent) -> Vec<PaneEvent> { ... }

// trait impl (wrapper)
impl Pane<PaneEvent> for BranchListPane {
    fn handle_key(&mut self, shared: &PaneShared, key: KeyEvent) -> Vec<PaneEvent> {
        self.handle_key(shared, key) // calls inherent
    }
}
```

After:
- `handle_key` → inherent メソッドを削除し、trait impl 内に直接ロジックを書く
- `render` → 同様
- `collect_search_matches` → 同様

inherent メソッドが **外部から直接呼ばれている**場合（例: `self.panes.branch_tab.list.handle_key()`）
は inherent を残す必要がある。

### 影響調査

| メソッド | 外部から直呼びされるケース |
|---------|-------------------------|
| `handle_key` | `git/state.rs` の modal dispatch（Phase で `dispatch_to_pane` に統合済み） |
| `render` | 各 `state.rs` で area ごとに呼ばれる → `PaneSet` 経由に統合済み |
| `collect_search_matches` | `pane.rs` の `execute_search` → trait 経由 |

→ 大半は trait 経由に統合済みだが、一部直接呼び出しが残っている可能性がある。
  各ペインごとに `grep` で確認してから進める。

### 変更ファイル

| ファイル | 変更 |
|---------|------|
| `src/git/panes/file_tree.rs` | inherent → trait 直接実装 |
| `src/git/panes/branch_list.rs` | 同上 |
| `src/git/panes/reflog.rs` | 同上 |
| `src/git/panes/git_log/mod.rs` | 同上 |
| `src/git/panes/diff_view/mod.rs` | 同上 |
| `src/github/panes/issue_list.rs` | 同上（Phase 5 で統合後） |
| `src/github/panes/pr_list.rs` | 同上 |

### 検証

```bash
cargo clippy --all-targets -- -D warnings
cargo test
```

---

## 実装順序とコミット戦略

各 Phase は **1 コミット** とする。Phase 間に依存があるため順番通りに進める。

| Phase | コミット gitmoji | 依存 |
|-------|-----------------|------|
| 1: カラーパレット | ♻️ | なし |
| 2: 検索ハイライト共通化 | ♻️ | Phase 1 |
| 3: 空リスト共通化 | ♻️ | Phase 1 |
| 4: ボーダーブロック共通化 | ♻️ | Phase 1 |
| 5: issue/pr 統合 | ♻️ | Phase 1, 3, 4 |
| 6: GitHub 検索追加 | ✨ | Phase 2, 5 |
| 7: trait ラッパー整理 | ♻️ | Phase 5, 6 |

Phase 2, 3, 4 は互いに独立だが、全て Phase 1 に依存する。

---

## 変更なしとする項目

### `process_events()` の統合

`git/state.rs` と `github/state.rs` の `process_events()` は構造が類似しているが、
ページ固有イベント（`SwitchBranch`, `DeleteBranch`, `OpenIssueBrowser` 等）が異なり、
sync ロジックが `&Repo` や `&mpsc::Sender` 等の外部状態に依存する。
抽象化するとクロージャや trait object が必要で複雑化するため対象外。

### `pad_line()` ユーティリティ

`branch_list.rs` にローカル定義されている `pad_line()` は branch action menu 固有。
他の場所で同様のパディングが必要になった時点で共有モジュールへ移動する。
