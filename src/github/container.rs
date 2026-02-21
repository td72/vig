use crate::core::app::AppContext;
use crate::core::container::PaneContainer;
use crate::core::pane::{DetailPane, SelectPane};
use crate::core::ui::status_bar;
use crate::core::view::{View, ViewAction};
use crate::github::panes::{GhDetailViewPane, GhIssueListPane, GhPrListPane};
use crate::github::state::{GhFocusedPane, GitHubState};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{layout::Rect, Frame};
use std::any::Any;

// === Domain types ===

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum GhDetailId {
    DetailView,
}

pub struct GhPaneGroup {
    pub select: &'static dyn SelectPane<GitHubState>,
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

pub fn handle_gh_view_key(ctx: &mut AppContext, gh: &mut GitHubState, key: KeyEvent) -> Result<ViewAction> {
    match key.code {
        KeyCode::Char('q') => {
            ctx.should_quit = true;
        }
        KeyCode::Char('?') => {
            ctx.show_help = true;
        }
        KeyCode::Char('r') => {
            if gh.focused_pane == GhFocusedPane::Detail {
                gh.refresh_detail();
            } else {
                gh.refresh();
            }
        }
        KeyCode::Char('w') => {
            gh.toggle_watch_mode();
        }
        _ => dispatch_gh_key(ctx, gh, key),
    }
    Ok(ViewAction::None)
}

// === Dispatch ===

/// Dispatch a key event to the currently focused GitHub pane.
pub fn dispatch_gh_key(ctx: &mut AppContext, gh: &mut GitHubState, key: KeyEvent) {
    match gh.focused_pane {
        GhFocusedPane::Detail => GhDetailViewPane.handle_key(ctx, gh, key),
        _ => GhContainer.dispatch(ctx, gh, key),
    }
}

// === Container ===

pub(crate) struct GhContainer;

impl PaneContainer<GitHubState> for GhContainer {
    fn current_index(&self, state: &GitHubState) -> Option<usize> {
        GH_GROUPS.iter().position(|g| g.id == state.focused_pane)
    }
    fn focus_index(&self, _ctx: &mut AppContext, state: &mut GitHubState, idx: usize) {
        state.focused_pane = GH_GROUPS[idx].id;
        load_gh_detail_for_tab(state);
    }
    fn pane_at(&self, idx: usize) -> &'static dyn SelectPane<GitHubState> {
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

fn load_gh_detail_for_tab(state: &mut GitHubState) {
    match state.focused_pane {
        GhFocusedPane::IssueList => state.load_selected_issue_detail(),
        GhFocusedPane::PrList => state.load_selected_pr_detail(),
        _ => {}
    }
}

// === View ===

pub struct GhView;

impl View for GhView {
    fn handle_key(&self, ctx: &mut AppContext, state: &mut dyn Any, key: KeyEvent) -> Result<ViewAction> {
        let gh = state.downcast_mut::<GitHubState>().unwrap();
        handle_gh_view_key(ctx, gh, key)
    }

    fn render(&self, f: &mut Frame, ctx: &AppContext, state: &mut dyn Any, area: Rect) {
        let gh = state.downcast_mut::<GitHubState>().unwrap();
        let gl = crate::github::layout::compute_gh_layout(area);
        status_bar::render_gh_header(f, ctx, gl.header);
        GhIssueListPane.render(f, ctx, gh, gl.issue_list);
        GhPrListPane.render(f, ctx, gh, gl.pr_list);
        GhDetailViewPane.render(f, ctx, gh, gl.main_pane);
        status_bar::render_gh_status_bar(f, ctx, gh, gl.status_bar);
    }

    fn on_tick(&self, _ctx: &mut AppContext, state: &mut dyn Any) {
        let gh = state.downcast_mut::<GitHubState>().unwrap();
        gh.handle_watch_tick();
    }

    fn on_activate(&self, _ctx: &mut AppContext, state: &mut dyn Any) {
        let gh = state.downcast_mut::<GitHubState>().unwrap();
        gh.initialize();
    }
}
