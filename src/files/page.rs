use crate::core::app::Page;
use crate::core::config::Config;
use crate::files::state::FilesState;
use anyhow::Result;
use ratatui_image::picker::Picker;
use std::path::Path;

/// Create the Files page rooted at `root` (the repository working directory).
/// `picker` draws image previews; `None` disables them.
pub fn new_page(root: &Path, cfg: &Config, picker: Option<Picker>) -> Result<Page> {
    Ok(Page::new(FilesState::new(root, cfg, picker)?))
}
