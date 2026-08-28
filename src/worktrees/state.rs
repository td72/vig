//! The Worktrees page: git worktrees and stashes on the left, a preview of
//! the selected item (HEAD commit summary / stash patch) on the right.
//! Read-only: nothing here adds, removes, applies or drops anything.

use crate::core::app::{AppContext, PageState};
use crate::core::config::{Config, LoadedPageConfig};
use crate::core::keymap::{Keymap, ViewAction};
use crate::core::layout::{split_page_frame, PageLayoutConfig};
use crate::core::page::PageAction;
use crate::core::pane::{self, Pane, PaneEvent, PaneSet, PaneShared};
use crate::core::search::SearchState;
use crate::core::ui::status_bar;
use crate::worktrees::domain::stash::list_stashes;
use crate::worktrees::domain::worktree::list_worktrees;
use crate::worktrees::panes::preview::{PreviewAction, PreviewPane};
use crate::worktrees::panes::stashes::{StashesAction, StashesPane};
use crate::worktrees::panes::worktrees::{WorktreesAction, WorktreesPane};
use anyhow::Result;
use crossterm::event::KeyEvent;
use ratatui::{layout::Rect, Frame};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Pane IDs resolved from the KDL config at construction time.
#[derive(Debug, Clone, Copy)]
pub struct WorktreesPaneIds {
    pub worktrees: usize,
    pub stashes: usize,
    pub preview: usize,
}

impl WorktreesPaneIds {
    fn from_config(cfg: &LoadedPageConfig) -> Self {
        Self {
            worktrees: cfg.resolve_id_expect("worktrees"),
            stashes: cfg.resolve_id_expect("stashes"),
            preview: cfg.resolve_id_expect("preview"),
        }
    }
}

pub struct WorktreesPanes {
    pub worktrees: WorktreesPane,
    pub stashes: StashesPane,
    pub preview: PreviewPane,
    pub ids: WorktreesPaneIds,
}

impl PaneSet for WorktreesPanes {
    fn get_mut(&mut self, idx: usize) -> Option<&mut dyn Pane<PaneEvent>> {
        if idx == self.ids.worktrees {
            Some(&mut self.worktrees)
        } else if idx == self.ids.stashes {
            Some(&mut self.stashes)
        } else if idx == self.ids.preview {
            Some(&mut self.preview)
        } else {
            None
        }
    }
}

pub struct WorktreesState {
    pub pane: PaneShared,
    pub panes: WorktreesPanes,
    /// Working directory vig runs in; git commands run here.
    pub root: PathBuf,
    /// Error from the last listing, shown in the status bar.
    pub error: Option<String>,
    /// select_id → detail_id from the KDL `bind` declarations.
    select_bindings: HashMap<usize, usize>,
    /// Identity of what the preview shows, to skip redundant git calls.
    preview_key: Option<String>,
    /// The list the preview content came from.
    preview_from: usize,
    layout_config: PageLayoutConfig,
    view_keymap: Keymap<ViewAction>,
}

impl pane::PageLayout for WorktreesState {
    type Panes = WorktreesPanes;
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

impl WorktreesState {
    pub fn new(root: &Path, cfg: &Config) -> Result<Self> {
        let page_cfg = cfg.worktrees_page()?;
        let theme = cfg.theme()?;
        let ids = WorktreesPaneIds::from_config(&page_cfg);
        let select_bindings = page_cfg.resolve_select_bindings();

        let worktrees_km = page_cfg.keymap::<WorktreesAction>("worktrees")?;
        let stashes_km = page_cfg.keymap::<StashesAction>("stashes")?;
        let preview_km = page_cfg.keymap::<PreviewAction>("preview")?;
        let view_km = page_cfg.keymap::<ViewAction>("view")?;

        let mut worktrees = WorktreesPane::new(ids.worktrees, ids.preview);
        worktrees.set_keymap(worktrees_km);
        let mut stashes = StashesPane::new(ids.stashes, ids.preview);
        stashes.set_keymap(stashes_km);
        let mut preview = PreviewPane::new(ids.preview, ids.worktrees, &theme);
        preview.set_keymap(preview_km);

        let mut state = Self {
            pane: PaneShared {
                focused_pane: ids.worktrees,
                previous_pane: ids.worktrees,
                search: SearchState::new(),
            },
            panes: WorktreesPanes {
                worktrees,
                stashes,
                preview,
                ids,
            },
            root: root.to_path_buf(),
            error: None,
            select_bindings,
            preview_key: None,
            preview_from: ids.worktrees,
            layout_config: page_cfg.layout,
            view_keymap: view_km,
        };
        state.reload(true);
        Ok(state)
    }

