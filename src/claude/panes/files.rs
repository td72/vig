//! Files pane: the files the session read, edited or wrote, the latest
//! touch first, with the strongest kind of touch.

use super::{ago, dim, now_ms};
use crate::claude::domain::types::{Touch, TouchedFile};
use crate::core::app::AppContext;
use crate::core::keymap::{ActionHelp, Keymap, NavAction, SearchAction};
use crate::core::pane::{self, Pane, PaneEvent, PaneShared};
use crate::core::search::SearchMatch;
use crate::core::theme;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::ListItem,
    Frame,
};

#[derive(Debug, Clone)]
pub enum FilesAction {
    Nav(NavAction),
    CopyPath,
    Search(SearchAction),
    Esc,
}

crate::impl_pane_action_from_str!(FilesAction, nav: Nav, search: Search, esc: Esc, CopyPath);

impl ActionHelp for FilesAction {
    fn label(&self) -> Option<&'static str> {
        match self {
            FilesAction::Nav(nav) => nav.label(),
            FilesAction::CopyPath => Some("Copy the path"),
            FilesAction::Search(sa) => sa.label(),
            FilesAction::Esc => Some("Clear search"),
        }
    }
}

pub struct FilesPane {
    pub items: Vec<TouchedFile>,
    pub selected_idx: usize,
    keymap: Keymap<FilesAction>,
    pane_id: usize,
    /// First visible row, kept across frames (see `theme::render_search_list`).
    list_scroll: usize,
    view_height: u16,
}

impl FilesPane {
    pub fn new(pane_id: usize) -> Self {
        Self {
            items: Vec::new(),
            selected_idx: 0,
            keymap: Keymap::new(),
            pane_id,
            list_scroll: 0,
            view_height: 20,
        }
    }

    pub fn set_keymap(&mut self, km: Keymap<FilesAction>) {
        self.keymap = km;
    }

    pub fn keymap(&self) -> &Keymap<FilesAction> {
        &self.keymap
    }

    /// Replace the list, keeping the selection on the same file.
    pub fn set_files(&mut self, files: Vec<TouchedFile>) {
        let keep = self.items.get(self.selected_idx).map(|f| f.path.clone());
        self.items = files;
        self.selected_idx = keep
            .and_then(|path| self.items.iter().position(|f| f.path == path))
            .unwrap_or(self.selected_idx)
            .min(self.items.len().saturating_sub(1));
    }

    pub fn reset(&mut self) {
        self.items.clear();
        self.selected_idx = 0;
        self.list_scroll = 0;
    }

    fn execute(&mut self, shared: &PaneShared, action: FilesAction) -> Vec<PaneEvent> {
        if let Some(events) = pane::try_dispatch_search_esc(&action, shared, self.pane_id, vec![]) {
            return events;
        }
        match action {
            FilesAction::Nav(nav) => pane::execute_list_nav(
                nav,
                &mut self.selected_idx,
                self.items.len(),
                Some(self.view_height),
            ),
            FilesAction::CopyPath => self
                .items
                .get(self.selected_idx)
                .map(|f| vec![PaneEvent::CopyToClipboard(f.path.clone())])
                .unwrap_or_default(),
            _ => vec![],
        }
    }

    fn render_row(file: &TouchedFile, now: i64, selected: bool) -> ListItem<'static> {
        let dim = dim(selected);
        let color = match file.touch {
            Touch::Read => dim.fg.unwrap_or(Color::DarkGray),
            Touch::Edit => Color::Yellow,
            Touch::Write => Color::Green,
        };
        let mut spans = vec![
            Span::raw(" "),
            Span::styled(file.touch.letter(), Style::default().fg(color)),
            Span::raw(" "),
            Span::raw(file.path.clone()),
        ];
        if file.count > 1 {
            spans.push(Span::styled(format!("  ×{}", file.count), dim));
        }
        spans.push(Span::styled(format!("  {}", ago(file.last_ms, now)), dim));
        ListItem::new(Line::from(spans))
    }
}

impl Pane<PaneEvent> for FilesPane {
    crate::impl_handle_key!(keymap);

    fn render(&mut self, f: &mut Frame, _ctx: &AppContext, shared: &PaneShared, area: Rect) {
        self.view_height = area.height.saturating_sub(2);
        let empty = self.items.is_empty().then_some("No files touched");
        let selected = (shared.focused_pane == self.pane_id).then_some(self.selected_idx);
        let now = now_ms();
        self.list_scroll = theme::render_list_pane(
            f,
            area,
            shared,
            self.pane_id,
            "Files",
            selected,
            empty,
            self.list_scroll,
            |match_set, current_match_idx| {
                self.items
                    .iter()
                    .enumerate()
                    .map(|(idx, file)| {
                        let mut li = Self::render_row(file, now, selected == Some(idx));
                        let hl = theme::search_highlight_for(match_set, current_match_idx, idx);
                        if hl.is_active() {
                            li = li.style(hl.apply(Style::default()));
                        }
                        li
                    })
                    .collect()
            },
        );
    }

    fn collect_search_matches(&self, _shared: &PaneShared, query: &str) -> Vec<SearchMatch> {
        pane::collect_list_search_matches(&self.items, query, |f| f.path.clone())
    }

    crate::impl_list_pane_selection!();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str) -> TouchedFile {
        TouchedFile {
            path: path.into(),
            touch: Touch::Edit,
            count: 1,
            last_ms: None,
        }
    }

    #[test]
    fn set_files_keeps_the_selection_on_its_file() {
        let mut pane = FilesPane::new(0);
        pane.set_files(vec![file("a"), file("b")]);
        pane.selected_idx = 1;
        // A newly touched file goes to the top.
        pane.set_files(vec![file("c"), file("a"), file("b")]);
        assert_eq!(pane.items[pane.selected_idx].path, "b");
        pane.set_files(vec![file("c")]);
        assert_eq!(pane.selected_idx, 0);
    }
}
