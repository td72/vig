//! Remembered trust decisions for repository-local `.vig.kdl` files.
//!
//! A tracked `.vig.kdl` is repo-provided, so vig asks before loading it and
//! remembers the answer here, keyed by **(worktree path, content hash)**:
//! when the file's content changes (e.g. on pull), the old decision no
//! longer applies and the dialog is shown again.
//!
//! The store lives at `$XDG_STATE_HOME/vig/trust.json`, falling back to
//! `~/.local/state/vig/trust.json`.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// What the user answered in the trust dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrustDecision {
    /// `[y]`: merge the repo-local layer.
    Load,
    /// `[n]`: start without it.
    Ignore,
}

impl std::fmt::Display for TrustDecision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrustDecision::Load => write!(f, "load"),
            TrustDecision::Ignore => write!(f, "ignore"),
        }
    }
}

/// One remembered decision. At most one entry exists per worktree path; a
/// new decision for the same worktree replaces the old one.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustEntry {
    /// Worktree root the `.vig.kdl` sits in.
    pub path: String,
    /// SHA-256 (hex) of the file content the decision was made for.
    pub hash: String,
    pub decision: TrustDecision,
    /// Unix timestamp (seconds) of the decision.
    pub decided_at: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct TrustStore {
    pub entries: Vec<TrustEntry>,
}

impl TrustStore {
    /// `$XDG_STATE_HOME/vig/trust.json` or `~/.local/state/vig/trust.json`.
    pub fn default_path() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_STATE_HOME")
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .or_else(|| dirs::home_dir().map(|h| h.join(".local").join("state")))?;
        Some(base.join("vig").join("trust.json"))
    }

    /// Load from `path`. A missing or corrupt file is an empty store — a
    /// broken state file must never prevent vig from starting.
    pub fn load_from(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Load from [`default_path`](Self::default_path) (empty when there is none).
    pub fn load_default() -> Self {
        Self::default_path()
            .map(|p| Self::load_from(&p))
            .unwrap_or_default()
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("cannot create state directory {}", dir.display()))?;
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)
            .with_context(|| format!("cannot write trust store {}", path.display()))
    }

    /// Save to [`default_path`](Self::default_path).
    pub fn save_default(&self) -> Result<()> {
        let path = Self::default_path()
            .context("cannot save trust store: home directory could not be determined")?;
        self.save_to(&path)
    }

    /// The remembered decision for this worktree, only when it was made for
    /// exactly this content hash — a changed file needs a fresh decision.
    pub fn decision(&self, worktree: &Path, hash: &str) -> Option<TrustDecision> {
        let key = normalize_key(worktree);
        self.entries
            .iter()
            .find(|e| e.path == key && e.hash == hash)
            .map(|e| e.decision)
    }

    /// Remember `decision` for `(worktree, hash)`, replacing any earlier
    /// entry for the same worktree.
    pub fn remember(&mut self, worktree: &Path, hash: &str, decision: TrustDecision) {
        let key = normalize_key(worktree);
        self.entries.retain(|e| e.path != key);
        self.entries.push(TrustEntry {
            path: key,
            hash: hash.to_string(),
            decision,
            decided_at: now_unix(),
        });
    }

    /// Drop the entry for `worktree` (as listed by `vig config trust`).
    /// `false` when nothing was remembered for it.
    pub fn forget(&mut self, worktree: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.path != worktree);
        self.entries.len() != before
    }
}

/// The store key for a worktree path. `components()` drops a trailing slash
/// (git2 workdirs end in one) and collapses `.` segments, so lookup,
/// remember and `--forget` agree. Idempotent: normalizing a key the store
/// already holds (e.g. one copied from the `vig config trust` listing)
/// yields that same key.
pub fn normalize_key(worktree: &Path) -> String {
    worktree
        .components()
        .as_path()
        .to_string_lossy()
        .into_owned()
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// `"YYYY-MM-DD HH:MM UTC"` for a unix timestamp (for `vig config trust`).
pub fn format_unix(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02} UTC",
        rem / 3600,
        (rem % 3600) / 60
    )
}

