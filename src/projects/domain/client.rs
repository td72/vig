//! Thin wrappers around the `gh` reads the page needs. Every command here
//! only reads: `gh repo view` (the linked projects) and `gh project
//! item-list / field-list` (a board). Nothing in this module (or the page)
//! adds, edits or deletes items or fields.

use crate::github::domain::client::run_gh_json;
use crate::projects::domain::types::*;

/// `gh project item-list --limit`: a board larger than this is shown
/// truncated (the status bar says so).
pub const ITEM_LIMIT: usize = 500;

/// The repository vig runs in, its owner and the projects linked to it.
pub fn repo_info() -> Result<RepoInfo, String> {
    run_gh_json(
        &["repo", "view", "--json", "nameWithOwner,owner,projectsV2"],
        "gh repo view failed",
    )
}

pub fn list_fields(owner: &str, number: u64) -> Result<Vec<ProjectField>, String> {
    let list: FieldList = run_gh_json(
        &[
            "project",
            "field-list",
            &number.to_string(),
            "--owner",
            owner,
            "--format",
            "json",
            "--limit",
            "100",
        ],
        "gh project field-list failed",
    )?;
    Ok(list.fields)
}

pub fn list_items(owner: &str, number: u64) -> Result<ItemList, String> {
    run_gh_json(
        &[
            "project",
            "item-list",
            &number.to_string(),
            "--owner",
            owner,
            "--format",
            "json",
            "--limit",
            &ITEM_LIMIT.to_string(),
        ],
        "gh project item-list failed",
    )
}

/// Fields and items of one project.
pub fn fetch_board(owner: &str, number: u64) -> Result<Board, String> {
    let fields = list_fields(owner, number)?;
    let items = list_items(owner, number)?;
    Ok(Board {
        number,
        fields,
        items: items.items,
        total_count: items.total_count,
    })
}

/// Whether a `gh` error means the token lacks the `project` scope
/// (`gh project` prints "missing required scopes [project]"; the GraphQL
/// API asks for `read:project`).
pub fn is_scope_error(msg: &str) -> bool {
    let m = msg.to_lowercase();
    m.contains("read:project") || (m.contains("scope") && m.contains("project"))
}

/// Whether a `gh` error means the CLI itself is missing.
pub fn is_gh_missing(msg: &str) -> bool {
    msg.contains("gh not found")
        || msg.contains("gh repo view failed: No such file")
        || msg.contains("os error 2")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_errors_are_recognised() {
        assert!(is_scope_error(
            "error: your authentication token is missing required scopes [project]\nTo request it, run:  gh auth refresh -s project"
        ));
        assert!(is_scope_error(
            "Your token has not been granted the required scopes to execute this query. The 'projectsV2' field requires one of the following scopes: ['read:project']"
        ));
        assert!(!is_scope_error("gh not found: No such file or directory"));
        assert!(!is_scope_error(
            "Could not resolve to a ProjectV2 with the number 99."
        ));
        assert!(!is_scope_error("HTTP 404: Not Found"));
    }

    #[test]
    fn missing_cli_is_recognised() {
        assert!(is_gh_missing(
            "gh not found: No such file or directory (os error 2)"
        ));
        assert!(!is_gh_missing("gh project list failed: exit status 1"));
    }
}
