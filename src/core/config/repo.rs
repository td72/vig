//! The repository-local config layer: a personal `.vig.kdl` at the worktree
//! root, merged on top of the user config (see the guide's Configuration
//! Basics chapter, `docs/guide/src/configuration-basics.md`).
//!
//! Trust is decided by git tracking: an **untracked** `.vig.kdl` is the
//! user's own file and loads silently; a **tracked** one is repo-provided
//! and needs an explicit decision (asked once per content, remembered in the
//! [`trust`](super::trust) store). Errors in this layer never abort startup:
//! the caller keeps the builtin + user config and reports the reason.

use crate::core::config::loader::Config;
use crate::core::config::trust::{TrustDecision, TrustStore};
use anyhow::Result;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// File name of the repo-local layer, looked up at the worktree root.
pub const REPO_CONFIG_FILE: &str = ".vig.kdl";

/// SHA-256 (hex) of the file content; the second half of the trust key.
pub fn content_hash(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for b in digest {
        use std::fmt::Write;
        let _ = write!(hex, "{b:02x}");
    }
    hex
}

/// Whether git tracks the repo-local `.vig.kdl`. `Unknown` — git could not
/// run or died abnormally, so the question has no answer — **fails closed**:
/// it is handled like a tracked file, so an undeterminable `.vig.kdl` goes
/// through the trust dialog instead of loading silently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tracked {
    Yes,
    No,
    Unknown,
}

/// Whether git tracks `.vig.kdl` in `workdir` (tracked = repo-provided),
/// asked via `git ls-files --error-unmatch`. See [`Tracked`] for how an
/// unclear answer is handled.
pub fn is_tracked(workdir: &Path) -> Tracked {
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(workdir)
        .args(["ls-files", "--error-unmatch", REPO_CONFIG_FILE])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    interpret_ls_files(status)
}

/// `git ls-files --error-unmatch` exits 0 for a tracked file and 1 for an
/// untracked one (`error: pathspec ... did not match`); real failures exit
/// differently (`fatal:` is 128) or never spawn at all. Only the definitive
/// exit codes answer the question — everything else is [`Tracked::Unknown`].
fn interpret_ls_files(status: std::io::Result<std::process::ExitStatus>) -> Tracked {
    match status {
        Ok(s) if s.success() => Tracked::Yes,
        Ok(s) if s.code() == Some(1) => Tracked::No,
        _ => Tracked::Unknown,
    }
}

/// What startup (and `vig config path`) should do about the repo layer.
#[derive(Debug)]
pub enum RepoLayer {
    /// No readable `.vig.kdl` at the worktree root.
    Absent { path: PathBuf },
    /// The user config says `repo-config "off"`: no load, no dialog.
    Disabled { path: PathBuf },
    /// Untracked (the user's own file) or trusted earlier: merge it.
    Load { path: PathBuf, text: String },
    /// Tracked and remembered as ignored: skip silently.
    Declined { path: PathBuf },
    /// Tracked with no decision for this content: ask before the app is built.
    Undecided {
        path: PathBuf,
        text: String,
        hash: String,
    },
}

/// Classify the repo layer for `workdir`. `tracked` answers "does git track
/// `.vig.kdl` here?" and is injected so tests need no real repository. Only
/// a definitive [`Tracked::No`] loads silently — [`Tracked::Unknown`] fails
/// closed onto the tracked (dialog) path.
pub fn classify(
    workdir: &Path,
    repo_config_enabled: bool,
    store: &TrustStore,
    tracked: impl FnOnce(&Path) -> Tracked,
) -> RepoLayer {
    let path = workdir.join(REPO_CONFIG_FILE);
    if !repo_config_enabled {
        return RepoLayer::Disabled { path };
    }
    let Ok(text) = std::fs::read_to_string(&path) else {
        return RepoLayer::Absent { path };
    };
    if tracked(workdir) == Tracked::No {
        return RepoLayer::Load { path, text };
    }
    let hash = content_hash(&text);
    match store.decision(workdir, &hash) {
        Some(TrustDecision::Load) => RepoLayer::Load { path, text },
        Some(TrustDecision::Ignore) => RepoLayer::Declined { path },
        None => RepoLayer::Undecided { path, text, hash },
    }
}

/// Parse `text` and merge it over `cfg` as the repo-local layer. On `Err`
/// the caller keeps `cfg` — this layer degrades, it never aborts.
pub fn apply(cfg: &Config, path: &Path, text: &str) -> Result<Config> {
    let doc = crate::core::config::source::parse_user_kdl(text, path)?;
    cfg.with_repo_layer(&doc, path.to_path_buf())
}

/// One status-bar line for a repo-layer error: the flattened error chain,
/// first line only (the full error goes to stderr).
pub fn summarize(err: &anyhow::Error) -> String {
    let chain = format!("{err:#}");
    chain.lines().next().unwrap_or("error").to_string()
}

