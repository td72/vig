//! Read-only filesystem access for the Files page: directory listings and
//! file previews.

use crate::files::domain::image::{self, ImageInfo};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Maximum bytes read for a preview.
pub const PREVIEW_MAX_BYTES: u64 = 1024 * 1024;
/// Maximum lines kept for a preview.
pub const PREVIEW_MAX_LINES: usize = 5000;
/// Entries never shown, whatever their type (`.git` is a directory in a normal
/// checkout but a file in worktrees and submodules).
const HIDDEN_NAMES: &[&str] = &[".git"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub path: PathBuf,
    /// Directory (following symlinks, so symlinked directories are browsable).
    pub is_dir: bool,
    pub is_symlink: bool,
    /// File size in bytes (0 for directories).
    pub size: u64,
}

impl DirEntry {
    /// Display name, `ls -F` style: directories get a trailing `/`, symlinks `@`.
    pub fn display_name(&self) -> String {
        match (self.is_dir, self.is_symlink) {
            (true, true) => format!("{}@/", self.name),
            (true, false) => format!("{}/", self.name),
            (false, true) => format!("{}@", self.name),
            (false, false) => self.name.clone(),
        }
    }
}

/// List `dir`, directories first, each group sorted case-insensitively.
/// Unreadable entries are skipped; an unreadable directory yields an error.
pub fn list_dir(dir: &Path) -> std::io::Result<Vec<DirEntry>> {
    let mut entries: Vec<DirEntry> = fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if HIDDEN_NAMES.contains(&name.as_str()) {
                return None;
            }
            let path = e.path();
            let is_symlink = fs::symlink_metadata(&path).ok()?.is_symlink();
            // Follow symlinks for the type/size; a dangling symlink is shown as a file.
            let meta = fs::metadata(&path).ok();
            let is_dir = meta.as_ref().is_some_and(|m| m.is_dir());
            Some(DirEntry {
                path,
                is_dir,
                is_symlink,
                size: if is_dir {
                    0
                } else {
                    meta.map_or(0, |m| m.len())
                },
                name,
            })
        })
        .collect();
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(entries)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Preview {
    /// Text file contents (possibly truncated).
    Text {
        lines: Vec<String>,
        truncated: bool,
    },
    /// A directory listing.
    Dir(Vec<DirEntry>),
    /// An image; decoding happens in the preview pane, this is header-only.
    Image(ImageInfo),
    Binary,
    Empty,
    Error(String),
}

/// Build a preview for `entry`.
pub fn preview(entry: &DirEntry) -> Preview {
    if entry.is_dir {
        return match list_dir(&entry.path) {
            Ok(entries) => Preview::Dir(entries),
            Err(e) => Preview::Error(e.to_string()),
        };
    }
    if image::is_image_path(&entry.path) {
        if let Some(info) = image::probe(&entry.path) {
            return Preview::Image(info);
        }
    }
    read_text_preview(&entry.path)
}

fn read_text_preview(path: &Path) -> Preview {
    let mut file = match fs::File::open(path) {
        Ok(f) => f,
        Err(e) => return Preview::Error(e.to_string()),
    };
    // Read one byte past the limit so a file of exactly PREVIEW_MAX_BYTES is
    // not reported as truncated.
    let mut buf = Vec::new();
    if let Err(e) = file
        .by_ref()
        .take(PREVIEW_MAX_BYTES + 1)
        .read_to_end(&mut buf)
    {
        return Preview::Error(e.to_string());
    }
    if buf.is_empty() {
        return Preview::Empty;
    }
    if buf.iter().take(8192).any(|&b| b == 0) {
        return Preview::Binary;
    }
    let mut truncated = buf.len() as u64 > PREVIEW_MAX_BYTES;
    buf.truncate(PREVIEW_MAX_BYTES as usize);
    let text = String::from_utf8_lossy(&buf);
    let mut lines: Vec<String> = text.lines().map(|l| l.replace('\t', "    ")).collect();
    if lines.len() > PREVIEW_MAX_LINES {
        lines.truncate(PREVIEW_MAX_LINES);
        truncated = true;
    }
    Preview::Text { lines, truncated }
}

