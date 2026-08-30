//! Job log parsing. `gh run view --log --job <id>` prints one line per log
//! record as `<job>\t<step>\t<timestamp> <text>`; the REST endpoint
//! (`gh api …/jobs/<id>/logs`, used while a job is still running) prints
//! the raw `<timestamp> <text>` form. Both are turned into the flat line
//! buffer the shared `TailPane` renders. `##[group]` markers become section
//! lines in both cases; step headers only exist for the `gh run view` form,
//! since the REST form carries no step column (they appear once the job
//! completes and the buffer is replaced by the `gh run view --log` output).
//!
//! Sections are encoded into the stored string with a leading marker byte so
//! the (plain `fn`) line formatter can recognise them without side tables:
//! `\u{1}S<step name>` for a step header and `\u{1}G<title>` for a group.

use crate::actions::domain::time::clock_of;

const MARK: char = '\u{1}';
const STEP: &str = "\u{1}S";
const GROUP: &str = "\u{1}G";

/// A decoded buffer line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogLine<'a> {
    /// Start of a step (the log's second column changed).
    Step(&'a str),
    /// A `##[group]` marker.
    Group(&'a str),
    /// `##[error]` / `##[warning]` annotations.
    Error(&'a str),
    Warning(&'a str),
    /// Ordinary text with its `HH:MM:SS` clock, if the line carried one.
    Text {
        clock: Option<&'a str>,
        text: &'a str,
    },
}

pub fn encode_step(name: &str) -> String {
    format!("{STEP}{name}")
}

/// Decode a stored buffer line.
pub fn decode(line: &str) -> LogLine<'_> {
    if let Some(name) = line.strip_prefix(STEP) {
        return LogLine::Step(name);
    }
    if let Some(title) = line.strip_prefix(GROUP) {
        return LogLine::Group(title);
    }
    let (clock, text) = match line.split_once(' ') {
        Some((ts, rest)) if clock_of(ts).is_some() => (clock_of(ts), rest),
        _ => (None, line),
    };
    if let Some(msg) = text.strip_prefix("##[error]") {
        return LogLine::Error(msg);
    }
    if let Some(msg) = text.strip_prefix("##[warning]") {
        return LogLine::Warning(msg);
    }
    LogLine::Text { clock, text }
}

/// Split a `gh run view --log` record into `(step, payload)`; records
/// without the tab-separated columns (REST output) have no step.
fn split_record(raw: &str) -> (Option<&str>, &str) {
    let mut cols = raw.splitn(3, '\t');
    let (a, b, c) = (cols.next(), cols.next(), cols.next());
    match (a, b, c) {
        (Some(_job), Some(step), Some(payload)) => (Some(step), payload),
        _ => (None, raw),
    }
}

/// Convert raw log text into buffer lines. A step header is emitted
/// whenever the step column changes; `##[group]` lines become group
/// headers and `##[endgroup]` lines are dropped. A UTF-8 BOM (which the
/// runner writes at the start of every step) and carriage returns are
/// stripped; ANSI escapes are removed.
pub fn parse_job_log(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current_step: Option<String> = None;
    for line in raw.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        let (step, payload) = split_record(line);
        if let Some(step) = step {
            if current_step.as_deref() != Some(step) {
                current_step = Some(step.to_string());
                out.push(encode_step(step));
            }
        }
        let payload = strip_ansi(payload.trim_start_matches('\u{feff}'));
        let payload = payload.replace('\u{feff}', "");
        let (ts, text) = match payload.split_once(' ') {
            Some((ts, rest)) if clock_of(ts).is_some() => (Some(ts), rest),
            _ => (None, payload.as_str()),
        };
        if let Some(title) = text.strip_prefix("##[group]") {
            out.push(format!("{GROUP}{}", title.trim()));
            continue;
        }
        if text.starts_with("##[endgroup]") {
            continue;
        }
        // The marker byte would make a payload line look like a section
        // header; strip just that one (other control characters pass).
        let text: String = text.chars().filter(|c| *c != MARK).collect();
        match ts {
            Some(ts) => out.push(format!("{ts} {text}")),
            None => out.push(text),
        }
    }
    out
}

/// Indices of the step headers named in `failed` (in buffer order).
pub fn failed_step_lines<'a>(
    lines: impl Iterator<Item = &'a str>,
    failed: &[String],
) -> Vec<usize> {
    lines
        .enumerate()
        .filter_map(|(i, l)| match decode(l) {
            LogLine::Step(name) if failed.iter().any(|f| f == name) => Some(i),
            _ => None,
        })
        .collect()
}

/// Index of the header line of step `name`, if present.
pub fn step_line<'a>(mut lines: impl Iterator<Item = &'a str>, name: &str) -> Option<usize> {
    lines.position(|l| matches!(decode(l), LogLine::Step(n) if n == name))
}

/// Lines of `incoming` that extend the current buffer (`n` lines, yielded
/// by `buffered`): when `incoming` starts with everything already buffered,
/// only the new tail is returned (`Ok`); otherwise the whole thing should
/// replace the buffer (`Err`).
pub fn new_tail<'a>(
    n: usize,
    buffered: impl Iterator<Item = &'a str>,
    incoming: Vec<String>,
) -> Result<Vec<String>, Vec<String>> {
    if incoming.len() < n {
        return Err(incoming);
    }
    let same_prefix = buffered.zip(incoming.iter()).all(|(a, b)| a == b);
    if same_prefix {
        Ok(incoming.into_iter().skip(n).collect())
    } else {
        Err(incoming)
    }
}

