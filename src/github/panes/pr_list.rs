use crate::core::app::AppContext;
use crate::core::keymap::{execute_nav, nav_bindings, ActionHelp, Keymap, NavAction};
use crate::core::pane::{Pane, PaneEvent, PaneShared};
use crate::core::theme;
use crate::github::domain::types::GhPrListItem;
use crate::github::domain::{client, disk_cache};
use crate::github::state::{GhBgMessage, GH_PANE_ISSUE_LIST, GH_PANE_PR_DETAIL, GH_PANE_PR_LIST};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{List, ListItem, ListState},
    Frame,
};
use std::sync::mpsc;

#[derive(Debug, Clone)]
pub enum PrListAction {
    Nav(NavAction),
    OpenDetail,
    SwitchToIssueList,
    OpenBrowser,
}

impl ActionHelp for PrListAction {
    fn label(&self) -> Option<&'static str> {
        match self {
            PrListAction::Nav(nav) => nav.label(),
            PrListAction::OpenDetail => Some("Open detail"),
            PrListAction::SwitchToIssueList => Some("Switch to Issues"),
            PrListAction::OpenBrowser => Some("Open in browser"),
        }
    }
}

pub fn default_keymap() -> Keymap<PrListAction> {
    Keymap::new()
        .bindings(nav_bindings(PrListAction::Nav))
        .key(KeyCode::Char('i'), PrListAction::OpenDetail)
        .key(KeyCode::Enter, PrListAction::OpenDetail)
        .key(KeyCode::BackTab, PrListAction::SwitchToIssueList)
        .key(KeyCode::Char('o'), PrListAction::OpenBrowser)
}

pub struct GhPrListPane {
    pub prs: Vec<GhPrListItem>,
    pub selected_idx: usize,
    pub loading: bool,
    keymap: Keymap<PrListAction>,
}

impl GhPrListPane {
    pub fn new() -> Self {
        Self {
            prs: Vec::new(),
            selected_idx: 0,
            loading: false,
            keymap: default_keymap(),
        }
    }

    /// Load disk cache + spawn background fetch.
    pub fn initialize(&mut self, tx: &mpsc::Sender<GhBgMessage>) {
        if let Some(prs) = disk_cache::load_pr_list() {
            self.prs = prs;
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
        self.prs = prs;
    }

    /// Return the number of the currently selected PR, if any.
    pub fn selected_number(&self) -> Option<u64> {
        self.prs.get(self.selected_idx).map(|pr| pr.number)
    }

    pub fn handle_key(&mut self, _shared: &PaneShared, key: KeyEvent) -> Vec<PaneEvent> {
        let action = match self.keymap.lookup(key) {
            Some(a) => a.clone(),
            None => return vec![],
        };
        self.execute(action)
    }

    fn execute(&mut self, action: PrListAction) -> Vec<PaneEvent> {
        match action {
            PrListAction::Nav(nav) => {
                if execute_nav(nav, &mut self.selected_idx, self.prs.len(), None) {
                    return vec![PaneEvent::SelectionChanged];
                }
            }
            PrListAction::OpenDetail => {
                if !self.prs.is_empty() {
                    return vec![PaneEvent::SetFocus(GH_PANE_PR_DETAIL)];
                }
            }
            PrListAction::SwitchToIssueList => {
                return vec![PaneEvent::SetFocus(GH_PANE_ISSUE_LIST)];
            }
            PrListAction::OpenBrowser => {
                if let Some(pr) = self.prs.get(self.selected_idx) {
                    return vec![PaneEvent::OpenPrBrowser(pr.number)];
                }
            }
        }
        vec![]
    }

    pub fn render(&mut self, f: &mut Frame, _ctx: &AppContext, shared: &PaneShared, area: Rect) {
        let is_focused = shared.focused_pane == GH_PANE_PR_LIST;
        let block = theme::pane_block("Pull Requests", shared.focused_pane == GH_PANE_PR_LIST);

        if self.loading && self.prs.is_empty() {
            theme::render_empty_list(f, area, block, "Loading...");
            return;
        }

        if self.prs.is_empty() {
            theme::render_empty_list(f, area, block, "No pull requests");
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

        let highlight_style = theme::list_highlight_style(false);

        let list = List::new(items)
            .block(block)
            .highlight_style(highlight_style);

        let mut list_state = ListState::default();
        if is_focused
            || (shared.focused_pane == GH_PANE_PR_DETAIL && shared.previous_pane == GH_PANE_PR_LIST)
        {
            list_state.select(Some(self.selected_idx));
        }
        f.render_stateful_widget(list, area, &mut list_state);
    }
}

impl Pane<PaneEvent> for GhPrListPane {
    fn handle_key(&mut self, shared: &PaneShared, key: KeyEvent) -> Vec<PaneEvent> {
        self.handle_key(shared, key)
    }
    fn render(&mut self, f: &mut Frame, ctx: &AppContext, shared: &PaneShared, area: Rect) {
        self.render(f, ctx, shared, area)
    }
}
