//! The Files page: a yazi-like three-column file browser (parent / current /
//! preview) rooted at the repository working directory.

use crate::core::app::{AppContext, PageState};
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
use crossterm::event::KeyEvent;
use ratatui::{layout::Rect, Frame};
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

pub struct FilesState {
    pub pane: PaneShared,
    pub panes: FilesPanes,
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
    pub fn new(root: &Path, cfg: &Config) -> Result<Self> {
        let page_cfg = cfg.files_page()?;
        let theme = cfg.theme()?;
        let ids = FilesPaneIds::from_config(&page_cfg);
        // Validates the bind declarations (dir_list → preview).
        let _ = page_cfg.resolve_select_bindings();

        let dir_list_km = page_cfg.keymap::<DirListAction>("dir_list")?;
        let preview_km = page_cfg.keymap::<PreviewAction>("preview")?;
        let view_km = page_cfg.keymap::<ViewAction>("view")?;

        let mut list = DirListPane::new(ids.dir_list, ids.preview, root);
        list.set_keymap(dir_list_km);
        let mut preview = PreviewPane::new(ids.preview, ids.dir_list, &theme);
        preview.set_keymap(preview_km);
        let parent = ParentDirPane::new(ids.parent_dir, root);

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
        self.pane.search.active
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
