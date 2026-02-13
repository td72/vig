use ratatui::style::Color;
use syntect::highlighting::{HighlightIterator, HighlightState, Highlighter, Theme, ThemeSet};
use syntect::parsing::{ParseState, ScopeStack, SyntaxDefinition, SyntaxReference, SyntaxSet};

pub struct SyntaxHighlighter {
    syntax_set: SyntaxSet,
    theme: Theme,
}

/// Cached highlight state for incremental processing.
/// Stores pre-expanded per-character colors and syntect parse state
/// so highlighting can resume where it left off on scroll.
pub struct HighlightCache {
    pub file_path: String,
    /// Pre-expanded per-character fg colors, indexed by row.
    pub left_colors: Vec<Vec<Color>>,
    pub right_colors: Vec<Vec<Color>>,
    /// How many rows have been highlighted so far.
    processed_up_to: usize,
    /// Incremental state for on-demand highlighting. None if pre-computed.
    incremental: Option<IncrementalState>,
}

struct IncrementalState {
    left_parse_state: ParseState,
    left_highlight_state: HighlightState,
    right_parse_state: ParseState,
    right_highlight_state: HighlightState,
    left_lines: Vec<String>,
    right_lines: Vec<String>,
    /// Row indices where hunks start — parser state resets here.
    hunk_starts: Vec<usize>,
}

impl HighlightCache {
    /// Create a cache from pre-computed background highlight results.
    pub fn from_precomputed(
        file_path: String,
        left_colors: Vec<Vec<Color>>,
        right_colors: Vec<Vec<Color>>,
    ) -> Self {
        let processed = left_colors.len();
        Self {
            file_path,
            left_colors,
            right_colors,
            processed_up_to: processed,
            incremental: None,
        }
    }
}

const TOML_SYNTAX: &str = r#"%YAML 1.2
---
name: TOML
file_extensions: [toml]
scope: source.toml
contexts:
  main:
    - match: '#.*$'
      scope: comment.line.number-sign.toml
    - match: '\[{1,2}'
      scope: punctuation.definition.table.begin.toml
      push: table_name
    - match: '([A-Za-z0-9_.-]+)\s*(=)'
      captures:
        1: entity.name.tag.toml
        2: punctuation.separator.key-value.toml
    - match: '"""'
      scope: punctuation.definition.string.begin.toml
      push: triple_double_string
    - match: "'''"
      scope: punctuation.definition.string.begin.toml
      push: triple_single_string
    - match: '"'
      scope: punctuation.definition.string.begin.toml
      push: double_string
    - match: "'"
      scope: punctuation.definition.string.begin.toml
      push: single_string
    - match: '\b(true|false)\b'
      scope: constant.language.boolean.toml
    - match: '\b\d{4}-\d{2}-\d{2}(T\d{2}:\d{2}:\d{2})?\b'
      scope: constant.other.datetime.toml
    - match: '[+-]?\b\d[\d_]*(\.[\d_]+)?([eE][+-]?\d+)?\b'
      scope: constant.numeric.toml
  table_name:
    - match: '\]{1,2}'
      scope: punctuation.definition.table.end.toml
      pop: true
    - match: '[^\]]+'
      scope: entity.name.section.toml
  double_string:
    - meta_scope: string.quoted.double.toml
    - match: '\\.'
      scope: constant.character.escape.toml
    - match: '"'
      scope: punctuation.definition.string.end.toml
      pop: true
  single_string:
    - meta_scope: string.quoted.single.toml
    - match: "'"
      scope: punctuation.definition.string.end.toml
      pop: true
  triple_double_string:
    - meta_scope: string.quoted.triple.double.toml
    - match: '\\.'
      scope: constant.character.escape.toml
    - match: '"""'
      scope: punctuation.definition.string.end.toml
      pop: true
  triple_single_string:
    - meta_scope: string.quoted.triple.single.toml
    - match: "'''"
      scope: punctuation.definition.string.end.toml
      pop: true
"#;

impl SyntaxHighlighter {
    pub fn new() -> Self {
        let mut builder = SyntaxSet::load_defaults_newlines().into_builder();
        if let Ok(toml_def) = SyntaxDefinition::load_from_str(TOML_SYNTAX, true, None) {
            builder.add(toml_def);
        }
        let syntax_set = builder.build();
        let theme_set = ThemeSet::load_defaults();
        let theme = theme_set
            .themes
            .get("base16-eighties.dark")
            .cloned()
            .or_else(|| theme_set.themes.values().next().cloned())
            .expect("No themes available in ThemeSet");
        Self { syntax_set, theme }
    }

    /// Find the syntax definition for a file path by extension,
    /// falling back to first-line detection.
    fn find_syntax(&self, path: &str, first_line: Option<&str>) -> Option<&SyntaxReference> {
        if let Some(ext) = path.rsplit('.').next() {
            if let Some(syn) = self.syntax_set.find_syntax_by_extension(ext) {
                return Some(syn);
            }
        }
        if let Some(line) = first_line {
            let syn = self.syntax_set.find_syntax_by_first_line(line)?;
            if syn.name != "Plain Text" {
                return Some(syn);
            }
        }
        None
    }

