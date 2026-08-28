//! Right pane of the Worktrees page: the HEAD commit of the selected
//! worktree, or the patch of the selected stash rendered with the Git
//! page's side-by-side diff view.

use crate::core::app::AppContext;
use crate::core::keymap::{
    half_page_step, nav_bindings, search_bindings, ActionHelp, Keymap, NavAction, SearchAction,
};
use crate::core::pane::{self, Pane, PaneEvent, PaneShared};
use crate::core::search::SearchMatch;
use crate::core::theme;
use crate::git::domain::diff::FileDiff;
use crate::git::panes::diff_view::keys::{execute_diff_scroll, DiffScrollAction};
use crate::git::panes::DiffViewPane;
use crate::worktrees::domain::stash::stash_patch;
use crate::worktrees::domain::types::{CommitSummary, Stash, Worktree};
use crate::worktrees::domain::worktree::head_summary;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::{layout::Rect, Frame};
use std::path::Path;
use std::rc::Rc;

#[derive(Debug, Clone)]
pub enum PreviewAction {
    Nav(NavAction),
    ScrollLeft,
    ScrollRight,
    /// Vim-style cursor mode in the stash diff (yank, visual selection).
    EnterNormalMode,
    /// Next / previous file of a multi-file stash.
    NextFile,
    PrevFile,
    Search(SearchAction),
    /// Return focus to the list the preview came from.
    Back,
    Esc,
}

crate::impl_pane_action_from_str!(
    PreviewAction, nav: Nav, search: Search, esc: Esc,
    ScrollLeft, ScrollRight, EnterNormalMode, NextFile, PrevFile, Back
);

impl ActionHelp for PreviewAction {
    fn label(&self) -> Option<&'static str> {
        match self {
            PreviewAction::Nav(NavAction::MoveDown) => Some("Scroll down"),
            PreviewAction::Nav(NavAction::MoveUp) => Some("Scroll up"),
            PreviewAction::Nav(nav) => nav.label(),
            PreviewAction::ScrollLeft => Some("Scroll left"),
            PreviewAction::ScrollRight => Some("Scroll right"),
            PreviewAction::EnterNormalMode => Some("Normal mode (cursor)"),
            PreviewAction::NextFile => Some("Next file in stash"),
            PreviewAction::PrevFile => Some("Prev file in stash"),
            PreviewAction::Search(sa) => sa.label(),
            PreviewAction::Back => Some("Back to list"),
            PreviewAction::Esc => Some("Clear search / Back"),
        }
    }
}

pub fn default_keymap() -> Keymap<PreviewAction> {
    Keymap::new()
        .bindings(nav_bindings(PreviewAction::Nav))
        .bindings(search_bindings(PreviewAction::Search))
        .key(KeyCode::Char('h'), PreviewAction::ScrollLeft)
        .key(KeyCode::Left, PreviewAction::ScrollLeft)
        .key(KeyCode::Char('l'), PreviewAction::ScrollRight)
        .key(KeyCode::Right, PreviewAction::ScrollRight)
        .key(KeyCode::Char(']'), PreviewAction::NextFile)
        .key(KeyCode::Char('['), PreviewAction::PrevFile)
        .key(KeyCode::Char('i'), PreviewAction::EnterNormalMode)
        .key(KeyCode::Backspace, PreviewAction::Back)
        .key(KeyCode::Esc, PreviewAction::Esc)
}

enum Content {
    Empty,
    Worktree {
        title: String,
        branch: String,
        summary: Result<CommitSummary, String>,
    },
    Stash {
        name: String,
        files: Rc<Vec<FileDiff>>,
        error: Option<String>,
    },
}

pub struct PreviewPane {
    pane_id: usize,
    keymap: Keymap<PreviewAction>,
    /// The Git page's diff widget, used for stash patches.
    diff: DiffViewPane,
    content: Content,
    /// Scroll offset of the text (worktree summary) view.
    scroll: usize,
    view_height: u16,
    /// Pane to return to with `Back` / `Esc`: the list the content came from.
    back_to: usize,
}

