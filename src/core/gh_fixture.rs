//! Replay of recorded `gh` output for demo recordings and integration
//! tests, so neither needs the network or spends GitHub API points.
//!
//! Recording-only and undocumented for users (like `VIG_PROCS_ROOT_PID`):
//! with `VIG_GH_FIXTURE=<dir>` every `gh` invocation reads the file named
//! after its arguments from `<dir>` instead of running; a missing file is
//! a loud error so a demo never half-works. `vig fixtures record <dir>`
//! runs the real commands and writes those files (see `src/fixtures.rs`).

use std::path::{Path, PathBuf};
use std::sync::RwLock;

#[derive(Default)]
struct Dirs {
    /// Read `gh` output from here (tests set it; the env var otherwise).
    replay: Option<PathBuf>,
    /// Tee real `gh` output into here.
    record: Option<PathBuf>,
}

static DIRS: RwLock<Dirs> = RwLock::new(Dirs {
    replay: None,
    record: None,
});

const ENV: &str = "VIG_GH_FIXTURE";

/// The replay directory: the programmatic override, else `$VIG_GH_FIXTURE`.
pub fn replay_dir() -> Option<PathBuf> {
    if let Some(dir) = DIRS.read().ok().and_then(|d| d.replay.clone()) {
        return Some(dir);
    }
    std::env::var_os(ENV)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// Whether `gh` output is replayed (disk caches are bypassed then, so a
/// recording always starts from the fixture state).
pub fn active() -> bool {
    replay_dir().is_some()
}

/// Replay from `dir` regardless of the environment (tests).
#[cfg(test)]
pub fn set_replay_dir(dir: Option<PathBuf>) {
    if let Ok(mut d) = DIRS.write() {
        d.replay = dir;
    }
}

/// Tee every real `gh` invocation's stdout into `dir`.
pub fn set_record_dir(dir: Option<PathBuf>) {
    if let Ok(mut d) = DIRS.write() {
        d.record = dir;
    }
}

fn record_dir() -> Option<PathBuf> {
    DIRS.read().ok().and_then(|d| d.record.clone())
}

/// The fixture file for a `gh` argument list: the arguments joined with
/// `_`, GraphQL query text replaced by a short tag, everything else
/// sanitised to `[A-Za-z0-9._-]` and cut to 150 chars. Deterministic, so
/// a replay asks for exactly what a recording wrote.
pub fn name_for(args: &[&str]) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(args.len());
    for arg in args {
        let part = match arg.strip_prefix("query=") {
            Some(q) => format!("query-{}", query_tag(q)),
            None => (*arg).to_string(),
        };
        let clean: String = part
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        parts.push(clean);
    }
    let mut name = parts.join("_");
    while name.contains("__") {
        name = name.replace("__", "_");
    }
    name.truncate(150);
    format!("{}.out", name.trim_matches('_'))
}

/// A short tag telling vig's GraphQL queries apart.
fn query_tag(query: &str) -> &'static str {
    if query.contains("pullRequests(states: OPEN") {
        "pr-stacks"
    } else if query.contains("items { totalCount }") {
        "project-meta"
    } else if query.contains("items(first: $first") {
        "project-items"
    } else if query.contains("views(first: 20)") {
        "project-views"
    } else if query.contains("rateLimit") {
        "rate-limit"
    } else {
        "graphql"
    }
}

/// The replayed stdout for `args`: `None` when replay is off, `Some(Ok)`
/// with the file's bytes, `Some(Err)` when the fixture is missing.
/// `gh auth status` needs no file: an existing fixture directory counts
/// as signed in.
pub fn replay(args: &[&str]) -> Option<Result<Vec<u8>, String>> {
    let dir = replay_dir()?;
    if args.first() == Some(&"auth") {
        return Some(if dir.is_dir() {
            Ok(Vec::new())
        } else {
            Err(format!("{ENV}: {} is not a directory", dir.display()))
        });
    }
    let path = dir.join(name_for(args));
    Some(std::fs::read(&path).map_err(|e| format!("{ENV}: no fixture {} ({e})", path.display())))
}

/// Write `stdout` as the fixture for `args` when recording is on.
pub fn record(args: &[&str], stdout: &[u8]) {
    let Some(dir) = record_dir() else {
        return;
    };
    write(&dir, args, stdout);
}

/// `tape/fixtures` of this checkout when it holds recordings (tests
/// against them skip otherwise, e.g. on a packaged source tree).
#[cfg(test)]
pub fn recorded_dir() -> Option<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tape/fixtures");
    dir.join("auth_status.out").exists().then_some(dir)
}

fn write(dir: &Path, args: &[&str], stdout: &[u8]) {
    let _ = std::fs::create_dir_all(dir);
    let _ = std::fs::write(dir.join(name_for(args)), stdout);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_deterministic_and_filesystem_safe() {
        let a = name_for(&["issue", "list", "--json", "number,title", "--limit", "50"]);
        assert_eq!(a, "issue_list_--json_number_title_--limit_50.out");
        assert_eq!(
            a,
            name_for(&["issue", "list", "--json", "number,title", "--limit", "50"])
        );
        let q = name_for(&[
            "api",
            "graphql",
            "-f",
            "query=query($login: String!) { user(login: $login) { projectV2 { items { totalCount } } } }",
            "-F",
            "login=td72",
        ]);
        assert_eq!(q, "api_graphql_-f_query-project-meta_-F_login_td72.out");
        assert!(
            name_for(&["api", "repos/{owner}/{repo}/actions/jobs/1/logs"]).contains("jobs_1_logs")
        );
        let long = "x".repeat(400);
        assert!(name_for(&[&long]).len() <= 154);
    }

    #[test]
    fn replay_reads_recordings_and_flags_missing_files() {
        let dir = std::env::temp_dir().join(format!("vig-fixture-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let args = ["issue", "view", "7", "--json", "title"];
        write(&dir, &args, b"{\"title\":\"seven\"}");
        // Through the file directly (the global replay dir is shared by
        // every test, so it is not touched here).
        let path = dir.join(name_for(&args));
        assert_eq!(std::fs::read(&path).unwrap(), b"{\"title\":\"seven\"}");
        assert!(!dir.join(name_for(&["issue", "view", "8"])).exists());
        let _ = std::fs::remove_dir_all(&dir);
        // Replay is off without a directory.
        assert!(replay_dir().is_none() || replay(&args).is_some());
    }
}
