use ratatui::style::Color;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::{SyntaxDefinition, SyntaxReference, SyntaxSet};

pub struct SyntaxHighlighter {
    syntax_set: SyntaxSet,
    theme: Theme,
}

/// A single highlighted line: Vec of (fg_color, text_fragment) pairs.
pub type HighlightedLine = Vec<(Color, String)>;

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
    pub fn find_syntax(&self, path: &str, first_line: Option<&str>) -> Option<&SyntaxReference> {
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

    /// Highlight all lines of a file, returning per-line color spans.
    /// Each line produces a Vec<(Color, String)> where Color is the foreground.
    pub fn highlight_lines(&self, lines: &[&str], path: &str) -> Vec<HighlightedLine> {
        let first_line = lines.first().copied();
        let syntax = match self.find_syntax(path, first_line) {
            Some(s) => s,
            None => return Vec::new(),
        };

        let mut highlighter =
            syntect::easy::HighlightLines::new(syntax, &self.theme);

        lines
            .iter()
            .map(|line| {
                let regions = highlighter
                    .highlight_line(line, &self.syntax_set)
                    .unwrap_or_default();
                regions
                    .into_iter()
                    .map(|(style, text)| {
                        let fg = syntect_to_ratatui_color(style.foreground);
                        (fg, text.to_string())
                    })
                    .collect()
            })
            .collect()
    }
}

fn syntect_to_ratatui_color(c: syntect::highlighting::Color) -> Color {
    Color::Rgb(c.r, c.g, c.b)
}
