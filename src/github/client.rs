use crate::github::types::*;
use std::process::Command;

pub fn check_gh_available() -> Result<(), String> {
    let output = Command::new("gh")
        .args(["auth", "status"])
        .output()
        .map_err(|e| format!("gh not found: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(stderr.trim().to_string())
    }
}

pub fn list_issues(limit: usize) -> Result<Vec<GhIssueListItem>, String> {
    let output = Command::new("gh")
        .args([
            "issue",
            "list",
            "--json",
            "number,title,state,author,labels,createdAt",
            "--limit",
            &limit.to_string(),
        ])
        .output()
        .map_err(|e| format!("gh issue list failed: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(stderr.trim().to_string());
    }
    serde_json::from_slice(&output.stdout).map_err(|e| format!("JSON parse error: {e}"))
}

pub fn list_prs(limit: usize) -> Result<Vec<GhPrListItem>, String> {
    let output = Command::new("gh")
        .args([
            "pr",
            "list",
            "--json",
            "number,title,state,author,labels,headRefName,createdAt,reviewDecision,isDraft",
            "--limit",
            &limit.to_string(),
        ])
        .output()
        .map_err(|e| format!("gh pr list failed: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(stderr.trim().to_string());
    }
    serde_json::from_slice(&output.stdout).map_err(|e| format!("JSON parse error: {e}"))
}

pub fn get_issue(number: u64) -> Result<GhIssueDetail, String> {
    let output = Command::new("gh")
        .args([
            "issue",
            "view",
            &number.to_string(),
            "--json",
            "number,title,state,author,body,comments,labels,createdAt",
        ])
        .output()
        .map_err(|e| format!("gh issue view failed: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(stderr.trim().to_string());
    }
    serde_json::from_slice(&output.stdout).map_err(|e| format!("JSON parse error: {e}"))
}

pub fn get_pr(number: u64) -> Result<GhPrDetail, String> {
    let output = Command::new("gh")
        .args([
            "pr",
            "view",
            &number.to_string(),
            "--json",
            "number,title,state,author,body,comments,reviews,labels,createdAt,reviewDecision,statusCheckRollup,additions,deletions,changedFiles,headRefName",
        ])
        .output()
        .map_err(|e| format!("gh pr view failed: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(stderr.trim().to_string());
    }
    serde_json::from_slice(&output.stdout).map_err(|e| format!("JSON parse error: {e}"))
}
