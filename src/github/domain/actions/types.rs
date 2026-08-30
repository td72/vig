//! Data types for the GitHub page's Workflow Runs column: workflow runs, jobs and steps as
//! returned by `gh run list --json` / `gh run view --json jobs`.

use serde::{Deserialize, Serialize};

/// One workflow run from `gh run list --json …`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkflowRun {
    #[serde(rename = "databaseId")]
    pub id: u64,
    pub number: u64,
    #[serde(default)]
    pub name: String,
    #[serde(rename = "workflowName", default)]
    pub workflow_name: String,
    /// `queued` / `in_progress` / `completed` / `waiting` / …
    #[serde(default)]
    pub status: String,
    /// `success` / `failure` / `cancelled` / `skipped` / … (empty until done).
    #[serde(default)]
    pub conclusion: String,
    #[serde(rename = "headBranch", default)]
    pub head_branch: String,
    #[serde(default)]
    pub event: String,
    #[serde(rename = "createdAt", default)]
    pub created_at: String,
    #[serde(rename = "updatedAt", default)]
    pub updated_at: String,
    #[serde(default)]
    pub url: String,
}

/// One job of a run (`gh run view --json jobs`).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Job {
    #[serde(rename = "databaseId")]
    pub id: u64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub conclusion: Option<String>,
    #[serde(rename = "startedAt", default)]
    pub started_at: Option<String>,
    #[serde(rename = "completedAt", default)]
    pub completed_at: Option<String>,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub steps: Vec<Step>,
}

/// One step of a job.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Step {
    #[serde(default)]
    pub number: u64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub conclusion: Option<String>,
    #[serde(rename = "startedAt", default)]
    pub started_at: Option<String>,
    #[serde(rename = "completedAt", default)]
    pub completed_at: Option<String>,
}

/// The `jobs` wrapper object `gh run view --json jobs` prints.
#[derive(Debug, Deserialize)]
pub struct JobsResponse {
    #[serde(default)]
    pub jobs: Vec<Job>,
}

/// Coarse state shared by runs, jobs and steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunState {
    Queued,
    InProgress,
    Success,
    Failure,
    Cancelled,
    Skipped,
    Other,
}

impl RunState {
    /// Derive the state from the `status` / `conclusion` pair that every
    /// Actions object carries (`conclusion` is empty until it completes).
    pub fn from_status(status: &str, conclusion: Option<&str>) -> Self {
        match status {
            "queued" | "waiting" | "requested" | "pending" => return Self::Queued,
            "in_progress" => return Self::InProgress,
            _ => {}
        }
        match conclusion.unwrap_or("") {
            "success" => Self::Success,
            "failure" | "timed_out" | "startup_failure" | "action_required" => Self::Failure,
            "cancelled" => Self::Cancelled,
            "skipped" | "neutral" => Self::Skipped,
            "" if status == "completed" => Self::Other,
            "" => Self::Queued,
            _ => Self::Other,
        }
    }

    /// Still queued or running: the object will change and is worth polling.
    pub fn is_active(self) -> bool {
        matches!(self, Self::Queued | Self::InProgress)
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::Queued => "◯",
            Self::InProgress => "◐",
            Self::Success => "✓",
            Self::Failure => "✗",
            Self::Cancelled => "⊘",
            Self::Skipped => "○",
            Self::Other => "?",
        }
    }
}

impl WorkflowRun {
    pub fn state(&self) -> RunState {
        RunState::from_status(&self.status, Some(&self.conclusion))
    }

    /// Workflow name, falling back to the run name for older `gh` output.
    pub fn title(&self) -> &str {
        if self.workflow_name.is_empty() {
            &self.name
        } else {
            &self.workflow_name
        }
    }
}

impl Job {
    pub fn state(&self) -> RunState {
        RunState::from_status(&self.status, self.conclusion.as_deref())
    }
}

impl Step {
    pub fn state(&self) -> RunState {
        RunState::from_status(&self.status, self.conclusion.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_from_status_and_conclusion() {
        use RunState::*;
        assert_eq!(RunState::from_status("queued", Some("")), Queued);
        assert_eq!(RunState::from_status("waiting", None), Queued);
        assert_eq!(RunState::from_status("in_progress", Some("")), InProgress);
        assert_eq!(RunState::from_status("completed", Some("success")), Success);
        assert_eq!(RunState::from_status("completed", Some("failure")), Failure);
        assert_eq!(
            RunState::from_status("completed", Some("timed_out")),
            Failure
        );
        assert_eq!(
            RunState::from_status("completed", Some("cancelled")),
            Cancelled
        );
        assert_eq!(RunState::from_status("completed", Some("skipped")), Skipped);
        assert_eq!(RunState::from_status("completed", Some("")), Other);
        assert!(Queued.is_active());
        assert!(InProgress.is_active());
        assert!(!Success.is_active());
    }
}
