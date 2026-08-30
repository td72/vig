use crate::core::app::Page;
use crate::core::config::Config;
use crate::projects::state::ProjectsState;
use anyhow::Result;

/// Create the Projects page.
pub fn new_page(cfg: &Config) -> Result<Page> {
    Ok(Page::new(ProjectsState::new(cfg)?))
}
