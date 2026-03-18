use super::{CursorPos, DiffSide, DiffViewMode};
use crate::core::keymap::{search_bindings, ActionHelp, Keymap, NavAction, SearchAction};
use crate::core::pane::{self, PaneShared};
use crate::git::state::{PaneEvent, PANE_DIFF_VIEW};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone)]
pub(crate) enum DiffScrollAction {
    Nav(NavAction),
    ScrollLeft,
    ScrollRight,
    EnterNormalMode,
    Search(SearchAction),
    Esc,
}

crate::impl_pane_action_from_str!(
    DiffScrollAction, nav: Nav, search: Search,
    ScrollLeft, ScrollRight, EnterNormalMode, Esc
);

impl ActionHelp for DiffScrollAction {
    fn label(&self) -> Option<&'static str> {
        match self {
            DiffScrollAction::Nav(nav) => nav.label(),
            DiffScrollAction::ScrollLeft => Some("Scroll left"),
            DiffScrollAction::ScrollRight => Some("Scroll right"),
            DiffScrollAction::EnterNormalMode => Some("Normal mode (cursor)"),
            DiffScrollAction::Search(sa) => sa.label(),
            DiffScrollAction::Esc => Some("Clear search / Back"),
        }
    }
}

pub(crate) fn default_scroll_keymap() -> Keymap<DiffScrollAction> {
    Keymap::new()
        .key(
            KeyCode::Char('j'),
            DiffScrollAction::Nav(NavAction::MoveDown),
        )
        .key(KeyCode::Down, DiffScrollAction::Nav(NavAction::MoveDown))
        .key(KeyCode::Char('k'), DiffScrollAction::Nav(NavAction::MoveUp))
        .key(KeyCode::Up, DiffScrollAction::Nav(NavAction::MoveUp))
        .ctrl('d', DiffScrollAction::Nav(NavAction::HalfPageDown))
        .ctrl('u', DiffScrollAction::Nav(NavAction::HalfPageUp))
        .key(
            KeyCode::Char('g'),
            DiffScrollAction::Nav(NavAction::JumpTop),
        )
        .key(
            KeyCode::Char('G'),
            DiffScrollAction::Nav(NavAction::JumpBottom),
        )
        .key(KeyCode::Char('h'), DiffScrollAction::ScrollLeft)
        .key(KeyCode::Left, DiffScrollAction::ScrollLeft)
        .key(KeyCode::Char('l'), DiffScrollAction::ScrollRight)
        .key(KeyCode::Right, DiffScrollAction::ScrollRight)
        .key(KeyCode::Char('i'), DiffScrollAction::EnterNormalMode)
        .key(KeyCode::Esc, DiffScrollAction::Esc)
        .bindings(search_bindings(DiffScrollAction::Search))
}

pub(crate) fn handle_diff_scroll_key(
    pane: &mut super::DiffViewPane,
    shared: &PaneShared,
    key: KeyEvent,
) -> Vec<PaneEvent> {
    let action = match pane.scroll_keymap.lookup(key) {
        Some(a) => a.clone(),
        None => return vec![],
    };
    execute_diff_scroll(pane, shared, action)
}

fn execute_diff_scroll(
    pane: &mut super::DiffViewPane,
    shared: &PaneShared,
    action: DiffScrollAction,
) -> Vec<PaneEvent> {
    let max_scroll = pane
        .scroll
        .total_lines
        .saturating_sub(pane.scroll.view_height);
    match action {
        DiffScrollAction::Nav(nav) => match nav {
            NavAction::MoveDown => {
                pane.scroll.y = (pane.scroll.y + 1).min(max_scroll);
            }
            NavAction::MoveUp => {
                pane.scroll.y = pane.scroll.y.saturating_sub(1);
            }
            NavAction::HalfPageDown => {
                let half = pane.scroll.view_height / 2;
                pane.scroll.y = (pane.scroll.y + half).min(max_scroll);
            }
            NavAction::HalfPageUp => {
                let half = pane.scroll.view_height / 2;
                pane.scroll.y = pane.scroll.y.saturating_sub(half);
            }
            NavAction::JumpTop => {
                pane.scroll.y = 0;
            }
            NavAction::JumpBottom => {
                pane.scroll.y = max_scroll;
            }
        },
        DiffScrollAction::ScrollLeft => {
            pane.scroll.x = pane.scroll.x.saturating_sub(4);
        }
        DiffScrollAction::ScrollRight => {
            pane.scroll.x = pane.scroll.x.saturating_add(4);
        }
        DiffScrollAction::Esc => {
            return pane::execute_esc(shared, vec![PaneEvent::SetFocus(shared.previous_pane)]);
        }
        DiffScrollAction::Search(sa) => {
            if sa == SearchAction::Start {
                pane.vim.pending_key = None;
            }
            return pane::execute_search(sa, PANE_DIFF_VIEW);
        }
        DiffScrollAction::EnterNormalMode => {
            let lines = content_lines(pane, shared);
            if !lines.is_empty() {
                pane.vim.mode = DiffViewMode::Normal;
                pane.vim.cursor = CursorPos {
                    row: pane.scroll.y as usize,
                    col: 0,
                    side: DiffSide::Left,
                };
            }
        }
    }
    vec![]
}

