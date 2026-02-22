pub(crate) mod view;

use crate::github::state::{
    GhDetailContent, GhDetailPane, GhDetailViewPane, GhPaneEvent, GhShared,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{layout::Rect, Frame};

impl GhDetailViewPane {
    pub fn handle_key(&mut self, shared: &GhShared, key: KeyEvent) -> Vec<GhPaneEvent> {
        // Determine item count for selection-based panes
        let pane = self.active_pane;
        let item_count = match pane {
            GhDetailPane::Status => {
                if let GhDetailContent::Pr(ref detail) = self.content {
                    view::sorted_checks(detail).len()
                } else {
                    0
                }
            }
            GhDetailPane::Reviews => {
                if let GhDetailContent::Pr(ref detail) = self.content {
                    view::meaningful_reviews(&detail.reviews).len()
                } else {
                    0
                }
            }
            GhDetailPane::Comments => match &self.content {
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
                    let s = self.active_scroll_mut();
                    if s.selected_idx + 1 < item_count {
                        s.selected_idx += 1;
                        s.scroll_y = 0;
                    } else {
                        s.scroll_y = s.scroll_y.saturating_add(1);
                    }
                } else if !selectable {
                    let s = self.active_scroll_mut();
                    s.scroll_y = s.scroll_y.saturating_add(1);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if selectable {
                    let s = self.active_scroll_mut();
                    if s.scroll_y > 0 {
                        s.scroll_y -= 1;
                    } else {
                        s.selected_idx = s.selected_idx.saturating_sub(1);
                    }
                } else {
                    let s = self.active_scroll_mut();
                    s.scroll_y = s.scroll_y.saturating_sub(1);
                }
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = (self.view_height / 2).max(1);
                let s = self.active_scroll_mut();
                s.scroll_y = s.scroll_y.saturating_add(half);
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = (self.view_height / 2).max(1);
                let s = self.active_scroll_mut();
                s.scroll_y = s.scroll_y.saturating_sub(half);
            }
            KeyCode::Char('g') => {
                let s = self.active_scroll_mut();
                if selectable {
                    s.selected_idx = 0;
                }
                s.scroll_y = 0;
            }
            KeyCode::Char('G') => {
                let s = self.active_scroll_mut();
                if selectable && item_count > 0 {
                    s.selected_idx = item_count - 1;
                }
                if !selectable || item_count > 0 {
                    s.scroll_y = u16::MAX / 2;
                }
            }
            KeyCode::Char('h') => {
                self.active_pane = GhDetailPane::Body;
            }
            KeyCode::Char('l') => {
                match self.active_pane {
                    GhDetailPane::Body => {
                        if self.is_pr() {
                            self.active_pane = GhDetailPane::Status;
                        } else {
                            self.active_pane = GhDetailPane::Comments;
                        }
                    }
                    _ if self.is_pr() => {
                        // Cycle right panes like Tab
                        self.active_pane = match self.active_pane {
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
                if self.is_pr() {
                    self.active_pane = match self.active_pane {
                        GhDetailPane::Status => GhDetailPane::Reviews,
                        GhDetailPane::Reviews => GhDetailPane::Comments,
                        GhDetailPane::Comments => GhDetailPane::Status,
                        other => other,
                    };
                }
            }
            KeyCode::BackTab => {
                if self.is_pr() {
                    self.active_pane = match self.active_pane {
                        GhDetailPane::Status => GhDetailPane::Comments,
                        GhDetailPane::Reviews => GhDetailPane::Status,
                        GhDetailPane::Comments => GhDetailPane::Reviews,
                        other => other,
                    };
                }
            }
            KeyCode::Char('o') => {
                return self.open_detail_item();
            }
            KeyCode::Esc => {
                return vec![GhPaneEvent::SetFocus(shared.previous_pane)];
            }
            _ => {}
        }
        vec![]
    }

    fn open_detail_item(&self) -> Vec<GhPaneEvent> {
        let url: Option<String> = match self.active_pane {
            GhDetailPane::Status => {
                if let GhDetailContent::Pr(ref detail) = self.content {
                    let sorted = view::sorted_checks(detail);
                    sorted
                        .get(self.status.selected_idx)
                        .and_then(|c| c.details_url.clone())
                } else {
                    None
                }
            }
            GhDetailPane::Reviews => {
                if let GhDetailContent::Pr(ref detail) = self.content {
                    let reviews = view::meaningful_reviews(&detail.reviews);
                    reviews.get(self.reviews.selected_idx).and_then(|r| {
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
            GhDetailPane::Comments => match &self.content {
                GhDetailContent::Issue(detail) => detail
                    .comments
                    .get(self.comments.selected_idx)
                    .and_then(|c| c.url.clone()),
                GhDetailContent::Pr(detail) => detail
                    .comments
                    .get(self.comments.selected_idx)
                    .and_then(|c| c.url.clone()),
                _ => None,
            },
            GhDetailPane::Body => match &self.content {
                GhDetailContent::Issue(issue) => {
                    return vec![GhPaneEvent::OpenIssueBrowser(issue.number)];
                }
                GhDetailContent::Pr(pr) => {
                    return vec![GhPaneEvent::OpenPrBrowser(pr.number)];
                }
                _ => return vec![],
            },
        };

        if let Some(url) = url {
            vec![GhPaneEvent::OpenUrl(url)]
        } else {
            vec![]
        }
    }

    pub fn render(&mut self, f: &mut Frame, shared: &GhShared, area: Rect) {
        view::render(f, self, shared, area);
    }
}
