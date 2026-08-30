//! Item detail pane: the selected card's project fields, then — for issues
//! and pull requests — the GitHub page's header, markdown body and comments
//! (fetched with `gh issue view` / `gh pr view`); drafts show their body.

use crate::core::app::AppContext;
use crate::core::keymap::{half_page_step, nav_bindings, ActionHelp, Keymap, NavAction};
use crate::core::pane::{Pane, PaneEvent, PaneShared};
use crate::core::theme;
use crate::github::domain::client as gh;
use crate::github::domain::types::{GhComment, GhIssueDetail, GhPrDetail};
use crate::github::panes::detail_view::view::{
    build_issue_header, build_pr_header, format_date, markdown_to_lines,
};
use crate::projects::domain::types::{ItemKind, ProjectField, ProjectItem};
use crate::projects::state::ProjectsBgMessage;
use crossterm::event::KeyCode;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
    Frame,
};
use std::collections::HashMap;
use std::sync::mpsc;

#[derive(Debug, Clone)]
pub enum DetailAction {
    Nav(NavAction),
    Back,
    OpenBrowser,
    Esc,
}

crate::impl_pane_action_from_str!(DetailAction, nav: Nav, Back, OpenBrowser, Esc);

impl ActionHelp for DetailAction {
    fn label(&self) -> Option<&'static str> {
        match self {
            DetailAction::Nav(NavAction::MoveDown) => Some("Scroll down"),
            DetailAction::Nav(NavAction::MoveUp) => Some("Scroll up"),
            DetailAction::Nav(nav) => nav.label(),
            DetailAction::Back => Some("Back to board"),
            DetailAction::OpenBrowser => Some("Open item in browser"),
            DetailAction::Esc => Some("Back to board"),
        }
    }
}

pub fn default_keymap() -> Keymap<DetailAction> {
    Keymap::new()
        .bindings(nav_bindings(DetailAction::Nav))
        .key(KeyCode::Char('h'), DetailAction::Back)
        .key(KeyCode::Left, DetailAction::Back)
        .key(KeyCode::Char('o'), DetailAction::OpenBrowser)
        .key(KeyCode::Esc, DetailAction::Esc)
}

/// The issue or PR behind a card, as the GitHub page fetches it.
#[derive(Debug, Clone)]
pub enum ItemDetail {
    Issue(Box<GhIssueDetail>),
    Pr(Box<GhPrDetail>),
}

impl ItemDetail {
    fn body(&self) -> &str {
        match self {
            Self::Issue(d) => &d.body,
            Self::Pr(d) => &d.body,
        }
    }

    fn comments(&self) -> &[GhComment] {
        match self {
            Self::Issue(d) => &d.comments,
            Self::Pr(d) => &d.comments,
        }
    }

    fn header(&self) -> Vec<Line<'static>> {
        match self {
            Self::Issue(d) => build_issue_header(d),
            Self::Pr(d) => build_pr_header(d),
        }
    }
}

/// Fetch state of the issue / PR behind the shown item.
#[derive(Debug, Clone, PartialEq, Eq)]
enum FetchState {
    /// Drafts and redacted items: nothing to fetch.
    NotNeeded,
    Loading,
    Ready,
    Error(String),
}

/// `owner/repo#number`: cache key and message correlation id.
fn detail_key(item: &ProjectItem) -> Option<String> {
    Some(format!("{}#{}", item.repository()?, item.number()?))
}

pub struct DetailPane {
    pane_id: usize,
    keymap: Keymap<DetailAction>,
    item: Option<ProjectItem>,
    fields: Vec<ProjectField>,
    detail: Option<ItemDetail>,
    state: FetchState,
    cache: HashMap<String, ItemDetail>,
    scroll: usize,
    view_height: u16,
    line_count: usize,
}

impl DetailPane {
    pub fn new(pane_id: usize) -> Self {
        Self {
            pane_id,
            keymap: default_keymap(),
            item: None,
            fields: Vec::new(),
            detail: None,
            state: FetchState::NotNeeded,
            cache: HashMap::new(),
            scroll: 0,
            view_height: 20,
            line_count: 0,
        }
    }

    pub fn set_keymap(&mut self, km: Keymap<DetailAction>) {
        self.keymap = km;
    }

    pub fn keymap(&self) -> &Keymap<DetailAction> {
        &self.keymap
    }

