use crate::core::syntax::{HighlightCache, HighlightPair, SyntaxHighlighter};
use std::collections::HashMap;
use std::sync::mpsc;

pub struct HighlightState {
    pub highlighter: SyntaxHighlighter,
    /// Theme name, kept so background threads can build an equivalent highlighter.
    theme_name: String,
    pub cache: Option<HighlightCache>,
    pub(crate) bg_highlights: HashMap<String, HighlightPair>,
    pub(crate) bg_highlight_rx: Option<mpsc::Receiver<(String, HighlightPair)>>,
}

impl HighlightState {
    /// `theme_name` must be one of [`crate::core::syntax::theme_names`]; the
    /// config loader validates it, so an unknown name here falls back to the
    /// default theme rather than failing.
    pub fn new(theme_name: &str) -> Self {
        let highlighter = SyntaxHighlighter::with_theme(theme_name).unwrap_or_default();
        Self {
            highlighter,
            theme_name: theme_name.to_string(),
            cache: None,
            bg_highlights: HashMap::new(),
            bg_highlight_rx: None,
        }
    }

    pub fn reset(&mut self) {
        self.cache = None;
        self.bg_highlights.clear();
        self.bg_highlight_rx = None;
    }

    /// Ensure syntax highlighting is available up to `up_to` rows for the given file.
    /// Uses pre-computed background results if available, otherwise falls back to on-demand.
    pub fn ensure_file_highlight(
        &mut self,
        path: &str,
        left_lines: Vec<String>,
        right_lines: Vec<String>,
        hunk_starts: Vec<usize>,
        up_to: usize,
    ) {
        let needs_init = self
            .cache
            .as_ref()
            .map(|c| c.file_path != path)
            .unwrap_or(true);

        if needs_init {
            // Check for pre-computed background highlight results first
            if let Some((lc, rc)) = self.bg_highlights.remove(path) {
                self.cache = Some(HighlightCache::from_precomputed(path.to_string(), lc, rc));
                return;
            }

            // Fall back to on-demand highlighting
            self.cache = self
                .highlighter
                .create_cache(path, left_lines, right_lines, hunk_starts);
        }

        if let Some(ref mut cache) = self.cache {
            self.highlighter.extend_cache(cache, up_to);
        }
    }

    /// Spawn a background thread to pre-highlight all files.
    #[allow(clippy::type_complexity)]
    pub(crate) fn spawn_bg_highlight(
        &mut self,
        file_data: Vec<(String, Vec<String>, Vec<String>, Vec<usize>)>,
    ) {
        if file_data.is_empty() {
            return;
        }

        let (tx, rx) = mpsc::channel();
        self.bg_highlight_rx = Some(rx);
        let theme_name = self.theme_name.clone();

        std::thread::spawn(move || {
            let highlighter = SyntaxHighlighter::with_theme(&theme_name).unwrap_or_default();
            for (path, left_lines, right_lines, hunk_starts) in file_data {
                if let Some(pair) =
                    highlighter.highlight_all_lines(&path, &left_lines, &right_lines, &hunk_starts)
                {
                    if tx.send((path, pair)).is_err() {
                        break; // Receiver dropped
                    }
                }
            }
        });
    }

    /// Drain completed background highlight results into the local cache.
    pub fn drain_bg_highlights(&mut self) {
        if let Some(ref rx) = self.bg_highlight_rx {
            while let Ok((path, pair)) = rx.try_recv() {
                self.bg_highlights.insert(path, pair);
            }
        }
    }
}
