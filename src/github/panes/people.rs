//! Assignees and pending review requests, as list rows and detail headers
//! show them.

use crate::core::text::truncate_to_width;
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};
use unicode_width::UnicodeWidthStr;

/// A title keeps at least this many columns before the people are cut
/// instead.
const MIN_TITLE_WIDTH: usize = 12;

/// The marker in front of a pending reviewer in a list row.
pub(crate) const REVIEWER_ARROW: &str = "→";

/// The people's color in a row: dimmer than the title, still legible on
/// the selection background (DarkGray is not).
const PEOPLE_FG: Color = Color::Gray;

/// List-row spans after the title: ` @alice @bob` for the assignees and
/// ` → carol` for the reviewers still to review. Empty when there is
/// nobody.
pub(crate) fn row_spans(assignees: &[&str], reviewers: &[&str]) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for login in assignees {
        spans.push(Span::styled(
            format!(" @{login}"),
            Style::default().fg(PEOPLE_FG),
        ));
    }
    if !reviewers.is_empty() {
        spans.push(Span::styled(
            format!(" {REVIEWER_ARROW} {}", reviewers.join(", ")),
            Style::default().fg(PEOPLE_FG),
        ));
    }
    spans
}

/// The detail header's people line — `  Assignees: alice, bob   Reviewers:
/// carol` — or `None` when there is nobody to show.
pub(crate) fn header_line(assignees: &[&str], reviewers: &[&str]) -> Option<Line<'static>> {
    if assignees.is_empty() && reviewers.is_empty() {
        return None;
    }
    let label = Style::default().fg(Color::DarkGray);
    let mut spans = vec![Span::raw(" ")];
    if !assignees.is_empty() {
        spans.push(Span::styled(" Assignees: ", label));
        spans.push(Span::raw(assignees.join(", ")));
    }
    if !reviewers.is_empty() {
        spans.push(Span::styled(" Reviewers: ", label));
        spans.push(Span::raw(reviewers.join(", ")));
    }
    Some(Line::from(spans))
}

/// Columns the `spans` take.
pub(crate) fn spans_width(spans: &[Span<'_>]) -> usize {
    spans.iter().map(|s| s.content.width()).sum()
}

/// The `title` cut so that the people after it still fit a row `width`
/// columns wide whose first `before` columns are taken (prefix, icon,
/// number): a long title loses its tail, not its assignees — down to
/// `MIN_TITLE_WIDTH` columns, below which the people are cut instead.
pub(crate) fn fit_title(title: &str, before: usize, people_width: usize, width: usize) -> String {
    let avail = width.saturating_sub(before);
    if title.width() + people_width <= avail {
        return title.to_string();
    }
    let keep = avail
        .saturating_sub(people_width)
        .max(MIN_TITLE_WIDTH.min(avail));
    truncate_to_width(title, keep)
}

/// `@alice @bob` for a list's search text, so `/` finds items by assignee.
pub(crate) fn search_text(assignees: &[&str]) -> String {
    assignees
        .iter()
        .map(|l| format!(" @{l}"))
        .collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(spans: &[Span<'static>]) -> String {
        spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn row_spans_list_assignees_then_pending_reviewers() {
        assert!(row_spans(&[], &[]).is_empty());
        assert_eq!(text(&row_spans(&["alice", "bob"], &[])), " @alice @bob");
        assert_eq!(
            text(&row_spans(&["alice"], &["carol", "team-x"])),
            " @alice → carol, team-x"
        );
    }

    #[test]
    fn header_line_only_when_someone_is_named() {
        assert!(header_line(&[], &[]).is_none());
        let line = header_line(&["alice"], &["carol"]).unwrap();
        assert_eq!(line.to_string(), "  Assignees: alice Reviewers: carol");
        let line = header_line(&[], &["carol"]).unwrap();
        assert_eq!(line.to_string(), "  Reviewers: carol");
    }

    #[test]
    fn fit_title_keeps_the_people_visible() {
        let people = spans_width(&row_spans(&["alice"], &[]));
        assert_eq!(people, 7);
        // Fits: untouched.
        assert_eq!(fit_title("short", 6, people, 40), "short");
        // Too long: the title gives way to the people (30 - 6 - 7 = 17).
        let long = "a title that runs past the column";
        assert_eq!(fit_title(long, 6, people, 30), "a title that run…");
        // Too narrow for both: the title keeps its minimum, people are cut.
        assert_eq!(fit_title(long, 6, people, 20), "a title tha…");
        assert_eq!(fit_title(long, 6, people, 10), "a t…");
    }

    #[test]
    fn search_text_names_assignees_with_an_at() {
        assert_eq!(search_text(&[]), "");
        assert_eq!(search_text(&["alice", "bob"]), " @alice @bob");
    }
}
