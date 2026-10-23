use crate::core::config::Config;
use crate::core::keymap::{build_app_keymap, AppAction, Keymap};
use crate::core::page::PageAction;
pub use crate::core::search::SearchMatch;
use anyhow::{anyhow, Result};
use crossterm::event::KeyEvent;
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub struct ErrorDialogState {
    pub title: String,
    pub message: String,
}

pub struct AppContext {
    pub should_quit: bool,
    pub active_page: usize,
    pub page_labels: Vec<&'static str>,
    /// Display strings of the app-keymap keys that switch to each page
    /// (same index as `page_labels`); derived by [`App::new`] from the
    /// `app { }` block, so a page may have several keys or none.
    pub page_keys: Vec<Vec<String>>,
    pub show_help: bool,
    pub status_message: Option<String>,
    pub error_dialog: Option<ErrorDialogState>,
    pub workdir: PathBuf,
    /// Set when terminal content outside ratatui's buffer (inline images)
    /// must be wiped: the main loop clears the terminal before the next draw.
    pub needs_full_redraw: bool,
    /// When the last key was pressed; automatic refreshes slow down after
    /// [`IDLE_AFTER`] without one (a forgotten terminal tab).
    pub last_input: Instant,
    /// `github-auto-refresh`: `false` leaves only manual `r` refreshes.
    pub auto_refresh: bool,
}

/// No key press for this long puts automatic refreshes into idle mode.
pub const IDLE_AFTER: Duration = Duration::from_secs(600);
/// Idle mode multiplies every automatic interval by this.
pub const IDLE_SCALE: u32 = 6;

/// The multiplier for automatic refresh intervals, or `None` when they
/// are stopped (auto-refresh off, or under 5 % of the GraphQL quota).
/// Idle ×6, under 20 % of the quota ×2, both compound.
pub fn refresh_scale(auto_refresh: bool, idle: bool, remaining: Option<u64>) -> Option<u32> {
    use crate::core::api_budget::{throttle_for, Throttle};
    if !auto_refresh {
        return None;
    }
    let quota = match throttle_for(remaining) {
        Throttle::Stop => return None,
        Throttle::Slow => 2,
        Throttle::Normal => 1,
    };
    Some(quota * if idle { IDLE_SCALE } else { 1 })
}

impl AppContext {
    /// No key press for [`IDLE_AFTER`].
    pub fn idle(&self) -> bool {
        self.last_input.elapsed() >= IDLE_AFTER
    }

    /// See [`refresh_scale`], with the live idle state and quota.
    pub fn refresh_scale(&self) -> Option<u32> {
        refresh_scale(
            self.auto_refresh,
            self.idle(),
            crate::core::api_budget::remaining(),
        )
    }

    /// `base` scaled for idle / quota, capped at `cap`; `None` when
    /// automatic refreshes are stopped.
    pub fn scaled_interval(&self, base: Duration, cap: Duration) -> Option<Duration> {
        self.refresh_scale().map(|k| (base * k).min(cap))
    }

    /// Header note about the refresh state: the quota when low, `idle`,
    /// or that automatic refreshes are off / stopped.
    pub fn refresh_notice(&self) -> Option<(String, bool)> {
        use crate::core::api_budget::{remaining, throttle_for, Throttle, WARN_BELOW};
        let left = remaining();
        let mut parts: Vec<String> = Vec::new();
        let mut red = false;
        if let Some(r) = left.filter(|r| *r < WARN_BELOW) {
            parts.push(format!("⚠ api {r} left"));
            red = throttle_for(left) == Throttle::Stop;
        }
        if !self.auto_refresh {
            parts.push("auto-refresh off".into());
        } else if throttle_for(left) == Throttle::Stop {
            parts.push("refresh stopped".into());
        } else if self.idle() {
            parts.push("idle".into());
        }
        (!parts.is_empty()).then(|| (parts.join(" · "), red))
    }

    /// Consume the pending full-redraw request.
    pub fn take_full_redraw(&mut self) -> bool {
        std::mem::take(&mut self.needs_full_redraw)
    }

