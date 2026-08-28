use crate::actions::state::ActionsState;
use crate::core::app::Page;
use crate::core::config::Config;
use anyhow::Result;

/// Create the Actions page.
pub fn new_page(cfg: &Config) -> Result<Page> {
    Ok(Page::new(ActionsState::new(cfg)?))
}
