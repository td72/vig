//! Containers pane: `docker ps -a`, nested under synthetic compose-project
//! rows, running containers first.

use crate::core::app::AppContext;
use crate::core::keymap::{
    nav_bindings, search_bindings, ActionHelp, Keymap, NavAction, SearchAction,
};
use crate::core::pane::{self, Pane, PaneEvent, PaneShared};
use crate::core::search::SearchMatch;
use crate::core::theme;
use crate::core::tree::{nest_by, TreePos};
use crate::docker::domain::types::{Container, ContainerState};
use crossterm::event::KeyCode;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::ListItem,
    Frame,
};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum ContainersAction {
    Nav(NavAction),
    OpenDetail,
    FocusLogs,
    Search(SearchAction),
    Esc,
}

crate::impl_pane_action_from_str!(
    ContainersAction, nav: Nav, search: Search, esc: Esc,
    OpenDetail, FocusLogs
);

impl ActionHelp for ContainersAction {
    fn label(&self) -> Option<&'static str> {
        match self {
            ContainersAction::Nav(nav) => nav.label(),
            ContainersAction::OpenDetail => Some("Focus detail"),
            ContainersAction::FocusLogs => Some("Focus logs"),
            ContainersAction::Search(sa) => sa.label(),
            ContainersAction::Esc => Some("Clear search"),
        }
    }
}

pub fn default_keymap() -> Keymap<ContainersAction> {
    Keymap::new()
        .bindings(nav_bindings(ContainersAction::Nav))
        .bindings(search_bindings(ContainersAction::Search))
        .key(KeyCode::Char('i'), ContainersAction::OpenDetail)
        .key(KeyCode::Enter, ContainersAction::OpenDetail)
        .key(KeyCode::Char('l'), ContainersAction::FocusLogs)
        .key(KeyCode::Esc, ContainersAction::Esc)
}

/// One list row: a compose project header or a container.
#[derive(Debug, Clone)]
pub enum ContainerRow {
    Project {
        name: String,
        running: usize,
        total: usize,
    },
    Container(Container),
}

impl ContainerRow {
    /// Stable identity used to keep the selection across refreshes.
    pub fn key(&self) -> String {
        match self {
            ContainerRow::Project { name, .. } => format!("p:{name}"),
            ContainerRow::Container(c) => format!("c:{}", c.id),
        }
    }

    pub fn container(&self) -> Option<&Container> {
        match self {
            ContainerRow::Container(c) => Some(c),
            ContainerRow::Project { .. } => None,
        }
    }

    fn search_text(&self) -> String {
        match self {
            ContainerRow::Project { name, .. } => name.clone(),
            ContainerRow::Container(c) => format!("{} {}", c.name, c.image),
        }
    }
}

/// Sort containers running-first then by name, insert a project row before
/// the first member of each compose project, and nest members under it.
pub fn build_rows(mut containers: Vec<Container>) -> (Vec<ContainerRow>, Vec<TreePos>) {
    containers.sort_by(|a, b| {
        a.state_kind()
            .sort_rank()
            .cmp(&b.state_kind().sort_rank())
            .then_with(|| a.name.cmp(&b.name))
    });
    let mut rows: Vec<ContainerRow> = Vec::with_capacity(containers.len());
    let mut project_row: HashMap<String, usize> = HashMap::new();
    for c in containers {
        if let Some(project) = c.compose_project() {
            let idx = *project_row.entry(project.to_string()).or_insert_with(|| {
                rows.push(ContainerRow::Project {
                    name: project.to_string(),
                    running: 0,
                    total: 0,
                });
                rows.len() - 1
            });
            if let ContainerRow::Project { running, total, .. } = &mut rows[idx] {
                *total += 1;
                if c.state_kind() == ContainerState::Running {
                    *running += 1;
                }
            }
        }
        rows.push(ContainerRow::Container(c));
    }
    let order = nest_by(
        rows.len(),
        |i| i as u64,
        |i| match &rows[i] {
            ContainerRow::Container(c) => c
                .compose_project()
                .and_then(|p| project_row.get(p))
                .map(|&idx| idx as u64),
            ContainerRow::Project { .. } => None,
        },
    );
    let mut slots: Vec<Option<ContainerRow>> = rows.into_iter().map(Some).collect();
    let mut out = Vec::with_capacity(slots.len());
    let mut positions = Vec::with_capacity(slots.len());
    for (i, pos) in order {
        if let Some(row) = slots[i].take() {
            out.push(row);
            positions.push(pos);
        }
    }
    (out, positions)
}

