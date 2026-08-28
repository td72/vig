//! A generic "tail -f" style component: a capped scrollback buffer with a
//! follow mode, plus a renderer. Pages wrap [`TailState`] in their own pane
//! (Docker logs, GitHub Actions job logs, …) and forward navigation /
//! search to it.
//!
//! Follow semantics: while following, the view is pinned to the end and
//! every appended line scrolls it. Any manual scroll pauses following;
//! `JumpBottom` (`G`) resumes it.

use crate::core::keymap::{half_page_step, NavAction};
use crate::core::search::SearchMatch;
use crate::core::theme;
use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Paragraph},
    Frame,
};
use std::collections::{HashSet, VecDeque};

/// Scrollback buffer + viewport state. Pure logic, no rendering.
#[derive(Debug, Clone)]
pub struct TailState {
    lines: VecDeque<String>,
    cap: usize,
    /// First visible line while not following.
    top: usize,
    follow: bool,
    view_height: usize,
}

// The buffer API is meant for reuse by other pages (Actions job logs), so
// not every accessor has a caller in the Docker page yet.
#[allow(dead_code)]
impl TailState {
    /// An empty buffer that keeps at most `cap` lines and follows the end.
    pub fn new(cap: usize) -> Self {
        Self {
            lines: VecDeque::new(),
            cap: cap.max(1),
            top: 0,
            follow: true,
            view_height: 1,
        }
    }

    pub fn cap(&self) -> usize {
        self.cap
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub fn line(&self, idx: usize) -> Option<&str> {
        self.lines.get(idx).map(String::as_str)
    }

    pub fn lines(&self) -> impl Iterator<Item = &str> {
        self.lines.iter().map(String::as_str)
    }

    /// The last `n` lines, oldest first.
    pub fn last_lines(&self, n: usize) -> impl Iterator<Item = &str> {
        let skip = self.lines.len().saturating_sub(n);
        self.lines.iter().skip(skip).map(String::as_str)
    }

    /// Drop everything and go back to following.
    pub fn clear(&mut self) {
        self.lines.clear();
        self.top = 0;
        self.follow = true;
    }

    /// Replace the whole buffer (keeps the follow / scroll state where possible).
    pub fn set_lines(&mut self, lines: impl IntoIterator<Item = String>) {
        self.lines.clear();
        self.lines.extend(lines);
        self.evict();
        self.clamp_top();
    }

    /// Append one line, evicting the oldest past the cap. A paused view keeps
    /// showing the same content when lines are evicted underneath it.
    pub fn push(&mut self, line: String) {
        self.lines.push_back(line);
        self.evict();
    }

    pub fn extend(&mut self, lines: impl IntoIterator<Item = String>) {
        for line in lines {
            self.push(line);
        }
    }

    fn evict(&mut self) {
        let over = self.lines.len().saturating_sub(self.cap);
        if over > 0 {
            self.lines.drain(..over);
            if !self.follow {
                self.top = self.top.saturating_sub(over);
            }
        }
    }

    pub fn is_following(&self) -> bool {
        self.follow
    }

    /// Resume following: the view snaps to the end and stays there.
    pub fn follow(&mut self) {
        self.follow = true;
        self.top = self.max_top();
    }

    /// Short mode label for titles / status bars.
    pub fn mode_label(&self) -> &'static str {
        if self.follow {
            "follow"
        } else {
            "paused"
        }
    }

    /// Number of visible rows; the renderer calls this every frame.
    pub fn set_view_height(&mut self, height: usize) {
        self.view_height = height.max(1);
        self.clamp_top();
    }

    pub fn view_height(&self) -> usize {
        self.view_height
    }

    /// Largest `top` that still fills the viewport.
    pub fn max_top(&self) -> usize {
        self.lines.len().saturating_sub(self.view_height)
    }

    /// First visible line (pinned to the end while following).
    pub fn top(&self) -> usize {
        if self.follow {
            self.max_top()
        } else {
            self.top.min(self.max_top())
        }
    }

    fn clamp_top(&mut self) {
        self.top = self.top.min(self.max_top());
    }

