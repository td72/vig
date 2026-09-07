//! Date math and span resolution for the Roadmap layout. No date crate:
//! days are counted from the civil epoch (1970-01-01) with Howard
//! Hinnant's algorithm, which is all a timeline needs.

use crate::projects::domain::types::{Board, ProjectItem};

/// Days since 1970-01-01 for a civil date.
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// Civil date for days since 1970-01-01.
pub fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// `YYYY-MM-DD` → days since the epoch.
pub fn parse_date(s: &str) -> Option<i64> {
    let s = s.get(..10)?;
    let mut parts = s.split('-');
    let y: i64 = parts.next()?.parse().ok()?;
    let m: i64 = parts.next()?.parse().ok()?;
    let d: i64 = parts.next()?.parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some(days_from_civil(y, m, d))
}

/// Days since the epoch for "now".
pub fn today() -> i64 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    secs.div_euclid(86400)
}

/// Where an item's span comes from: a start / end date field pair, a
/// single date field, or an iteration field — resolved once per board.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpanSpec {
    /// Item key of the start date field.
    pub start: Option<String>,
    /// Item key of the end date field.
    pub end: Option<String>,
    /// Item key of the iteration field (used when the dates say nothing).
    pub iteration: Option<String>,
}

/// Find the fields that give items a time span. Date fields are `.fields`
/// entries whose value on some item looks like `YYYY-MM-DD`; a name
/// containing `start` is the span start, one containing `target`, `end`
/// or `due` the span end. A single date field is both. Iteration values
/// (objects with `startDate` / `duration`) fill in for items without
/// dates.
pub fn span_spec(board: &Board) -> SpanSpec {
    let mut dates: Vec<(String, String)> = Vec::new(); // (name lowercased, key)
    let mut iteration = None;
    for field in &board.fields {
        let key = field.item_key();
        if field.kind == "ProjectV2IterationField" {
            iteration.get_or_insert(key);
            continue;
        }
        let has_date = board.items.iter().any(|i| {
            matches!(i.fields.get(&key), Some(serde_json::Value::String(s)) if parse_date(s).is_some())
        });
        if has_date {
            dates.push((field.name.to_lowercase(), key));
        }
    }
    let find = |words: &[&str]| {
        dates
            .iter()
            .find(|(name, _)| words.iter().any(|w| name.contains(w)))
            .map(|(_, key)| key.clone())
    };
    let mut start = find(&["start", "begin"]);
    let mut end = find(&["target", "end", "due", "finish"]);
    if start.is_none() && end.is_none() {
        // A single (or first) date field is both ends of a point span.
        if let Some((_, key)) = dates.first() {
            start = Some(key.clone());
            end = Some(key.clone());
        }
    }
    SpanSpec {
        start,
        end,
        iteration,
    }
}

/// The item's `(start, end)` in epoch days, if any of the span fields has
/// a value. A single date gives a one-day span; a start / end pair takes
/// whichever ends are present; an iteration covers its start + duration.
pub fn item_span(item: &ProjectItem, spec: &SpanSpec) -> Option<(i64, i64)> {
    let date_of = |key: &Option<String>| {
        key.as_ref()
            .and_then(|k| item.fields.get(k))
            .and_then(|v| v.as_str())
            .and_then(parse_date)
    };
    let (s, e) = (date_of(&spec.start), date_of(&spec.end));
    if s.is_some() || e.is_some() {
        let start = s.or(e).unwrap();
        let end = e.or(s).unwrap();
        return Some((start.min(end), start.max(end)));
    }
    let it = spec
        .iteration
        .as_ref()
        .and_then(|k| item.fields.get(k))?
        .as_object()?;
    let start = it
        .get("startDate")
        .and_then(|v| v.as_str())
        .and_then(parse_date)?;
    let days = it
        .get("duration")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(14);
    Some((start, start + days.max(1) - 1))
}

/// One iteration seen on the board's items: a shaded band on the scale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Iteration {
    pub title: String,
    pub start: i64,
    pub end: i64,
}

/// Distinct iterations across the items, sorted by start.
pub fn iterations(board: &Board, spec: &SpanSpec) -> Vec<Iteration> {
    let Some(key) = &spec.iteration else {
        return Vec::new();
    };
    let mut out: Vec<Iteration> = Vec::new();
    for item in &board.items {
        let Some(it) = item.fields.get(key).and_then(|v| v.as_object()) else {
            continue;
        };
        let Some(start) = it
            .get("startDate")
            .and_then(|v| v.as_str())
            .and_then(parse_date)
        else {
            continue;
        };
        let days = it
            .get("duration")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(14);
        let title = it
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let iter = Iteration {
            title,
            start,
            end: start + days.max(1) - 1,
        };
        if !out.contains(&iter) {
            out.push(iter);
        }
    }
    out.sort_by_key(|i| i.start);
    out
}

/// Zoom level of the time scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zoom {
    /// 1 cell = 3 days.
    Month,
    /// 1 cell = 1 day.
    Week,
    /// 3 cells = 1 day.
    Day,
}

