use crate::git::diff::{DiffState, FileDiff};
use crate::git::repository::Repo;
use crate::syntax::{HighlightCache, SyntaxHighlighter};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::Color;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusedPane {
    FileTree,
    DiffView,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffViewMode {
    Scroll,
    Normal,
    Visual,
    VisualLine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffSide {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorPos {
    pub row: usize,
    pub col: usize,
    pub side: DiffSide,
}

#[derive(Debug, Clone)]
pub enum TreeEntry {
    Dir {
        path: String,
        depth: usize,
        collapsed: bool,
    },
    File {
        file_idx: usize,
        depth: usize,
    },
}

pub struct App {
    pub should_quit: bool,
    pub repo: Repo,
    pub diff_state: DiffState,
    pub collapsed_dirs: HashSet<String>,
    pub selected_tree_idx: usize,
    pub focused_pane: FocusedPane,
    pub diff_scroll_y: u16,
    pub diff_scroll_x: u16,
    pub diff_total_lines: u16,
    pub diff_view_height: u16,
    pub show_help: bool,
    pub status_message: Option<String>,
    pub diff_view_mode: DiffViewMode,
    pub cursor_pos: CursorPos,
    pub visual_anchor: Option<CursorPos>,
    pub pending_key: Option<char>,
    pub count: Option<usize>,
    pub highlighter: SyntaxHighlighter,
    pub highlight_cache: Option<HighlightCache>,
    /// Cached content_lines result: (file_path, side, lines). Invalidated on file/side switch.
    content_lines_cache: Option<(String, DiffSide, Vec<String>)>,
    /// Pre-computed highlight results from background thread, keyed by file path.
    bg_highlights: HashMap<String, (Vec<Vec<Color>>, Vec<Vec<Color>>)>,
    /// Receiver for background highlight results.
    bg_highlight_rx: Option<mpsc::Receiver<(String, Vec<Vec<Color>>, Vec<Vec<Color>>)>>,
}

impl App {
    pub fn new(repo: Repo) -> Result<Self> {
        let diff_state = repo.diff_workdir()?;
        let mut app = Self {
            should_quit: false,
            repo,
            diff_state,
            collapsed_dirs: HashSet::new(),
            selected_tree_idx: 0,
            focused_pane: FocusedPane::FileTree,
            diff_scroll_y: 0,
            diff_scroll_x: 0,
            diff_total_lines: 0,
            diff_view_height: 0,
            show_help: false,
            status_message: None,
            diff_view_mode: DiffViewMode::Scroll,
            cursor_pos: CursorPos { row: 0, col: 0, side: DiffSide::Left },
            visual_anchor: None,
            pending_key: None,
            count: None,
            highlighter: SyntaxHighlighter::new(),
            highlight_cache: None,
            content_lines_cache: None,
            bg_highlights: HashMap::new(),
            bg_highlight_rx: None,
        };
        app.spawn_bg_highlight();
        Ok(app)
    }

    pub fn selected_file(&self) -> Option<&FileDiff> {
        let entries = self.build_tree_entries();
        if let Some(TreeEntry::File { file_idx, .. }) = entries.get(self.selected_tree_idx) {
            self.diff_state.files.get(*file_idx)
        } else {
            None
        }
    }

    /// Ensure syntax highlighting is available up to `up_to` rows for the given file.
    /// Uses pre-computed background results if available, otherwise falls back to on-demand.
    pub fn ensure_file_highlight(&mut self, file: &FileDiff, up_to: usize) {
        let needs_init = self
            .highlight_cache
            .as_ref()
            .map(|c| c.file_path != file.path)
            .unwrap_or(true);

        if needs_init {
            // Check for pre-computed background highlight results first
            if let Some((lc, rc)) = self.bg_highlights.remove(&file.path) {
                self.highlight_cache =
                    Some(HighlightCache::from_precomputed(file.path.clone(), lc, rc));
                return;
            }

            // Fall back to on-demand highlighting
            let mut left_lines = Vec::new();
            let mut right_lines = Vec::new();
            let mut hunk_starts = Vec::new();
            for hunk in &file.hunks {
                hunk_starts.push(left_lines.len());
                left_lines.push(String::new());
                right_lines.push(String::new());
                for row in &hunk.rows {
                    left_lines.push(
                        row.left.as_ref().map(|s| s.content.clone()).unwrap_or_default(),
                    );
                    right_lines.push(
                        row.right.as_ref().map(|s| s.content.clone()).unwrap_or_default(),
                    );
                }
            }
            self.highlight_cache =
                self.highlighter
                    .create_cache(&file.path, left_lines, right_lines, hunk_starts);
        }

        if let Some(ref mut cache) = self.highlight_cache {
            self.highlighter.extend_cache(cache, up_to);
        }
    }

    pub fn refresh_diff(&mut self) -> Result<()> {
        let old_path = self.selected_file().map(|f| f.path.clone());
        self.diff_state = self.repo.diff_workdir()?;
        // Preserve selection by path
        if let Some(path) = old_path {
            let entries = self.build_tree_entries();
            self.selected_tree_idx = entries
                .iter()
                .position(|e| matches!(e, TreeEntry::File { file_idx, .. } if self.diff_state.files.get(*file_idx).map(|f| &f.path) == Some(&path)))
                .unwrap_or(0);
        }
        let entries = self.build_tree_entries();
        if self.selected_tree_idx >= entries.len() && !entries.is_empty() {
            self.selected_tree_idx = entries.len() - 1;
        }
        self.diff_scroll_y = 0;
        self.diff_scroll_x = 0;
        self.status_message = None;
        self.highlight_cache = None;
        self.content_lines_cache = None;
        self.bg_highlights.clear();
        self.bg_highlight_rx = None; // Drop old receiver, stops old thread
        self.spawn_bg_highlight();
        Ok(())
    }

    /// Spawn a background thread to pre-highlight all files.
    fn spawn_bg_highlight(&mut self) {
        let mut file_data: Vec<(String, Vec<String>, Vec<String>, Vec<usize>)> = Vec::new();
        for file in &self.diff_state.files {
            if file.is_binary {
                continue;
            }
            let mut left_lines = Vec::new();
            let mut right_lines = Vec::new();
            let mut hunk_starts = Vec::new();
            for hunk in &file.hunks {
                hunk_starts.push(left_lines.len());
                left_lines.push(String::new());
                right_lines.push(String::new());
                for row in &hunk.rows {
                    left_lines.push(
                        row.left.as_ref().map(|s| s.content.clone()).unwrap_or_default(),
                    );
                    right_lines.push(
                        row.right.as_ref().map(|s| s.content.clone()).unwrap_or_default(),
                    );
                }
            }
            file_data.push((file.path.clone(), left_lines, right_lines, hunk_starts));
        }

        if file_data.is_empty() {
            return;
        }

        let (tx, rx) = mpsc::channel();
        self.bg_highlight_rx = Some(rx);

        std::thread::spawn(move || {
            let highlighter = SyntaxHighlighter::new();
            for (path, left_lines, right_lines, hunk_starts) in file_data {
                if let Some((lc, rc)) = highlighter.highlight_all_lines(
                    &path, &left_lines, &right_lines, &hunk_starts,
                ) {
                    if tx.send((path, lc, rc)).is_err() {
                        break; // Receiver dropped
                    }
                }
            }
        });
    }

    /// Drain completed background highlight results into the local cache.
    pub fn drain_bg_highlights(&mut self) {
        if let Some(ref rx) = self.bg_highlight_rx {
            while let Ok((path, left, right)) = rx.try_recv() {
                self.bg_highlights.insert(path, (left, right));
            }
        }
    }

    pub fn build_tree_entries(&self) -> Vec<TreeEntry> {
        let files = &self.diff_state.files;
        if files.is_empty() {
            return Vec::new();
        }

        // Count files per directory to detect single-file directories
        let mut dir_file_count: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for file in files {
            let parts: Vec<&str> = file.path.rsplitn(2, '/').collect();
            if parts.len() == 2 {
                // Has a directory component
                let dir = parts[1];
                // Count for this dir and all ancestor dirs
                let mut current = String::new();
                for segment in dir.split('/') {
                    if !current.is_empty() {
                        current.push('/');
                    }
                    current.push_str(segment);
                    *dir_file_count.entry(current.clone()).or_insert(0) += 1;
                }
            }
        }

        let mut entries = Vec::new();
        let mut prev_dir_parts: Vec<&str> = Vec::new();

        for (file_idx, file) in files.iter().enumerate() {
            let parts: Vec<&str> = file.path.rsplitn(2, '/').collect();
            if parts.len() == 2 {
                let dir = parts[1];
                let dir_parts: Vec<&str> = dir.split('/').collect();

                // Check if the entire path from root is single-file at every level
                // If so, inline the file (show full path, no directory node)
                let leaf_dir = dir.to_string();
                if dir_file_count.get(&leaf_dir).copied().unwrap_or(0) == 1 {
                    // Single file in this directory — inline with full path at depth 0
                    entries.push(TreeEntry::File {
                        file_idx,
                        depth: 0,
                    });
                    // Don't update prev_dir_parts since we inlined
                    prev_dir_parts = Vec::new();
                    continue;
                }

                // Find common prefix with previous directory
                let common_len = prev_dir_parts
                    .iter()
                    .zip(dir_parts.iter())
                    .take_while(|(a, b)| a == b)
                    .count();

                // Emit new directory entries for parts beyond common prefix
                let mut collapsed_ancestor = false;
                for i in common_len..dir_parts.len() {
                    let dir_path: String = dir_parts[..=i].join("/");
                    let is_collapsed = self.collapsed_dirs.contains(&dir_path);
                    if !collapsed_ancestor {
                        entries.push(TreeEntry::Dir {
                            path: dir_path.clone(),
                            depth: i,
                            collapsed: is_collapsed,
                        });
                    }
                    if is_collapsed {
                        collapsed_ancestor = true;
                    }
                }

                // Check if any ancestor dir is collapsed
                let mut skip_file = false;
                let mut check_path = String::new();
                for part in &dir_parts {
                    if !check_path.is_empty() {
                        check_path.push('/');
                    }
                    check_path.push_str(part);
                    if self.collapsed_dirs.contains(&check_path) {
                        skip_file = true;
                        break;
                    }
                }

                if !skip_file {
                    entries.push(TreeEntry::File {
                        file_idx,
                        depth: dir_parts.len(),
                    });
                }

                prev_dir_parts = dir_parts;
            } else {
                // Root-level file (no directory component)
                prev_dir_parts = Vec::new();
                entries.push(TreeEntry::File {
                    file_idx,
                    depth: 0,
                });
            }
        }

        entries
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Result<bool> {
        if self.show_help {
            self.show_help = false;
            return Ok(false);
        }

        // Ctrl+c always quits
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return Ok(false);
        }

        // In Normal/Visual modes, keys are handled by the mode handler exclusively
        if self.focused_pane == FocusedPane::DiffView
            && self.diff_view_mode != DiffViewMode::Scroll
        {
            self.handle_diff_view_key(key);
            return Ok(false);
        }

        match key.code {
            KeyCode::Char('q') => {
                self.should_quit = true;
                return Ok(false);
            }
            KeyCode::Char('?') => {
                self.show_help = true;
            }
            KeyCode::Char('r') => {
                self.refresh_diff()?;
            }
            KeyCode::Char('e') => {
                return Ok(true); // Signal to open editor
            }
            KeyCode::Tab | KeyCode::BackTab => {
                self.focused_pane = match self.focused_pane {
                    FocusedPane::FileTree => FocusedPane::DiffView,
                    FocusedPane::DiffView => FocusedPane::FileTree,
                };
            }
            _ => match self.focused_pane {
                FocusedPane::FileTree => self.handle_file_tree_key(key),
                FocusedPane::DiffView => self.handle_diff_view_key(key),
            },
        }
        Ok(false)
    }

    fn handle_file_tree_key(&mut self, key: KeyEvent) {
        let entries = self.build_tree_entries();
        if entries.is_empty() {
            return;
        }
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if self.selected_tree_idx + 1 < entries.len() {
                    self.selected_tree_idx += 1;
                    self.diff_scroll_y = 0;
                    self.diff_scroll_x = 0;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.selected_tree_idx > 0 {
                    self.selected_tree_idx -= 1;
                    self.diff_scroll_y = 0;
                    self.diff_scroll_x = 0;
                }
            }
            KeyCode::Char(' ') => {
                if let Some(TreeEntry::Dir { path, .. }) = entries.get(self.selected_tree_idx) {
                    let path = path.clone();
                    if self.collapsed_dirs.contains(&path) {
                        self.collapsed_dirs.remove(&path);
                    } else {
                        self.collapsed_dirs.insert(path);
                    }
                }
            }
            KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter => {
                match entries.get(self.selected_tree_idx) {
                    Some(TreeEntry::Dir { path, .. }) => {
                        let path = path.clone();
                        if self.collapsed_dirs.contains(&path) {
                            self.collapsed_dirs.remove(&path);
                        } else {
                            self.collapsed_dirs.insert(path);
                        }
                    }
                    Some(TreeEntry::File { .. }) => {
                        self.focused_pane = FocusedPane::DiffView;
                        self.diff_scroll_y = 0;
                        self.diff_scroll_x = 0;
                    }
                    None => {}
                }
            }
            _ => {}
        }
    }

    fn handle_diff_view_key(&mut self, key: KeyEvent) {
        match self.diff_view_mode {
            DiffViewMode::Scroll => self.handle_diff_scroll_key(key),
            DiffViewMode::Normal => self.handle_diff_normal_key(key),
            DiffViewMode::Visual | DiffViewMode::VisualLine => self.handle_diff_visual_key(key),
        }
    }

    fn handle_diff_scroll_key(&mut self, key: KeyEvent) {
        let max_scroll = self.diff_total_lines.saturating_sub(self.diff_view_height);
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.diff_scroll_y = (self.diff_scroll_y + 1).min(max_scroll);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.diff_scroll_y = self.diff_scroll_y.saturating_sub(1);
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = self.diff_view_height / 2;
                self.diff_scroll_y = (self.diff_scroll_y + half).min(max_scroll);
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = self.diff_view_height / 2;
                self.diff_scroll_y = self.diff_scroll_y.saturating_sub(half);
            }
            KeyCode::Char('g') => {
                self.diff_scroll_y = 0;
            }
            KeyCode::Char('G') => {
                self.diff_scroll_y = max_scroll;
            }
            KeyCode::Char('h') | KeyCode::Left => {
                self.diff_scroll_x = self.diff_scroll_x.saturating_sub(4);
            }
            KeyCode::Esc => {
                self.focused_pane = FocusedPane::FileTree;
            }
            KeyCode::Char('l') | KeyCode::Right => {
                self.diff_scroll_x = self.diff_scroll_x.saturating_add(4);
            }
            KeyCode::Char('i') => {
                // Enter Normal mode with cursor at top-left of visible area
                let lines = self.content_lines();
                if !lines.is_empty() {
                    self.diff_view_mode = DiffViewMode::Normal;
                    self.cursor_pos = CursorPos {
                        row: self.diff_scroll_y as usize,
                        col: 0,
                        side: DiffSide::Left,
                    };
                }
            }
            _ => {}
        }
    }

    fn handle_diff_normal_key(&mut self, key: KeyEvent) {
        // Handle Ctrl+w prefix for panel switching
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('w') {
            self.pending_key = Some('w');
            return;
        }

        // Handle pending key sequences
        if let Some(pending) = self.pending_key {
            self.pending_key = None;
            match pending {
                'w' => {
                    match key.code {
                        KeyCode::Char('h') => self.cursor_pos.side = DiffSide::Left,
                        KeyCode::Char('l') => self.cursor_pos.side = DiffSide::Right,
                        _ => {}
                    }
                    self.count = None;
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
                            if let Some(n) = self.count.take() {
                                self.cursor_pos.row = (n.saturating_sub(1)).min(lines.len().saturating_sub(1));
                            } else {
                                self.cursor_pos.row = 0;
                            }
                            self.cursor_pos.col = 0;
                            self.clamp_col(&lines);
                        }
                        _ => {}
                    }
                    self.count = None;
                    self.scroll_to_cursor();
                    return;
                }
                _ => {}
            }
            self.count = None;
            return;
        }

        // Accumulate digit count prefix (1-9 start, 0 appends)
        if let KeyCode::Char(c @ '1'..='9') = key.code {
            let digit = (c as usize) - ('0' as usize);
            self.count = Some(self.count.unwrap_or(0) * 10 + digit);
            return;
        }
        if let KeyCode::Char('0') = key.code {
            if self.count.is_some() {
                self.count = Some(self.count.unwrap() * 10);
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
                self.cursor_pos.col = self.cursor_pos.col.saturating_sub(n);
            }
            KeyCode::Char('l') | KeyCode::Right => {
                let line_len = self.current_line_len(&lines);
                self.cursor_pos.col = (self.cursor_pos.col + n).min(line_len.saturating_sub(1));
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.cursor_pos.row = (self.cursor_pos.row + n).min(total - 1);
                self.clamp_col(&lines);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.cursor_pos.row = self.cursor_pos.row.saturating_sub(n);
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
                self.cursor_pos.col = 0;
            }
            KeyCode::Char('$') => {
                let line_len = self.current_line_len(&lines);
                self.cursor_pos.col = line_len.saturating_sub(1);
            }
            KeyCode::Char('g') => {
                self.pending_key = Some('g');
            }
            KeyCode::Char('G') => {
                // G or {count}G — go to last line or specific line
                // Note: count was already consumed, but if n > 1, user typed {n}G
                if n > 1 {
                    self.cursor_pos.row = (n - 1).min(total - 1);
                } else {
                    self.cursor_pos.row = total - 1;
                }
                self.cursor_pos.col = 0;
                self.clamp_col(&lines);
            }
            KeyCode::Char('y') => {
                self.pending_key = Some('y');
            }
            KeyCode::Char('v') => {
                self.diff_view_mode = DiffViewMode::Visual;
                self.visual_anchor = Some(self.cursor_pos);
            }
            KeyCode::Char('V') => {
                self.diff_view_mode = DiffViewMode::VisualLine;
                self.visual_anchor = Some(self.cursor_pos);
            }
            KeyCode::Esc => {
                self.diff_view_mode = DiffViewMode::Scroll;
                self.pending_key = None;
                self.count = None;
            }
            _ => {}
        }
        self.scroll_to_cursor();
    }

    fn take_count(&mut self) -> usize {
        self.count.take().unwrap_or(1)
    }

    /// Execute y + motion (yy, yw, y$, y0, yb, ye) with count
    fn execute_yank_motion(&mut self, motion: KeyCode, lines: &[String], count: usize) {
        let text = match motion {
            // yy or {n}yy — yank current line(s)
            KeyCode::Char('y') => {
                let start = self.cursor_pos.row;
                let end = (start + count).min(lines.len());
                let yanked: Vec<&str> = lines[start..end].iter().map(|s| s.as_str()).collect();
                yanked.join("\n")
            }
            // yw — yank from cursor to next word start
            KeyCode::Char('w') => {
                let saved = self.cursor_pos;
                for _ in 0..count {
                    self.move_word_forward(lines);
                }
                let end = self.cursor_pos;
                self.cursor_pos = saved;
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
                let saved = self.cursor_pos;
                for _ in 0..count {
                    self.move_word_end(lines);
                }
                let end = self.cursor_pos;
                self.cursor_pos = saved;
                self.extract_range(lines, saved, end)
            }
            // yb — yank from previous word start to cursor
            KeyCode::Char('b') => {
                let saved = self.cursor_pos;
                for _ in 0..count {
                    self.move_word_backward(lines);
                }
                let start = self.cursor_pos;
                self.cursor_pos = saved;
                self.extract_range(lines, start, saved)
            }
            // y$ — yank to end of line
            KeyCode::Char('$') => {
                if let Some(line) = lines.get(self.cursor_pos.row) {
                    let chars: Vec<char> = line.chars().collect();
                    let col = self.cursor_pos.col.min(chars.len());
                    chars[col..].iter().collect()
                } else {
                    String::new()
                }
            }
            // y0 — yank to beginning of line
            KeyCode::Char('0') => {
                if let Some(line) = lines.get(self.cursor_pos.row) {
                    let chars: Vec<char> = line.chars().collect();
                    let col = self.cursor_pos.col.min(chars.len());
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

    fn handle_diff_visual_key(&mut self, key: KeyEvent) {
        // Handle pending key sequences
        if let Some(prefix) = self.pending_key {
            self.pending_key = None;
            match prefix {
                'i' | 'a' => {
                    let lines = self.content_lines();
                    self.apply_text_object(prefix, key.code, &lines);
                }
                'g' => {
                    let lines = self.content_lines();
                    if key.code == KeyCode::Char('g') {
                        if let Some(n) = self.count.take() {
                            self.cursor_pos.row = (n.saturating_sub(1)).min(lines.len().saturating_sub(1));
                        } else {
                            self.cursor_pos.row = 0;
                        }
                        self.cursor_pos.col = 0;
                        self.clamp_col(&lines);
                    }
                    self.count = None;
                }
                _ => {}
            }
            self.scroll_to_cursor();
            return;
        }

        // Accumulate digit count prefix
        if let KeyCode::Char(c @ '1'..='9') = key.code {
            let digit = (c as usize) - ('0' as usize);
            self.count = Some(self.count.unwrap_or(0) * 10 + digit);
            return;
        }
        if let KeyCode::Char('0') = key.code {
            if self.count.is_some() {
                self.count = Some(self.count.unwrap() * 10);
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
                self.cursor_pos.col = self.cursor_pos.col.saturating_sub(n);
            }
            KeyCode::Char('l') | KeyCode::Right => {
                let line_len = self.current_line_len(&lines);
                self.cursor_pos.col = (self.cursor_pos.col + n).min(line_len.saturating_sub(1));
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.cursor_pos.row = (self.cursor_pos.row + n).min(total - 1);
                self.clamp_col(&lines);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.cursor_pos.row = self.cursor_pos.row.saturating_sub(n);
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
                self.cursor_pos.col = 0;
            }
            KeyCode::Char('$') => {
                let line_len = self.current_line_len(&lines);
                self.cursor_pos.col = line_len.saturating_sub(1);
            }
            KeyCode::Char('g') => {
                self.pending_key = Some('g');
            }
            KeyCode::Char('G') => {
                if n > 1 {
                    self.cursor_pos.row = (n - 1).min(total - 1);
                } else {
                    self.cursor_pos.row = total - 1;
                }
                self.cursor_pos.col = 0;
                self.clamp_col(&lines);
            }
            KeyCode::Char('i') | KeyCode::Char('a') => {
                if let KeyCode::Char(c) = key.code {
                    self.pending_key = Some(c);
                }
            }
            KeyCode::Char('y') => {
                let text = self.yank_selection(&lines);
                self.copy_to_clipboard(&text);
                self.diff_view_mode = DiffViewMode::Normal;
                self.visual_anchor = None;
            }
            KeyCode::Char('v') => {
                if self.diff_view_mode == DiffViewMode::Visual {
                    self.diff_view_mode = DiffViewMode::Normal;
                    self.visual_anchor = None;
                } else {
                    self.diff_view_mode = DiffViewMode::Visual;
                    self.visual_anchor = Some(self.cursor_pos);
                }
            }
            KeyCode::Char('V') => {
                if self.diff_view_mode == DiffViewMode::VisualLine {
                    self.diff_view_mode = DiffViewMode::Normal;
                    self.visual_anchor = None;
                } else {
                    self.diff_view_mode = DiffViewMode::VisualLine;
                    self.visual_anchor = Some(self.cursor_pos);
                }
            }
            KeyCode::Esc => {
                self.diff_view_mode = DiffViewMode::Normal;
                self.visual_anchor = None;
                self.pending_key = None;
                self.count = None;
            }
            _ => {}
        }
        self.scroll_to_cursor();
    }

    fn copy_to_clipboard(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let line_count = text.lines().count().max(1);
        match arboard::Clipboard::new() {
            Ok(mut clip) => {
                if clip.set_text(text).is_ok() {
                    self.status_message = Some(format!(
                        "Yanked {line_count} line{}",
                        if line_count == 1 { "" } else { "s" }
                    ));
                } else {
                    self.status_message = Some("Clipboard error".to_string());
                }
            }
            Err(_) => {
                self.status_message = Some("Clipboard unavailable".to_string());
            }
        }
    }

    /// Build flat list of content strings for the current side of the diff.
    /// Results are cached and reused until the file or side changes.
    pub fn content_lines(&mut self) -> Vec<String> {
        let file = match self.selected_file() {
            Some(f) => f.clone(),
            None => return Vec::new(),
        };
        let side = self.cursor_pos.side;

        // Return cached result if still valid
        if let Some((ref path, cached_side, ref lines)) = self.content_lines_cache {
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
        self.content_lines_cache = Some((file.path.clone(), side, lines.clone()));
        lines
    }

    fn current_line_len(&self, lines: &[String]) -> usize {
        lines
            .get(self.cursor_pos.row)
            .map(|l| l.chars().count().max(1))
            .unwrap_or(1)
    }

    fn clamp_col(&mut self, lines: &[String]) {
        let len = self.current_line_len(lines);
        if self.cursor_pos.col >= len {
            self.cursor_pos.col = len.saturating_sub(1);
        }
    }

    fn scroll_to_cursor(&mut self) {
        let row = self.cursor_pos.row as u16;
        let height = self.diff_view_height;
        if height == 0 {
            return;
        }
        if row < self.diff_scroll_y {
            self.diff_scroll_y = row;
        } else if row >= self.diff_scroll_y + height {
            self.diff_scroll_y = row - height + 1;
        }
    }

    fn move_word_forward(&mut self, lines: &[String]) {
        let total = lines.len();
        if total == 0 {
            return;
        }
        let line: Vec<char> = lines[self.cursor_pos.row].chars().collect();
        let mut col = self.cursor_pos.col;
        let mut row = self.cursor_pos.row;

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
        self.cursor_pos.row = row;
        self.cursor_pos.col = col.min(self.line_len_at(lines, row).saturating_sub(1));
    }

    fn move_word_backward(&mut self, lines: &[String]) {
        if lines.is_empty() {
            return;
        }
        let line: Vec<char> = lines[self.cursor_pos.row].chars().collect();
        let mut col = self.cursor_pos.col;
        let mut row = self.cursor_pos.row;

        if col == 0 {
            if row > 0 {
                row -= 1;
                col = self.line_len_at(lines, row).saturating_sub(1);
            }
            self.cursor_pos.row = row;
            self.cursor_pos.col = col;
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
        self.cursor_pos.row = row;
        self.cursor_pos.col = col;
    }

    fn move_word_end(&mut self, lines: &[String]) {
        let total = lines.len();
        if total == 0 {
            return;
        }
        let line: Vec<char> = lines[self.cursor_pos.row].chars().collect();
        let mut col = self.cursor_pos.col;
        let mut row = self.cursor_pos.row;

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
        self.cursor_pos.row = row;
        self.cursor_pos.col = col.min(self.line_len_at(lines, row).saturating_sub(1));
    }

    fn line_len_at(&self, lines: &[String], row: usize) -> usize {
        lines.get(row).map(|l| l.chars().count().max(1)).unwrap_or(1)
    }

    fn yank_selection(&self, lines: &[String]) -> String {
        let anchor = match self.visual_anchor {
            Some(a) => a,
            None => return String::new(),
        };
        match self.diff_view_mode {
            DiffViewMode::VisualLine => {
                let start_row = anchor.row.min(self.cursor_pos.row);
                let end_row = anchor.row.max(self.cursor_pos.row);
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
        if anchor.row < self.cursor_pos.row
            || (anchor.row == self.cursor_pos.row && anchor.col <= self.cursor_pos.col)
        {
            (anchor, self.cursor_pos)
        } else {
            (self.cursor_pos, anchor)
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
        if let Some(line) = lines.get(self.cursor_pos.row) {
            let chars: Vec<char> = line.chars().collect();
            let col = self.cursor_pos.col.min(chars.len().saturating_sub(1));
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
            self.visual_anchor = Some(CursorPos { row: self.cursor_pos.row, col: start, side: self.cursor_pos.side });
            self.cursor_pos.col = end;
        }
    }

    fn select_text_object_delim(&mut self, inner: bool, open: char, close: char, lines: &[String]) {
        if let Some(line) = lines.get(self.cursor_pos.row) {
            let chars: Vec<char> = line.chars().collect();
            let col = self.cursor_pos.col.min(chars.len().saturating_sub(1));
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
                    self.visual_anchor = Some(CursorPos { row: self.cursor_pos.row, col: op + 1, side: self.cursor_pos.side });
                    self.cursor_pos.col = cp.saturating_sub(1);
                } else {
                    self.visual_anchor = Some(CursorPos { row: self.cursor_pos.row, col: op, side: self.cursor_pos.side });
                    self.cursor_pos.col = cp;
                }
            }
        }
    }
}
