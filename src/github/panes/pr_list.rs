use crate::core::pane::PaneEvent;
use crate::github::domain::types::GhPrListItem;
use crate::github::domain::{client, disk_cache};
use crate::github::panes::gh_list::{GhListItem, GhListPane, TreePos};
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

    fn render_item(&self, tree: &TreePos) -> ListItem<'static> {
        let (icon, icon_color) = match self.state.as_str() {
            "MERGED" => ("⊕", Color::Magenta),
            "CLOSED" => ("✓", Color::Red),
            _ => ("●", Color::Green),
        };

        let mut spans = vec![
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

        // Stacked PR: show which branch it is based on
        if tree.depth > 0 && !self.base_ref_name.is_empty() {
            spans.push(Span::styled(
                format!(" ← {}", self.base_ref_name),
                Style::default().fg(Color::DarkGray),
            ));
        }

        ListItem::new(Line::from(spans))
    }

    fn number(&self) -> u64 {
        self.number
    }

    /// A stacked PR: its base branch is another listed PR's head branch.
    /// With several candidates (unusual), the lowest number wins.
    fn parent_number(&self, items: &[Self]) -> Option<u64> {
        if self.base_ref_name.is_empty() {
            return None;
        }
        items
            .iter()
            .filter(|p| p.number != self.number && p.head_ref_name == self.base_ref_name)
            .map(|p| p.number)
            .min()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn pr(number: u64, head: &str, base: &str) -> GhPrListItem {
        GhPrListItem {
            number,
            title: format!("pr {number}"),
            state: "OPEN".into(),
            author: None,
            labels: vec![],
            head_ref_name: head.into(),
            base_ref_name: base.into(),
            created_at: String::new(),
            review_decision: None,
            is_draft: false,
        }
    }

    #[test]
    fn stacked_pr_parent_is_the_pr_owning_its_base_branch() {
        let items = vec![
            pr(3, "feat/c", "feat/b"),
            pr(2, "feat/b", "feat/a"),
            pr(1, "feat/a", "main"),
            pr(9, "feat/a", "main"), // duplicate head: lowest number wins
        ];
        assert_eq!(items[0].parent_number(&items), Some(2));
        assert_eq!(items[1].parent_number(&items), Some(1));
        assert_eq!(items[2].parent_number(&items), None);
        // Caches predating baseRefName have an empty base → top-level.
        assert_eq!(pr(5, "x", "").parent_number(&items), None);
    }
}