/// Remove ANSI escape sequences (CSI, OSC and two-byte ESC sequences).
pub fn strip_ansi(s: &str) -> String {
    if !s.contains('\u{1b}') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('[') => {
                for c in chars.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&c) {
                        break;
                    }
                }
            }
            Some(']') => {
                let mut prev = '\0';
                for c in chars.by_ref() {
                    if c == '\u{7}' || (prev == '\u{1b}' && c == '\\') {
                        break;
                    }
                    prev = c;
                }
            }
            Some(_) | None => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const GH_LOG: &str = "test (ubuntu)\tSet up job\t\u{feff}2026-08-28T08:17:27.3105452Z Current runner version: '2.336.0'\n\
test (ubuntu)\tSet up job\t2026-08-28T08:17:27.3126470Z ##[group]Runner Image Provisioner\n\
test (ubuntu)\tSet up job\t2026-08-28T08:17:27.3127232Z Hosted Compute Agent\n\
test (ubuntu)\tSet up job\t2026-08-28T08:17:27.3127500Z ##[endgroup]\n\
test (ubuntu)\tcargo test\t\u{feff}2026-08-28T08:17:46.0000000Z \u{1b}[32mrunning 12 tests\u{1b}[0m\r\n\
test (ubuntu)\tcargo test\t2026-08-28T08:18:11.0000000Z ##[error]Process completed with exit code 101.\n\
\n\
test (ubuntu)\tcargo build\t2026-08-28T08:18:11.5000000Z ##[warning]slow\n";

    #[test]
    fn parses_gh_columns_into_sections_and_text() {
        let lines = parse_job_log(GH_LOG);
        assert_eq!(
            lines,
            [
                "\u{1}SSet up job",
                "2026-08-28T08:17:27.3105452Z Current runner version: '2.336.0'",
                "\u{1}GRunner Image Provisioner",
                "2026-08-28T08:17:27.3127232Z Hosted Compute Agent",
                "\u{1}Scargo test",
                "2026-08-28T08:17:46.0000000Z running 12 tests",
                "2026-08-28T08:18:11.0000000Z ##[error]Process completed with exit code 101.",
                "\u{1}Scargo build",
                "2026-08-28T08:18:11.5000000Z ##[warning]slow",
            ]
        );
        assert_eq!(decode(&lines[0]), LogLine::Step("Set up job"));
        assert_eq!(
            decode(&lines[2]),
            LogLine::Group("Runner Image Provisioner")
        );
        assert_eq!(
            decode(&lines[3]),
            LogLine::Text {
                clock: Some("08:17:27"),
                text: "Hosted Compute Agent"
            }
        );
        assert_eq!(
            decode(&lines[6]),
            LogLine::Error("Process completed with exit code 101.")
        );
        assert_eq!(decode(&lines[8]), LogLine::Warning("slow"));
    }

    #[test]
    fn parses_rest_output_without_columns() {
        let raw = "2026-08-28T08:17:27.3105452Z Current runner version\n\
                   2026-08-28T08:17:27.4000000Z ##[group]Operating System\n\
                   2026-08-28T08:17:27.5000000Z Ubuntu\n\
                   plain line without timestamp\n";
        let lines = parse_job_log(raw);
        assert_eq!(
            lines,
            [
                "2026-08-28T08:17:27.3105452Z Current runner version",
                "\u{1}GOperating System",
                "2026-08-28T08:17:27.5000000Z Ubuntu",
                "plain line without timestamp",
            ]
        );
        assert_eq!(
            decode(&lines[3]),
            LogLine::Text {
                clock: None,
                text: "plain line without timestamp"
            }
        );
    }

    #[test]
    fn finds_failed_and_named_steps() {
        let lines = parse_job_log(GH_LOG);
        let it = || lines.iter().map(String::as_str);
        assert_eq!(
            failed_step_lines(it(), &["cargo test".to_string()]),
            vec![4]
        );
        assert_eq!(
            failed_step_lines(it(), &["cargo build".into(), "Set up job".into()]),
            vec![0, 7]
        );
        assert!(failed_step_lines(it(), &[]).is_empty());
        assert_eq!(step_line(it(), "cargo build"), Some(7));
        assert_eq!(step_line(it(), "missing"), None);
    }

    #[test]
    fn new_tail_extends_or_replaces() {
        let buf = ["a".to_string(), "b".to_string()];
        let it = || buf.iter().map(String::as_str);
        assert_eq!(
            new_tail(buf.len(), it(), vec!["a".into(), "b".into(), "c".into()]),
            Ok(vec!["c".to_string()])
        );
        assert_eq!(
            new_tail(buf.len(), it(), vec!["a".into(), "b".into()]),
            Ok(vec![])
        );
        assert_eq!(
            new_tail(buf.len(), it(), vec!["x".into(), "b".into(), "c".into()]),
            Err(vec!["x".to_string(), "b".into(), "c".into()])
        );
        assert_eq!(
            new_tail(buf.len(), it(), vec!["a".into()]),
            Err(vec!["a".to_string()])
        );
    }

    #[test]
    fn strips_ansi_and_marker_bytes() {
        assert_eq!(strip_ansi("\u{1b}[1;31mred\u{1b}[0m ok"), "red ok");
        assert_eq!(strip_ansi("\u{1b}]0;title\u{7}text"), "text");
        let lines = parse_job_log("j\ts\t2026-08-28T08:17:27.3105452Z a\u{1}b\n");
        assert_eq!(lines[1], "2026-08-28T08:17:27.3105452Z ab");
    }
}
