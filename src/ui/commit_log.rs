use crate::app::{App, FocusedPane, SearchMatch, SearchOrigin};
use std::collections::HashSet;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let border_color = if app.focused_pane == FocusedPane::GitLog {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let block = Block::default()
        .title(" Git Log ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    if app.git_log.commits.is_empty() {
        let msg = Paragraph::new(Line::from(Span::styled(
            "  No commits",
            Style::default().fg(Color::DarkGray),
        )))
        .block(block);
        f.render_widget(msg, area);
        return;
    }

    // Build set of matched commit entry indices
    let (match_set, current_match_idx) = if app.search.origin == SearchOrigin::CommitLog {
        let set: HashSet<usize> = app
            .search
            .matches
            .iter()
            .filter_map(|m| match m {
                SearchMatch::CommitEntry(idx) => Some(*idx),
                _ => None,
            })
            .collect();
        let current = app.search.current_match_idx.and_then(|ci| {
            match app.search.matches.get(ci) {
                Some(SearchMatch::CommitEntry(idx)) => Some(*idx),
                _ => None,
            }
        });
        (set, current)
    } else {
        (HashSet::new(), None)
    };

    let lines: Vec<Line> = app
        .git_log
        .commits
        .iter()
        .enumerate()
        .map(|(idx, commit)| {
            let is_current = current_match_idx == Some(idx);
            let is_match = match_set.contains(&idx);
            let bg = if is_current {
                Some(Color::Rgb(200, 120, 0))
            } else if is_match {
                Some(Color::Rgb(60, 60, 0))
            } else {
                None
            };
            let fg_override = if is_current { Some(Color::Black) } else { None };

            let hash_style = {
                let mut s = Style::default().fg(fg_override.unwrap_or(Color::Yellow));
                if let Some(bg) = bg { s = s.bg(bg); }
                s
            };
            let date_style = {
                let mut s = Style::default().fg(fg_override.unwrap_or(Color::DarkGray));
                if let Some(bg) = bg { s = s.bg(bg); }
                s
            };
            let author_style = {
                let mut s = Style::default().fg(fg_override.unwrap_or(Color::Cyan));
                if let Some(bg) = bg { s = s.bg(bg); }
                s
            };
            let msg_style = {
                let mut s = Style::default();
                if let Some(fg) = fg_override { s = s.fg(fg); }
                if let Some(bg) = bg { s = s.bg(bg); }
                s
            };

            Line::from(vec![
                Span::styled(format!(" {} ", commit.short_hash), hash_style),
                Span::styled(format!("{} ", commit.date), date_style),
                Span::styled(format!("{:<12} ", commit.author), author_style),
                Span::styled(commit.message.clone(), msg_style),
            ])
        })
        .collect();

    let para = Paragraph::new(lines)
        .block(block)
        .scroll((app.git_log.scroll, 0));
    f.render_widget(para, area);
}
