//! Tiny UTC time helpers: ISO 8601 parsing, durations and "3m ago" labels.
//! GitHub always reports UTC (`2026-08-28T08:17:23Z`), so no time zones.

use std::time::{SystemTime, UNIX_EPOCH};

/// Seconds since the Unix epoch for `2026-08-28T08:17:23Z` (fractional
/// seconds and the trailing `Z` are ignored). `None` if malformed.
pub fn parse_iso8601(iso: &str) -> Option<i64> {
    let b = iso.as_bytes();
    if b.len() < 19 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' {
        return None;
    }
    let num = |from: usize, to: usize| -> Option<i64> {
        let s = iso.get(from..to)?;
        if !s.bytes().all(|c| c.is_ascii_digit()) {
            return None;
        }
        s.parse().ok()
    };
    let y = num(0, 4)?;
    let mo = num(5, 7)?;
    let d = num(8, 10)?;
    let h = num(11, 13)?;
    let mi = num(14, 16)?;
    let se = num(17, 19)?;
    if !(1..=12).contains(&mo)
        || !(1..=days_in_month(y, mo)).contains(&d)
        || h > 23
        || mi > 59
        || se > 60
    {
        return None;
    }
    Some(days_from_civil(y, mo, d) * 86_400 + h * 3600 + mi * 60 + se)
}

fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
            if leap {
                29
            } else {
                28
            }
        }
    }
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's
/// `days_from_civil`).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Current Unix time in seconds.
pub fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// `45s`, `1m23s`, `1h02m`, `2d03h`.
pub fn format_duration(secs: i64) -> String {
    let secs = secs.max(0);
    let (d, h, m, s) = (
        secs / 86_400,
        secs % 86_400 / 3600,
        secs % 3600 / 60,
        secs % 60,
    );
    if d > 0 {
        format!("{d}d{h:02}h")
    } else if h > 0 {
        format!("{h}h{m:02}m")
    } else if m > 0 {
        format!("{m}m{s:02}s")
    } else {
        format!("{s}s")
    }
}

/// Duration between two timestamps, or from `started` until `now` when
/// `completed` is missing (a running object). Empty when unparsable.
pub fn duration_between(started: Option<&str>, completed: Option<&str>, now: i64) -> String {
    let Some(start) = started.and_then(parse_iso8601) else {
        return String::new();
    };
    let end = match completed {
        Some(c) => match parse_iso8601(c) {
            Some(end) => end,
            None => return String::new(),
        },
        None => now,
    };
    format_duration(end - start)
}

/// `just now`, `5m ago`, `3h ago`, `2d ago`, `3w ago`, `4mo ago`, `1y ago`.
pub fn format_relative(secs_ago: i64) -> String {
    let s = secs_ago.max(0);
    match s {
        0..=59 => "just now".to_string(),
        60..=3_599 => format!("{}m ago", s / 60),
        3_600..=86_399 => format!("{}h ago", s / 3600),
        86_400..=604_799 => format!("{}d ago", s / 86_400),
        604_800..=2_591_999 => format!("{}w ago", s / 604_800),
        2_592_000..=31_535_999 => format!("{}mo ago", s / 2_592_000),
        _ => format!("{}y ago", s / 31_536_000),
    }
}

/// `2026-08-28T08:17:29.4631070Z` → `08:17:29`.
pub fn clock_of(iso: &str) -> Option<&str> {
    let s = iso.get(11..19)?;
    (parse_iso8601(iso).is_some()).then_some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_iso8601_utc() {
        assert_eq!(parse_iso8601("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_iso8601("2000-03-01T00:00:00Z"), Some(951_868_800));
        assert_eq!(
            parse_iso8601("2026-08-28T08:17:29.4631070Z"),
            Some(1_787_905_049)
        );
        assert_eq!(parse_iso8601("2026-08-28"), None);
        assert_eq!(parse_iso8601("2026-13-01T00:00:00Z"), None);
        assert_eq!(parse_iso8601("garbage-in-here-x"), None);
        assert_eq!(parse_iso8601(""), None);
    }

    #[test]
    fn formats_durations() {
        assert_eq!(format_duration(0), "0s");
        assert_eq!(format_duration(45), "45s");
        assert_eq!(format_duration(83), "1m23s");
        assert_eq!(format_duration(3_720), "1h02m");
        assert_eq!(format_duration(183_600), "2d03h");
        assert_eq!(format_duration(-5), "0s");
    }

    #[test]
    fn duration_between_uses_now_for_running_objects() {
        let started = "2026-08-28T08:17:23Z";
        assert_eq!(
            duration_between(Some(started), Some("2026-08-28T08:18:44Z"), 0),
            "1m21s"
        );
        let now = parse_iso8601(started).unwrap() + 30;
        assert_eq!(duration_between(Some(started), None, now), "30s");
        assert_eq!(duration_between(None, None, now), "");
        assert_eq!(duration_between(Some("bad"), None, now), "");
        assert_eq!(duration_between(Some(started), Some("bad"), now), "");
    }

    #[test]
    fn formats_relative_times() {
        assert_eq!(format_relative(5), "just now");
        assert_eq!(format_relative(90), "1m ago");
        assert_eq!(format_relative(7_200), "2h ago");
        assert_eq!(format_relative(3 * 86_400), "3d ago");
        assert_eq!(format_relative(15 * 86_400), "2w ago");
        assert_eq!(format_relative(100 * 86_400), "3mo ago");
        assert_eq!(format_relative(800 * 86_400), "2y ago");
        assert_eq!(format_relative(-10), "just now");
    }

    #[test]
    fn clock_of_extracts_hh_mm_ss() {
        assert_eq!(clock_of("2026-08-28T08:17:29.4631070Z"), Some("08:17:29"));
        assert_eq!(clock_of("not a time"), None);
    }

    #[test]
    fn parse_iso8601_rejects_impossible_dates() {
        assert!(parse_iso8601("2026-02-31T00:00:00Z").is_none());
        assert!(parse_iso8601("2025-02-29T00:00:00Z").is_none());
        assert!(parse_iso8601("2024-02-29T00:00:00Z").is_some());
        assert!(parse_iso8601("2026-04-31T00:00:00Z").is_none());
        assert!(parse_iso8601("2026-04-30T00:00:00Z").is_some());
    }
}