/// Gregorian (year, month, day) for a day count since 1970-01-01
/// (Howard Hinnant's `civil_from_days`).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (y + i64::from(m <= 2), m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_file(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("vig-trust-test-{}-{name}", std::process::id()))
    }

    #[test]
    fn roundtrip() {
        let path = tmp_file("roundtrip.json");
        let mut store = TrustStore::default();
        store.remember(Path::new("/w/a"), "abc123", TrustDecision::Load);
        store.remember(Path::new("/w/b"), "def456", TrustDecision::Ignore);
        store.save_to(&path).unwrap();
        let loaded = TrustStore::load_from(&path);
        std::fs::remove_file(&path).ok();
        assert_eq!(
            loaded.decision(Path::new("/w/a"), "abc123"),
            Some(TrustDecision::Load)
        );
        assert_eq!(
            loaded.decision(Path::new("/w/b"), "def456"),
            Some(TrustDecision::Ignore)
        );
        assert!(loaded.entries.iter().all(|e| e.decided_at > 0));
    }

    #[test]
    fn changed_hash_invalidates_decision() {
        let mut store = TrustStore::default();
        store.remember(Path::new("/w/a"), "oldhash", TrustDecision::Load);
        assert_eq!(store.decision(Path::new("/w/a"), "newhash"), None);
        // A fresh decision replaces the stale entry instead of piling up.
        store.remember(Path::new("/w/a"), "newhash", TrustDecision::Ignore);
        assert_eq!(store.entries.len(), 1);
        assert_eq!(
            store.decision(Path::new("/w/a"), "newhash"),
            Some(TrustDecision::Ignore)
        );
        assert_eq!(store.decision(Path::new("/w/a"), "oldhash"), None);
    }

    #[test]
    fn corrupt_or_missing_store_is_empty() {
        let path = tmp_file("corrupt.json");
        std::fs::write(&path, "{ not json !!").unwrap();
        let store = TrustStore::load_from(&path);
        std::fs::remove_file(&path).ok();
        assert!(store.entries.is_empty());
        assert!(TrustStore::load_from(Path::new("/nonexistent/trust.json"))
            .entries
            .is_empty());
    }

    #[test]
    fn trailing_slash_and_plain_paths_share_one_key() {
        let mut store = TrustStore::default();
        // git2 workdirs come with a trailing slash.
        store.remember(Path::new("/w/a/"), "h1", TrustDecision::Load);
        assert_eq!(
            store.decision(Path::new("/w/a"), "h1"),
            Some(TrustDecision::Load)
        );
        assert_eq!(store.entries[0].path, "/w/a");
        assert!(store.forget("/w/a"));
    }

    #[test]
    fn forget_by_normalized_listing_output_always_matches() {
        let mut store = TrustStore::default();
        // git2 hands us a trailing slash; the listing prints the stored key.
        store.remember(Path::new("/w/a/"), "h1", TrustDecision::Load);
        let listed = store.entries[0].path.clone();
        // Normalizing a verbatim copy of the listing is a no-op...
        assert_eq!(normalize_key(Path::new(&listed)), listed);
        // ...so forgetting by it always matches, canonicalize() not needed.
        assert!(store.forget(&normalize_key(Path::new(&listed))));
        // A trailing slash typed by hand normalizes to the same key too.
        store.remember(Path::new("/w/a"), "h1", TrustDecision::Load);
        assert!(store.forget(&normalize_key(Path::new("/w/a/"))));
        assert!(store.entries.is_empty());
    }

    #[test]
    fn forget_removes_one_entry() {
        let mut store = TrustStore::default();
        store.remember(Path::new("/w/a"), "h1", TrustDecision::Load);
        store.remember(Path::new("/w/b"), "h2", TrustDecision::Load);
        assert!(store.forget("/w/a"));
        assert!(!store.forget("/w/a"), "already forgotten");
        assert_eq!(store.entries.len(), 1);
        assert_eq!(store.entries[0].path, "/w/b");
    }

    #[test]
    fn format_unix_is_utc_civil() {
        assert_eq!(format_unix(0), "1970-01-01 00:00 UTC");
        assert_eq!(format_unix(951_827_640), "2000-02-29 12:34 UTC");
    }
}
