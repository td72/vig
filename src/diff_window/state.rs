use crate::core::app::{AppContext, PageState};
use crate::core::config::LoadedPageConfig;
use crate::core::keymap::{Keymap, ViewAction};
use crate::core::layout::{split_page_frame, PageLayoutConfig};
use crate::core::page::{ExternalCommand, PageAction};
use crate::core::pane::{self, Pane, PaneEvent, PaneSet, PaneShared};
use crate::core::search::SearchState;
use crate::core::tab::Tab;
use crate::core::ui::status_bar;
use crate::git::domain::diff::{compute_stats, files_from_patch, DiffStats, FileDiff};
use crate::git::panes::diff_view::keys::DiffScrollAction;
use crate::git::panes::file_tree::FileTreeAction;
use crate::git::panes::{DiffViewPane, FileTreePane};
use crate::git::state::FileTab;
use anyhow::Result;
use crossterm::event::KeyEvent;
use ratatui::layout::Rect;
use ratatui::Frame;
use std::rc::Rc;
use std::sync::mpsc;
use std::sync::Arc;

/// A patch fetcher (runs on a worker thread).
pub type PatchFetch = Arc<dyn Fn() -> Result<Vec<u8>, String> + Send + Sync>;
/// Remembers a freshly fetched patch (a disk cache).
pub type PatchStore = Arc<dyn Fn(&[u8]) + Send + Sync>;

/// Where a diff window's patch comes from and how the header names it.
pub struct DiffSource {
    /// The badge after ` vig ` in the header, e.g. `PR #123`.
    pub badge: String,
    /// The refs compared, e.g. `feat/x → main` (when known).
    pub refs: Option<String>,
    /// The item's title, after the refs.
    pub title: String,
    /// What `o` opens and `y` copies.
    pub url: Option<String>,
    /// A patch seen earlier, shown at once while `fetch` runs.
    pub cached: Option<Vec<u8>>,
    pub fetch: PatchFetch,
    pub store: Option<PatchStore>,
}

/// Resolved pane IDs of the window, from the `window "diff"` block.
pub struct DiffWindowIds {
    pub file_tree: usize,
    pub diff_view: usize,
}

impl DiffWindowIds {
    fn from_config(cfg: &LoadedPageConfig) -> Self {
        Self {
            file_tree: cfg.resolve_id_expect("file_tree"),
            diff_view: cfg.resolve_id_expect("diff_view"),
        }
    }
}

pub struct DiffWindowPanes {
    pub file_tab: FileTab,
    pub ids: DiffWindowIds,
}

impl PaneSet for DiffWindowPanes {
    fn get_mut(&mut self, idx: usize) -> Option<&mut dyn Pane<PaneEvent>> {
        let (ft, dv) = (self.ids.file_tree, self.ids.diff_view);
        self.file_tab.get_pane_mut(ft, dv, idx)
    }
}

/// The answer of a patch fetch: the parsed files, or why not.
type FetchResult = Result<Vec<FileDiff>, String>;

pub struct DiffWindowState {
    pub pane: PaneShared,
    pub panes: DiffWindowPanes,
    pub source: DiffSource,
    pub stats: DiffStats,
    pub file_count: usize,
    /// A fetch is outstanding (the cached patch, if any, is shown meanwhile).
    pub loading: bool,
    /// The last fetch failed (the previous patch, if any, stays shown).
    pub error: Option<String>,
    rx: Option<mpsc::Receiver<FetchResult>>,
    layout_config: PageLayoutConfig,
    view_keymap: Keymap<ViewAction>,
}

