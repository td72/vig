use crate::core::app::App;
use crate::core::container::PaneContainer;
use crate::core::pane::{DetailPane, SelectPane};
use crate::github::panes::{GhDetailViewPane, GhIssueListPane, GhPrListPane};
use crate::github::state::GhFocusedPane;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

// === Domain types ===

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum GhDetailId {
    DetailView,
}

pub struct GhPaneGroup {
    pub select: &'static dyn SelectPane,
    #[allow(dead_code)]
    pub detail: GhDetailId,
    pub id: GhFocusedPane,
}

// === Tab definitions ===

pub static GH_GROUPS: &[GhPaneGroup] = &[
    GhPaneGroup {
        select: &GhIssueListPane,
        detail: GhDetailId::DetailView,
        id: GhFocusedPane::IssueList,
    },
    GhPaneGroup {
        select: &GhPrListPane,
        detail: GhDetailId::DetailView,
        id: GhFocusedPane::PrList,
    },
];

// === View-level key handling ===

pub fn handle_gh_view_key(app: &mut App, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Char('q') => {
            app.ctx.should_quit = true;
        }
        KeyCode::Char('?') => {
            app.ctx.show_help = true;
        }
        KeyCode::Char('r') => {
            if app.github.focused_pane == GhFocusedPane::Detail {
                app.github.refresh_detail();
            } else {
                app.github.refresh();
            }
        }
        KeyCode::Char('w') => {
            app.github.toggle_watch_mode();
        }
        _ => dispatch_gh_key(app, key),
    }
    Ok(false)
}

// === Dispatch ===

/// Dispatch a key event to the currently focused GitHub pane.
pub fn dispatch_gh_key(app: &mut App, key: KeyEvent) {
    match app.github.focused_pane {
        GhFocusedPane::Detail => GhDetailViewPane.handle_key(app, key),
        _ => GhContainer.dispatch(app, key),
    }
}

// === Container ===

pub(crate) struct GhContainer;

impl PaneContainer for GhContainer {
    fn current_index(&self, app: &App) -> Option<usize> {
        GH_GROUPS.iter().position(|g| g.id == app.github.focused_pane)
    }
    fn focus_index(&self, app: &mut App, idx: usize) {
        app.github.focused_pane = GH_GROUPS[idx].id;
        load_gh_detail_for_tab(app);
    }
    fn pane_at(&self, idx: usize) -> &'static dyn SelectPane {
        GH_GROUPS[idx].select
    }
    fn len(&self) -> usize {
        GH_GROUPS.len()
    }

    fn is_prev_key(&self, key: &KeyEvent) -> bool {
        matches!(key.code, KeyCode::Char('h') | KeyCode::BackTab)
    }
    fn is_next_key(&self, key: &KeyEvent) -> bool {
        matches!(key.code, KeyCode::Char('l') | KeyCode::Tab)
    }
}

fn load_gh_detail_for_tab(app: &mut App) {
    match app.github.focused_pane {
        GhFocusedPane::IssueList => app.github.load_selected_issue_detail(),
        GhFocusedPane::PrList => app.github.load_selected_pr_detail(),
        _ => {}
    }
}
