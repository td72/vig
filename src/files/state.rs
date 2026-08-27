//! The Files page: a yazi-like three-column file browser (parent / current /
//! preview) rooted at the repository working directory.

use crate::core::app::{AppContext, PageState};
use crate::core::browser;
use crate::core::config::{Config, LoadedPageConfig};
use crate::core::keymap::{Keymap, ViewAction};
use crate::core::layout::{split_page_frame, PageLayoutConfig};
use crate::core::page::{ExternalCommand, PageAction};
use crate::core::pane::{self, Pane, PaneEvent, PaneSet, PaneShared};
use crate::core::search::SearchState;
use crate::core::tab::Tab;
use crate::core::ui::status_bar;
use crate::files::domain::fs::DirEntry;
use crate::files::panes::dir_list::{DirListAction, DirListPane};
use crate::files::panes::parent_dir::ParentDirPane;
use crate::files::panes::preview::{PreviewAction, PreviewPane};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{layout::Rect, Frame};
use ratatui_image::picker::Picker;
use std::path::{Path, PathBuf};

/// Pane IDs resolved from the KDL config at construction time.
#[derive(Debug, Clone, Copy)]
pub struct FilesPaneIds {
    pub parent_dir: usize,
    pub dir_list: usize,
    pub preview: usize,
}

impl FilesPaneIds {
    fn from_config(cfg: &LoadedPageConfig) -> Self {
        Self {
            parent_dir: cfg.resolve_id_expect("parent_dir"),
            dir_list: cfg.resolve_id_expect("dir_list"),
            preview: cfg.resolve_id_expect("preview"),
        }
    }
}

pub type BrowseTab = Tab<DirListPane, PreviewPane>;

impl BrowseTab {
    /// Load the preview for the selected entry.
    pub fn sync_detail(&mut self) {
        self.detail.load(self.list.selected());
    }
}

pub struct FilesPanes {
    pub parent: ParentDirPane,
    pub tab: BrowseTab,
    pub ids: FilesPaneIds,
}

impl PaneSet for FilesPanes {
    fn get_mut(&mut self, idx: usize) -> Option<&mut dyn Pane<PaneEvent>> {
        if idx == self.ids.parent_dir {
            Some(&mut self.parent)
        } else {
            self.tab
                .get_pane_mut(self.ids.dir_list, self.ids.preview, idx)
        }
    }
}

/// One-line input for the `OpenWith` action (`O`): the application name to
/// open the selected entry with.
#[derive(Debug, Default)]
pub struct OpenWithPrompt {
    pub active: bool,
    pub input: String,
    /// Last confirmed application name; pre-filled the next time.
    last: String,
}

impl OpenWithPrompt {
    fn start(&mut self) {
        self.active = true;
        self.input = self.last.clone();
    }

    /// Handle a key while the prompt is active. Returns `Some(app)` when the
    /// user confirmed a non-empty application name.
    fn handle_key(&mut self, key: KeyEvent) -> Option<String> {
        match key.code {
            KeyCode::Enter => {
                self.active = false;
                let app = self.input.trim().to_string();
                if app.is_empty() {
                    return None;
                }
                self.last = app.clone();
                Some(app)
            }
            KeyCode::Esc => {
                self.active = false;
                None
            }
            KeyCode::Backspace => {
                self.input.pop();
                None
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.clear();
                None
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.push(c);
                None
            }
            _ => None,
        }
    }
}

pub struct FilesState {
    pub pane: PaneShared,
    pub panes: FilesPanes,
    pub open_with: OpenWithPrompt,
    /// Repository working directory; the header shows paths relative to it.
    pub root: PathBuf,
    layout_config: PageLayoutConfig,
    view_keymap: Keymap<ViewAction>,
}

impl pane::PageLayout for FilesState {
    type Panes = FilesPanes;
    fn page_parts_mut(
        &mut self,
    ) -> (
        &mut PaneShared,
        &mut Self::Panes,
        &Keymap<ViewAction>,
        &PageLayoutConfig,
    ) {
        (
            &mut self.pane,
            &mut self.panes,
            &self.view_keymap,
            &self.layout_config,
        )
    }
}

