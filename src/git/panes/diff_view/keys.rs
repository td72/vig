use crate::core::app::{App, SearchOrigin};
use crate::git::state::{CursorPos, DiffSide, DiffViewMode};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

impl App {
    pub(crate) fn handle_diff_scroll_key(&mut self, key: KeyEvent) {
        let max_scroll = self
            .git
            .diff_total_lines
            .saturating_sub(self.git.diff_view_height);
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.git.diff_scroll_y = (self.git.diff_scroll_y + 1).min(max_scroll);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.git.diff_scroll_y = self.git.diff_scroll_y.saturating_sub(1);
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = self.git.diff_view_height / 2;
                self.git.diff_scroll_y = (self.git.diff_scroll_y + half).min(max_scroll);
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = self.git.diff_view_height / 2;
                self.git.diff_scroll_y = self.git.diff_scroll_y.saturating_sub(half);
            }
            KeyCode::Char('g') => {
                self.git.diff_scroll_y = 0;
            }
            KeyCode::Char('G') => {
                self.git.diff_scroll_y = max_scroll;
            }
            KeyCode::Char('h') | KeyCode::Left => {
                self.git.diff_scroll_x = self.git.diff_scroll_x.saturating_sub(4);
            }
            KeyCode::Esc => {
                if self.git.search.query.is_some() {
                    self.git.search.clear();
                } else {
                    self.git.set_focus(self.git.previous_pane);
                }
            }
            KeyCode::Char('l') | KeyCode::Right => {
                self.git.diff_scroll_x = self.git.diff_scroll_x.saturating_add(4);
            }
            KeyCode::Char('/') => {
                self.git.search.start(SearchOrigin::DiffView);
                self.git.pending_key = None;
            }
            KeyCode::Char('n') => {
                self.jump_to_git_match(true);
            }
            KeyCode::Char('N') => {
                self.jump_to_git_match(false);
            }
            KeyCode::Char('i') => {
                // Enter Normal mode with cursor at top-left of visible area
                let lines = self.content_lines();
                if !lines.is_empty() {
                    self.git.diff_view_mode = DiffViewMode::Normal;
                    self.git.cursor_pos = CursorPos {
                        row: self.git.diff_scroll_y as usize,
                        col: 0,
                        side: DiffSide::Left,
                    };
                }
            }
            _ => {}
        }
    }

    pub(crate) fn handle_diff_normal_key(&mut self, key: KeyEvent) {
        // Handle Ctrl+w prefix for panel switching
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('w') {
            self.git.pending_key = Some('w');
            return;
        }

        // Handle pending key sequences
        if let Some(pending) = self.git.pending_key {
            self.git.pending_key = None;
            match pending {
                'w' => {
                    match key.code {
                        KeyCode::Char('h') => self.git.cursor_pos.side = DiffSide::Left,
                        KeyCode::Char('l') => self.git.cursor_pos.side = DiffSide::Right,
                        _ => {}
                    }
                    self.git.count = None;
                    return;
                }
                'y' => {
                    let lines = self.content_lines();
                    let n = self.take_count();
                    self.execute_yank_motion(key.code, &lines, n);
                    return;
                }
                'g' => {
                    let lines = self.content_lines();
                    match key.code {
                        KeyCode::Char('g') => {
                            // gg or {count}gg — go to line
                            if let Some(n) = self.git.count.take() {
                                self.git.cursor_pos.row =
                                    (n.saturating_sub(1)).min(lines.len().saturating_sub(1));
                            } else {
                                self.git.cursor_pos.row = 0;
                            }
                            self.git.cursor_pos.col = 0;
                            self.clamp_col(&lines);
                        }
                        _ => {}
                    }
                    self.git.count = None;
                    self.scroll_to_cursor();
                    return;
                }
                _ => {}
            }
            self.git.count = None;
            return;
        }

        // Accumulate digit count prefix (1-9 start, 0 appends)
        if let KeyCode::Char(c @ '1'..='9') = key.code {
            let digit = (c as usize) - ('0' as usize);
            self.git.count = Some(self.git.count.unwrap_or(0) * 10 + digit);
            return;
        }
        if let KeyCode::Char('0') = key.code {
            if self.git.count.is_some() {
                self.git.count = Some(self.git.count.unwrap() * 10);
                return;
            }
            // else fall through to handle '0' as go-to-line-start
        }

        let n = self.take_count();
        let lines = self.content_lines();
        let total = lines.len();
        if total == 0 {
            return;
        }

        match key.code {
            KeyCode::Char('h') | KeyCode::Left => {
                self.git.cursor_pos.col = self.git.cursor_pos.col.saturating_sub(n);
            }
            KeyCode::Char('l') | KeyCode::Right => {
                let line_len = self.current_line_len(&lines);
                self.git.cursor_pos.col =
                    (self.git.cursor_pos.col + n).min(line_len.saturating_sub(1));
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.git.cursor_pos.row = (self.git.cursor_pos.row + n).min(total - 1);
                self.clamp_col(&lines);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.git.cursor_pos.row = self.git.cursor_pos.row.saturating_sub(n);
                self.clamp_col(&lines);
            }
            KeyCode::Char('w') => {
                for _ in 0..n {
                    self.move_word_forward(&lines);
                }
            }
            KeyCode::Char('b') => {
                for _ in 0..n {
                    self.move_word_backward(&lines);
                }
            }
            KeyCode::Char('e') => {
                for _ in 0..n {
                    self.move_word_end(&lines);
                }
            }
            KeyCode::Char('0') => {
                self.git.cursor_pos.col = 0;
            }
            KeyCode::Char('$') => {
                let line_len = self.current_line_len(&lines);
                self.git.cursor_pos.col = line_len.saturating_sub(1);
            }
            KeyCode::Char('g') => {
                self.git.pending_key = Some('g');
            }
            KeyCode::Char('G') => {
                // G or {count}G — go to last line or specific line
                // Note: count was already consumed, but if n > 1, user typed {n}G
                if n > 1 {
                    self.git.cursor_pos.row = (n - 1).min(total - 1);
                } else {
                    self.git.cursor_pos.row = total - 1;
                }
                self.git.cursor_pos.col = 0;
                self.clamp_col(&lines);
            }
            KeyCode::Char('y') => {
                self.git.pending_key = Some('y');
            }
            KeyCode::Char('v') => {
                self.git.diff_view_mode = DiffViewMode::Visual;
                self.git.visual_anchor = Some(self.git.cursor_pos);
            }
            KeyCode::Char('V') => {
                self.git.diff_view_mode = DiffViewMode::VisualLine;
                self.git.visual_anchor = Some(self.git.cursor_pos);
            }
            KeyCode::Char('/') => {
                self.git.search.start(SearchOrigin::DiffView);
                self.git.pending_key = None;
                self.git.count = None;
            }
            KeyCode::Char('n') => {
                self.jump_to_git_match(true);
            }
            KeyCode::Char('N') => {
                self.jump_to_git_match(false);
            }
            KeyCode::Esc => {
                if self.git.search.query.is_some() {
                    self.git.search.clear();
                } else {
                    self.git.diff_view_mode = DiffViewMode::Scroll;
                    self.git.pending_key = None;
                    self.git.count = None;
                }
            }
            _ => {}
        }
        self.scroll_to_cursor();
    }

    fn take_count(&mut self) -> usize {
        self.git.count.take().unwrap_or(1)
    }

    /// Execute y + motion (yy, yw, y$, y0, yb, ye) with count
    fn execute_yank_motion(&mut self, motion: KeyCode, lines: &[String], count: usize) {
        let text = match motion {
            // yy or {n}yy — yank current line(s)
            KeyCode::Char('y') => {
                let start = self.git.cursor_pos.row;
                let end = (start + count).min(lines.len());
                let yanked: Vec<&str> = lines[start..end].iter().map(|s| s.as_str()).collect();
                yanked.join("\n")
            }
            // yw — yank from cursor to next word start
            KeyCode::Char('w') => {
                let saved = self.git.cursor_pos;
                for _ in 0..count {
                    self.move_word_forward(lines);
                }
                let end = self.git.cursor_pos;
                self.git.cursor_pos = saved;
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
                    self.copy_to_clipboard(&text);
                    return;
                }
                let adjusted_end = if end.row > saved.row {
                    let prev_line_len = self.line_len_at(lines, end.row.saturating_sub(1));
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
                self.extract_range(lines, saved, adjusted_end)
            }
            // ye — yank from cursor to end of word
            KeyCode::Char('e') => {
                let saved = self.git.cursor_pos;
                for _ in 0..count {
                    self.move_word_end(lines);
                }
                let end = self.git.cursor_pos;
                self.git.cursor_pos = saved;
                self.extract_range(lines, saved, end)
            }
            // yb — yank from previous word start to cursor
            KeyCode::Char('b') => {
                let saved = self.git.cursor_pos;
                for _ in 0..count {
                    self.move_word_backward(lines);
                }
                let start = self.git.cursor_pos;
                self.git.cursor_pos = saved;
                self.extract_range(lines, start, saved)
            }
            // y$ — yank to end of line
            KeyCode::Char('$') => {
                if let Some(line) = lines.get(self.git.cursor_pos.row) {
                    let chars: Vec<char> = line.chars().collect();
                    let col = self.git.cursor_pos.col.min(chars.len());
                    chars[col..].iter().collect()
                } else {
                    String::new()
                }
            }
            // y0 — yank to beginning of line
            KeyCode::Char('0') => {
                if let Some(line) = lines.get(self.git.cursor_pos.row) {
                    let chars: Vec<char> = line.chars().collect();
                    let col = self.git.cursor_pos.col.min(chars.len());
                    chars[..col].iter().collect()
                } else {
                    String::new()
                }
            }
            _ => return,
        };
        self.copy_to_clipboard(&text);
    }

    /// Extract text between two positions (inclusive)
    fn extract_range(&self, lines: &[String], start: CursorPos, end: CursorPos) -> String {
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

    pub(crate) fn handle_diff_visual_key(&mut self, key: KeyEvent) {
        // Handle pending key sequences
        if let Some(prefix) = self.git.pending_key {
            self.git.pending_key = None;
            match prefix {
                'i' | 'a' => {
                    let lines = self.content_lines();
                    self.apply_text_object(prefix, key.code, &lines);
                }
                'g' => {
                    let lines = self.content_lines();
                    if key.code == KeyCode::Char('g') {
                        if let Some(n) = self.git.count.take() {
                            self.git.cursor_pos.row =
                                (n.saturating_sub(1)).min(lines.len().saturating_sub(1));
                        } else {
                            self.git.cursor_pos.row = 0;
                        }
                        self.git.cursor_pos.col = 0;
                        self.clamp_col(&lines);
                    }
                    self.git.count = None;
                }
                _ => {}
            }
            self.scroll_to_cursor();
            return;
        }

        // Accumulate digit count prefix
        if let KeyCode::Char(c @ '1'..='9') = key.code {
            let digit = (c as usize) - ('0' as usize);
            self.git.count = Some(self.git.count.unwrap_or(0) * 10 + digit);
            return;
        }
        if let KeyCode::Char('0') = key.code {
            if self.git.count.is_some() {
                self.git.count = Some(self.git.count.unwrap() * 10);
                return;
            }
        }

        let n = self.take_count();
        let lines = self.content_lines();
        let total = lines.len();
        if total == 0 {
            return;
        }

        match key.code {
            KeyCode::Char('h') | KeyCode::Left => {
                self.git.cursor_pos.col = self.git.cursor_pos.col.saturating_sub(n);
            }
            KeyCode::Char('l') | KeyCode::Right => {
                let line_len = self.current_line_len(&lines);
                self.git.cursor_pos.col =
                    (self.git.cursor_pos.col + n).min(line_len.saturating_sub(1));
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.git.cursor_pos.row = (self.git.cursor_pos.row + n).min(total - 1);
                self.clamp_col(&lines);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.git.cursor_pos.row = self.git.cursor_pos.row.saturating_sub(n);
                self.clamp_col(&lines);
            }
            KeyCode::Char('w') => {
                for _ in 0..n {
                    self.move_word_forward(&lines);
                }
            }
            KeyCode::Char('b') => {
                for _ in 0..n {
                    self.move_word_backward(&lines);
                }
            }
            KeyCode::Char('e') => {
                for _ in 0..n {
                    self.move_word_end(&lines);
                }
            }
            KeyCode::Char('0') => {
                self.git.cursor_pos.col = 0;
            }
            KeyCode::Char('$') => {
                let line_len = self.current_line_len(&lines);
                self.git.cursor_pos.col = line_len.saturating_sub(1);
            }
            KeyCode::Char('g') => {
                self.git.pending_key = Some('g');
            }
            KeyCode::Char('G') => {
                if n > 1 {
                    self.git.cursor_pos.row = (n - 1).min(total - 1);
                } else {
                    self.git.cursor_pos.row = total - 1;
                }
                self.git.cursor_pos.col = 0;
                self.clamp_col(&lines);
            }
            KeyCode::Char('i') | KeyCode::Char('a') => {
                if let KeyCode::Char(c) = key.code {
                    self.git.pending_key = Some(c);
                }
            }
            KeyCode::Char('y') => {
                let text = self.yank_selection(&lines);
                self.copy_to_clipboard(&text);
                self.git.diff_view_mode = DiffViewMode::Normal;
                self.git.visual_anchor = None;
            }
            KeyCode::Char('v') => {
                if self.git.diff_view_mode == DiffViewMode::Visual {
                    self.git.diff_view_mode = DiffViewMode::Normal;
                    self.git.visual_anchor = None;
                } else {
                    self.git.diff_view_mode = DiffViewMode::Visual;
                    self.git.visual_anchor = Some(self.git.cursor_pos);
                }
            }
            KeyCode::Char('V') => {
                if self.git.diff_view_mode == DiffViewMode::VisualLine {
                    self.git.diff_view_mode = DiffViewMode::Normal;
                    self.git.visual_anchor = None;
                } else {
                    self.git.diff_view_mode = DiffViewMode::VisualLine;
                    self.git.visual_anchor = Some(self.git.cursor_pos);
                }
            }
            KeyCode::Char('/') => {
                self.git.search.start(SearchOrigin::DiffView);
                self.git.pending_key = None;
                self.git.count = None;
            }
            KeyCode::Char('n') => {
                self.jump_to_git_match(true);
            }
            KeyCode::Char('N') => {
                self.jump_to_git_match(false);
            }
            KeyCode::Esc => {
                self.git.diff_view_mode = DiffViewMode::Normal;
                self.git.visual_anchor = None;
                self.git.pending_key = None;
                self.git.count = None;
            }
            _ => {}
        }
        self.scroll_to_cursor();
    }

    pub(crate) fn copy_to_clipboard(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let line_count = text.lines().count().max(1);
        match arboard::Clipboard::new() {
            Ok(mut clip) => {
                if clip.set_text(text).is_ok() {
                    self.ctx.status_message = Some(format!(
                        "Yanked {line_count} line{}",
                        if line_count == 1 { "" } else { "s" }
                    ));
                } else {
                    self.ctx.status_message = Some("Clipboard error".to_string());
                }
            }
            Err(_) => {
                self.ctx.status_message = Some("Clipboard unavailable".to_string());
            }
        }
    }

    /// Build flat list of content strings for the current side of the diff.
    /// Results are cached and reused until the file or side changes.
    pub fn content_lines(&mut self) -> Vec<String> {
        let file = match self.git.selected_file() {
            Some(f) => f.clone(),
            None => return Vec::new(),
        };
        let side = self.git.cursor_pos.side;

        // Return cached result if still valid
        if let Some((ref path, cached_side, ref lines)) = self.git.content_lines_cache {
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
        self.git.content_lines_cache = Some((file.path.clone(), side, lines.clone()));
        lines
    }

    fn current_line_len(&self, lines: &[String]) -> usize {
        lines
            .get(self.git.cursor_pos.row)
            .map(|l| l.chars().count().max(1))
            .unwrap_or(1)
    }

    fn clamp_col(&mut self, lines: &[String]) {
        let len = self.current_line_len(lines);
        if self.git.cursor_pos.col >= len {
            self.git.cursor_pos.col = len.saturating_sub(1);
        }
    }

    pub(crate) fn scroll_to_cursor(&mut self) {
        let row = self.git.cursor_pos.row as u16;
        let height = self.git.diff_view_height;
        if height == 0 {
            return;
        }
        if row < self.git.diff_scroll_y {
            self.git.diff_scroll_y = row;
        } else if row >= self.git.diff_scroll_y + height {
            self.git.diff_scroll_y = row - height + 1;
        }
    }

    fn move_word_forward(&mut self, lines: &[String]) {
        let total = lines.len();
        if total == 0 {
            return;
        }
        let line: Vec<char> = lines[self.git.cursor_pos.row].chars().collect();
        let mut col = self.git.cursor_pos.col;
        let mut row = self.git.cursor_pos.row;

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
        self.git.cursor_pos.row = row;
        self.git.cursor_pos.col = col.min(self.line_len_at(lines, row).saturating_sub(1));
    }

    fn move_word_backward(&mut self, lines: &[String]) {
        if lines.is_empty() {
            return;
        }
        let line: Vec<char> = lines[self.git.cursor_pos.row].chars().collect();
        let mut col = self.git.cursor_pos.col;
        let mut row = self.git.cursor_pos.row;

        if col == 0 {
            if row > 0 {
                row -= 1;
                col = self.line_len_at(lines, row).saturating_sub(1);
            }
            self.git.cursor_pos.row = row;
            self.git.cursor_pos.col = col;
            return;
        }

        // Move back one
        col = col.saturating_sub(1);
        // Skip whitespace backward
        while col > 0 && line.get(col).map_or(false, |c| c.is_whitespace()) {
            col -= 1;
        }
        // Skip word chars backward
        while col > 0 && line.get(col - 1).map_or(false, |c| !c.is_whitespace()) {
            col -= 1;
        }
        self.git.cursor_pos.row = row;
        self.git.cursor_pos.col = col;
    }

    fn move_word_end(&mut self, lines: &[String]) {
        let total = lines.len();
        if total == 0 {
            return;
        }
        let line: Vec<char> = lines[self.git.cursor_pos.row].chars().collect();
        let mut col = self.git.cursor_pos.col;
        let mut row = self.git.cursor_pos.row;

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
        self.git.cursor_pos.row = row;
        self.git.cursor_pos.col = col.min(self.line_len_at(lines, row).saturating_sub(1));
    }

    fn line_len_at(&self, lines: &[String], row: usize) -> usize {
        lines.get(row).map(|l| l.chars().count().max(1)).unwrap_or(1)
    }

    fn yank_selection(&self, lines: &[String]) -> String {
        let anchor = match self.git.visual_anchor {
            Some(a) => a,
            None => return String::new(),
        };
        match self.git.diff_view_mode {
            DiffViewMode::VisualLine => {
                let start_row = anchor.row.min(self.git.cursor_pos.row);
                let end_row = anchor.row.max(self.git.cursor_pos.row);
                let mut result = Vec::new();
                for r in start_row..=end_row {
                    if let Some(line) = lines.get(r) {
                        result.push(line.as_str());
                    }
                }
                result.join("\n")
            }
            DiffViewMode::Visual => {
                let (start, end) = self.ordered_selection(anchor);
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

    fn ordered_selection(&self, anchor: CursorPos) -> (CursorPos, CursorPos) {
        if anchor.row < self.git.cursor_pos.row
            || (anchor.row == self.git.cursor_pos.row && anchor.col <= self.git.cursor_pos.col)
        {
            (anchor, self.git.cursor_pos)
        } else {
            (self.git.cursor_pos, anchor)
        }
    }

    fn apply_text_object(&mut self, prefix: char, key: KeyCode, lines: &[String]) {
        let inner = prefix == 'i';
        match key {
            KeyCode::Char('w') => self.select_text_object_word(inner, lines),
            KeyCode::Char('"') => self.select_text_object_delim(inner, '"', '"', lines),
            KeyCode::Char('\'') => self.select_text_object_delim(inner, '\'', '\'', lines),
            KeyCode::Char('(') | KeyCode::Char(')') => {
                self.select_text_object_delim(inner, '(', ')', lines);
            }
            KeyCode::Char('{') | KeyCode::Char('}') => {
                self.select_text_object_delim(inner, '{', '}', lines);
            }
            _ => {}
        }
    }

    fn select_text_object_word(&mut self, inner: bool, lines: &[String]) {
        if let Some(line) = lines.get(self.git.cursor_pos.row) {
            let chars: Vec<char> = line.chars().collect();
            let col = self.git.cursor_pos.col.min(chars.len().saturating_sub(1));
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
            self.git.visual_anchor = Some(CursorPos {
                row: self.git.cursor_pos.row,
                col: start,
                side: self.git.cursor_pos.side,
            });
            self.git.cursor_pos.col = end;
        }
    }

    fn select_text_object_delim(
        &mut self,
        inner: bool,
        open: char,
        close: char,
        lines: &[String],
    ) {
        if let Some(line) = lines.get(self.git.cursor_pos.row) {
            let chars: Vec<char> = line.chars().collect();
            let col = self.git.cursor_pos.col.min(chars.len().saturating_sub(1));
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
            for i in (col + 1)..chars.len() {
                if chars[i] == close {
                    close_pos = Some(i);
                    break;
                }
            }
            if let (Some(op), Some(cp)) = (open_pos, close_pos) {
                if inner {
                    self.git.visual_anchor = Some(CursorPos {
                        row: self.git.cursor_pos.row,
                        col: op + 1,
                        side: self.git.cursor_pos.side,
                    });
                    self.git.cursor_pos.col = cp.saturating_sub(1);
                } else {
                    self.git.visual_anchor = Some(CursorPos {
                        row: self.git.cursor_pos.row,
                        col: op,
                        side: self.git.cursor_pos.side,
                    });
                    self.git.cursor_pos.col = cp;
                }
            }
        }
    }

    /// Re-execute DiffView search when file selection changes (preserves query)
    pub(crate) fn re_search_on_file_change(&mut self) {
        if self.git.search.origin == SearchOrigin::DiffView && self.git.search.query.is_some() {
            self.git.search.reset_matches();
            self.git.content_lines_cache = None;
            let query = self.git.search.query.clone().unwrap();
            self.search_git_diff_view(&query);
        }
    }
}
