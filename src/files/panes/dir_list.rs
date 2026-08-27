//! The main column of the Files page: entries of the current directory.

use crate::core::app::AppContext;
use crate::core::keymap::{
    nav_bindings, search_bindings, ActionHelp, Keymap, NavAction, SearchAction,
};
use crate::core::pane::{self, Pane, PaneEvent, PaneShared};
use crate::core::search::SearchMatch;
use crate::core::theme;
use crate::files::domain::fs::{list_dir, DirEntry};
use crate::files::panes::entry_line;
use crossterm::event::KeyCode;
use ratatui::style::Style;
use ratatui::{layout::Rect, widgets::ListItem, Frame};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub enum DirListAction {
    Nav(NavAction),
    /// Enter the selected directory, or focus the preview for a file.
    Enter,
    /// Go to the parent directory.
    Parent,
    FocusPreview,
    Search(SearchAction),
    Esc,
}

crate::impl_pane_action_from_str!(
    DirListAction, nav: Nav, search: Search, esc: Esc,
    Enter, Parent, FocusPreview
);

impl ActionHelp for DirListAction {
    fn label(&self) -> Option<&'static str> {
        match self {
            DirListAction::Nav(nav) => nav.label(),
            DirListAction::Enter => Some("Enter directory / Open preview"),
            DirListAction::Parent => Some("Parent directory"),
            DirListAction::FocusPreview => Some("Focus preview"),
            DirListAction::Search(sa) => sa.label(),
            DirListAction::Esc => Some("Clear search"),
        }
    }
}

pub fn default_keymap() -> Keymap<DirListAction> {
    Keymap::new()
        .bindings(nav_bindings(DirListAction::Nav))
        .bindings(search_bindings(DirListAction::Search))
        .key(KeyCode::Char('l'), DirListAction::Enter)
        .key(KeyCode::Right, DirListAction::Enter)
        .key(KeyCode::Enter, DirListAction::Enter)
        .key(KeyCode::Char('h'), DirListAction::Parent)
        .key(KeyCode::Left, DirListAction::Parent)
        .key(KeyCode::Backspace, DirListAction::Parent)
        .key(KeyCode::Char('i'), DirListAction::FocusPreview)
        .key(KeyCode::Esc, DirListAction::Esc)
}

pub struct DirListPane {
    /// Browsing never goes above this directory (the repository root).
    root: PathBuf,
    pub cwd: PathBuf,
    pub entries: Vec<DirEntry>,
    pub selected_idx: usize,
    /// Error from the last directory read, shown instead of the list.
    pub error: Option<String>,
    keymap: Keymap<DirListAction>,
    pane_id: usize,
    preview_pane_id: usize,
    view_height: u16,
}

impl DirListPane {
    pub fn new(pane_id: usize, preview_pane_id: usize, root: &Path) -> Self {
        let mut p = Self {
            root: root.to_path_buf(),
            cwd: root.to_path_buf(),
            entries: Vec::new(),
            selected_idx: 0,
            error: None,
            keymap: default_keymap(),
            pane_id,
            preview_pane_id,
            view_height: 20,
        };
        p.reload();
        p
    }

    pub fn set_keymap(&mut self, km: Keymap<DirListAction>) {
        self.keymap = km;
    }

    pub fn keymap(&self) -> &Keymap<DirListAction> {
        &self.keymap
    }

    pub fn selected(&self) -> Option<&DirEntry> {
        self.entries.get(self.selected_idx)
    }

    /// Re-read the current directory, keeping the selection on the same name
    /// when possible.
    pub fn reload(&mut self) {
        let keep = self.selected().map(|e| e.name.clone());
        match list_dir(&self.cwd) {
            Ok(entries) => {
                self.entries = entries;
                self.error = None;
            }
            Err(e) => {
                self.entries.clear();
                self.error = Some(e.to_string());
            }
        }
        self.select_name(keep.as_deref());
    }

    fn select_name(&mut self, name: Option<&str>) {
        self.selected_idx = name
            .and_then(|n| self.entries.iter().position(|e| e.name == n))
            .unwrap_or(0)
            .min(self.entries.len().saturating_sub(1));
    }

    /// Change into `dir`. Returns `false` if it cannot be listed.
    pub fn enter(&mut self, dir: &Path) -> bool {
        match list_dir(dir) {
            Ok(entries) => {
                self.cwd = dir.to_path_buf();
                self.entries = entries;
                self.error = None;
                self.selected_idx = 0;
                true
            }
            Err(e) => {
                self.error = Some(format!("{}: {e}", dir.display()));
                false
            }
        }
    }

    /// Go to the parent directory (never above the root), selecting the
    /// directory we came from.
    pub fn go_parent(&mut self) -> bool {
        if self.cwd == self.root {
            return false;
        }
        let Some(parent) = self.cwd.parent().map(Path::to_path_buf) else {
            return false;
        };
        let from = self
            .cwd
            .file_name()
            .map(|n| n.to_string_lossy().into_owned());
        if !self.enter(&parent) {
            return false;
        }
        self.select_name(from.as_deref());
        true
    }

    fn execute(&mut self, shared: &PaneShared, action: DirListAction) -> Vec<PaneEvent> {
        if let Some(events) = pane::try_dispatch_search_esc(&action, shared, self.pane_id, vec![]) {
            return events;
        }
        match action {
            DirListAction::Nav(nav) => pane::execute_list_nav(
                nav,
                &mut self.selected_idx,
                self.entries.len(),
                Some(self.view_height),
            ),
            DirListAction::Enter => match self.selected().cloned() {
                Some(e) if e.is_dir => {
                    if self.enter(&e.path) {
                        vec![PaneEvent::DirChanged]
                    } else {
                        vec![]
                    }
                }
                Some(_) => vec![PaneEvent::SetFocus(self.preview_pane_id)],
                None => vec![],
            },
            DirListAction::Parent => {
                if self.go_parent() {
                    vec![PaneEvent::DirChanged]
                } else {
                    vec![]
                }
            }
            DirListAction::FocusPreview if !self.entries.is_empty() => {
                vec![PaneEvent::SetFocus(self.preview_pane_id)]
            }
            _ => vec![],
        }
    }
}

impl Pane<PaneEvent> for DirListPane {
    crate::impl_handle_key!(keymap);

    fn render(&mut self, f: &mut Frame, _ctx: &AppContext, shared: &PaneShared, area: Rect) {
        self.view_height = area.height.saturating_sub(2);
        let title = self
            .cwd
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "/".to_string());
        let empty = match &self.error {
            Some(e) => Some(e.clone()),
            None if self.entries.is_empty() => Some("(empty)".to_string()),
            None => None,
        };
        let is_focused = shared.focused_pane == self.pane_id;
        let show_selection = is_focused
            || (shared.focused_pane == self.preview_pane_id
                && shared.previous_pane == self.pane_id);
        let selected = show_selection.then_some(self.selected_idx);
        let width = area.width.saturating_sub(2) as usize;
        theme::render_list_pane(
            f,
            area,
            shared,
            self.pane_id,
            &title,
            selected,
            empty.as_deref(),
            |match_set, current_match_idx| {
                self.entries
                    .iter()
                    .enumerate()
                    .map(|(idx, e)| {
                        let mut li = ListItem::new(entry_line(e, width));
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
        pane::collect_list_search_matches(&self.entries, query, |e| e.name.clone())
    }

    crate::impl_list_pane_selection!();
}
