use crate::github::domain::types::*;
use serde::Deserialize;
use std::collections::HashMap;
use std::process::Command;

/// Run a `gh` command and return its stdout on success.
pub(crate) fn run_gh(args: &[&str], context: &str) -> Result<Vec<u8>, String> {
    // Recorded output stands in for `gh` in demo recordings and tests.
    if let Some(replayed) = crate::core::gh_fixture::replay(args) {
        return replayed;
    }
    let output = Command::new("gh")
        .args(args)
        .output()
        .map_err(|e| format!("{context}: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(stderr.trim().to_string());
    }
    crate::core::gh_fixture::record(args, &output.stdout);
    Ok(output.stdout)
}

/// Run a `gh` command and parse the JSON output.
pub(crate) fn run_gh_json<T: serde::de::DeserializeOwned>(
    args: &[&str],
    context: &str,
) -> Result<T, String> {
    let stdout = run_gh(args, context)?;
    serde_json::from_slice(&stdout).map_err(|e| format!("JSON parse error: {e}"))
}

/// Whether a `gh` error message is a GitHub rate-limit rejection.
///
/// REST answers 403 / 429 with "API rate limit exceeded for ..." ("... have
/// exceeded a secondary rate limit" for abuse limits); GraphQL answers with
/// an error of type `RATE_LIMITED` and "API rate limit already exceeded".
pub fn is_rate_limited(msg: &str) -> bool {
    msg.contains("API rate limit exceeded")
        || msg.contains("rate limit already exceeded")
        || msg.contains("RATE_LIMITED")
        || msg.contains("secondary rate limit")
}

/// Unix time the REST core quota resets, via `gh api rate_limit` (that
/// endpoint answers even while the quota is exhausted, so it is safe to
/// call from a rate-limited session).
pub fn fetch_rate_limit_reset() -> Option<i64> {
    let out = run_gh(
        &["api", "rate_limit", "--jq", ".resources.core.reset"],
        "gh api rate_limit failed",
    )
    .ok()?;
    String::from_utf8_lossy(&out).trim().parse().ok()
}

/// `owner/repo` derived locally from the `origin` remote URL — no API
/// request. `None` for non-github.com remotes (or no remote).
pub(crate) fn origin_github_nwo() -> Option<String> {
    static NWO: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    NWO.get_or_init(|| {
        let out = Command::new("git")
            .args(["remote", "get-url", "origin"])
            .output()
            .ok()
            .filter(|o| o.status.success())?;
        crate::github::domain::remote::parse_github_nwo(String::from_utf8_lossy(&out.stdout).trim())
    })
    .clone()
}

/// Open a GitHub issue or PR in the browser.
///
/// The URL is built locally from the origin remote — pressing `o` must not
/// spend an API request. Non-github.com remotes fall back to `gh … --web`.
fn open_in_browser(entity: &str, number: u64) -> Result<(), String> {
    if let Some(nwo) = origin_github_nwo() {
        let path = if entity == "pr" { "pull" } else { "issues" };
        return crate::core::browser::open_url(&format!(
            "https://github.com/{nwo}/{path}/{number}"
        ));
    }
    Command::new("gh")
        .args([entity, "view", &number.to_string(), "--web"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to open {entity} in browser: {e}"))?;
    Ok(())
}

/// Locally built URL for an issue — no API request. `None` when the
/// origin remote is not github.com.
pub fn issue_url(number: u64) -> Option<String> {
    origin_github_nwo().map(|nwo| format!("https://github.com/{nwo}/issues/{number}"))
}

/// Locally built URL for a pull request (see `issue_url`).
pub fn pr_url(number: u64) -> Option<String> {
    origin_github_nwo().map(|nwo| format!("https://github.com/{nwo}/pull/{number}"))
}

/// Locally built URL for a commit (see `issue_url`).
pub fn commit_url(hash: &str) -> Option<String> {
    origin_github_nwo().map(|nwo| format!("https://github.com/{nwo}/commit/{hash}"))
}

/// What the last change check saw: the ETag GitHub gave for "the most
/// recently updated issue or PR" and that item's `updated_at`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListWatermark {
    pub etag: Option<String>,
    pub updated_at: Option<String>,
}

/// Outcome of a change check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListCheck {
    /// `304 Not Modified`: nothing changed — and the request was free.
    NotModified,
    /// `200`: the current watermark (which may still equal the previous one).
    Fresh(ListWatermark),
}

/// The free change check behind the issue / PR list auto-refresh: one
/// conditional REST request for the single most recently updated issue or
/// PR (`/issues` covers both). With `prev.etag` sent as `If-None-Match`,
/// an unchanged repository answers `304` at no cost against the quota.
pub fn check_lists(prev: &ListWatermark) -> Result<ListCheck, String> {
    // A replayed recording holds one unconditional answer; the header
    // would name a fixture that was never written.
    let etag = prev
        .etag
        .as_deref()
        .filter(|_| !crate::core::gh_fixture::active());
    let args = check_lists_args(etag);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = run_gh(&refs, "gh api (change check) failed")?;
    parse_list_check(&String::from_utf8_lossy(&out))
}

/// The `gh` arguments of the change check, conditional on `etag` when given
/// (also names its fixture for the recorder).
pub fn check_lists_args(etag: Option<&str>) -> Vec<String> {
    let mut args: Vec<String> = vec!["api".into(), "-i".into()];
    if let Some(etag) = etag {
        args.push("-H".into());
        args.push(format!("If-None-Match: {etag}"));
    }
    args.push(
        "repos/{owner}/{repo}/issues?state=all&sort=updated&direction=desc&per_page=1".into(),
    );
    args
}

/// Parse a `gh api -i` response: the status line, the headers up to the
/// blank line, then the JSON body.
pub fn parse_list_check(raw: &str) -> Result<ListCheck, String> {
    let mut lines = raw.lines();
    let status = lines.next().unwrap_or("");
    if status.split_whitespace().nth(1) == Some("304") {
        return Ok(ListCheck::NotModified);
    }
    if !status.starts_with("HTTP/") {
        return Err(format!("change check: unexpected response {status:?}"));
    }
    let mut etag = None;
    let mut body = String::new();
    let mut in_body = false;
    for line in lines {
        if in_body {
            body.push_str(line);
        } else if line.trim().is_empty() {
            in_body = true;
        } else if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("etag") {
                etag = Some(value.trim().to_string());
            }
        }
    }
    let updated_at = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| v.get(0)?.get("updated_at")?.as_str().map(str::to_string));
    Ok(ListCheck::Fresh(ListWatermark { etag, updated_at }))
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
    get_issue_in(None, number)
}