    /// Create a new highlight cache for a file. Returns None if syntax is unsupported.
    pub fn create_cache(
        &self,
        file_path: &str,
        left_lines: Vec<String>,
        right_lines: Vec<String>,
        hunk_starts: Vec<usize>,
    ) -> Option<HighlightCache> {
        // Use first non-header line for first-line syntax detection
        let first_content = left_lines
            .iter()
            .enumerate()
            .find(|(i, s)| !hunk_starts.contains(i) && !s.is_empty())
            .map(|(_, s)| s.as_str());
        let syntax = self.find_syntax(file_path, first_content)?;
        let highlighter = Highlighter::new(&self.theme);
        Some(HighlightCache {
            file_path: file_path.to_string(),
            left_colors: Vec::with_capacity(left_lines.len()),
            right_colors: Vec::with_capacity(right_lines.len()),
            processed_up_to: 0,
            incremental: Some(IncrementalState {
                left_parse_state: ParseState::new(syntax),
                left_highlight_state: HighlightState::new(&highlighter, ScopeStack::new()),
                right_parse_state: ParseState::new(syntax),
                right_highlight_state: HighlightState::new(&highlighter, ScopeStack::new()),
                left_lines,
                right_lines,
                hunk_starts,
            }),
        })
    }

    /// Extend the cache to cover at least `up_to` rows.
    /// Only processes rows not yet highlighted (incremental).
    /// No-op for pre-computed caches.
    /// Resets parser state at hunk boundaries so unclosed strings/comments
    /// in one hunk don't corrupt highlighting in the next.
    pub fn extend_cache(&self, cache: &mut HighlightCache, up_to: usize) {
        let inc = match &mut cache.incremental {
            Some(inc) => inc,
            None => return, // pre-computed, nothing to extend
        };
        let target = up_to.min(inc.left_lines.len());
        if cache.processed_up_to >= target {
            return;
        }

        let highlighter = Highlighter::new(&self.theme);
        for i in cache.processed_up_to..target {
            // Reset parser state at hunk boundaries
            if inc.hunk_starts.contains(&i) {
                if let Some(syntax) = self.find_syntax(&cache.file_path, None) {
                    inc.left_parse_state = ParseState::new(syntax);
                    inc.left_highlight_state =
                        HighlightState::new(&highlighter, ScopeStack::new());
                    inc.right_parse_state = ParseState::new(syntax);
                    inc.right_highlight_state =
                        HighlightState::new(&highlighter, ScopeStack::new());
                }
                cache.left_colors.push(Vec::new());
                cache.right_colors.push(Vec::new());
                continue;
            }

            // Left side
            let left = highlight_line_colors(
                &inc.left_lines[i],
                &mut inc.left_parse_state,
                &mut inc.left_highlight_state,
                &self.syntax_set,
                &highlighter,
            );
            cache.left_colors.push(left);

            // Right side
            let right = highlight_line_colors(
                &inc.right_lines[i],
                &mut inc.right_parse_state,
                &mut inc.right_highlight_state,
                &self.syntax_set,
                &highlighter,
            );
            cache.right_colors.push(right);
        }
        cache.processed_up_to = target;
    }

    /// Highlight all lines of a file at once. Used by background thread.
    /// Resets parser state at each hunk boundary.
    pub fn highlight_all_lines(
        &self,
        file_path: &str,
        left_lines: &[String],
        right_lines: &[String],
        hunk_starts: &[usize],
    ) -> Option<(Vec<Vec<Color>>, Vec<Vec<Color>>)> {
        let first_content = left_lines
            .iter()
            .enumerate()
            .find(|(i, s)| !hunk_starts.contains(i) && !s.is_empty())
            .map(|(_, s)| s.as_str());
        let syntax = self.find_syntax(file_path, first_content)?;
        let highlighter = Highlighter::new(&self.theme);

        let mut left_parse = ParseState::new(syntax);
        let mut left_hl = HighlightState::new(&highlighter, ScopeStack::new());
        let mut right_parse = ParseState::new(syntax);
        let mut right_hl = HighlightState::new(&highlighter, ScopeStack::new());

        let mut left_colors = Vec::with_capacity(left_lines.len());
        let mut right_colors = Vec::with_capacity(right_lines.len());

        for (i, (l, r)) in left_lines.iter().zip(right_lines.iter()).enumerate() {
            if hunk_starts.contains(&i) {
                // Reset parser state at hunk boundary
                left_parse = ParseState::new(syntax);
                left_hl = HighlightState::new(&highlighter, ScopeStack::new());
                right_parse = ParseState::new(syntax);
                right_hl = HighlightState::new(&highlighter, ScopeStack::new());
                left_colors.push(Vec::new());
                right_colors.push(Vec::new());
                continue;
            }
            left_colors.push(highlight_line_colors(
                l, &mut left_parse, &mut left_hl, &self.syntax_set, &highlighter,
            ));
            right_colors.push(highlight_line_colors(
                r, &mut right_parse, &mut right_hl, &self.syntax_set, &highlighter,
            ));
        }

        Some((left_colors, right_colors))
    }
}

/// Highlight a single line using low-level syntect API, returning per-character colors.
fn highlight_line_colors(
    line: &str,
    parse_state: &mut ParseState,
    highlight_state: &mut HighlightState,
    syntax_set: &SyntaxSet,
    highlighter: &Highlighter,
) -> Vec<Color> {
    // Append '\n' so that single-line comment scopes (matching `$`) close properly.
    let line_with_nl = format!("{}\n", line);
    let ops = match parse_state.parse_line(&line_with_nl, syntax_set) {
        Ok(ops) => ops,
        Err(_) => return Vec::new(),
    };
    let mut colors = Vec::new();
    for (style, text) in HighlightIterator::new(highlight_state, &ops, &line_with_nl, highlighter)
    {
        let color = syntect_to_ratatui_color(style.foreground);
        for _ in text.chars() {
            colors.push(color);
        }
    }
    // Remove the trailing color entry produced by the appended '\n'.
    colors.pop();
    colors
}

fn syntect_to_ratatui_color(c: syntect::highlighting::Color) -> Color {
    Color::Rgb(c.r, c.g, c.b)
}
