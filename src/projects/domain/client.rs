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

/// Fields, items and saved views of one project. A views fetch failure is
/// not fatal: the board still loads and the page falls back to the fixed
/// Status kanban.
pub fn fetch_board(owner: &str, owner_kind: &str, number: u64) -> Result<Board, String> {
    let fields = list_fields(owner, number)?;
    let items = list_items(owner, number)?;
    let views = fetch_views(owner, owner_kind, number).unwrap_or_default();
    Ok(Board {
        number,
        fields,
        items: items.items,
        total_count: items.total_count,
        views,
    })
}

/// The project's saved views via GraphQL (`ProjectV2.views` — `gh project`
/// does not expose them). `owner_kind` is `User` / `Organization` from the
/// linked project's `resourcePath`; anything else tries the user query
/// first, then the organization one.
///
/// The query caps what it reads — 20 views, 5 group / sort fields each,
/// 30 visible fields — far above what the GitHub UI produces (grouping
/// and sorting take one field there); anything beyond a cap is ignored
/// rather than paginated.
pub fn fetch_views(owner: &str, owner_kind: &str, number: u64) -> Result<Vec<ProjectView>, String> {
    match owner_kind {
        "User" => fetch_views_as(owner, number, false),
        "Organization" => fetch_views_as(owner, number, true),
        _ => fetch_views_as(owner, number, false).or_else(|_| fetch_views_as(owner, number, true)),
    }
}

fn fetch_views_as(owner: &str, number: u64, org: bool) -> Result<Vec<ProjectView>, String> {
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Resp {
        data: serde_json::Value,
    }
    #[derive(Deserialize, Default)]
    struct Views {
        nodes: Vec<ViewNode>,
    }
    #[derive(Deserialize)]
    struct ViewNode {
        number: u64,
        #[serde(default)]
        name: String,
        #[serde(default)]
        layout: String,
        #[serde(default)]
        filter: Option<String>,
        #[serde(rename = "groupByFields", default)]
        group_by: Named,
        #[serde(rename = "verticalGroupByFields", default)]
        vertical_group_by: Named,
        #[serde(rename = "sortByFields", default)]
        sort_by: Sorts,
        #[serde(default)]
        fields: Named,
    }
    #[derive(Deserialize, Default)]
    struct Named {
        nodes: Vec<NameNode>,
    }
    #[derive(Deserialize, Default)]
    struct NameNode {
        #[serde(default)]
        name: String,
    }
    #[derive(Deserialize, Default)]
    struct Sorts {
        nodes: Vec<SortNode>,
    }
    #[derive(Deserialize)]
    struct SortNode {
        #[serde(default)]
        direction: String,
        #[serde(default)]
        field: NameNode,
    }

    let root = if org { "organization" } else { "user" };
    let query = format!(
        "query($login: String!, $number: Int!) {{ {root}(login: $login) {{ \
           projectV2(number: $number) {{ views(first: 20) {{ nodes {{ \
             name number layout filter \
             groupByFields(first: 5) {{ nodes {{ ... on ProjectV2FieldCommon {{ name }} }} }} \
             verticalGroupByFields(first: 5) {{ nodes {{ ... on ProjectV2FieldCommon {{ name }} }} }} \
             sortByFields(first: 5) {{ nodes {{ direction field {{ ... on ProjectV2FieldCommon {{ name }} }} }} }} \
             fields(first: 30) {{ nodes {{ ... on ProjectV2FieldCommon {{ name }} }} }} }} }} }} }} }}"
    );
    let resp: Resp = crate::github::domain::client::run_gh_json(
        &[
            "api",
            "graphql",
            "-f",
            &format!("query={query}"),
            "-F",
            &format!("login={owner}"),
            "-F",
            &format!("number={number}"),
        ],
        "gh api graphql (project views) failed",
    )?;
    let views: Views = resp
        .data
        .get(root)
        .and_then(|v| v.get("projectV2"))
        .and_then(|v| v.get("views"))
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|e| format!("project views: JSON parse error: {e}"))?
        .unwrap_or_default();
    let names = |n: &Named| -> Vec<String> {
        n.nodes
            .iter()
            .map(|f| f.name.clone())
            .filter(|s| !s.is_empty())
            .collect()
    };
    Ok(views
        .nodes
        .into_iter()
        .filter_map(|v| {
            Some(ProjectView {
                number: v.number,
                name: v.name,
                layout: ViewLayout::parse(&v.layout)?,
                filter: v.filter.filter(|f| !f.trim().is_empty()),
                group_by: names(&v.group_by),
                vertical_group_by: names(&v.vertical_group_by),
                sort_by: v
                    .sort_by
                    .nodes
                    .into_iter()
                    .filter(|sn| !sn.field.name.is_empty())
                    .map(|sn| ViewSort {
                        field: sn.field.name.clone(),
                        desc: sn.direction == "DESC",
                    })
                    .collect(),
                visible_fields: names(&v.fields),
            })
        })
        .collect())
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
