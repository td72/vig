//! Plain data for the Procs page: one process, one listening port, and the
//! sort order of the process tree. Everything here is a snapshot value —
//! nothing holds a handle to a live process.

use std::cmp::Ordering;

/// One process at snapshot time.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessInfo {
    pub pid: u32,
    pub ppid: Option<u32>,
    /// Short name (the executable's basename).
    pub name: String,
    /// Full command line (program + arguments). Falls back to `name` when
    /// the command line is not readable.
    pub cmd: String,
    /// CPU usage in percent of one core since the previous snapshot.
    pub cpu: f32,
    /// Resident set size in bytes.
    pub rss: u64,
    /// Seconds the process has been running; `None` when the OS reports no
    /// start time (kernel and some system processes).
    pub run_time: Option<u64>,
    /// Working directory; `None` when it needs privileges we do not have.
    pub cwd: Option<String>,
    /// Executable path; `None` when it needs privileges we do not have.
    pub exe: Option<String>,
    /// Owner's user name (or numeric uid); `None` when unknown.
    pub user: Option<String>,
    /// Scheduler state (`Run`, `Sleep`, …) as reported by the OS.
    pub status: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Proto {
    Tcp,
    Udp,
}

impl Proto {
    pub fn label(self) -> &'static str {
        match self {
            Proto::Tcp => "tcp",
            Proto::Udp => "udp",
        }
    }
}

/// One listening socket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortEntry {
    pub proto: Proto,
    /// Bound address as printed by the tool (`*`, `127.0.0.1`, `[::1]`, …).
    pub addr: String,
    pub port: u16,
    /// Owning process; `None` when the current user may not see it.
    pub pid: Option<u32>,
    pub name: Option<String>,
}

impl PortEntry {
    /// `addr:port`.
    pub fn address(&self) -> String {
        format!("{}:{}", self.addr, self.port)
    }

    /// Stable display order: port, then protocol, then address.
    pub fn sort_key(&self) -> (u16, Proto, &str) {
        (self.port, self.proto, self.addr.as_str())
    }
}

/// Order of the process tree's roots (and of siblings under one parent).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortKey {
    #[default]
    Cpu,
    Mem,
    Pid,
}

impl SortKey {
    /// `s` cycles CPU → MEM → PID → CPU.
    pub fn next(self) -> Self {
        match self {
            SortKey::Cpu => SortKey::Mem,
            SortKey::Mem => SortKey::Pid,
            SortKey::Pid => SortKey::Cpu,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SortKey::Cpu => "CPU",
            SortKey::Mem => "MEM",
            SortKey::Pid => "PID",
        }
    }

    /// Compare two processes under this key. CPU and MEM sort descending
    /// with the pid as a tie-breaker so refreshes do not shuffle equal rows.
    pub fn compare(self, a: &ProcessInfo, b: &ProcessInfo) -> Ordering {
        match self {
            SortKey::Cpu => b
                .cpu
                .partial_cmp(&a.cpu)
                .unwrap_or(Ordering::Equal)
                .then(a.pid.cmp(&b.pid)),
            SortKey::Mem => b.rss.cmp(&a.rss).then(a.pid.cmp(&b.pid)),
            SortKey::Pid => a.pid.cmp(&b.pid),
        }
    }
}

/// Sort processes in place under `key` (stable).
pub fn sort_processes(procs: &mut [ProcessInfo], key: SortKey) {
    procs.sort_by(|a, b| key.compare(a, b));
}

/// Elapsed time as `3h 12m` / `2d 4h` / `45s`.
pub fn format_elapsed(secs: u64) -> String {
    let (d, h, m, s) = (
        secs / 86_400,
        (secs / 3600) % 24,
        (secs / 60) % 60,
        secs % 60,
    );
    if d > 0 {
        format!("{d}d {h}h")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else if m > 0 {
        format!("{m}m {s}s")
    } else {
        format!("{s}s")
    }
}

/// Test fixture: a process with the given tree position and load.
#[cfg(test)]
pub(crate) fn proc(pid: u32, ppid: Option<u32>, cpu: f32, rss: u64) -> ProcessInfo {
    ProcessInfo {
        pid,
        ppid,
        name: format!("p{pid}"),
        cmd: format!("p{pid} --arg"),
        cpu,
        rss,
        run_time: None,
        cwd: None,
        exe: None,
        user: None,
        status: "Run".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sort_key_cycles_cpu_mem_pid() {
        assert_eq!(SortKey::Cpu.next(), SortKey::Mem);
        assert_eq!(SortKey::Mem.next(), SortKey::Pid);
        assert_eq!(SortKey::Pid.next(), SortKey::Cpu);
        assert_eq!(SortKey::default(), SortKey::Cpu);
    }

    #[test]
    fn sort_orders_by_key_with_pid_tie_break() {
        let mut v = vec![
            proc(30, None, 1.0, 500),
            proc(10, None, 5.0, 100),
            proc(20, None, 1.0, 900),
        ];
        sort_processes(&mut v, SortKey::Cpu);
        assert_eq!(v.iter().map(|p| p.pid).collect::<Vec<_>>(), [10, 20, 30]);
        sort_processes(&mut v, SortKey::Mem);
        assert_eq!(v.iter().map(|p| p.pid).collect::<Vec<_>>(), [20, 30, 10]);
        sort_processes(&mut v, SortKey::Pid);
        assert_eq!(v.iter().map(|p| p.pid).collect::<Vec<_>>(), [10, 20, 30]);
    }

    #[test]
    fn elapsed_formats() {
        assert_eq!(format_elapsed(45), "45s");
        assert_eq!(format_elapsed(125), "2m 5s");
        assert_eq!(format_elapsed(3600 * 3 + 60 * 12), "3h 12m");
        assert_eq!(format_elapsed(86_400 * 2 + 3600 * 4), "2d 4h");
    }

    #[test]
    fn port_entry_address() {
        let e = PortEntry {
            proto: Proto::Tcp,
            addr: "[::1]".into(),
            port: 8080,
            pid: None,
            name: None,
        };
        assert_eq!(e.address(), "[::1]:8080");
    }
}