impl pane::PageLayout for DiffWindowState {
    type Panes = DiffWindowPanes;
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

impl DiffWindowState {
    /// A window over `source`, laid out and keyed by the `window "diff"`
    /// block. The cached patch (if any) is shown right away and a fresh
    /// one is fetched in the background.
    pub fn new(cfg: &LoadedPageConfig, theme: &str, source: DiffSource) -> Result<Self> {
        let ids = DiffWindowIds::from_config(cfg);
        let file_tree_km = cfg.keymap::<FileTreeAction>("file_tree")?;
        let diff_view_km = cfg.keymap::<DiffScrollAction>("diff_view")?;
        let view_km = cfg.keymap::<ViewAction>("view")?;

        let files: Rc<Vec<FileDiff>> = Rc::new(Vec::new());
        let mut file_tree = FileTreePane::new(Rc::clone(&files), ids.file_tree, ids.diff_view);
        file_tree.set_keymap(file_tree_km);
        let mut diff_view = DiffViewPane::new(files, ids.diff_view, theme);
        diff_view.set_scroll_keymap(diff_view_km);

        let focused = ids.file_tree;
        let mut state = Self {
            pane: PaneShared {
                focused_pane: focused,
                previous_pane: focused,
                search: SearchState::new(),
            },
            panes: DiffWindowPanes {
                file_tab: Tab {
                    list: file_tree,
                    detail: diff_view,
                },
                ids,
            },
            source,
            stats: DiffStats {
                additions: 0,
                deletions: 0,
            },
            file_count: 0,
            loading: false,
            error: None,
            rx: None,
            layout_config: cfg.layout.clone(),
            view_keymap: view_km,
        };
        if let Some(files) = state
            .source
            .cached
            .take()
            .and_then(|patch| files_from_patch(&patch).ok())
        {
            state.apply_files(files);
        }
        state.spawn_fetch();
        Ok(state)
    }

    /// Fetch the patch on a worker thread; a fetch still in flight is
    /// abandoned (its channel closes).
    pub fn spawn_fetch(&mut self) {
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        self.loading = true;
        self.error = None;
        let fetch = Arc::clone(&self.source.fetch);
        let store = self.source.store.clone();
        std::thread::spawn(move || {
            let result = fetch().and_then(|patch| {
                let files =
                    files_from_patch(&patch).map_err(|e| format!("Cannot parse the diff: {e}"))?;
                if let Some(store) = store {
                    store(&patch);
                }
                Ok(files)
            });
            let _ = tx.send(result);
        });
    }

    /// Show `files`: the selected file (by path) and the diff scroll survive
    /// a refresh, as on the Git page.
    pub fn apply_files(&mut self, files: Vec<FileDiff>) {
        let old_path = self.selected_file().map(|f| f.path.clone());
        self.stats = compute_stats(&files);
        self.file_count = files.len();
        let files = Rc::new(files);
        self.panes.file_tab.list.set_files(Rc::clone(&files));
        self.panes.file_tab.detail.set_files(files);
        self.panes.file_tab.on_files_changed(old_path);
        self.pane.search.reset_matches();
    }

    pub fn selected_file(&self) -> Option<&FileDiff> {
        self.panes.file_tab.list.selected_file()
    }

    /// The files shown (tests).
    #[cfg(test)]
    pub fn files(&self) -> Rc<Vec<FileDiff>> {
        Rc::clone(&self.panes.file_tab.list.files)
    }

    fn apply_fetch(&mut self, result: FetchResult) {
        self.loading = false;
        match result {
            Ok(files) => self.apply_files(files),
            Err(e) => self.error = Some(e),
        }
    }

    /// Keep the diff view's search in step with the selected file.
    fn sync_detail(&mut self) {
        self.panes.file_tab.sync_detail();
        let dv = self.panes.ids.diff_view;
        if self.pane.search.origin == dv && self.pane.search.query.is_some() {
            self.panes.file_tab.detail.content_lines_cache = None;
            self.pane.execute_search(&mut self.panes);
        }
    }

    /// `Esc` / `q`: a search query is cleared first, the diff view hands
    /// focus back to the tree, and the tree closes the window.
    fn close_or_back(&mut self, force: bool) -> PageAction {
        if self.pane.search.query.is_some() {
            self.pane.search.clear();
            return PageAction::None;
        }
        if !force && self.pane.focused_pane == self.panes.ids.diff_view {
            self.pane.set_focus(self.panes.ids.file_tree);
            return PageAction::None;
        }
        PageAction::Pop
    }

    fn process_events(&mut self, ctx: &mut AppContext, events: Vec<PaneEvent>) {
        for event in events {
            if pane::process_common_event(&mut self.pane, ctx, &event) {
                continue;
            }
            match event {
                PaneEvent::SelectionChanged => self.sync_detail(),
                PaneEvent::JumpToMatch(forward) => {
                    let ft = self.panes.ids.file_tree;
                    if self
                        .pane
                        .jump_to_search_match(&mut self.panes, ctx, forward)
                        == Some(ft)
                    {
                        self.sync_detail();
                    }
                }
                _ => {}
            }
        }
    }

