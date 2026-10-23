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
        // Merged / closed PRs (shown with `x`) are dimmed.
        let title_style = if self.state == "OPEN" {
            Style::default()
        } else {
            Style::default().fg(Color::DarkGray)
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
            Span::styled(self.title.clone(), title_style),
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

        // GitHub Stack: the bottom row names the stack, nested rows show
        // the branch they build on.
        if let Some(stack) = &self.stack {
            let note = if tree.depth == 0 {
                format!(" stack #{}", stack.number)
            } else {
                format!(" ← {}", self.base_ref_name)
            };
            spans.push(Span::styled(note, Style::default().fg(Color::DarkGray)));
        }

        ListItem::new(Line::from(spans))
    }

    fn number(&self) -> u64 {
        self.number
    }

    /// A PR in a GitHub Stack nests under the listed PR one position below
    /// it in the same stack (the closest lower position, so merged / closed
    /// entries missing from the open list are skipped over).
    fn parent_number(&self, items: &[Self]) -> Option<u64> {
        let mine = self.stack.as_ref()?;
        items
            .iter()
            .filter(|p| p.number != self.number)
            .filter_map(|p| {
                let s = p.stack.as_ref()?;
                (s.number == mine.number && s.position < mine.position)
                    .then_some((s.position, p.number))
            })
            .max_by_key(|(position, _)| *position)
            .map(|(_, number)| number)
    }

    fn search_text(&self) -> String {
        format!("#{} {}", self.number, self.title)
    }

    fn browser_event(&self) -> PaneEvent {
        PaneEvent::OpenPrBrowser(self.number)
    }

    fn copy_url(&self) -> Option<String> {
        client::pr_url(self.number)
    }

    fn diff_event(&self) -> Option<PaneEvent> {
        Some(PaneEvent::ShowPrDiff(self.number))
    }

    fn worktree_event(&self) -> Option<PaneEvent> {
        Some(PaneEvent::OpenPrWorktree(self.number))
    }

    fn load_disk_cache() -> Option<Vec<Self>> {
        disk_cache::load_pr_list()
    }

    fn save_disk_cache(items: &[Self]) {
        disk_cache::save_pr_list(items);
    }

    fn fetch_list(show_closed: bool) -> Result<Vec<Self>, String> {
        let mut prs = client::list_prs(50, show_closed)?;
        // Stack membership is best-effort: without it (older gh, API
        // without stacks) the list is simply flat.
        if let Ok(stacks) = client::list_pr_stacks(50) {
            for pr in &mut prs {
                pr.stack = stacks.get(&pr.number).cloned();
            }
        }
        Ok(prs)
    }

    fn wrap_bg_message(result: Result<Vec<Self>, String>, show_closed: bool) -> GhBgMessage {
        GhBgMessage::PrList {
            result,
            show_closed,
        }
    }
}

pub type GhPrListPane = GhListPane<GhPrListItem>;

pub fn new_pane(pane_id: usize, detail_id: usize, switch_target: usize) -> GhPrListPane {
    GhListPane::new(pane_id, detail_id, KeyCode::BackTab, switch_target)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::github::domain::types::GhPrStackRef;

    fn pr(number: u64, stack: Option<(u64, u32)>) -> GhPrListItem {
        GhPrListItem {
            number,
            title: format!("pr {number}"),
            state: "OPEN".into(),
            author: None,
            labels: vec![],
            head_ref_name: format!("feat/{number}"),
            base_ref_name: "main".into(),
            created_at: String::new(),
            review_decision: None,
            is_draft: false,
            stack: stack.map(|(number, position)| GhPrStackRef {
                number,
                position,
                size: 3,
            }),
        }
    }

    /// `d` on a PR row asks the page for that PR's diff window; the
    /// header labels come from the row (refs, title) or the detail.
    #[test]
    fn show_diff_names_the_selected_pr() {
        use crate::core::config::Config;
        use crate::core::pane::{Pane, PaneShared};
        use crate::core::search::SearchState;
        use crate::github::panes::gh_list::GhListAction;
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut pane = new_pane(1, 4, 0);
        pane.set_keymap(
            Config::builtin()
                .github_page()
                .unwrap()
                .keymap::<GhListAction>("pr_list")
                .unwrap(),
        );
        pane.set_items(vec![pr(10, None), pr(11, None)]);
        pane.selected_idx = 1;
        let shared = PaneShared {
            focused_pane: 1,
            previous_pane: 1,
            search: SearchState::new(),
        };
        let events = pane.handle_key(
            &shared,
            KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE),
        );
        assert!(matches!(events[..], [PaneEvent::ShowPrDiff(11)]));

        let src = crate::github::pr_diff::source_for(11, pane.items.get(1), None, None);
        assert_eq!(src.badge, "PR #11");
        assert_eq!(src.refs.as_deref(), Some("feat/11 → main"));
        assert_eq!(src.title, "pr 11");
        let bare = crate::github::pr_diff::source_for(12, None, None, None);
        assert_eq!(bare.refs, None);
        assert_eq!(bare.title, "");
    }

    #[test]
    fn stack_entries_nest_under_the_closest_lower_position() {
        let items = vec![
            pr(3, Some((7, 3))),
            pr(1, Some((7, 1))),
            pr(8, Some((9, 1))), // another stack
            pr(4, None),         // not stacked, even though its base could match
        ];
        // Position 2 of stack #7 was merged and is not listed: 3 skips to 1.
        assert_eq!(items[0].parent_number(&items), Some(1));
        assert_eq!(items[1].parent_number(&items), None);
        assert_eq!(items[2].parent_number(&items), None);
        assert_eq!(items[3].parent_number(&items), None);
    }
}
