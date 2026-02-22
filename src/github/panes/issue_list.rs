use crate::github::domain::types::GhIssueListItem;
use crate::github::domain::{client, disk_cache};
use crate::github::state::{GhBgMessage, GhFocusedPane, GhPaneEvent, GhShared};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};
use std::sync::mpsc;

pub struct GhIssueListPane {
    pub issues: Vec<GhIssueListItem>,
    pub selected_idx: usize,
    pub loading: bool,
}

impl GhIssueListPane {
    pub fn new() -> Self {
        Self {
            issues: Vec::new(),
            selected_idx: 0,
            loading: false,
        }
    }

    /// Load disk cache + spawn background fetch.
    pub fn initialize(&mut self, tx: &mpsc::Sender<GhBgMessage>) {
        if let Some(issues) = disk_cache::load_issue_list() {
            self.issues = issues;
        }
        self.loading = true;
        self.spawn_fetch(tx);
    }

    /// Spawn background fetch thread.
    pub fn spawn_fetch(&mut self, tx: &mpsc::Sender<GhBgMessage>) {
        self.loading = true;
        let tx = tx.clone();
        std::thread::spawn(move || {
            let issues = client::list_issues(50);
            let _ = tx.send(GhBgMessage::IssueList(issues));
        });
    }

    /// Apply a freshly fetched list — save to disk cache and update state.
    pub fn apply_list(&mut self, issues: Vec<GhIssueListItem>) {
        disk_cache::save_issue_list(&issues);
        self.issues = issues;
    }

    pub fn handle_key(&mut self, _shared: &GhShared, key: KeyEvent) -> Vec<GhPaneEvent> {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if !self.issues.is_empty() && self.selected_idx + 1 < self.issues.len() {
                    self.selected_idx += 1;
                    return vec![GhPaneEvent::LoadSelectedIssueDetail];
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.selected_idx > 0 {
                    self.selected_idx -= 1;
                    return vec![GhPaneEvent::LoadSelectedIssueDetail];
                }
            }
            KeyCode::Char('g') => {
                self.selected_idx = 0;
                return vec![GhPaneEvent::LoadSelectedIssueDetail];
            }
            KeyCode::Char('G') => {
                if !self.issues.is_empty() {
                    self.selected_idx = self.issues.len() - 1;
                    return vec![GhPaneEvent::LoadSelectedIssueDetail];
                }
            }
            KeyCode::Char('i') | KeyCode::Enter => {
                if !self.issues.is_empty() {
                    return vec![
                        GhPaneEvent::SetFocus(GhFocusedPane::Detail),
                        GhPaneEvent::LoadSelectedIssueDetail,
                    ];
                }
            }
            KeyCode::Char('o') => {
                if let Some(issue) = self.issues.get(self.selected_idx) {
                    return vec![GhPaneEvent::OpenIssueBrowser(issue.number)];
                }
            }
            _ => {}
        }
        vec![]
    }

    pub fn render(&self, f: &mut Frame, shared: &GhShared, area: Rect) {
        let is_focused = shared.focused_pane == GhFocusedPane::IssueList;
        let border_color = if is_focused {
            Color::Cyan
        } else {
            Color::DarkGray
        };

        let block = Block::default()
            .title(" Issues ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color));

        if self.loading && self.issues.is_empty() {
            let items = vec![ListItem::new(Line::from(Span::styled(
                "  Loading...",
                Style::default().fg(Color::DarkGray),
            )))];
            let list = List::new(items).block(block);
            f.render_widget(list, area);
            return;
        }

        if self.issues.is_empty() {
            let items = vec![ListItem::new(Line::from(Span::styled(
                "  No issues",
                Style::default().fg(Color::DarkGray),
            )))];
            let list = List::new(items).block(block);
            f.render_widget(list, area);
            return;
        }

        let items: Vec<ListItem> = self
            .issues
            .iter()
            .map(|issue| {
                let icon = if issue.state == "OPEN" { "●" } else { "✓" };
                let icon_color = if issue.state == "OPEN" {
                    Color::Green
                } else {
                    Color::Red
                };

                ListItem::new(Line::from(vec![
                    Span::raw(" "),
                    Span::styled(icon, Style::default().fg(icon_color)),
                    Span::raw(" "),
                    Span::styled(
                        format!("#{}", issue.number),
                        Style::default().fg(Color::Yellow),
                    ),
                    Span::raw(" "),
                    Span::raw(&issue.title),
                ]))
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
                && shared.previous_pane == GhFocusedPane::IssueList)
        {
            list_state.select(Some(self.selected_idx));
        }
        f.render_stateful_widget(list, area, &mut list_state);
    }
}
