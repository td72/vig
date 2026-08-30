//! Thin wrappers around `gh project …` and the `gh api` reads the page
//! needs. Every command here only reads: `gh repo view`, `gh project list /
//! item-list / field-list` and a GraphQL query for `updatedAt`. Nothing in
//! this module (or the page) adds, edits or deletes items or fields.

use crate::github::domain::client::run_gh_json;
use crate::projects::domain::types::*;
use serde::Deserialize;
use std::collections::HashMap;

/// `gh project item-list --limit`: a board larger than this is shown
/// truncated (the status bar says so).
pub const ITEM_LIMIT: usize = 500;

/// `gh project list --limit`.
pub const PROJECT_LIMIT: usize = 100;

/// Repository owner and the projects linked to the repository vig runs in.
pub fn repo_info() -> Result<RepoInfo, String> {
    run_gh_json(
        &["repo", "view", "--json", "owner,projectsV2"],
        "gh repo view failed",
    )
}

/// `gh project list --owner <owner>` (open projects only), with
/// `updatedAt` merged in from GraphQL when that query succeeds.
pub fn list_projects(owner: &str) -> Result<Vec<Project>, String> {
    let list: ProjectList = run_gh_json(
        &[
            "project",
            "list",
            "--owner",
            owner,
            "--format",
            "json",
            "--limit",
            &PROJECT_LIMIT.to_string(),
        ],
        "gh project list failed",
    )?;
    let mut projects: Vec<Project> = list.projects.into_iter().filter(|p| !p.closed).collect();
    if let Ok(updated) = project_updated_at(owner) {
        for p in &mut projects {
            p.updated_at = updated.get(&p.number).cloned();
        }
    }
    Ok(projects)
}

/// `updatedAt` per project number of `owner` (user or organization).
fn project_updated_at(owner: &str) -> Result<HashMap<u64, String>, String> {
    #[derive(Deserialize)]
    struct Resp {
        data: Data,
    }
    #[derive(Deserialize)]
    struct Data {
        #[serde(rename = "repositoryOwner")]
        repository_owner: Option<Owner>,
    }
    #[derive(Deserialize)]
    struct Owner {
        #[serde(rename = "projectsV2")]
        projects_v2: Option<Conn>,
    }
    #[derive(Deserialize)]
    struct Conn {
        nodes: Vec<Node>,
    }
    #[derive(Deserialize)]
    struct Node {
        number: u64,
        #[serde(rename = "updatedAt")]
        updated_at: String,
    }
    const QUERY: &str = "query($owner: String!) { repositoryOwner(login: $owner) { \
        ... on ProjectV2Owner { projectsV2(first: 100) { nodes { number updatedAt } } } } }";
    let resp: Resp = run_gh_json(
        &[
            "api",
            "graphql",
            "-f",
            &format!("query={QUERY}"),
            "-F",
            &format!("owner={owner}"),
        ],
        "gh api graphql (projects) failed",
    )?;
    Ok(resp
        .data
        .repository_owner
        .and_then(|o| o.projects_v2)
        .map(|c| {
            c.nodes
                .into_iter()
                .map(|n| (n.number, n.updated_at))
                .collect()
        })
        .unwrap_or_default())
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