    pub fn show_error(&mut self, title: &str, message: String) {
        self.error_dialog = Some(ErrorDialogState {
            title: title.to_string(),
            message,
        });
    }

    pub fn open_url(&mut self, url: &str) {
        match crate::core::browser::open_url(url) {
            Ok(()) => self.status_message = Some("Opening in browser...".to_string()),
            Err(e) => self.status_message = Some(e),
        }
    }

    pub fn copy_to_clipboard(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let line_count = text.lines().count().max(1);
        match arboard::Clipboard::new() {
            Ok(mut clip) => {
                if clip.set_text(text).is_ok() {
                    self.status_message = Some(if line_count == 1 {
                        let mut chars = text.chars();
                        let shown: String = chars.by_ref().take(60).collect();
                        let ellipsis = if chars.next().is_some() { "…" } else { "" };
                        format!("Copied {shown}{ellipsis}")
                    } else {
                        format!("Yanked {line_count} lines")
                    });
                } else {
                    self.status_message = Some("Clipboard error".to_string());
                }
            }
            Err(_) => {
                self.status_message = Some("Clipboard unavailable".to_string());
            }
        }
    }
}

pub trait PageState {
    /// Canonical, stable page identifier used in config (e.g. `"git"`, `"github"`).
    /// This is what `default.kdl`'s `app` block references via `page:<id>` — keep it
    /// distinct from [`label`](Self::label), which is the human-facing display name.
    fn id(&self) -> &'static str;
    fn label(&self) -> &'static str;
    fn help_bindings(&self) -> Vec<(String, String)> {
        vec![]
    }
    fn handle_key(&mut self, ctx: &mut AppContext, key: KeyEvent) -> Result<PageAction>;
    fn render(&mut self, f: &mut ratatui::Frame, ctx: &AppContext, area: ratatui::layout::Rect);
    fn intercepts_all_keys(&self) -> bool {
        false
    }
    fn on_tick(&mut self, _ctx: &mut AppContext) {}
    fn on_fs_change(&mut self, _ctx: &mut AppContext) -> Result<()> {
        Ok(())
    }
    fn on_suspend_return(
        &mut self,
        _ctx: &mut AppContext,
        _status: std::io::Result<std::process::ExitStatus>,
    ) -> Result<()> {
        Ok(())
    }
    fn on_activate(&mut self, _ctx: &mut AppContext) {}
    fn drain_background(&mut self) {}
}

// --- Public Page wrapper ---

pub struct Page(Box<dyn PageState>);

impl Page {
    pub fn new(state: impl PageState + 'static) -> Self {
        Self(Box::new(state))
    }

    pub fn id(&self) -> &'static str {
        self.0.id()
    }

    pub fn label(&self) -> &'static str {
        self.0.label()
    }

    pub fn help_bindings(&self) -> Vec<(String, String)> {
        self.0.help_bindings()
    }

    fn handle_key(&mut self, ctx: &mut AppContext, key: KeyEvent) -> Result<PageAction> {
        self.0.handle_key(ctx, key)
    }

    fn render(&mut self, f: &mut ratatui::Frame, ctx: &AppContext, area: ratatui::layout::Rect) {
        self.0.render(f, ctx, area);
    }

    fn intercepts_all_keys(&self) -> bool {
        self.0.intercepts_all_keys()
    }

    fn on_tick(&mut self, ctx: &mut AppContext) {
        self.0.on_tick(ctx);
    }

    fn on_fs_change(&mut self, ctx: &mut AppContext) -> Result<()> {
        self.0.on_fs_change(ctx)
    }

    fn on_suspend_return(
        &mut self,
        ctx: &mut AppContext,
        status: std::io::Result<std::process::ExitStatus>,
    ) -> Result<()> {
        self.0.on_suspend_return(ctx, status)
    }

    fn on_activate(&mut self, ctx: &mut AppContext) {
        self.0.on_activate(ctx);
    }

    fn drain_background(&mut self) {
        self.0.drain_background();
    }
}

pub struct App {
    pub ctx: AppContext,
    pages: Vec<Page>,
    /// Pages pushed on top of the tab row (`PageAction::Push`), innermost
    /// last. While any is open it receives the keys, ticks and drawing;
    /// the tab pages underneath keep their state untouched.
    windows: Vec<Page>,
    app_keymap: Keymap<AppAction>,
}

