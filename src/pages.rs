//! Page registry: builds the pages a config enables, in its slot order.
//!
//! The `pages "git" "files" ...` node of the config (see `Config::pages`)
//! decides which pages exist and the tab position of each; the vector
//! returned here follows that order, so index `n` is header slot `n + 1`.

use crate::core::app::Page;
use crate::core::config::Config;
use crate::git::domain::repository::Repo;
use anyhow::{anyhow, Result};
use ratatui_image::picker::Picker;
use std::path::{Path, PathBuf};

/// Create every page listed in `cfg.pages()`, in that order, plus the
/// repository working directory the pages are rooted at (discovered from
/// `cwd`). `picker` is handed to the Files page when that page is enabled.
pub fn build_pages(
    cfg: &Config,
    cwd: &Path,
    mut picker: Option<Picker>,
) -> Result<(Vec<Page>, PathBuf)> {
    let workdir = Repo::discover(cwd)?.workdir().to_path_buf();
    let mut pages = Vec::new();
    for name in cfg.pages()? {
        let page = match name.as_str() {
            "git" => crate::git::page::new_page(cwd, cfg)?.0,
            "github" => crate::github::page::new_page(cfg)?,
            "files" => crate::files::page::new_page(&workdir, cfg, picker.take())?,
            "docker" => crate::docker::page::new_page(cfg)?,
            "procs" => crate::procs::page::new_page(cfg)?,
            "worktrees" => crate::worktrees::page::new_page(&workdir, cfg)?,
            other => {
                // `Config::pages` validates against the built-in `page` blocks,
                // so this only fires when a page has a KDL block but no
                // constructor here.
                return Err(anyhow!(
                    "page {other:?} is configured but has no implementation"
                ));
            }
        };
        debug_assert_eq!(page.id(), name, "page id must match its config name");
        pages.push(page);
    }
    Ok((pages, workdir))
}