/// The status column of the repo-local line in `vig config path`.
/// `apply_error` is the failure summary when a loadable layer did not merge.
pub fn status_text(layer: &RepoLayer, apply_error: Option<&str>) -> String {
    match layer {
        RepoLayer::Absent { .. } => "not found".to_string(),
        RepoLayer::Disabled { .. } => "ignored (repo-config \"off\")".to_string(),
        RepoLayer::Declined { .. } => {
            "ignored (trust declined; see `vig config trust`)".to_string()
        }
        RepoLayer::Undecided { .. } => "pending trust decision (start vig to decide)".to_string(),
        RepoLayer::Load { .. } => match apply_error {
            None => "loaded".to_string(),
            Some(e) => format!("ignored ({e})"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_worktree(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("vig-repo-test-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn content_hash_is_sha256_hex() {
        // sha256("") is a well-known constant.
        assert_eq!(
            content_hash(""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_ne!(content_hash("a"), content_hash("b"));
    }

    #[test]
    fn classify_absent_disabled_untracked() {
        let dir = tmp_worktree("classify");
        let store = TrustStore::default();
        assert!(matches!(
            classify(&dir, true, &store, |_| Tracked::No),
            RepoLayer::Absent { .. }
        ));
        std::fs::write(dir.join(REPO_CONFIG_FILE), "theme \"InspiredGitHub\"\n").unwrap();
        // Kill switch wins over everything, dialog included.
        assert!(matches!(
            classify(&dir, false, &store, |_| Tracked::Yes),
            RepoLayer::Disabled { .. }
        ));
        // Untracked file: the user's own, loaded silently.
        match classify(&dir, true, &store, |_| Tracked::No) {
            RepoLayer::Load { text, .. } => assert!(text.contains("InspiredGitHub")),
            other => panic!("expected Load, got {other:?}"),
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn classify_tracked_consults_trust_store() {
        let dir = tmp_worktree("tracked");
        let text = "theme \"InspiredGitHub\"\n";
        std::fs::write(dir.join(REPO_CONFIG_FILE), text).unwrap();
        let hash = content_hash(text);

        let mut store = TrustStore::default();
        assert!(matches!(
            classify(&dir, true, &store, |_| Tracked::Yes),
            RepoLayer::Undecided { .. }
        ));

        store.remember(&dir, &hash, TrustDecision::Load);
        assert!(matches!(
            classify(&dir, true, &store, |_| Tracked::Yes),
            RepoLayer::Load { .. }
        ));

        store.remember(&dir, &hash, TrustDecision::Ignore);
        assert!(matches!(
            classify(&dir, true, &store, |_| Tracked::Yes),
            RepoLayer::Declined { .. }
        ));

        // Content changed since the decision: ask again.
        std::fs::write(dir.join(REPO_CONFIG_FILE), "theme \"Solarized (dark)\"\n").unwrap();
        assert!(matches!(
            classify(&dir, true, &store, |_| Tracked::Yes),
            RepoLayer::Undecided { .. }
        ));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn undeterminable_tracking_fails_closed() {
        let dir = tmp_worktree("unknown");
        let text = "theme \"InspiredGitHub\"\n";
        std::fs::write(dir.join(REPO_CONFIG_FILE), text).unwrap();
        let store = TrustStore::default();
        // Unknown is handled like tracked: dialog path, never a silent load.
        assert!(matches!(
            classify(&dir, true, &store, |_| Tracked::Unknown),
            RepoLayer::Undecided { .. }
        ));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn spawn_failure_is_undeterminable() {
        assert_eq!(
            interpret_ls_files(Err(std::io::Error::other("git missing"))),
            Tracked::Unknown
        );
    }

    #[cfg(unix)]
    #[test]
    fn interpret_ls_files_distinguishes_untracked_from_failure() {
        use std::os::unix::process::ExitStatusExt;
        let exit = |code: i32| Ok(std::process::ExitStatus::from_raw(code << 8));
        assert_eq!(interpret_ls_files(exit(0)), Tracked::Yes);
        // Exit 1 is git's definitive "untracked".
        assert_eq!(interpret_ls_files(exit(1)), Tracked::No);
        // `fatal:` (128) or death by signal: undeterminable, fail closed.
        assert_eq!(interpret_ls_files(exit(128)), Tracked::Unknown);
        assert_eq!(
            interpret_ls_files(Ok(std::process::ExitStatus::from_raw(9))),
            Tracked::Unknown
        );
    }

    #[test]
    fn apply_error_degrades_and_names_the_file() {
        let cfg = Config::builtin();
        let err = apply(&cfg, Path::new("/w/.vig.kdl"), "theme \"no-such-theme\"").unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("/w/.vig.kdl"), "{msg}");
        // The base config is untouched and still usable.
        assert!(cfg.theme().is_ok());
        let summary = summarize(&err);
        assert_eq!(summary.lines().count(), 1);
    }

    #[test]
    fn status_text_covers_every_outcome() {
        let p = PathBuf::from("/w/.vig.kdl");
        let load = RepoLayer::Load {
            path: p.clone(),
            text: String::new(),
        };
        assert_eq!(status_text(&load, None), "loaded");
        assert_eq!(status_text(&load, Some("boom")), "ignored (boom)");
        assert_eq!(
            status_text(&RepoLayer::Absent { path: p.clone() }, None),
            "not found"
        );
        assert!(status_text(&RepoLayer::Disabled { path: p.clone() }, None)
            .contains("repo-config \"off\""));
        assert!(
            status_text(&RepoLayer::Declined { path: p.clone() }, None).contains("trust declined")
        );
        assert!(status_text(
            &RepoLayer::Undecided {
                path: p,
                text: String::new(),
                hash: String::new()
            },
            None
        )
        .contains("pending"));
    }
}