/// Human-readable size (`1.2K`, `3.4M`, …).
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "K", "M", "G", "T"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{bytes}B")
    } else {
        format!("{v:.1}{}", UNITS[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir() -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "vig-fs-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn list_dir_sorts_dirs_first_and_hides_git() {
        let d = tmpdir();
        fs::create_dir(d.join("src")).unwrap();
        fs::create_dir(d.join(".git")).unwrap();
        fs::create_dir(d.join("Assets")).unwrap();
        fs::write(d.join("b.txt"), "b").unwrap();
        fs::write(d.join("A.txt"), "a").unwrap();
        fs::write(d.join(".hidden"), "").unwrap();
        let names: Vec<String> = list_dir(&d)
            .unwrap()
            .iter()
            .map(DirEntry::display_name)
            .collect();
        assert_eq!(names, vec!["Assets/", "src/", ".hidden", "A.txt", "b.txt"]);
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn git_file_hidden_and_symlinks_marked() {
        let d = tmpdir();
        fs::write(d.join(".git"), "gitdir: ../.git/worktrees/x").unwrap();
        fs::create_dir(d.join("real")).unwrap();
        fs::write(d.join("file.txt"), "x").unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(d.join("real"), d.join("link_dir")).unwrap();
            std::os::unix::fs::symlink(d.join("file.txt"), d.join("link_file")).unwrap();
        }
        let names: Vec<String> = list_dir(&d)
            .unwrap()
            .iter()
            .map(DirEntry::display_name)
            .collect();
        assert!(!names.iter().any(|n| n.starts_with(".git")), "{names:?}");
        #[cfg(unix)]
        assert_eq!(names, vec!["link_dir@/", "real/", "file.txt", "link_file@"]);
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn preview_exactly_at_limit_is_not_truncated() {
        let d = tmpdir();
        let exact = "a".repeat(PREVIEW_MAX_BYTES as usize);
        fs::write(d.join("exact"), &exact).unwrap();
        fs::write(d.join("over"), format!("{exact}b")).unwrap();
        let entries = list_dir(&d).unwrap();
        let by_name = |n: &str| entries.iter().find(|e| e.name == n).unwrap();
        assert!(matches!(
            preview(by_name("exact")),
            Preview::Text {
                truncated: false,
                ..
            }
        ));
        assert!(matches!(
            preview(by_name("over")),
            Preview::Text {
                truncated: true,
                ..
            }
        ));
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn preview_kinds() {
        let d = tmpdir();
        fs::write(d.join("t.rs"), "fn main() {\n\tlet x = 1;\n}\n").unwrap();
        fs::write(d.join("bin"), [0u8, 1, 2, 3]).unwrap();
        fs::write(d.join("empty"), "").unwrap();
        fs::create_dir(d.join("sub")).unwrap();
        let entries = list_dir(&d).unwrap();
        let by_name = |n: &str| entries.iter().find(|e| e.name == n).unwrap();
        assert_eq!(
            preview(by_name("t.rs")),
            Preview::Text {
                lines: vec!["fn main() {".into(), "    let x = 1;".into(), "}".into()],
                truncated: false
            }
        );
        assert_eq!(preview(by_name("bin")), Preview::Binary);
        ::image::RgbImage::new(3, 2)
            .save(d.join("pic.png"))
            .unwrap();
        fs::write(d.join("broken.png"), b"\x00\x00 not a png").unwrap();
        let entries = list_dir(&d).unwrap();
        let by_name = |n: &str| entries.iter().find(|e| e.name == n).unwrap();
        assert_eq!(
            preview(by_name("pic.png")),
            Preview::Image(ImageInfo {
                format: "PNG".into(),
                width: 3,
                height: 2
            })
        );
        // Not decodable → falls through to the ordinary text/binary check.
        assert_eq!(preview(by_name("broken.png")), Preview::Binary);
        assert_eq!(preview(by_name("empty")), Preview::Empty);
        assert!(matches!(preview(by_name("sub")), Preview::Dir(ref v) if v.is_empty()));
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn human_size_formats() {
        assert_eq!(human_size(0), "0B");
        assert_eq!(human_size(512), "512B");
        assert_eq!(human_size(1536), "1.5K");
        assert_eq!(human_size(3 * 1024 * 1024), "3.0M");
    }
}
