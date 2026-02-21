pub(crate) mod view;

use crate::core::app::AppContext;
use crate::core::pane::DetailPane;
use crate::github::state::{GhDetailContent, GhDetailPane, GitHubState};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{layout::Rect, Frame};

pub(crate) fn open_gh_detail_item(ctx: &mut AppContext, gh: &mut GitHubState) {
    let url: Option<String> = match gh.detail_pane {
        GhDetailPane::Status => {
            if let GhDetailContent::Pr(ref detail) = gh.detail {
                let sorted = view::sorted_checks(detail);
                sorted
                    .get(gh.detail_check_idx)
                    .and_then(|c| c.details_url.clone())
            } else {
                None
            }
        }
        GhDetailPane::Reviews => {
            if let GhDetailContent::Pr(ref detail) = gh.detail {
                let reviews = view::meaningful_reviews(&detail.reviews);
                reviews.get(gh.detail_review_idx).and_then(|r| {
                    r.id.as_ref().and_then(|id| {
                        crate::github::client::repo_nwo().map(|nwo| {
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
                .get(gh.detail_comment_idx)
                .and_then(|c| c.url.clone()),
            GhDetailContent::Pr(detail) => detail
                .comments
                .get(gh.detail_comment_idx)
                .and_then(|c| c.url.clone()),
            _ => None,
        },
        GhDetailPane::Body => {
            // Open the issue/PR page itself
            match &gh.detail {
                GhDetailContent::Issue(issue) => {
                    let n = issue.number;
                    match crate::github::client::open_issue_in_browser(n) {
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
                    match crate::github::client::open_pr_in_browser(n) {
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
        match crate::github::client::open_url(&url) {
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
                    let idx = state.active_selected_idx_mut();
                    if *idx + 1 < item_count {
                        *idx += 1;
                        // Reset intra-item scroll when selection moves
                        *state.active_detail_scroll_mut() = 0;
                    } else {
                        // At last item — scroll within
                        let scroll = state.active_detail_scroll_mut();
                        *scroll = scroll.saturating_add(1);
                    }
                } else if !selectable {
                    let scroll = state.active_detail_scroll_mut();
                    *scroll = scroll.saturating_add(1);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if selectable {
                    let scroll_val = *state.active_detail_scroll_mut();
                    if scroll_val > 0 {
                        // Scroll back within current item first
                        *state.active_detail_scroll_mut() = scroll_val - 1;
                    } else {
                        let idx = state.active_selected_idx_mut();
                        *idx = idx.saturating_sub(1);
                    }
                } else {
                    let scroll = state.active_detail_scroll_mut();
                    *scroll = scroll.saturating_sub(1);
                }
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = (state.detail_view_height / 2).max(1);
                let scroll = state.active_detail_scroll_mut();
                *scroll = scroll.saturating_add(half);
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = (state.detail_view_height / 2).max(1);
                let scroll = state.active_detail_scroll_mut();
                *scroll = scroll.saturating_sub(half);
            }
            KeyCode::Char('g') => {
                if selectable {
                    *state.active_selected_idx_mut() = 0;
                }
                *state.active_detail_scroll_mut() = 0;
            }
            KeyCode::Char('G') => {
                if selectable && item_count > 0 {
                    *state.active_selected_idx_mut() = item_count - 1;
                }
                if !selectable || item_count > 0 {
                    *state.active_detail_scroll_mut() = u16::MAX / 2;
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
                state.focused_pane = state.previous_pane;
                state.watch_mode = false;
            }
            _ => {}
        }
    }

    fn render(&self, f: &mut Frame, _ctx: &AppContext, state: &mut GitHubState, area: Rect) {
        view::render(f, state, area);
    }
}
