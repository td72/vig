pub(crate) mod view;

use crate::core::app::AppContext;
use crate::core::pane::{DetailPane, DetailState, FocusState};
use crate::github::state::{GhDetailContent, GhDetailPane, GitHubState};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{layout::Rect, Frame};

pub(crate) fn open_gh_detail_item(ctx: &mut AppContext, gh: &mut GitHubState) {
    let url: Option<String> = match gh.detail_pane {
        GhDetailPane::Status => {
            if let GhDetailContent::Pr(ref detail) = gh.detail {
                let sorted = view::sorted_checks(detail);
                sorted
                    .get(gh.detail_status.selected_idx)
                    .and_then(|c| c.details_url.clone())
            } else {
                None
            }
        }
        GhDetailPane::Reviews => {
            if let GhDetailContent::Pr(ref detail) = gh.detail {
                let reviews = view::meaningful_reviews(&detail.reviews);
                reviews.get(gh.detail_reviews.selected_idx).and_then(|r| {
                    r.id.as_ref().and_then(|id| {
                        crate::github::domain::client::repo_nwo().map(|nwo| {
                            format!(
                                "https://github.com/{}/pull/{}#pullrequestreview-{}",
                                nwo, detail.number, id
                            )
                        })
                    })
                })
            } else {
                None
            }
        }
        GhDetailPane::Comments => match &gh.detail {
            GhDetailContent::Issue(detail) => detail
                .comments
                .get(gh.detail_comments.selected_idx)
                .and_then(|c| c.url.clone()),
            GhDetailContent::Pr(detail) => detail
                .comments
                .get(gh.detail_comments.selected_idx)
                .and_then(|c| c.url.clone()),
            _ => None,
        },
        GhDetailPane::Body => {
            // Open the issue/PR page itself
            match &gh.detail {
                GhDetailContent::Issue(issue) => {
                    let n = issue.number;
                    match crate::github::domain::client::open_issue_in_browser(n) {
                        Ok(()) => {
                            ctx.status_message = Some(format!("Opening issue #{n} in browser..."));
                        }
                        Err(e) => {
                            ctx.status_message = Some(format!("Failed to open browser: {e}"));
                        }
                    }
                    return;
                }
                GhDetailContent::Pr(pr) => {
                    let n = pr.number;
                    match crate::github::domain::client::open_pr_in_browser(n) {
                        Ok(()) => {
                            ctx.status_message = Some(format!("Opening PR #{n} in browser..."));
                        }
                        Err(e) => {
                            ctx.status_message = Some(format!("Failed to open browser: {e}"));
                        }
                    }
                    return;
                }
                _ => return,
            }
        }
    };

    if let Some(url) = url {
        match crate::github::domain::client::open_url(&url) {
            Ok(()) => {
                ctx.status_message = Some("Opening in browser...".to_string());
            }
            Err(e) => {
                ctx.status_message = Some(e);
            }
        }
    }
}

pub struct GhDetailViewPane;

impl DetailPane<GitHubState> for GhDetailViewPane {
    fn handle_key(&self, ctx: &mut AppContext, state: &mut GitHubState, key: KeyEvent) {
        // Determine item count for selection-based panes
        let pane = state.detail_pane;
        let item_count = match pane {
            GhDetailPane::Status => {
                if let GhDetailContent::Pr(ref detail) = state.detail {
                    view::sorted_checks(detail).len()
                } else {
                    0
                }
            }
            GhDetailPane::Reviews => {
                if let GhDetailContent::Pr(ref detail) = state.detail {
                    view::meaningful_reviews(&detail.reviews).len()
                } else {
                    0
                }
            }
            GhDetailPane::Comments => match &state.detail {
                GhDetailContent::Issue(detail) => detail.comments.len(),
                GhDetailContent::Pr(detail) => detail.comments.len(),
                _ => 0,
            },
            GhDetailPane::Body => 0, // scroll-based
        };
        let selectable = pane != GhDetailPane::Body;

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if selectable && item_count > 0 {
                    let s = state.active_scroll_mut();
                    if s.selected_idx + 1 < item_count {
                        s.selected_idx += 1;
                        s.scroll_y = 0;
                    } else {
                        s.scroll_y = s.scroll_y.saturating_add(1);
                    }
                } else if !selectable {
                    let s = state.active_scroll_mut();
                    s.scroll_y = s.scroll_y.saturating_add(1);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if selectable {
                    let s = state.active_scroll_mut();
                    if s.scroll_y > 0 {
                        s.scroll_y -= 1;
                    } else {
                        s.selected_idx = s.selected_idx.saturating_sub(1);
                    }
                } else {
                    let s = state.active_scroll_mut();
                    s.scroll_y = s.scroll_y.saturating_sub(1);
                }
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = (state.detail_view_height / 2).max(1);
                let s = state.active_scroll_mut();
                s.scroll_y = s.scroll_y.saturating_add(half);
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = (state.detail_view_height / 2).max(1);
                let s = state.active_scroll_mut();
                s.scroll_y = s.scroll_y.saturating_sub(half);
            }
            KeyCode::Char('g') => {
                let s = state.active_scroll_mut();
                if selectable {
                    s.selected_idx = 0;
                }
                s.scroll_y = 0;
            }
            KeyCode::Char('G') => {
                let s = state.active_scroll_mut();
                if selectable && item_count > 0 {
                    s.selected_idx = item_count - 1;
                }
                if !selectable || item_count > 0 {
                    s.scroll_y = u16::MAX / 2;
                }
            }
            KeyCode::Char('h') => {
                state.detail_pane = GhDetailPane::Body;
            }
            KeyCode::Char('l') => {
                match state.detail_pane {
                    GhDetailPane::Body => {
                        if state.is_pr() {
                            state.detail_pane = GhDetailPane::Status;
                        } else {
                            state.detail_pane = GhDetailPane::Comments;
                        }
                    }
                    _ if state.is_pr() => {
                        // Cycle right panes like Tab
                        state.detail_pane = match state.detail_pane {
                            GhDetailPane::Status => GhDetailPane::Reviews,
                            GhDetailPane::Reviews => GhDetailPane::Comments,
                            GhDetailPane::Comments => GhDetailPane::Status,
                            other => other,
                        };
                    }
                    _ => {}
                }
            }
            KeyCode::Tab => {
                // Cycle right panes forward: Status → Reviews → Comments → Status (PR only)
                if state.is_pr() {
                    state.detail_pane = match state.detail_pane {
                        GhDetailPane::Status => GhDetailPane::Reviews,
                        GhDetailPane::Reviews => GhDetailPane::Comments,
                        GhDetailPane::Comments => GhDetailPane::Status,
                        other => other,
                    };
                }
            }
            KeyCode::BackTab => {
                // Cycle right panes backward (PR only)
                if state.is_pr() {
                    state.detail_pane = match state.detail_pane {
                        GhDetailPane::Status => GhDetailPane::Comments,
                        GhDetailPane::Reviews => GhDetailPane::Status,
                        GhDetailPane::Comments => GhDetailPane::Reviews,
                        other => other,
                    };
                }
            }
            KeyCode::Char('o') => {
                open_gh_detail_item(ctx, state);
            }
            KeyCode::Esc => {
                let prev = state.previous_pane;
                state.set_focus(prev);
                state.watch_mode = false;
            }
            _ => {}
        }
    }

    fn render(&self, f: &mut Frame, _ctx: &AppContext, state: &mut GitHubState, area: Rect) {
        view::render(f, state, area);
    }
}
