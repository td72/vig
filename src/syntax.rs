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
    // Syntect state for incremental continuation
    left_parse_state: ParseState,
    left_highlight_state: HighlightState,
    right_parse_state: ParseState,
    right_highlight_state: HighlightState,
    // Raw line text (stored once on file switch)
    left_lines: Vec<String>,
    right_lines: Vec<String>,
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
        let theme = theme_set.themes["base16-eighties.dark"].clone();
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
    ) -> Option<HighlightCache> {
        let first_line = left_lines.first().map(|s| s.as_str());
        let syntax = self.find_syntax(file_path, first_line)?;
        let highlighter = Highlighter::new(&self.theme);
        Some(HighlightCache {
            file_path: file_path.to_string(),
            left_colors: Vec::with_capacity(left_lines.len()),
            right_colors: Vec::with_capacity(right_lines.len()),
            processed_up_to: 0,
            left_parse_state: ParseState::new(syntax),
            left_highlight_state: HighlightState::new(&highlighter, ScopeStack::new()),
            right_parse_state: ParseState::new(syntax),
            right_highlight_state: HighlightState::new(&highlighter, ScopeStack::new()),
            left_lines,
            right_lines,
        })
    }

    /// Extend the cache to cover at least `up_to` rows.
    /// Only processes rows not yet highlighted (incremental).
    pub fn extend_cache(&self, cache: &mut HighlightCache, up_to: usize) {
        let target = up_to.min(cache.left_lines.len());
        if cache.processed_up_to >= target {
            return;
        }

        let highlighter = Highlighter::new(&self.theme);
        for i in cache.processed_up_to..target {
            // Left side
            let left = highlight_line_colors(
                &cache.left_lines[i],
                &mut cache.left_parse_state,
                &mut cache.left_highlight_state,
                &self.syntax_set,
                &highlighter,
            );
            cache.left_colors.push(left);

            // Right side
            let right = highlight_line_colors(
                &cache.right_lines[i],
                &mut cache.right_parse_state,
                &mut cache.right_highlight_state,
                &self.syntax_set,
                &highlighter,
            );
            cache.right_colors.push(right);
        }
        cache.processed_up_to = target;
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
    let ops = match parse_state.parse_line(line, syntax_set) {
        Ok(ops) => ops,
        Err(_) => return Vec::new(),
    };
    let mut colors = Vec::new();
    for (style, text) in HighlightIterator::new(highlight_state, &ops, line, highlighter) {
        let color = syntect_to_ratatui_color(style.foreground);
        for _ in text.chars() {
            colors.push(color);
        }
    }
    colors
}

fn syntect_to_ratatui_color(c: syntect::highlighting::Color) -> Color {
    Color::Rgb(c.r, c.g, c.b)
}
