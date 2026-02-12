use crate::git::diff::{compute_stats, parse_diff, DiffState};
use anyhow::{Context, Result};
use git2::Repository;
use std::path::Path;

pub struct BranchInfo {
    pub name: String,
    pub is_head: bool,
}

pub struct CommitInfo {
    pub short_hash: String,
    pub author: String,
    pub date: String,
    pub message: String,
}

pub struct ReflogEntry {
    pub short_hash: String,
    pub full_hash: String,
    pub selector: String,
    pub action: String,
    pub message: String,
}

pub struct Repo {
    inner: Repository,
}

impl Repo {
    pub fn discover(path: &Path) -> Result<Self> {
        let repo = Repository::discover(path).context("Not a git repository")?;
        Ok(Self { inner: repo })
    }

    pub fn workdir(&self) -> &Path {
        self.inner
            .workdir()
            .unwrap_or_else(|| self.inner.path())
    }

    pub fn branch_name(&self) -> String {
        self.inner
            .head()
            .ok()
            .and_then(|r| {
                if r.is_branch() {
                    r.shorthand().map(|s| s.to_string())
                } else {
                    // Detached HEAD — show short hash
                    r.target()
                        .map(|oid| format!("{:.7}", oid))
                }
            })
            .unwrap_or_else(|| "HEAD".to_string())
    }

    pub fn diff_workdir(&self, base_ref: Option<&str>) -> Result<DiffState> {
        let files = parse_diff(&self.inner, base_ref)?;
        let stats = compute_stats(&files);
        let branch_name = self.branch_name();
        Ok(DiffState {
            files,
            branch_name,
            stats,
        })
    }

    pub fn list_local_branches(&self) -> Vec<BranchInfo> {
        let head_name = self.branch_name();
        let mut branches: Vec<BranchInfo> =
            match self.inner.branches(Some(git2::BranchType::Local)) {
                Ok(iter) => iter
                    .filter_map(|b| b.ok())
                    .filter_map(|(branch, _)| {
                        branch.name().ok().flatten().map(|name| BranchInfo {
                            name: name.to_string(),
                            is_head: name == head_name,
                        })
                    })
                    .collect(),
                Err(_) => Vec::new(),
            };
        branches.sort_by(|a, b| match (a.is_head, b.is_head) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.cmp(&b.name),
        });
        branches
    }

    pub fn log_for_ref(&self, ref_name: &str, limit: usize) -> Vec<CommitInfo> {
        let obj = match self.inner.revparse_single(ref_name) {
            Ok(obj) => obj,
            Err(_) => return Vec::new(),
        };
        let mut revwalk = match self.inner.revwalk() {
            Ok(rw) => rw,
            Err(_) => return Vec::new(),
        };
        if revwalk.push(obj.id()).is_err() {
            return Vec::new();
        }
        let _ = revwalk.set_sorting(git2::Sort::TIME);

        let mut commits = Vec::new();
        for oid in revwalk.take(limit) {
            let oid = match oid {
                Ok(o) => o,
                Err(_) => break,
            };
            let commit = match self.inner.find_commit(oid) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let hash_str = oid.to_string();
            let short_hash = hash_str[..7.min(hash_str.len())].to_string();
            let author = commit
                .author()
                .name()
                .unwrap_or("unknown")
                .to_string();
            let date = epoch_to_date(commit.time().seconds());
            let message = commit.summary().unwrap_or("").to_string();
            commits.push(CommitInfo {
                short_hash,
                author,
                date,
                message,
            });
        }
        commits
    }

    /// Switch to the given branch using `git switch`.
    pub fn switch_branch(&self, name: &str) -> Result<()> {
        let workdir = self.workdir();
        let output = std::process::Command::new("git")
            .arg("switch")
            .arg(name)
            .current_dir(workdir)
            .output()
            .context("Failed to run git switch")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("git switch failed: {}", stderr.trim());
        }
        Ok(())
    }

    /// Delete the given branch using `git branch -d` (safe delete only).
    pub fn delete_branch(&self, name: &str) -> Result<()> {
        let workdir = self.workdir();
        let output = std::process::Command::new("git")
            .arg("branch")
            .arg("-d")
            .arg(name)
            .current_dir(workdir)
            .output()
            .context("Failed to run git branch -d")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("git branch -d failed: {}", stderr.trim());
        }
        Ok(())
    }

    pub fn reflog(&self, limit: usize) -> Vec<ReflogEntry> {
        let reflog = match self.inner.reflog("HEAD") {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };
        reflog
            .iter()
            .take(limit)
            .enumerate()
            .map(|(i, entry)| {
                let oid = entry.id_new();
                let full_hash = oid.to_string();
                let short_hash = full_hash[..7.min(full_hash.len())].to_string();
                let selector = format!("HEAD@{{{i}}}");
                let raw_message = entry.message().unwrap_or("").to_string();
                let (action, message) = match raw_message.split_once(": ") {
                    Some((a, m)) => (a.to_string(), m.to_string()),
                    None => (raw_message.clone(), raw_message),
                };
                ReflogEntry {
                    short_hash,
                    full_hash,
                    selector,
                    action,
                    message,
                }
            })
            .collect()
    }

    #[allow(dead_code)]
    pub fn inner(&self) -> &Repository {
        &self.inner
    }
}

fn epoch_to_date(epoch: i64) -> String {
    // Howard Hinnant's civil_from_days algorithm
    let z = (epoch / 86400) as i32 + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i32 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}
