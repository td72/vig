use ratatui::layout::{Constraint, Direction, Layout, Rect};

pub struct AppLayout {
    pub header: Rect,
    pub file_tree: Rect,
    pub branch_list: Rect,
    pub main_pane: Rect,
    pub status_bar: Rect,
}

pub fn compute_layout(area: Rect) -> AppLayout {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),      // header
            Constraint::Percentage(40), // top row (files + branches)
            Constraint::Min(3),         // main pane (diff or log)
            Constraint::Length(1),      // status bar
        ])
        .split(area);

    let top_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(30), // file tree
            Constraint::Min(20),   // branch list
        ])
        .split(vertical[1]);

    AppLayout {
        header: vertical[0],
        file_tree: top_row[0],
        branch_list: top_row[1],
        main_pane: vertical[2],
        status_bar: vertical[3],
    }
}
