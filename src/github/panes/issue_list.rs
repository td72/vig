use crate::core::pane::PaneEvent;
use crate::github::domain::types::GhIssueListItem;
use crate::github::domain::{client, disk_cache};
use crate::github::panes::gh_list::{GhListItem, GhListPane};
use crate::github::state::{
    GhBgMessage, GH_PANE_ISSUE_DETAIL, GH_PANE_ISSUE_LIST, GH_PANE_PR_LIST,
};
use crossterm::event::KeyCode;
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
    widgets::ListItem,
};
use std::sync::mpsc;

impl GhListItem for GhIssueListItem {
    fn pane_title() -> &'static str {
        "Issues"
    }

    fn empty_message() -> &'static str {
        "No issues"
    }

    fn render_item(&self) -> ListItem<'static> {
        let icon = if self.state == "OPEN" { "●" } else { "✓" };
        let icon_color = if self.state == "OPEN" {
            Color::Green
        } else {
            Color::Red
        };

        ListItem::new(Line::from(vec![
            Span::raw(" "),
            Span::styled(icon, Style::default().fg(icon_color)),
            Span::raw(" "),
            Span::styled(
                format!("#{}", self.number),
                Style::default().fg(Color::Yellow),
            ),
            Span::raw(" "),
            Span::raw(self.title.clone()),
        ]))
    }

    fn number(&self) -> u64 {
        self.number
    }

    fn search_text(&self) -> String {
        format!("#{} {}", self.number, self.title)
    }

    fn browser_event(&self) -> PaneEvent {
        PaneEvent::OpenIssueBrowser(self.number)
    }
}

pub type GhIssueListPane = GhListPane<GhIssueListItem>;

pub fn new_pane() -> GhIssueListPane {
    GhListPane::new(
        GH_PANE_ISSUE_LIST,
        GH_PANE_ISSUE_DETAIL,
        KeyCode::Tab,
        GH_PANE_PR_LIST,
    )
}

impl GhIssueListPane {
    /// Load disk cache + spawn background fetch.
    pub fn initialize(&mut self, tx: &mpsc::Sender<GhBgMessage>) {
        if let Some(issues) = disk_cache::load_issue_list() {
            self.items = issues;
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
        self.items = issues;
    }
}
