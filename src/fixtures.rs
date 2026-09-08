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
    step("pull requests + stacks");
    let prs = gh::list_prs(50).map_err(anyhow::Error::msg)?;
    let _ = gh::list_pr_stacks(50);
    for p in &prs {
        gh::get_pr(p.number).map_err(anyhow::Error::msg)?;
    }
    step("workflow runs, jobs and logs");
    let runs = actions::list_runs(actions::RUN_LIST_LIMIT).map_err(anyhow::Error::msg)?;
    for run in runs.iter().take(RUNS_WITH_JOBS) {
        let jobs = actions::list_jobs(run.id).map_err(anyhow::Error::msg)?;
        for job in &jobs {
            let _ = actions::fetch_job_log(run.id, job.id, false);
        }
    }

    step("linked projects");
    let info = projects::repo_info().map_err(anyhow::Error::msg)?;
    let _ = projects::viewer_login();
    for project in info.linked_projects() {
        step(&format!("project #{} ({})", project.number, project.title));
        let board = graphql::fetch_board(&project.owner.login, &project.owner.kind, project.number)
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
