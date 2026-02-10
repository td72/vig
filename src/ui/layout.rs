use ratatui::layout::{Constraint, Direction, Layout, Rect};

pub struct AppLayout {
    pub header: Rect,
    pub file_tree: Rect,
    pub diff_pane: Rect,
    pub status_bar: Rect,
}

pub fn compute_layout(area: Rect) -> AppLayout {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // header
            Constraint::Min(3),   // main content
            Constraint::Length(1), // status bar
        ])
        .split(area);

    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(30), // file tree
            Constraint::Min(20),   // diff pane
        ])
        .split(vertical[1]);

    AppLayout {
        header: vertical[0],
        file_tree: horizontal[0],
        diff_pane: horizontal[1],
        status_bar: vertical[2],
    }
}
