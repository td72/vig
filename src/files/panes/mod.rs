pub mod dir_list;
pub mod parent_dir;
pub mod preview;

use crate::files::domain::fs::DirEntry;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// One directory entry as a list line: an optional Nerd Font icon, then the
/// name (directories bold/blue, files plain), with the size right-aligned
/// when there is room.
pub fn entry_line(entry: &DirEntry, width: usize, icons: bool) -> Line<'static> {
    let name = entry.display_name();
    let name_style = if entry.is_dir {
        Style::default()
            .fg(Color::Blue)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let mut spans = vec![Span::raw(" ")];
    let mut used = 1;
    if icons {
        let icon = crate::files::domain::icons::icon_for(entry);
        spans.push(Span::styled(
            format!("{} ", icon.glyph),
            Style::default().fg(icon.color),
        ));
        used += 2;
    }
    spans.push(Span::styled(name.clone(), name_style));
    if !entry.is_dir {
        let size = crate::files::domain::fs::human_size(entry.size);
        let used = used + name.chars().count();
        if width > used + size.len() + 2 {
            spans.push(Span::raw(" ".repeat(width - used - size.len() - 1)));
            spans.push(Span::styled(size, Style::default().fg(Color::DarkGray)));
        }
    }
    Line::from(spans)
}
