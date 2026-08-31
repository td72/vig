//! Left column of the Procs page: every process, nested under its parent.

use crate::core::app::AppContext;
use crate::core::keymap::{
    nav_bindings, search_bindings, ActionHelp, Keymap, NavAction, SearchAction,
};
use crate::core::pane::{self, Pane, PaneEvent, PaneShared};
use crate::core::search::SearchMatch;
use crate::core::tree::{nest_by, TreePos};
use crate::files::domain::fs::human_size;
use crate::procs::domain::types::{sort_processes, ProcessInfo, SortKey};
use crate::procs::panes::{dim, render_table_pane, truncate_chars};
use crossterm::event::KeyCode;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::ListItem;
use ratatui::{layout::Rect, Frame};

#[derive(Debug, Clone)]
pub enum ProcessesAction {
    Nav(NavAction),
    /// Move focus to the detail pane.
    FocusDetail,
    /// CPU → MEM → PID.
    CycleSort,
    /// Toggle the graphs pane between CPU history and per-core bars.
    TogglePerCore,
    Search(SearchAction),
    Esc,
}

crate::impl_pane_action_from_str!(
    ProcessesAction, nav: Nav, search: Search, esc: Esc,
    FocusDetail, CycleSort, TogglePerCore
);

impl ActionHelp for ProcessesAction {
    fn label(&self) -> Option<&'static str> {
        match self {
            ProcessesAction::Nav(nav) => nav.label(),
            ProcessesAction::FocusDetail => Some("Focus detail"),
            ProcessesAction::CycleSort => Some("Cycle sort (CPU / MEM / PID)"),
            ProcessesAction::TogglePerCore => Some("Toggle per-core CPU bars"),
            ProcessesAction::Search(sa) => sa.label(),
            ProcessesAction::Esc => Some("Clear search"),
        }
    }
}

pub fn default_keymap() -> Keymap<ProcessesAction> {
    Keymap::new()
        .bindings(nav_bindings(ProcessesAction::Nav))
        .bindings(search_bindings(ProcessesAction::Search))
        .key(KeyCode::Enter, ProcessesAction::FocusDetail)
        .key(KeyCode::Char('i'), ProcessesAction::FocusDetail)
        .key(KeyCode::Char('l'), ProcessesAction::FocusDetail)
        .key(KeyCode::Char('s'), ProcessesAction::CycleSort)
        .key(KeyCode::Char('c'), ProcessesAction::TogglePerCore)
        .key(KeyCode::Esc, ProcessesAction::Esc)
}

/// One display row: a process and where it sits in the tree.
#[derive(Debug, Clone)]
pub struct ProcRow {
    pub info: ProcessInfo,
    pub tree: TreePos,
}

/// Sort `procs` under `sort`, then nest children under their parent. Roots
/// and siblings keep the sorted order; a child whose parent is not in the
/// list is a root.
pub fn build_rows(mut procs: Vec<ProcessInfo>, sort: SortKey) -> Vec<ProcRow> {
    sort_processes(&mut procs, sort);
    let order = nest_by(
        procs.len(),
        |i| u64::from(procs[i].pid),
        |i| procs[i].ppid.map(u64::from),
    );
    let mut slots: Vec<Option<ProcessInfo>> = procs.into_iter().map(Some).collect();
    order
        .into_iter()
        .filter_map(|(i, tree)| slots[i].take().map(|info| ProcRow { info, tree }))
        .collect()
}

/// Width of the fixed columns before the command: `PID CPU% MEM `.
const FIXED_COLS: usize = 6 + 1 + 5 + 1 + 7 + 2;
const HEADER: &str = "   PID  CPU%     MEM  COMMAND";

pub struct ProcessesPane {
    pub rows: Vec<ProcRow>,
    pub selected_idx: usize,
    pub sort: SortKey,
    /// No snapshot has arrived yet.
    pub loading: bool,
    keymap: Keymap<ProcessesAction>,
    pane_id: usize,
    detail_pane_id: usize,
    view_height: u16,
}

impl ProcessesPane {
    pub fn new(pane_id: usize, detail_pane_id: usize) -> Self {
        Self {
            rows: Vec::new(),
            selected_idx: 0,
            sort: SortKey::default(),
            loading: true,
            keymap: default_keymap(),
            pane_id,
            detail_pane_id,
            view_height: 20,
        }
    }

