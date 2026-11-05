//! Assignees and pending review requests, as list rows and detail headers
//! show them.

use crate::core::text::truncate_to_width;
use ratatui::{
    style::{Color, Modifier, Style},
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

/// The color of everything that points at the viewer: their login among
/// the people, and the state icon of an item that is theirs.
pub(crate) const MINE_FG: Color = Color::Yellow;

/// Whether `login` is the viewer (GitHub logins are case-insensitive).
pub(crate) fn is_me(login: &str, viewer: Option<&str>) -> bool {
    viewer.is_some_and(|me| me.eq_ignore_ascii_case(login))
}

/// Whether the viewer is among `logins`.
pub(crate) fn names_me(logins: &[&str], viewer: Option<&str>) -> bool {
    logins.iter().any(|l| is_me(l, viewer))
}

/// A login's style among the people: the viewer stands out.
fn login_style(login: &str, viewer: Option<&str>, others: Style) -> Style {
    if is_me(login, viewer) {
        Style::default().fg(MINE_FG).add_modifier(Modifier::BOLD)
    } else {
        others
    }
}

/// List-row spans after the title: ` @alice @bob` for the assignees and
/// ` → carol` for the reviewers still to review; the viewer's login is
/// highlighted. Empty when there is nobody.
pub(crate) fn row_spans(
    assignees: &[&str],
    reviewers: &[&str],
    viewer: Option<&str>,
) -> Vec<Span<'static>> {
    let others = Style::default().fg(PEOPLE_FG);
    let mut spans = Vec::new();
    for login in assignees {
        spans.push(Span::styled(
            format!(" @{login}"),
            login_style(login, viewer, others),
        ));
    }
    if !reviewers.is_empty() {
        spans.push(Span::styled(format!(" {REVIEWER_ARROW} "), others));
        spans.extend(joined(reviewers, viewer, others));
    }
    spans
}

/// `logins` separated by `, `, each in its own span so the viewer's can
/// differ.
fn joined(logins: &[&str], viewer: Option<&str>, others: Style) -> Vec<Span<'static>> {
    let mut spans = Vec::with_capacity(logins.len() * 2);
    for (i, login) in logins.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(", ", others));
        }
        spans.push(Span::styled(
            login.to_string(),
            login_style(login, viewer, others),
        ));
    }
    spans
}

/// The detail header's people line — `  Assignees: alice, bob   Reviewers:
/// carol` — or `None` when there is nobody to show.
pub(crate) fn header_line(
    assignees: &[&str],
    reviewers: &[&str],
    viewer: Option<&str>,
) -> Option<Line<'static>> {
    if assignees.is_empty() && reviewers.is_empty() {
        return None;
    }
    let label = Style::default().fg(Color::DarkGray);
    let mut spans = vec![Span::raw(" ")];
    if !assignees.is_empty() {
        spans.push(Span::styled(" Assignees: ", label));
        spans.extend(joined(assignees, viewer, Style::default()));
    }
    if !reviewers.is_empty() {
        spans.push(Span::styled(" Reviewers: ", label));
        spans.extend(joined(reviewers, viewer, Style::default()));
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
        assert!(row_spans(&[], &[], None).is_empty());
        assert_eq!(
            text(&row_spans(&["alice", "bob"], &[], None)),
            " @alice @bob"
        );
        assert_eq!(
            text(&row_spans(&["alice"], &["carol", "team-x"], None)),
            " @alice → carol, team-x"
        );
    }

    /// The viewer's login is the only highlighted span, in a row and in
    /// the header; the match ignores case.
    #[test]
    fn the_viewer_stands_out_among_the_people() {
        let mine = |s: &Span<'static>| s.style.fg == Some(MINE_FG);
        let spans = row_spans(&["alice", "Bob"], &["carol", "bob"], Some("bob"));
        assert_eq!(text(&spans), " @alice @Bob → carol, bob");
        let highlighted: Vec<&str> = spans
            .iter()
            .filter(|s| mine(s))
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(highlighted, vec![" @Bob", "bob"]);
        assert!(row_spans(&["alice"], &[], Some("bob"))
            .iter()
            .all(|s| !mine(s)));

        let line = header_line(&["alice", "bob"], &["carol"], Some("bob")).unwrap();
        assert_eq!(line.to_string(), "  Assignees: alice, bob Reviewers: carol");
        let highlighted: Vec<&str> = line
            .spans
            .iter()
            .filter(|s| mine(s))
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(highlighted, vec!["bob"]);

        assert!(names_me(&["alice", "BOB"], Some("bob")));
        assert!(!names_me(&["alice"], Some("bob")));
        assert!(!names_me(&["bob"], None));
    }

    #[test]
    fn header_line_only_when_someone_is_named() {
        assert!(header_line(&[], &[], None).is_none());
        let line = header_line(&["alice"], &["carol"], None).unwrap();
        assert_eq!(line.to_string(), "  Assignees: alice Reviewers: carol");
        let line = header_line(&[], &["carol"], None).unwrap();
        assert_eq!(line.to_string(), "  Reviewers: carol");
    }

    #[test]
    fn fit_title_keeps_the_people_visible() {
        let people = spans_width(&row_spans(&["alice"], &[], None));
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
