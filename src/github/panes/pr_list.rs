use crate::core::pane::PaneEvent;
use crate::github::domain::types::GhPrListItem;
use crate::github::domain::{client, disk_cache};
use crate::github::panes::gh_list::{GhListItem, GhListPane};
use crate::github::state::GhBgMessage;
use crossterm::event::KeyCode;
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
    widgets::ListItem,
};

impl GhListItem for GhPrListItem {
    fn pane_title() -> &'static str {
        "Pull Requests"
    }

    fn empty_message() -> &'static str {
        "No pull requests"
    }

    fn render_item(&self) -> ListItem<'static> {
        let (icon, icon_color) = match self.state.as_str() {
            "MERGED" => ("⊕", Color::Magenta),
            "CLOSED" => ("✓", Color::Red),
            _ => ("●", Color::Green),
        };

        let mut spans = vec![
            Span::raw(" "),
            Span::styled(icon, Style::default().fg(icon_color)),
            Span::raw(" "),
            Span::styled(
                format!("#{}", self.number),
                Style::default().fg(Color::Yellow),
            ),
            Span::raw(" "),
            Span::raw(self.title.clone()),
        ];

        // Review badge
        if let Some(ref decision) = self.review_decision {
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
        if self.is_draft {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                "[draft]",
                Style::default().fg(Color::DarkGray),
            ));
        }

        ListItem::new(Line::from(spans))
    }

    fn number(&self) -> u64 {
        self.number
    }

    fn search_text(&self) -> String {
        format!("#{} {}", self.number, self.title)
    }

    fn browser_event(&self) -> PaneEvent {
        PaneEvent::OpenPrBrowser(self.number)
    }

    fn load_disk_cache() -> Option<Vec<Self>> {
        disk_cache::load_pr_list()
    }

    fn save_disk_cache(items: &[Self]) {
        disk_cache::save_pr_list(items);
    }

    fn fetch_list() -> Result<Vec<Self>, String> {
        client::list_prs(50)
    }

    fn wrap_bg_message(result: Result<Vec<Self>, String>) -> GhBgMessage {
        GhBgMessage::PrList(result)
    }
}

pub type GhPrListPane = GhListPane<GhPrListItem>;

pub fn new_pane(pane_id: usize, detail_id: usize, switch_target: usize) -> GhPrListPane {
    GhListPane::new(pane_id, detail_id, KeyCode::BackTab, switch_target)
}