    #[cfg(test)]
    pub fn item(&self) -> Option<&ProjectItem> {
        self.item.as_ref()
    }

    pub fn is_loading(&self) -> bool {
        self.state == FetchState::Loading
    }

    pub fn show_none(&mut self) {
        self.item = None;
        self.detail = None;
        self.state = FetchState::NotNeeded;
        self.scroll = 0;
    }

    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    /// Show `item`: its fields at once, the issue / PR from the cache or a
    /// background fetch. Re-selecting the shown item is a no-op.
    pub fn load(
        &mut self,
        item: &ProjectItem,
        fields: &[ProjectField],
        tx: &mpsc::Sender<ProjectsBgMessage>,
    ) {
        if self.item.as_ref().is_some_and(|i| i.id == item.id) {
            self.fields = fields.to_vec();
            return;
        }
        self.item = Some(item.clone());
        self.fields = fields.to_vec();
        self.detail = None;
        self.scroll = 0;
        let key = match (item.kind(), detail_key(item)) {
            (ItemKind::Issue | ItemKind::PullRequest, Some(key)) => key,
            _ => {
                self.state = FetchState::NotNeeded;
                return;
            }
        };
        if let Some(cached) = self.cache.get(&key) {
            self.detail = Some(cached.clone());
            self.state = FetchState::Ready;
            return;
        }
        self.state = FetchState::Loading;
        self.spawn_fetch(item, key, tx);
    }

    /// Re-fetch the shown issue / PR (`r`), keeping what is on screen.
    pub fn reload_current(&mut self, tx: &mpsc::Sender<ProjectsBgMessage>) {
        let Some(item) = self.item.clone() else {
            return;
        };
        let Some(key) = detail_key(&item) else {
            return;
        };
        if !matches!(item.kind(), ItemKind::Issue | ItemKind::PullRequest) {
            return;
        }
        self.cache.remove(&key);
        if self.detail.is_none() {
            self.state = FetchState::Loading;
        }
        self.spawn_fetch(&item, key, tx);
    }

    fn spawn_fetch(&self, item: &ProjectItem, key: String, tx: &mpsc::Sender<ProjectsBgMessage>) {
        let tx = tx.clone();
        let kind = item.kind();
        let repo = item.repository().map(str::to_string);
        let Some(number) = item.number() else {
            return;
        };
        std::thread::spawn(move || {
            let result = match kind {
                ItemKind::PullRequest => {
                    gh::get_pr_in(repo.as_deref(), number).map(|d| ItemDetail::Pr(Box::new(d)))
                }
                _ => gh::get_issue_in(repo.as_deref(), number)
                    .map(|d| ItemDetail::Issue(Box::new(d))),
            };
            let _ = tx.send(ProjectsBgMessage::ItemDetail { key, result });
        });
    }

    /// Apply a fetch result; a result for another item only warms the cache.
    pub fn apply(&mut self, key: &str, result: Result<ItemDetail, String>) {
        let current = self.item.as_ref().and_then(detail_key);
        match result {
            Ok(detail) => {
                self.cache.insert(key.to_string(), detail.clone());
                if current.as_deref() == Some(key) {
                    self.detail = Some(detail);
                    self.state = FetchState::Ready;
                }
            }
            Err(e) => {
                if current.as_deref() == Some(key) && self.detail.is_none() {
                    self.state = FetchState::Error(e);
                }
            }
        }
    }

    fn execute(&mut self, shared: &PaneShared, action: DetailAction) -> Vec<PaneEvent> {
        match action {
            DetailAction::Nav(nav) => {
                let max = self.line_count.saturating_sub(self.view_height as usize);
                let half = half_page_step(self.view_height) as usize;
                self.scroll = match nav {
                    NavAction::MoveDown => self.scroll + 1,
                    NavAction::MoveUp => self.scroll.saturating_sub(1),
                    NavAction::HalfPageDown => self.scroll + half,
                    NavAction::HalfPageUp => self.scroll.saturating_sub(half),
                    NavAction::JumpTop => 0,
                    NavAction::JumpBottom => max,
                }
                .min(max);
                vec![]
            }
            DetailAction::OpenBrowser => match self.item.as_ref().and_then(ProjectItem::url) {
                Some(url) => vec![PaneEvent::OpenUrl(url.to_string())],
                None => vec![],
            },
            DetailAction::Back | DetailAction::Esc => {
                vec![PaneEvent::SetFocus(shared.previous_pane)]
            }
        }
    }