    /// Scroll by `delta` rows. Any manual scroll pauses following.
    pub fn scroll_by(&mut self, delta: isize) {
        let cur = self.top();
        self.follow = false;
        self.top = if delta < 0 {
            cur.saturating_sub(delta.unsigned_abs())
        } else {
            cur.saturating_add(delta as usize).min(self.max_top())
        };
    }

    pub fn scroll_to_top(&mut self) {
        self.follow = false;
        self.top = 0;
    }

    /// Bring `line` into view (pauses following). The line lands at the top
    /// of the viewport unless that would leave the end of the buffer empty.
    pub fn scroll_to(&mut self, line: usize) {
        self.follow = false;
        self.top = line.min(self.max_top());
    }

    /// Map the standard navigation actions onto the buffer: `j`/`k` scroll
    /// one line, `Ctrl+d`/`Ctrl+u` half a page, `g` jumps to the top and
    /// `G` resumes following.
    pub fn apply_nav(&mut self, nav: NavAction) {
        let half = half_page_step(self.view_height as u16) as isize;
        match nav {
            NavAction::MoveDown => self.scroll_by(1),
            NavAction::MoveUp => self.scroll_by(-1),
            NavAction::HalfPageDown => self.scroll_by(half),
            NavAction::HalfPageUp => self.scroll_by(-half),
            NavAction::JumpTop => self.scroll_to_top(),
            NavAction::JumpBottom => self.follow(),
        }
    }

    /// Case-insensitive substring search over the buffer, as list entries
    /// (line indices) so the page's `SearchState` can step through them.
    pub fn search_matches(&self, query: &str) -> Vec<SearchMatch> {
        let q = query.to_lowercase();
        if q.is_empty() {
            return vec![];
        }
        self.lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.to_lowercase().contains(&q))
            .map(|(i, _)| SearchMatch::ListEntry(i))
            .collect()
    }

    /// Jump the viewport to a search match.
    pub fn jump_to_match(&mut self, m: &SearchMatch) {
        if let SearchMatch::ListEntry(idx) = m {
            self.scroll_to(*idx);
        }
    }
}

/// How a raw buffer line is turned into a styled row.
pub type LineFormatter = fn(&str) -> Line<'static>;

fn plain_line(raw: &str) -> Line<'static> {
    Line::from(Span::raw(raw.to_string()))
}

/// Renderer for a [`TailState`]. Build one per frame.
pub struct TailPane<'a> {
    block: Block<'a>,
    empty: Option<&'a str>,
    match_set: Option<&'a HashSet<usize>>,
    current_match: Option<usize>,
    format: LineFormatter,
}

impl<'a> TailPane<'a> {
    pub fn new(block: Block<'a>) -> Self {
        Self {
            block,
            empty: None,
            match_set: None,
            current_match: None,
            format: plain_line,
        }
    }

    /// Placeholder shown (dimmed) when the buffer is empty.
    pub fn empty_message(mut self, msg: &'a str) -> Self {
        self.empty = Some(msg);
        self
    }

    /// Search highlights: the set of matching line indices and the current one.
    pub fn highlights(mut self, match_set: &'a HashSet<usize>, current: Option<usize>) -> Self {
        self.match_set = Some(match_set);
        self.current_match = current;
        self
    }

    /// Custom per-line styling (e.g. dim timestamps).
    pub fn formatter(mut self, format: LineFormatter) -> Self {
        self.format = format;
        self
    }

