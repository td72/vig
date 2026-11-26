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
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub enum DirListAction {
    Nav(NavAction),
    /// Enter the selected directory, or focus the preview for a file.
    Enter,
    /// Go to the parent directory.
    Parent,
    FocusPreview,
    /// Mark / unmark the selected entry and move to the next one.
    ToggleMark,
    /// Copy the paths of the marked entries (or the selected one), relative
    /// to the repository root / absolute.
    CopyPath,
    CopyAbsPath,
    Search(SearchAction),
    Esc,
}

crate::impl_pane_action_from_str!(
    DirListAction, nav: Nav, search: Search, esc: Esc,
    Enter, Parent, FocusPreview, ToggleMark, CopyPath, CopyAbsPath
);

impl ActionHelp for DirListAction {
    fn label(&self) -> Option<&'static str> {
        match self {
            DirListAction::Nav(nav) => nav.label(),
            DirListAction::Enter => Some("Enter directory / Open preview"),
            DirListAction::Parent => Some("Parent directory"),
            DirListAction::FocusPreview => Some("Focus preview"),
            DirListAction::Search(sa) => sa.label(),
            DirListAction::ToggleMark => Some("Mark / unmark entry"),
            DirListAction::CopyPath => Some("Copy path(s)"),
            DirListAction::CopyAbsPath => Some("Copy absolute path(s)"),
            DirListAction::Esc => Some("Clear search / marks"),
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
        .key(KeyCode::Char(' '), DirListAction::ToggleMark)
        .key(KeyCode::Char('y'), DirListAction::CopyPath)
        .key(KeyCode::Char('Y'), DirListAction::CopyAbsPath)
        .key(KeyCode::Esc, DirListAction::Esc)
}

pub struct DirListPane {
    /// Browsing never goes above this directory (the repository root).
    root: PathBuf,
    pub cwd: PathBuf,
    pub entries: Vec<DirEntry>,
    pub selected_idx: usize,
    /// Marked entries, by path. Marks outlive directory changes, so entries
    /// from several directories can be opened or copied together.
    marks: BTreeSet<PathBuf>,
    /// Error from the last directory read, shown instead of the list.
    pub error: Option<String>,
    keymap: Keymap<DirListAction>,
    pane_id: usize,
    /// First visible row, kept across frames (see `theme::render_search_list`).
    list_scroll: usize,
    preview_pane_id: usize,
    view_height: u16,
    icons: bool,
}

impl DirListPane {
    pub fn new(pane_id: usize, preview_pane_id: usize, root: &Path, icons: bool) -> Self {
        let mut p = Self {
            icons,
            root: root.to_path_buf(),
            cwd: root.to_path_buf(),
            entries: Vec::new(),
            selected_idx: 0,
            marks: BTreeSet::new(),
            error: None,
            keymap: default_keymap(),
            pane_id,
            list_scroll: 0,
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

    pub fn mark_count(&self) -> usize {
        self.marks.len()
    }

    /// What `o` / `O` / `y` act on: the marked entries (in path order), or
    /// the selected entry when nothing is marked.
    pub fn targets(&self) -> Vec<PathBuf> {
        if self.marks.is_empty() {
            self.selected()
                .map(|e| e.path.clone())
                .into_iter()
                .collect()
        } else {
            self.marks.iter().cloned().collect()
        }
    }

    /// Mark or unmark the selected entry, then step to the next one so
    /// repeated presses mark a run. Returns whether the selection moved.
    fn toggle_mark(&mut self) -> bool {
        let Some(path) = self.selected().map(|e| e.path.clone()) else {
            return false;
        };
        if !self.marks.remove(&path) {
            self.marks.insert(path);
        }
        if self.selected_idx + 1 < self.entries.len() {
            self.selected_idx += 1;
            true
        } else {
            false
        }
    }

    /// The targets as clipboard text, one path per line.
    fn paths_text(&self, absolute: bool) -> String {
        self.targets()
            .iter()
            .map(|p| {
                let shown = if absolute {
                    p.as_path()
                } else {
                    p.strip_prefix(&self.root).unwrap_or(p)
                };
                shown.to_string_lossy().into_owned()
            })
            .collect::<Vec<_>>()
            .join("\n")
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
        // Forget marks whose entry is gone (deleted or renamed outside vig).
        self.marks.retain(|p| p.symlink_metadata().is_ok());
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
        // Esc clears the search first, then the marks.
        if matches!(action, DirListAction::Esc) && shared.search.query.is_none() {
            self.marks.clear();
            return vec![];
        }
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
            DirListAction::ToggleMark => {
                if self.toggle_mark() {
                    vec![PaneEvent::SelectionChanged]
                } else {
                    vec![]
                }
            }
            DirListAction::CopyPath | DirListAction::CopyAbsPath => {
                let absolute = matches!(action, DirListAction::CopyAbsPath);
                match self.paths_text(absolute) {
                    text if text.is_empty() => {
                        vec![PaneEvent::StatusMessage("No entry selected".to_string())]
                    }
                    text => vec![PaneEvent::CopyToClipboard(text)],
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
        self.list_scroll = theme::render_list_pane(
            f,
            area,
            shared,
            self.pane_id,
            &title,
            selected,
            empty.as_deref(),
            self.list_scroll,
            |match_set, current_match_idx| {
                self.entries
                    .iter()
                    .enumerate()
                    .map(|(idx, e)| {
                        let mut li = ListItem::new(entry_line(
                            e,
                            width,
                            self.icons,
                            self.marks.contains(&e.path),
                        ));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::search::SearchState;
    use std::fs;

    fn tmpdir() -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "vig-dirlist-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(d.join("sub")).unwrap();
        for name in ["a.txt", "b.txt", "c.txt"] {
            fs::write(d.join(name), "x").unwrap();
        }
        fs::write(d.join("sub/d.txt"), "x").unwrap();
        d
    }

    fn shared() -> PaneShared {
        PaneShared {
            focused_pane: 1,
            previous_pane: 1,
            search: SearchState::new(),
        }
    }

    fn names(paths: &[PathBuf]) -> Vec<String> {
        paths
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn targets_are_the_marks_or_the_selected_entry() {
        let d = tmpdir();
        let sh = shared();
        // Entries: sub/, a.txt, b.txt, c.txt
        let mut p = DirListPane::new(1, 2, &d, false);
        assert_eq!(names(&p.targets()), vec!["sub"]);

        p.selected_idx = 1;
        // Marking steps down, so two presses mark a.txt and b.txt.
        let ev = p.execute(&sh, DirListAction::ToggleMark);
        assert!(matches!(&ev[..], [PaneEvent::SelectionChanged]));
        p.execute(&sh, DirListAction::ToggleMark);
        assert_eq!(p.selected_idx, 3);
        assert_eq!(names(&p.targets()), vec!["a.txt", "b.txt"]);

        // On the last entry the cursor stays; a second press unmarks.
        assert!(p.execute(&sh, DirListAction::ToggleMark).is_empty());
        assert_eq!(p.mark_count(), 3);
        p.execute(&sh, DirListAction::ToggleMark);
        assert_eq!(p.mark_count(), 2);

        // Marks survive a directory change and collect across directories.
        p.enter(&d.join("sub"));
        p.execute(&sh, DirListAction::ToggleMark);
        assert_eq!(names(&p.targets()), vec!["a.txt", "b.txt", "d.txt"]);

        // Esc clears them; the selected entry is the target again.
        p.execute(&sh, DirListAction::Esc);
        assert_eq!(names(&p.targets()), vec!["d.txt"]);
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn esc_clears_the_search_before_the_marks() {
        let d = tmpdir();
        let mut sh = shared();
        let mut p = DirListPane::new(1, 2, &d, false);
        p.execute(&sh, DirListAction::ToggleMark);
        sh.search.query = Some("a".to_string());
        let ev = p.execute(&sh, DirListAction::Esc);
        assert!(matches!(&ev[..], [PaneEvent::ClearSearch]));
        assert_eq!(p.mark_count(), 1);
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn copy_path_is_relative_to_the_root_and_one_per_line() {
        let d = tmpdir();
        let sh = shared();
        let mut p = DirListPane::new(1, 2, &d, false);
        p.enter(&d.join("sub"));
        let ev = p.execute(&sh, DirListAction::CopyPath);
        let rel = Path::new("sub")
            .join("d.txt")
            .to_string_lossy()
            .into_owned();
        assert!(matches!(&ev[..], [PaneEvent::CopyToClipboard(t)] if *t == rel));
        let ev = p.execute(&sh, DirListAction::CopyAbsPath);
        let abs = d.join("sub/d.txt").to_string_lossy().into_owned();
        assert!(matches!(&ev[..], [PaneEvent::CopyToClipboard(t)] if *t == abs));

        p.execute(&sh, DirListAction::ToggleMark);
        p.go_parent();
        p.selected_idx = 1;
        p.execute(&sh, DirListAction::ToggleMark);
        let ev = p.execute(&sh, DirListAction::CopyPath);
        assert!(
            matches!(&ev[..], [PaneEvent::CopyToClipboard(t)] if *t == format!("a.txt\n{rel}"))
        );
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn reload_forgets_marks_of_deleted_entries() {
        let d = tmpdir();
        let sh = shared();
        let mut p = DirListPane::new(1, 2, &d, false);
        p.selected_idx = 1;
        p.execute(&sh, DirListAction::ToggleMark);
        p.execute(&sh, DirListAction::ToggleMark);
        fs::remove_file(d.join("a.txt")).unwrap();
        p.reload();
        assert_eq!(names(&p.targets()), vec!["b.txt"]);
        fs::remove_dir_all(&d).unwrap();
    }
}