    fn handle_view_key(&mut self, ctx: &mut AppContext, key: KeyEvent) -> Result<PageAction> {
        if let Some(action) = self.view_keymap.lookup(key) {
            if let Some(page_action) = pane::execute_common_view_action(ctx, *action) {
                return Ok(page_action);
            }
            match action {
                ViewAction::Close => {
                    let force = key.code == crossterm::event::KeyCode::Char('q');
                    return Ok(self.close_or_back(force));
                }
                ViewAction::Refresh => {
                    self.spawn_fetch();
                    return Ok(PageAction::None);
                }
                ViewAction::OpenEditor => return Ok(self.open_editor(ctx)),
                ViewAction::OpenBrowser => {
                    match &self.source.url {
                        Some(url) => ctx.open_url(url),
                        None => ctx.status_message = Some("No URL for this diff".into()),
                    }
                    return Ok(PageAction::None);
                }
                ViewAction::CopyUrl => {
                    match self.source.url.clone() {
                        Some(url) => ctx.copy_to_clipboard(&url),
                        None => ctx.status_message = Some("No URL for this diff".into()),
                    }
                    return Ok(PageAction::None);
                }
                _ => {} // Tab navigation is handled by dispatch_key
            }
        }
        let events = pane::dispatch_page_key(self, key);
        self.process_events(ctx, events);
        Ok(PageAction::None)
    }

    /// `e`: the selected file in `$EDITOR`, when the working tree has it
    /// (a PR's file need not exist locally).
    fn open_editor(&self, ctx: &mut AppContext) -> PageAction {
        let Some(file) = self.selected_file() else {
            return PageAction::None;
        };
        let path = ctx.workdir.join(&file.path);
        if !path.is_file() {
            ctx.status_message = Some(format!("{} is not in the working tree", file.path));
            return PageAction::None;
        }
        let editor = std::env::var("EDITOR")
            .or_else(|_| std::env::var("VISUAL"))
            .unwrap_or_else(|_| "vi".to_string());
        PageAction::Suspend(ExternalCommand {
            program: editor,
            args: vec![path.into()],
        })
    }
}

impl PageState for DiffWindowState {
    fn id(&self) -> &'static str {
        "diff"
    }

    fn label(&self) -> &'static str {
        "Diff"
    }

    fn help_bindings(&self) -> Vec<(String, String)> {
        use crate::core::keymap::help_section;

        let s = |k: &str, v: &str| (k.to_string(), v.to_string());
        let mut entries = self.view_keymap.help_entries();
        entries.push(s("v / V", "Visual / Visual Line"));
        entries.push(s("y", "Yank (copy) selection"));
        entries.extend(help_section("File Tree"));
        entries.extend(self.panes.file_tab.list.keymap().help_entries());
        entries.extend(help_section("Diff View (Scroll)"));
        entries.extend(self.panes.file_tab.detail.scroll_keymap.help_entries());
        entries
    }

    fn handle_key(&mut self, ctx: &mut AppContext, key: KeyEvent) -> Result<PageAction> {
        // Normal / Visual mode of the diff view takes every key.
        let dv = self.panes.ids.diff_view;
        if self.pane.focused_pane == dv && self.panes.file_tab.detail.intercepts_keys() {
            let events = pane::dispatch_page_key(self, key);
            self.process_events(ctx, events);
            return Ok(PageAction::None);
        }
        if self.pane.handle_search_input(&mut self.panes, ctx, key) {
            // A confirmed tree search moves the selection without an
            // event: the diff follows here.
            let origin = self.pane.search.origin;
            if !self.pane.search.active && origin == self.panes.ids.file_tree {
                self.sync_detail();
            }
            return Ok(PageAction::None);
        }
        self.handle_view_key(ctx, key)
    }

    fn render(&mut self, f: &mut Frame, ctx: &AppContext, area: Rect) {
        let frame = split_page_frame(area);
        status_bar::render_diff_window_header(f, ctx, self, frame.header);
        pane::render_page_content(self, f, ctx, frame.content);
        status_bar::render_diff_window_status_bar(f, ctx, self, frame.status_bar);
    }

    fn intercepts_all_keys(&self) -> bool {
        self.pane.search.active
    }

    fn on_suspend_return(
        &mut self,
        ctx: &mut AppContext,
        status: std::io::Result<std::process::ExitStatus>,
    ) -> Result<()> {
        match status {
            Ok(s) if s.success() => {}
            Ok(s) => ctx.status_message = Some(format!("Editor exited with: {s}")),
            Err(e) => ctx.status_message = Some(format!("Failed to open editor: {e}")),
        }
        Ok(())
    }

    fn drain_background(&mut self) {
        if let Some(result) = self.rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.rx = None;
            self.apply_fetch(result);
        }
        self.panes.file_tab.detail.drain_background();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::Config;
    use crate::core::keymap::KeyInput;
    use crossterm::event::{KeyCode, KeyModifiers};
    use ratatui::backend::TestBackend;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    const PATCH: &str = "\
diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 fn a() {}
-fn b() {}
+fn b() { todo!() }
 fn c() {}
