use crate::core::app::AppContext;
use crate::core::pane::FocusState;
use crate::core::search::SearchOrigin;
use crate::git::domain::search::{jump_to_git_match, scroll_to_cursor};
use crate::git::state::{CursorPos, DiffSide, DiffViewMode, GitState};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub(crate) fn handle_diff_scroll_key(ctx: &mut AppContext, git: &mut GitState, key: KeyEvent) {
    let max_scroll = git
        .scroll
        .total_lines
        .saturating_sub(git.scroll.view_height);
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            git.scroll.y = (git.scroll.y + 1).min(max_scroll);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            git.scroll.y = git.scroll.y.saturating_sub(1);
        }
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let half = git.scroll.view_height / 2;
            git.scroll.y = (git.scroll.y + half).min(max_scroll);
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let half = git.scroll.view_height / 2;
            git.scroll.y = git.scroll.y.saturating_sub(half);
        }
        KeyCode::Char('g') => {
            git.scroll.y = 0;
        }
        KeyCode::Char('G') => {
            git.scroll.y = max_scroll;
        }
        KeyCode::Char('h') | KeyCode::Left => {
            git.scroll.x = git.scroll.x.saturating_sub(4);
        }
        KeyCode::Esc => {
            if git.search.query.is_some() {
                git.search.clear();
            } else {
                git.set_focus(git.previous_pane);
            }
        }
        KeyCode::Char('l') | KeyCode::Right => {
            git.scroll.x = git.scroll.x.saturating_add(4);
        }
        KeyCode::Char('/') => {
            git.search.start(SearchOrigin::DiffView);
            git.vim.pending_key = None;
        }
        KeyCode::Char('n') => {
            jump_to_git_match(ctx, git, true);
        }
        KeyCode::Char('N') => {
            jump_to_git_match(ctx, git, false);
        }
        KeyCode::Char('i') => {
            // Enter Normal mode with cursor at top-left of visible area
            let lines = content_lines(git);
            if !lines.is_empty() {
                git.vim.mode = DiffViewMode::Normal;
                git.vim.cursor = CursorPos {
                    row: git.scroll.y as usize,
                    col: 0,
                    side: DiffSide::Left,
                };
            }
        }
        _ => {}
    }
}

