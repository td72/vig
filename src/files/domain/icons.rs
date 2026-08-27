//! Nerd Font icons for directory entries.
//!
//! Only glyphs from the stable Nerd Font ranges are used (Devicons `e7xx`,
//! Seti `e6xx`, Font Awesome `f0xx`–`f2xx`, Octicons `f4xx`), which render the
//! same in Nerd Fonts v2 and v3.

use crate::files::domain::fs::DirEntry;
use ratatui::style::Color;

/// Valid values for the `icons` config option.
pub const ICON_MODES: &[&str] = &["nerd", "none"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Icon {
    pub glyph: &'static str,
    pub color: Color,
}

const DIR: Icon = Icon {
    glyph: "\u{f07b}",
    color: Color::Blue,
};
const FILE: Icon = Icon {
    glyph: "\u{f15b}",
    color: Color::Gray,
};
const SYMLINK: Icon = Icon {
    glyph: "\u{f481}",
    color: Color::Cyan,
};

const RUST: Color = Color::Rgb(222, 165, 132);

/// Pick the icon for `entry`: directories, then well-known file names, then
/// extensions, then a generic file glyph.
pub fn icon_for(entry: &DirEntry) -> Icon {
    if entry.is_dir {
        return DIR;
    }
    if let Some(icon) = by_name(&entry.name) {
        return icon;
    }
    let ext = entry
        .name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    if let Some(icon) = by_extension(&ext) {
        return icon;
    }
    if entry.is_symlink {
        return SYMLINK;
    }
    FILE
}

fn icon(glyph: &'static str, color: Color) -> Option<Icon> {
    Some(Icon { glyph, color })
}

fn by_name(name: &str) -> Option<Icon> {
    match name {
        "Cargo.toml" | "Cargo.lock" => icon("\u{e7a8}", RUST),
        "Dockerfile" | "docker-compose.yml" | "docker-compose.yaml" | "compose.yaml" => {
            icon("\u{e7b0}", Color::Blue)
        }
        "Makefile" | "makefile" | "justfile" | "Justfile" => icon("\u{f085}", Color::Gray),
        ".gitignore" | ".gitattributes" | ".gitmodules" | ".gitconfig" => {
            icon("\u{e702}", Color::Rgb(240, 80, 50))
        }
        "LICENSE" | "LICENSE.md" | "LICENSE.txt" | "COPYING" => icon("\u{f0e3}", Color::Yellow),
        "README" | "README.md" | "README.rst" => icon("\u{f48a}", Color::Blue),
        "package.json" | "package-lock.json" => icon("\u{e71e}", Color::Green),
        _ => None,
    }
}

fn by_extension(ext: &str) -> Option<Icon> {
    match ext {
        "rs" => icon("\u{e7a8}", RUST),
        "md" | "markdown" | "rst" => icon("\u{f48a}", Color::Blue),
        "toml" | "yaml" | "yml" | "ini" | "conf" | "cfg" | "kdl" | "env" => {
            icon("\u{e615}", Color::Gray)
        }
        "json" | "jsonc" => icon("\u{e60b}", Color::Yellow),
        "lock" => icon("\u{f023}", Color::Gray),
        "py" | "pyi" => icon("\u{e73c}", Color::Yellow),
        "js" | "mjs" | "cjs" => icon("\u{e74e}", Color::Yellow),
        "ts" | "mts" | "cts" => icon("\u{e628}", Color::Blue),
        "jsx" | "tsx" => icon("\u{e7ba}", Color::Cyan),
        "go" => icon("\u{e627}", Color::Cyan),
        "rb" | "erb" | "rake" => icon("\u{e739}", Color::Red),
        "sh" | "bash" | "zsh" | "fish" => icon("\u{e795}", Color::Green),
        "html" | "htm" => icon("\u{e736}", Color::Red),
        "css" | "scss" | "sass" | "less" => icon("\u{e749}", Color::Blue),
        "c" | "h" => icon("\u{e61e}", Color::Blue),
        "cpp" | "cc" | "cxx" | "hpp" | "hh" => icon("\u{e61d}", Color::Blue),
        "java" | "kt" | "kts" => icon("\u{e738}", Color::Red),
        "lua" => icon("\u{e620}", Color::Blue),
        "vim" => icon("\u{e62b}", Color::Green),
        "sql" | "db" | "sqlite" | "sqlite3" => icon("\u{f1c0}", Color::Yellow),
        "csv" | "tsv" => icon("\u{f1c3}", Color::Green),
        "xml" | "svg" => icon("\u{e619}", Color::Yellow),
        "txt" | "log" => icon("\u{f15c}", Color::Gray),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" => icon("\u{f1c5}", Color::Magenta),
        "mp4" | "mov" | "mkv" | "webm" | "tape" => icon("\u{f03d}", Color::Magenta),
        "zip" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "zst" | "7z" | "rar" => {
            icon("\u{f410}", Color::Yellow)
        }
        "pdf" => icon("\u{f1c1}", Color::Red),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn entry(name: &str, is_dir: bool, is_symlink: bool) -> DirEntry {
        DirEntry {
            name: name.to_string(),
            path: PathBuf::from(name),
            is_dir,
            is_symlink,
            size: 0,
        }
    }

    #[test]
    fn picks_dir_name_extension_and_fallbacks() {
        assert_eq!(icon_for(&entry("src", true, false)), DIR);
        assert_eq!(icon_for(&entry("src", true, true)), DIR);
        assert_eq!(
            icon_for(&entry("Cargo.toml", false, false)).glyph,
            "\u{e7a8}"
        );
        assert_eq!(icon_for(&entry("main.rs", false, false)).glyph, "\u{e7a8}");
        assert_eq!(icon_for(&entry("MAIN.RS", false, false)).glyph, "\u{e7a8}");
        assert_eq!(icon_for(&entry("notes.md", false, false)).glyph, "\u{f48a}");
        assert_eq!(icon_for(&entry("link", false, true)), SYMLINK);
        assert_eq!(icon_for(&entry("unknown.xyz", false, false)), FILE);
        assert_eq!(icon_for(&entry("noext", false, false)), FILE);
    }

    #[test]
    fn glyphs_are_single_column() {
        for e in [
            "src", "main.rs", "x.md", "x.json", "x.png", "x.zip", "link", "z",
        ] {
            let g = icon_for(&entry(e, e == "src", e == "link")).glyph;
            assert_eq!(ratatui::text::Span::raw(g).width(), 1, "{e}: {g:?}");
        }
    }
}
