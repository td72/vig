//! `git stash list` and the patch of a stash entry.

use super::git_output;
use super::types::Stash;
use crate::git::domain::diff::{files_from_diff, FileDiff};
use anyhow::Result;
use std::path::Path;

/// Field separator used in the `--format` below (ASCII unit separator).
const SEP: char = '\x1f';

/// List the stash entries of the repository containing `root`.
pub fn list_stashes(root: &Path) -> Result<Vec<Stash>> {
    let out = git_output(root, &["stash", "list", "--format=%gd%x1f%gs%x1f%cr%x1f%H"])?;
    Ok(parse_stash_list(&String::from_utf8_lossy(&out)))
}

/// Parse `git stash list --format=%gd␟%gs␟%cr␟%H` output.
pub fn parse_stash_list(text: &str) -> Vec<Stash> {
    text.lines()
        .enumerate()
        .filter_map(|(pos, line)| {
            let mut f = line.splitn(4, SEP);
            let selector = f.next()?.trim();
            let subject = f.next().unwrap_or("").trim();
            let relative_date = f.next().unwrap_or("").trim().to_string();
            let hash = f.next().unwrap_or("").trim().to_string();
            let index = selector
                .strip_prefix("stash@{")
                .and_then(|s| s.strip_suffix('}'))
                .and_then(|s| s.parse().ok())
                .unwrap_or(pos);
            let (branch, message) = split_subject(subject);
            Some(Stash {
                index,
                branch,
                message,
                relative_date,
                hash,
            })
        })
        .collect()
}

/// Split a stash reflog subject into the branch it was created on and the
/// message: `WIP on main: abc1234 subject` / `On main: my message`.
/// Anything else is kept verbatim as the message.
pub fn split_subject(subject: &str) -> (Option<String>, String) {
    for prefix in ["WIP on ", "On "] {
        if let Some(rest) = subject.strip_prefix(prefix) {
            if let Some((branch, msg)) = rest.split_once(": ") {
                return (Some(branch.to_string()), msg.trim().to_string());
            }
            if let Some(branch) = rest.strip_suffix(':') {
                return (Some(branch.to_string()), String::new());
            }
        }
    }
    (None, subject.to_string())
}

/// The patch of `stash@{index}` (working tree changes plus untracked files
/// when git supports `--include-untracked`), parsed into side-by-side diffs.
pub fn stash_patch(root: &Path, index: usize) -> Result<Vec<FileDiff>> {
    let selector = format!("stash@{{{index}}}");
    // The `-c` options pin the `a/` `b/` prefixes libgit2's patch parser
    // expects, regardless of the user's diff.noprefix / mnemonicPrefix.
    let base = [
        "-c",
        "diff.noprefix=false",
        "-c",
        "diff.mnemonicPrefix=false",
        "stash",
        "show",
        "-p",
        "--no-color",
        "--no-ext-diff",
    ];
    let mut with_untracked: Vec<&str> = base.to_vec();
    with_untracked.push("--include-untracked");
    with_untracked.push(&selector);
    let out = match git_output(root, &with_untracked) {
        Ok(out) => out,
        Err(_) => {
            // `--include-untracked` needs git >= 2.32; retry without it.
            let mut plain: Vec<&str> = base.to_vec();
            plain.push(&selector);
            git_output(root, &plain)?
        }
    };
    patch_to_files(&out)
}

/// Parse a unified diff into side-by-side `FileDiff`s via libgit2.
pub fn patch_to_files(patch: &[u8]) -> Result<Vec<FileDiff>> {
    if patch.iter().all(|b| b.is_ascii_whitespace()) {
        return Ok(Vec::new());
    }
    let diff = git2::Diff::from_buffer(patch)?;
    files_from_diff(&diff)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::domain::diff::{FileStatus, LineType};

    #[test]
    fn parses_stash_list() {
        let text = "stash@{0}\x1fWIP on feat/x: 8762b49 Address review feedback\x1f7 months ago\x1fe643fb63\n\
                    stash@{1}\x1fOn main: wip: greeting tweak\x1f2 days ago\x1fb4c61e18\n\
                    stash@{2}\x1fautostash\x1fjust now\x1fdeadbeef\n";
        let list = parse_stash_list(text);
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].index, 0);
        assert_eq!(list[0].name(), "stash@{0}");
        assert_eq!(list[0].branch.as_deref(), Some("feat/x"));
        assert_eq!(list[0].message, "8762b49 Address review feedback");
        assert_eq!(list[0].relative_date, "7 months ago");
        assert_eq!(list[0].hash, "e643fb63");

        assert_eq!(list[1].index, 1);
        assert_eq!(list[1].branch.as_deref(), Some("main"));
        assert_eq!(list[1].message, "wip: greeting tweak");

        assert_eq!(list[2].branch, None);
        assert_eq!(list[2].message, "autostash");
        assert!(parse_stash_list("").is_empty());
    }

    #[test]
    fn splits_subject_variants() {
        assert_eq!(
            split_subject("WIP on main: abc1234 initial"),
            (Some("main".to_string()), "abc1234 initial".to_string())
        );
        assert_eq!(
            split_subject("On feat/a: msg: with colon"),
            (Some("feat/a".to_string()), "msg: with colon".to_string())
        );
        assert_eq!(
            split_subject("On (no branch): detached work"),
            (Some("(no branch)".to_string()), "detached work".to_string())
        );
        assert_eq!(split_subject("custom"), (None, "custom".to_string()));
    }

    #[test]
    fn patch_buffer_becomes_side_by_side_files() {
        // Built line by line: a `\`-continued string literal would strip
        // the leading space of context lines.
        let patch = concat!(
            "diff --git a/src/lib.rs b/src/lib.rs\n",
            "index 1111111..2222222 100644\n",
            "--- a/src/lib.rs\n",
            "+++ b/src/lib.rs\n",
            "@@ -1,3 +1,3 @@\n",
            " fn a() {}\n",
            "-fn b() {}\n",
            "+fn b() { todo!() }\n",
            " fn c() {}\n",
            "diff --git a/notes.md b/notes.md\n",
            "new file mode 100644\n",
            "index 0000000..3333333\n",
            "--- /dev/null\n",
            "+++ b/notes.md\n",
            "@@ -0,0 +1 @@\n",
            "+hello\n",
        );
        let files = patch_to_files(patch.as_bytes()).unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "src/lib.rs");
        assert_eq!(files[0].status, FileStatus::Modified);
        let rows = &files[0].hunks[0].rows;
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1].line_type, LineType::Deleted);
        assert_eq!(rows[1].left.as_ref().unwrap().content, "fn b() {}");
        assert_eq!(
            rows[1].right.as_ref().unwrap().content,
            "fn b() { todo!() }"
        );
        assert_eq!(files[1].path, "notes.md");
        assert_eq!(files[1].status, FileStatus::Added);
        assert_eq!(files[1].hunks[0].rows[0].line_type, LineType::Added);

        assert!(patch_to_files(b"\n").unwrap().is_empty());
    }
}
