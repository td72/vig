use crate::github::domain::disk_cache;
use crate::github::domain::types::GhPrListItem;
use crate::github::state::{GhFocusedPane, GhPaneEvent, GhShared};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};

pub struct GhPrListPane {
    pub prs: Vec<GhPrListItem>,
    pub selected_idx: usize,
    pub loading: bool,
}

impl GhPrListPane {
    pub fn new() -> Self {
        Self {
            prs: Vec::new(),
            selected_idx: 0,
            loading: false,
        }
    }

    /// Load cached list from disk for instant display.
    pub fn load_from_cache(&mut self) {
        if let Some(prs) = disk_cache::load_pr_list() {
            self.prs = prs;
        }
    }

    /// Apply a freshly fetched list — save to disk cache and update state.
    pub fn apply_list(&mut self, prs: Vec<GhPrListItem>) {
        disk_cache::save_pr_list(&prs);
        self.prs = prs;
    }

    pub fn handle_key(&mut self, _shared: &GhShared, key: KeyEvent) -> Vec<GhPaneEvent> {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if !self.prs.is_empty() && self.selected_idx + 1 < self.prs.len() {
                    self.selected_idx += 1;
                    return vec![GhPaneEvent::LoadSelectedPrDetail];
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.selected_idx > 0 {
                    self.selected_idx -= 1;
                    return vec![GhPaneEvent::LoadSelectedPrDetail];
                }
            }
            KeyCode::Char('g') => {
                self.selected_idx = 0;
                return vec![GhPaneEvent::LoadSelectedPrDetail];
            }
            KeyCode::Char('G') => {
                if !self.prs.is_empty() {
                    self.selected_idx = self.prs.len() - 1;
                    return vec![GhPaneEvent::LoadSelectedPrDetail];
                }
            }
            KeyCode::Char('i') | KeyCode::Enter => {
                if !self.prs.is_empty() {
                    return vec![
                        GhPaneEvent::SetFocus(GhFocusedPane::Detail),
                        GhPaneEvent::LoadSelectedPrDetail,
                    ];
                }
            }
            KeyCode::Char('o') => {
                if let Some(pr) = self.prs.get(self.selected_idx) {
                    return vec![GhPaneEvent::OpenPrBrowser(pr.number)];
                }
            }
            _ => {}
        }
        vec![]
    }

    pub fn render(&self, f: &mut Frame, shared: &GhShared, area: Rect) {
        let is_focused = shared.focused_pane == GhFocusedPane::PrList;
        let border_color = if is_focused {
            Color::Cyan
        } else {
            Color::DarkGray
        };

        let block = Block::default()
            .title(" Pull Requests ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color));

        if self.loading && self.prs.is_empty() {
            let items = vec![ListItem::new(Line::from(Span::styled(
                "  Loading...",
                Style::default().fg(Color::DarkGray),
            )))];
            let list = List::new(items).block(block);
            f.render_widget(list, area);
            return;
        }

        if self.prs.is_empty() {
            let items = vec![ListItem::new(Line::from(Span::styled(
                "  No pull requests",
                Style::default().fg(Color::DarkGray),
            )))];
            let list = List::new(items).block(block);
            f.render_widget(list, area);
            return;
        }

        let items: Vec<ListItem> = self
            .prs
            .iter()
            .map(|pr| {
                let (icon, icon_color) = match pr.state.as_str() {
                    "MERGED" => ("⊕", Color::Magenta),
                    "CLOSED" => ("✓", Color::Red),
                    _ => ("●", Color::Green), // OPEN
                };

                let mut spans = vec![
                    Span::raw(" "),
                    Span::styled(icon, Style::default().fg(icon_color)),
                    Span::raw(" "),
                    Span::styled(
                        format!("#{}", pr.number),
                        Style::default().fg(Color::Yellow),
                    ),
                    Span::raw(" "),
                    Span::raw(&pr.title),
                ];

                // Review badge
                if let Some(ref decision) = pr.review_decision {
                    match decision.as_str() {
                        "APPROVED" => {
                            spans.push(Span::raw(" "));
                            spans.push(Span::styled("✓", Style::default().fg(Color::Green)));
                        }
                        "CHANGES_REQUESTED" => {
                            spans.push(Span::raw(" "));
                            spans.push(Span::styled("✗", Style::default().fg(Color::Red)));
                        }
                        _ => {}
                    }
                }

                // Draft badge
                if pr.is_draft {
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(
                        "[draft]",
                        Style::default().fg(Color::DarkGray),
                    ));
                }

                ListItem::new(Line::from(spans))
            })
            .collect();

        let highlight_style = Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD);

        let list = List::new(items)
            .block(block)
            .highlight_style(highlight_style);

        let mut list_state = ListState::default();
        if is_focused
            || (shared.focused_pane == GhFocusedPane::Detail
                && shared.previous_pane == GhFocusedPane::PrList)
        {
            list_state.select(Some(self.selected_idx));
        }
        f.render_stateful_widget(list, area, &mut list_state);
    }
}
