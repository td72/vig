//! Project list pane: the projects linked to the current repository. The
//! built-in layout does not place it (the board fills the page and `p` /
//! `P` switch projects); a user layout that places it gets a selectable
//! list whose selection drives the board.

use crate::core::app::AppContext;
use crate::core::keymap::{
    nav_bindings, search_bindings, ActionHelp, Keymap, NavAction, SearchAction,
};
use crate::core::pane::{self, Pane, PaneEvent, PaneShared};
use crate::core::search::SearchMatch;
use crate::core::theme;
use crate::projects::domain::types::Project;
use crossterm::event::KeyCode;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::ListItem,
    Frame,
};

#[derive(Debug, Clone)]
pub enum ProjectsAction {
    Nav(NavAction),
    OpenBoard,
    OpenBrowser,
    CopyUrl,
    Search(SearchAction),
    Esc,
}

crate::impl_pane_action_from_str!(
    ProjectsAction, nav: Nav, search: Search, esc: Esc,
    OpenBoard, OpenBrowser, CopyUrl
);

impl ActionHelp for ProjectsAction {
    fn label(&self) -> Option<&'static str> {
        match self {
            ProjectsAction::Nav(nav) => nav.label(),
            ProjectsAction::OpenBoard => Some("Focus board"),
            ProjectsAction::OpenBrowser => Some("Open project in browser"),
            ProjectsAction::CopyUrl => Some("Copy project URL"),
            ProjectsAction::Search(sa) => sa.label(),
            ProjectsAction::Esc => Some("Clear search"),
        }
    }
}

pub fn default_keymap() -> Keymap<ProjectsAction> {
    Keymap::new()
        .bindings(nav_bindings(ProjectsAction::Nav))
        .bindings(search_bindings(ProjectsAction::Search))
        .key(KeyCode::Enter, ProjectsAction::OpenBoard)
        .key(KeyCode::Char('i'), ProjectsAction::OpenBoard)
        .key(KeyCode::Char('l'), ProjectsAction::OpenBoard)
        .key(KeyCode::Char('o'), ProjectsAction::OpenBrowser)
        .key(KeyCode::Char('y'), ProjectsAction::CopyUrl)
        .key(KeyCode::Esc, ProjectsAction::Esc)
}

pub struct ProjectsPane {
    /// The linked projects, in GitHub's order.
    pub items: Vec<Project>,
    /// The current project: the one the board shows.
    pub selected_idx: usize,
    loading: bool,
    keymap: Keymap<ProjectsAction>,
    pane_id: usize,
    board_pane_id: usize,
    view_height: u16,
}

impl ProjectsPane {
    pub fn new(pane_id: usize, board_pane_id: usize) -> Self {
        Self {
            items: Vec::new(),
            selected_idx: 0,
            loading: false,
            keymap: default_keymap(),
            pane_id,
            board_pane_id,
            view_height: 20,
        }
    }

    pub fn set_keymap(&mut self, km: Keymap<ProjectsAction>) {
        self.keymap = km;
    }

    pub fn keymap(&self) -> &Keymap<ProjectsAction> {
        &self.keymap
    }

    pub fn set_loading(&mut self, loading: bool) {
        self.loading = loading;
    }

    pub fn is_loading(&self) -> bool {
        self.loading
    }

    pub fn selected(&self) -> Option<&Project> {
        self.items.get(self.selected_idx)
    }

    pub fn selected_number(&self) -> Option<u64> {
        self.selected().map(|p| p.number)
    }

    /// Replace the list, keeping the selection on the same project.
    pub fn set_projects(&mut self, projects: Vec<Project>) {
        let keep = self.selected_number();
        self.items = projects;
        self.selected_idx = keep
            .and_then(|n| self.items.iter().position(|p| p.number == n))
            .unwrap_or(self.selected_idx)
            .min(self.items.len().saturating_sub(1));
    }

    /// A fetched board tells how many items project `number` has.
    pub fn set_item_count(&mut self, number: u64, count: u64) {
        if let Some(p) = self.items.iter_mut().find(|p| p.number == number) {
            p.items.total_count = count;
        }
    }

    fn execute(&mut self, shared: &PaneShared, action: ProjectsAction) -> Vec<PaneEvent> {
        if let Some(events) = pane::try_dispatch_search_esc(&action, shared, self.pane_id, vec![]) {
            return events;
        }
        match action {
            ProjectsAction::Nav(nav) => pane::execute_list_nav(
                nav,
                &mut self.selected_idx,
                self.items.len(),
                Some(self.view_height),
            ),
            ProjectsAction::OpenBoard if !self.items.is_empty() => {
                vec![PaneEvent::SetFocus(self.board_pane_id)]
            }
            ProjectsAction::OpenBrowser => match self.selected().filter(|p| !p.url.is_empty()) {
                Some(p) => vec![PaneEvent::OpenUrl(p.url.clone())],
                None => vec![],
            },
            ProjectsAction::CopyUrl => vec![crate::github::panes::gh_list::copy_url_event(
                self.selected().map(|p| p.url.clone()),
            )],
            _ => vec![],
        }
    }