/// Display strings of the keys bound to `SwitchPage(idx)` for every page
/// index below `page_count`, in keymap insertion order.
fn page_keys(app_keymap: &Keymap<AppAction>, page_count: usize) -> Vec<Vec<String>> {
    let mut keys = vec![Vec::new(); page_count];
    for (ki, action) in app_keymap.entries() {
        if let AppAction::SwitchPage(idx) = action {
            if let Some(slot) = keys.get_mut(*idx) {
                slot.push(ki.to_string());
            }
        }
    }
    keys
}

impl App {
    pub fn new(mut ctx: AppContext, pages: Vec<Page>, cfg: &Config) -> Result<Self> {
        let page_names: Vec<&str> = pages.iter().map(|p| p.id()).collect();
        let entries = cfg.app_entries()?;
        let app_keymap = build_app_keymap(&entries, &page_names)
            .map_err(|e| anyhow!("invalid {}: app block: {e}", cfg.describe()))?;
        ctx.page_keys = page_keys(&app_keymap, pages.len());
        Ok(Self {
            ctx,
            pages,
            windows: Vec::new(),
            app_keymap,
        })
    }

    /// The page receiving keys and drawing: the innermost pushed window,
    /// else the active tab page. A free function over the fields so the
    /// context can be borrowed alongside.
    fn active_page_mut<'a>(
        windows: &'a mut [Page],
        pages: &'a mut [Page],
        active_page: usize,
    ) -> &'a mut Page {
        match windows.last_mut() {
            Some(w) => w,
            None => &mut pages[active_page],
        }
    }

    fn active(&self) -> &Page {
        match self.windows.last() {
            Some(w) => w,
            None => &self.pages[self.ctx.active_page],
        }
    }

    /// How many pushed windows are open (tests).
    #[cfg(test)]
    pub fn window_depth(&self) -> usize {
        self.windows.len()
    }

    /// Apply what the active page asked for after a key: a window is
    /// pushed / popped here, everything else goes back to the main loop.
    fn apply_page_action(&mut self, action: PageAction) -> PageAction {
        match action {
            PageAction::Push(mut window) => {
                // The page's last status line is not the window's.
                self.ctx.status_message = None;
                window.on_activate(&mut self.ctx);
                self.windows.push(window);
                PageAction::None
            }
            PageAction::Pop => {
                self.pop_window();
                PageAction::None
            }
            other => other,
        }
    }

    /// Close the innermost window; the page revealed is re-activated.
    fn pop_window(&mut self) {
        if self.windows.pop().is_some() {
            self.ctx.status_message = None;
            let ctx = &mut self.ctx;
            Self::active_page_mut(&mut self.windows, &mut self.pages, ctx.active_page)
                .on_activate(ctx);
        }
    }

    pub fn drain_all_background(&mut self) {
        for page in self.pages.iter_mut().chain(self.windows.iter_mut()) {
            page.drain_background();
        }
    }

    pub fn render(&mut self, f: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let ctx = &self.ctx;
        match self.windows.last_mut() {
            Some(w) => w.render(f, ctx, area),
            None => self.pages[ctx.active_page].render(f, ctx, area),
        }
    }

    pub fn on_fs_change(&mut self) -> Result<()> {
        for page in self.pages.iter_mut().chain(self.windows.iter_mut()) {
            page.on_fs_change(&mut self.ctx)?;
        }
        Ok(())
    }

    pub fn on_tick(&mut self) {
        let ctx = &mut self.ctx;
        match self.windows.last_mut() {
            Some(w) => w.on_tick(ctx),
            None => self.pages[ctx.active_page].on_tick(ctx),
        }
    }

    pub fn on_suspend_return(
        &mut self,
        status: std::io::Result<std::process::ExitStatus>,
    ) -> Result<()> {
        let ctx = &mut self.ctx;
        match self.windows.last_mut() {
            Some(w) => w.on_suspend_return(ctx, status),
            None => self.pages[ctx.active_page].on_suspend_return(ctx, status),
        }
    }

    /// Help entries of the active page, preceded by the page-switch keys of
    /// the app keymap (`1 / 2 / … `, "Switch view") when any page has one.
    pub fn active_help_bindings(&self) -> Vec<(String, String)> {
        let mut entries = Vec::new();
        let switch_keys = self
            .ctx
            .page_keys
            .iter()
            .flatten()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(" / ");
        if !switch_keys.is_empty() {
            entries.push((switch_keys, "Switch view".to_string()));
        }
        entries.extend(self.active().help_bindings());
        entries
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Result<PageAction> {
        self.ctx.last_input = Instant::now();
        if self.ctx.show_help {
            self.ctx.show_help = false;
            return Ok(PageAction::None);
        }

        // Error dialog: any key dismisses
        if self.ctx.error_dialog.is_some() {
            self.ctx.error_dialog = None;
            return Ok(PageAction::None);
        }

        let idx = self.ctx.active_page;

        // If the page intercepts all keys (modal menu, search input), delegate immediately
        if self.active().intercepts_all_keys() {
            let ctx = &mut self.ctx;
            let action = Self::active_page_mut(&mut self.windows, &mut self.pages, ctx.active_page)
                .handle_key(ctx, key)?;
            return Ok(self.apply_page_action(action));
        }

        // App-level keymap handles Quit and page switching. Switching
        // closes every pushed window: the tab row is the destination.
        if let Some(action) = self.app_keymap.lookup(key) {
            match action.clone() {
                AppAction::Quit => {
                    self.ctx.should_quit = true;
                    return Ok(PageAction::None);
                }
                AppAction::SwitchPage(new_idx) => {
                    if new_idx < self.pages.len() {
                        let had_windows = !self.windows.is_empty();
                        self.windows.clear();
                        if new_idx != idx || had_windows {
                            self.ctx.active_page = new_idx;
                            self.ctx.status_message = None;
                            self.pages[new_idx].on_activate(&mut self.ctx);
                        }
                    }
                    return Ok(PageAction::None);
                }
            }
        }

        // Delegate to the active page (or window)
        let ctx = &mut self.ctx;
        let action = Self::active_page_mut(&mut self.windows, &mut self.pages, ctx.active_page)
            .handle_key(ctx, key)?;
        Ok(self.apply_page_action(action))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::keymap::KeyInput;

    /// Regression guard for the page-switch bindings in `default.kdl`'s `app` block.
    ///
    /// The block references pages by their canonical id (`page:git`, `page:github`),
    /// so `App::new` must resolve those against [`PageState::id`] — not the display
    /// `label()` ("Git"/"GitHub"). Using `label()` made every `page:*` binding fail to
    /// resolve, which previously was silently dropped (1/2 page switching did nothing)
    /// and now panics via the embedded-default `expect`. This builds the keymap the
    /// same way `App::new` does, from the real pages, so the two can't drift apart.
    /// Every enabled page, created from `cfg` the way `run_tui` does
    /// (through the shared `pages::build_pages` registry, in slot order).
    fn all_pages(cfg: &Config) -> (Vec<Page>, PathBuf) {
        let cwd = std::env::current_dir().unwrap();
        crate::pages::build_pages(cfg, &cwd, None).expect("pages")
    }

    /// Build the `App` from `cfg` exactly as production does; `App::new`
    /// fails (failing the test) if the page ids ever drift from the
    #[test]
    fn refresh_scale_compounds_idle_and_quota_and_stops() {
        // Normal: ×1; idle: ×6; low quota (<20 %): ×2; both: ×12.
        assert_eq!(refresh_scale(true, false, None), Some(1));
        assert_eq!(refresh_scale(true, false, Some(4000)), Some(1));
        assert_eq!(refresh_scale(true, true, Some(4000)), Some(IDLE_SCALE));
        assert_eq!(refresh_scale(true, false, Some(900)), Some(2));
        assert_eq!(refresh_scale(true, true, Some(900)), Some(2 * IDLE_SCALE));
        // Under 5 % of the quota, or auto-refresh off: stopped.
        assert_eq!(refresh_scale(true, false, Some(200)), None);
        assert_eq!(refresh_scale(true, true, Some(200)), None);
        assert_eq!(refresh_scale(false, false, None), None);
    }

    #[test]
    fn refresh_notice_reports_idle_and_off() {
        let mut ctx = AppContext {
            should_quit: false,
            active_page: 0,
            page_labels: vec![],
            page_keys: vec![],
            show_help: false,
            status_message: None,
            error_dialog: None,
            workdir: PathBuf::new(),
            needs_full_redraw: false,
            last_input: Instant::now(),
            auto_refresh: true,
        };
        // Fresh input, quota unknown: nothing to say.
        assert_eq!(ctx.refresh_notice(), None);
        // Idle: the header says so, in yellow.
        ctx.last_input = Instant::now() - IDLE_AFTER;
        assert_eq!(ctx.refresh_notice(), Some(("idle".to_string(), false)));
        // Auto-refresh off wins over idle.
        ctx.auto_refresh = false;
        assert_eq!(
            ctx.refresh_notice(),
            Some(("auto-refresh off".to_string(), false))
        );
    }

    /// `page:*` references in the config.
    fn app_with(cfg: &Config) -> App {
        let (pages, workdir) = all_pages(cfg);
        let ctx = AppContext {
            should_quit: false,
            active_page: 0,
            page_labels: pages.iter().map(|p| p.label()).collect(),
            page_keys: Vec::new(),
            show_help: false,
            status_message: None,
            error_dialog: None,
            workdir,
            needs_full_redraw: false,
            last_input: Instant::now(),
            auto_refresh: true,
        };
        App::new(ctx, pages, cfg).expect("app keymap")
    }

    fn user_config(kdl: &str) -> Config {
        let doc: kdl::KdlDocument = kdl.parse().unwrap();
        Config::with_user(&doc, PathBuf::from("/u/config.kdl")).expect("user config")
    }

    fn page_index(app: &App, id: &str) -> usize {
        app.pages.iter().position(|p| p.id() == id).unwrap()
    }

    fn labels(app: &App) -> Vec<&'static str> {
        app.ctx.page_labels.clone()
    }

    #[test]
    fn app_keymap_resolves_page_switch_bindings() {
        let cfg = Config::builtin();
        let app = app_with(&cfg);
        assert_eq!(
            labels(&app),
            [
                "Git",
                "GitHub",
                "Files",
                "Docker",
                "Procs",
                "Worktrees",
                "Projects"
            ],
            "builtin `pages` order defines the slots"
        );
        let procs_idx = page_index(&app, "procs");
        let worktrees_idx = page_index(&app, "worktrees");
        let projects_idx = page_index(&app, "projects");

        let look = |s: &str| {
            let ki: KeyInput = s.parse().unwrap();
            app.app_keymap
                .lookup(KeyEvent::new(ki.code, ki.modifiers))
                .cloned()
        };
        assert_eq!(look("1"), Some(AppAction::SwitchPage(0)));
        assert_eq!(look("2"), Some(AppAction::SwitchPage(1)));
        assert_eq!(look("3"), Some(AppAction::SwitchPage(2)));
        assert_eq!(look("4"), Some(AppAction::SwitchPage(3)));
        assert_eq!(look("5"), Some(AppAction::SwitchPage(procs_idx)));
        assert_eq!(look("6"), Some(AppAction::SwitchPage(worktrees_idx)));
        assert_eq!(look("7"), Some(AppAction::SwitchPage(projects_idx)));
        assert_eq!(look("8"), None);
        assert_eq!(look("Ctrl+c"), Some(AppAction::Quit));
    }

    /// The header tab labels and the help "Switch view" entry are derived
    /// from the `app { }` block, so a page bound to `6` shows `6`, not its
    /// position (`5`).
    #[test]
    fn page_keys_follow_the_builtin_app_block() {
        let app = app_with(&Config::builtin());
        let keys: Vec<Vec<&str>> = app
            .ctx
            .page_keys
            .iter()
            .map(|k| k.iter().map(String::as_str).collect())
            .collect();
        assert_eq!(
            keys,
            vec![
                vec!["1"],
                vec!["2"],
                vec!["3"],
                vec!["4"],
                vec!["5"],
                vec!["6"],
                vec!["7"]
            ]
        );
        let help = app.active_help_bindings();
        assert_eq!(
            help[0],
            (
                "1 / 2 / 3 / 4 / 5 / 6 / 7".to_string(),
                "Switch view".to_string()
            )
        );
        // Pages no longer add their own view-switch entry.
        assert_eq!(help.iter().filter(|(_, v)| v == "Switch view").count(), 1);
    }

    #[test]
    fn page_keys_follow_user_rebindings() {
        // An extra key is appended after the built-in one.
        let app = app_with(&user_config(r#"app { "w" "page:worktrees" }"#));
        let idx = page_index(&app, "worktrees");
        assert_eq!(app.ctx.page_keys[idx], vec!["6", "w"]);
        assert_eq!(
            app.active_help_bindings()[0].0,
            "1 / 2 / 3 / 4 / 5 / 6 / w / 7"
        );

        // Unbinding the built-in key leaves only the user's key, so the
        // header shows `w:Worktrees`.
        let app = app_with(&user_config(r#"app { "6" "None"; "w" "page:worktrees" }"#));
        let idx = page_index(&app, "worktrees");
        assert_eq!(app.ctx.page_keys[idx], vec!["w"]);
        assert_eq!(app.active_help_bindings()[0].0, "1 / 2 / 3 / 4 / 5 / w / 7");
    }

    /// `pages` reorders the slots: the header (`page_labels`) follows the
    /// listed order while keys keep addressing pages by name.
    #[test]
    fn user_pages_reorder_slots_and_keep_keys_by_name() {
        let app = app_with(&user_config(r#"pages "files" "git""#));
        assert_eq!(labels(&app), ["Files", "Git"]);
        assert_eq!(page_index(&app, "files"), 0, "slot 1 is Files");
        assert_eq!(app.ctx.page_keys, vec![vec!["3"], vec!["1"]]);
        assert_eq!(app.active_help_bindings()[0].0, "3 / 1");

        let mut app = app_with(&user_config(r#"pages "worktrees" "git""#));
        assert_eq!(labels(&app), ["Worktrees", "Git"]);
        assert_eq!(app.active_help_bindings()[0].0, "6 / 1");
        // Keys still switch to the page they name, wherever its slot is.
        let ki: KeyInput = "1".parse().unwrap();
        app.handle_key(KeyEvent::new(ki.code, ki.modifiers))
            .unwrap();
        assert_eq!(app.ctx.active_page, page_index(&app, "git"));
    }

    /// Disabled pages are not constructed; their built-in keys are dropped
    /// (so `2` does nothing) rather than being an error.
    #[test]
    fn user_pages_disable_pages() {
        let mut app = app_with(&user_config(r#"pages "git" "files" "worktrees""#));
        assert_eq!(labels(&app), ["Git", "Files", "Worktrees"]);
        assert_eq!(app.pages.len(), 3);
        assert_eq!(app.ctx.page_keys, vec![vec!["1"], vec!["3"], vec!["6"]]);
        assert_eq!(app.active_help_bindings()[0].0, "1 / 3 / 6");
        let ki: KeyInput = "2".parse().unwrap();
        assert!(app
            .app_keymap
            .lookup(KeyEvent::new(ki.code, ki.modifiers))
            .is_none());
        app.handle_key(KeyEvent::new(ki.code, ki.modifiers))
            .unwrap();
        assert_eq!(app.ctx.active_page, 0);
    }

    /// A page that pushes a window on `d`; the window pops itself on `q`.
    struct Pusher;
    struct Window;

    impl PageState for Pusher {
        fn id(&self) -> &'static str {
            "pusher"
        }
        fn label(&self) -> &'static str {
            "Pusher"
        }
        fn handle_key(&mut self, _ctx: &mut AppContext, key: KeyEvent) -> Result<PageAction> {
            Ok(match key.code {
                crossterm::event::KeyCode::Char('d') => PageAction::Push(Page::new(Window)),
                _ => PageAction::None,
            })
        }
        fn render(
            &mut self,
            _f: &mut ratatui::Frame,
            _ctx: &AppContext,
            _a: ratatui::layout::Rect,
        ) {
        }
    }

    impl PageState for Window {
        fn id(&self) -> &'static str {
            "window"
        }
        fn label(&self) -> &'static str {
            "Window"
        }
        fn help_bindings(&self) -> Vec<(String, String)> {
            vec![("q".into(), "Close window".into())]
        }
        fn handle_key(&mut self, ctx: &mut AppContext, key: KeyEvent) -> Result<PageAction> {
            Ok(match key.code {
                crossterm::event::KeyCode::Char('q') => PageAction::Pop,
                crossterm::event::KeyCode::Char('m') => {
                    ctx.status_message = Some("from the window".into());
                    PageAction::None
                }
                _ => PageAction::None,
            })
        }
        fn render(
            &mut self,
            _f: &mut ratatui::Frame,
            _ctx: &AppContext,
            _a: ratatui::layout::Rect,
        ) {
        }
    }

    fn key(c: char) -> KeyEvent {
        KeyEvent::new(
            crossterm::event::KeyCode::Char(c),
            crossterm::event::KeyModifiers::NONE,
        )
    }

    /// `PageAction::Push` shows a window on top of the active page: it gets
    /// the keys and the help, `Pop` closes it (clearing its status message)
    /// and a page-switch key closes every window on its way to the page.
    #[test]
    fn windows_push_and_pop_over_the_active_page() {
        let cfg = Config::builtin();
        let cwd = std::env::current_dir().unwrap();
        let files = crate::files::page::new_page(&cwd, &cfg, None).unwrap();
        let app_keymap = build_app_keymap(
            &[
                ("1".to_string(), "page:pusher".to_string()),
                ("3".to_string(), "page:files".to_string()),
            ],
            &["pusher", "files"],
        )
        .unwrap();
        let mut app = App {
            ctx: AppContext {
                should_quit: false,
                active_page: 0,
                page_labels: vec!["Pusher", "Files"],
                page_keys: vec![],
                show_help: false,
                status_message: None,
                error_dialog: None,
                workdir: PathBuf::new(),
                needs_full_redraw: false,
                last_input: Instant::now(),
                auto_refresh: true,
            },
            pages: vec![Page::new(Pusher), files],
            windows: Vec::new(),
            app_keymap,
        };
        assert_eq!(app.window_depth(), 0);
        assert!(matches!(
            app.handle_key(key('d')).unwrap(),
            PageAction::None
        ));
        assert_eq!(app.window_depth(), 1);
        assert_eq!(
            app.active_help_bindings().last().unwrap().1,
            "Close window",
            "the window's help replaces the page's"
        );
        // Keys reach the window, not the page.
        app.handle_key(key('m')).unwrap();
        assert_eq!(app.ctx.status_message.as_deref(), Some("from the window"));
        app.handle_key(key('q')).unwrap();
        assert_eq!(app.window_depth(), 0);
        assert_eq!(
            app.ctx.status_message, None,
            "the window's message goes with it"
        );
        // A second window, then a page switch: the window closes.
        app.handle_key(key('d')).unwrap();
        assert_eq!(app.window_depth(), 1);
        app.handle_key(key('3')).unwrap();
        assert_eq!(app.window_depth(), 0);
        assert_eq!(app.ctx.active_page, 1);
    }

    #[test]
    fn page_keys_helper_groups_by_page_in_insertion_order() {
        let km = build_app_keymap(
            &[
                ("Ctrl+c".to_string(), "Quit".to_string()),
                ("2".to_string(), "page:b".to_string()),
                ("1".to_string(), "page:a".to_string()),
                ("Space".to_string(), "page:b".to_string()),
                ("9".to_string(), "SwitchPage:9".to_string()),
            ],
            &["a", "b", "c"],
        )
        .unwrap();
        let keys = page_keys(&km, 3);
        assert_eq!(keys[0], vec!["1"]);
        assert_eq!(keys[1], vec!["2", "Space"]);
        assert!(keys[2].is_empty());
        assert_eq!(keys.len(), 3, "out-of-range page indices are ignored");
    }
}
