//! Thin wrappers around `gh run …` and `gh api` for the Actions page.
//! Every command here is read-only: `run list`, `run view` and a GET of the
//! job-log endpoint. Nothing reruns, cancels or deletes anything.

use crate::actions::domain::types::{Job, JobsResponse, WorkflowRun};
use crate::github::domain::client::{run_gh, run_gh_json};
use crate::github::domain::disk_cache;

/// Runs fetched per list request.
pub const RUN_LIST_LIMIT: usize = 50;

const RUN_FIELDS: &str =
    "databaseId,number,name,workflowName,status,conclusion,headBranch,event,createdAt,updatedAt,url";

pub fn list_runs(limit: usize) -> Result<Vec<WorkflowRun>, String> {
    run_gh_json(
        &[
            "run",
            "list",
            "--limit",
            &limit.to_string(),
            "--json",
            RUN_FIELDS,
        ],
        "gh run list failed",
    )
}

pub fn list_jobs(run_id: u64) -> Result<Vec<Job>, String> {
    let resp: JobsResponse = run_gh_json(
        &["run", "view", &run_id.to_string(), "--json", "jobs"],
        "gh run view failed",
    )?;
    Ok(resp.jobs)
}

/// Raw log text of one job. Completed runs come from `gh run view --log
/// --job`, which `gh` refuses for runs still in progress; those fall back
/// to the REST job-log endpoint, which serves whatever has been written so
/// far. `in_progress` skips straight to the fallback.
pub fn fetch_job_log(run_id: u64, job_id: u64, in_progress: bool) -> Result<String, String> {
    if !in_progress {
        match run_gh(
            &[
                "run",
                "view",
                &run_id.to_string(),
                "--log",
                "--job",
                &job_id.to_string(),
            ],
            "gh run view --log failed",
        ) {
            Ok(out) => return Ok(String::from_utf8_lossy(&out).into_owned()),
            Err(e) if !e.contains("in progress") => return Err(e),
            Err(_) => {}
        }
    }
    // Runner output carries ANSI colour codes, which `gh api` refuses to
    // print unless told otherwise; they are stripped when the log is parsed.
    let out = run_gh(
        &[
            "api",
            "--allow-escape-sequences",
            &format!("repos/{{owner}}/{{repo}}/actions/jobs/{job_id}/logs"),
        ],
        "gh api (job logs) failed",
    )?;
    Ok(String::from_utf8_lossy(&out).into_owned())
}

// === Disk cache (same directory as the GitHub page) ===

pub fn load_run_list() -> Option<Vec<WorkflowRun>> {
    disk_cache::load_json(&disk_cache::cache_dir()?.join("runs.json"))
}

pub fn save_run_list(runs: &[WorkflowRun]) {
    if let Some(dir) = disk_cache::cache_dir() {
        disk_cache::save_json(&dir.join("runs.json"), &runs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::domain::types::RunState;

    const RUN_LIST: &str = r#"[
{"conclusion":"skipped","createdAt":"2026-08-28T08:17:23Z","databaseId":33154752341,"event":"pull_request","headBranch":"feat/docker-view","name":"Release","number":156,"status":"completed","updatedAt":"2026-08-28T08:17:24Z","url":"https://github.com/td72/vig/actions/runs/33154752341","workflowName":"Release"},
{"conclusion":"","createdAt":"2026-08-28T08:17:23Z","databaseId":33154751728,"event":"push","headBranch":"main","name":"CI","number":188,"status":"in_progress","updatedAt":"2026-08-28T08:17:23Z","url":"https://github.com/td72/vig/actions/runs/33154751728","workflowName":"CI"}
]"#;

    #[test]
    fn parses_run_list_json() {
        let runs: Vec<WorkflowRun> = serde_json::from_str(RUN_LIST).unwrap();
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].id, 33154752341);
        assert_eq!(runs[0].number, 156);
        assert_eq!(runs[0].title(), "Release");
        assert_eq!(runs[0].head_branch, "feat/docker-view");
        assert_eq!(runs[0].event, "pull_request");
        assert_eq!(runs[0].state(), RunState::Skipped);
        assert_eq!(runs[1].state(), RunState::InProgress);
        assert!(runs[1].url.ends_with("/runs/33154751728"));
        // Cache round trip keeps every field.
        let json = serde_json::to_string(&runs).unwrap();
        let again: Vec<WorkflowRun> = serde_json::from_str(&json).unwrap();
        assert_eq!(again[1].id, runs[1].id);
    }

    const JOBS: &str = r#"{"jobs":[
{"completedAt":"2026-08-28T08:18:43Z","conclusion":"success","databaseId":98794749357,"name":"test (macos-latest)","startedAt":"2026-08-28T08:17:27Z","status":"completed",
 "steps":[{"completedAt":"2026-08-28T08:17:32Z","conclusion":"success","name":"Set up job","number":1,"startedAt":"2026-08-28T08:17:29Z","status":"completed"},
          {"completedAt":"2026-08-28T08:18:29Z","conclusion":"failure","name":"cargo test","number":5,"startedAt":"2026-08-28T08:18:01Z","status":"completed"}],
 "url":"https://github.com/td72/vig/actions/runs/33154751728/job/98794749357"},
{"completedAt":null,"conclusion":null,"databaseId":98794749526,"name":"test (ubuntu-latest)","startedAt":"2026-08-28T08:17:26Z","status":"in_progress","steps":[],"url":""}
]}"#;

    #[test]
    fn parses_jobs_json() {
        let resp: JobsResponse = serde_json::from_str(JOBS).unwrap();
        let jobs = resp.jobs;
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].id, 98794749357);
        assert_eq!(jobs[0].name, "test (macos-latest)");
        assert_eq!(jobs[0].state(), RunState::Success);
        assert_eq!(jobs[0].steps.len(), 2);
        assert_eq!(jobs[0].steps[1].name, "cargo test");
        assert_eq!(jobs[0].steps[1].state(), RunState::Failure);
        assert_eq!(jobs[1].state(), RunState::InProgress);
        assert!(jobs[1].completed_at.is_none());
        assert!(jobs[1].steps.is_empty());
    }
}
