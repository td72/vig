use crate::core::app::Page;
use crate::core::config::Config;
use crate::github::state::GitHubState;
use anyhow::Result;

/// Create a GitHub page.
pub fn new_page(cfg: &Config) -> Result<Page> {
    Ok(Page::new(GitHubState::new(cfg)?))
}
