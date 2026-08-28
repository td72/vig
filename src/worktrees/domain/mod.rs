pub(crate) mod stash;
pub(crate) mod types;
pub(crate) mod worktree;

use anyhow::{anyhow, Result};
use std::path::Path;
use std::process::Command;

/// Run `git <args>` in `dir` and return its stdout. Stderr is folded into
/// the error message so the panes can show why a command failed.
pub(crate) fn git_output(dir: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(|e| anyhow!("failed to run git: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();
        let sub = args
            .iter()
            .find(|a| !a.starts_with('-') && !a.contains('='))
            .copied()
            .unwrap_or("");
        return Err(anyhow!(
            "git {sub} failed{}",
            if stderr.is_empty() {
                String::new()
            } else {
                format!(": {stderr}")
            }
        ));
    }
    Ok(output.stdout)
}
