use crate::core::page::{PageAction, PageHandler};
pub use crate::core::search::{SearchMatch, SearchOrigin, SearchState};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::any::Any;
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
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

pub struct Page {
    pub handler: &'static dyn PageHandler,
    pub state: Box<dyn PageState>,
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
            page.state.drain_background();
        }
    }

    pub fn render(&mut self, f: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let idx = self.ctx.active_page;
        let handler = self.pages[idx].handler;
        let state = self.pages[idx].state.as_any_mut();
        handler.render(f, &self.ctx, state, area);
    }

    pub fn on_fs_change(&mut self) -> Result<()> {
        for page in &mut self.pages {
            let handler = page.handler;
            let state = page.state.as_any_mut();
            handler.on_fs_change(&mut self.ctx, state)?;
        }
        Ok(())
    }

    pub fn on_tick(&mut self) {
        let idx = self.ctx.active_page;
        let handler = self.pages[idx].handler;
        let state = self.pages[idx].state.as_any_mut();
        handler.on_tick(&mut self.ctx, state);
    }

    pub fn on_suspend_return(
        &mut self,
        status: std::io::Result<std::process::ExitStatus>,
    ) -> Result<()> {
        let idx = self.ctx.active_page;
        let handler = self.pages[idx].handler;
        let state = self.pages[idx].state.as_any_mut();
        handler.on_suspend_return(&mut self.ctx, state, status)
    }

    pub fn active_help_bindings(&self) -> Vec<(&'static str, &'static str)> {
        self.pages[self.ctx.active_page].handler.help_bindings()
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
        let handler = self.pages[idx].handler;
        if handler.intercepts_all_keys(&self.ctx, self.pages[idx].state.as_any()) {
            let state = self.pages[idx].state.as_any_mut();
            return handler.handle_key(&mut self.ctx, state, key);
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
                let handler = self.pages[new_idx].handler;
                let state = self.pages[new_idx].state.as_any_mut();
                handler.on_activate(&mut self.ctx, state);
            }
            return Ok(PageAction::None);
        }

        // Delegate to active page
        let state = self.pages[idx].state.as_any_mut();
        handler.handle_key(&mut self.ctx, state, key)
    }
}