    pub fn set_keymap(&mut self, km: Keymap<ProcessesAction>) {
        self.keymap = km;
    }

    pub fn keymap(&self) -> &Keymap<ProcessesAction> {
        &self.keymap
    }

    pub fn selected(&self) -> Option<&ProcessInfo> {
        self.rows.get(self.selected_idx).map(|r| &r.info)
    }

    pub fn selected_pid(&self) -> Option<u32> {
        self.selected().map(|p| p.pid)
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn name_of(&self, pid: u32) -> Option<String> {
        self.rows
            .iter()
            .find(|r| r.info.pid == pid)
            .map(|r| r.info.name.clone())
    }

    /// Direct children of `pid`, in display order.
    pub fn children_of(&self, pid: u32) -> Vec<(u32, String)> {
        self.rows
            .iter()
            .filter(|r| r.info.ppid == Some(pid) && r.info.pid != pid)
            .map(|r| (r.info.pid, r.info.name.clone()))
            .collect()
    }

    /// Replace the list with a fresh snapshot; the selection stays on the
    /// same pid when it is still alive.
    pub fn apply_snapshot(&mut self, procs: Vec<ProcessInfo>) {
        let keep = self.selected_pid();
        self.loading = false;
        self.rows = build_rows(procs, self.sort);
        self.select_pid(keep);
    }

    /// Advance the sort order and re-nest, keeping the selected pid.
    pub fn cycle_sort(&mut self) {
        self.sort = self.sort.next();
        let keep = self.selected_pid();
        let procs = self.rows.drain(..).map(|r| r.info).collect();
        self.rows = build_rows(procs, self.sort);
        self.select_pid(keep);
    }

    /// Select `pid`. Returns `false` (and clamps the current index) when it
    /// is not in the list.
    pub fn select_pid(&mut self, pid: Option<u32>) -> bool {
        match pid.and_then(|p| self.rows.iter().position(|r| r.info.pid == p)) {
            Some(idx) => {
                self.selected_idx = idx;
                true
            }
            None => {
                self.selected_idx = self.selected_idx.min(self.rows.len().saturating_sub(1));
                false
            }
        }
    }

    fn execute(&mut self, shared: &PaneShared, action: ProcessesAction) -> Vec<PaneEvent> {
        if let Some(events) = pane::try_dispatch_search_esc(&action, shared, self.pane_id, vec![]) {
            return events;
        }
        match action {
            ProcessesAction::Nav(nav) => pane::execute_list_nav(
                nav,
                &mut self.selected_idx,
                self.rows.len(),
                Some(self.view_height),
            ),
            ProcessesAction::FocusDetail if !self.rows.is_empty() => {
                vec![PaneEvent::SetFocus(self.detail_pane_id)]
            }
            ProcessesAction::CycleSort => {
                self.cycle_sort();
                vec![PaneEvent::SelectionChanged]
            }
            ProcessesAction::TogglePerCore => vec![PaneEvent::ToggleCpuCores],
            _ => vec![],
        }
    }

    fn row_line(row: &ProcRow, width: usize) -> Line<'static> {
        let info = &row.info;
        let cpu_style = if info.cpu >= 50.0 {
            Style::default().fg(Color::Red)
        } else if info.cpu >= 10.0 {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };
        let cmd_width = width.saturating_sub(FIXED_COLS + row.tree.prefix.chars().count());
        Line::from(vec![
            Span::styled(
                format!("{:>6} ", info.pid),
                Style::default().fg(Color::Cyan),
            ),
            Span::styled(format!("{:>5.1} ", info.cpu), cpu_style),
            Span::raw(format!("{:>7}  ", human_size(info.rss))),
            Span::styled(row.tree.prefix.clone(), dim()),
            Span::raw(truncate_chars(&info.cmd, cmd_width)),
        ])
    }
}

impl Pane<PaneEvent> for ProcessesPane {
    crate::impl_handle_key!(keymap);

