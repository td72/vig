use crate::core::app::Page;
use crate::git::state::GitState;
use anyhow::Result;
use std::path::{Path, PathBuf};

/// Create a Git page. Returns the page and the resolved workdir path.
pub fn new_page(cwd: &Path) -> Result<(Page, PathBuf)> {
    let git = GitState::new(cwd)?;
    let workdir = git.shared.repo.workdir().to_path_buf();
    let page = Page::new(git);
    Ok((page, workdir))
}
