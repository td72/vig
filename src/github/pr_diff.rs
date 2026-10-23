//! `d` on a pull request: its diff in a window on top of the GitHub page.

use crate::core::app::Page;
use crate::core::config::LoadedPageConfig;
use crate::diff_window::{DiffSource, DiffWindowState};
use crate::github::domain::types::{GhPrDetail, GhPrListItem};
use crate::github::domain::{client, disk_cache};
use anyhow::Result;
use std::sync::Arc;

/// What the window's header says about the PR, from whichever of the
/// list row and the detail is at hand.
pub(crate) fn source_for(
    number: u64,
    item: Option<&GhPrListItem>,
    detail: Option<&GhPrDetail>,
) -> DiffSource {
    let title = item
        .map(|i| i.title.clone())
        .or_else(|| detail.map(|d| d.title.clone()))
        .unwrap_or_default();
    let refs = match (item, detail) {
        (Some(i), _) if !i.base_ref_name.is_empty() => {
            Some(format!("{} → {}", i.head_ref_name, i.base_ref_name))
        }
        (Some(i), _) => Some(i.head_ref_name.clone()),
        (None, Some(d)) => Some(d.head_ref_name.clone()),
        (None, None) => None,
    }
    .filter(|r| !r.is_empty());
    DiffSource {
        badge: format!("PR #{number}"),
        refs,
        title,
        url: client::pr_files_url(number),
        cached: disk_cache::load_pr_diff(number),
        fetch: Arc::new(move || client::pr_diff(number)),
        store: Some(Arc::new(move |patch: &[u8]| {
            disk_cache::save_pr_diff(number, patch)
        })),
    }
}

/// The diff window for PR `number`.
pub(crate) fn open(
    cfg: &LoadedPageConfig,
    theme: &str,
    number: u64,
    item: Option<&GhPrListItem>,
    detail: Option<&GhPrDetail>,
) -> Result<Page> {
    let source = source_for(number, item, detail);
    Ok(Page::new(DiffWindowState::new(cfg, theme, source)?))
}
