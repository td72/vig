//! Process snapshots via `sysinfo`. The sampler lives on a background
//! thread because CPU percentages are deltas between two refreshes: it keeps
//! one `System` alive and re-reads it on every request.
//!
//! Environment variables are deliberately never requested (`with_environ`
//! is not part of the refresh kind) — the page shows command lines only.

use crate::procs::domain::types::ProcessInfo;
use std::sync::mpsc;
use std::time::Duration;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind, Users};

pub struct Sampler {
    sys: System,
    users: Users,
}

fn refresh_kind() -> ProcessRefreshKind {
    ProcessRefreshKind::nothing()
        .with_cpu()
        .with_memory()
        .with_user(UpdateKind::OnlyIfNotSet)
        .with_cmd(UpdateKind::OnlyIfNotSet)
        .with_exe(UpdateKind::OnlyIfNotSet)
        .with_cwd(UpdateKind::OnlyIfNotSet)
        // Threads are not processes for this page.
        .without_tasks()
}

impl Sampler {
    /// Take the baseline refresh; the first `take()` after
    /// [`sysinfo::MINIMUM_CPU_UPDATE_INTERVAL`] yields meaningful CPU values.
    pub fn new() -> Self {
        let mut sys = System::new();
        sys.refresh_processes_specifics(ProcessesToUpdate::All, true, refresh_kind());
        Self {
            sys,
            users: Users::new_with_refreshed_list(),
        }
    }

    /// Refresh and copy every visible process out of `sysinfo`.
    pub fn take(&mut self) -> Vec<ProcessInfo> {
        self.sys
            .refresh_processes_specifics(ProcessesToUpdate::All, true, refresh_kind());
        self.sys
            .processes()
            .values()
            .map(|p| {
                let name = p.name().to_string_lossy().into_owned();
                let cmd: Vec<String> = p
                    .cmd()
                    .iter()
                    .map(|a| a.to_string_lossy().into_owned())
                    .collect();
                let cmd = if cmd.is_empty() {
                    name.clone()
                } else {
                    cmd.join(" ")
                };
                let user = p.user_id().map(|uid| {
                    self.users
                        .get_user_by_id(uid)
                        .map(|u| u.name().to_string())
                        .unwrap_or_else(|| uid.to_string())
                });
                ProcessInfo {
                    pid: p.pid().as_u32(),
                    ppid: p.parent().map(|pp| pp.as_u32()),
                    name,
                    cmd,
                    cpu: p.cpu_usage(),
                    rss: p.memory(),
                    // Kernel / system processes report no start time; a
                    // run time computed from 0 would be the epoch age.
                    run_time: (p.start_time() > 0).then(|| p.run_time()),
                    cwd: p.cwd().map(|c| c.to_string_lossy().into_owned()),
                    exe: p.exe().map(|e| e.to_string_lossy().into_owned()),
                    user,
                    status: p.status().to_string(),
                }
            })
            .collect()
    }
}

/// Start the sampler thread. Every `()` received on the returned sender
/// produces one snapshot on `out`; the thread ends when the sender is
/// dropped. The first request waits for the CPU baseline to settle.
pub fn spawn_worker<M: Send + 'static>(
    out: mpsc::Sender<M>,
    wrap: impl Fn(Vec<ProcessInfo>) -> M + Send + 'static,
) -> mpsc::Sender<()> {
    let (req_tx, req_rx) = mpsc::channel::<()>();
    std::thread::spawn(move || {
        let mut sampler = Sampler::new();
        // `sysinfo` reports 0% CPU for refreshes closer together than this.
        std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL.max(Duration::from_millis(200)));
        for () in req_rx {
            if out.send(wrap(sampler.take())).is_err() {
                break;
            }
        }
    });
    req_tx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_contains_this_process_with_its_parent() {
        let mut s = Sampler::new();
        let procs = s.take();
        let me = std::process::id();
        let mine = procs
            .iter()
            .find(|p| p.pid == me)
            .expect("own process is listed");
        assert!(!mine.name.is_empty());
        assert!(!mine.cmd.is_empty());
        assert!(mine.ppid.is_some());
        // Our own cwd and exe are readable without privileges.
        assert!(mine.cwd.is_some());
        assert!(mine.exe.is_some());
    }
}