pub struct ContainersPane {
    pub rows: Vec<ContainerRow>,
    positions: Vec<TreePos>,
    pub selected_idx: usize,
    loading: bool,
    keymap: Keymap<ContainersAction>,
    pane_id: usize,
    detail_pane_id: usize,
    logs_pane_id: usize,
    view_height: u16,
}

impl ContainersPane {
    pub fn new(pane_id: usize, detail_pane_id: usize, logs_pane_id: usize) -> Self {
        Self {
            rows: Vec::new(),
            positions: Vec::new(),
            selected_idx: 0,
            loading: false,
            keymap: default_keymap(),
            pane_id,
            detail_pane_id,
            logs_pane_id,
            view_height: 20,
        }
    }

    pub fn set_keymap(&mut self, km: Keymap<ContainersAction>) {
        self.keymap = km;
    }

    pub fn keymap(&self) -> &Keymap<ContainersAction> {
        &self.keymap
    }

    pub fn set_loading(&mut self, loading: bool) {
        self.loading = loading;
    }

    pub fn is_loading(&self) -> bool {
        self.loading
    }

    pub fn selected(&self) -> Option<&ContainerRow> {
        self.rows.get(self.selected_idx)
    }

    pub fn selected_container(&self) -> Option<&Container> {
        self.selected().and_then(ContainerRow::container)
    }

    /// Number of containers (project rows excluded) and how many run.
    pub fn counts(&self) -> (usize, usize) {
        let containers = self.rows.iter().filter_map(ContainerRow::container);
        let total = containers.clone().count();
        let running = containers
            .filter(|c| c.state_kind() == ContainerState::Running)
            .count();
        (total, running)
    }

    /// Members of the compose project `name` (name, state, status), for the
    /// detail pane's project summary.
    pub fn project_members(&self, name: &str) -> Vec<&Container> {
        self.rows
            .iter()
            .filter_map(ContainerRow::container)
            .filter(|c| c.compose_project() == Some(name))
            .collect()
    }

    /// Replace the list, keeping the selection on the same row when possible.
    pub fn set_containers(&mut self, containers: Vec<Container>) {
        let keep = self.selected().map(ContainerRow::key);
        let (rows, positions) = build_rows(containers);
        self.rows = rows;
        self.positions = positions;
        self.selected_idx = keep
            .and_then(|k| self.rows.iter().position(|r| r.key() == k))
            .unwrap_or(self.selected_idx)
            .min(self.rows.len().saturating_sub(1));
    }

    fn execute(&mut self, shared: &PaneShared, action: ContainersAction) -> Vec<PaneEvent> {
        if let Some(events) = pane::try_dispatch_search_esc(&action, shared, self.pane_id, vec![]) {
            return events;
        }
        match action {
            ContainersAction::Nav(nav) => pane::execute_list_nav(
                nav,
                &mut self.selected_idx,
                self.rows.len(),
                Some(self.view_height),
            ),
            ContainersAction::OpenDetail if !self.rows.is_empty() => {
                vec![PaneEvent::SetFocus(self.detail_pane_id)]
            }
            ContainersAction::FocusLogs if self.selected_container().is_some() => {
                vec![PaneEvent::SetFocus(self.logs_pane_id)]
            }
            _ => vec![],
        }
    }

    fn render_row(row: &ContainerRow, tree: &TreePos) -> ListItem<'static> {
        let dim = Style::default().fg(Color::DarkGray);
        let mut spans = vec![Span::raw(" "), Span::styled(tree.prefix.clone(), dim)];
        match row {
            ContainerRow::Project {
                name,
                running,
                total,
            } => {
                spans.push(Span::styled("▣", Style::default().fg(Color::Cyan)));
                spans.push(Span::raw(" "));
                spans.push(Span::styled(
                    name.clone(),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ));
                spans.push(Span::styled(format!("  {running}/{total} running"), dim));
            }
            ContainerRow::Container(c) => {
                let state = c.state_kind();
                let icon_color = match state {
                    ContainerState::Running => Color::Green,
                    ContainerState::Restarting | ContainerState::Paused => Color::Yellow,
                    ContainerState::Exited | ContainerState::Dead => Color::Red,
                    ContainerState::Created | ContainerState::Other => Color::DarkGray,
                };
                let name_style = if state == ContainerState::Running {
                    Style::default().add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                spans.push(Span::styled(state.icon(), Style::default().fg(icon_color)));
                spans.push(Span::raw(" "));
                spans.push(Span::styled(c.name.clone(), name_style));
                spans.push(Span::raw("  "));
                spans.push(Span::styled(
                    c.image.clone(),
                    Style::default().fg(Color::Blue),
                ));
                if !c.ports.is_empty() {
                    spans.push(Span::raw("  "));
                    spans.push(Span::styled(
                        c.ports.clone(),
                        Style::default().fg(Color::Magenta),
                    ));
                }
                if !c.status.is_empty() {
                    spans.push(Span::styled(format!("  {}", c.status), dim));
                }
            }
        }
        ListItem::new(Line::from(spans))
    }
}