    /// Re-run `git worktree list` / `git stash list` and re-sync the
    /// preview. `force` reloads the preview even for an unchanged item.
    pub fn reload(&mut self, force: bool) {
        let mut error = None;
        match list_worktrees(&self.root) {
            Ok(list) => self.panes.worktrees.set_items(list),
            Err(e) => {
                self.panes.worktrees.set_items(Vec::new());
                error = Some(e.to_string());
            }
        }
        match list_stashes(&self.root) {
            Ok(list) => self.panes.stashes.set_items(list),
            Err(e) => {
                self.panes.stashes.set_items(Vec::new());
                error.get_or_insert(e.to_string());
            }
        }
        self.error = error;
        self.pane.search.reset_matches();
        let src = self.preview_source();
        self.sync_preview(src, force);
    }

    /// The list whose selection the preview should follow.
    fn preview_source(&self) -> usize {
        let focused = self.pane.focused_pane;
        if self.select_bindings.contains_key(&focused) {
            focused
        } else {
            self.preview_from
        }
    }

    /// Load the preview for the selection of list pane `list_id`.
    fn sync_preview(&mut self, list_id: usize, force: bool) {
        let ids = self.panes.ids;
        let key = if list_id == ids.worktrees {
            self.panes.worktrees.selected().map(|wt| {
                format!(
                    "wt:{}@{}",
                    wt.path.display(),
                    wt.head.as_deref().unwrap_or("")
                )
            })
        } else if list_id == ids.stashes {
            self.panes
                .stashes
                .selected()
                .map(|s| format!("stash:{}", s.hash))
        } else {
            return;
        };
        self.preview_from = list_id;
        if !force && key.is_some() && key == self.preview_key {
            return;
        }
        if key.is_none() {
            self.panes.preview.clear();
        } else if list_id == ids.worktrees {
            if let Some(wt) = self.panes.worktrees.selected().cloned() {
                self.panes.preview.show_worktree(&wt, ids.worktrees);
            }
        } else if let Some(stash) = self.panes.stashes.selected().cloned() {
            self.panes
                .preview
                .show_stash(&self.root, &stash, ids.stashes);
        }
        self.preview_key = key;
        // A search started in the preview follows the new content.
        if self.pane.search.origin == ids.preview && self.pane.search.query.is_some() {
            self.panes.preview.invalidate_search_cache();
            self.pane.execute_search(&mut self.panes);
        }
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
                PaneEvent::SetFocus(id) if self.select_bindings.contains_key(&id) => {
                    self.sync_preview(id, false);
                }
                PaneEvent::SelectionChanged => {
                    let focused = self.pane.focused_pane;
                    if self.select_bindings.contains_key(&focused) {
                        self.sync_preview(focused, false);
                    }
                }
                PaneEvent::JumpToMatch(forward) => {
                    if let Some(origin) =
                        self.pane
                            .jump_to_search_match(&mut self.panes, ctx, forward)
                    {
                        if self.select_bindings.contains_key(&origin) {
                            self.sync_preview(origin, false);
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(PageAction::None)
    }

    fn handle_key_inner(&mut self, ctx: &mut AppContext, key: KeyEvent) -> Result<PageAction> {
        // Normal / Visual mode in the stash diff owns every key.
        let pv = self.panes.ids.preview;
        if self.pane.focused_pane == pv && self.panes.preview.intercepts_keys() {
            let events = pane::dispatch_page_key(self, key);
            return self.process_events(ctx, events);
        }
        if self.pane.handle_search_input(&mut self.panes, ctx, key) {
            // Incremental search moves the list selection without emitting
            // an event, so keep the preview in sync here.
            let origin = self.pane.search.origin;
            if self.select_bindings.contains_key(&origin) {
                self.sync_preview(origin, false);
            }
            return Ok(PageAction::None);
        }
        if let Some(action) = self.view_keymap.lookup(key) {
            if let Some(page_action) = pane::execute_common_view_action(ctx, *action) {
                return Ok(page_action);
            }
            if let ViewAction::Refresh = action {
                self.reload(true);
                ctx.status_message = Some("Refreshed".to_string());
                return Ok(PageAction::None);
            }
        }
        let events = pane::dispatch_page_key(self, key);
        self.process_events(ctx, events)
    }
}

impl PageState for WorktreesState {
    fn id(&self) -> &'static str {
        "worktrees"
    }

    fn label(&self) -> &'static str {
        "Worktrees"
    }

    fn help_bindings(&self) -> Vec<(String, String)> {
        use crate::core::keymap::help_section;
        let s = |k: &str, v: &str| (k.to_string(), v.to_string());
        let mut entries = vec![s("1 … 7", "Switch view")];
        entries.extend(self.view_keymap.help_entries());
        entries.extend(help_section("Worktrees"));
        entries.extend(self.panes.worktrees.keymap().help_entries());
        entries.extend(help_section("Stashes"));
        entries.extend(self.panes.stashes.keymap().help_entries());
        entries.extend(help_section("Preview"));
        entries.extend(self.panes.preview.keymap().help_entries());
        entries.push(s("v / V", "Visual / Visual Line (stash diff)"));
        entries.push(s("y", "Yank (copy) selection"));
        entries
    }

    fn handle_key(&mut self, ctx: &mut AppContext, key: KeyEvent) -> Result<PageAction> {
        self.handle_key_inner(ctx, key)
    }

    fn render(&mut self, f: &mut Frame, ctx: &AppContext, area: Rect) {
        let frame = split_page_frame(area);
        status_bar::render_worktrees_header(f, ctx, self, frame.header);
        pane::render_page_content(self, f, ctx, frame.content);
        status_bar::render_worktrees_status_bar(f, ctx, self, frame.status_bar);
    }

    fn intercepts_all_keys(&self) -> bool {
        self.pane.search.active
    }

    fn on_fs_change(&mut self, _ctx: &mut AppContext) -> Result<()> {
        self.reload(false);
        Ok(())
    }

    fn on_activate(&mut self, _ctx: &mut AppContext) {
        // `git worktree add/remove` only touches `.git/worktrees`, which the
        // watcher ignores, so catch up when the page is shown.
        self.reload(false);
    }

    fn drain_background(&mut self) {
        self.panes.preview.drain_background();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::Config;
    use crate::core::keymap::KeyInput;
    use crossterm::event::KeyEvent;

    fn key(s: &str) -> KeyEvent {
        let ki: KeyInput = s.parse().unwrap();
        KeyEvent::new(ki.code, ki.modifiers)
    }

    #[test]
    fn default_kdl_wires_panes_bindings_and_keys() {
        let cfg = Config::builtin().worktrees_page().unwrap();
        let ids = WorktreesPaneIds::from_config(&cfg);
        assert_eq!(
            cfg.layout.tab_panes,
            vec![ids.worktrees, ids.stashes, ids.preview]
        );
        let bindings = cfg.resolve_select_bindings();
        assert_eq!(bindings.get(&ids.worktrees), Some(&ids.preview));
        assert_eq!(bindings.get(&ids.stashes), Some(&ids.preview));

        let km = cfg.keymap::<WorktreesAction>("worktrees").unwrap();
        assert!(matches!(
            km.lookup(key("i")),
            Some(WorktreesAction::FocusPreview)
        ));
        assert!(matches!(
            km.lookup(key("/")),
            Some(WorktreesAction::Search(_))
        ));
        let km = cfg.keymap::<StashesAction>("stashes").unwrap();
        assert!(matches!(
            km.lookup(key("Enter")),
            Some(StashesAction::FocusPreview)
        ));
        let km = cfg.keymap::<PreviewAction>("preview").unwrap();
        assert!(matches!(km.lookup(key("]")), Some(PreviewAction::NextFile)));
        assert!(matches!(
            km.lookup(key("i")),
            Some(PreviewAction::EnterNormalMode)
        ));
        assert!(matches!(km.lookup(key("Esc")), Some(PreviewAction::Esc)));
        let km = cfg.keymap::<ViewAction>("view").unwrap();
        assert!(matches!(
            km.lookup(key("Tab")),
            Some(ViewAction::CyclePaneForward)
        ));
        assert!(matches!(km.lookup(key("r")), Some(ViewAction::Refresh)));
    }

    #[test]
    fn page_loads_for_this_repository() {
        let cwd = std::env::current_dir().unwrap();
        let state = WorktreesState::new(&cwd, &Config::builtin()).expect("worktrees page");
        assert_eq!(state.id(), "worktrees");
        assert!(state.error.is_none(), "{:?}", state.error);
        // vig's own repository is a worktree, and this test runs in it.
        assert!(state.panes.worktrees.items.iter().any(|w| w.is_current));
        assert!(!state.help_bindings().is_empty());
    }
}
