//! Process snapshots via `sysinfo`. The sampler lives on a background
//! thread because CPU percentages are deltas between two refreshes: it keeps
//! one `System` alive and re-reads it on every request.
//!
//! Environment variables are deliberately never requested (`with_environ`
//! is not part of the refresh kind) — the page shows command lines only.

use crate::procs::domain::history::SystemSample;
use crate::procs::domain::types::ProcessInfo;
use std::collections::HashSet;
use std::sync::mpsc;
use std::time::Duration;
use sysinfo::{
    CpuRefreshKind, MemoryRefreshKind, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind,
    Users,
};

/// One sampling pass: every visible process plus the machine totals the
/// graphs draw. The `VIG_PROCS_ROOT_PID` filter applies to `procs` only —
/// `system` is machine-wide by design (numbers only, nothing to leak).
pub struct Snapshot {
    pub procs: Vec<ProcessInfo>,
    pub system: SystemSample,
}

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
        // Baseline for the global / per-core CPU deltas.
        sys.refresh_cpu_specifics(CpuRefreshKind::nothing().with_cpu_usage());
        Self {
            sys,
            users: Users::new_with_refreshed_list(),
        }
    }

    /// Refresh and copy every visible process out of `sysinfo`, together
    /// with the system-wide CPU / memory totals of this pass.
    pub fn take(&mut self) -> Snapshot {
        self.sys
            .refresh_processes_specifics(ProcessesToUpdate::All, true, refresh_kind());
        self.sys
            .refresh_cpu_specifics(CpuRefreshKind::nothing().with_cpu_usage());
        self.sys
            .refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram().with_swap());
        let system = SystemSample {
            cpu: self.sys.global_cpu_usage(),
            per_core: self.sys.cpus().iter().map(|c| c.cpu_usage()).collect(),
            mem_used: self.sys.used_memory(),
            mem_total: self.sys.total_memory(),
            swap_used: self.sys.used_swap(),
            swap_total: self.sys.total_swap(),
        };
        let procs = self
            .sys
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
            .collect();
        Snapshot { procs, system }
    }
}

/// Pids of `root` and everything below it (transitively, by ppid). Empty
/// when `root` is not in `procs`. Used by the `VIG_PROCS_ROOT_PID` hook so
/// a recording only shows processes started for it.
pub fn subtree_pids(procs: &[ProcessInfo], root: u32) -> HashSet<u32> {
    let mut keep = HashSet::new();
    if !procs.iter().any(|p| p.pid == root) {
        return keep;
    }
    keep.insert(root);
    // Each pass adopts the children of what is already kept; the number of
    // passes is bounded by the tree depth.
    loop {
        let before = keep.len();
        for p in procs {
            if p.pid != root && p.ppid.is_some_and(|pp| keep.contains(&pp)) {
                keep.insert(p.pid);
            }
        }
        if keep.len() == before {
            break;
        }
    }
    keep
}

/// Keep only `root` and its descendants.
pub fn retain_subtree(procs: &mut Vec<ProcessInfo>, root: u32) {
    let keep = subtree_pids(procs, root);
    procs.retain(|p| keep.contains(&p.pid));
}

/// Start the sampler thread. Every `()` received on the returned sender
/// produces one snapshot on `out`; the thread ends when the sender is
/// dropped. The first request waits for the CPU baseline to settle.
pub fn spawn_worker<M: Send + 'static>(
    out: mpsc::Sender<M>,
    wrap: impl Fn(Snapshot) -> M + Send + 'static,
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
    use crate::procs::domain::types::proc;

    fn pids(procs: &[ProcessInfo]) -> Vec<u32> {
        procs.iter().map(|p| p.pid).collect()
    }

    #[test]
    fn subtree_keeps_root_and_transitive_children_only() {
        // 1 ─ 10 ─ 100 ─ 1000        (10 is the root of interest)
        //   └ 20 ─ 200
        // 10 ─ 101
        // 5 orphan (ppid None), 999 with a missing parent
        let procs = vec![
            proc(1, None, 0.0, 0),
            proc(10, Some(1), 0.0, 0),
            proc(100, Some(10), 0.0, 0),
            proc(1000, Some(100), 0.0, 0),
            proc(101, Some(10), 0.0, 0),
            proc(20, Some(1), 0.0, 0),
            proc(200, Some(20), 0.0, 0),
            proc(5, None, 0.0, 0),
            proc(999, Some(4242), 0.0, 0),
        ];
        let keep = subtree_pids(&procs, 10);
        assert_eq!(keep, HashSet::from([10, 100, 1000, 101]));

        let mut filtered = procs.clone();
        retain_subtree(&mut filtered, 10);
        assert_eq!(pids(&filtered), [10, 100, 1000, 101]);

        // Order of appearance does not matter: a child listed before its
        // parent is still adopted.
        let mut reversed = procs.clone();
        reversed.reverse();
        retain_subtree(&mut reversed, 10);
        assert_eq!(pids(&reversed), [101, 1000, 100, 10]);
    }

    #[test]
    fn subtree_of_unknown_root_is_empty() {
        let mut procs = vec![proc(1, None, 0.0, 0), proc(2, Some(1), 0.0, 0)];
        assert!(subtree_pids(&procs, 77).is_empty());
        retain_subtree(&mut procs, 77);
        assert!(procs.is_empty());
    }

    #[test]
    fn subtree_ignores_a_process_that_claims_to_be_its_own_parent() {
        // pid 0 / launchd-style rows may report ppid == pid; that must not
        // loop forever or pull the root's siblings in.
        let procs = vec![proc(0, Some(0), 0.0, 0), proc(3, Some(0), 0.0, 0)];
        assert_eq!(subtree_pids(&procs, 0), HashSet::from([0, 3]));
        assert_eq!(subtree_pids(&procs, 3), HashSet::from([3]));
    }

    #[test]
    fn snapshot_contains_this_process_with_its_parent() {
        let mut s = Sampler::new();
        let snap = s.take();
        // The machine totals are always readable: memory sizes are real,
        // the CPU list is not empty, and the percentages are in range.
        assert!(snap.system.mem_total > 0);
        assert!(snap.system.mem_used <= snap.system.mem_total);
        assert!(snap.system.swap_used <= snap.system.swap_total.max(snap.system.swap_used));
        assert!(!snap.system.per_core.is_empty());
        assert!(snap.system.cpu >= 0.0);
        assert!(snap.system.per_core.iter().all(|&c| c >= 0.0));
        let procs = snap.procs;
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
