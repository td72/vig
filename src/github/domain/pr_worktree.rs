//! `W` on a pull request: fetch its head and add a worktree for it.
//!
//! The only two commands vig runs here, both shown in the confirmation
//! dialog first:
//!
//! ```text
//! git fetch origin refs/pull/<n>/head
//! git worktree add <path> -b <branch> FETCH_HEAD
//! ```
//!
//! `refs/pull/<n>/head` exists for every pull request, forks included, so
//! there is one path for all of them. Neither command touches the current
//! checkout; vig never removes a worktree.

use crate::core::config::WorktreeSettings;
use crate::worktrees::domain::types::Worktree;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

/// Where a PR's worktree goes and what its branch is called.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreePlan {
    pub number: u64,
    /// Absolute (lexically normalised) path of the worktree.
    pub path: PathBuf,
    /// The path relative to the main worktree, as the dialog shows it.
    pub display_path: String,
    pub branch: String,
}

impl WorktreePlan {
    /// The plan for PR `number` under `settings`, for a repository whose
    /// main worktree is `main` (the directory name is `{repo}`).
    pub fn new(settings: &WorktreeSettings, main: &Path, number: u64) -> WorktreePlan {
        let repo = main
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "repo".to_string());
        let name = WorktreeSettings::expand(&settings.name, &repo, number);
        let branch = WorktreeSettings::expand(&settings.branch, &repo, number);
        let relative = Path::new(&settings.dir).join(&name);
        let path = normalize(&main.join(&relative));
        WorktreePlan {
            number,
            path,
            display_path: relative.to_string_lossy().into_owned(),
            branch,
        }
    }

    /// The commands `run` executes, as shown in the confirmation dialog.
    pub fn commands(&self) -> [String; 2] {
        [
            format!("git fetch origin refs/pull/{}/head", self.number),
            format!(
                "git worktree add {} -b {} FETCH_HEAD",
                self.display_path, self.branch
            ),
        ]
    }

    /// A worktree that already serves this PR: one checked out on the
    /// plan's branch, or one at the plan's path.
    pub fn existing<'a>(&self, worktrees: &'a [Worktree]) -> Option<&'a Worktree> {
        let path = canon(&self.path);
        worktrees
            .iter()
            .find(|w| w.branch.as_deref() == Some(self.branch.as_str()) || canon(&w.path) == path)
    }

    /// Why the worktree cannot be added: the directory or the branch
    /// already exists (without being a worktree of this PR).
    pub fn conflict(&self, root: &Path) -> Option<String> {
        if self.path.exists() {
            return Some(format!(
                "{} already exists (and is not a worktree of PR #{})",
                self.display_path, self.number
            ));
        }
        if branch_exists(root, &self.branch) {
            return Some(format!(
                "branch {} already exists (and is not checked out in a worktree)",
                self.branch
            ));
        }
        None
    }

    /// Fetch the PR's head and add the worktree; the worktree's path on
    /// success. Runs on a worker thread (a fetch takes a while).
    pub fn run(&self, root: &Path) -> Result<PathBuf, String> {
        git(
            root,
            &[
                "fetch",
                "origin",
                &format!("refs/pull/{}/head", self.number),
            ],
        )?;
        git(
            root,
            &[
                "worktree",
                "add",
                &self.path.to_string_lossy(),
                "-b",
                &self.branch,
                "FETCH_HEAD",
            ],
        )?;
        Ok(self.path.clone())
    }
}

fn branch_exists(root: &Path, branch: &str) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ])
        .status()
        .is_ok_and(|s| s.success())
}

/// Run `git <args>` in `root`; stderr becomes the error text.
fn git(root: &Path, args: &[&str]) -> Result<(), String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr = stderr.trim();
    Err(format!(
        "git {} failed{}",
        args.first().copied().unwrap_or(""),
        if stderr.is_empty() {
            String::new()
        } else {
            format!(": {stderr}")
        }
    ))
}

