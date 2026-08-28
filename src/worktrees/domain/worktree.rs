//! `git worktree list --porcelain` and the HEAD summary of a worktree.

use super::git_output;
use super::types::{CommitSummary, Worktree};
use anyhow::{anyhow, Result};
use std::path::{Component, Path, PathBuf};

/// List the worktrees of the repository containing `root`, marking the one
/// vig is running in (`root` itself).
pub fn list_worktrees(root: &Path) -> Result<Vec<Worktree>> {
    let out = git_output(root, &["worktree", "list", "--porcelain"])?;
    let mut list = parse_porcelain(&String::from_utf8_lossy(&out));
    finish(&mut list, root);
    Ok(list)
}

/// Fill in `display_path` and `is_current` for a parsed listing.
pub fn finish(list: &mut [Worktree], current: &Path) {
    let main = list.first().map(|w| w.path.clone());
    let current_canon = canon(current);
    for wt in list.iter_mut() {
        wt.display_path = match &main {
            Some(main) => display_path(main, &wt.path),
            None => wt.path.to_string_lossy().into_owned(),
        };
        wt.is_current = canon(&wt.path) == current_canon;
    }
}

fn canon(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

/// Parse the porcelain listing: entries are separated by blank lines and
/// consist of `worktree <path>` followed by attribute lines.
pub fn parse_porcelain(text: &str) -> Vec<Worktree> {
    let mut list = Vec::new();
    let mut cur: Option<Worktree> = None;
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            if let Some(wt) = cur.take() {
                list.push(wt);
            }
            continue;
        }
        let (key, value) = match line.split_once(' ') {
            Some((k, v)) => (k, Some(v)),
            None => (line, None),
        };
        match key {
            "worktree" => {
                if let Some(wt) = cur.take() {
                    list.push(wt);
                }
                let path = unquote(value.unwrap_or_default());
                cur = Some(Worktree {
                    path: PathBuf::from(path),
                    display_path: String::new(),
                    head: None,
                    branch: None,
                    detached: false,
                    bare: false,
                    locked: None,
                    prunable: None,
                    is_main: list.is_empty(),
                    is_current: false,
                });
            }
            _ => {
                let Some(wt) = cur.as_mut() else { continue };
                match key {
                    "HEAD" => wt.head = value.map(str::to_string),
                    "branch" => {
                        wt.branch =
                            value.map(|v| v.strip_prefix("refs/heads/").unwrap_or(v).to_string())
                    }
                    "detached" => wt.detached = true,
                    "bare" => wt.bare = true,
                    "locked" => wt.locked = Some(unquote(value.unwrap_or_default())),
                    "prunable" => wt.prunable = Some(unquote(value.unwrap_or_default())),
                    _ => {}
                }
            }
        }
    }
    if let Some(wt) = cur.take() {
        list.push(wt);
    }
    list
}

/// Undo the C-style quoting git applies to unusual paths / reasons.
fn unquote(s: &str) -> String {
    let Some(inner) = s.strip_prefix('"').and_then(|s| s.strip_suffix('"')) else {
        return s.to_string();
    };
    // Octal escapes are raw bytes (UTF-8 sequences come out as `\303\251`),
    // so decode into bytes first and interpret them as UTF-8 at the end.
    let mut out: Vec<u8> = Vec::new();
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            let mut buf = [0u8; 4];
            out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            continue;
        }
        match chars.next() {
            Some('n') => out.push(b'\n'),
            Some('t') => out.push(b'\t'),
            Some('"') => out.push(b'"'),
            Some('\\') => out.push(b'\\'),
            Some(d) if d.is_digit(8) => {
                let mut v = d.to_digit(8).unwrap_or(0);
                for _ in 0..2 {
                    match chars.peek().and_then(|c| c.to_digit(8)) {
                        Some(n) => {
                            v = v * 8 + n;
                            chars.next();
                        }
                        None => break,
                    }
                }
                out.push(v as u8);
            }
            Some(other) => {
                let mut buf = [0u8; 4];
                out.extend_from_slice(other.encode_utf8(&mut buf).as_bytes());
            }
            None => {}
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// How a worktree path is shown: the main worktree by its directory name,
/// the others relative to the main worktree (`../wt-feature`,
/// `.claude/worktrees/x`), falling back to the absolute path when the two
/// share no common ancestor.
pub fn display_path(main: &Path, path: &Path) -> String {
    if path == main {
        return main
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| main.to_string_lossy().into_owned());
    }
    let base: Vec<Component> = main.components().collect();
    let target: Vec<Component> = path.components().collect();
    let common = base
        .iter()
        .zip(target.iter())
        .take_while(|(a, b)| a == b)
        .count();
    if common == 0 {
        return path.to_string_lossy().into_owned();
    }
    let mut rel = PathBuf::new();
    for _ in common..base.len() {
        rel.push("..");
    }
    for c in &target[common..] {
        rel.push(c.as_os_str());
    }
    rel.to_string_lossy().into_owned()
}