pub(crate) fn handle_diff_normal_key(ctx: &mut AppContext, git: &mut GitState, key: KeyEvent) {
    // Handle Ctrl+w prefix for panel switching
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('w') {
        git.vim.pending_key = Some('w');
        return;
    }

    // Handle pending key sequences
    if let Some(pending) = git.vim.pending_key {
        git.vim.pending_key = None;
        match pending {
            'w' => {
                match key.code {
                    KeyCode::Char('h') => git.vim.cursor.side = DiffSide::Left,
                    KeyCode::Char('l') => git.vim.cursor.side = DiffSide::Right,
                    _ => {}
                }
                git.vim.count = None;
                return;
            }
            'y' => {
                let lines = content_lines(git);
                let n = take_count(git);
                execute_yank_motion(ctx, git, key.code, &lines, n);
                return;
            }
            'g' => {
                let lines = content_lines(git);
                if let KeyCode::Char('g') = key.code {
                    // gg or {count}gg — go to line
                    if let Some(n) = git.vim.count.take() {
                        git.vim.cursor.row =
                            (n.saturating_sub(1)).min(lines.len().saturating_sub(1));
                    } else {
                        git.vim.cursor.row = 0;
                    }
                    git.vim.cursor.col = 0;
                    clamp_col(git, &lines);
                }
                git.vim.count = None;
                scroll_to_cursor(git);
                return;
            }
            _ => {}
        }
        git.vim.count = None;
        return;
    }

    // Accumulate digit count prefix (1-9 start, 0 appends)
    if let KeyCode::Char(c @ '1'..='9') = key.code {
        let digit = (c as usize) - ('0' as usize);
        git.vim.count = Some(git.vim.count.unwrap_or(0) * 10 + digit);
        return;
    }
    if let KeyCode::Char('0') = key.code {
        if git.vim.count.is_some() {
            git.vim.count = Some(git.vim.count.unwrap() * 10);
            return;
        }
        // else fall through to handle '0' as go-to-line-start
    }

    let n = take_count(git);
    let lines = content_lines(git);
    let total = lines.len();
    if total == 0 {
        return;
    }

    match key.code {
        KeyCode::Char('h') | KeyCode::Left => {
            git.vim.cursor.col = git.vim.cursor.col.saturating_sub(n);
        }
        KeyCode::Char('l') | KeyCode::Right => {
            let line_len = current_line_len(git, &lines);
            git.vim.cursor.col = (git.vim.cursor.col + n).min(line_len.saturating_sub(1));
        }
        KeyCode::Char('j') | KeyCode::Down => {
            git.vim.cursor.row = (git.vim.cursor.row + n).min(total - 1);
            clamp_col(git, &lines);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            git.vim.cursor.row = git.vim.cursor.row.saturating_sub(n);
            clamp_col(git, &lines);
        }
        KeyCode::Char('w') => {
            for _ in 0..n {
                move_word_forward(git, &lines);
            }
        }
        KeyCode::Char('b') => {
            for _ in 0..n {
                move_word_backward(git, &lines);
            }
        }
        KeyCode::Char('e') => {
            for _ in 0..n {
                move_word_end(git, &lines);
            }
        }
        KeyCode::Char('0') => {
            git.vim.cursor.col = 0;
        }
        KeyCode::Char('$') => {
            let line_len = current_line_len(git, &lines);
            git.vim.cursor.col = line_len.saturating_sub(1);
        }
        KeyCode::Char('g') => {
            git.vim.pending_key = Some('g');
        }
        KeyCode::Char('G') => {
            // G or {count}G — go to last line or specific line
            // Note: count was already consumed, but if n > 1, user typed {n}G
            if n > 1 {
                git.vim.cursor.row = (n - 1).min(total - 1);
            } else {
                git.vim.cursor.row = total - 1;
            }
            git.vim.cursor.col = 0;
            clamp_col(git, &lines);
        }
        KeyCode::Char('y') => {
            git.vim.pending_key = Some('y');
        }
        KeyCode::Char('v') => {
            git.vim.mode = DiffViewMode::Visual;
            git.vim.visual_anchor = Some(git.vim.cursor);
        }
        KeyCode::Char('V') => {
            git.vim.mode = DiffViewMode::VisualLine;
            git.vim.visual_anchor = Some(git.vim.cursor);
        }
        KeyCode::Char('/') => {
            git.search.start(SearchOrigin::DiffView);
            git.vim.pending_key = None;
            git.vim.count = None;
        }
        KeyCode::Char('n') => {
            jump_to_git_match(ctx, git, true);
        }
        KeyCode::Char('N') => {
            jump_to_git_match(ctx, git, false);
        }
        KeyCode::Esc => {
            if git.search.query.is_some() {
                git.search.clear();
            } else {
                git.vim.mode = DiffViewMode::Scroll;
                git.vim.pending_key = None;
                git.vim.count = None;
            }
        }
        _ => {}
    }
    scroll_to_cursor(git);
}

fn take_count(git: &mut GitState) -> usize {
    git.vim.count.take().unwrap_or(1)
}

