use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct GhAuthor {
    pub login: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GhLabel {
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GhComment {
    pub author: Option<GhAuthor>,
    pub body: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GhReview {
    pub author: Option<GhAuthor>,
    pub body: String,
    pub state: String,
    #[serde(rename = "submittedAt")]
    pub submitted_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GhStatusCheck {
    pub name: String,
    pub status: String,
    pub conclusion: Option<String>,
    #[serde(rename = "workflowName")]
    pub workflow_name: Option<String>,
}

// Issue list item — some fields populated by serde only
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct GhIssueListItem {
    pub number: u64,
    pub title: String,
    pub state: String,
    pub author: Option<GhAuthor>,
    pub labels: Vec<GhLabel>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

// Issue detail
#[derive(Debug, Clone, Deserialize)]
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
#[derive(Debug, Clone, Deserialize)]
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
#[derive(Debug, Clone, Deserialize)]
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
