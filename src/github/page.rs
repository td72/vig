use crate::core::app::AppContext;
use crate::core::page::{PageAction, PageHandler};
use crate::core::ui::status_bar;
use crate::github::state::{GhFocusedPane, GhPaneEvent, GitHubState};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{layout::Rect, Frame};

/// Create a GitHub page.
pub fn new_page() -> crate::core::app::Page {
    crate::core::app::Page::new(&GhPageHandler, GitHubState::new())
}

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
            if gh.shared.focused_pane == GhFocusedPane::Detail {
                gh.refresh_detail();
            } else {
                gh.refresh();
            }
        }
        KeyCode::Char('w') => {
            gh.toggle_watch_mode();
        }
        _ => {
            let events = dispatch_gh_key(gh, key);
            return process_gh_events(ctx, gh, events);
        }
    }
    Ok(PageAction::None)
}

// === Dispatch ===

fn dispatch_gh_key(gh: &mut GitHubState, key: KeyEvent) -> Vec<GhPaneEvent> {
    match gh.shared.focused_pane {
        GhFocusedPane::IssueList => match key.code {
            KeyCode::Char('l') | KeyCode::Tab => {
                vec![GhPaneEvent::SetFocus(GhFocusedPane::PrList)]
            }
            _ => gh.issue_list.handle_key(&gh.shared, key),
        },
        GhFocusedPane::PrList => match key.code {
            KeyCode::Char('h') | KeyCode::BackTab => {
                vec![GhPaneEvent::SetFocus(GhFocusedPane::IssueList)]
            }
            _ => gh.pr_list.handle_key(&gh.shared, key),
        },
        GhFocusedPane::Detail => gh.detail_view.handle_key(&gh.shared, key),
    }
}

fn load_gh_detail_for_tab(gh: &mut GitHubState) {
    gh.load_detail();
}

fn process_gh_events(
    ctx: &mut AppContext,
    gh: &mut GitHubState,
    events: Vec<GhPaneEvent>,
) -> Result<PageAction> {
    for event in events {
        match event {
            GhPaneEvent::SetFocus(pane) => {
                gh.set_focus(pane);
                load_gh_detail_for_tab(gh);
            }
            GhPaneEvent::LoadDetail => {
                gh.load_detail();
            }
            GhPaneEvent::OpenIssueBrowser(n) => {
                match crate::github::domain::client::open_issue_in_browser(n) {
                    Ok(()) => {
                        ctx.status_message = Some(format!("Opening issue #{n} in browser..."));
                    }
                    Err(e) => {
                        ctx.status_message = Some(format!("Failed to open browser: {e}"));
                    }
                }
            }
            GhPaneEvent::OpenPrBrowser(n) => {
                match crate::github::domain::client::open_pr_in_browser(n) {
                    Ok(()) => {
                        ctx.status_message = Some(format!("Opening PR #{n} in browser..."));
                    }
                    Err(e) => {
                        ctx.status_message = Some(format!("Failed to open browser: {e}"));
                    }
                }
            }
            GhPaneEvent::OpenUrl(url) => match crate::github::domain::client::open_url(&url) {
                Ok(()) => {
                    ctx.status_message = Some("Opening in browser...".to_string());
                }
                Err(e) => {
                    ctx.status_message = Some(e);
                }
            },
        }
    }
    Ok(PageAction::None)
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
        gh.issue_list.render(f, &gh.shared, gl.issue_list);
        gh.pr_list.render(f, &gh.shared, gl.pr_list);
        gh.detail_view.render(f, &gh.shared, gl.main_pane);
        status_bar::render_gh_status_bar(f, ctx, gh, gl.status_bar);
    }

    fn on_tick(&self, _ctx: &mut AppContext, gh: &mut GitHubState) {
        gh.handle_watch_tick();
    }

    fn on_activate(&self, _ctx: &mut AppContext, gh: &mut GitHubState) {
        gh.initialize();
    }
}