impl FilesState {
    pub fn new(root: &Path, cfg: &Config, picker: Option<Picker>) -> Result<Self> {
        let page_cfg = cfg.files_page()?;
        let theme = cfg.theme()?;
        let icons = cfg.icons()?;
        let ids = FilesPaneIds::from_config(&page_cfg);
        // Validates the bind declarations (dir_list → preview).
        let _ = page_cfg.resolve_select_bindings();

        let dir_list_km = page_cfg.keymap::<DirListAction>("dir_list")?;
        let preview_km = page_cfg.keymap::<PreviewAction>("preview")?;
        let view_km = page_cfg.keymap::<ViewAction>("view")?;

        let mut list = DirListPane::new(ids.dir_list, ids.preview, root, icons);
        list.set_keymap(dir_list_km);
        let mut preview = PreviewPane::new(ids.preview, ids.dir_list, &theme, icons, picker);
        preview.set_keymap(preview_km);
        let parent = ParentDirPane::new(ids.parent_dir, root, icons);

        let mut state = Self {
            pane: PaneShared {
                focused_pane: ids.dir_list,
                previous_pane: ids.dir_list,
                search: SearchState::new(),
            },
            panes: FilesPanes {
                parent,
                tab: Tab {
                    list,
                    detail: preview,
                },
                ids,
            },
            open_with: OpenWithPrompt::default(),
            root: root.to_path_buf(),
            layout_config: page_cfg.layout,
            view_keymap: view_km,
        };
        state.panes.tab.sync_detail();
        Ok(state)
    }

    /// Current directory shown relative to the repository root (`.` at the root).
    pub fn cwd_display(&self) -> String {
        let cwd = &self.panes.tab.list.cwd;
        match cwd.strip_prefix(&self.root) {
            Ok(rel) if rel.as_os_str().is_empty() => ".".to_string(),
            Ok(rel) => rel.to_string_lossy().into_owned(),
            Err(_) => cwd.to_string_lossy().into_owned(),
        }
    }

    pub fn selected(&self) -> Option<&DirEntry> {
        self.panes.tab.list.selected()
    }

    fn on_dir_changed(&mut self) {
        let cwd = self.panes.tab.list.cwd.clone();
        self.panes.parent.update(&cwd);
        self.panes.tab.sync_detail();
    }

    /// Open the selected entry with the OS default application, or with
    /// `app`. Directories open in the system file manager (Finder,
    /// Explorer, ...) because that is what `open` / `explorer` /
    /// `xdg-open` do with a directory path.
    fn open_selected(&self, ctx: &mut AppContext, app: Option<&str>) {
        let Some(entry) = self.selected() else {
            ctx.status_message = Some("No entry selected".to_string());
            return;
        };
        let result = match app {
            Some(app) => browser::open_path_with(app, &entry.path),
            None => browser::open_path(&entry.path),
        };
        ctx.status_message = Some(match result {
            Ok(()) => match app {
                Some(app) => format!("Opening with {app}..."),
                None => "Opening...".to_string(),
            },
            Err(e) => e,
        });
    }

    /// Re-read the current directory (fs change, refresh, editor return).
    fn reload(&mut self) {
        self.panes.tab.list.reload();
        self.on_dir_changed();
    }

    fn process_events(
        &mut self,
        ctx: &mut AppContext,
        events: Vec<PaneEvent>,
    ) -> Result<PageAction> {
        for event in events {
            if pane::process_common_event(&mut self.pane, ctx, &event) {
                continue;
            }
            match event {
                PaneEvent::SelectionChanged => self.panes.tab.sync_detail(),
                PaneEvent::DirChanged => self.on_dir_changed(),
                PaneEvent::JumpToMatch(forward) => {
                    let jumped = self
                        .pane
                        .jump_to_search_match(&mut self.panes, ctx, forward)
                        .is_some();
                    if jumped {
                        self.panes.tab.sync_detail();
                    }
                }
                _ => {}
            }
        }
        Ok(PageAction::None)
    }

