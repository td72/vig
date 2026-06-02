use crate::core::keymap::{build_app_keymap, AppAction, Keymap};
use crate::core::page::PageAction;
pub use crate::core::search::SearchMatch;
use anyhow::Result;
use crossterm::event::KeyEvent;
use std::path::PathBuf;

pub struct ErrorDialogState {
    pub title: String,
    pub message: String,
}

pub struct AppContext {
    pub should_quit: bool,
    pub active_page: usize,
    pub page_labels: Vec<&'static str>,
    pub show_help: bool,
    pub status_message: Option<String>,
    pub error_dialog: Option<ErrorDialogState>,
    pub workdir: PathBuf,
}

impl AppContext {
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
                    self.status_message = Some(format!(
                        "Yanked {line_count} line{}",
                        if line_count == 1 { "" } else { "s" }
                    ));
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
    app_keymap: Keymap<AppAction>,
}

impl App {
    pub fn new(ctx: AppContext, pages: Vec<Page>) -> Self {
        let page_names: Vec<&str> = pages.iter().map(|p| p.label()).collect();
        let entries = crate::core::config::load_app_entries();
        let app_keymap = build_app_keymap(&entries, &page_names)
            .expect("default.kdl app keymap is always valid");
        Self {
            ctx,
            pages,
            app_keymap,
        }
    }

    pub fn drain_all_background(&mut self) {
        for page in &mut self.pages {
            page.drain_background();
        }
    }

    pub fn render(&mut self, f: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let idx = self.ctx.active_page;
        self.pages[idx].render(f, &self.ctx, area);
    }

    pub fn on_fs_change(&mut self) -> Result<()> {
        for page in &mut self.pages {
            page.on_fs_change(&mut self.ctx)?;
        }
        Ok(())
    }

    pub fn on_tick(&mut self) {
        let idx = self.ctx.active_page;
        self.pages[idx].on_tick(&mut self.ctx);
    }

    pub fn on_suspend_return(
        &mut self,
        status: std::io::Result<std::process::ExitStatus>,
    ) -> Result<()> {
        let idx = self.ctx.active_page;
        self.pages[idx].on_suspend_return(&mut self.ctx, status)
    }

    pub fn active_help_bindings(&self) -> Vec<(String, String)> {
        self.pages[self.ctx.active_page].help_bindings()
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Result<PageAction> {
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
        if self.pages[idx].intercepts_all_keys() {
            return self.pages[idx].handle_key(&mut self.ctx, key);
        }

        // App-level keymap handles Quit and page switching.
        if let Some(action) = self.app_keymap.lookup(key) {
            match action.clone() {
                AppAction::Quit => {
                    self.ctx.should_quit = true;
                    return Ok(PageAction::None);
                }
                AppAction::SwitchPage(new_idx) => {
                    if new_idx < self.pages.len() && new_idx != idx {
                        self.ctx.active_page = new_idx;
                        self.pages[new_idx].on_activate(&mut self.ctx);
                    }
                    return Ok(PageAction::None);
                }
            }
        }

        // Delegate to active page
        self.pages[idx].handle_key(&mut self.ctx, key)
    }
}
