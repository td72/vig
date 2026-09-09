//! `vig fixtures record <dir>`: run every `gh` command the GitHub and
//! Projects pages issue against the current repository and write their
//! output into `<dir>`, for API-free demo recordings and integration
//! tests (`core::gh_fixture` replays them with `VIG_GH_FIXTURE=<dir>`).
//! The only step of the demo pipeline that touches the GitHub API.

use crate::core::gh_fixture;
use crate::github::domain::actions::client as actions;
use crate::github::domain::client as gh;
use crate::projects::domain::{client as projects, graphql};
use anyhow::{Context, Result};
use std::path::Path;

/// Runs whose jobs and logs are recorded (the demos open the first few).
const RUNS_WITH_JOBS: usize = 3;

pub fn record(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    // Start clean so the set is exactly this recording (runs and their
    // logs churn; nothing would remove the old ones otherwise).
    for entry in std::fs::read_dir(dir).with_context(|| format!("read {}", dir.display()))? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "out") {
            std::fs::remove_file(&path).with_context(|| format!("remove {}", path.display()))?;
        }
    }
    gh_fixture::set_record_dir(Some(dir.to_path_buf()));
    let step = |what: &str| eprintln!("  {what}");

    step("gh auth status");
    gh::check_gh_available().map_err(anyhow::Error::msg)?;
    step("gh repo view (nameWithOwner)");
    let nwo = gh::repo_nwo().context("gh repo view")?;

    step("issues");
    let issues = gh::list_issues(50).map_err(anyhow::Error::msg)?;
    for i in &issues {
        gh::get_issue(i.number).map_err(anyhow::Error::msg)?;
    }
    step("list change check");
    gh::check_lists(&gh::ListWatermark::default()).map_err(anyhow::Error::msg)?;
    trim_check_fixture(dir)?;
    step("pull requests + stacks");
    let prs = gh::list_prs(50).map_err(anyhow::Error::msg)?;
    let _ = gh::list_pr_stacks(50);
    for p in &prs {
        gh::get_pr(p.number).map_err(anyhow::Error::msg)?;
    }
    step("workflow runs, jobs and logs");
    let runs = actions::list_runs(actions::RUN_LIST_LIMIT).map_err(anyhow::Error::msg)?;
    let mut missing_logs = Vec::new();
    for run in runs.iter().take(RUNS_WITH_JOBS) {
        let jobs = actions::list_jobs(run.id).map_err(anyhow::Error::msg)?;
        for job in &jobs {
            // Logs expire on GitHub's side (and are absent for queued
            // jobs), so a miss is reported rather than fatal.
            if let Err(e) = actions::fetch_job_log(run.id, job.id, false) {
                missing_logs.push(format!("run {} job {} ({e})", run.number, job.id));
            }
        }
    }
    for m in &missing_logs {
        eprintln!("warning: no log recorded for {m}");
    }

    step("linked projects");
    let info = projects::repo_info().map_err(anyhow::Error::msg)?;
    let _ = projects::viewer_login();
    for project in info.linked_projects() {
        step(&format!("project #{} ({})", project.number, project.title));
        let board = graphql::fetch_board(&project.owner.login, &project.owner.kind, project.number)
            .map_err(anyhow::Error::msg)?;
        // The change probe: replayed unchanged, so the demo never re-fetches.
        graphql::probe_updated_at(&project.owner.login, &project.owner.kind, project.number)
            .map_err(anyhow::Error::msg)?;
        for item in &board.items {
            let Some(number) = item.number() else {
                continue;
            };
            let repo = item.repository().filter(|r| !r.eq_ignore_ascii_case(&nwo));
            let _ = match item.kind() {
                crate::projects::domain::types::ItemKind::Issue => {
                    gh::get_issue_in(repo, number).map(|_| ())
                }
                crate::projects::domain::types::ItemKind::PullRequest => {
                    gh::get_pr_in(repo, number).map(|_| ())
                }
                _ => Ok(()),
            };
        }
    }
    gh_fixture::set_record_dir(None);
    let count = std::fs::read_dir(dir).map(|d| d.count()).unwrap_or(0);
    eprintln!("recorded {count} fixtures into {}", dir.display());
    Ok(())
}

/// Keep only what the change check reads from its `gh api -i` recording:
/// the status line, the `Etag` header and the body. The other response
/// headers describe the recording account (OAuth scopes, request ids)
/// and have no place in a checked-in fixture.
fn trim_check_fixture(dir: &Path) -> Result<()> {
    let args = gh::check_lists_args(None);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let path = dir.join(gh_fixture::name_for(&refs));
    let raw = std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let mut kept = String::new();
    let mut lines = raw.lines();
    if let Some(status) = lines.next() {
        kept.push_str(status);
        kept.push('\n');
    }
    let mut in_body = false;
    for line in lines {
        if in_body {
            kept.push_str(line);
            kept.push('\n');
        } else if line.is_empty() {
            kept.push('\n');
            in_body = true;
        } else if line.to_ascii_lowercase().starts_with("etag:") {
            kept.push_str(line);
            kept.push('\n');
        }
    }
    std::fs::write(&path, kept).with_context(|| format!("write {}", path.display()))
}
