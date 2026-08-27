use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GhAuthor {
    pub login: String,
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
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "reviewDecision")]
    pub review_decision: Option<String>,
    #[serde(rename = "isDraft")]
    pub is_draft: bool,
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
}
