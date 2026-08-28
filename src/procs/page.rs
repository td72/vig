use crate::core::app::Page;
use crate::core::config::Config;
use crate::procs::state::ProcsState;
use anyhow::Result;

/// Create the Procs page (process tree, listening ports, process detail).
pub fn new_page(cfg: &Config) -> Result<Page> {
    Ok(Page::new(ProcsState::new(cfg)?))
}