    fn handle_view_key(&mut self, ctx: &mut AppContext, key: KeyEvent) -> Result<PageAction> {
        if let Some(action) = self.view_keymap.lookup(key) {
            if let Some(page_action) = pane::execute_common_view_action(ctx, *action) {
                return Ok(page_action);
            }
            match action {
                ViewAction::Refresh => {
                    self.reload();
                    return Ok(PageAction::None);
                }
                ViewAction::OpenEditor => {
                    if let Some(entry) = self.selected().filter(|e| !e.is_dir) {
                        let editor = std::env::var("EDITOR")
                            .or_else(|_| std::env::var("VISUAL"))
                            .unwrap_or_else(|_| "vi".to_string());
                        return Ok(PageAction::Suspend(ExternalCommand {
                            program: editor,
                            args: vec![entry.path.clone().into()],
                        }));
                    }
                    return Ok(PageAction::None);
                }
                ViewAction::OpenDefault => {
                    self.open_selected(ctx, None);
                    return Ok(PageAction::None);
                }
                ViewAction::OpenWith => {
                    if self.selected().is_some() {
                        self.open_with.start();
                    } else {
                        ctx.status_message = Some("No entry selected".to_string());
                    }
                    return Ok(PageAction::None);
                }
                _ => {}
            }
        }
        let events = pane::dispatch_page_key(self, key);
        self.process_events(ctx, events)
    }
}

impl PageState for FilesState {
    fn id(&self) -> &'static str {
        "files"
    }

    fn label(&self) -> &'static str {
        "Files"
    }

    fn help_bindings(&self) -> Vec<(String, String)> {
        use crate::core::keymap::help_section;
        let s = |k: &str, v: &str| (k.to_string(), v.to_string());
        let mut entries = vec![s("1 / 2 / 3", "Switch view")];
        entries.extend(self.view_keymap.help_entries());
        entries.extend(help_section("Files"));
        entries.extend(self.panes.tab.list.keymap().help_entries());
        entries.extend(help_section("Preview"));
        entries.extend(self.panes.tab.detail.keymap().help_entries());
        entries
    }

    fn handle_key(&mut self, ctx: &mut AppContext, key: KeyEvent) -> Result<PageAction> {
        if self.open_with.active {
            if let Some(app) = self.open_with.handle_key(key) {
                self.open_selected(ctx, Some(&app));
            }
            return Ok(PageAction::None);
        }
        if self.pane.handle_search_input(&mut self.panes, ctx, key) {
            // Incremental search moves the list selection without emitting
            // an event, so keep the preview in sync here.
            if self.pane.search.origin == self.panes.ids.dir_list {
                self.panes.tab.sync_detail();
            }
            return Ok(PageAction::None);
        }
        self.handle_view_key(ctx, key)
    }

    fn render(&mut self, f: &mut Frame, ctx: &AppContext, area: Rect) {
        let frame = split_page_frame(area);
        status_bar::render_files_header(f, ctx, self, frame.header);
        pane::render_page_content(self, f, ctx, frame.content);
        status_bar::render_files_status_bar(f, ctx, self, frame.status_bar);
    }

    fn intercepts_all_keys(&self) -> bool {
        self.pane.search.active || self.open_with.active
    }

    fn on_fs_change(&mut self, _ctx: &mut AppContext) -> Result<()> {
        self.reload();
        Ok(())
    }

    fn on_suspend_return(
        &mut self,
        _ctx: &mut AppContext,
        _status: std::io::Result<std::process::ExitStatus>,
    ) -> Result<()> {
        self.reload();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn type_str(p: &mut OpenWithPrompt, s: &str) {
        for c in s.chars() {
            assert_eq!(p.handle_key(key(KeyCode::Char(c))), None);
        }
    }

    #[test]
    fn open_with_prompt_confirms_trimmed_name_and_remembers_it() {
        let mut p = OpenWithPrompt::default();
        p.start();
        assert!(p.active);
        assert_eq!(p.input, "");
        type_str(&mut p, " Preview ");
        assert_eq!(
            p.handle_key(key(KeyCode::Enter)),
            Some("Preview".to_string())
        );
        assert!(!p.active);

        // The last name is pre-filled next time and can be cleared with Ctrl+u.
        p.start();
        assert_eq!(p.input, "Preview");
        assert_eq!(p.handle_key(ctrl('u')), None);
        assert_eq!(p.input, "");
        type_str(&mut p, "Xcode");
        assert_eq!(p.handle_key(key(KeyCode::Backspace)), None);
        assert_eq!(p.input, "Xcod");
    }

    #[test]
    fn open_with_prompt_esc_and_empty_enter_cancel() {
        let mut p = OpenWithPrompt::default();
        p.start();
        type_str(&mut p, "abc");
        assert_eq!(p.handle_key(key(KeyCode::Esc)), None);
        assert!(!p.active);

        p.start();
        assert_eq!(p.handle_key(key(KeyCode::Enter)), None);
        assert!(!p.active);
        assert_eq!(p.last, "");
    }
}