    /// Two lines per project: `#2 vig demo board` and `td72 · 8 items`
    /// (the count shows once its board has been fetched).
    pub fn row_lines(p: &Project) -> Vec<Line<'static>> {
        let dim = Style::default().fg(Color::Gray);
        let n = p.items.total_count;
        let mut sub = p.owner.login.clone();
        if n > 0 {
            if !sub.is_empty() {
                sub.push_str(" · ");
            }
            sub.push_str(&format!("{n} item{}", if n == 1 { "" } else { "s" }));
        }
        vec![
            Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    format!("#{} ", p.number),
                    Style::default().fg(Color::Yellow),
                ),
                Span::raw(p.title.clone()),
            ]),
            Line::from(Span::styled(format!("   {sub}"), dim)),
        ]
    }
}

impl Pane<PaneEvent> for ProjectsPane {
    crate::impl_handle_key!(keymap);

    fn render(&mut self, f: &mut Frame, _ctx: &AppContext, shared: &PaneShared, area: Rect) {
        // Two lines per row.
        self.view_height = area.height.saturating_sub(2) / 2;
        let empty = if self.loading && self.items.is_empty() {
            Some("Loading...")
        } else if self.items.is_empty() {
            Some("No linked projects")
        } else {
            None
        };
        // The selection is always shown: it is the project the board follows.
        let selected = (!self.items.is_empty()).then_some(self.selected_idx);
        theme::render_list_pane(
            f,
            area,
            shared,
            self.pane_id,
            "Projects",
            selected,
            empty,
            |match_set, current_match_idx| {
                self.items
                    .iter()
                    .enumerate()
                    .map(|(idx, p)| {
                        let mut li = ListItem::new(Self::row_lines(p));
                        let hl = theme::search_highlight_for(match_set, current_match_idx, idx);
                        if hl.is_active() {
                            li = li.style(hl.apply(Style::default()));
                        }
                        li
                    })
                    .collect()
            },
        );
    }

    fn collect_search_matches(&self, _shared: &PaneShared, query: &str) -> Vec<SearchMatch> {
        pane::collect_list_search_matches(&self.items, query, |p| {
            format!("#{} {}", p.number, p.title)
        })
    }

    crate::impl_list_pane_selection!();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::search::SearchState;
    use crate::projects::domain::types::tests::repo_info;

    fn projects() -> Vec<Project> {
        repo_info().linked_projects()
    }

    fn shared(focus: usize) -> PaneShared {
        PaneShared {
            focused_pane: focus,
            previous_pane: focus,
            search: SearchState::new(),
        }
    }

    #[test]
    fn selection_survives_refresh_and_counts_come_from_boards() {
        let mut pane = ProjectsPane::new(0, 1);
        pane.set_projects(projects());
        assert_eq!(pane.selected_number(), Some(2), "GitHub's order");
        pane.selected_idx = 1;
        pane.set_projects(projects());
        assert_eq!(pane.selected_number(), Some(7));
        pane.set_item_count(7, 12);
        assert_eq!(pane.items[1].items.total_count, 12);
        pane.set_item_count(99, 1);
        pane.set_projects(vec![]);
        assert!(pane.selected().is_none());
        assert_eq!(pane.selected_idx, 0);
    }

    #[test]
    fn rows_show_number_title_owner_and_count() {
        let mut ps = projects();
        ps[0].items.total_count = 8;
        let text = |p: &Project| -> Vec<String> {
            ProjectsPane::row_lines(p)
                .iter()
                .map(|l| l.spans.iter().map(|s| s.content.to_string()).collect())
                .collect()
        };
        assert_eq!(text(&ps[0]), [" #2 vig demo board", "   td72 · 8 items"]);
        assert_eq!(text(&ps[1]), [" #7 Roadmap", "   acme"]);
    }

    #[test]
    fn enter_focuses_the_board_and_o_opens_the_project() {
        let mut pane = ProjectsPane::new(0, 1);
        let sh = shared(0);
        assert!(pane.execute(&sh, ProjectsAction::OpenBoard).is_empty());
        pane.set_projects(projects());
        let ev = pane.execute(&sh, ProjectsAction::OpenBoard);
        assert!(matches!(ev.as_slice(), [PaneEvent::SetFocus(1)]));
        let ev = pane.execute(&sh, ProjectsAction::OpenBrowser);
        assert!(matches!(ev.as_slice(), [PaneEvent::OpenUrl(u)] if u.ends_with("/projects/2")));
        let ev = pane.execute(&sh, ProjectsAction::Nav(NavAction::MoveDown));
        assert!(matches!(ev.as_slice(), [PaneEvent::SelectionChanged]));
        assert_eq!(pane.collect_search_matches(&sh, "demo").len(), 1);
        assert_eq!(pane.collect_search_matches(&sh, "#7").len(), 1);
    }
}