impl PreviewPane {
    pub fn new(pane_id: usize, back_to: usize, theme: &str) -> Self {
        Self {
            pane_id,
            keymap: default_keymap(),
            diff: DiffViewPane::new(Rc::new(Vec::new()), pane_id, theme),
            content: Content::Empty,
            scroll: 0,
            view_height: 20,
            back_to,
        }
    }

    pub fn set_keymap(&mut self, km: Keymap<PreviewAction>) {
        self.keymap = km;
    }

    pub fn keymap(&self) -> &Keymap<PreviewAction> {
        &self.keymap
    }

    /// True while the diff view is in Normal / Visual mode and owns every key.
    pub fn intercepts_keys(&self) -> bool {
        matches!(self.content, Content::Stash { .. }) && self.diff.intercepts_keys()
    }

    /// Whether the diff view is currently showing a patch (as opposed to the
    /// worktree summary text).
    fn showing_diff(&self) -> bool {
        matches!(&self.content, Content::Stash { files, error: None, .. } if !files.is_empty())
    }

    /// Drop the diff view's per-file line cache so a search re-scans the
    /// file that is now shown.
    pub fn invalidate_search_cache(&mut self) {
        self.diff.content_lines_cache = None;
    }

    pub fn clear(&mut self) {
        self.content = Content::Empty;
        self.scroll = 0;
        self.set_diff_files(Vec::new());
    }

    /// Show the HEAD commit of `wt`; `back_to` is the worktrees pane.
    pub fn show_worktree(&mut self, wt: &Worktree, back_to: usize) {
        self.back_to = back_to;
        self.scroll = 0;
        let summary = head_summary(&wt.path).map_err(|e| e.to_string());
        self.content = Content::Worktree {
            title: wt.display_path.clone(),
            branch: wt.ref_label(),
            summary,
        };
        self.set_diff_files(Vec::new());
    }

    /// Show the patch of `stash`; `back_to` is the stashes pane.
    pub fn show_stash(&mut self, root: &Path, stash: &Stash, back_to: usize) {
        self.back_to = back_to;
        self.scroll = 0;
        let (files, error) = match stash_patch(root, stash.index) {
            Ok(files) => (files, None),
            Err(e) => (Vec::new(), Some(e.to_string())),
        };
        self.set_stash(stash.name(), files, error);
    }

    /// Show an already-parsed stash patch (see [`show_stash`](Self::show_stash)).
    fn set_stash(&mut self, name: String, files: Vec<FileDiff>, error: Option<String>) {
        self.content = Content::Stash {
            name,
            files: Rc::new(files.clone()),
            error,
        };
        self.set_diff_files(files);
    }

    fn set_diff_files(&mut self, files: Vec<FileDiff>) {
        let first = (!files.is_empty()).then_some(0);
        let file_data = files
            .iter()
            .filter(|f| !f.is_binary)
            .map(|f| f.highlight_data())
            .collect();
        self.diff.set_files(Rc::new(files));
        self.diff.vim = Default::default();
        self.diff.reset_to_file(first);
        self.diff.spawn_highlight(file_data);
    }

    fn step_file(&mut self, delta: isize) {
        let Content::Stash { files, .. } = &self.content else {
            return;
        };
        let n = files.len();
        if n == 0 {
            return;
        }
        let cur = self.diff.current_file_idx.unwrap_or(0) as isize;
        let next = (cur + delta).rem_euclid(n as isize) as usize;
        self.diff.set_file(Some(next));
        self.diff.reset_scroll();
        self.invalidate_search_cache();
    }

