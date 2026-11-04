use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GhAuthor {
    pub login: String,
}

/// Someone a PR's review was requested from: a user (`login`) or a team
/// (`slug`, with its display `name`), as `gh pr view --json reviewRequests`
/// reports them. Only the pending requests appear here — a submitted
/// review moves its author to `reviews`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct GhReviewRequest {
    #[serde(default)]
    pub login: Option<String>,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

impl GhReviewRequest {
    /// The user's login or the team's slug (its name when the slug is
    /// missing).
    pub fn handle(&self) -> &str {
        self.login
            .as_deref()
            .or(self.slug.as_deref())
            .or(self.name.as_deref())
            .unwrap_or("?")
    }
}

/// The logins of `people` (assignees).
pub fn logins(people: &[GhAuthor]) -> Vec<&str> {
    people.iter().map(|p| p.login.as_str()).collect()
}

/// The handles of pending review requests.
pub fn reviewer_handles(requests: &[GhReviewRequest]) -> Vec<&str> {
    requests.iter().map(GhReviewRequest::handle).collect()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GhLabel {
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GhComment {
    pub author: Option<GhAuthor>,
    pub body: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GhReview {
    pub author: Option<GhAuthor>,
    pub body: String,
    pub state: String,
    pub id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GhStatusCheck {
    pub name: String,
    pub status: String,
    pub conclusion: Option<String>,
    #[serde(rename = "workflowName")]
    pub workflow_name: Option<String>,
    #[serde(rename = "startedAt")]
    pub started_at: Option<String>,
    #[serde(rename = "completedAt")]
    pub completed_at: Option<String>,
    #[serde(rename = "detailsUrl")]
    pub details_url: Option<String>,
}

// Issue list item — some fields populated by serde only
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GhIssueListItem {
    pub number: u64,
    pub title: String,
    pub state: String,
    pub author: Option<GhAuthor>,
    pub labels: Vec<GhLabel>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    /// Parent issue when this is a sub-issue (absent in caches written
    /// before the field existed).
    #[serde(default)]
    pub parent: Option<GhIssueRef>,
    /// Assignees (absent in caches written before the field existed).
    #[serde(default)]
    pub assignees: Vec<GhAuthor>,
}

/// Minimal reference to another issue (e.g. a sub-issue's parent).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GhIssueRef {
    pub number: u64,
}

// Issue detail
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GhIssueDetail {
    pub number: u64,
    pub title: String,
    pub state: String,
    pub author: Option<GhAuthor>,
    pub body: String,
    pub comments: Vec<GhComment>,
    pub labels: Vec<GhLabel>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    /// Assignees (absent in caches written before the field existed).
    #[serde(default)]
    pub assignees: Vec<GhAuthor>,
}

// PR list item — some fields populated by serde only
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GhPrListItem {
    pub number: u64,
    pub title: String,
    pub state: String,
    pub author: Option<GhAuthor>,
    pub labels: Vec<GhLabel>,
    #[serde(rename = "headRefName")]
    pub head_ref_name: String,
    /// Branch this PR merges into (absent in caches written before the
    /// field existed).
    #[serde(rename = "baseRefName", default)]
    pub base_ref_name: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "reviewDecision")]
    pub review_decision: Option<String>,
    #[serde(rename = "isDraft")]
    pub is_draft: bool,
    /// Membership of a GitHub pull request Stack (as created by `gh stack`),
    /// merged in from a separate GraphQL query; absent in older caches.
    #[serde(default)]
    pub stack: Option<GhPrStackRef>,
    /// Assignees and pending review requests (absent in caches written
    /// before the fields existed).
    #[serde(default)]
    pub assignees: Vec<GhAuthor>,
    #[serde(rename = "reviewRequests", default)]
    pub review_requests: Vec<GhReviewRequest>,
}

/// A PR's place in a GitHub Stack: `position` is 1 at the bottom (closest to
/// the stack's base branch) and grows towards the top.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct GhPrStackRef {
    pub number: u64,
    pub position: u32,
    pub size: u32,
}

// PR detail
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GhPrDetail {
    pub number: u64,
    pub title: String,
    pub state: String,
    pub author: Option<GhAuthor>,
    pub body: String,
    pub comments: Vec<GhComment>,
    pub reviews: Vec<GhReview>,
    pub labels: Vec<GhLabel>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "reviewDecision")]
    pub review_decision: Option<String>,
    #[serde(rename = "statusCheckRollup")]
    pub status_check_rollup: Option<Vec<GhStatusCheck>>,
    pub additions: u64,
    pub deletions: u64,
    #[serde(rename = "changedFiles")]
    pub changed_files: u64,
    #[serde(rename = "headRefName")]
    pub head_ref_name: String,
    /// Assignees and pending review requests (absent in caches written
    /// before the fields existed).
    #[serde(default)]
    pub assignees: Vec<GhAuthor>,
    #[serde(rename = "reviewRequests", default)]
    pub review_requests: Vec<GhReviewRequest>,
    /// Still a draft (absent in older caches).
    #[serde(rename = "isDraft", default)]
    pub is_draft: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `gh pr view --json reviewRequests,assignees`: users carry a
    /// `login`, teams a `slug` and `name`; older caches have neither field.
    #[test]
    fn review_requests_and_assignees_deserialize_from_gh_and_old_caches() {
        let json = r#"{"number":1,"title":"t","state":"OPEN","author":null,"labels":[],
            "headRefName":"f","baseRefName":"main","createdAt":"","reviewDecision":null,"isDraft":false,
            "assignees":[{"id":"U1","login":"alice","name":"Alice"}],
            "reviewRequests":[{"__typename":"User","login":"carol"},
                              {"__typename":"Team","name":"Core team","slug":"core"}]}"#;
        let pr: GhPrListItem = serde_json::from_str(json).unwrap();
        assert_eq!(logins(&pr.assignees), ["alice"]);
        assert_eq!(reviewer_handles(&pr.review_requests), ["carol", "core"]);

        let old = r#"{"number":1,"title":"t","state":"OPEN","author":null,"labels":[],
            "headRefName":"f","createdAt":"","reviewDecision":null,"isDraft":false}"#;
        let pr: GhPrListItem = serde_json::from_str(old).unwrap();
        assert!(pr.assignees.is_empty() && pr.review_requests.is_empty());
    }
}
