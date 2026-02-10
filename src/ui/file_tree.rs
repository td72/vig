use crate::app::{App, FocusedPane, TreeEntry};
use crate::git::diff::FileStatus;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let border_color = if app.focused_pane == FocusedPane::FileTree {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let block = Block::default()
        .title(" Files ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let entries = app.build_tree_entries();

    if entries.is_empty() {
        let items: Vec<ListItem> = vec![ListItem::new(Line::from(Span::styled(
            "  Working tree clean",
            Style::default().fg(Color::DarkGray),
        )))];
        let list = List::new(items).block(block);
        f.render_widget(list, area);
        return;
    }

    let items: Vec<ListItem> = entries
        .iter()
        .map(|entry| match entry {
            TreeEntry::Dir {
                path,
                depth,
                collapsed,
            } => {
                let indent = " ".repeat(depth * 2);
                let icon = if *collapsed { "▶" } else { "▼" };
                let dir_name = path.rsplit('/').next().unwrap_or(path);
                let line = Line::from(vec![
                    Span::raw(format!(" {indent}  ")),
                    Span::styled(
                        format!("{icon} {dir_name}/"),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]);
                ListItem::new(line)
            }
            TreeEntry::File { file_idx, depth } => {
                let file = &app.diff_state.files[*file_idx];
                let indent = " ".repeat(depth * 2);
                let icon_color = match file.status {
                    FileStatus::Modified => Color::Yellow,
                    FileStatus::Added => Color::Green,
                    FileStatus::Deleted => Color::Red,
                    FileStatus::Renamed => Color::Blue,
                    FileStatus::Untracked => Color::DarkGray,
                };
                // For depth > 0, show only filename; for depth 0, show full path
                let display_name = if *depth > 0 {
                    file.path.rsplit('/').next().unwrap_or(&file.path)
                } else {
                    &file.path
                };
                let line = Line::from(vec![
                    Span::raw(format!(" {indent}")),
                    Span::styled(
                        format!("{} ", file.status.icon()),
                        Style::default()
                            .fg(icon_color)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(display_name),
                ]);
                ListItem::new(line)
            }
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );

    let mut state = ListState::default();
    state.select(Some(app.selected_tree_idx));
    f.render_stateful_widget(list, area, &mut state);
}