    fn render(&mut self, f: &mut Frame, _ctx: &AppContext, shared: &PaneShared, area: Rect) {
        // Border (2) + column header (1).
        self.view_height = area.height.saturating_sub(3);
        let title = format!("Processes [sort: {}]", self.sort.label());
        let empty = if self.rows.is_empty() {
            Some(if self.loading {
                "Loading..."
            } else {
                "(no processes)"
            })
        } else {
            None
        };
        let is_focused = shared.focused_pane == self.pane_id;
        let emphasized = is_focused
            || (shared.focused_pane == self.detail_pane_id && shared.previous_pane == self.pane_id);
        let selected = (!self.rows.is_empty()).then_some(self.selected_idx);
        let width = area.width.saturating_sub(2) as usize;
        render_table_pane(
            f,
            area,
            shared,
            self.pane_id,
            &title,
            HEADER,
            selected,
            emphasized,
            empty,
            |match_set, current_match_idx| {
                self.rows
                    .iter()
                    .enumerate()
                    .map(|(idx, row)| {
                        let mut li = ListItem::new(Self::row_line(row, width));
                        let hl = crate::core::theme::search_highlight_for(
                            match_set,
                            current_match_idx,
                            idx,
                        );
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
        pane::collect_list_search_matches(&self.rows, query, |r| {
            format!("{} {} {}", r.info.pid, r.info.name, r.info.cmd)
        })
    }

    crate::impl_list_pane_selection!();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procs::domain::types::proc;

    fn pids(rows: &[ProcRow]) -> Vec<String> {
        rows.iter()
            .map(|r| format!("{}{}", r.tree.prefix, r.info.pid))
            .collect()
    }

    #[test]
    fn rows_nest_children_under_parent_in_sorted_order() {
        let procs = vec![
            proc(1, None, 0.0, 10),
            proc(200, Some(1), 5.0, 10),
            proc(300, Some(200), 1.0, 10),
            proc(100, Some(1), 9.0, 10),
            proc(400, Some(999), 0.5, 10), // orphan → root
        ];
        let rows = build_rows(procs.clone(), SortKey::Cpu);
        // Roots by CPU desc: pid 400 (0.5) then pid 1 (0.0); children of 1
        // by CPU desc: 100 (9.0), 200 (5.0) → 300.
        assert_eq!(pids(&rows), ["400", "1", "├─ 100", "└─ 200", "   └─ 300"]);
        let rows = build_rows(procs, SortKey::Pid);
        assert_eq!(pids(&rows), ["1", "├─ 100", "└─ 200", "   └─ 300", "400"]);
    }

    #[test]
    fn selection_follows_pid_across_refresh_and_sort() {
        let mut pane = ProcessesPane::new(0, 1);
        pane.apply_snapshot(vec![
            proc(1, None, 0.0, 10),
            proc(20, Some(1), 5.0, 100),
            proc(30, Some(1), 1.0, 900),
        ]);
        assert!(!pane.loading);
        pane.selected_idx = 2; // pid 30 (CPU order: 1, 20, 30)
        assert_eq!(pane.selected_pid(), Some(30));

        pane.cycle_sort(); // MEM: 30 before 20
        assert_eq!(pane.sort, SortKey::Mem);
        assert_eq!(pane.selected_pid(), Some(30));
        assert_eq!(pane.selected_idx, 1);

        // A refresh where pid 30 moved again still keeps it selected.
        pane.apply_snapshot(vec![
            proc(1, None, 0.0, 10),
            proc(30, Some(1), 1.0, 50),
            proc(20, Some(1), 5.0, 100),
            proc(40, Some(1), 0.0, 999),
        ]);
        assert_eq!(pane.selected_pid(), Some(30));

        // The pid disappearing clamps instead of jumping to the top.
        pane.selected_idx = 3;
        pane.apply_snapshot(vec![proc(1, None, 0.0, 10), proc(20, Some(1), 5.0, 100)]);
        assert_eq!(pane.selected_idx, 1);
        assert!(!pane.select_pid(Some(12345)));
    }

    #[test]
    fn children_and_names() {
        let mut pane = ProcessesPane::new(0, 1);
        pane.apply_snapshot(vec![
            proc(1, None, 0.0, 10),
            proc(2, Some(1), 0.0, 10),
            proc(3, Some(1), 0.0, 10),
            proc(4, Some(2), 0.0, 10),
        ]);
        assert_eq!(
            pane.children_of(1),
            vec![(2, "p2".to_string()), (3, "p3".to_string())]
        );
        assert_eq!(pane.children_of(4), vec![]);
        assert_eq!(pane.name_of(4).as_deref(), Some("p4"));
        assert_eq!(pane.name_of(9), None);
    }
}
