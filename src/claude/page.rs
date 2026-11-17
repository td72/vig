use crate::claude::state::ClaudeState;
use crate::core::app::Page;
use crate::core::config::Config;
use anyhow::Result;
use std::path::Path;

/// Create the Claude page for the repository at `workdir`.
pub fn new_page(workdir: &Path, cfg: &Config) -> Result<Page> {
    Ok(Page::new(ClaudeState::new(workdir, cfg)?))
}
