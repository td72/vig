use crate::core::pane::PaneEvent;
use crate::github::domain::types::GhPrListItem;
use crate::github::domain::{client, disk_cache};
use crate::github::panes::gh_list::{GhListItem, GhListPane};
use crate::github::state::{GhBgMessage, GH_PANE_ISSUE_LIST, GH_PANE_PR_DETAIL, GH_PANE_PR_LIST};
use crossterm::event::KeyCode;
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
    widgets::ListItem,
};
use std::sync::mpsc;

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
}

pub type GhPrListPane = GhListPane<GhPrListItem>;

pub fn new_pane() -> GhPrListPane {
    GhListPane::new(
        GH_PANE_PR_LIST,
        GH_PANE_PR_DETAIL,
        KeyCode::BackTab,
        GH_PANE_ISSUE_LIST,
    )
}

impl GhPrListPane {
    /// Load disk cache + spawn background fetch.
    pub fn initialize(&mut self, tx: &mpsc::Sender<GhBgMessage>) {
        if let Some(prs) = disk_cache::load_pr_list() {
            self.items = prs;
        }
        self.loading = true;
        self.spawn_fetch(tx);
    }

    /// Spawn background fetch thread.
    pub fn spawn_fetch(&mut self, tx: &mpsc::Sender<GhBgMessage>) {
        self.loading = true;
        let tx = tx.clone();
        std::thread::spawn(move || {
            let prs = client::list_prs(50);
            let _ = tx.send(GhBgMessage::PrList(prs));
        });
    }

    /// Apply a freshly fetched list — save to disk cache and update state.
    pub fn apply_list(&mut self, prs: Vec<GhPrListItem>) {
        disk_cache::save_pr_list(&prs);
        self.items = prs;
    }
}
