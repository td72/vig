//! Sample history for the Procs page graphs: a fixed-capacity ring buffer,
//! the system-wide totals of one snapshot, and the per-process series the
//! detail pane draws. Everything here is pure data — sampling happens in
//! `snapshot.rs`, accumulation only while the page is active.

use crate::procs::domain::types::ProcessInfo;
use std::collections::{HashMap, VecDeque};

/// A ring buffer that keeps the last `capacity` values pushed.
#[derive(Debug, Clone)]
pub struct Ring<T> {
    buf: VecDeque<T>,
    capacity: usize,
}

impl<T> Ring<T> {
    /// A ring keeping at most `capacity` values (at least 1).
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            buf: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Append `value`, evicting the oldest value when full.
    pub fn push(&mut self, value: T) {
        if self.buf.len() == self.capacity {
            self.buf.pop_front();
        }
        self.buf.push_back(value);
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    #[cfg(test)]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Oldest → newest.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &T> + ExactSizeIterator {
        self.buf.iter()
    }

    /// The most recent value.
    pub fn last(&self) -> Option<&T> {
        self.buf.back()
    }
}

/// Machine-wide totals of one snapshot. Numbers only — nothing here can
/// name a process, a user or the machine.
#[derive(Debug, Clone, PartialEq)]
pub struct SystemSample {
    /// Global CPU usage in percent (0–100, all cores combined).
    pub cpu: f32,
    /// Per-core usage in percent, in `sysinfo`'s core order.
    pub per_core: Vec<f32>,
    /// Used / total physical memory in bytes.
    pub mem_used: u64,
    pub mem_total: u64,
    /// Used / total swap in bytes; `swap_total == 0` means no swap.
    pub swap_used: u64,
    pub swap_total: u64,
}

/// CPU% and RSS series of one process.
#[derive(Debug, Clone)]
pub struct ProcSeries {
    pub cpu: Ring<f32>,
    pub rss: Ring<u64>,
}

impl ProcSeries {
    fn new(capacity: usize) -> Self {
        Self {
            cpu: Ring::new(capacity),
            rss: Ring::new(capacity),
        }
    }
}

/// Per-pid history, bounded two ways: each series keeps at most `capacity`
/// samples, and only pids seen in the latest snapshot are tracked at all —
/// a process that disappears is dropped on the next [`record`](Self::record).
#[derive(Debug, Clone)]
pub struct ProcHistory {
    capacity: usize,
    map: HashMap<u32, ProcSeries>,
}

impl ProcHistory {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            map: HashMap::new(),
        }
    }

    /// Append one snapshot's values and forget every pid it does not contain.
    pub fn record(&mut self, procs: &[ProcessInfo]) {
        for p in procs {
            let series = self
                .map
                .entry(p.pid)
                .or_insert_with(|| ProcSeries::new(self.capacity));
            series.cpu.push(p.cpu);
            series.rss.push(p.rss);
        }
        let alive: std::collections::HashSet<u32> = procs.iter().map(|p| p.pid).collect();
        self.map.retain(|pid, _| alive.contains(pid));
    }

    pub fn series(&self, pid: u32) -> Option<&ProcSeries> {
        self.map.get(&pid)
    }

    /// Number of pids currently tracked.
    #[cfg(test)]
    pub fn tracked(&self) -> usize {
        self.map.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procs::domain::types::proc;

    #[test]
    fn ring_pushes_and_evicts_oldest() {
        let mut r = Ring::new(3);
        assert!(r.is_empty());
        assert_eq!(r.capacity(), 3);
        r.push(1);
        r.push(2);
        assert_eq!(r.len(), 2);
        assert!(!r.is_empty());
        r.push(3);
        r.push(4); // evicts 1
        assert_eq!(r.len(), 3);
        assert_eq!(r.iter().copied().collect::<Vec<_>>(), [2, 3, 4]);
        assert_eq!(r.last(), Some(&4));
    }

    #[test]
    fn ring_capacity_is_at_least_one() {
        let mut r = Ring::new(0);
        assert_eq!(r.capacity(), 1);
        r.push(7);
        r.push(8);
        assert_eq!(r.iter().copied().collect::<Vec<_>>(), [8]);
    }

    #[test]
    fn proc_history_tracks_caps_and_prunes() {
        let mut h = ProcHistory::new(2);
        h.record(&[proc(1, None, 1.0, 100), proc(2, None, 2.0, 200)]);
        h.record(&[proc(1, None, 3.0, 300), proc(2, None, 4.0, 400)]);
        h.record(&[proc(1, None, 5.0, 500)]); // pid 2 disappeared
        assert_eq!(h.tracked(), 1);
        assert!(h.series(2).is_none());
        let s = h.series(1).expect("pid 1 tracked");
        // Capacity 2: the first sample was evicted.
        assert_eq!(s.cpu.iter().copied().collect::<Vec<_>>(), [3.0, 5.0]);
        assert_eq!(s.rss.iter().copied().collect::<Vec<_>>(), [300, 500]);
    }

    #[test]
    fn proc_history_new_pid_starts_fresh() {
        let mut h = ProcHistory::new(4);
        h.record(&[proc(1, None, 1.0, 10)]);
        h.record(&[proc(1, None, 2.0, 20), proc(9, None, 9.0, 90)]);
        assert_eq!(h.tracked(), 2);
        assert_eq!(h.series(9).unwrap().cpu.len(), 1);
        assert_eq!(h.series(1).unwrap().cpu.len(), 2);
    }
}
