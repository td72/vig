use crate::core::app::Page;
use crate::core::config::Config;
use crate::files::state::FilesState;
use anyhow::Result;
use std::path::Path;

/// Create the Files page rooted at `root` (the repository working directory).
pub fn new_page(root: &Path, cfg: &Config) -> Result<Page> {
    Ok(Page::new(FilesState::new(root, cfg)?))
}