impl Pane<PaneEvent> for ContainersPane {
    crate::impl_handle_key!(keymap);

    fn render(&mut self, f: &mut Frame, _ctx: &AppContext, shared: &PaneShared, area: Rect) {
        self.view_height = area.height.saturating_sub(2);
        let empty = if self.loading && self.rows.is_empty() {
            Some("Loading...")
        } else if self.rows.is_empty() {
            Some("No containers")
        } else {
            None
        };
        let is_focused = shared.focused_pane == self.pane_id;
        let show_selection = is_focused
            || ((shared.focused_pane == self.detail_pane_id
                || shared.focused_pane == self.logs_pane_id)
                && shared.previous_pane == self.pane_id);
        let selected = show_selection.then_some(self.selected_idx);
        theme::render_list_pane(
            f,
            area,
            shared,
            self.pane_id,
            "Containers",
            selected,
            empty,
            |match_set, current_match_idx| {
                self.rows
                    .iter()
                    .enumerate()
                    .map(|(idx, row)| {
                        let tree = self.positions.get(idx).cloned().unwrap_or_default();
                        let mut li = Self::render_row(row, &tree);
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
        pane::collect_list_search_matches(&self.rows, query, ContainerRow::search_text)
    }

    crate::impl_list_pane_selection!();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docker::domain::types::COMPOSE_PROJECT_LABEL;
    use std::collections::BTreeMap;

    fn container(name: &str, state: &str, project: Option<&str>) -> Container {
        let mut labels = BTreeMap::new();
        if let Some(p) = project {
            labels.insert(COMPOSE_PROJECT_LABEL.to_string(), p.to_string());
        }
        Container {
            id: format!("id-{name}"),
            name: name.to_string(),
            image: "img".into(),
            state: state.into(),
            status: String::new(),
            ports: String::new(),
            labels,
        }
    }

    fn rendered(rows: &[ContainerRow], positions: &[TreePos]) -> Vec<String> {
        rows.iter()
            .zip(positions)
            .map(|(r, p)| {
                let label = match r {
                    ContainerRow::Project { name, .. } => format!("[{name}]"),
                    ContainerRow::Container(c) => c.name.clone(),
                };
                format!("{}{label}", p.prefix)
            })
            .collect()
    }

    #[test]
    fn groups_compose_members_under_a_project_row_running_first() {
        let (rows, positions) = build_rows(vec![
            container("zeta", "exited", None),
            container("demo-web-1", "exited", Some("demo")),
            container("alpha", "running", None),
            container("demo-db-1", "running", Some("demo")),
            container("other-app-1", "running", Some("other")),
        ]);
        assert_eq!(
            rendered(&rows, &positions),
            [
                "alpha",
                "[demo]",
                "├─ demo-db-1",
                "└─ demo-web-1",
                "[other]",
                "└─ other-app-1",
                "zeta",
            ]
        );
        match &rows[1] {
            ContainerRow::Project { running, total, .. } => {
                assert_eq!((*running, *total), (1, 2));
            }
            _ => panic!("expected project row"),
        }
    }

    #[test]
    fn set_containers_keeps_selection_by_id() {
        let mut pane = ContainersPane::new(0, 1, 2);
        pane.set_containers(vec![
            container("a", "running", None),
            container("b", "running", None),
        ]);
        pane.selected_idx = 1;
        pane.set_containers(vec![
            container("c", "running", None),
            container("a", "running", None),
            container("b", "exited", None),
        ]);
        assert_eq!(pane.selected_container().unwrap().name, "b");
        assert_eq!(pane.counts(), (3, 2));
        pane.set_containers(vec![]);
        assert_eq!(pane.selected_idx, 0);
        assert!(pane.selected().is_none());
    }
}
