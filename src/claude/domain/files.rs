//! The files a session touched, from its recent tool calls.
//!
//! `iktinos` models the calls; which files they add up to is this page's
//! own question. Only the tools that name one file count: what a shell
//! command touched is not known.

use super::types::{Touch, TouchedFile};
use iktinos::record::Timestamp;
use iktinos::session::Session;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The files `session` read, edited or wrote, the latest touch first.
/// `roots` are the repository's worktree roots: a file under one of them
/// is shown relative to it.
pub fn touched(session: &Session, roots: &[PathBuf]) -> Vec<TouchedFile> {
    let mut by_path: HashMap<String, TouchedFile> = HashMap::new();
    for tool in &session.recent_tools {
        let Some(touch) = Touch::of_tool(&tool.name) else {
            continue;
        };
        // A summary that was cut is not a path any more.
        let Some(path) = tool.summary.as_deref().filter(|p| !p.ends_with('…')) else {
            continue;
        };
        let path = display_path(path, roots);
        let at = tool.started_at.map(Timestamp::as_millis);
        let file = by_path.entry(path.clone()).or_insert(TouchedFile {
            path,
            touch,
            count: 0,
            last_ms: None,
        });
        file.touch = file.touch.max(touch);
        file.count += 1;
        file.last_ms = file.last_ms.max(at);
    }
    let mut files: Vec<TouchedFile> = by_path.into_values().collect();
    files.sort_by(|a, b| b.last_ms.cmp(&a.last_ms).then_with(|| a.path.cmp(&b.path)));
    files
}

/// `path` relative to the root it is under, `~/…` under the home
/// directory, or as it is.
pub fn display_path(path: &str, roots: &[PathBuf]) -> String {
    let full = Path::new(path);
    roots
        .iter()
        .filter_map(|root| full.strip_prefix(root).ok())
        .filter(|rel| !rel.as_os_str().is_empty())
        // The deepest root wins (a worktree nested in the repository).
        .min_by_key(|rel| rel.as_os_str().len())
        .map_or_else(
            || home_relative(full),
            |rel| rel.to_string_lossy().into_owned(),
        )
}

fn home_relative(path: &Path) -> String {
    dirs::home_dir()
        .and_then(|home| {
            path.strip_prefix(home)
                .ok()
                .map(|rel| format!("~/{}", rel.display()))
        })
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use iktinos::record::Record;
    use iktinos::session::SessionStore;

    fn tool_use(n: u32, name: &str, input: &str) -> String {
        format!(
            r#"{{"type":"assistant","uuid":"a{n}","timestamp":"2026-11-02T10:00:{n:02}.000Z","message":{{"id":"m{n}","model":"claude-fable-5-2","content":[{{"type":"tool_use","id":"t{n}","name":"{name}","input":{input}}}]}}}}"#
        )
    }

    #[test]
    fn files_keep_the_strongest_touch_and_are_relative_to_the_repository() {
        let long = format!("/repo/{}", "x".repeat(200));
        let lines = [
            tool_use(1, "Read", r#"{"file_path":"/repo/src/a.rs"}"#),
            tool_use(2, "Edit", r#"{"file_path":"/repo/src/a.rs"}"#),
            tool_use(3, "Read", r#"{"file_path":"/repo/src/a.rs"}"#),
            tool_use(4, "Write", r#"{"file_path":"/elsewhere/b.txt"}"#),
            tool_use(5, "Bash", r#"{"command":"cat /repo/c"}"#),
            tool_use(6, "Read", &format!(r#"{{"file_path":"{long}"}}"#)),
        ];
        let mut store = SessionStore::new();
        for line in &lines {
            store.apply("s1", &Record::parse(line));
        }
        let files = touched(store.get("s1").unwrap(), &[PathBuf::from("/repo")]);
        let files: Vec<(&str, Touch, usize)> = files
            .iter()
            .map(|f| (f.path.as_str(), f.touch, f.count))
            .collect();
        // Latest touch first; the cut path and the Bash call are not files.
        assert_eq!(
            files,
            [
                ("/elsewhere/b.txt", Touch::Write, 1),
                ("src/a.rs", Touch::Edit, 3)
            ]
        );
    }

    #[test]
    fn a_path_is_shown_from_the_deepest_root() {
        let roots = [PathBuf::from("/repo"), PathBuf::from("/repo/wt/feature")];
        assert_eq!(
            display_path("/repo/wt/feature/src/a.rs", &roots),
            "src/a.rs"
        );
        assert_eq!(display_path("/repo/b.rs", &roots), "b.rs");
        assert_eq!(display_path("/other/c.rs", &roots), "/other/c.rs");
    }
}