pub(crate) fn handle_diff_normal_key(
    pane: &mut super::DiffViewPane,
    shared: &PaneShared,
    key: KeyEvent,
) -> Vec<PaneEvent> {
    // Handle Ctrl+w prefix for panel switching
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('w') {
        pane.vim.pending_key = Some('w');
        return vec![];
    }

    // Handle pending key sequences
    if let Some(pending) = pane.vim.pending_key {
        pane.vim.pending_key = None;
        match pending {
            'w' => {
                match key.code {
                    KeyCode::Char('h') => pane.vim.cursor.side = DiffSide::Left,
                    KeyCode::Char('l') => pane.vim.cursor.side = DiffSide::Right,
                    _ => {}
                }
                pane.vim.count = None;
                return vec![];
            }
            'y' => {
                let lines = content_lines(pane, shared);
                let n = take_count(pane);
                if let Some(text) = execute_yank_motion(pane, key.code, &lines, n) {
                    return vec![PaneEvent::CopyToClipboard(text)];
                }
                return vec![];
            }
            'g' => {
                let lines = content_lines(pane, shared);
                if let KeyCode::Char('g') = key.code {
                    // gg or {count}gg — go to line
                    if let Some(n) = pane.vim.count.take() {
                        pane.vim.cursor.row =
                            (n.saturating_sub(1)).min(lines.len().saturating_sub(1));
                    } else {
                        pane.vim.cursor.row = 0;
                    }
                    pane.vim.cursor.col = 0;
                    clamp_col(pane, &lines);
                }
                pane.vim.count = None;
                pane.scroll_to_cursor();
                return vec![];
            }
            _ => {}
        }
        pane.vim.count = None;
        return vec![];
    }

    // Accumulate digit count prefix (1-9 start, 0 appends)
    if let KeyCode::Char(c @ '1'..='9') = key.code {
        let digit = (c as usize) - ('0' as usize);
        pane.vim.count = Some(pane.vim.count.unwrap_or(0) * 10 + digit);
        return vec![];
    }
    if let KeyCode::Char('0') = key.code {
        if pane.vim.count.is_some() {
            pane.vim.count = Some(pane.vim.count.unwrap() * 10);
            return vec![];
        }
        // else fall through to handle '0' as go-to-line-start
    }

    let n = take_count(pane);
    let lines = content_lines(pane, shared);
    let total = lines.len();
    if total == 0 {
        return vec![];
    }

    let mut events = Vec::new();

    match key.code {
        KeyCode::Char('h') | KeyCode::Left => {
            pane.vim.cursor.col = pane.vim.cursor.col.saturating_sub(n);
        }
        KeyCode::Char('l') | KeyCode::Right => {
            let line_len = current_line_len(pane, &lines);
            pane.vim.cursor.col = (pane.vim.cursor.col + n).min(line_len.saturating_sub(1));
        }
        KeyCode::Char('j') | KeyCode::Down => {
            pane.vim.cursor.row = (pane.vim.cursor.row + n).min(total - 1);
            clamp_col(pane, &lines);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            pane.vim.cursor.row = pane.vim.cursor.row.saturating_sub(n);
            clamp_col(pane, &lines);
        }
        KeyCode::Char('w') => {
            for _ in 0..n {
                move_word_forward(pane, &lines);
            }
        }
        KeyCode::Char('b') => {
            for _ in 0..n {
                move_word_backward(pane, &lines);
            }
        }
        KeyCode::Char('e') => {
            for _ in 0..n {
                move_word_end(pane, &lines);
            }
        }
        KeyCode::Char('0') => {
            pane.vim.cursor.col = 0;
        }
        KeyCode::Char('$') => {
            let line_len = current_line_len(pane, &lines);
            pane.vim.cursor.col = line_len.saturating_sub(1);
        }
        KeyCode::Char('g') => {
            pane.vim.pending_key = Some('g');
        }
        KeyCode::Char('G') => {
            // G or {count}G — go to last line or specific line
            // Note: count was already consumed, but if n > 1, user typed {n}G
            if n > 1 {
                pane.vim.cursor.row = (n - 1).min(total - 1);
            } else {
                pane.vim.cursor.row = total - 1;
            }
            pane.vim.cursor.col = 0;
            clamp_col(pane, &lines);
        }
        KeyCode::Char('y') => {
            pane.vim.pending_key = Some('y');
        }
        KeyCode::Char('v') => {
            pane.vim.mode = DiffViewMode::Visual;
            pane.vim.visual_anchor = Some(pane.vim.cursor);
        }
        KeyCode::Char('V') => {
            pane.vim.mode = DiffViewMode::VisualLine;
            pane.vim.visual_anchor = Some(pane.vim.cursor);
        }
        KeyCode::Char('/') => {
            pane.vim.pending_key = None;
            pane.vim.count = None;
            events.push(PaneEvent::StartSearch(PANE_DIFF_VIEW));
        }
        KeyCode::Char('n') => {
            events.push(PaneEvent::JumpToMatch(true));
        }
        KeyCode::Char('N') => {
            events.push(PaneEvent::JumpToMatch(false));
        }
        KeyCode::Esc => {
            if shared.search.query.is_some() {
                events.push(PaneEvent::ClearSearch);
            } else {
                pane.vim.mode = DiffViewMode::Scroll;
                pane.vim.pending_key = None;
                pane.vim.count = None;
            }
        }
        _ => {}
    }
    pane.scroll_to_cursor();
    events
}