    /// Lines of the worktree summary (or an error / placeholder).
    fn text_lines(&self) -> Vec<Line<'static>> {
        let dim = Style::default().fg(Color::DarkGray);
        match &self.content {
            Content::Empty => vec![Line::from(Span::styled(
                "  Select a worktree or stash",
                dim,
            ))],
            Content::Worktree {
                summary: Err(e), ..
            } => vec![Line::from(Span::styled(
                format!("  {e}"),
                Style::default().fg(Color::Red),
            ))],
            Content::Worktree {
                summary: Ok(s),
                branch,
                ..
            } => summary_lines(s, branch),
            Content::Stash { error: Some(e), .. } => vec![Line::from(Span::styled(
                format!("  {e}"),
                Style::default().fg(Color::Red),
            ))],
            Content::Stash { .. } => vec![Line::from(Span::styled("  (empty stash)", dim))],
        }
    }

    fn execute(&mut self, shared: &PaneShared, action: PreviewAction) -> Vec<PaneEvent> {
        if let Some(events) = pane::try_dispatch_search_esc(
            &action,
            shared,
            self.pane_id,
            vec![PaneEvent::SetFocus(self.back_to)],
        ) {
            return events;
        }
        match action {
            PreviewAction::Back => return vec![PaneEvent::SetFocus(self.back_to)],
            PreviewAction::NextFile => self.step_file(1),
            PreviewAction::PrevFile => self.step_file(-1),
            other if self.showing_diff() => {
                let diff_action = match other {
                    PreviewAction::Nav(nav) => DiffScrollAction::Nav(nav),
                    PreviewAction::ScrollLeft => DiffScrollAction::ScrollLeft,
                    PreviewAction::ScrollRight => DiffScrollAction::ScrollRight,
                    PreviewAction::EnterNormalMode => DiffScrollAction::EnterNormalMode,
                    _ => return vec![],
                };
                return execute_diff_scroll(&mut self.diff, shared, diff_action);
            }
            PreviewAction::Nav(nav) => {
                let total = self.text_lines().len();
                let max = total.saturating_sub(self.view_height as usize);
                let half = half_page_step(self.view_height) as usize;
                self.scroll = match nav {
                    NavAction::MoveDown => self.scroll + 1,
                    NavAction::MoveUp => self.scroll.saturating_sub(1),
                    NavAction::HalfPageDown => self.scroll + half,
                    NavAction::HalfPageUp => self.scroll.saturating_sub(half),
                    NavAction::JumpTop => 0,
                    NavAction::JumpBottom => max,
                }
                .min(max);
            }
            _ => {}
        }
        vec![]
    }
}

/// `commit abc1234 (main)` / Author / Date / subject / `--stat` lines.
fn summary_lines(s: &CommitSummary, branch: &str) -> Vec<Line<'static>> {
    let dim = Style::default().fg(Color::DarkGray);
    let mut lines = vec![
        Line::from(vec![
            Span::styled("  commit ", Style::default().fg(Color::Yellow)),
            Span::styled(
                s.short_hash().to_string(),
                Style::default().fg(Color::Yellow),
            ),
            Span::raw(" "),
            Span::styled(format!("({branch})"), Style::default().fg(Color::Blue)),
        ]),
        Line::from(vec![
            Span::styled("  Author: ", dim),
            Span::raw(format!("{} <{}>", s.author_name, s.author_email)),
        ]),
        Line::from(vec![
            Span::styled("  Date:   ", dim),
            Span::raw(s.date.clone()),
            Span::styled(format!(" ({})", s.relative_date), dim),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            format!("      {}", s.subject),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];
    if s.stat.is_empty() {
        lines.push(Line::from(Span::styled("  (no changed files)", dim)));
        return lines;
    }
    for (i, stat) in s.stat.iter().enumerate() {
        if i + 1 == s.stat.len() {
            // Totals line: "3 files changed, 164 insertions(+), 160 deletions(-)"
            lines.push(Line::from(Span::styled(format!(" {stat}"), dim)));
            break;
        }
        lines.push(stat_line(stat));
    }
    lines
}

/// Colour the `+`/`-` bar of a `--stat` line: ` path | 12 +++--`.
fn stat_line(stat: &str) -> Line<'static> {
    let Some((path, rest)) = stat.split_once(" | ") else {
        return Line::from(Span::raw(format!(" {stat}")));
    };
    let mut spans = vec![
        Span::raw(format!(" {path}")),
        Span::styled(" | ", Style::default().fg(Color::DarkGray)),
    ];
    let count_end = rest.find(['+', '-']).unwrap_or(rest.len());
    spans.push(Span::raw(rest[..count_end].to_string()));
    let bar = &rest[count_end..];
    let plus: String = bar.chars().filter(|c| *c == '+').collect();
    let minus: String = bar.chars().filter(|c| *c == '-').collect();
    if !plus.is_empty() {
        spans.push(Span::styled(plus, Style::default().fg(Color::Green)));
    }
    if !minus.is_empty() {
        spans.push(Span::styled(minus, Style::default().fg(Color::Red)));
    }
    Line::from(spans)
}

