use crate::core::app::{AppContext, Page};
use crate::core::page::{PageAction, PageHandler};
use crate::core::pane::{DetailPane, SelectPane};
use crate::core::pane_router::{PaneRouter, PaneTab};
use crate::core::ui::status_bar;
use crate::github::panes::{GhDetailViewPane, GhIssueListPane, GhPrListPane};
use crate::github::state::{GhFocusedPane, GitHubState};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{layout::Rect, Frame};

/// Create a GitHub page.
pub fn new_page() -> Page {
    Page::new(&GhPageHandler, GitHubState::new())
}

// === Tab definitions ===

pub static GH_TABS: &[PaneTab<GitHubState, GhFocusedPane>] = &[
    PaneTab {
        select: &GhIssueListPane,
        id: GhFocusedPane::IssueList,
    },
    PaneTab {
        select: &GhPrListPane,
        id: GhFocusedPane::PrList,
    },
];

// === View-level key handling ===

pub fn handle_gh_view_key(
    ctx: &mut AppContext,
    gh: &mut GitHubState,
    key: KeyEvent,
) -> Result<PageAction> {
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
    Ok(PageAction::None)
}

// === Dispatch ===

/// Dispatch a key event to the currently focused GitHub pane.
pub fn dispatch_gh_key(ctx: &mut AppContext, gh: &mut GitHubState, key: KeyEvent) {
    match gh.focused_pane {
        GhFocusedPane::Detail => GhDetailViewPane.handle_key(ctx, gh, key),
        _ => GhPaneRouter.dispatch(ctx, gh, key),
    }
}

// === Container ===

pub(crate) struct GhPaneRouter;

impl PaneRouter<GitHubState, GhFocusedPane> for GhPaneRouter {
    fn tabs(&self) -> &'static [PaneTab<GitHubState, GhFocusedPane>] {
        GH_TABS
    }
    fn on_focus_change(&self, _ctx: &mut AppContext, state: &mut GitHubState) {
        load_gh_detail_for_tab(state);
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

pub struct GhPageHandler;

impl PageHandler<GitHubState> for GhPageHandler {
    fn label(&self) -> &'static str {
        "GitHub"
    }

    fn help_bindings(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            ("1 / 2", "Switch view"),
            ("h / l", "Issues ↔ PRs (list)"),
            ("j / k", "Navigate list"),
            ("i / Enter", "Open detail"),
            ("o", "Open in browser"),
            ("Esc", "Back to list"),
            ("h / l", "Body ↔ Right pane (detail)"),
            ("Tab / S-Tab", "Cycle right panes (detail)"),
            ("Ctrl+d", "Half page down (detail)"),
            ("Ctrl+u", "Half page up (detail)"),
            ("g / G", "Top / Bottom"),
            ("r", "Refresh data"),
            ("w", "Toggle watch mode (PR)"),
            ("?", "Toggle help"),
            ("q", "Quit"),
        ]
    }

    fn handle_key(
        &self,
        ctx: &mut AppContext,
        gh: &mut GitHubState,
        key: KeyEvent,
    ) -> Result<PageAction> {
        handle_gh_view_key(ctx, gh, key)
    }

    fn render(&self, f: &mut Frame, ctx: &AppContext, gh: &mut GitHubState, area: Rect) {
        let gl = crate::github::layout::compute_gh_layout(area);
        status_bar::render_gh_header(f, ctx, gl.header);
        GhIssueListPane.render(f, ctx, gh, gl.issue_list);
        GhPrListPane.render(f, ctx, gh, gl.pr_list);
        GhDetailViewPane.render(f, ctx, gh, gl.main_pane);
        status_bar::render_gh_status_bar(f, ctx, gh, gl.status_bar);
    }

    fn on_tick(&self, _ctx: &mut AppContext, gh: &mut GitHubState) {
        gh.handle_watch_tick();
    }

    fn on_activate(&self, _ctx: &mut AppContext, gh: &mut GitHubState) {
        gh.initialize();
    }
}
