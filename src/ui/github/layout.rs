use ratatui::layout::{Constraint, Direction, Layout, Rect};

pub struct GhLayout {
    pub header: Rect,
    pub issue_list: Rect,
    pub pr_list: Rect,
    pub main_pane: Rect,
    pub status_bar: Rect,
}

pub fn compute_gh_layout(area: Rect) -> GhLayout {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),      // header
            Constraint::Percentage(40), // top row (issue list + pr list)
            Constraint::Min(3),         // main pane (detail view)
            Constraint::Length(1),      // status bar
        ])
        .split(area);

    let top_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50), // issue list
            Constraint::Percentage(50), // pr list
        ])
        .split(vertical[1]);

    GhLayout {
        header: vertical[0],
        issue_list: top_row[0],
        pr_list: top_row[1],
        main_pane: vertical[2],
        status_bar: vertical[3],
    }
}
