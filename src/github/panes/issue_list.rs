use crate::core::app::AppContext;
use crate::core::keymap::{execute_nav, nav_bindings, ActionHelp, Keymap, NavAction};
use crate::core::pane::{Pane, PaneEvent, PaneShared};
use crate::github::domain::types::GhIssueListItem;
use crate::github::domain::{client, disk_cache};
use crate::github::state::{
    GhBgMessage, GH_PANE_ISSUE_DETAIL, GH_PANE_ISSUE_LIST, GH_PANE_PR_LIST,
};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};
use std::sync::mpsc;

#[derive(Debug, Clone)]
pub enum IssueListAction {
    Nav(NavAction),
    OpenDetail,
    SwitchToPrList,
    OpenBrowser,
}

impl ActionHelp for IssueListAction {
    fn label(&self) -> Option<&'static str> {
        match self {
            IssueListAction::Nav(nav) => nav.label(),
            IssueListAction::OpenDetail => Some("Open detail"),
            IssueListAction::SwitchToPrList => Some("Switch to PRs"),
            IssueListAction::OpenBrowser => Some("Open in browser"),
        }
    }
}

pub fn default_keymap() -> Keymap<IssueListAction> {
    Keymap::new()
        .bindings(nav_bindings(IssueListAction::Nav))
        .key(KeyCode::Char('i'), IssueListAction::OpenDetail)
        .key(KeyCode::Enter, IssueListAction::OpenDetail)
        .key(KeyCode::Tab, IssueListAction::SwitchToPrList)
        .key(KeyCode::Char('o'), IssueListAction::OpenBrowser)
}

pub struct GhIssueListPane {
    pub issues: Vec<GhIssueListItem>,
    pub selected_idx: usize,
    pub loading: bool,
    keymap: Keymap<IssueListAction>,
}

impl GhIssueListPane {
    pub fn new() -> Self {
        Self {
            issues: Vec::new(),
            selected_idx: 0,
            loading: false,
            keymap: default_keymap(),
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

    /// Return the number of the currently selected issue, if any.
    pub fn selected_number(&self) -> Option<u64> {
        self.issues.get(self.selected_idx).map(|i| i.number)
    }

    pub fn handle_key(&mut self, _shared: &PaneShared, key: KeyEvent) -> Vec<PaneEvent> {
        let action = match self.keymap.lookup(key) {
            Some(a) => a.clone(),
            None => return vec![],
        };
        self.execute(action)
    }

    fn execute(&mut self, action: IssueListAction) -> Vec<PaneEvent> {
        match action {
            IssueListAction::Nav(nav) => {
                if execute_nav(nav, &mut self.selected_idx, self.issues.len(), None) {
                    return vec![PaneEvent::SelectionChanged];
                }
            }
            IssueListAction::OpenDetail => {
                if !self.issues.is_empty() {
                    return vec![PaneEvent::SetFocus(GH_PANE_ISSUE_DETAIL)];
                }
            }
            IssueListAction::SwitchToPrList => {
                return vec![PaneEvent::SetFocus(GH_PANE_PR_LIST)];
            }
            IssueListAction::OpenBrowser => {
                if let Some(issue) = self.issues.get(self.selected_idx) {
                    return vec![PaneEvent::OpenIssueBrowser(issue.number)];
                }
            }
        }
        vec![]
    }

    pub fn render(&mut self, f: &mut Frame, _ctx: &AppContext, shared: &PaneShared, area: Rect) {
        let is_focused = shared.focused_pane == GH_PANE_ISSUE_LIST;
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
            || (shared.focused_pane == GH_PANE_ISSUE_DETAIL
                && shared.previous_pane == GH_PANE_ISSUE_LIST)
        {
            list_state.select(Some(self.selected_idx));
        }
        f.render_stateful_widget(list, area, &mut list_state);
    }
}

impl Pane<PaneEvent> for GhIssueListPane {
    fn handle_key(&mut self, shared: &PaneShared, key: KeyEvent) -> Vec<PaneEvent> {
        self.handle_key(shared, key)
    }
    fn render(&mut self, f: &mut Frame, ctx: &AppContext, shared: &PaneShared, area: Rect) {
        self.render(f, ctx, shared, area)
    }
}
