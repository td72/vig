//! Images pane: `docker images`, dangling (`<none>`) images dimmed at the end.

use crate::core::app::AppContext;
use crate::core::keymap::{
    nav_bindings, search_bindings, ActionHelp, Keymap, NavAction, SearchAction,
};
use crate::core::pane::{self, Pane, PaneEvent, PaneShared};
use crate::core::search::SearchMatch;
use crate::core::theme;
use crate::docker::domain::types::Image;
use crossterm::event::KeyCode;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::ListItem,
    Frame,
};

#[derive(Debug, Clone)]
pub enum ImagesAction {
    Nav(NavAction),
    OpenDetail,
    Search(SearchAction),
    Esc,
}

crate::impl_pane_action_from_str!(ImagesAction, nav: Nav, search: Search, esc: Esc, OpenDetail);

impl ActionHelp for ImagesAction {
    fn label(&self) -> Option<&'static str> {
        match self {
            ImagesAction::Nav(nav) => nav.label(),
            ImagesAction::OpenDetail => Some("Focus detail"),
            ImagesAction::Search(sa) => sa.label(),
            ImagesAction::Esc => Some("Clear search"),
        }
    }
}

pub fn default_keymap() -> Keymap<ImagesAction> {
    Keymap::new()
        .bindings(nav_bindings(ImagesAction::Nav))
        .bindings(search_bindings(ImagesAction::Search))
        .key(KeyCode::Char('i'), ImagesAction::OpenDetail)
        .key(KeyCode::Enter, ImagesAction::OpenDetail)
        .key(KeyCode::Esc, ImagesAction::Esc)
}

/// Tagged images by name, then dangling ones by id.
pub fn sort_images(mut images: Vec<Image>) -> Vec<Image> {
    images.sort_by(|a, b| {
        a.is_dangling()
            .cmp(&b.is_dangling())
            .then_with(|| a.name().cmp(&b.name()))
            .then_with(|| a.id.cmp(&b.id))
    });
    images
}

pub struct ImagesPane {
    pub items: Vec<Image>,
    pub selected_idx: usize,
    loading: bool,
    keymap: Keymap<ImagesAction>,
    pane_id: usize,
    detail_pane_id: usize,
    view_height: u16,
}

impl ImagesPane {
    pub fn new(pane_id: usize, detail_pane_id: usize) -> Self {
        Self {
            items: Vec::new(),
            selected_idx: 0,
            loading: false,
            keymap: default_keymap(),
            pane_id,
            detail_pane_id,
            view_height: 20,
        }
    }

    pub fn set_keymap(&mut self, km: Keymap<ImagesAction>) {
        self.keymap = km;
    }

    pub fn keymap(&self) -> &Keymap<ImagesAction> {
        &self.keymap
    }

    pub fn set_loading(&mut self, loading: bool) {
        self.loading = loading;
    }

    pub fn is_loading(&self) -> bool {
        self.loading
    }

    pub fn selected(&self) -> Option<&Image> {
        self.items.get(self.selected_idx)
    }

    /// Replace the list, keeping the selection on the same image when possible.
    pub fn set_images(&mut self, images: Vec<Image>) {
        let keep = self.selected().map(|i| (i.id.clone(), i.name()));
        self.items = sort_images(images);
        self.selected_idx = keep
            .and_then(|(id, name)| {
                self.items
                    .iter()
                    .position(|i| i.id == id && i.name() == name)
            })
            .unwrap_or(self.selected_idx)
            .min(self.items.len().saturating_sub(1));
    }

    fn execute(&mut self, shared: &PaneShared, action: ImagesAction) -> Vec<PaneEvent> {
        if let Some(events) = pane::try_dispatch_search_esc(&action, shared, self.pane_id, vec![]) {
            return events;
        }
        match action {
            ImagesAction::Nav(nav) => pane::execute_list_nav(
                nav,
                &mut self.selected_idx,
                self.items.len(),
                Some(self.view_height),
            ),
            ImagesAction::OpenDetail if !self.items.is_empty() => {
                vec![PaneEvent::SetFocus(self.detail_pane_id)]
            }
            _ => vec![],
        }
    }

    fn render_row(img: &Image) -> ListItem<'static> {
        let dim = Style::default().fg(Color::DarkGray);
        if img.is_dangling() {
            return ListItem::new(Line::from(Span::styled(
                format!(
                    " {}  {}  {}  {}",
                    img.name(),
                    img.id,
                    img.size,
                    img.created_since
                ),
                dim,
            )));
        }
        ListItem::new(Line::from(vec![
            Span::raw(" "),
            Span::raw(img.name()),
            Span::raw("  "),
            Span::styled(img.id.clone(), Style::default().fg(Color::Yellow)),
            Span::styled(format!("  {}  {}", img.size, img.created_since), dim),
        ]))
    }
}

impl Pane<PaneEvent> for ImagesPane {
    crate::impl_handle_key!(keymap);

    fn render(&mut self, f: &mut Frame, _ctx: &AppContext, shared: &PaneShared, area: Rect) {
        self.view_height = area.height.saturating_sub(2);
        let empty = if self.loading && self.items.is_empty() {
            Some("Loading...")
        } else if self.items.is_empty() {
            Some("No images")
        } else {
            None
        };
        let is_focused = shared.focused_pane == self.pane_id;
        let show_selection = is_focused
            || (shared.focused_pane == self.detail_pane_id && shared.previous_pane == self.pane_id);
        let selected = show_selection.then_some(self.selected_idx);
        theme::render_list_pane(
            f,
            area,
            shared,
            self.pane_id,
            "Images",
            selected,
            empty,
            |match_set, current_match_idx| {
                self.items
                    .iter()
                    .enumerate()
                    .map(|(idx, img)| {
                        let mut li = Self::render_row(img);
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
        pane::collect_list_search_matches(&self.items, query, |i| format!("{} {}", i.name(), i.id))
    }

    crate::impl_list_pane_selection!();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(repo: &str, tag: &str, id: &str) -> Image {
        Image {
            id: id.into(),
            repository: repo.into(),
            tag: tag.into(),
            size: "1MB".into(),
            created_since: "now".into(),
        }
    }

    #[test]
    fn dangling_images_sort_last() {
        let sorted = sort_images(vec![
            image("<none>", "<none>", "b"),
            image("ubuntu", "24.04", "c"),
            image("<none>", "<none>", "a"),
            image("nginx", "alpine", "d"),
        ]);
        let names: Vec<String> = sorted
            .iter()
            .map(|i| format!("{}:{}", i.name(), i.id))
            .collect();
        assert_eq!(
            names,
            [
                "nginx:alpine:d",
                "ubuntu:24.04:c",
                "<none>:<none>:a",
                "<none>:<none>:b"
            ]
        );
    }

    #[test]
    fn set_images_keeps_selection() {
        let mut pane = ImagesPane::new(0, 1);
        pane.set_images(vec![image("a", "1", "x"), image("b", "1", "y")]);
        pane.selected_idx = 1;
        pane.set_images(vec![
            image("0", "1", "z"),
            image("b", "1", "y"),
            image("a", "1", "x"),
        ]);
        assert_eq!(pane.selected().unwrap().name(), "b:1");
    }
}
