use crate::git::diff::{compute_stats, parse_diff, DiffState};
use anyhow::{Context, Result};
use git2::Repository;
use std::path::Path;

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

    pub fn diff_workdir(&self) -> Result<DiffState> {
        let files = parse_diff(&self.inner)?;
        let stats = compute_stats(&files);
        let branch_name = self.branch_name();
        Ok(DiffState {
            files,
            branch_name,
            stats,
        })
    }

    #[allow(dead_code)]
    pub fn inner(&self) -> &Repository {
        &self.inner
    }
}