    pub fn render(self, f: &mut Frame, area: Rect, state: &mut TailState) {
        let inner = self.block.inner(area);
        state.set_view_height(inner.height as usize);
        let lines: Vec<Line<'static>> = if state.is_empty() {
            match self.empty {
                Some(msg) => vec![Line::from(Span::styled(
                    format!("  {msg}"),
                    Style::default().fg(theme::EMPTY_TEXT_FG),
                ))],
                None => vec![],
            }
        } else {
            let top = state.top();
            let empty_set = HashSet::new();
            let match_set = self.match_set.unwrap_or(&empty_set);
            state
                .lines()
                .enumerate()
                .skip(top)
                .take(state.view_height())
                .map(|(idx, raw)| {
                    let mut line = (self.format)(raw);
                    let hl = theme::search_highlight_for(match_set, self.current_match, idx);
                    if hl.is_active() {
                        line = line.style(hl.apply(Style::default()));
                    }
                    line
                })
                .collect()
        };
        f.render_widget(Paragraph::new(lines).block(self.block), area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filled(n: usize, cap: usize, height: usize) -> TailState {
        let mut s = TailState::new(cap);
        s.set_view_height(height);
        s.extend((0..n).map(|i| format!("line {i}")));
        s
    }

    #[test]
    fn follows_the_end_as_lines_arrive() {
        let mut s = filled(10, 100, 4);
        assert!(s.is_following());
        assert_eq!(s.top(), 6);
        s.push("line 10".into());
        assert_eq!(s.top(), 7);
        assert_eq!(s.line(s.top()), Some("line 7"));
    }

    #[test]
    fn manual_scroll_pauses_and_g_resumes() {
        let mut s = filled(10, 100, 4);
        s.apply_nav(NavAction::MoveUp);
        assert!(!s.is_following());
        assert_eq!(s.top(), 5);
        // New lines no longer move the view.
        s.push("line 10".into());
        assert_eq!(s.top(), 5);
        s.apply_nav(NavAction::MoveDown);
        assert_eq!(s.top(), 6);
        s.apply_nav(NavAction::JumpBottom);
        assert!(s.is_following());
        assert_eq!(s.top(), 7);
        assert_eq!(s.mode_label(), "follow");
    }

    #[test]
    fn half_page_and_top_navigation_clamp() {
        let mut s = filled(20, 100, 6);
        s.apply_nav(NavAction::HalfPageUp);
        assert_eq!(s.top(), 14 - 3);
        s.apply_nav(NavAction::JumpTop);
        assert_eq!(s.top(), 0);
        s.apply_nav(NavAction::HalfPageUp);
        assert_eq!(s.top(), 0);
        s.apply_nav(NavAction::HalfPageDown);
        assert_eq!(s.top(), 3);
        s.scroll_by(1000);
        assert_eq!(s.top(), 14);
        assert!(!s.is_following());
    }

    #[test]
    fn cap_evicts_oldest_and_keeps_paused_view_stable() {
        let mut s = filled(5, 5, 2);
        s.scroll_to(1);
        assert_eq!(s.line(s.top()), Some("line 1"));
        s.push("line 5".into());
        assert_eq!(s.len(), 5);
        assert_eq!(s.line(0), Some("line 1"));
        // The same content stays at the top of the view after eviction.
        assert_eq!(s.line(s.top()), Some("line 1"));
        // Paused at the very top: eviction cannot go negative.
        s.scroll_to_top();
        s.extend(["a".to_string(), "b".to_string()]);
        assert_eq!(s.top(), 0);
        assert_eq!(s.len(), 5);
    }

    #[test]
    fn short_buffers_never_scroll() {
        let mut s = filled(2, 10, 5);
        assert_eq!(s.top(), 0);
        s.apply_nav(NavAction::MoveDown);
        assert_eq!(s.top(), 0);
        s.set_view_height(1);
        assert_eq!(s.max_top(), 1);
    }

    #[test]
    fn clear_and_set_lines_reset_state() {
        let mut s = filled(10, 100, 3);
        s.scroll_to_top();
        s.clear();
        assert!(s.is_empty());
        assert!(s.is_following());
        s.set_lines((0..200).map(|i| i.to_string()));
        assert_eq!(s.len(), 100);
        assert_eq!(s.line(0), Some("100"));
        assert_eq!(s.top(), 97);
    }

    #[test]
    fn search_matches_are_case_insensitive_line_entries() {
        let mut s = TailState::new(10);
        s.set_view_height(2);
        s.extend(["Error: boom".into(), "ok".into(), "another error".into()]);
        let m = s.search_matches("ERROR");
        assert!(matches!(
            m.as_slice(),
            [SearchMatch::ListEntry(0), SearchMatch::ListEntry(2)]
        ));
        assert!(s.search_matches("").is_empty());
        s.jump_to_match(&m[1]);
        assert!(!s.is_following());
        assert_eq!(s.top(), 1); // clamped to max_top
    }

    #[test]
    fn last_lines_returns_tail_in_order() {
        let s = filled(5, 10, 2);
        let tail: Vec<&str> = s.last_lines(2).collect();
        assert_eq!(tail, ["line 3", "line 4"]);
        assert_eq!(s.last_lines(99).count(), 5);
    }
}