/// Execute y + motion (yy, yw, y$, y0, yb, ye) with count
fn execute_yank_motion(
    ctx: &mut AppContext,
    git: &mut GitState,
    motion: KeyCode,
    lines: &[String],
    count: usize,
) {
    let text = match motion {
        // yy or {n}yy — yank current line(s)
        KeyCode::Char('y') => {
            let start = git.vim.cursor.row;
            let end = (start + count).min(lines.len());
            let yanked: Vec<&str> = lines[start..end].iter().map(|s| s.as_str()).collect();
            yanked.join("\n")
        }
        // yw — yank from cursor to next word start
        KeyCode::Char('w') => {
            let saved = git.vim.cursor;
            for _ in 0..count {
                move_word_forward(git, lines);
            }
            let end = git.vim.cursor;
            git.vim.cursor = saved;
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
                copy_to_clipboard(ctx, &text);
                return;
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
            let saved = git.vim.cursor;
            for _ in 0..count {
                move_word_end(git, lines);
            }
            let end = git.vim.cursor;
            git.vim.cursor = saved;
            extract_range(lines, saved, end)
        }
        // yb — yank from previous word start to cursor
        KeyCode::Char('b') => {
            let saved = git.vim.cursor;
            for _ in 0..count {
                move_word_backward(git, lines);
            }
            let start = git.vim.cursor;
            git.vim.cursor = saved;
            extract_range(lines, start, saved)
        }
        // y$ — yank to end of line
        KeyCode::Char('$') => {
            if let Some(line) = lines.get(git.vim.cursor.row) {
                let chars: Vec<char> = line.chars().collect();
                let col = git.vim.cursor.col.min(chars.len());
                chars[col..].iter().collect()
            } else {
                String::new()
            }
        }
        // y0 — yank to beginning of line
        KeyCode::Char('0') => {
            if let Some(line) = lines.get(git.vim.cursor.row) {
                let chars: Vec<char> = line.chars().collect();
                let col = git.vim.cursor.col.min(chars.len());
                chars[..col].iter().collect()
            } else {
                String::new()
            }
        }
        _ => return,
    };
    copy_to_clipboard(ctx, &text);
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

pub(crate) fn handle_diff_visual_key(ctx: &mut AppContext, git: &mut GitState, key: KeyEvent) {
    // Handle pending key sequences
    if let Some(prefix) = git.vim.pending_key {
        git.vim.pending_key = None;
        match prefix {
            'i' | 'a' => {
                let lines = content_lines(git);
                apply_text_object(git, prefix, key.code, &lines);
            }
            'g' => {
                let lines = content_lines(git);
                if key.code == KeyCode::Char('g') {
                    if let Some(n) = git.vim.count.take() {
                        git.vim.cursor.row =
                            (n.saturating_sub(1)).min(lines.len().saturating_sub(1));
                    } else {
                        git.vim.cursor.row = 0;
                    }
                    git.vim.cursor.col = 0;
                    clamp_col(git, &lines);
                }
                git.vim.count = None;
            }
            _ => {}
        }
        scroll_to_cursor(git);
        return;
    }

    // Accumulate digit count prefix
    if let KeyCode::Char(c @ '1'..='9') = key.code {
        let digit = (c as usize) - ('0' as usize);
        git.vim.count = Some(git.vim.count.unwrap_or(0) * 10 + digit);
        return;
    }
    if let KeyCode::Char('0') = key.code {
        if git.vim.count.is_some() {
            git.vim.count = Some(git.vim.count.unwrap() * 10);
            return;
        }
    }

    let n = take_count(git);
    let lines = content_lines(git);
    let total = lines.len();
    if total == 0 {
        return;
    }

    match key.code {
        KeyCode::Char('h') | KeyCode::Left => {
            git.vim.cursor.col = git.vim.cursor.col.saturating_sub(n);
        }
        KeyCode::Char('l') | KeyCode::Right => {
            let line_len = current_line_len(git, &lines);
            git.vim.cursor.col = (git.vim.cursor.col + n).min(line_len.saturating_sub(1));
        }
        KeyCode::Char('j') | KeyCode::Down => {
            git.vim.cursor.row = (git.vim.cursor.row + n).min(total - 1);
            clamp_col(git, &lines);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            git.vim.cursor.row = git.vim.cursor.row.saturating_sub(n);
            clamp_col(git, &lines);
        }
        KeyCode::Char('w') => {
            for _ in 0..n {
                move_word_forward(git, &lines);
            }
        }
        KeyCode::Char('b') => {
            for _ in 0..n {
                move_word_backward(git, &lines);
            }
        }
        KeyCode::Char('e') => {
            for _ in 0..n {
                move_word_end(git, &lines);
            }
        }
        KeyCode::Char('0') => {
            git.vim.cursor.col = 0;
        }
        KeyCode::Char('$') => {
            let line_len = current_line_len(git, &lines);
            git.vim.cursor.col = line_len.saturating_sub(1);
        }
        KeyCode::Char('g') => {
            git.vim.pending_key = Some('g');
        }
        KeyCode::Char('G') => {
            if n > 1 {
                git.vim.cursor.row = (n - 1).min(total - 1);
            } else {
                git.vim.cursor.row = total - 1;
            }
            git.vim.cursor.col = 0;
            clamp_col(git, &lines);
        }
        KeyCode::Char('i') | KeyCode::Char('a') => {
            if let KeyCode::Char(c) = key.code {
                git.vim.pending_key = Some(c);
            }
        }
        KeyCode::Char('y') => {
            let text = yank_selection(git, &lines);
            copy_to_clipboard(ctx, &text);
            git.vim.mode = DiffViewMode::Normal;
            git.vim.visual_anchor = None;
        }
        KeyCode::Char('v') => {
            if git.vim.mode == DiffViewMode::Visual {
                git.vim.mode = DiffViewMode::Normal;
                git.vim.visual_anchor = None;
            } else {
                git.vim.mode = DiffViewMode::Visual;
                git.vim.visual_anchor = Some(git.vim.cursor);
            }
        }
        KeyCode::Char('V') => {
            if git.vim.mode == DiffViewMode::VisualLine {
                git.vim.mode = DiffViewMode::Normal;
                git.vim.visual_anchor = None;
            } else {
                git.vim.mode = DiffViewMode::VisualLine;
                git.vim.visual_anchor = Some(git.vim.cursor);
            }
        }
        KeyCode::Char('/') => {
            git.search.start(SearchOrigin::DiffView);
            git.vim.pending_key = None;
            git.vim.count = None;
        }
        KeyCode::Char('n') => {
            jump_to_git_match(ctx, git, true);
        }
        KeyCode::Char('N') => {
            jump_to_git_match(ctx, git, false);
        }
        KeyCode::Esc => {
            git.vim.mode = DiffViewMode::Normal;
            git.vim.visual_anchor = None;
            git.vim.pending_key = None;
            git.vim.count = None;
        }
        _ => {}
    }
    scroll_to_cursor(git);
}

pub(crate) fn copy_to_clipboard(ctx: &mut AppContext, text: &str) {
    if text.is_empty() {
        return;
    }
    let line_count = text.lines().count().max(1);
    match arboard::Clipboard::new() {
        Ok(mut clip) => {
            if clip.set_text(text).is_ok() {
                ctx.status_message = Some(format!(
                    "Yanked {line_count} line{}",
                    if line_count == 1 { "" } else { "s" }
                ));
            } else {
                ctx.status_message = Some("Clipboard error".to_string());
            }
        }
        Err(_) => {
            ctx.status_message = Some("Clipboard unavailable".to_string());
        }
    }
}

/// Build flat list of content strings for the current side of the diff.
/// Results are cached and reused until the file or side changes.
pub(crate) fn content_lines(git: &mut GitState) -> Vec<String> {
    let file = match git.selected_file() {
        Some(f) => f.clone(),
        None => return Vec::new(),
    };
    let side = git.vim.cursor.side;

    // Return cached result if still valid
    if let Some((ref path, cached_side, ref lines)) = git.highlight.content_lines_cache {
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
    git.highlight.content_lines_cache = Some((file.path.clone(), side, lines.clone()));
    lines
}

fn current_line_len(git: &GitState, lines: &[String]) -> usize {
    lines
        .get(git.vim.cursor.row)
        .map(|l| l.chars().count().max(1))
        .unwrap_or(1)
}

fn clamp_col(git: &mut GitState, lines: &[String]) {
    let len = current_line_len(git, lines);
    if git.vim.cursor.col >= len {
        git.vim.cursor.col = len.saturating_sub(1);
    }
}

fn move_word_forward(git: &mut GitState, lines: &[String]) {
    let total = lines.len();
    if total == 0 {
        return;
    }
    let line: Vec<char> = lines[git.vim.cursor.row].chars().collect();
    let mut col = git.vim.cursor.col;
    let mut row = git.vim.cursor.row;

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
    git.vim.cursor.row = row;
    git.vim.cursor.col = col.min(line_len_at(lines, row).saturating_sub(1));
}

fn move_word_backward(git: &mut GitState, lines: &[String]) {
    if lines.is_empty() {
        return;
    }
    let line: Vec<char> = lines[git.vim.cursor.row].chars().collect();
    let mut col = git.vim.cursor.col;
    let mut row = git.vim.cursor.row;

    if col == 0 {
        if row > 0 {
            row -= 1;
            col = line_len_at(lines, row).saturating_sub(1);
        }
        git.vim.cursor.row = row;
        git.vim.cursor.col = col;
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
    git.vim.cursor.row = row;
    git.vim.cursor.col = col;
}

fn move_word_end(git: &mut GitState, lines: &[String]) {
    let total = lines.len();
    if total == 0 {
        return;
    }
    let line: Vec<char> = lines[git.vim.cursor.row].chars().collect();
    let mut col = git.vim.cursor.col;
    let mut row = git.vim.cursor.row;

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
    git.vim.cursor.row = row;
    git.vim.cursor.col = col.min(line_len_at(lines, row).saturating_sub(1));
}

fn line_len_at(lines: &[String], row: usize) -> usize {
    lines
        .get(row)
        .map(|l| l.chars().count().max(1))
        .unwrap_or(1)
}

fn yank_selection(git: &GitState, lines: &[String]) -> String {
    let anchor = match git.vim.visual_anchor {
        Some(a) => a,
        None => return String::new(),
    };
    match git.vim.mode {
        DiffViewMode::VisualLine => {
            let start_row = anchor.row.min(git.vim.cursor.row);
            let end_row = anchor.row.max(git.vim.cursor.row);
            let mut result = Vec::new();
            for r in start_row..=end_row {
                if let Some(line) = lines.get(r) {
                    result.push(line.as_str());
                }
            }
            result.join("\n")
        }
        DiffViewMode::Visual => {
            let (start, end) = ordered_selection(git, anchor);
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

fn ordered_selection(git: &GitState, anchor: CursorPos) -> (CursorPos, CursorPos) {
    if anchor.row < git.vim.cursor.row
        || (anchor.row == git.vim.cursor.row && anchor.col <= git.vim.cursor.col)
    {
        (anchor, git.vim.cursor)
    } else {
        (git.vim.cursor, anchor)
    }
}

fn apply_text_object(git: &mut GitState, prefix: char, key: KeyCode, lines: &[String]) {
    let inner = prefix == 'i';
    match key {
        KeyCode::Char('w') => select_text_object_word(git, inner, lines),
        KeyCode::Char('"') => select_text_object_delim(git, inner, '"', '"', lines),
        KeyCode::Char('\'') => select_text_object_delim(git, inner, '\'', '\'', lines),
        KeyCode::Char('(') | KeyCode::Char(')') => {
            select_text_object_delim(git, inner, '(', ')', lines);
        }
        KeyCode::Char('{') | KeyCode::Char('}') => {
            select_text_object_delim(git, inner, '{', '}', lines);
        }
        _ => {}
    }
}

fn select_text_object_word(git: &mut GitState, inner: bool, lines: &[String]) {
    if let Some(line) = lines.get(git.vim.cursor.row) {
        let chars: Vec<char> = line.chars().collect();
        let col = git.vim.cursor.col.min(chars.len().saturating_sub(1));
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
        git.vim.visual_anchor = Some(CursorPos {
            row: git.vim.cursor.row,
            col: start,
            side: git.vim.cursor.side,
        });
        git.vim.cursor.col = end;
    }
}

fn select_text_object_delim(
    git: &mut GitState,
    inner: bool,
    open: char,
    close: char,
    lines: &[String],
) {
    if let Some(line) = lines.get(git.vim.cursor.row) {
        let chars: Vec<char> = line.chars().collect();
        let col = git.vim.cursor.col.min(chars.len().saturating_sub(1));
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
                git.vim.visual_anchor = Some(CursorPos {
                    row: git.vim.cursor.row,
                    col: op + 1,
                    side: git.vim.cursor.side,
                });
                git.vim.cursor.col = cp.saturating_sub(1);
            } else {
                git.vim.visual_anchor = Some(CursorPos {
                    row: git.vim.cursor.row,
                    col: op,
                    side: git.vim.cursor.side,
                });
                git.vim.cursor.col = cp;
            }
        }
    }
}
