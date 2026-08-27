use crate::core::pane::PaneEvent;
use crate::github::domain::types::GhIssueListItem;
use crate::github::domain::{client, disk_cache};
use crate::github::panes::gh_list::{GhListItem, GhListPane, TreePos};
use crate::github::state::GhBgMessage;
use crossterm::event::KeyCode;
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
    widgets::ListItem,
};

impl GhListItem for GhIssueListItem {
    fn pane_title() -> &'static str {
        "Issues"
    }

    fn empty_message() -> &'static str {
        "No issues"
    }

    fn render_item(&self, tree: &TreePos) -> ListItem<'static> {
        let icon = if self.state == "OPEN" { "●" } else { "✓" };
        let icon_color = if self.state == "OPEN" {
            Color::Green
        } else {
            Color::Red
        };

        ListItem::new(Line::from(vec![
            Span::raw(" "),
            Span::styled(tree.prefix.clone(), Style::default().fg(Color::DarkGray)),
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

    fn parent_number(&self, _items: &[Self]) -> Option<u64> {
        self.parent.as_ref().map(|p| p.number)
    }

    fn search_text(&self) -> String {
        format!("#{} {}", self.number, self.title)
    }

    fn browser_event(&self) -> PaneEvent {
        PaneEvent::OpenIssueBrowser(self.number)
    }

    fn load_disk_cache() -> Option<Vec<Self>> {
        disk_cache::load_issue_list()
    }

    fn save_disk_cache(items: &[Self]) {
        disk_cache::save_issue_list(items);
    }

    fn fetch_list() -> Result<Vec<Self>, String> {
        client::list_issues(50)
    }

    fn wrap_bg_message(result: Result<Vec<Self>, String>) -> GhBgMessage {
        GhBgMessage::IssueList(result)
    }
}

pub type GhIssueListPane = GhListPane<GhIssueListItem>;

pub fn new_pane(pane_id: usize, detail_id: usize, switch_target: usize) -> GhIssueListPane {
    GhListPane::new(pane_id, detail_id, KeyCode::Tab, switch_target)
}
