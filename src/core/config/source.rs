//! Locating and reading the user's config file.
//!
//! Lookup order:
//! 1. `--config <path>` (explicit)
//! 2. `$VIG_CONFIG` (explicit)
//! 3. `$XDG_CONFIG_HOME/vig/config.kdl`, falling back to `~/.config/vig/config.kdl`
//!
//! `dirs::config_dir()` is deliberately not used: on macOS it points at
//! `~/Library/Application Support`, whereas `~/.config/vig` is what users of
//! zellij / helix / etc. expect on every platform.

use crate::core::config::loader::Config;
use anyhow::{anyhow, Context, Result};
use kdl::KdlDocument;
use std::path::{Path, PathBuf};

pub const ENV_VAR: &str = "VIG_CONFIG";

/// Where the config would be read from, and whether the user asked for it explicitly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigSource {
    /// From `--config` or `$VIG_CONFIG`. A missing file is an error.
    Explicit(PathBuf),
    /// The default location. A missing file means "use the built-in defaults".
    Default(PathBuf),
    /// No home directory could be determined.
    Unavailable,
}

impl ConfigSource {
    pub fn resolve(explicit: Option<PathBuf>) -> Self {
        if let Some(p) = explicit {
            return Self::Explicit(p);
        }
        if let Some(p) = std::env::var_os(ENV_VAR).filter(|s| !s.is_empty()) {
            return Self::Explicit(PathBuf::from(p));
        }
        match default_config_path() {
            Some(p) => Self::Default(p),
            None => Self::Unavailable,
        }
    }

    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::Explicit(p) | Self::Default(p) => Some(p),
            Self::Unavailable => None,
        }
    }
}

/// `$XDG_CONFIG_HOME/vig/config.kdl` or `~/.config/vig/config.kdl`.
pub fn default_config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".config")))?;
    Some(base.join("vig").join("config.kdl"))
}

/// Resolve the config source and load it, merged over the built-in defaults.
pub fn load(explicit: Option<PathBuf>) -> Result<Config> {
    match ConfigSource::resolve(explicit) {
        ConfigSource::Unavailable => Ok(Config::builtin()),
        ConfigSource::Explicit(path) => {
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("cannot read config file {}", path.display()))?;
            load_from_text(&text, &path)
        }
        ConfigSource::Default(path) => match std::fs::read_to_string(&path) {
            Ok(text) => load_from_text(&text, &path),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::builtin()),
            Err(e) => Err(e).with_context(|| format!("cannot read config file {}", path.display())),
        },
    }
}

fn load_from_text(text: &str, path: &Path) -> Result<Config> {
    let doc = parse_user_kdl(text, path)?;
    Config::with_user(&doc, path.to_path_buf())
}

/// Parse user KDL, turning parser diagnostics into `path:line:col: message` lines.
pub fn parse_user_kdl(text: &str, path: &Path) -> Result<KdlDocument> {
    text.parse::<KdlDocument>().map_err(|e| {
        let mut msg = format!("failed to parse config file {}", path.display());
        for d in &e.diagnostics {
            let (line, col) = line_col(text, d.span.offset());
            msg.push_str(&format!(
                "\n  {}:{line}:{col}: {}",
                path.display(),
                d.message.as_deref().unwrap_or("syntax error")
            ));
            if let Some(help) = &d.help {
                msg.push_str(&format!(" ({help})"));
            }
        }
        anyhow!(msg)
    })
}

/// 1-based (line, column) of a byte offset.
fn line_col(text: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(text.len());
    let before = &text[..offset];
    let line = before.matches('\n').count() + 1;
    let col = before.rfind('\n').map_or(offset, |nl| offset - nl - 1) + 1;
    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_col_basic() {
        assert_eq!(line_col("abc", 0), (1, 1));
        assert_eq!(line_col("abc", 2), (1, 3));
        assert_eq!(line_col("ab\ncd", 3), (2, 1));
        assert_eq!(line_col("ab\ncd", 4), (2, 2));
        assert_eq!(line_col("ab", 99), (1, 3));
    }

    #[test]
    fn parse_error_mentions_path_and_line() {
        let err = parse_user_kdl("app {\n  \"q\" \"Quit\"\n", Path::new("/x/config.kdl"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("/x/config.kdl:"), "{err}");
        assert!(
            err.contains("failed to parse config file /x/config.kdl"),
            "{err}"
        );
    }

    #[test]
    fn explicit_path_wins() {
        let src = ConfigSource::resolve(Some(PathBuf::from("/tmp/x.kdl")));
        assert_eq!(src, ConfigSource::Explicit(PathBuf::from("/tmp/x.kdl")));
    }
}
