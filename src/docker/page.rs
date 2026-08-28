use crate::core::app::Page;
use crate::core::config::Config;
use crate::docker::state::DockerState;
use anyhow::Result;

/// Create the Docker page.
pub fn new_page(cfg: &Config) -> Result<Page> {
    Ok(Page::new(DockerState::new(cfg)?))
}