diff --git a/README.md b/README.md
index 3333333..4444444 100644
--- a/README.md
+++ b/README.md
@@ -1 +1,2 @@
 # title
+more
";

    fn key(s: &str) -> KeyEvent {
        let ki: KeyInput = s.parse().unwrap();
        KeyEvent::new(ki.code, ki.modifiers)
    }

    fn ctx() -> AppContext {
        AppContext {
            should_quit: false,
            active_page: 0,
            page_labels: vec![],
            page_keys: vec![],
            show_help: false,
            status_message: None,
            error_dialog: None,
            workdir: PathBuf::from("/nonexistent"),
            needs_full_redraw: false,
            last_input: Instant::now(),
            auto_refresh: true,
        }
    }

    fn source(fetch: Result<&'static str, &'static str>, cached: Option<&str>) -> DiffSource {
        DiffSource {
            badge: "PR #7".into(),
            refs: Some("feat/x → main".into()),
            title: "Seven".into(),
            url: Some("https://github.com/o/r/pull/7/files".into()),
            cached: cached.map(|c| c.as_bytes().to_vec()),
            fetch: Arc::new(move || fetch.map(|p| p.as_bytes().to_vec()).map_err(String::from)),
            store: None,
        }
    }

    fn window(src: DiffSource) -> DiffWindowState {
        let cfg = Config::builtin();
        DiffWindowState::new(&cfg.diff_window().unwrap(), &cfg.theme().unwrap(), src).unwrap()
    }

    fn settle(win: &mut DiffWindowState) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while win.loading && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
            win.drain_background();
        }
        assert!(!win.loading, "fetch answered");
    }

    fn screen(win: &mut DiffWindowState) -> String {
        let mut term = ratatui::Terminal::new(TestBackend::new(100, 20)).unwrap();
        let ctx = ctx();
        term.draw(|f| win.render(f, &ctx, f.area())).unwrap();
        let buf = term.backend().buffer().clone();
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn fetched_patch_fills_the_tree_and_the_diff() {
        let mut win = window(source(Ok(PATCH), None));
        assert!(win.loading);
        assert_eq!(win.file_count, 0);
        settle(&mut win);
        assert_eq!(win.file_count, 2);
        assert_eq!((win.stats.additions, win.stats.deletions), (2, 1));
        let text = screen(&mut win);
        assert!(text.contains("PR #7"), "{text}");
        assert!(text.contains("feat/x → main"), "{text}");
        assert!(text.contains("lib.rs"), "{text}");
        assert!(text.contains("2 files"), "{text}");
        assert!(text.contains("+2"), "{text}");
    }

    #[test]
    fn cached_patch_shows_at_once_and_the_fetch_replaces_it() {
        let cached = "\
diff --git a/old.txt b/old.txt
index 1111111..2222222 100644
--- a/old.txt
+++ b/old.txt
@@ -1 +1 @@
-x
+y
";
        let mut win = window(source(Ok(PATCH), Some(cached)));
        assert_eq!(
            win.file_count, 1,
            "the cached patch is shown while fetching"
        );
        assert!(win.loading);
        assert!(screen(&mut win).contains("refreshing"));
        settle(&mut win);
        assert_eq!(win.file_count, 2);
        assert!(win.files().iter().any(|f| f.path == "README.md"));
    }

    #[test]
    fn a_failed_fetch_keeps_the_cached_patch_and_reports() {
        let mut win = window(source(Err("gh pr diff failed: too large"), None));
        settle(&mut win);
        assert_eq!(win.error.as_deref(), Some("gh pr diff failed: too large"));
        assert!(screen(&mut win).contains("too large"));
    }

    /// `Esc` goes diff view → tree → close; `q` closes from anywhere; a
    /// search query is cleared before either moves.
    #[test]
    fn esc_and_q_close_the_window_in_steps() {
        let mut win = window(source(Ok(PATCH), None));
        settle(&mut win);
        let mut ctx = ctx();
        let ids = (win.panes.ids.file_tree, win.panes.ids.diff_view);
        assert_eq!(win.pane.focused_pane, ids.0);
        // `i` focuses the diff, Esc returns to the tree, Esc again closes.
        win.handle_key(&mut ctx, key("i")).unwrap();
        assert_eq!(win.pane.focused_pane, ids.1);
        assert!(matches!(
            win.handle_key(&mut ctx, key("Esc")).unwrap(),
            PageAction::None
        ));
        assert_eq!(win.pane.focused_pane, ids.0);
        assert!(matches!(
            win.handle_key(&mut ctx, key("Esc")).unwrap(),
            PageAction::Pop
        ));
        // `q` from the diff view closes outright.
        win.handle_key(&mut ctx, key("i")).unwrap();
        assert!(matches!(
            win.handle_key(&mut ctx, key("q")).unwrap(),
            PageAction::Pop
        ));
        // A search query: Esc clears it first.
        win.handle_key(&mut ctx, key("/")).unwrap();
        for c in "todo".chars() {
            win.handle_key(
                &mut ctx,
                KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE),
            )
            .unwrap();
        }
        win.handle_key(&mut ctx, key("Enter")).unwrap();
        assert!(win.pane.search.query.is_some());
        assert!(matches!(
            win.handle_key(&mut ctx, key("Esc")).unwrap(),
            PageAction::None
        ));
        assert!(win.pane.search.query.is_none());
    }

    /// A confirmed `/` search in the tree moves the selection and the diff
    /// follows it (the confirmation itself raises no event).
    #[test]
    fn tree_search_moves_the_diff() {
        let mut win = window(source(Ok(PATCH), None));
        settle(&mut win);
        let mut ctx = ctx();
        assert_eq!(
            win.selected_file().map(|f| f.path.as_str()),
            Some("src/lib.rs"),
            "directories sort first"
        );
        win.handle_key(&mut ctx, key("/")).unwrap();
        for c in "READ".chars() {
            win.handle_key(
                &mut ctx,
                KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE),
            )
            .unwrap();
        }
        win.handle_key(&mut ctx, key("Enter")).unwrap();
        assert_eq!(
            win.selected_file().map(|f| f.path.as_str()),
            Some("README.md")
        );
        assert_eq!(
            win.panes
                .file_tab
                .detail
                .current_file()
                .map(|f| f.path.as_str()),
            Some("README.md"),
            "the diff view follows the search"
        );
    }

    /// `o` / `y` use the source's URL; `e` refuses a file the working tree
    /// does not have (a PR's file need not exist locally).
    #[test]
    fn url_and_editor_keys() {
        let mut win = window(source(Ok(PATCH), None));
        settle(&mut win);
        let mut ctx = ctx();
        assert!(matches!(
            win.handle_key(&mut ctx, key("e")).unwrap(),
            PageAction::None
        ));
        assert!(ctx
            .status_message
            .as_deref()
            .is_some_and(|m| m.contains("not in the working tree")));
        // Help lists the window's own keys.
        let help = win.help_bindings();
        assert!(
            help.iter()
                .any(|(k, v)| k.contains('q') && v == "Close window"),
            "{help:?}"
        );
        assert!(help.iter().any(|(k, v)| k == "o" && v == "Open in browser"));
    }
}
