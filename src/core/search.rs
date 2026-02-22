use crate::git::state::DiffSide;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// SearchOrigin is the pane index where the search was initiated.
pub type SearchOrigin = usize;

#[derive(Debug, Clone)]
pub enum SearchMatch {
    /// Matches in a list-based pane (file tree, branch list, commit log, reflog, etc.)
    ListEntry(usize),
    /// Matches in the diff view
    DiffLine {
        row: usize,
        col_start: usize,
        col_end: usize,
        side: DiffSide,
    },
}

#[derive(Debug, Clone)]
pub struct SearchState {
    pub active: bool,
    pub input: String,
    pub query: Option<String>,
    pub origin: SearchOrigin,
    pub matches: Vec<SearchMatch>,
    pub current_match_idx: Option<usize>,
    /// Last confirmed query — preserved across clear() for n/N reuse
    pub last_query: Option<String>,
    /// Search history (oldest first)
    pub history: Vec<String>,
    /// Current position in history during input (None = editing new input)
    pub(crate) history_idx: Option<usize>,
    /// Saved input before browsing history
    saved_input: String,
}

impl SearchState {
    pub fn new() -> Self {
        Self {
            active: false,
            input: String::new(),
            query: None,
            origin: 0,
            matches: Vec::new(),
            current_match_idx: None,
            last_query: None,
            history: Vec::new(),
            history_idx: None,
            saved_input: String::new(),
        }
    }

    pub fn start(&mut self, origin: SearchOrigin) {
        self.active = true;
        self.input.clear();
        self.query = None;
        self.origin = origin;
        self.matches.clear();
        self.current_match_idx = None;
        self.history_idx = None;
        self.saved_input.clear();
    }

    pub fn reset_matches(&mut self) {
        self.matches.clear();
        self.current_match_idx = None;
    }

    /// Clear highlights but preserve last_query and history for n/N reuse
    pub fn clear(&mut self) {
        self.active = false;
        self.input.clear();
        if self.query.is_some() {
            self.last_query = self.query.take();
        }
        self.matches.clear();
        self.current_match_idx = None;
    }

    /// Handle a key event during search input mode.
    /// Returns `true` if the user confirmed a search query (Enter), signaling
    /// the caller to execute the search and jump to the first match.
    pub fn handle_input_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Enter => {
                let query = self.input.clone();
                if query.is_empty() {
                    self.active = false;
                    return false;
                }
                self.push_history(&query);
                self.active = false;
                self.query = Some(query);
                true
            }
            KeyCode::Esc => {
                self.active = false;
                self.input.clear();
                false
            }
            KeyCode::Backspace => {
                self.input.pop();
                self.history_idx = None;
                false
            }
            KeyCode::Up | KeyCode::Char('p')
                if key.code == KeyCode::Up || key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                self.history_prev();
                false
            }
            KeyCode::Down | KeyCode::Char('n')
                if key.code == KeyCode::Down || key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                self.history_next();
                false
            }
            KeyCode::Char(c) => {
                self.input.push(c);
                self.history_idx = None;
                false
            }
            _ => false,
        }
    }

    /// Navigate to previous history entry
    pub fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        match self.history_idx {
            None => {
                // Save current input, jump to most recent history
                self.saved_input = self.input.clone();
                let idx = self.history.len() - 1;
                self.history_idx = Some(idx);
                self.input = self.history[idx].clone();
            }
            Some(idx) if idx > 0 => {
                let new_idx = idx - 1;
                self.history_idx = Some(new_idx);
                self.input = self.history[new_idx].clone();
            }
            _ => {}
        }
    }

    /// Navigate to next history entry (or back to saved input)
    pub fn history_next(&mut self) {
        if let Some(idx) = self.history_idx {
            if idx + 1 < self.history.len() {
                let new_idx = idx + 1;
                self.history_idx = Some(new_idx);
                self.input = self.history[new_idx].clone();
            } else {
                // Back to the input the user was typing
                self.history_idx = None;
                self.input = self.saved_input.clone();
            }
        }
    }

    /// Add query to history (deduplicates consecutive)
    pub fn push_history(&mut self, query: &str) {
        if query.is_empty() {
            return;
        }
        if self.history.last().map(|s| s.as_str()) != Some(query) {
            self.history.push(query.to_string());
        }
    }
}