fn canon(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

/// Resolve `.` and `..` lexically (the path need not exist yet).
fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worktrees::domain::worktree::list_worktrees;

    fn settings() -> WorktreeSettings {
        crate::core::config::Config::builtin()
            .pr_worktree()
            .unwrap()
    }

    #[test]
    fn plan_expands_the_templates_next_to_the_main_worktree() {
        let plan = WorktreePlan::new(&settings(), Path::new("/work/vig"), 7);
        assert_eq!(plan.path, PathBuf::from("/work/vig-pr-7"));
        assert_eq!(plan.display_path, "../vig-pr-7");
        assert_eq!(plan.branch, "pr/7");
        assert_eq!(
            plan.commands(),
            [
                "git fetch origin refs/pull/7/head".to_string(),
                "git worktree add ../vig-pr-7 -b pr/7 FETCH_HEAD".to_string(),
            ]
        );
        let inside = WorktreeSettings {
            dir: ".worktrees".into(),
            name: "pr-{n}".into(),
            branch: "review/{repo}-{n}".into(),
        };
        let plan = WorktreePlan::new(&inside, Path::new("/work/vig"), 7);
        assert_eq!(plan.path, PathBuf::from("/work/vig/.worktrees/pr-7"));
        assert_eq!(plan.display_path, ".worktrees/pr-7");
        assert_eq!(plan.branch, "review/vig-7");
    }

    fn sh(dir: &Path, args: &[&str]) {
        let st = Command::new(args[0])
            .args(&args[1..])
            .current_dir(dir)
            .output()
            .expect("run");
        assert!(
            st.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&st.stderr)
        );
    }

    /// A local "origin" with `refs/pull/7/head`, and a clone of it (one
    /// directory per test: they run in parallel).
    fn repos(tag: &str) -> (PathBuf, PathBuf) {
        let base =
            std::env::temp_dir().join(format!("vig-pr-worktree-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let origin = base.join("origin");
        let main = base.join("vig");
        std::fs::create_dir_all(&origin).unwrap();
        sh(&origin, &["git", "init", "-q", "-b", "main"]);
        sh(&origin, &["git", "config", "user.email", "a@b.c"]);
        sh(&origin, &["git", "config", "user.name", "t"]);
        std::fs::write(origin.join("a.txt"), "a\n").unwrap();
        sh(&origin, &["git", "add", "-A"]);
        sh(&origin, &["git", "commit", "-q", "-m", "init"]);
        std::fs::write(origin.join("a.txt"), "a\nb\n").unwrap();
        sh(&origin, &["git", "commit", "-q", "-am", "pr"]);
        sh(&origin, &["git", "update-ref", "refs/pull/7/head", "HEAD"]);
        sh(&origin, &["git", "reset", "-q", "--hard", "HEAD~1"]);
        sh(
            &base,
            &["git", "clone", "-q", origin.to_str().unwrap(), "vig"],
        );
        (base, main)
    }

    #[test]
    fn run_fetches_the_pr_head_and_adds_the_worktree() {
        let (base, main) = repos("run");
        let plan = WorktreePlan::new(&settings(), &main, 7);
        assert_eq!(plan.path, base.join("vig-pr-7"));
        assert_eq!(plan.conflict(&main), None);
        let before = list_worktrees(&main).unwrap();
        assert!(plan.existing(&before).is_none());

        let path = plan.run(&main).unwrap();
        assert_eq!(path, plan.path);
        assert_eq!(
            std::fs::read_to_string(path.join("a.txt")).unwrap(),
            "a\nb\n",
            "the PR's head is checked out"
        );
        let after = list_worktrees(&main).unwrap();
        let wt = plan.existing(&after).expect("the new worktree is found");
        assert_eq!(wt.branch.as_deref(), Some("pr/7"));
        // The main checkout is untouched.
        assert_eq!(std::fs::read_to_string(main.join("a.txt")).unwrap(), "a\n");
        // Now both the directory and the branch exist: a conflict is reported
        // only when neither belongs to a worktree of this PR.
        assert!(plan.conflict(&main).is_some());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn conflict_names_an_existing_branch() {
        let (base, main) = repos("conflict");
        sh(&main, &["git", "branch", "pr/7"]);
        let plan = WorktreePlan::new(&settings(), &main, 7);
        let why = plan.conflict(&main).expect("conflict");
        assert!(why.contains("branch pr/7 already exists"), "{why}");
        assert!(plan.run(&main).is_err());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn normalize_resolves_parent_components() {
        assert_eq!(
            normalize(Path::new("/a/b/../c/./d")),
            PathBuf::from("/a/c/d")
        );
    }
}
