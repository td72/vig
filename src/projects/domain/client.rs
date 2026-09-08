//! Thin wrappers around the `gh` reads the page needs. Every command here
//! only reads: `gh repo view` (the linked projects), `gh api graphql`
//! (a board — see `graphql.rs`) and, as the fallback, `gh project
//! item-list / field-list`. Nothing in this module (or the page) adds,
//! edits or deletes items or fields.

use crate::github::domain::client::run_gh_json;
use crate::projects::domain::types::*;

/// `gh project item-list --limit`: a board larger than this is shown
/// truncated (the status bar says so).
pub const ITEM_LIMIT: usize = 500;

/// First `item-list` fetch. GraphQL cost scales with the **requested**
/// limit (measured: `--limit 500` ≈ 101 points, `--limit 50` ≈ 51), so
/// boards are fetched small first and re-fetched at [`ITEM_LIMIT`] only
/// when `totalCount` says more items exist.
pub const ITEM_FIRST_FETCH: usize = 100;

/// `field-list` limit: projects rarely define more than a couple dozen
/// fields; a full page here costs as much as an item page.
pub const FIELD_LIMIT: usize = 30;

/// The larger limit to retry with when a first fetch came back full:
/// `Some(bigger)` when `fetched` filled `limit` and `total` says there is
/// more (a `total` of 0 means the CLI did not report one — retry too).
pub fn needs_bigger_fetch(fetched: usize, total: u64, limit: usize, max: usize) -> Option<usize> {
    (fetched >= limit && limit < max && (total == 0 || total > limit as u64)).then_some(max)
}

/// The repository vig runs in, its owner and the projects linked to it.
pub fn repo_info() -> Result<RepoInfo, String> {
    run_gh_json(
        &["repo", "view", "--json", "nameWithOwner,owner,projectsV2"],
        "gh repo view failed",
    )
}

pub fn list_fields(owner: &str, number: u64) -> Result<Vec<ProjectField>, String> {
    let fields = list_fields_limited(owner, number, FIELD_LIMIT)?;
    match needs_bigger_fetch(fields.len(), 0, FIELD_LIMIT, 100) {
        Some(bigger) => list_fields_limited(owner, number, bigger),
        None => Ok(fields),
    }
}

fn list_fields_limited(
    owner: &str,
    number: u64,
    limit: usize,
) -> Result<Vec<ProjectField>, String> {
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
            &limit.to_string(),
        ],
        "gh project field-list failed",
    )?;
    Ok(list.fields)
}

pub fn list_items(owner: &str, number: u64) -> Result<ItemList, String> {
    let list = list_items_limited(owner, number, ITEM_FIRST_FETCH)?;
    match needs_bigger_fetch(
        list.items.len(),
        list.total_count,
        ITEM_FIRST_FETCH,
        ITEM_LIMIT,
    ) {
        Some(bigger) => list_items_limited(owner, number, bigger),
        None => Ok(list),
    }
}

fn list_items_limited(owner: &str, number: u64, limit: usize) -> Result<ItemList, String> {
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
            &limit.to_string(),
        ],
        "gh project item-list failed",
    )
}

/// Fields, items and saved views of one project: the GraphQL path
/// ([`graphql::fetch_board`], a couple of points) first, and the
/// `gh project` CLI path when that fails (a schema the account's API
/// does not serve, an old `gh`). On the CLI path a views fetch failure
/// is not fatal: the board still loads with the fixed Status kanban.
pub fn fetch_board(owner: &str, owner_kind: &str, number: u64) -> Result<Board, String> {
    if let Ok(board) = crate::projects::domain::graphql::fetch_board(owner, owner_kind, number) {
        return Ok(board);
    }
    let fields = list_fields(owner, number)?;
    let items = list_items(owner, number)?;
    let (views, _api_remaining) = fetch_views(owner, owner_kind, number).unwrap_or_default();
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
pub fn fetch_views(
    owner: &str,
    owner_kind: &str,
    number: u64,
) -> Result<(Vec<ProjectView>, Option<u64>), String> {
    match owner_kind {
        "User" => fetch_views_as(owner, number, false),
        "Organization" => fetch_views_as(owner, number, true),
        _ => fetch_views_as(owner, number, false).or_else(|_| fetch_views_as(owner, number, true)),
    }
}

fn fetch_views_as(
    owner: &str,
    number: u64,
    org: bool,
) -> Result<(Vec<ProjectView>, Option<u64>), String> {
    let root = if org { "organization" } else { "user" };
    let query = format!(
        "query($login: String!, $number: Int!) {{ {root}(login: $login) {{ \
           projectV2(number: $number) {{ views(first: 20) {{ nodes {{ \
             name number layout filter \
             groupByFields(first: 5) {{ nodes {{ ... on ProjectV2FieldCommon {{ name }} }} }} \
             verticalGroupByFields(first: 5) {{ nodes {{ ... on ProjectV2FieldCommon {{ name }} }} }} \
             sortByFields(first: 5) {{ nodes {{ direction field {{ ... on ProjectV2FieldCommon {{ name }} }} }} }} \
             fields(first: 30) {{ nodes {{ ... on ProjectV2FieldCommon {{ name }} }} }} }} }} }} }} \
           rateLimit {{ remaining }} }}"
    );
    let resp: serde_json::Value = crate::github::domain::client::run_gh_json(
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
    let api_remaining = resp
        .pointer("/data/rateLimit/remaining")
        .and_then(serde_json::Value::as_u64);
    if let Some(left) = api_remaining {
        crate::core::api_budget::note(left);
    }
    let views = resp
        .pointer(&format!("/data/{root}/projectV2/views"))
        .map(crate::projects::domain::graphql::views_from)
        .unwrap_or_default();
    Ok((views, api_remaining))
}

/// The signed-in login (`gh api user`, REST — no GraphQL points), for
/// `assignee:@me` in view filters. `None` when unavailable.
pub fn viewer_login() -> Option<String> {
    let out = crate::github::domain::client::run_gh(
        &["api", "user", "--jq", ".login"],
        "gh api user failed",
    )
    .ok()?;
    let login = String::from_utf8_lossy(&out).trim().to_string();
    (!login.is_empty()).then_some(login)
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
    fn bigger_fetch_only_when_the_first_page_was_full() {
        // Under the limit: done.
        assert_eq!(needs_bigger_fetch(8, 8, 100, 500), None);
        assert_eq!(needs_bigger_fetch(99, 99, 100, 500), None);
        // Full page and the total says more: retry at max.
        assert_eq!(needs_bigger_fetch(100, 340, 100, 500), Some(500));
        // Full page, no reported total (field-list): retry too.
        assert_eq!(needs_bigger_fetch(30, 0, 30, 100), Some(100));
        // Full page but the total says that was everything.
        assert_eq!(needs_bigger_fetch(100, 100, 100, 500), None);
        // Already at the max: never retry.
        assert_eq!(needs_bigger_fetch(500, 900, 500, 500), None);
    }

    #[test]
    fn missing_cli_is_recognised() {
        assert!(is_gh_missing(
            "gh not found: No such file or directory (os error 2)"
        ));
        assert!(!is_gh_missing("gh project list failed: exit status 1"));
    }
}
