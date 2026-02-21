use crate::core::page::{PageAction, PageHandler};
pub use crate::core::search::{SearchMatch, SearchOrigin, SearchState};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
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

pub trait PageState {
    fn drain_background(&mut self);
    fn search(&self) -> &SearchState;
    #[allow(dead_code)]
    fn search_mut(&mut self) -> &mut SearchState;
}

// --- Type-erasure layer (private) ---

trait PageInner {
    fn label(&self) -> &'static str;
    fn help_bindings(&self) -> Vec<(&'static str, &'static str)>;
    fn handle_key(&mut self, ctx: &mut AppContext, key: KeyEvent) -> Result<PageAction>;
    fn render(&mut self, f: &mut ratatui::Frame, ctx: &AppContext, area: ratatui::layout::Rect);
    fn intercepts_all_keys(&self, ctx: &AppContext) -> bool;
    fn on_tick(&mut self, ctx: &mut AppContext);
    fn on_fs_change(&mut self, ctx: &mut AppContext) -> Result<()>;
    fn on_suspend_return(
        &mut self,
        ctx: &mut AppContext,
        status: std::io::Result<std::process::ExitStatus>,
    ) -> Result<()>;
    fn on_activate(&mut self, ctx: &mut AppContext);
    fn drain_background(&mut self);
}

struct TypedPage<S: 'static> {
    handler: &'static dyn PageHandler<S>,
    state: S,
}

impl<S: PageState + 'static> PageInner for TypedPage<S> {
    fn label(&self) -> &'static str {
        self.handler.label()
    }
    fn help_bindings(&self) -> Vec<(&'static str, &'static str)> {
        self.handler.help_bindings()
    }
    fn handle_key(&mut self, ctx: &mut AppContext, key: KeyEvent) -> Result<PageAction> {
        self.handler.handle_key(ctx, &mut self.state, key)
    }
    fn render(&mut self, f: &mut ratatui::Frame, ctx: &AppContext, area: ratatui::layout::Rect) {
        self.handler.render(f, ctx, &mut self.state, area);
    }
    fn intercepts_all_keys(&self, ctx: &AppContext) -> bool {
        // Search input is a shared concern: when active, intercept all keys
        // regardless of individual PageHandler logic.
        self.state.search().active || self.handler.intercepts_all_keys(ctx, &self.state)
    }
    fn on_tick(&mut self, ctx: &mut AppContext) {
        self.handler.on_tick(ctx, &mut self.state);
    }
    fn on_fs_change(&mut self, ctx: &mut AppContext) -> Result<()> {
        self.handler.on_fs_change(ctx, &mut self.state)
    }
    fn on_suspend_return(
        &mut self,
        ctx: &mut AppContext,
        status: std::io::Result<std::process::ExitStatus>,
    ) -> Result<()> {
        self.handler.on_suspend_return(ctx, &mut self.state, status)
    }
    fn on_activate(&mut self, ctx: &mut AppContext) {
        self.handler.on_activate(ctx, &mut self.state);
    }
    fn drain_background(&mut self) {
        self.state.drain_background();
    }
}

// --- Public Page wrapper ---

pub struct Page(Box<dyn PageInner>);

impl Page {
    pub fn new<S: PageState + 'static>(handler: &'static dyn PageHandler<S>, state: S) -> Self {
        Self(Box::new(TypedPage { handler, state }))
    }

    pub fn label(&self) -> &'static str {
        self.0.label()
    }

    pub fn help_bindings(&self) -> Vec<(&'static str, &'static str)> {
        self.0.help_bindings()
    }

    fn handle_key(&mut self, ctx: &mut AppContext, key: KeyEvent) -> Result<PageAction> {
        self.0.handle_key(ctx, key)
    }

    fn render(&mut self, f: &mut ratatui::Frame, ctx: &AppContext, area: ratatui::layout::Rect) {
        self.0.render(f, ctx, area);
    }

    fn intercepts_all_keys(&self, ctx: &AppContext) -> bool {
        self.0.intercepts_all_keys(ctx)
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
}

impl App {
    pub fn new(ctx: AppContext, pages: Vec<Page>) -> Self {
        Self { ctx, pages }
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

    pub fn active_help_bindings(&self) -> Vec<(&'static str, &'static str)> {
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
        if self.pages[idx].intercepts_all_keys(&self.ctx) {
            return self.pages[idx].handle_key(&mut self.ctx, key);
        }

        // Ctrl+c always quits
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.ctx.should_quit = true;
            return Ok(PageAction::None);
        }

        // Page switching: '1'..'9' maps to page index 0..8
        if let KeyCode::Char(c @ '1'..='9') = key.code {
            let new_idx = (c as usize) - ('1' as usize);
            if new_idx < self.pages.len() && new_idx != idx {
                self.ctx.active_page = new_idx;
                self.pages[new_idx].on_activate(&mut self.ctx);
            }
            return Ok(PageAction::None);
        }

        // Delegate to active page
        self.pages[idx].handle_key(&mut self.ctx, key)
    }
}
