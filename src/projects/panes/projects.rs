//! Project list pane: the owner's open projects, the ones linked to the
//! current repository first.

use crate::core::app::AppContext;
use crate::core::keymap::{
    nav_bindings, search_bindings, ActionHelp, Keymap, NavAction, SearchAction,
};
use crate::core::pane::{self, Pane, PaneEvent, PaneShared};
use crate::core::search::SearchMatch;
use crate::core::theme;
use crate::github::domain::actions::time::{format_relative, now_secs, parse_iso8601};
use crate::projects::domain::types::Project;
use crossterm::event::KeyCode;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::ListItem,
    Frame,
};

#[derive(Debug, Clone)]
pub enum ProjectsAction {
    Nav(NavAction),
    OpenBoard,
    OpenBrowser,
    Search(SearchAction),
    Esc,
}

crate::impl_pane_action_from_str!(
    ProjectsAction, nav: Nav, search: Search, esc: Esc,
    OpenBoard, OpenBrowser
);

impl ActionHelp for ProjectsAction {
    fn label(&self) -> Option<&'static str> {
        match self {
            ProjectsAction::Nav(nav) => nav.label(),
            ProjectsAction::OpenBoard => Some("Focus board"),
            ProjectsAction::OpenBrowser => Some("Open project in browser"),
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
        .key(KeyCode::Esc, ProjectsAction::Esc)
}

pub struct ProjectsPane {
    pub items: Vec<Project>,
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
            _ => vec![],
        }
    }

    /// Two lines per project: `▸ #2 vig demo board` (linked projects are
    /// marked and bold) and `8 items · 3d ago`.
    pub fn row_lines(p: &Project, now: i64) -> Vec<Line<'static>> {
        let dim = Style::default().fg(Color::Gray);
        let marker = if p.linked {
            Span::styled("▸ ", Style::default().fg(Color::Cyan))
        } else {
            Span::raw("  ")
        };
        let title_style = if p.linked {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let n = p.items.total_count;
        let updated = p
            .updated_at
            .as_deref()
            .and_then(parse_iso8601)
            .map(|t| format_relative(now - t))
            .unwrap_or_else(|| "-".to_string());
        vec![
            Line::from(vec![
                Span::raw(" "),
                marker,
                Span::styled(
                    format!("#{} ", p.number),
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled(p.title.clone(), title_style),
            ]),
            Line::from(Span::styled(
                format!("     {n} item{} · {updated}", if n == 1 { "" } else { "s" }),
                dim,
            )),
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
            Some("No projects")
        } else {
            None
        };
        // The selection is always shown: it is the project the board follows.
        let selected = (!self.items.is_empty()).then_some(self.selected_idx);
        let now = now_secs();
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
                        let mut li = ListItem::new(Self::row_lines(p, now));
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
    use crate::projects::domain::types::{order_projects, ProjectList};

    fn projects() -> Vec<Project> {
        let list: ProjectList = serde_json::from_str(
            r#"{"projects":[
              {"number":1,"title":"life","items":{"totalCount":6},"url":"https://github.com/users/td72/projects/1"},
              {"number":2,"title":"vig demo board","items":{"totalCount":8},"url":"https://github.com/users/td72/projects/2","updatedAt":"2026-08-30T08:58:09Z"}
            ],"totalCount":2}"#,
        )
        .unwrap();
        order_projects(list.projects, &[2])
    }

    fn shared(focus: usize) -> PaneShared {
        PaneShared {
            focused_pane: focus,
            previous_pane: focus,
            search: SearchState::new(),
        }
    }

    #[test]
    fn linked_projects_come_first_and_selection_survives_refresh() {
        let mut pane = ProjectsPane::new(0, 1);
        pane.set_projects(projects());
        assert_eq!(pane.selected_number(), Some(2), "linked project first");
        pane.selected_idx = 1;
        pane.set_projects(projects());
        assert_eq!(pane.selected_number(), Some(1));
        pane.set_projects(vec![]);
        assert!(pane.selected().is_none());
        assert_eq!(pane.selected_idx, 0);
    }

    #[test]
    fn rows_show_number_title_count_and_relative_update() {
        let ps = projects();
        let now = parse_iso8601("2026-08-30T10:58:09Z").unwrap();
        let text = |p: &Project| -> Vec<String> {
            ProjectsPane::row_lines(p, now)
                .iter()
                .map(|l| l.spans.iter().map(|s| s.content.to_string()).collect())
                .collect()
        };
        assert_eq!(
            text(&ps[0]),
            [" ▸ #2 vig demo board", "     8 items · 2h ago"]
        );
        assert_eq!(text(&ps[1]), ["   #1 life", "     6 items · -"]);
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
        assert_eq!(pane.collect_search_matches(&sh, "#1").len(), 1);
    }
}
