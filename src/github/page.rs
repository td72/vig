use crate::core::app::Page;
use crate::core::config::Config;
use crate::github::state::GitHubState;
use anyhow::Result;
use std::path::Path;

/// Create a GitHub page rooted at `workdir` (where `W` adds a PR's
/// worktree next to).
pub fn new_page(workdir: &Path, cfg: &Config) -> Result<Page> {
    Ok(Page::new(GitHubState::new_in(workdir, cfg)?))
}