    pub fn lines(&self, width: usize) -> Vec<Line<'static>> {
        let dim = Style::default().fg(Color::DarkGray);
        let Some(item) = &self.item else {
            return vec![Line::from(Span::styled("  Select an item", dim))];
        };
        let kind = item.kind();
        let mut lines: Vec<Line<'static>> = match &self.detail {
            Some(d) => d.header(),
            None => item_header(item),
        };
        lines.push(Line::default());

        lines.push(section("Fields"));
        let values = item.field_values(&self.fields);
        if values.is_empty() {
            lines.push(Line::from(Span::styled("  (no fields)", dim)));
        }
        for (name, value) in values {
            lines.push(kv(&name, value));
        }
        lines.push(Line::default());

        match (&self.state, &self.detail) {
            (FetchState::Loading, None) => {
                lines.push(Line::from(Span::styled(
                    format!("  Loading {}...", kind.label()),
                    dim,
                )));
            }
            (FetchState::Error(e), None) => {
                lines.push(Line::from(Span::styled(
                    format!("  Error: {e}"),
                    Style::default().fg(Color::Red),
                )));
            }
            (_, detail) => {
                let body = detail
                    .as_ref()
                    .map(ItemDetail::body)
                    .unwrap_or_else(|| item.body());
                lines.push(section("Body"));
                if body.trim().is_empty() {
                    lines.push(Line::from(Span::styled("  (no description)", dim)));
                } else {
                    lines.extend(markdown_to_lines(body, "  ", width));
                }
                if let Some(detail) = detail {
                    let comments = detail.comments();
                    lines.push(Line::default());
                    lines.push(section(&format!("Comments ({})", comments.len())));
                    for c in comments {
                        let author = c
                            .author
                            .as_ref()
                            .map(|a| a.login.as_str())
                            .unwrap_or("unknown");
                        lines.push(Line::from(vec![
                            Span::raw("  "),
                            Span::styled(author.to_string(), Style::default().fg(Color::Cyan)),
                            Span::styled(format!("  {}", format_date(&c.created_at)), dim),
                        ]));
                        lines.extend(markdown_to_lines(&c.body, "    ", width));
                        lines.push(Line::default());
                    }
                }
            }
        }
        lines
    }
}

/// Header for items without a fetched issue / PR: title, kind badge,
/// repository.
fn item_header(item: &ProjectItem) -> Vec<Line<'static>> {
    let kind = item.kind();
    let number = item.number().map(|n| format!("#{n} ")).unwrap_or_default();
    let title = Line::from(vec![
        Span::raw("  "),
        Span::styled(
            format!("{number}{}", item.title()),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    let badge_bg = match kind {
        ItemKind::Issue => Color::Rgb(35, 134, 54),
        ItemKind::PullRequest => Color::Rgb(130, 80, 160),
        ItemKind::Draft | ItemKind::Other => Color::Rgb(110, 119, 129),
    };
    let mut spans = vec![Span::raw(" "), badge(kind.label(), badge_bg)];
    if let Some(repo) = item.repository() {
        spans.push(Span::raw(" "));
        spans.push(badge(repo, Color::Rgb(68, 71, 78)));
    }
    vec![title, Line::from(spans)]
}

fn badge(text: &str, bg: Color) -> Span<'static> {
    Span::styled(
        format!(" {text} "),
        Style::default().fg(Color::White).bg(bg),
    )
}

const KEY_WIDTH: usize = 14;

fn section(title: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!(" {title}"),
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ))
}

fn kv(key: &str, value: impl Into<String>) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("  {key:<KEY_WIDTH$} "),
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw(value.into()),
    ])
}

impl Pane<PaneEvent> for DetailPane {
    crate::impl_handle_key!(keymap);