impl Pane<PaneEvent> for PreviewPane {
    /// Scroll mode goes through this pane's keymap (which maps the scroll /
    /// search / file-stepping actions onto the diff widget). Once the stash
    /// diff has entered Normal / Visual mode, the widget owns every key —
    /// `h j k l w b`, `v` / `V`, `y` motions, counts, `Esc` — so the event is
    /// handed to it untouched, exactly as the Git page does for its diff view.
    fn handle_key(&mut self, shared: &PaneShared, key: KeyEvent) -> Vec<PaneEvent> {
        if self.intercepts_keys() {
            return self.diff.handle_key(shared, key);
        }
        match self.keymap.lookup(key).cloned() {
            Some(action) => self.execute(shared, action),
            // Unmapped keys are not forwarded to the widget's built-in scroll
            // keymap: the KDL `preview` block is the single source of truth
            // for scroll-mode bindings (including keys the user unbinds).
            None => vec![],
        }
    }

    fn render(&mut self, f: &mut Frame, ctx: &AppContext, shared: &PaneShared, area: Rect) {
        self.view_height = area.height.saturating_sub(2);
        if let Content::Stash { name, files, .. } = &self.content {
            if self.showing_diff() {
                let idx = self.diff.current_file_idx.unwrap_or(0);
                let path = files.get(idx).map(|f| f.path.as_str()).unwrap_or("");
                self.diff.title = format!("{name}: {path} [{}/{}]", idx + 1, files.len());
                self.diff.render(f, ctx, shared, area);
                return;
            }
        }
        let title = match &self.content {
            Content::Worktree { title, .. } => title.clone(),
            Content::Stash { name, .. } => name.clone(),
            Content::Empty => "Preview".to_string(),
        };
        let lines = self.text_lines();
        self.scroll = self
            .scroll
            .min(lines.len().saturating_sub(self.view_height as usize));
        let block = theme::pane_block(&title, shared.focused_pane == self.pane_id);
        let visible: Vec<Line> = lines
            .into_iter()
            .skip(self.scroll)
            .take(self.view_height as usize)
            .collect();
        f.render_widget(Paragraph::new(visible).block(block), area);
    }

    fn collect_search_matches(&self, shared: &PaneShared, query: &str) -> Vec<SearchMatch> {
        if self.showing_diff() {
            self.diff.collect_search_matches(shared, query)
        } else {
            vec![]
        }
    }

    fn jump_to_match(&mut self, shared: &PaneShared, search_match: &SearchMatch) {
        if self.showing_diff() {
            self.diff.jump_to_match(shared, search_match);
        }
    }

    fn drain_background(&mut self) {
        self.diff.drain_background();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::search::SearchState;
    use crate::git::panes::diff_view::DiffViewMode;
    use crate::worktrees::domain::stash::patch_to_files;
    use crossterm::event::KeyModifiers;

    const PREVIEW: usize = 2;
    const STASHES: usize = 1;

    fn shared() -> PaneShared {
        PaneShared {
            focused_pane: PREVIEW,
            previous_pane: STASHES,
            search: SearchState::new(),
        }
    }

    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn esc() -> KeyEvent {
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)
    }

    /// A preview showing a one-file stash patch, as `show_stash` would.
    fn stash_preview() -> PreviewPane {
        let patch = concat!(
            "diff --git a/src/lib.rs b/src/lib.rs\n",
            "index 1111111..2222222 100644\n",
            "--- a/src/lib.rs\n",
            "+++ b/src/lib.rs\n",
            "@@ -1,3 +1,3 @@\n",
            " fn a() {}\n",
            "-fn b() {}\n",
            "+fn b() { todo!() }\n",
            " fn c() {}\n",
        );
        let files = patch_to_files(patch.as_bytes()).unwrap();
        let mut p = PreviewPane::new(PREVIEW, STASHES, "base16-eighties.dark");
        p.set_stash("stash@{0}".to_string(), files, None);
        p
    }