/// The HEAD commit of the worktree at `path`, with its changed files
/// (`git show --stat`, diffed against the first parent for merges).
pub fn head_summary(path: &Path) -> Result<CommitSummary> {
    let out = git_output(
        path,
        &[
            "show",
            "--no-color",
            "--no-ext-diff",
            "--first-parent",
            "--stat=72",
            "--date=format:%Y-%m-%d %H:%M",
            "--format=%H%x1f%an%x1f%ae%x1f%ad%x1f%ar%x1f%s%x1e",
            "HEAD",
        ],
    )?;
    parse_show(&String::from_utf8_lossy(&out))
}

/// Parse the `git show` output produced by [`head_summary`].
pub fn parse_show(text: &str) -> Result<CommitSummary> {
    let (header, rest) = text
        .split_once('\x1e')
        .ok_or_else(|| anyhow!("unexpected git show output"))?;
    let mut fields = header.split('\x1f');
    let mut next = || fields.next().unwrap_or("").trim().to_string();
    let hash = next();
    let author_name = next();
    let author_email = next();
    let date = next();
    let relative_date = next();
    let subject = next();
    if hash.is_empty() {
        return Err(anyhow!("unexpected git show output"));
    }
    let stat = rest
        .lines()
        .map(|l| l.trim_end().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    Ok(CommitSummary {
        hash,
        author_name,
        author_email,
        date,
        relative_date,
        subject,
        stat,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PORCELAIN: &str = "\
worktree /repo
HEAD 06ee70fc4b03fe3701895bd34e4605ba5d3c579b
branch refs/heads/main

worktree /repo/.claude/worktrees/agent-1
HEAD 06ee70fc4b03fe3701895bd34e4605ba5d3c579b
branch refs/heads/feat/procs-view
locked claude agent (pid 40747)

worktree /elsewhere/wt-detached
HEAD 1234567890abcdef1234567890abcdef12345678
detached
prunable gitdir file points to non-existent location

worktree /srv/bare.git
bare
";

    #[test]
    fn parses_porcelain_entries() {
        let list = parse_porcelain(PORCELAIN);
        assert_eq!(list.len(), 4);

        let main = &list[0];
        assert_eq!(main.path, PathBuf::from("/repo"));
        assert_eq!(main.branch.as_deref(), Some("main"));
        assert!(main.is_main);
        assert!(!main.detached);
        assert_eq!(main.short_head(), "06ee70f");

        let locked = &list[1];
        assert!(!locked.is_main);
        assert_eq!(locked.branch.as_deref(), Some("feat/procs-view"));
        assert_eq!(locked.locked.as_deref(), Some("claude agent (pid 40747)"));
        assert_eq!(locked.flags(), vec!["locked"]);

        let detached = &list[2];
        assert!(detached.detached);
        assert_eq!(detached.branch, None);
        assert_eq!(detached.ref_label(), "(detached 1234567)");
        assert!(detached.prunable.is_some());
        assert_eq!(detached.flags(), vec!["prunable"]);

        let bare = &list[3];
        assert!(bare.bare);
        assert_eq!(bare.head, None);
        assert_eq!(bare.ref_label(), "(bare)");
        assert_eq!(bare.flags(), vec!["bare"]);
    }

    #[test]
    fn display_paths_are_relative_to_main() {
        let mut list = parse_porcelain(PORCELAIN);
        finish(&mut list, Path::new("/repo/.claude/worktrees/agent-1"));
        assert_eq!(list[0].display_path, "repo");
        assert_eq!(list[1].display_path, ".claude/worktrees/agent-1");
        assert_eq!(list[2].display_path, "../elsewhere/wt-detached");
        assert_eq!(list[3].display_path, "../srv/bare.git");
        assert_eq!(list[0].flags(), vec!["main"]);
    }

    #[test]
    fn current_worktree_is_detected() {
        let mut list = parse_porcelain(PORCELAIN);
        // Paths in the fixture do not exist, so canonicalisation falls back
        // to a plain comparison.
        finish(&mut list, Path::new("/repo/.claude/worktrees/agent-1"));
        let current: Vec<bool> = list.iter().map(|w| w.is_current).collect();
        assert_eq!(current, vec![false, true, false, false]);

        finish(&mut list, Path::new("/repo"));
        assert!(list[0].is_current);
        assert!(!list[1].is_current);
    }

    #[test]
    fn current_worktree_survives_symlinked_root() {
        // A real directory reached through a symlink (macOS `/tmp` →
        // `/private/tmp`) must still match the canonical git path.
        let dir = std::env::temp_dir().join(format!("vig-wt-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let canonical = std::fs::canonicalize(&dir).unwrap();
        let mut list = parse_porcelain(&format!("worktree {}\nHEAD abc\n", canonical.display()));
        finish(&mut list, &dir);
        assert!(list[0].is_current);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn display_path_sibling_and_unrelated() {
        assert_eq!(
            display_path(Path::new("/tmp/x/repo"), Path::new("/tmp/x/wt-feature")),
            "../wt-feature"
        );
        assert_eq!(
            display_path(Path::new("/tmp/x/repo"), Path::new("/tmp/x/repo/sub")),
            "sub"
        );
        assert_eq!(
            display_path(Path::new("/tmp/x/repo"), Path::new("/tmp/x/repo")),
            "repo"
        );
        // No common root at all: absolute path.
        assert_eq!(
            display_path(Path::new("rel/repo"), Path::new("/abs/wt")),
            "/abs/wt"
        );
    }

    #[test]
    fn unquotes_git_style_paths() {
        assert_eq!(unquote("plain"), "plain");
        assert_eq!(unquote("\"with space\""), "with space");
        assert_eq!(unquote("\"tab\\there\""), "tab\there");
        assert_eq!(unquote("\"q\\\"uote\""), "q\"uote");
        assert_eq!(unquote("\"\\303\\251\""), "\u{e9}");
        let list = parse_porcelain("worktree \"/p/with space\"\nHEAD abc\nbranch refs/heads/x\n");
        assert_eq!(list[0].path, PathBuf::from("/p/with space"));
    }

    #[test]
    fn parses_show_output() {
        let text = "06ee70fc4b03fe3701895bd34e4605ba5d3c579b\x1fKohei\x1fk@example.com\x1f2026-08-28 16:20\x1f6 minutes ago\x1fMerge pull request #108\x1e\n\n src/core/mod.rs  |   1 +\n src/core/tree.rs | 162 ++++++\n 2 files changed, 163 insertions(+)\n";
        let s = parse_show(text).unwrap();
        assert_eq!(s.short_hash(), "06ee70f");
        assert_eq!(s.author_name, "Kohei");
        assert_eq!(s.author_email, "k@example.com");
        assert_eq!(s.date, "2026-08-28 16:20");
        assert_eq!(s.relative_date, "6 minutes ago");
        assert_eq!(s.subject, "Merge pull request #108");
        assert_eq!(s.stat.len(), 3);
        assert_eq!(s.stat[0], " src/core/mod.rs  |   1 +");
        assert!(parse_show("garbage").is_err());
    }
}
