use crate::github::domain::types::*;
use serde::Deserialize;
use std::collections::HashMap;
use std::process::Command;

/// Run a `gh` command and return its stdout on success.
fn run_gh(args: &[&str], context: &str) -> Result<Vec<u8>, String> {
    let output = Command::new("gh")
        .args(args)
        .output()
        .map_err(|e| format!("{context}: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(stderr.trim().to_string());
    }
    Ok(output.stdout)
}

/// Run a `gh` command and parse the JSON output.
fn run_gh_json<T: serde::de::DeserializeOwned>(args: &[&str], context: &str) -> Result<T, String> {
    let stdout = run_gh(args, context)?;
    serde_json::from_slice(&stdout).map_err(|e| format!("JSON parse error: {e}"))
}

/// Open a GitHub issue or PR in the browser.
fn open_in_browser(entity: &str, number: u64) -> Result<(), String> {
    Command::new("gh")
        .args([entity, "view", &number.to_string(), "--web"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to open {entity} in browser: {e}"))?;
    Ok(())
}

pub fn check_gh_available() -> Result<(), String> {
    run_gh(&["auth", "status"], "gh not found").map(|_| ())
}

pub fn list_issues(limit: usize) -> Result<Vec<GhIssueListItem>, String> {
    run_gh_json(
        &[
            "issue",
            "list",
            "--json",
            "number,title,state,author,labels,createdAt,parent",
            "--limit",
            &limit.to_string(),
        ],
        "gh issue list failed",
    )
}

pub fn list_prs(limit: usize) -> Result<Vec<GhPrListItem>, String> {
    run_gh_json(
        &[
            "pr",
            "list",
            "--json",
            "number,title,state,author,labels,headRefName,baseRefName,createdAt,reviewDecision,isDraft",
            "--limit",
            &limit.to_string(),
        ],
        "gh pr list failed",
    )
}

pub fn get_issue(number: u64) -> Result<GhIssueDetail, String> {
    run_gh_json(
        &[
            "issue",
            "view",
            &number.to_string(),
            "--json",
            "number,title,state,author,body,comments,labels,createdAt",
        ],
        "gh issue view failed",
    )
}

pub fn get_pr(number: u64) -> Result<GhPrDetail, String> {
    run_gh_json(
        &[
            "pr",
            "view",
            &number.to_string(),
            "--json",
            "number,title,state,author,body,comments,reviews,labels,createdAt,reviewDecision,statusCheckRollup,additions,deletions,changedFiles,headRefName",
        ],
        "gh pr view failed",
    )
}

pub fn open_issue_in_browser(number: u64) -> Result<(), String> {
    open_in_browser("issue", number)
}

pub fn open_pr_in_browser(number: u64) -> Result<(), String> {
    open_in_browser("pr", number)
}

/// GitHub Stack membership of the open PRs, keyed by PR number.
///
/// `gh pr list --json` does not expose stacks, so this goes through the
/// GraphQL API (`PullRequest.stack` / `stackEntry`). PRs that are not in a
/// stack are absent from the map.
pub fn list_pr_stacks(limit: usize) -> Result<HashMap<u64, GhPrStackRef>, String> {
    #[derive(Deserialize)]
    struct Resp {
        data: Data,
    }
    #[derive(Deserialize)]
    struct Data {
        repository: Repo,
    }
    #[derive(Deserialize)]
    struct Repo {
        #[serde(rename = "pullRequests")]
        pull_requests: Conn,
    }
    #[derive(Deserialize)]
    struct Conn {
        nodes: Vec<Node>,
    }
    #[derive(Deserialize)]
    struct Node {
        number: u64,
        stack: Option<Stack>,
        #[serde(rename = "stackEntry")]
        stack_entry: Option<Entry>,
    }
    #[derive(Deserialize)]
    struct Stack {
        number: u64,
        size: u32,
    }
    #[derive(Deserialize)]
    struct Entry {
        position: u32,
    }

    let nwo = repo_nwo().ok_or("gh repo view failed")?;
    let (owner, name) = nwo
        .split_once('/')
        .ok_or_else(|| format!("unexpected repository name {nwo:?}"))?;
    const QUERY: &str = "query($owner: String!, $name: String!, $first: Int!) { \
        repository(owner: $owner, name: $name) { \
          pullRequests(states: OPEN, first: $first, orderBy: {field: CREATED_AT, direction: DESC}) { \
            nodes { number stack { number size } stackEntry { position } } } } }";
    let resp: Resp = run_gh_json(
        &[
            "api",
            "graphql",
            "-f",
            &format!("query={QUERY}"),
            "-F",
            &format!("owner={owner}"),
            "-F",
            &format!("name={name}"),
            "-F",
            &format!("first={}", limit.min(100)),
        ],
        "gh api graphql (pull request stacks) failed",
    )?;
    Ok(resp
        .data
        .repository
        .pull_requests
        .nodes
        .into_iter()
        .filter_map(|n| {
            let (stack, entry) = (n.stack?, n.stack_entry?);
            Some((
                n.number,
                GhPrStackRef {
                    number: stack.number,
                    position: entry.position,
                    size: stack.size,
                },
            ))
        })
        .collect())
}

/// Get the "owner/repo" string for the current repository using `gh`.
pub fn repo_nwo() -> Option<String> {
    let stdout = run_gh(
        &[
            "repo",
            "view",
            "--json",
            "nameWithOwner",
            "-q",
            ".nameWithOwner",
        ],
        "gh repo view",
    )
    .ok()?;
    Some(String::from_utf8_lossy(&stdout).trim().to_string())
}