/// `gh issue view` in `repo` (`owner/repo`; `None` for the current one).
pub fn get_issue_in(repo: Option<&str>, number: u64) -> Result<GhIssueDetail, String> {
    let number = number.to_string();
    let mut args = vec![
        "issue",
        "view",
        &number,
        "--json",
        "number,title,state,author,body,comments,labels,createdAt",
    ];
    if let Some(repo) = repo {
        args.extend(["--repo", repo]);
    }
    run_gh_json(&args, "gh issue view failed")
}

pub fn get_pr(number: u64) -> Result<GhPrDetail, String> {
    get_pr_in(None, number)
}

/// `gh pr view` in `repo` (`owner/repo`; `None` for the current one).
pub fn get_pr_in(repo: Option<&str>, number: u64) -> Result<GhPrDetail, String> {
    let number = number.to_string();
    let mut args = vec![
        "pr",
        "view",
        &number,
        "--json",
        "number,title,state,author,body,comments,reviews,labels,createdAt,reviewDecision,statusCheckRollup,additions,deletions,changedFiles,headRefName",
    ];
    if let Some(repo) = repo {
        args.extend(["--repo", repo]);
    }
    run_gh_json(&args, "gh pr view failed")
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
        #[serde(rename = "rateLimit", default)]
        rate_limit: Option<RateLimit>,
    }
    #[derive(Deserialize, Default)]
    struct RateLimit {
        #[serde(default)]
        remaining: Option<u64>,
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
            nodes { number stack { number size } stackEntry { position } } } } \
        rateLimit { remaining } }";
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
    if let Some(left) = resp.data.rate_limit.as_ref().and_then(|r| r.remaining) {
        crate::core::api_budget::note(left);
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn change_check_parses_304_and_200() {
        assert_eq!(
            parse_list_check("HTTP/2.0 304 Not Modified\r\nEtag: \"abc\"\r\n\r\n").unwrap(),
            ListCheck::NotModified
        );
        let raw = "HTTP/2.0 200 OK\r\nContent-Type: application/json\r\nETag: W/\"deadbeef\"\r\n\r\n[{\"number\":7,\"updated_at\":\"2026-09-09T01:02:03Z\"}]\n";
        assert_eq!(
            parse_list_check(raw).unwrap(),
            ListCheck::Fresh(ListWatermark {
                etag: Some("W/\"deadbeef\"".into()),
                updated_at: Some("2026-09-09T01:02:03Z".into()),
            })
        );
        // An empty repository: 200 with no items.
        assert_eq!(
            parse_list_check("HTTP/2.0 200 OK\r\nETag: \"e\"\r\n\r\n[]").unwrap(),
            ListCheck::Fresh(ListWatermark {
                etag: Some("\"e\"".into()),
                updated_at: None,
            })
        );
        assert!(parse_list_check("garbage").is_err());
    }

    #[test]
    fn recognises_rate_limit_messages() {
        // REST (HTTP 403 / 429)
        assert!(is_rate_limited(
            "HTTP 403: API rate limit exceeded for user ID 12345. \
             (https://api.github.com/repos/td72/vig/issues)"
        ));
        assert!(is_rate_limited(
            "HTTP 403: You have exceeded a secondary rate limit. \
             Please wait a few minutes before you try again."
        ));
        // GraphQL
        assert!(is_rate_limited(
            "GraphQL: API rate limit already exceeded (repository)"
        ));
        assert!(is_rate_limited("GraphQL error: RATE_LIMITED"));
        // Other gh failures are not rate limits.
        assert!(!is_rate_limited(
            "gh pr list failed: No such file or directory (os error 2)"
        ));
        assert!(!is_rate_limited("HTTP 404: Not Found"));
        assert!(!is_rate_limited(
            "JSON parse error: expected value at line 1"
        ));
        assert!(!is_rate_limited(""));
    }
}
