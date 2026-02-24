use crate::git::state::{PANE_BRANCH_LIST, PANE_FILE_TREE, PANE_REFLOG};
use ratatui::layout::{Constraint, Direction, Layout, Rect};

pub struct AppLayout {
    pub header: Rect,
    pub file_tree: Rect,
    pub branch_list: Rect,
    pub reflog: Rect,
    pub main_pane: Rect,
    pub status_bar: Rect,
}

impl AppLayout {
    pub fn pane_areas(&self, main_pane_idx: usize) -> [(usize, Rect); 4] {
        [
            (PANE_FILE_TREE, self.file_tree),
            (PANE_BRANCH_LIST, self.branch_list),
            (PANE_REFLOG, self.reflog),
            (main_pane_idx, self.main_pane),
        ]
    }
}

pub fn compute_layout(area: Rect) -> AppLayout {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),      // header
            Constraint::Percentage(40), // top row (files + branches + reflog)
            Constraint::Min(3),         // main pane (diff or log)
            Constraint::Length(1),      // status bar
        ])
        .split(area);

    let top_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(30),     // file tree
            Constraint::Percentage(35), // branch list
            Constraint::Min(20),        // reflog
        ])
        .split(vertical[1]);

    AppLayout {
        header: vertical[0],
        file_tree: top_row[0],
        branch_list: top_row[1],
        reflog: top_row[2],
        main_pane: vertical[2],
        status_bar: vertical[3],
    }
}
