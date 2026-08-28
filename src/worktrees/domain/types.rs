//! Plain data for the Worktrees page: worktrees, stash entries and the
//! HEAD commit summary shown for a worktree.

use std::path::PathBuf;

/// One entry of `git worktree list --porcelain`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    /// Absolute path as reported by git.
    pub path: PathBuf,
    /// Path shown in the list: the directory name for the main worktree,
    /// a path relative to it for the others (see `worktree::display_path`).
    pub display_path: String,
    /// Full HEAD hash (`None` for a bare entry without a resolvable HEAD).
    pub head: Option<String>,
    /// Checked-out branch (short name), `None` when detached or bare.
    pub branch: Option<String>,
    pub detached: bool,
    pub bare: bool,
    /// `Some(reason)` when locked; the reason may be empty.
    pub locked: Option<String>,
    /// `Some(reason)` when git considers the entry prunable.
    pub prunable: Option<String>,
    /// The first entry of the listing (the main worktree).
    pub is_main: bool,
    /// The worktree vig was started in.
    pub is_current: bool,
}

impl Worktree {
    pub fn short_head(&self) -> &str {
        match &self.head {
            Some(h) => &h[..h.len().min(7)],
            None => "",
        }
    }

    /// What is checked out: the branch name, `(detached abc1234)` or `(bare)`.
    pub fn ref_label(&self) -> String {
        if let Some(b) = &self.branch {
            return b.clone();
        }
        if self.bare {
            return "(bare)".to_string();
        }
        if self.head.is_some() {
            return format!("(detached {})", self.short_head());
        }
        String::new()
    }

    /// Status flags in display order.
    pub fn flags(&self) -> Vec<&'static str> {
        let mut flags = Vec::new();
        if self.is_main {
            flags.push("main");
        }
        if self.bare {
            flags.push("bare");
        }
        if self.locked.is_some() {
            flags.push("locked");
        }
        if self.prunable.is_some() {
            flags.push("prunable");
        }
        flags
    }
}

/// One entry of `git stash list`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stash {
    /// Position in the stash list (`stash@{index}`).
    pub index: usize,
    /// Branch the stash was created on, when git recorded it.
    pub branch: Option<String>,
    /// Message without the `WIP on <branch>:` / `On <branch>:` prefix.
    pub message: String,
    /// Relative commit time (`%cr`), e.g. `3 days ago`.
    pub relative_date: String,
    /// Commit hash of the stash entry.
    pub hash: String,
}

impl Stash {
    pub fn name(&self) -> String {
        format!("stash@{{{}}}", self.index)
    }
}

/// The HEAD commit of a worktree, as shown in the preview pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitSummary {
    pub hash: String,
    pub author_name: String,
    pub author_email: String,
    pub date: String,
    pub relative_date: String,
    pub subject: String,
    /// `git show --stat` lines (one per changed file plus the totals line).
    pub stat: Vec<String>,
}

impl CommitSummary {
    pub fn short_hash(&self) -> &str {
        &self.hash[..self.hash.len().min(7)]
    }
}
