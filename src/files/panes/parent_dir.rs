//! Display-only column showing the parent directory with the current
//! directory highlighted (yazi-style).

use crate::core::app::AppContext;
use crate::core::pane::{Pane, PaneEvent, PaneShared};
use crate::core::theme;
use crate::files::domain::fs::{list_dir, DirEntry};
use crate::files::panes::entry_line;
use crossterm::event::KeyEvent;
use ratatui::widgets::{List, ListItem, ListState};
use ratatui::{layout::Rect, Frame};
use std::path::{Path, PathBuf};

pub struct ParentDirPane {
    pane_id: usize,
    icons: bool,
    /// The repository root; nothing above it is shown.
    root: PathBuf,
    parent: Option<PathBuf>,
    entries: Vec<DirEntry>,
    current_idx: Option<usize>,
}

impl ParentDirPane {
    pub fn new(pane_id: usize, root: &Path, icons: bool) -> Self {
        let mut p = Self {
            pane_id,
            icons,
            root: root.to_path_buf(),
            parent: None,
            entries: Vec::new(),
            current_idx: None,
        };
        p.update(root);
        p
    }

    /// Re-read the parent of `cwd` and highlight `cwd` in it. At the root
    /// there is no parent column content.
    pub fn update(&mut self, cwd: &Path) {
        self.parent = if cwd == self.root {
            None
        } else {
            cwd.parent().map(Path::to_path_buf)
        };
        self.entries = self
            .parent
            .as_deref()
            .and_then(|p| list_dir(p).ok())
            .unwrap_or_default();
        self.current_idx = self.entries.iter().position(|e| e.path == cwd);
    }
}

impl Pane<PaneEvent> for ParentDirPane {
    fn handle_key(&mut self, _shared: &PaneShared, _key: KeyEvent) -> Vec<PaneEvent> {
        vec![]
    }

    fn render(&mut self, f: &mut Frame, _ctx: &AppContext, shared: &PaneShared, area: Rect) {
        let title = match &self.parent {
            Some(p) => p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "/".to_string()),
            None => "Parent".to_string(),
        };
        let block = theme::pane_block(&title, shared.focused_pane == self.pane_id);
        if self.parent.is_none() {
            theme::render_empty_list(f, area, block, "(repository root)");
            return;
        }
        let width = area.width.saturating_sub(2) as usize;
        let items: Vec<ListItem> = self
            .entries
            .iter()
            .map(|e| ListItem::new(entry_line(e, width, self.icons)))
            .collect();
        let list = List::new(items)
            .block(block)
            .highlight_style(theme::list_highlight_style(false));
        let mut state = ListState::default();
        state.select(self.current_idx);
        f.render_stateful_widget(list, area, &mut state);
    }
}
