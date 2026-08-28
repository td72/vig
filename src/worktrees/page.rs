use crate::core::app::Page;
use crate::core::config::Config;
use crate::worktrees::state::WorktreesState;
use anyhow::Result;
use std::path::Path;

/// Create the Worktrees page for the repository whose working directory is
/// `root` (the worktree vig runs in; it is highlighted in the list).
pub fn new_page(root: &Path, cfg: &Config) -> Result<Page> {
    Ok(Page::new(WorktreesState::new(root, cfg)?))
}
