//! Process-wide view of the GitHub GraphQL quota (5,000 points per hour,
//! account-wide). Every custom GraphQL query vig sends piggy-backs
//! `rateLimit { remaining }` (cost 0) and reports it here; the header shows
//! it when low and automatic refreshes slow down or stop on it.

use std::sync::atomic::{AtomicU64, Ordering};

/// GitHub's hourly GraphQL point budget.
pub const HOURLY_LIMIT: u64 = 5000;
/// The header warns below this many points.
pub const WARN_BELOW: u64 = 1500;

const UNKNOWN: u64 = u64::MAX;
static REMAINING: AtomicU64 = AtomicU64::new(UNKNOWN);

/// Record the points left, as reported by the latest GraphQL response.
pub fn note(remaining: u64) {
    REMAINING.store(remaining, Ordering::Relaxed);
}

/// Points left at the last GraphQL response; `None` before the first one.
pub fn remaining() -> Option<u64> {
    let r = REMAINING.load(Ordering::Relaxed);
    (r != UNKNOWN).then_some(r)
}

/// How automatic refreshes should behave for `remaining` points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Throttle {
    Normal,
    /// Under 20 %: intervals double.
    Slow,
    /// Under 5 %: no automatic refresh, manual `r` only.
    Stop,
}

pub fn throttle_for(remaining: Option<u64>) -> Throttle {
    match remaining {
        Some(r) if r * 20 < HOURLY_LIMIT => Throttle::Stop,
        Some(r) if r * 5 < HOURLY_LIMIT => Throttle::Slow,
        _ => Throttle::Normal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thresholds() {
        assert_eq!(throttle_for(None), Throttle::Normal);
        assert_eq!(throttle_for(Some(5000)), Throttle::Normal);
        assert_eq!(throttle_for(Some(1000)), Throttle::Normal); // exactly 20 %
        assert_eq!(throttle_for(Some(999)), Throttle::Slow);
        assert_eq!(throttle_for(Some(250)), Throttle::Slow); // exactly 5 %
        assert_eq!(throttle_for(Some(249)), Throttle::Stop);
        assert_eq!(throttle_for(Some(0)), Throttle::Stop);
    }
}