fn take_count(pane: &mut super::DiffViewPane) -> usize {
    pane.vim.count.take().unwrap_or(1)
}

/// Execute y + motion (yy, yw, y$, y0, yb, ye) with count.
/// Returns the yanked text, or None if nothing to yank.
fn execute_yank_motion(
    pane: &mut super::DiffViewPane,
    motion: KeyCode,
    lines: &[String],
    count: usize,
) -> Option<String> {
    let text = match motion {
        // yy or {n}yy — yank current line(s)
        KeyCode::Char('y') => {
            let start = pane.vim.cursor.row;
            let end = (start + count).min(lines.len());
            let yanked: Vec<&str> = lines[start..end].iter().map(|s| s.as_str()).collect();
            yanked.join("\n")
        }
        // yw — yank from cursor to next word start
        KeyCode::Char('w') => {
            let saved = pane.vim.cursor;
            for _ in 0..count {
                move_word_forward(pane, lines);
            }
            let end = pane.vim.cursor;
            pane.vim.cursor = saved;
            // If motion crossed a line boundary, clamp to end of the previous line
            // No movement — yank to end of current line
            if end == saved {
                let text = if let Some(line) = lines.get(saved.row) {
                    let chars: Vec<char> = line.chars().collect();
                    let col = saved.col.min(chars.len());
                    chars[col..].iter().collect()
                } else {
                    String::new()
                };
                return if text.is_empty() { None } else { Some(text) };
            }
            let adjusted_end = if end.row > saved.row {
                let prev_line_len = line_len_at(lines, end.row.saturating_sub(1));
                CursorPos {
                    row: end.row - 1,
                    col: prev_line_len.saturating_sub(1),
                    side: saved.side,
                }
            } else {
                CursorPos {
                    row: end.row,
                    col: end.col.saturating_sub(1),
                    side: saved.side,
                }
            };
            extract_range(lines, saved, adjusted_end)
        }
        // ye — yank from cursor to end of word
        KeyCode::Char('e') => {
            let saved = pane.vim.cursor;
            for _ in 0..count {
                move_word_end(pane, lines);
            }
            let end = pane.vim.cursor;
            pane.vim.cursor = saved;
            extract_range(lines, saved, end)
        }
        // yb — yank from previous word start to cursor
        KeyCode::Char('b') => {
            let saved = pane.vim.cursor;
            for _ in 0..count {
                move_word_backward(pane, lines);
            }
            let start = pane.vim.cursor;
            pane.vim.cursor = saved;
            extract_range(lines, start, saved)
        }
        // y$ — yank to end of line
        KeyCode::Char('$') => {
            if let Some(line) = lines.get(pane.vim.cursor.row) {
                let chars: Vec<char> = line.chars().collect();
                let col = pane.vim.cursor.col.min(chars.len());
                chars[col..].iter().collect()
            } else {
                String::new()
            }
        }
        // y0 — yank to beginning of line
        KeyCode::Char('0') => {
            if let Some(line) = lines.get(pane.vim.cursor.row) {
                let chars: Vec<char> = line.chars().collect();
                let col = pane.vim.cursor.col.min(chars.len());
                chars[..col].iter().collect()
            } else {
                String::new()
            }
        }
        _ => return None,
    };
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

/// Extract text between two positions (inclusive)
fn extract_range(lines: &[String], start: CursorPos, end: CursorPos) -> String {
    if start.row == end.row {
        if let Some(line) = lines.get(start.row) {
            let chars: Vec<char> = line.chars().collect();
            let s = start.col.min(chars.len());
            let e = (end.col + 1).min(chars.len());
            return chars[s..e].iter().collect();
        }
        return String::new();
    }
    let mut result = String::new();
    for r in start.row..=end.row {
        if let Some(line) = lines.get(r) {
            let chars: Vec<char> = line.chars().collect();
            if r == start.row {
                let s = start.col.min(chars.len());
                result.extend(&chars[s..]);
            } else if r == end.row {
                result.push('\n');
                let e = (end.col + 1).min(chars.len());
                result.extend(&chars[..e]);
            } else {
                result.push('\n');
                result.extend(&chars[..]);
            }
        }
    }
    result
}

pub(crate) fn handle_diff_visual_key(
    pane: &mut super::DiffViewPane,
    shared: &PaneShared,
    key: KeyEvent,
) -> Vec<PaneEvent> {
    // Handle pending key sequences
    if let Some(prefix) = pane.vim.pending_key {
        pane.vim.pending_key = None;
        match prefix {
            'i' | 'a' => {
                let lines = content_lines(pane, shared);
                apply_text_object(pane, prefix, key.code, &lines);
            }
            'g' => {
                let lines = content_lines(pane, shared);
                if key.code == KeyCode::Char('g') {
                    if let Some(n) = pane.vim.count.take() {
                        pane.vim.cursor.row =
                            (n.saturating_sub(1)).min(lines.len().saturating_sub(1));
                    } else {
                        pane.vim.cursor.row = 0;
                    }
                    pane.vim.cursor.col = 0;
                    clamp_col(pane, &lines);
                }
                pane.vim.count = None;
            }
            _ => {}
        }
        pane.scroll_to_cursor();
        return vec![];
    }

    // Accumulate digit count prefix
    if let KeyCode::Char(c @ '1'..='9') = key.code {
        let digit = (c as usize) - ('0' as usize);
        pane.vim.count = Some(pane.vim.count.unwrap_or(0) * 10 + digit);
        return vec![];
    }
    if let KeyCode::Char('0') = key.code {
        if pane.vim.count.is_some() {
            pane.vim.count = Some(pane.vim.count.unwrap() * 10);
            return vec![];
        }
    }

    let n = take_count(pane);
    let lines = content_lines(pane, shared);
    let total = lines.len();
    if total == 0 {
        return vec![];
    }

    let mut events = Vec::new();

    match key.code {
        KeyCode::Char('h') | KeyCode::Left => {
            pane.vim.cursor.col = pane.vim.cursor.col.saturating_sub(n);
        }
        KeyCode::Char('l') | KeyCode::Right => {
            let line_len = current_line_len(pane, &lines);
            pane.vim.cursor.col = (pane.vim.cursor.col + n).min(line_len.saturating_sub(1));
        }
        KeyCode::Char('j') | KeyCode::Down => {
            pane.vim.cursor.row = (pane.vim.cursor.row + n).min(total - 1);
            clamp_col(pane, &lines);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            pane.vim.cursor.row = pane.vim.cursor.row.saturating_sub(n);
            clamp_col(pane, &lines);
        }
        KeyCode::Char('w') => {
            for _ in 0..n {
                move_word_forward(pane, &lines);
            }
        }
        KeyCode::Char('b') => {
            for _ in 0..n {
                move_word_backward(pane, &lines);
            }
        }
        KeyCode::Char('e') => {
            for _ in 0..n {
                move_word_end(pane, &lines);
            }
        }
        KeyCode::Char('0') => {
            pane.vim.cursor.col = 0;
        }
        KeyCode::Char('$') => {
            let line_len = current_line_len(pane, &lines);
            pane.vim.cursor.col = line_len.saturating_sub(1);
        }
        KeyCode::Char('g') => {
            pane.vim.pending_key = Some('g');
        }
        KeyCode::Char('G') => {
            if n > 1 {
                pane.vim.cursor.row = (n - 1).min(total - 1);
            } else {
                pane.vim.cursor.row = total - 1;
            }
            pane.vim.cursor.col = 0;
            clamp_col(pane, &lines);
        }
        KeyCode::Char('i') | KeyCode::Char('a') => {
            if let KeyCode::Char(c) = key.code {
                pane.vim.pending_key = Some(c);
            }
        }
        KeyCode::Char('y') => {
            let text = yank_selection(pane, &lines);
            pane.vim.mode = DiffViewMode::Normal;
            pane.vim.visual_anchor = None;
            if !text.is_empty() {
                events.push(PaneEvent::CopyToClipboard(text));
            }
        }
        KeyCode::Char('v') => {
            if pane.vim.mode == DiffViewMode::Visual {
                pane.vim.mode = DiffViewMode::Normal;
                pane.vim.visual_anchor = None;
            } else {
                pane.vim.mode = DiffViewMode::Visual;
                pane.vim.visual_anchor = Some(pane.vim.cursor);
            }
        }
        KeyCode::Char('V') => {
            if pane.vim.mode == DiffViewMode::VisualLine {
                pane.vim.mode = DiffViewMode::Normal;
                pane.vim.visual_anchor = None;
            } else {
                pane.vim.mode = DiffViewMode::VisualLine;
                pane.vim.visual_anchor = Some(pane.vim.cursor);
            }
        }
        KeyCode::Char('/') => {
            pane.vim.pending_key = None;
            pane.vim.count = None;
            events.push(PaneEvent::StartSearch(PANE_DIFF_VIEW));
        }
        KeyCode::Char('n') => {
            events.push(PaneEvent::JumpToMatch(true));
        }
        KeyCode::Char('N') => {
            events.push(PaneEvent::JumpToMatch(false));
        }
        KeyCode::Esc => {
            pane.vim.mode = DiffViewMode::Normal;
            pane.vim.visual_anchor = None;
            pane.vim.pending_key = None;
            pane.vim.count = None;
        }
        _ => {}
    }
    pane.scroll_to_cursor();
    events
}

/// Build flat list of content strings for the current side of the diff.
/// Results are cached and reused until the file or side changes.
pub(crate) fn content_lines(pane: &mut super::DiffViewPane, _shared: &PaneShared) -> Vec<String> {
    let files = std::rc::Rc::clone(&pane.files);
    let file = match pane.current_file_idx.and_then(|i| files.get(i)) {
        Some(f) => f,
        None => return Vec::new(),
    };
    let side = pane.vim.cursor.side;

    // Return cached result if still valid
    if let Some((ref path, cached_side, ref lines)) = pane.content_lines_cache {
        if *path == file.path && cached_side == side {
            return lines.clone();
        }
    }

    let mut lines = Vec::new();
    for hunk in &file.hunks {
        lines.push(hunk.header.clone());
        for row in &hunk.rows {
            let side_line = match side {
                DiffSide::Left => row.left.as_ref(),
                DiffSide::Right => row.right.as_ref(),
            };
            match side_line {
                Some(sl) => lines.push(sl.content.clone()),
                None => lines.push(String::new()),
            }
        }
    }
    pane.content_lines_cache = Some((file.path.clone(), side, lines.clone()));
    lines
}

fn current_line_len(pane: &super::DiffViewPane, lines: &[String]) -> usize {
    lines
        .get(pane.vim.cursor.row)
        .map(|l| l.chars().count().max(1))
        .unwrap_or(1)
}

fn clamp_col(pane: &mut super::DiffViewPane, lines: &[String]) {
    let len = current_line_len(pane, lines);
    if pane.vim.cursor.col >= len {
        pane.vim.cursor.col = len.saturating_sub(1);
    }
}

fn move_word_forward(pane: &mut super::DiffViewPane, lines: &[String]) {
    let total = lines.len();
    if total == 0 {
        return;
    }
    let line: Vec<char> = lines[pane.vim.cursor.row].chars().collect();
    let mut col = pane.vim.cursor.col;
    let mut row = pane.vim.cursor.row;

    // Skip current word chars
    while col < line.len() && !line[col].is_whitespace() {
        col += 1;
    }
    // Skip whitespace
    while col < line.len() && line[col].is_whitespace() {
        col += 1;
    }
    // If at end of line, go to next line col 0
    if col >= line.len() && row + 1 < total {
        row += 1;
        col = 0;
        // Skip leading whitespace on new line
        let next_line: Vec<char> = lines[row].chars().collect();
        while col < next_line.len() && next_line[col].is_whitespace() {
            col += 1;
        }
    }
    pane.vim.cursor.row = row;
    pane.vim.cursor.col = col.min(line_len_at(lines, row).saturating_sub(1));
}

fn move_word_backward(pane: &mut super::DiffViewPane, lines: &[String]) {
    if lines.is_empty() {
        return;
    }
    let line: Vec<char> = lines[pane.vim.cursor.row].chars().collect();
    let mut col = pane.vim.cursor.col;
    let mut row = pane.vim.cursor.row;

    if col == 0 {
        if row > 0 {
            row -= 1;
            col = line_len_at(lines, row).saturating_sub(1);
        }
        pane.vim.cursor.row = row;
        pane.vim.cursor.col = col;
        return;
    }

    // Move back one
    col = col.saturating_sub(1);
    // Skip whitespace backward
    while col > 0 && line.get(col).is_some_and(|c| c.is_whitespace()) {
        col -= 1;
    }
    // Skip word chars backward
    while col > 0 && line.get(col - 1).is_some_and(|c| !c.is_whitespace()) {
        col -= 1;
    }
    pane.vim.cursor.row = row;
    pane.vim.cursor.col = col;
}

fn move_word_end(pane: &mut super::DiffViewPane, lines: &[String]) {
    let total = lines.len();
    if total == 0 {
        return;
    }
    let line: Vec<char> = lines[pane.vim.cursor.row].chars().collect();
    let mut col = pane.vim.cursor.col;
    let mut row = pane.vim.cursor.row;

    // Move forward at least one
    col += 1;
    if col >= line.len() && row + 1 < total {
        row += 1;
        col = 0;
    }
    let cur_line: Vec<char> = lines[row].chars().collect();
    // Skip whitespace
    while col < cur_line.len() && cur_line[col].is_whitespace() {
        col += 1;
    }
    // Move to end of word
    while col + 1 < cur_line.len() && !cur_line[col + 1].is_whitespace() {
        col += 1;
    }
    pane.vim.cursor.row = row;
    pane.vim.cursor.col = col.min(line_len_at(lines, row).saturating_sub(1));
}

fn line_len_at(lines: &[String], row: usize) -> usize {
    lines
        .get(row)
        .map(|l| l.chars().count().max(1))
        .unwrap_or(1)
}

fn yank_selection(pane: &super::DiffViewPane, lines: &[String]) -> String {
    let anchor = match pane.vim.visual_anchor {
        Some(a) => a,
        None => return String::new(),
    };
    match pane.vim.mode {
        DiffViewMode::VisualLine => {
            let start_row = anchor.row.min(pane.vim.cursor.row);
            let end_row = anchor.row.max(pane.vim.cursor.row);
            let mut result = Vec::new();
            for r in start_row..=end_row {
                if let Some(line) = lines.get(r) {
                    result.push(line.as_str());
                }
            }
            result.join("\n")
        }
        DiffViewMode::Visual => {
            let (start, end) = ordered_selection(pane, anchor);
            if start.row == end.row {
                if let Some(line) = lines.get(start.row) {
                    let chars: Vec<char> = line.chars().collect();
                    let s = start.col.min(chars.len());
                    let e = (end.col + 1).min(chars.len());
                    return chars[s..e].iter().collect();
                }
                return String::new();
            }
            let mut result = String::new();
            for r in start.row..=end.row {
                if let Some(line) = lines.get(r) {
                    let chars: Vec<char> = line.chars().collect();
                    if r == start.row {
                        let s = start.col.min(chars.len());
                        result.extend(&chars[s..]);
                    } else if r == end.row {
                        result.push('\n');
                        let e = (end.col + 1).min(chars.len());
                        result.extend(&chars[..e]);
                    } else {
                        result.push('\n');
                        result.extend(&chars[..]);
                    }
                }
            }
            result
        }
        _ => String::new(),
    }
}

fn ordered_selection(pane: &super::DiffViewPane, anchor: CursorPos) -> (CursorPos, CursorPos) {
    if anchor.row < pane.vim.cursor.row
        || (anchor.row == pane.vim.cursor.row && anchor.col <= pane.vim.cursor.col)
    {
        (anchor, pane.vim.cursor)
    } else {
        (pane.vim.cursor, anchor)
    }
}

fn apply_text_object(pane: &mut super::DiffViewPane, prefix: char, key: KeyCode, lines: &[String]) {
    let inner = prefix == 'i';
    match key {
        KeyCode::Char('w') => select_text_object_word(pane, inner, lines),
        KeyCode::Char('"') => select_text_object_delim(pane, inner, '"', '"', lines),
        KeyCode::Char('\'') => select_text_object_delim(pane, inner, '\'', '\'', lines),
        KeyCode::Char('(') | KeyCode::Char(')') => {
            select_text_object_delim(pane, inner, '(', ')', lines);
        }
        KeyCode::Char('{') | KeyCode::Char('}') => {
            select_text_object_delim(pane, inner, '{', '}', lines);
        }
        _ => {}
    }
}

fn select_text_object_word(pane: &mut super::DiffViewPane, inner: bool, lines: &[String]) {
    if let Some(line) = lines.get(pane.vim.cursor.row) {
        let chars: Vec<char> = line.chars().collect();
        let col = pane.vim.cursor.col.min(chars.len().saturating_sub(1));
        if chars.is_empty() {
            return;
        }
        // Find word boundaries
        let mut start = col;
        while start > 0 && !chars[start - 1].is_whitespace() {
            start -= 1;
        }
        let mut end = col;
        while end + 1 < chars.len() && !chars[end + 1].is_whitespace() {
            end += 1;
        }
        if !inner {
            // Include trailing whitespace
            while end + 1 < chars.len() && chars[end + 1].is_whitespace() {
                end += 1;
            }
        }
        pane.vim.visual_anchor = Some(CursorPos {
            row: pane.vim.cursor.row,
            col: start,
            side: pane.vim.cursor.side,
        });
        pane.vim.cursor.col = end;
    }
}

fn select_text_object_delim(
    pane: &mut super::DiffViewPane,
    inner: bool,
    open: char,
    close: char,
    lines: &[String],
) {
    if let Some(line) = lines.get(pane.vim.cursor.row) {
        let chars: Vec<char> = line.chars().collect();
        let col = pane.vim.cursor.col.min(chars.len().saturating_sub(1));
        // Search backward for open
        let mut open_pos = None;
        for i in (0..=col).rev() {
            if chars[i] == open {
                open_pos = Some(i);
                break;
            }
        }
        // Search forward for close
        let mut close_pos = None;
        for (i, &ch) in chars.iter().enumerate().skip(col + 1) {
            if ch == close {
                close_pos = Some(i);
                break;
            }
        }
        if let (Some(op), Some(cp)) = (open_pos, close_pos) {
            if inner {
                pane.vim.visual_anchor = Some(CursorPos {
                    row: pane.vim.cursor.row,
                    col: op + 1,
                    side: pane.vim.cursor.side,
                });
                pane.vim.cursor.col = cp.saturating_sub(1);
            } else {
                pane.vim.visual_anchor = Some(CursorPos {
                    row: pane.vim.cursor.row,
                    col: op,
                    side: pane.vim.cursor.side,
                });
                pane.vim.cursor.col = cp;
            }
        }
    }
}