    #[test]
    fn normal_and_visual_mode_keys_reach_the_diff_widget() {
        let mut p = stash_preview();
        let sh = shared();
        assert!(!p.intercepts_keys());

        // `i` (this pane's keymap) enters Normal mode; from then on the
        // widget owns the keys.
        assert!(p.handle_key(&sh, key('i')).is_empty());
        assert_eq!(p.diff.vim.mode, DiffViewMode::Normal);
        assert!(p.intercepts_keys());

        // `j` moves the cursor (row 1 = "fn a() {}") instead of scrolling.
        p.handle_key(&sh, key('j'));
        assert_eq!(p.diff.vim.cursor.row, 1);
        assert_eq!(p.diff.scroll.y, 0);

        // `V` then `y` yanks the selected line and drops back to Normal mode.
        p.handle_key(&sh, key('V'));
        assert_eq!(p.diff.vim.mode, DiffViewMode::VisualLine);
        let events = p.handle_key(&sh, key('y'));
        assert!(
            matches!(&events[..], [PaneEvent::CopyToClipboard(t)] if t == "fn a() {}"),
            "expected a yank of the selected line"
        );
        assert_eq!(p.diff.vim.mode, DiffViewMode::Normal);

        // `v` + `y` (character-wise) works too.
        p.handle_key(&sh, key('v'));
        assert_eq!(p.diff.vim.mode, DiffViewMode::Visual);
        p.handle_key(&sh, key('l'));
        let events = p.handle_key(&sh, key('y'));
        assert!(matches!(&events[..], [PaneEvent::CopyToClipboard(t)] if t == "fn"));

        // Esc leaves Normal mode without leaving the pane; a second Esc
        // (Scroll mode, this pane's keymap) goes back to the stash list.
        assert!(p.handle_key(&sh, esc()).is_empty());
        assert_eq!(p.diff.vim.mode, DiffViewMode::Scroll);
        assert!(!p.intercepts_keys());
        let events = p.handle_key(&sh, esc());
        assert!(matches!(&events[..], [PaneEvent::SetFocus(STASHES)]));
    }

    #[test]
    fn scroll_mode_uses_the_pane_keymap_only() {
        let mut p = stash_preview();
        let sh = shared();
        // `v` is not bound in Scroll mode: nothing happens, no mode change.
        assert!(p.handle_key(&sh, key('v')).is_empty());
        assert_eq!(p.diff.vim.mode, DiffViewMode::Scroll);
        // Backspace (Back) returns to the list.
        let events = p.handle_key(&sh, KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert!(matches!(&events[..], [PaneEvent::SetFocus(STASHES)]));
    }

    fn text(line: &Line) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn summary_lines_layout() {
        let s = CommitSummary {
            hash: "06ee70fc4b03fe37".to_string(),
            author_name: "Kohei".to_string(),
            author_email: "k@example.com".to_string(),
            date: "2026-08-28 16:20".to_string(),
            relative_date: "6 minutes ago".to_string(),
            subject: "Merge pull request #108".to_string(),
            stat: vec![
                " src/core/mod.rs | 1 +".to_string(),
                " src/x.rs        | 4 ++--".to_string(),
                " 2 files changed, 3 insertions(+), 2 deletions(-)".to_string(),
            ],
        };
        let lines = summary_lines(&s, "main");
        assert_eq!(text(&lines[0]), "  commit 06ee70f (main)");
        assert_eq!(text(&lines[1]), "  Author: Kohei <k@example.com>");
        assert_eq!(
            text(&lines[2]),
            "  Date:   2026-08-28 16:20 (6 minutes ago)"
        );
        assert_eq!(text(&lines[4]), "      Merge pull request #108");
        assert_eq!(text(&lines[6]), "  src/core/mod.rs | 1 +");
        assert_eq!(text(&lines[7]), "  src/x.rs        | 4 ++--");
        assert_eq!(
            text(&lines[8]),
            "  2 files changed, 3 insertions(+), 2 deletions(-)"
        );
        // The bar is split into a green and a red span.
        let bar: Vec<&str> = lines[7].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(bar.contains(&"++"));
        assert!(bar.contains(&"--"));
    }
}
