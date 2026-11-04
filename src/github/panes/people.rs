//! Assignees and pending review requests, as list rows and detail headers
//! show them.

use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

/// The marker in front of a pending reviewer in a list row.
pub(crate) const REVIEWER_ARROW: &str = "→";

/// List-row spans after the title: ` @alice @bob` for the assignees and
/// ` → carol` for the reviewers still to review. Empty when there is
/// nobody.
pub(crate) fn row_spans(assignees: &[&str], reviewers: &[&str]) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for login in assignees {
        spans.push(Span::styled(
            format!(" @{login}"),
            Style::default().fg(Color::DarkGray),
        ));
    }
    if !reviewers.is_empty() {
        spans.push(Span::styled(
            format!(" {REVIEWER_ARROW} {}", reviewers.join(", ")),
            Style::default().fg(Color::DarkGray),
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
    fn search_text_names_assignees_with_an_at() {
        assert_eq!(search_text(&[]), "");
        assert_eq!(search_text(&["alice", "bob"]), " @alice @bob");
    }
}