impl Zoom {
    pub fn label(self) -> &'static str {
        match self {
            Self::Month => "month",
            Self::Week => "week",
            Self::Day => "day",
        }
    }

    pub fn zoom_in(self) -> Self {
        match self {
            Self::Month => Self::Week,
            Self::Week | Self::Day => Self::Day,
        }
    }

    pub fn zoom_out(self) -> Self {
        match self {
            Self::Day => Self::Week,
            Self::Week | Self::Month => Self::Month,
        }
    }

    /// Cell column for a day, relative to `origin` (may be negative).
    pub fn x(self, day: i64, origin: i64) -> i64 {
        let d = day - origin;
        match self {
            Self::Month => d.div_euclid(3),
            Self::Week => d,
            Self::Day => d * 3,
        }
    }

    /// The last cell column a day covers (bars end inclusively).
    pub fn x_end(self, day: i64, origin: i64) -> i64 {
        match self {
            Self::Day => self.x(day, origin) + 2,
            _ => self.x(day, origin),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projects::domain::types::tests::board;

    #[test]
    fn civil_round_trip_and_parse() {
        for (y, m, d) in [(1970, 1, 1), (2026, 9, 7), (2000, 2, 29), (1969, 12, 31)] {
            let z = days_from_civil(y, m, d);
            assert_eq!(civil_from_days(z), (y, m, d));
        }
        assert_eq!(parse_date("1970-01-01"), Some(0));
        assert_eq!(parse_date("1970-01-02"), Some(1));
        assert_eq!(parse_date("2026-13-01"), None);
        assert_eq!(parse_date("not a date"), None);
        // Timestamps work too (only the date part is read).
        assert_eq!(parse_date("1970-01-02T10:00:00Z"), Some(1));
    }

    #[test]
    fn span_spec_finds_start_and_iteration_fields() {
        // The fixture's I2 carries `"start date": "2026-08-25"` and a
        // Sprint 3 iteration, so both resolve from the items.
        let spec = span_spec(&board());
        assert_eq!(spec.start.as_deref(), Some("start date"));
        // No end-named date field exists.
        assert_eq!(spec.end, None);
        assert_eq!(spec.iteration.as_deref(), Some("iteration"));

        // A board whose only date field is end-named gives a point span.
        let mut b = board();
        for item in &mut b.items {
            item.fields.remove("start date");
        }
        b.fields.push(crate::projects::domain::types::ProjectField {
            id: "FX".into(),
            name: "Due date".into(),
            kind: "ProjectV2Field".into(),
            options: vec![],
        });
        b.items[0].fields.insert(
            "due date".into(),
            serde_json::Value::String("2026-09-05".into()),
        );
        let spec = span_spec(&b);
        assert_eq!(spec.start, None);
        assert_eq!(spec.end.as_deref(), Some("due date"));
    }

    #[test]
    fn item_spans_from_dates_and_iterations() {
        let spec = SpanSpec {
            start: Some("start date".into()),
            end: Some("target date".into()),
            iteration: Some("iteration".into()),
        };
        let mut item = board().items[0].clone();
        item.fields.insert(
            "start date".into(),
            serde_json::Value::String("2026-09-01".into()),
        );
        item.fields.insert(
            "target date".into(),
            serde_json::Value::String("2026-09-05".into()),
        );
        let (s, e) = item_span(&item, &spec).unwrap();
        assert_eq!(e - s, 4);

        // End only: a one-day span at the end date.
        let mut only_end = board().items[0].clone();
        only_end.fields.insert(
            "target date".into(),
            serde_json::Value::String("2026-09-05".into()),
        );
        let (s, e) = item_span(&only_end, &spec).unwrap();
        assert_eq!(s, e);

        // Iteration fallback: start + duration - 1.
        let mut it = board().items[0].clone();
        it.fields.insert(
            "iteration".into(),
            serde_json::json!({"title": "It 1", "startDate": "2026-09-01", "duration": 14}),
        );
        let (s, e) = item_span(&it, &spec).unwrap();
        assert_eq!(e - s, 13);

        // Nothing set: no span.
        assert_eq!(item_span(&board().items[0], &spec), None);
    }

    #[test]
    fn iterations_are_distinct_and_sorted() {
        let mut b = board();
        let spec = SpanSpec {
            iteration: Some("iteration".into()),
            ..Default::default()
        };
        for (i, start) in [(0usize, "2026-09-15"), (1, "2026-09-01"), (3, "2026-09-01")] {
            b.items[i].fields.insert(
                "iteration".into(),
                serde_json::json!({"title": start, "startDate": start, "duration": 14}),
            );
        }
        let its = iterations(&b, &spec);
        assert_eq!(its.len(), 2);
        assert!(its[0].start < its[1].start);
    }

    #[test]
    fn zoom_scales_and_cycles() {
        assert_eq!(Zoom::Week.x(10, 0), 10);
        assert_eq!(Zoom::Day.x(2, 0), 6);
        assert_eq!(Zoom::Month.x(6, 0), 2);
        assert_eq!(Zoom::Month.x(-3, 0), -1);
        assert_eq!(Zoom::Week.zoom_in(), Zoom::Day);
        assert_eq!(Zoom::Day.zoom_in(), Zoom::Day);
        assert_eq!(Zoom::Week.zoom_out(), Zoom::Month);
        assert_eq!(Zoom::Month.zoom_out(), Zoom::Month);
    }
}
