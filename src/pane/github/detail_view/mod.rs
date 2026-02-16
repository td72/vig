pub(crate) mod view;

use crate::app::App;
use crate::github::state::{GhDetailContent, GhDetailPane};
use crate::pane::DetailPane;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{layout::Rect, Frame};

pub struct GhDetailViewPane;

impl DetailPane for GhDetailViewPane {
    fn handle_key(&self, app: &mut App, key: KeyEvent) {
        // Determine item count for selection-based panes
        let pane = app.github.detail_pane;
        let item_count = match pane {
            GhDetailPane::Status => {
                if let GhDetailContent::Pr(ref detail) = app.github.detail {
                    view::sorted_checks(detail).len()
                } else {
                    0
                }
            }
            GhDetailPane::Reviews => {
                if let GhDetailContent::Pr(ref detail) = app.github.detail {
                    view::meaningful_reviews(&detail.reviews).len()
                } else {
                    0
                }
            }
            GhDetailPane::Comments => match &app.github.detail {
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
                    let idx = app.github.active_selected_idx_mut();
                    if *idx + 1 < item_count {
                        *idx += 1;
                        // Reset intra-item scroll when selection moves
                        *app.github.active_detail_scroll_mut() = 0;
                    } else {
                        // At last item — scroll within
                        let scroll = app.github.active_detail_scroll_mut();
                        *scroll = scroll.saturating_add(1);
                    }
                } else if !selectable {
                    let scroll = app.github.active_detail_scroll_mut();
                    *scroll = scroll.saturating_add(1);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if selectable {
                    let scroll_val = *app.github.active_detail_scroll_mut();
                    if scroll_val > 0 {
                        // Scroll back within current item first
                        *app.github.active_detail_scroll_mut() = scroll_val - 1;
                    } else {
                        let idx = app.github.active_selected_idx_mut();
                        *idx = idx.saturating_sub(1);
                    }
                } else {
                    let scroll = app.github.active_detail_scroll_mut();
                    *scroll = scroll.saturating_sub(1);
                }
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = (app.github.detail_view_height / 2).max(1);
                let scroll = app.github.active_detail_scroll_mut();
                *scroll = scroll.saturating_add(half);
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = (app.github.detail_view_height / 2).max(1);
                let scroll = app.github.active_detail_scroll_mut();
                *scroll = scroll.saturating_sub(half);
            }
            KeyCode::Char('g') => {
                if selectable {
                    *app.github.active_selected_idx_mut() = 0;
                }
                *app.github.active_detail_scroll_mut() = 0;
            }
            KeyCode::Char('G') => {
                if selectable && item_count > 0 {
                    *app.github.active_selected_idx_mut() = item_count - 1;
                }
                if !selectable || item_count > 0 {
                    *app.github.active_detail_scroll_mut() = u16::MAX / 2;
                }
            }
            KeyCode::Char('h') => {
                app.github.detail_pane = GhDetailPane::Body;
            }
            KeyCode::Char('l') => {
                match app.github.detail_pane {
                    GhDetailPane::Body => {
                        if app.github.is_pr() {
                            app.github.detail_pane = GhDetailPane::Status;
                        } else {
                            app.github.detail_pane = GhDetailPane::Comments;
                        }
                    }
                    _ if app.github.is_pr() => {
                        // Cycle right panes like Tab
                        app.github.detail_pane = match app.github.detail_pane {
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
                if app.github.is_pr() {
                    app.github.detail_pane = match app.github.detail_pane {
                        GhDetailPane::Status => GhDetailPane::Reviews,
                        GhDetailPane::Reviews => GhDetailPane::Comments,
                        GhDetailPane::Comments => GhDetailPane::Status,
                        other => other,
                    };
                }
            }
            KeyCode::BackTab => {
                // Cycle right panes backward (PR only)
                if app.github.is_pr() {
                    app.github.detail_pane = match app.github.detail_pane {
                        GhDetailPane::Status => GhDetailPane::Comments,
                        GhDetailPane::Reviews => GhDetailPane::Status,
                        GhDetailPane::Comments => GhDetailPane::Reviews,
                        other => other,
                    };
                }
            }
            KeyCode::Char('o') => {
                app.open_gh_detail_item();
            }
            KeyCode::Esc => {
                app.github.focused_pane = app.github.previous_pane;
                app.github.watch_mode = false;
            }
            _ => {}
        }
    }

    fn render(&self, f: &mut Frame, app: &mut App, area: Rect) {
        view::render(f, app, area);
    }
}