    fn render(&mut self, f: &mut Frame, _ctx: &AppContext, shared: &PaneShared, area: Rect) {
        self.view_height = area.height.saturating_sub(2);
        let lines = self.lines(area.width.saturating_sub(2) as usize);
        self.line_count = lines.len();
        self.scroll = self
            .scroll
            .min(self.line_count.saturating_sub(self.view_height as usize));
        let block = theme::pane_block("Detail", shared.focused_pane == self.pane_id);
        let para = Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((self.scroll as u16, 0));
        f.render_widget(para, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projects::domain::types::tests::board;

    fn text(lines: &[Line<'static>]) -> String {
        lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn issue(number: u64) -> GhIssueDetail {
        GhIssueDetail {
            number,
            title: "Config: explicit page slots".into(),
            state: "CLOSED".into(),
            author: None,
            body: "## Problem\n\nTab order is fixed.".into(),
            comments: vec![GhComment {
                author: None,
                body: "Looks good".into(),
                created_at: "2026-08-27T10:00:00Z".into(),
                url: None,
            }],
            labels: vec![],
            created_at: "2026-08-26T10:00:00Z".into(),
        }
    }

    #[test]
    fn drafts_show_fields_and_body_without_a_fetch() {
        let (tx, rx) = mpsc::channel();
        let b = board();
        let mut pane = DetailPane::new(2);
        pane.load(&b.items[2], &b.fields, &tx);
        assert!(!pane.is_loading());
        assert!(rx.try_recv().is_err(), "no fetch for a draft");
        let t = text(&pane.lines(60));
        assert!(t.contains("Record the Projects demo tape"));
        assert!(t.contains(" draft "));
        assert!(t.contains(" Fields"));
        assert!(t.contains("(no fields)"));
        assert!(t.contains("Draft item used by the vig demo."));
        assert!(!t.contains("Comments"));
        assert!(pane
            .execute(&shared(), DetailAction::OpenBrowser)
            .is_empty());
    }

    fn shared() -> PaneShared {
        PaneShared {
            focused_pane: 2,
            previous_pane: 1,
            search: crate::core::search::SearchState::new(),
        }
    }

    #[test]
    fn issues_list_every_field_then_the_fetched_body_and_comments() {
        let (tx, _rx) = mpsc::channel();
        let b = board();
        let mut pane = DetailPane::new(2);
        pane.load(&b.items[0], &b.fields, &tx);
        assert!(pane.is_loading());
        let t = text(&pane.lines(60));
        assert!(t.contains("#114 Config: explicit page slots"));
        assert!(t.contains("Status         Done"));
        assert!(t.contains("Priority       P1"));
        assert!(t.contains("Estimate       3"));
        assert!(t.contains("Labels         enhancement"));
        assert!(t.contains("Loading issue..."));
        // A result for another item only warms the cache.
        pane.apply("td72/vig#999", Ok(ItemDetail::Issue(Box::new(issue(999)))));
        assert!(pane.is_loading());
        pane.apply("td72/vig#114", Ok(ItemDetail::Issue(Box::new(issue(114)))));
        assert!(!pane.is_loading());
        let t = text(&pane.lines(60));
        assert!(t.contains(" CLOSED "));
        assert!(t.contains("Problem"));
        assert!(t.contains("Tab order is fixed."));
        assert!(t.contains("Comments (1)"));
        assert!(t.contains("Looks good"));
        // Back to the same item from the cache, no new fetch.
        let (tx2, rx2) = mpsc::channel();
        pane.load(&b.items[2], &b.fields, &tx2);
        pane.load(&b.items[0], &b.fields, &tx2);
        assert!(rx2.try_recv().is_err());
        assert!(!pane.is_loading());
        // `o` opens the issue; Esc / h go back to the board.
        let ev = pane.execute(&shared(), DetailAction::OpenBrowser);
        assert!(matches!(ev.as_slice(), [PaneEvent::OpenUrl(u)] if u.ends_with("/issues/114")));
        let ev = pane.execute(&shared(), DetailAction::Back);
        assert!(matches!(ev.as_slice(), [PaneEvent::SetFocus(1)]));
    }

    #[test]
    fn a_failed_fetch_keeps_the_fields_and_shows_the_error() {
        let (tx, _rx) = mpsc::channel();
        let b = board();
        let mut pane = DetailPane::new(2);
        pane.load(&b.items[1], &b.fields, &tx);
        pane.apply("td72/vig#124", Err("boom".into()));
        let t = text(&pane.lines(60));
        assert!(t.contains("#124 Fold the Actions page"));
        assert!(t.contains("Iteration      Sprint 3 (2026-08-24)"));
        assert!(t.contains("Error: boom"));
        // A redacted item (no content) needs no fetch.
        pane.load(&b.items[4], &b.fields, &tx);
        assert!(!pane.is_loading());
        assert!(text(&pane.lines(60)).contains("Status         Blocked"));
        pane.show_none();
        assert!(text(&pane.lines(60)).contains("Select an item"));
    }
}
