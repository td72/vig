//! Board pane: the selected project's items as kanban columns (one per
//! `Status` option, GitHub's order, plus `No status`) or, in table mode,
//! one row per item with the project fields as sortable columns.

use crate::core::app::AppContext;
use crate::core::keymap::{
    nav_bindings, search_bindings, ActionHelp, Keymap, NavAction, SearchAction,
};
use crate::core::pane::{self, Pane, PaneEvent, PaneShared};
use crate::core::search::SearchMatch;
use crate::core::theme;
use crate::projects::domain::filter::{self, Filter};
use crate::projects::domain::roadmap::{self, Zoom};
use crate::projects::domain::types::{
    columns_by, group_rows, item_key, sort_items_dir, table_columns, view_table_columns, Board,
    Column, ItemKind, ProjectField, ProjectItem, ProjectView, TableColumn, ViewLayout,
};
use crossterm::event::KeyCode;
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState},
    Frame,
};
use std::collections::HashSet;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Narrowest a kanban column gets before the board scrolls horizontally.
const MIN_COLUMN_WIDTH: u16 = 24;
/// Lines per card (title line + assignees line).
const CARD_HEIGHT: usize = 2;
/// Widest a table column (other than Title) grows to.
const MAX_CELL_WIDTH: usize = 24;

#[derive(Debug, Clone)]
pub enum BoardAction {
    Nav(NavAction),
    PrevColumn,
    NextColumn,
    ToggleTable,
    CycleSort,
    NextView,
    PrevView,
    /// Collapse / expand the selected swimlane (grouped board views).
    ToggleLane,
    /// Roadmap: a finer / coarser time scale.
    ZoomIn,
    ZoomOut,
    OpenDetail,
    OpenBrowser,
    CopyUrl,
    Search(SearchAction),
    Esc,
}

crate::impl_pane_action_from_str!(
    BoardAction, nav: Nav, search: Search, esc: Esc,
    PrevColumn, NextColumn, ToggleTable, CycleSort, NextView, PrevView, ToggleLane,
    ZoomIn, ZoomOut, OpenDetail, OpenBrowser, CopyUrl
);

impl ActionHelp for BoardAction {
    fn label(&self) -> Option<&'static str> {
        match self {
            BoardAction::Nav(NavAction::MoveDown) => Some("Next card / row"),
            BoardAction::Nav(NavAction::MoveUp) => Some("Prev card / row"),
            BoardAction::Nav(nav) => nav.label(),
            BoardAction::PrevColumn => Some("Prev column"),
            BoardAction::NextColumn => Some("Next column"),
            BoardAction::ToggleTable => Some("Toggle table mode"),
            BoardAction::CycleSort => Some("Cycle sort column (table)"),
            BoardAction::NextView => Some("Next saved view"),
            BoardAction::PrevView => Some("Prev saved view"),
            BoardAction::ToggleLane => Some("Collapse / expand lane"),
            BoardAction::ZoomIn => Some("Zoom in (roadmap)"),
            BoardAction::ZoomOut => Some("Zoom out (roadmap)"),
            BoardAction::OpenDetail => Some("Focus detail"),
            BoardAction::OpenBrowser => Some("Open item in browser"),
            BoardAction::CopyUrl => Some("Copy item URL"),
            BoardAction::Search(sa) => sa.label(),
            BoardAction::Esc => Some("Clear search / back to projects"),
        }
    }
}

pub fn default_keymap() -> Keymap<BoardAction> {
    Keymap::new()
        .bindings(nav_bindings(BoardAction::Nav))
        .bindings(search_bindings(BoardAction::Search))
        .key(KeyCode::Char('h'), BoardAction::PrevColumn)
        .key(KeyCode::Left, BoardAction::PrevColumn)
        .key(KeyCode::Char('l'), BoardAction::NextColumn)
        .key(KeyCode::Right, BoardAction::NextColumn)
        .key(KeyCode::Char('t'), BoardAction::ToggleTable)
        .key(KeyCode::Char('s'), BoardAction::CycleSort)
        .key(KeyCode::Char('v'), BoardAction::NextView)
        .key(KeyCode::Char('V'), BoardAction::PrevView)
        .key(KeyCode::Char(' '), BoardAction::ToggleLane)
        .key(KeyCode::Char('+'), BoardAction::ZoomIn)
        .key(KeyCode::Char('-'), BoardAction::ZoomOut)
        .key(KeyCode::Enter, BoardAction::OpenDetail)
        .key(KeyCode::Char('i'), BoardAction::OpenDetail)
        .key(KeyCode::Char('o'), BoardAction::OpenBrowser)
        .key(KeyCode::Char('y'), BoardAction::CopyUrl)
        .key(KeyCode::Esc, BoardAction::Esc)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoardMode {
    Board,
    Table,
    Roadmap,
}

/// One swimlane of a board view: a full set of the board's columns holding
/// the items whose grouping field has this lane's value. An ungrouped board
/// is a single unnamed lane.
pub struct BoardLane {
    /// `None` for the single lane of an ungrouped board.
    pub name: Option<String>,
    pub collapsed: bool,
    pub columns: Vec<Column>,
}

impl BoardLane {
    /// Items across the lane's columns.
    fn len(&self) -> usize {
        self.columns.iter().map(|c| c.items.len()).sum()
    }
}

pub struct BoardPane {
    pane_id: usize,
    detail_pane_id: usize,
    /// The project list pane, when the layout places it (Esc goes there).
    projects_pane_id: Option<usize>,
    keymap: Keymap<BoardAction>,
    pub board: Option<Board>,
    /// URL of the project the board belongs to (`o` on a draft opens it).
    project_url: Option<String>,
    /// `owner/repo` vig runs in: cards from other repositories say so.
    repo: Option<String>,
    /// Shown instead of a board (loading, nothing linked).
    notice: Option<String>,
    /// The board's swimlanes (one unnamed lane when the view has no
    /// horizontal grouping); every lane holds the same column set.
    lanes: Vec<BoardLane>,
    /// The name of the field driving the columns, when it is not `Status`.
    column_field_name: Option<String>,
    table_cols: Vec<TableColumn>,
    /// Table row order: indices into `board.items`.
    sorted: Vec<usize>,
    pub mode: BoardMode,
    /// Board mode: selected lane, column and card within it.
    pub lane: usize,
    pub col: usize,
    pub row: usize,
    /// Board mode: first visible lane (vertical scroll over the lanes).
    lane_offset: usize,
    /// Board mode: first visible column (horizontal scroll) and how many
    /// columns fit the pane (from the last render).
    col_offset: usize,
    visible_cols: usize,
    /// Board mode: first visible card per lane and column.
    col_scroll: Vec<Vec<usize>>,
    /// Table mode: selected row (into `sorted`) and scroll state.
    pub table_row: usize,
    table_state: TableState,
    pub sort_col: usize,
    /// Sort direction (`true` = descending), seeded by the view's sort.
    pub sort_desc: bool,
    /// Table group headers: `(label, item count)` in display order, from
    /// the view's `groupByFields` (empty when ungrouped).
    groups: Vec<(String, usize)>,
    /// The grouping field of the current view, resolved against the board.
    group_field: Option<ProjectField>,
    /// The shown saved view: an index into `board.views` (0 when the
    /// project has none — the fixed Status kanban).
    pub view_idx: usize,
    /// The view number the pane last applied (mode / sort seeded from it);
    /// seeing a different one re-applies.
    applied_view: Option<u64>,
    /// The current view's filter expression, parsed (`None` when the view
    /// has none); items failing it are left out of every layout.
    filter: Option<Filter>,
    /// The signed-in login, for `assignee:@me` in filters.
    viewer: Option<String>,
    /// Roadmap: time scale, horizontal scroll in cells (`None` = start of
    /// the timeline) and the first visible row.
    zoom: Zoom,
    roadmap_scroll: Option<i64>,
    roadmap_offset: usize,
    loading: bool,
    error: Option<String>,
    view_height: u16,
}

impl BoardPane {
    pub fn new(pane_id: usize, detail_pane_id: usize, projects_pane_id: Option<usize>) -> Self {
        Self {
            pane_id,
            detail_pane_id,
            projects_pane_id,
            keymap: default_keymap(),
            board: None,
            project_url: None,
            repo: None,
            notice: None,
            lanes: Vec::new(),
            column_field_name: None,
            table_cols: Vec::new(),
            sorted: Vec::new(),
            mode: BoardMode::Board,
            lane: 0,
            col: 0,
            row: 0,
            lane_offset: 0,
            col_offset: 0,
            visible_cols: 1,
            col_scroll: Vec::new(),
            table_row: 0,
            table_state: TableState::default(),
            sort_col: 0,
            sort_desc: false,
            groups: Vec::new(),
            group_field: None,
            view_idx: 0,
            applied_view: None,
            filter: None,
            viewer: None,
            zoom: Zoom::Week,
            roadmap_scroll: None,
            roadmap_offset: 0,
            loading: false,
            error: None,
            view_height: 20,
        }
    }

    pub fn set_keymap(&mut self, km: Keymap<BoardAction>) {
        self.keymap = km;
    }

    pub fn keymap(&self) -> &Keymap<BoardAction> {
        &self.keymap
    }

    pub fn set_loading(&mut self, loading: bool) {
        self.loading = loading;
    }

    pub fn is_loading(&self) -> bool {
        self.loading
    }

    pub fn set_error(&mut self, e: Option<String>) {
        self.error = e;
    }

    pub fn set_project_url(&mut self, url: Option<String>) {
        self.project_url = url;
    }

    pub fn set_repo(&mut self, repo: Option<String>) {
        self.repo = repo;
    }

    /// The signed-in login (`assignee:@me`); re-applies the view's filter.
    pub fn set_viewer(&mut self, login: Option<String>) {
        self.viewer = login;
        if self.board.is_some() {
            self.apply_view(false);
        }
    }

    /// Items the view's filter hides.
    pub fn filtered_out(&self) -> usize {
        match (&self.filter, &self.board) {
            (Some(_), Some(b)) => b.items.len().saturating_sub(self.sorted.len()),
            _ => 0,
        }
    }

    /// `⚠ filter: unsupported "…"` for tokens the filter ignored.
    pub fn filter_notice(&self) -> Option<String> {
        let f = self.filter.as_ref()?;
        if f.unsupported.is_empty() {
            return None;
        }
        let list: Vec<String> = f.unsupported.iter().map(|t| format!("\"{t}\"")).collect();
        Some(format!("⚠ filter: unsupported {}", list.join(", ")))
    }

    fn passes(filter: Option<&Filter>, item: &ProjectItem) -> bool {
        filter.is_none_or(|f| f.matches(item))
    }

    pub fn set_notice(&mut self, notice: Option<String>) {
        self.notice = notice;
    }

    #[cfg(test)]
    pub fn notice(&self) -> Option<&String> {
        self.notice.as_ref()
    }

    /// Drop the board (no project selected).
    pub fn clear(&mut self) {
        self.board = None;
        self.lanes.clear();
        self.column_field_name = None;
        self.table_cols.clear();
        self.sorted.clear();
        self.col_scroll.clear();
        self.lane = 0;
        self.col = 0;
        self.row = 0;
        self.lane_offset = 0;
        self.col_offset = 0;
        self.table_row = 0;
        self.view_idx = 0;
        self.applied_view = None;
        self.filter = None;
        self.zoom = Zoom::Week;
        self.roadmap_scroll = None;
        self.roadmap_offset = 0;
        self.sort_desc = false;
        self.groups.clear();
        self.group_field = None;
        self.error = None;
    }

    /// Show `board`, keeping the selection on the same item when it is
    /// still there (a refresh may move it to another column).
    pub fn set_board(&mut self, board: Board) {
        let keep = self.selected_item().map(|i| i.id.clone());
        // Keep the shown view (by number: the saved views may have changed)
        // across a refresh of the same project; another project starts on
        // its default local view, else its first view.
        if self.board.as_ref().map(|b| b.number) != Some(board.number) {
            self.view_idx = board.views.iter().position(|v| v.initial).unwrap_or(0);
        } else if let Some(idx) = board
            .views
            .iter()
            .position(|v| Some(v.number) == self.applied_view)
        {
            self.view_idx = idx;
        }
        self.view_idx = self.view_idx.min(board.views.len().saturating_sub(1));
        let same_project = self.board.as_ref().map(|b| b.number) == Some(board.number);
        self.error = None;
        self.board = Some(board);
        self.apply_view(!same_project);
        let idx = keep.and_then(|id| self.board.as_ref()?.items.iter().position(|i| i.id == id));
        match idx {
            Some(idx) => self.select_item(idx),
            None => {
                self.lane = self.lane.min(self.lanes.len().saturating_sub(1));
                self.col = self.col.min(self.column_count().saturating_sub(1));
                if self.column_len(self.col) == 0 {
                    // Start on the first column of the lane that has a card.
                    let cur = self.lanes.get(self.lane);
                    if let Some(c) =
                        cur.and_then(|l| l.columns.iter().position(|c| !c.items.is_empty()))
                    {
                        self.col = c;
                    }
                }
                self.row = self.row.min(self.column_len(self.col).saturating_sub(1));
                self.table_row = self.table_row.min(self.sorted.len().saturating_sub(1));
            }
        }
    }

    /// Items shown (the view's filter, when any, already applied).
    pub fn item_count(&self) -> usize {
        match &self.filter {
            Some(_) => self.sorted.len(),
            None => self.board.as_ref().map_or(0, |b| b.items.len()),
        }
    }

    pub fn column_count(&self) -> usize {
        self.lanes.first().map_or(0, |l| l.columns.len())
    }

    pub fn truncated(&self) -> bool {
        self.board.as_ref().is_some_and(Board::truncated)
    }

    fn column_len(&self, col: usize) -> usize {
        self.lanes
            .get(self.lane)
            .and_then(|l| l.columns.get(col))
            .map_or(0, |c| c.items.len())
    }

    /// Index into `board.items` of the selected card / row.
    pub fn selected_index(&self) -> Option<usize> {
        match self.mode {
            BoardMode::Board => self
                .lanes
                .get(self.lane)?
                .columns
                .get(self.col)?
                .items
                .get(self.row)
                .copied(),
            BoardMode::Table | BoardMode::Roadmap => self.sorted.get(self.table_row).copied(),
        }
    }

    /// URL of the selected item (issues / PRs their own, drafts the
    /// project's), for both `o` and `y`.
    fn selected_url(&self) -> Option<String> {
        self.selected_item()
            .and_then(|i| i.url().map(str::to_string))
            .filter(|u| !u.is_empty())
            .or_else(|| self.project_url.clone())
    }

    pub fn selected_item(&self) -> Option<&ProjectItem> {
        let idx = self.selected_index()?;
        self.board.as_ref()?.items.get(idx)
    }

    /// Point both modes' selections at item `idx` (expanding the lane
    /// holding it, so the selection never hides inside a collapsed one).
    fn select_item(&mut self, idx: usize) {
        let found = self.lanes.iter().enumerate().find_map(|(li, lane)| {
            lane.columns
                .iter()
                .enumerate()
                .find_map(|(c, col)| col.items.iter().position(|&i| i == idx).map(|r| (li, c, r)))
        });
        if let Some((li, c, r)) = found {
            self.lane = li;
            self.col = c;
            self.row = r;
            self.lanes[li].collapsed = false;
        }
        if let Some(r) = self.sorted.iter().position(|&i| i == idx) {
            self.table_row = r;
        }
    }

    /// The selected lane collapsed: move the selection to the nearest
    /// expanded lane with any card (its first non-empty column).
    fn move_off_lane(&mut self) {
        let after = self.lane + 1..self.lanes.len();
        let before = (0..self.lane).rev();
        for li in after.chain(before) {
            let lane = &self.lanes[li];
            if lane.collapsed || lane.len() == 0 {
                continue;
            }
            if let Some(c) = lane.columns.iter().position(|c| !c.items.is_empty()) {
                self.lane = li;
                self.col = c;
                self.row = 0;
                return;
            }
        }
    }

    /// Move the selection to the next / previous expanded lane that has a
    /// card in the current column.
    fn cross_lane(&mut self, forward: bool) {
        let candidates: Vec<usize> = if forward {
            (self.lane + 1..self.lanes.len()).collect()
        } else {
            (0..self.lane).rev().collect()
        };
        for li in candidates {
            let lane = &self.lanes[li];
            if lane.collapsed {
                continue;
            }
            let len = lane.columns.get(self.col).map_or(0, |c| c.items.len());
            if len > 0 {
                self.lane = li;
                self.row = if forward { 0 } else { len - 1 };
                return;
            }
        }
    }

    /// The shown saved view, when the project has any.
    pub fn current_view(&self) -> Option<&ProjectView> {
        self.board.as_ref()?.views.get(self.view_idx)
    }

    /// The shown view comes from the config (`projects-view`), not GitHub.
    pub fn current_view_is_local(&self) -> bool {
        self.current_view().is_some_and(|v| v.local)
    }

    /// `⚠ view "Mine": no field "Foo"` when the current view names fields
    /// the board does not have (those settings are ignored).
    pub fn view_notice(&self) -> Option<String> {
        let board = self.board.as_ref()?;
        let view = self.current_view()?;
        let known = |name: &String| {
            matches!(name.as_str(), "Title" | "Assignees")
                || board.fields.iter().any(|f| &f.name == name)
        };
        // Each name once, in first-seen order (a field may be named in
        // several settings).
        let mut missing: Vec<&String> = Vec::new();
        for name in view
            .vertical_group_by
            .iter()
            .chain(view.group_by.iter())
            .chain(view.sort_by.iter().map(|s| &s.field))
            .chain(view.visible_fields.iter())
        {
            if !known(name) && !missing.contains(&name) {
                missing.push(name);
            }
        }
        if missing.is_empty() {
            return None;
        }
        let list: Vec<String> = missing.iter().map(|n| format!("{n:?}")).collect();
        Some(format!(
            "⚠ view {:?}: no field {}",
            view.name,
            list.join(", ")
        ))
    }

    /// `(view name, position, count)` for the header.
    pub fn view_label(&self) -> Option<(&str, usize, usize)> {
        let views = &self.board.as_ref()?.views;
        let view = views.get(self.view_idx)?;
        Some((view.name.as_str(), self.view_idx + 1, views.len()))
    }

    /// `v` / `V`: show the next / previous saved view.
    fn cycle_view(&mut self, forward: bool) -> Vec<PaneEvent> {
        let n = self.board.as_ref().map_or(0, |b| b.views.len());
        if n == 0 {
            return vec![PaneEvent::StatusMessage(
                "This project has no saved views".into(),
            )];
        }
        if n > 1 {
            self.view_idx = if forward {
                (self.view_idx + 1) % n
            } else {
                (self.view_idx + n - 1) % n
            };
            self.apply_view(true);
        }
        vec![]
    }

    /// Point the pane at the current saved view: its table columns,
    /// grouping, initial sort and — when `reset` (a view or project
    /// switch) — the mode its layout calls for. Without a saved view the
    /// defaults stay (all fields, Status kanban).
    fn apply_view(&mut self, force: bool) {
        let Some(board) = &self.board else {
            return;
        };
        let view = board.views.get(self.view_idx);
        // A different view than last time (a switch, or views arriving for
        // the first time) seeds mode and sort even without `force`.
        let reset = force || view.map(|v| v.number) != self.applied_view;
        self.applied_view = view.map(|v| v.number);
        self.table_cols = match view {
            Some(v) if !v.visible_fields.is_empty() => view_table_columns(v, &board.fields),
            _ => table_columns(&board.fields),
        };
        self.group_field = view
            .and_then(|v| v.group_by.first())
            .and_then(|name| board.fields.iter().find(|f| &f.name == name))
            .cloned();
        self.filter = view
            .and_then(|v| v.filter.as_deref())
            .map(|expr| filter::parse(expr, self.viewer.as_deref()))
            .filter(|f| !f.is_empty() || !f.unsupported.is_empty());
        if reset {
            let sort = view.and_then(|v| v.sort_by.first());
            self.sort_col = sort
                .and_then(|s| {
                    self.table_cols
                        .iter()
                        .position(|c| c.header() == s.field.as_str())
                })
                .unwrap_or(0);
            self.sort_desc = sort.is_some_and(|s| s.desc);
            self.mode = match view.map(|v| v.layout) {
                Some(ViewLayout::Table) => BoardMode::Table,
                Some(ViewLayout::Roadmap) => BoardMode::Roadmap,
                _ => BoardMode::Board,
            };
            self.roadmap_scroll = None;
        }
        self.sort_col = self.sort_col.min(self.table_cols.len().saturating_sub(1));
        self.resort();
        self.rebuild_lanes();
    }

    /// Card order for the board: the view's first sort key, else the
    /// fetch order.
    fn board_card_order(board: &Board, view: Option<&ProjectView>) -> Vec<usize> {
        let Some(sort) = view.and_then(|v| v.sort_by.first()) else {
            return (0..board.items.len()).collect();
        };
        let col = match sort.field.as_str() {
            "Title" => TableColumn::Title,
            "Assignees" => TableColumn::Assignees,
            name => TableColumn::Field {
                name: name.to_string(),
                key: item_key(name),
            },
        };
        sort_items_dir(&board.items, &col, board, sort.desc)
    }

    /// Build the swimlanes for the current view: columns from its
    /// `verticalGroupByFields` (else `Status`), one lane per value of its
    /// `groupByFields` (else a single unnamed lane), cards ordered by its
    /// sort. Collapsed lanes stay collapsed across a refresh (by name).
    fn rebuild_lanes(&mut self) {
        let Some(board) = &self.board else {
            self.lanes.clear();
            return;
        };
        let view = board.views.get(self.view_idx);
        let mut order = Self::board_card_order(board, view);
        order.retain(|&i| Self::passes(self.filter.as_ref(), &board.items[i]));
        let find_field = |name: &String| board.fields.iter().find(|f| &f.name == name).cloned();
        let col_field = view
            .and_then(|v| v.vertical_group_by.first())
            .and_then(find_field);
        let lane_field = view.and_then(|v| v.group_by.first()).and_then(find_field);
        self.column_field_name = col_field.as_ref().map(|f| f.name.clone());
        let template = columns_by(board, col_field.as_ref(), &order);
        let collapsed: std::collections::HashSet<String> = self
            .lanes
            .iter()
            .filter(|l| l.collapsed)
            .filter_map(|l| l.name.clone())
            .collect();
        self.lanes = match &lane_field {
            None => vec![BoardLane {
                name: None,
                collapsed: false,
                columns: template,
            }],
            Some(field) => {
                let key = col_field
                    .as_ref()
                    .map(|f| f.item_key())
                    .unwrap_or_else(|| "status".to_string());
                let no_label = col_field
                    .as_ref()
                    .map(|f| format!("No {}", f.name.to_lowercase()))
                    .unwrap_or_else(|| crate::projects::domain::types::NO_STATUS.to_string());
                group_rows(board, field, &order)
                    .into_iter()
                    .map(|(label, items)| {
                        let mut columns: Vec<Column> =
                            template.iter().map(|c| Column::new(&c.name)).collect();
                        for idx in items {
                            let value = board.items[idx]
                                .field_text(&key)
                                .filter(|v| !v.is_empty())
                                .unwrap_or_else(|| no_label.clone());
                            if let Some(col) = columns.iter_mut().find(|c| c.name == value) {
                                col.items.push(idx);
                            }
                        }
                        BoardLane {
                            collapsed: collapsed.contains(&label),
                            name: Some(label),
                            columns,
                        }
                    })
                    .collect()
            }
        };
        self.lane = self.lane.min(self.lanes.len().saturating_sub(1));
        self.lane_offset = self.lane_offset.min(self.lane);
        self.col = self.col.min(self.column_count().saturating_sub(1));
        self.row = self.row.min(self.column_len(self.col).saturating_sub(1));
        self.col_scroll = vec![vec![0; self.column_count()]; self.lanes.len()];
    }

    /// The sort column's header, for the pane title.
    pub fn sort_label(&self) -> Option<&str> {
        self.table_cols.get(self.sort_col).map(TableColumn::header)
    }

    fn resort(&mut self) {
        let keep = self.selected_index();
        if let (Some(board), Some(col)) = (&self.board, self.table_cols.get(self.sort_col)) {
            let mut order = sort_items_dir(&board.items, col, board, self.sort_desc);
            order.retain(|&i| Self::passes(self.filter.as_ref(), &board.items[i]));
            match &self.group_field {
                Some(field) => {
                    let grouped = group_rows(board, field, &order);
                    self.groups = grouped
                        .iter()
                        .map(|(label, items)| (label.clone(), items.len()))
                        .collect();
                    self.sorted = grouped.into_iter().flat_map(|(_, items)| items).collect();
                }
                None => {
                    self.groups.clear();
                    self.sorted = order;
                }
            }
        }
        self.table_row = self.table_row.min(self.sorted.len().saturating_sub(1));
        if let Some(idx) = keep {
            self.select_item(idx);
        }
    }

    /// Group header rows rendered above row `row` (indices into `sorted`).
    fn headers_before(&self, row: usize) -> usize {
        let mut headers = 0;
        let mut start = 0;
        for (_, count) in &self.groups {
            if row >= start {
                headers += 1;
            } else {
                break;
            }
            start += count;
        }
        headers
    }

    fn execute(&mut self, shared: &PaneShared, action: BoardAction) -> Vec<PaneEvent> {
        let back = self
            .projects_pane_id
            .map(|id| vec![PaneEvent::SetFocus(id)])
            .unwrap_or_default();
        if let Some(events) = pane::try_dispatch_search_esc(&action, shared, self.pane_id, back) {
            return events;
        }
        let before = self.selected_index();
        match action {
            BoardAction::Nav(nav) => match self.mode {
                BoardMode::Board => {
                    let len = self.column_len(self.col);
                    match nav {
                        // At a column's edge, j / k cross into the next /
                        // previous lane that has a card here.
                        NavAction::MoveDown if self.row + 1 >= len => self.cross_lane(true),
                        NavAction::MoveUp if self.row == 0 => self.cross_lane(false),
                        _ => {
                            pane::execute_list_nav(nav, &mut self.row, len, Some(self.view_height));
                        }
                    }
                }
                BoardMode::Table | BoardMode::Roadmap => {
                    pane::execute_list_nav(
                        nav,
                        &mut self.table_row,
                        self.sorted.len(),
                        Some(self.view_height),
                    );
                }
            },
            BoardAction::PrevColumn | BoardAction::NextColumn => {
                if self.mode == BoardMode::Roadmap {
                    let step = if matches!(action, BoardAction::NextColumn) {
                        7
                    } else {
                        -7
                    };
                    self.roadmap_scroll = Some((self.roadmap_scroll.unwrap_or(0) + step).max(0));
                } else if self.mode == BoardMode::Table {
                    // Left / right pick the sort column in table mode.
                    if !self.table_cols.is_empty() {
                        let n = self.table_cols.len();
                        self.sort_col = if matches!(action, BoardAction::NextColumn) {
                            (self.sort_col + 1) % n
                        } else {
                            (self.sort_col + n - 1) % n
                        };
                        self.sort_desc = false;
                        self.resort();
                    }
                } else if self.column_count() > 0 {
                    let n = self.column_count();
                    self.col = if matches!(action, BoardAction::NextColumn) {
                        (self.col + 1).min(n - 1)
                    } else {
                        self.col.saturating_sub(1)
                    };
                    self.row = self.row.min(self.column_len(self.col).saturating_sub(1));
                }
            }
            BoardAction::ToggleTable => {
                self.mode = match self.mode {
                    BoardMode::Board | BoardMode::Roadmap => BoardMode::Table,
                    // Back to whatever the current view's layout renders as.
                    BoardMode::Table => match self.current_view().map(|v| v.layout) {
                        Some(ViewLayout::Roadmap) => BoardMode::Roadmap,
                        _ => BoardMode::Board,
                    },
                };
                if let Some(idx) = before {
                    self.select_item(idx);
                }
            }
            BoardAction::CycleSort => {
                if self.mode == BoardMode::Table && !self.table_cols.is_empty() {
                    self.sort_col = (self.sort_col + 1) % self.table_cols.len();
                    self.sort_desc = false;
                    self.resort();
                }
            }
            BoardAction::ToggleLane => {
                if self.mode == BoardMode::Board && self.lanes.len() > 1 {
                    let collapsed = {
                        let lane = &mut self.lanes[self.lane];
                        lane.collapsed = !lane.collapsed;
                        lane.collapsed
                    };
                    if collapsed {
                        self.move_off_lane();
                    }
                }
            }
            BoardAction::ZoomIn | BoardAction::ZoomOut => {
                if self.mode == BoardMode::Roadmap {
                    self.zoom = if matches!(action, BoardAction::ZoomIn) {
                        self.zoom.zoom_in()
                    } else {
                        self.zoom.zoom_out()
                    };
                    self.roadmap_scroll = None;
                }
            }
            BoardAction::NextView => return self.cycle_view(true),
            BoardAction::PrevView => return self.cycle_view(false),
            BoardAction::OpenDetail => {
                if self.selected_item().is_some() {
                    return vec![PaneEvent::SetFocus(self.detail_pane_id)];
                }
            }
            BoardAction::OpenBrowser => {
                let url = self.selected_url();
                return match url {
                    Some(u) if !u.is_empty() => vec![PaneEvent::OpenUrl(u)],
                    _ => vec![],
                };
            }
            BoardAction::CopyUrl => {
                return vec![crate::github::panes::gh_list::copy_url_event(
                    self.selected_url(),
                )];
            }
            BoardAction::Search(_) | BoardAction::Esc => {}
        }
        if self.selected_index() != before {
            vec![PaneEvent::SelectionChanged]
        } else {
            vec![]
        }
    }

    // === Rendering ===

    fn title(&self) -> String {
        let name = self
            .board
            .as_ref()
            .map(|b| format!(" #{}", b.number))
            .unwrap_or_default();
        match self.mode {
            BoardMode::Board => {
                let n = self.column_count();
                let visible = self.visible_cols;
                let mut t = if n > visible {
                    let last = (self.col_offset + visible).min(n);
                    format!(
                        "Board{name} (columns {}-{last} of {n})",
                        self.col_offset + 1
                    )
                } else {
                    format!("Board{name}")
                };
                if let Some(f) = self.column_field_name.as_deref().filter(|f| *f != "Status") {
                    t.push_str(&format!(" [columns: {f}]"));
                }
                if self.lanes.len() > 1 {
                    if let Some(f) = &self.group_field {
                        t.push_str(&format!(" · lanes: {}", f.name));
                    }
                }
                t
            }
            BoardMode::Roadmap => format!("Roadmap{name} [{}]", self.zoom.label()),
            BoardMode::Table => {
                let mut t = match self.sort_label() {
                    Some(s) => format!(
                        "Table{name} [sort: {s}{}]",
                        if self.sort_desc { " desc" } else { "" }
                    ),
                    None => format!("Table{name}"),
                };
                if let Some(f) = &self.group_field {
                    t.push_str(&format!(" · by {}", f.name));
                }
                t
            }
        }
    }

    /// How many columns fit `inner_width`, scrolling so the selected
    /// column stays on screen.
    fn layout_columns(&mut self, inner_width: u16) {
        let n = self.column_count().max(1);
        let visible = ((inner_width / MIN_COLUMN_WIDTH) as usize).clamp(1, n);
        self.visible_cols = visible;
        if self.col < self.col_offset {
            self.col_offset = self.col;
        } else if self.col >= self.col_offset + visible {
            self.col_offset = self.col + 1 - visible;
        }
        self.col_offset = self.col_offset.min(n - visible);
    }

    fn render_board(
        &mut self,
        f: &mut Frame,
        inner: Rect,
        is_focused: bool,
        show_selection: bool,
        match_set: &HashSet<usize>,
        current_match: Option<usize>,
    ) {
        if self.lanes.len() <= 1 {
            self.render_lane(
                f,
                0,
                inner,
                is_focused,
                show_selection,
                match_set,
                current_match,
            );
            return;
        }
        // Swimlanes: one header line per lane, expanded lanes get their
        // columns below it. Lanes scroll (`lane_offset`) so the selected
        // one is always on screen with room for at least one card.
        let h = inner.height;
        let wants: Vec<u16> = self
            .lanes
            .iter()
            .map(|lane| {
                let cards = lane
                    .columns
                    .iter()
                    .map(|c| c.items.len())
                    .max()
                    .unwrap_or(0);
                let want = cards.max(1) * CARD_HEIGHT + 2;
                want.min(h.saturating_sub(1) as usize) as u16
            })
            .collect();
        self.lane_offset = self.lane_offset.min(self.lane);
        loop {
            let mut y = 0u16;
            let mut current_placed = false;
            for (li, want) in wants.iter().enumerate().skip(self.lane_offset) {
                if y >= h {
                    break;
                }
                y += 1; // header
                if self.lanes[li].collapsed {
                    if li == self.lane {
                        current_placed = true;
                    }
                } else {
                    let give = (*want).min(h - y);
                    if li == self.lane && give >= 4 {
                        current_placed = true;
                    }
                    y += give;
                }
            }
            if current_placed || self.lane_offset >= self.lane {
                break;
            }
            self.lane_offset += 1;
        }
        let bottom = inner.y + h;
        let mut y = inner.y;
        for (li, want) in wants.iter().enumerate().skip(self.lane_offset) {
            if y >= bottom {
                break;
            }
            let want = *want;
            let lane = &self.lanes[li];
            let marker = if lane.collapsed { "▸" } else { "▾" };
            let label = format!(
                " {marker} {} ({})",
                lane.name.as_deref().unwrap_or(""),
                lane.len()
            );
            let style = if li == self.lane && is_focused {
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Magenta)
            };
            let header = Rect {
                x: inner.x,
                y,
                width: inner.width,
                height: 1,
            };
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(label, style))),
                header,
            );
            y += 1;
            if !self.lanes[li].collapsed && y < bottom {
                let give = want.min(bottom - y);
                if give >= 3 {
                    let area = Rect {
                        x: inner.x,
                        y,
                        width: inner.width,
                        height: give,
                    };
                    self.render_lane(
                        f,
                        li,
                        area,
                        is_focused,
                        show_selection,
                        match_set,
                        current_match,
                    );
                }
                y += give;
            }
        }
    }

    /// One lane's columns (the whole pane for an ungrouped board).
    #[allow(clippy::too_many_arguments)]
    fn render_lane(
        &mut self,
        f: &mut Frame,
        lane_idx: usize,
        inner: Rect,
        is_focused: bool,
        show_selection: bool,
        match_set: &HashSet<usize>,
        current_match: Option<usize>,
    ) {
        let visible = self.visible_cols.min(self.column_count());
        if visible == 0 || inner.height == 0 {
            return;
        }
        let constraints: Vec<Constraint> = (0..visible)
            .map(|_| Constraint::Ratio(1, visible as u32))
            .collect();
        let areas = Layout::horizontal(constraints).split(inner);
        let Some(board) = &self.board else {
            return;
        };
        let in_lane = lane_idx == self.lane;
        for (slot, ci) in (self.col_offset..self.col_offset + visible).enumerate() {
            let column = &self.lanes[lane_idx].columns[ci];
            let area = areas[slot];
            let active = in_lane && ci == self.col;
            let (border, title_style) = if active && is_focused {
                (
                    Style::default().fg(theme::BORDER_FOCUSED),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )
            } else if active {
                (
                    Style::default().fg(theme::BORDER_UNFOCUSED),
                    Style::default().add_modifier(Modifier::BOLD),
                )
            } else {
                (
                    Style::default().fg(theme::BORDER_UNFOCUSED),
                    Style::default().fg(Color::DarkGray),
                )
            };
            let block = Block::default()
                .title(Line::from(Span::styled(
                    format!(" {} ({}) ", column.name, column.items.len()),
                    title_style,
                )))
                .borders(Borders::ALL)
                .border_style(border);
            let body = block.inner(area);
            f.render_widget(block, area);
            if body.height == 0 || body.width == 0 {
                continue;
            }
            let per_page = (body.height as usize / CARD_HEIGHT).max(1);
            if active {
                self.view_height = per_page as u16;
            }
            let scroll = &mut self.col_scroll[lane_idx][ci];
            if active {
                if self.row < *scroll {
                    *scroll = self.row;
                } else if self.row >= *scroll + per_page {
                    *scroll = self.row + 1 - per_page;
                }
            }
            *scroll = (*scroll).min(column.items.len().saturating_sub(per_page));
            let width = body.width as usize;
            let mut lines: Vec<Line<'static>> = Vec::with_capacity(per_page * CARD_HEIGHT);
            for (k, &idx) in column.items.iter().enumerate().skip(*scroll).take(per_page) {
                let item = &board.items[idx];
                let selected = show_selection && active && k == self.row;
                let hl = theme::search_highlight_for(match_set, current_match, idx);
                let bg = if hl.is_active() {
                    hl.bg
                } else if selected {
                    Some(theme::LIST_SELECTION_BG)
                } else {
                    None
                };
                for line in card_lines(item, self.repo.as_deref(), width, selected, hl.fg_override)
                {
                    lines.push(match bg {
                        Some(bg) => line.style(Style::default().bg(bg)),
                        None => line,
                    });
                }
            }
            if column.items.is_empty() {
                lines.push(Line::from(Span::styled(
                    " (empty)",
                    Style::default().fg(theme::EMPTY_TEXT_FG),
                )));
            }
            f.render_widget(Paragraph::new(lines), body);
        }
    }

    fn render_table(
        &mut self,
        f: &mut Frame,
        inner: Rect,
        show_selection: bool,
        match_set: &HashSet<usize>,
        current_match: Option<usize>,
    ) {
        let Some(board) = &self.board else {
            return;
        };
        self.view_height = inner.height.saturating_sub(1);
        let cells: Vec<Vec<String>> = self
            .sorted
            .iter()
            .map(|&idx| {
                let item = &board.items[idx];
                self.table_cols.iter().map(|c| c.cell(item)).collect()
            })
            .collect();
        let widths: Vec<Constraint> = self
            .table_cols
            .iter()
            .enumerate()
            .map(|(ci, col)| match col {
                TableColumn::Title => Constraint::Min(20),
                _ => {
                    let w = cells
                        .iter()
                        .map(|r| r[ci].width())
                        .max()
                        .unwrap_or(0)
                        .max(col.header().width() + 2)
                        .min(MAX_CELL_WIDTH);
                    Constraint::Length(w as u16)
                }
            })
            .collect();
        let header = Row::new(self.table_cols.iter().enumerate().map(|(ci, col)| {
            let text = if ci == self.sort_col {
                format!(
                    "{} {}",
                    col.header(),
                    if self.sort_desc { "▴" } else { "▾" }
                )
            } else {
                col.header().to_string()
            };
            Cell::from(Span::styled(
                text,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ))
        }));
        // Group boundaries (from the view's grouping): row index in
        // `sorted` where each group starts, with its header label.
        let mut group_starts: std::collections::HashMap<usize, String> =
            std::collections::HashMap::new();
        let mut start = 0;
        for (label, count) in &self.groups {
            group_starts.insert(start, format!("{label} ({count})"));
            start += count;
        }
        let mut rows: Vec<Row> = Vec::with_capacity(self.sorted.len() + self.groups.len());
        for (ri, (&idx, cells)) in self.sorted.iter().zip(&cells).enumerate() {
            if let Some(label) = group_starts.get(&ri) {
                // The label goes into the Title column, wherever the view
                // put it; every other cell stays empty so the row has the
                // table's column count.
                let title_col = self
                    .table_cols
                    .iter()
                    .position(|c| matches!(c, TableColumn::Title))
                    .unwrap_or(0);
                rows.push(Row::new((0..self.table_cols.len()).map(|ci| {
                    if ci == title_col {
                        Cell::from(Span::styled(
                            label.clone(),
                            Style::default()
                                .fg(Color::Magenta)
                                .add_modifier(Modifier::BOLD),
                        ))
                    } else {
                        Cell::from("")
                    }
                })));
            }
            let item = &board.items[idx];
            let hl = theme::search_highlight_for(match_set, current_match, idx);
            let mut row = Row::new(cells.iter().enumerate().map(|(ci, text)| {
                let mut style = match self.table_cols[ci] {
                    TableColumn::Number => Style::default().fg(Color::Yellow),
                    TableColumn::Assignees => Style::default().fg(Color::Gray),
                    TableColumn::Title => Style::default(),
                    TableColumn::Field { .. } => Style::default().fg(Color::Blue),
                };
                if let Some(fg) = hl.fg_override {
                    style = style.fg(fg);
                }
                let text = if ci == 1 {
                    format!("{} {}", item.kind().icon(), text)
                } else {
                    truncate_to_width(text, MAX_CELL_WIDTH)
                };
                Cell::from(Span::styled(text, style))
            }));
            if let Some(bg) = hl.bg {
                row = row.style(Style::default().bg(bg));
            }
            rows.push(row);
        }
        let selected_is_match = self
            .selected_index()
            .is_some_and(|i| match_set.contains(&i));
        let table = Table::new(rows, widths)
            .header(header)
            .column_spacing(1)
            .row_highlight_style(theme::list_highlight_style(selected_is_match));
        // The visual row of the selection: its position in `sorted` plus
        // the group headers rendered above it.
        let visual = self.table_row + self.headers_before(self.table_row);
        self.table_state.select(show_selection.then_some(visual));
        f.render_stateful_widget(table, inner, &mut self.table_state);
    }
    /// The Roadmap layout: item rows on the left, a time scale with one
    /// bar per item on the right, a today marker and shaded iteration
    /// bands. Spans come from date / iteration fields (`span_spec`).
    fn render_roadmap(
        &mut self,
        f: &mut Frame,
        inner: Rect,
        show_selection: bool,
        match_set: &HashSet<usize>,
        current_match: Option<usize>,
    ) {
        let Some(board) = &self.board else {
            return;
        };
        let spec = roadmap::span_spec(board);
        let spans: Vec<Option<(i64, i64)>> = self
            .sorted
            .iter()
            .map(|&i| roadmap::item_span(&board.items[i], &spec))
            .collect();
        let today = roadmap::today();
        let left_w = (inner.width / 3).clamp(16, 40) as usize;
        let chart_w = (inner.width as usize).saturating_sub(left_w + 1);
        // The timeline starts a little before the earliest span (or today).
        let min_day = spans
            .iter()
            .flatten()
            .map(|s| s.0)
            .min()
            .unwrap_or(today)
            .min(today);
        let origin = min_day - 2;
        let scroll = self.roadmap_scroll.unwrap_or(0);
        let zoom = self.zoom;
        let header_h = 2usize;
        let rows_h = (inner.height as usize).saturating_sub(header_h);
        self.view_height = rows_h as u16;
        // Keep the selected row on screen.
        if self.table_row < self.roadmap_offset {
            self.roadmap_offset = self.table_row;
        } else if rows_h > 0 && self.table_row >= self.roadmap_offset + rows_h {
            self.roadmap_offset = self.table_row + 1 - rows_h;
        }
        // Day of the leftmost visible cell, and a cell → in-view test.
        let visible = |x: i64| -> Option<usize> {
            let c = x - scroll;
            (0..chart_w as i64).contains(&c).then_some(c as usize)
        };

        let mut lines: Vec<Line<'static>> = Vec::with_capacity(header_h + rows_h);

        // Header 1: month labels at each month start.
        let mut months = vec![(' ', Style::default()); chart_w];
        // Header 2: week ticks and iteration bands.
        let dim = Style::default().fg(Color::DarkGray);
        let mut ticks = vec![('·', dim); chart_w];
        let iterations = roadmap::iterations(board, &spec);
        for (i, it) in iterations.iter().enumerate() {
            let band = if i % 2 == 0 {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else {
                Style::default().fg(Color::Black).bg(Color::Blue)
            };
            let label: Vec<char> = format!(" {} ", it.title).chars().collect();
            let (bs, be) = (zoom.x(it.start, origin), zoom.x_end(it.end, origin));
            for x in bs..=be {
                if let Some(c) = visible(x) {
                    let k = (x - bs) as usize;
                    ticks[c] = (*label.get(k).unwrap_or(&' '), band);
                }
            }
        }
        // Walk the visible days for month starts and week ticks.
        let days_per_cell = match zoom {
            Zoom::Month => 3,
            Zoom::Week => 1,
            Zoom::Day => 1,
        };
        let first_day = origin + scroll * days_per_cell / if zoom == Zoom::Day { 3 } else { 1 };
        let last_day = first_day + (chart_w as i64) * days_per_cell + 3;
        for day in first_day - 3..=last_day {
            let (y, m, d) = roadmap::civil_from_days(day);
            if d == 1 {
                const NAMES: [&str; 12] = [
                    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov",
                    "Dec",
                ];
                let label = format!("{} {y}", NAMES[(m - 1) as usize]);
                for (k, ch) in label.chars().enumerate() {
                    if let Some(c) = visible(zoom.x(day, origin) + k as i64) {
                        months[c] = (ch, Style::default().fg(Color::Cyan));
                    }
                }
            }
            // Monday ticks with the day of month (epoch day 0 = Thursday).
            if day.rem_euclid(7) == 4 && iterations.is_empty() {
                for (k, ch) in d.to_string().chars().enumerate() {
                    if let Some(c) = visible(zoom.x(day, origin) + k as i64) {
                        ticks[c] = (ch, dim);
                    }
                }
            }
        }
        // Today marker on both header rows.
        let today_cells: Vec<usize> = (zoom.x(today, origin)..=zoom.x_end(today, origin))
            .filter_map(visible)
            .collect();
        let today_style = Style::default().fg(Color::Black).bg(Color::Yellow);
        for &c in &today_cells {
            months[c] = (months[c].0, today_style);
            ticks[c] = (ticks[c].0, today_style);
        }

        let cells_to_line = |left: Vec<Span<'static>>, cells: &[(char, Style)]| -> Line<'static> {
            let mut spans = left;
            spans.push(Span::styled("│", dim));
            let mut run = String::new();
            let mut run_style = Style::default();
            for (ch, style) in cells {
                if *style != run_style && !run.is_empty() {
                    spans.push(Span::styled(std::mem::take(&mut run), run_style));
                }
                run_style = *style;
                run.push(*ch);
            }
            if !run.is_empty() {
                spans.push(Span::styled(run, run_style));
            }
            Line::from(spans)
        };
        let pad = |text: &str, w: usize| -> String {
            let t = truncate_to_width(text, w);
            format!("{t}{}", " ".repeat(w.saturating_sub(t.width())))
        };
        lines.push(cells_to_line(vec![Span::raw(" ".repeat(left_w))], &months));
        lines.push(cells_to_line(vec![Span::raw(" ".repeat(left_w))], &ticks));

        for (ri, (&idx, span)) in self
            .sorted
            .iter()
            .zip(&spans)
            .enumerate()
            .skip(self.roadmap_offset)
            .take(rows_h)
        {
            let item = &board.items[idx];
            let selected = show_selection && ri == self.table_row;
            let hl = theme::search_highlight_for(match_set, current_match, idx);
            let row_bg = if hl.is_active() {
                hl.bg
            } else if selected {
                Some(theme::LIST_SELECTION_BG)
            } else {
                None
            };
            let kind = item.kind();
            let icon_color = match kind {
                ItemKind::Issue => Color::Green,
                ItemKind::PullRequest => Color::Magenta,
                ItemKind::Draft | ItemKind::Other => Color::DarkGray,
            };
            let number = item.number().map(|n| format!("#{n} ")).unwrap_or_default();
            let head = format!("{} {number}", kind.icon());
            let title_w = left_w.saturating_sub(head.width());
            let mut left = vec![
                Span::styled(
                    head,
                    Style::default().fg(hl.fg_override.unwrap_or(icon_color)),
                ),
                Span::styled(
                    pad(item.title(), title_w),
                    match hl.fg_override {
                        Some(fg) => Style::default().fg(fg),
                        None => Style::default(),
                    },
                ),
            ];
            if selected {
                for s in &mut left {
                    s.style = s.style.add_modifier(Modifier::BOLD);
                }
            }
            let mut cells = vec![(' ', Style::default()); chart_w];
            for &c in &today_cells {
                cells[c] = ('┊', Style::default().fg(Color::Yellow));
            }
            if let Some((s0, e0)) = span {
                // Draft grey would sink into the (selection) background.
                let bar_color = match kind {
                    ItemKind::Draft | ItemKind::Other => Color::Gray,
                    _ => icon_color,
                };
                let bar = Style::default().fg(bar_color);
                for x in zoom.x(*s0, origin)..=zoom.x_end(*e0, origin) {
                    if let Some(c) = visible(x) {
                        cells[c] = ('█', bar);
                    }
                }
            }
            let mut line = cells_to_line(left, &cells);
            if let Some(bg) = row_bg {
                line = line.style(Style::default().bg(bg));
            }
            lines.push(line);
        }
        if spans.iter().all(Option::is_none) && !self.sorted.is_empty() {
            lines.push(Line::from(Span::styled(
                "  (no date or iteration values — bars appear when items get them)",
                Style::default().fg(theme::EMPTY_TEXT_FG),
            )));
        }
        f.render_widget(Paragraph::new(lines), inner);
    }
}

/// The `owner/repo` prefix of a card whose content lives in another
/// repository than `repo` (the one vig runs in); `None` for local items,
/// drafts and when the repository is unknown.
fn cross_repo_prefix<'a>(item: &'a ProjectItem, repo: Option<&str>) -> Option<&'a str> {
    let theirs = item.repository()?;
    let mine = repo?;
    (!theirs.eq_ignore_ascii_case(mine)).then_some(theirs)
}

/// The two lines of a card: `● #12 Title…` (`● owner/repo#12 Title…` for
/// an item of another repository) and the assignees and labels, padded to
/// `width`.
fn card_lines(
    item: &ProjectItem,
    repo: Option<&str>,
    width: usize,
    selected: bool,
    fg_override: Option<Color>,
) -> Vec<Line<'static>> {
    let kind = item.kind();
    let icon_color = match kind {
        ItemKind::Issue => Color::Green,
        ItemKind::PullRequest => Color::Magenta,
        ItemKind::Draft | ItemKind::Other => Color::DarkGray,
    };
    let fg = |c: Color| fg_override.unwrap_or(c);
    let bold = if selected {
        Modifier::BOLD
    } else {
        Modifier::empty()
    };
    let number = item.number().map(|n| format!("#{n} ")).unwrap_or_default();
    let prefix = cross_repo_prefix(item, repo)
        .filter(|_| !number.is_empty())
        .map(str::to_string)
        .unwrap_or_default();
    let head_w = 2 + prefix.width() + number.width();
    let title = truncate_to_width(item.title(), width.saturating_sub(head_w));
    let title_w = head_w + title.width();
    let mut title_style = Style::default().add_modifier(bold);
    if let Some(fg) = fg_override {
        title_style = title_style.fg(fg);
    }
    let first = vec![
        Span::styled(
            format!("{} ", kind.icon()),
            Style::default().fg(fg(icon_color)).add_modifier(bold),
        ),
        Span::styled(
            prefix,
            Style::default().fg(fg(Color::DarkGray)).add_modifier(bold),
        ),
        Span::styled(
            number,
            Style::default().fg(fg(Color::Yellow)).add_modifier(bold),
        ),
        Span::styled(title, title_style),
        Span::raw(" ".repeat(width.saturating_sub(title_w))),
    ];
    let mut parts: Vec<String> = item.assignees().iter().map(|a| format!("@{a}")).collect();
    if let Some(labels) = item.field_text("labels") {
        parts.push(labels);
    }
    let sub = truncate_to_width(&parts.join("  "), width.saturating_sub(2));
    let sub_w = 2 + sub.width();
    let second = vec![
        Span::styled(format!("  {sub}"), Style::default().fg(fg(Color::Gray))),
        Span::raw(" ".repeat(width.saturating_sub(sub_w))),
    ];
    vec![Line::from(first), Line::from(second)]
}

/// Cut `s` to at most `width` columns, ending in `…` when cut.
pub fn truncate_to_width(s: &str, width: usize) -> String {
    if s.width() <= width {
        return s.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut w = 0;
    for c in s.chars() {
        let cw = c.width().unwrap_or(0);
        if w + cw > width.saturating_sub(1) {
            break;
        }
        out.push(c);
        w += cw;
    }
    out.push('…');
    out
}

impl Pane<PaneEvent> for BoardPane {
    crate::impl_handle_key!(keymap);

    fn render(&mut self, f: &mut Frame, _ctx: &AppContext, shared: &PaneShared, area: Rect) {
        let is_focused = shared.focused_pane == self.pane_id;
        if self.mode == BoardMode::Board {
            self.layout_columns(area.width.saturating_sub(2));
        }
        let title = self.title();
        let block = theme::pane_block(&title, is_focused);
        let empty = if let Some(e) = &self.error {
            Some(format!("Error: {e}"))
        } else if self.board.is_none() && self.notice.is_some() {
            self.notice.clone()
        } else if self.loading && self.board.is_none() {
            Some("Loading...".to_string())
        } else if self.board.is_none() {
            Some("Select a project".to_string())
        } else if self.item_count() == 0 {
            Some("No items".to_string())
        } else {
            None
        };
        if let Some(message) = empty {
            theme::render_empty_list(f, area, block, &message);
            return;
        }
        let inner = block.inner(area);
        f.render_widget(block, area);
        if inner.width == 0 || inner.height == 0 {
            return;
        }
        let show_selection = is_focused
            || (shared.focused_pane == self.detail_pane_id && shared.previous_pane == self.pane_id);
        let (match_set, current_match) = theme::list_search_highlights(shared, self.pane_id);
        match self.mode {
            BoardMode::Board => self.render_board(
                f,
                inner,
                is_focused,
                show_selection,
                &match_set,
                current_match,
            ),
            BoardMode::Table => {
                self.render_table(f, inner, show_selection, &match_set, current_match)
            }
            BoardMode::Roadmap => {
                self.render_roadmap(f, inner, show_selection, &match_set, current_match)
            }
        }
    }

    /// Search covers titles and `#numbers`; a match is an item index.
    fn collect_search_matches(&self, _shared: &PaneShared, query: &str) -> Vec<SearchMatch> {
        match &self.board {
            Some(b) => pane::collect_list_search_matches(&b.items, query, |i| {
                format!(
                    "{} {}",
                    i.number().map(|n| format!("#{n}")).unwrap_or_default(),
                    i.title()
                )
            }),
            None => vec![],
        }
    }

    fn set_selected_idx(&mut self, idx: usize) {
        self.select_item(idx);
    }

    fn jump_to_match(&mut self, _shared: &PaneShared, search_match: &SearchMatch) {
        if let SearchMatch::ListEntry(idx) = search_match {
            self.select_item(*idx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::search::SearchState;
    use crate::projects::domain::types::tests::board;

    fn pane() -> BoardPane {
        let mut p = BoardPane::new(1, 2, Some(0));
        p.set_board(board());
        p
    }

    pub(super) fn shared() -> PaneShared {
        PaneShared {
            focused_pane: 1,
            previous_pane: 0,
            search: SearchState::new(),
        }
    }

    fn selected_id(p: &BoardPane) -> &str {
        p.selected_item().map(|i| i.id.as_str()).unwrap_or("")
    }

    pub(super) fn view(n: u64, name: &str) -> ProjectView {
        ProjectView {
            number: n,
            name: name.into(),
            layout: crate::projects::domain::types::ViewLayout::Table,
            filter: None,
            group_by: vec![],
            vertical_group_by: vec![],
            sort_by: vec![],
            visible_fields: vec![],
            local: false,
            initial: false,
        }
    }

    #[test]
    fn board_view_columns_come_from_vertical_group_by() {
        let mut p = pane();
        let mut b = board();
        let mut v = view(1, "By priority");
        v.layout = ViewLayout::Board;
        v.vertical_group_by = vec!["Priority".into()];
        b.views = vec![v];
        p.set_board(b);
        assert_eq!(p.mode, BoardMode::Board);
        let names: Vec<&str> = p.lanes[0].columns.iter().map(|c| c.name.as_str()).collect();
        // P1 / P2 in option order (kept even when empty), unset items last.
        assert_eq!(names, vec!["P1", "P2", "No priority"]);
        assert!(p.title().contains("[columns: Priority]"));
        let total: usize = p.lanes[0].columns.iter().map(|c| c.items.len()).sum();
        assert_eq!(total, p.item_count());
    }

    #[test]
    fn board_view_group_by_builds_swimlanes_and_j_crosses_them() {
        let mut p = pane();
        let sh = shared();
        let mut b = board();
        let mut v = view(1, "Lanes");
        v.layout = ViewLayout::Board;
        v.group_by = vec!["Priority".into()];
        b.views = vec![v];
        p.set_board(b);
        // One lane per priority value, unset items in "No priority".
        let names: Vec<&str> = p.lanes.iter().filter_map(|l| l.name.as_deref()).collect();
        assert_eq!(names, vec!["P1", "P2", "No priority"]);
        assert!(p.lanes.iter().all(|l| l.columns.len() == p.column_count()));

        // j at the bottom of a column crosses into the next lane that has
        // a card in that column.
        p.lane = 0;
        p.col = p.lanes[0]
            .columns
            .iter()
            .position(|c| !c.items.is_empty())
            .unwrap();
        p.row = p.lanes[0].columns[p.col].items.len() - 1;
        let before = (p.lane, p.selected_index());
        p.execute(&sh, BoardAction::Nav(NavAction::MoveDown));
        if p.lanes
            .iter()
            .skip(1)
            .any(|l| !l.columns[p.col].items.is_empty())
        {
            assert_ne!(p.lane, before.0);
            assert_eq!(p.row, 0);
        }

        // Space collapses the lane and moves the selection out of it.
        p.lane = 0;
        p.col = p.lanes[0]
            .columns
            .iter()
            .position(|c| !c.items.is_empty())
            .unwrap();
        p.row = 0;
        p.execute(&sh, BoardAction::ToggleLane);
        assert!(p.lanes[0].collapsed);
        assert_ne!(p.lane, 0);

        // Jumping to an item inside the collapsed lane expands it again.
        let hidden = p.lanes[0]
            .columns
            .iter()
            .flat_map(|c| c.items.iter())
            .next()
            .copied()
            .unwrap();
        p.select_item(hidden);
        assert!(!p.lanes[0].collapsed);
        assert_eq!(p.lane, 0);
    }

    #[test]
    fn board_view_sort_orders_cards_inside_columns() {
        let mut p = pane();
        let mut b = board();
        let mut v = view(1, "Sorted");
        v.layout = ViewLayout::Board;
        v.sort_by = vec![crate::projects::domain::types::ViewSort {
            field: "Title".into(),
            desc: false,
        }];
        b.views = vec![v];
        p.set_board(b);
        for col in &p.lanes[0].columns {
            let titles: Vec<String> = col
                .items
                .iter()
                .map(|&i| p.board.as_ref().unwrap().items[i].title().to_lowercase())
                .collect();
            let mut sorted = titles.clone();
            sorted.sort();
            assert_eq!(titles, sorted, "column {}", col.name);
        }
    }

    #[test]
    fn roadmap_view_drives_mode_zoom_and_scroll() {
        let mut p = pane();
        let sh = shared();
        let mut b = board();
        let mut v = view(1, "Timeline");
        v.layout = ViewLayout::Roadmap;
        b.views = vec![v];
        p.set_board(b);
        assert_eq!(p.mode, BoardMode::Roadmap);
        assert!(p.title().starts_with("Roadmap"));
        assert!(p.title().contains("[week]"));

        // j / k move over the sorted rows.
        let before = p.selected_index();
        p.execute(&sh, BoardAction::Nav(NavAction::MoveDown));
        assert_ne!(p.selected_index(), before);

        // + / - change the zoom (clamped at both ends), resetting scroll.
        p.execute(&sh, BoardAction::NextColumn);
        assert_eq!(p.roadmap_scroll, Some(7));
        p.execute(&sh, BoardAction::PrevColumn);
        p.execute(&sh, BoardAction::PrevColumn);
        assert_eq!(p.roadmap_scroll, Some(0)); // never negative
        p.execute(&sh, BoardAction::ZoomIn);
        assert!(p.title().contains("[day]"));
        assert_eq!(p.roadmap_scroll, None);
        p.execute(&sh, BoardAction::ZoomOut);
        p.execute(&sh, BoardAction::ZoomOut);
        assert!(p.title().contains("[month]"));
        p.execute(&sh, BoardAction::ZoomOut);
        assert!(p.title().contains("[month]"));

        // t leaves for the table and returns to the roadmap.
        p.execute(&sh, BoardAction::ToggleTable);
        assert_eq!(p.mode, BoardMode::Table);
        p.execute(&sh, BoardAction::ToggleTable);
        assert_eq!(p.mode, BoardMode::Roadmap);
    }

    #[test]
    fn view_filter_hides_items_in_every_layout_and_reports_unsupported() {
        let mut p = pane();
        let sh = shared();
        let mut b = board();
        let total = b.items.len();
        let mut v = view(1, "Open work");
        v.filter = Some("-status:Done updated:>2026-01-01".into());
        b.views = vec![v];
        p.set_board(b);
        // Table rows and the board's lane both drop the Done items.
        let done = board()
            .items
            .iter()
            .filter(|i| i.status.as_deref() == Some("Done"))
            .count();
        assert!(done > 0);
        assert_eq!(p.item_count(), total - done);
        assert_eq!(p.filtered_out(), done);
        let lane_total: usize = p.lanes[0].columns.iter().map(|c| c.items.len()).sum();
        assert_eq!(lane_total, total - done);
        // The range token is reported, not applied.
        assert_eq!(
            p.filter_notice().as_deref(),
            Some("⚠ filter: unsupported \"updated:>2026-01-01\"")
        );
        // Switching to a view without a filter shows everything again.
        let mut b = board();
        b.views = vec![view(1, "Open work"), view(2, "All")];
        b.views[0].filter = Some("-status:Done".into());
        p.set_board(b);
        p.execute(&sh, BoardAction::NextView);
        assert_eq!(p.item_count(), total);
        assert_eq!(p.filtered_out(), 0);
        assert_eq!(p.filter_notice(), None);

        // `assignee:@me` resolves once the login is known.
        let mut b = board();
        b.views = vec![view(1, "Mine")];
        b.views[0].filter = Some("assignee:@me".into());
        p.set_board(b);
        assert_eq!(p.item_count(), 0);
        p.set_viewer(Some("td72".into()));
        assert!(p.item_count() > 0);
    }

    #[test]
    fn table_view_drives_mode_columns_sort_and_groups() {
        let mut p = pane();
        let sh = shared();
        let mut b = board();
        let mut table_view = view(1, "Sprint");
        table_view.visible_fields = vec!["Title".into(), "Priority".into()];
        table_view.sort_by = vec![crate::projects::domain::types::ViewSort {
            field: "Priority".into(),
            desc: true,
        }];
        table_view.group_by = vec!["Status".into()];
        let mut board_view = view(2, "By status");
        board_view.layout = ViewLayout::Board;
        b.views = vec![table_view, board_view];
        p.set_board(b);

        // The Table view puts the pane in table mode with its columns,
        // its descending initial sort and its grouping.
        assert_eq!(p.mode, BoardMode::Table);
        let headers: Vec<&str> = p.table_cols.iter().map(TableColumn::header).collect();
        assert_eq!(headers, vec!["#", "Title", "Priority"]);
        assert_eq!(p.sort_label(), Some("Priority"));
        assert!(p.sort_desc);
        assert!(!p.groups.is_empty());
        assert!(p.title().contains("by Status"));

        // s picks the next sort column and resets the direction.
        p.execute(&sh, BoardAction::CycleSort);
        assert!(!p.sort_desc);

        // Switching to the Board view goes back to the kanban with the
        // default (all-fields) table columns.
        p.execute(&sh, BoardAction::NextView);
        assert_eq!(p.mode, BoardMode::Board);
        assert!(p.groups.is_empty());
        assert!(p.table_cols.iter().any(|c| c.header() == "Estimate"));
    }

    #[test]
    fn group_headers_offset_the_visual_selection() {
        let mut p = pane();
        let mut b = board();
        let mut v = view(1, "Grouped");
        v.group_by = vec!["Status".into()];
        b.views = vec![v];
        p.set_board(b);
        // Row 0 sits under the first group header; a row in the second
        // group has two headers above it.
        assert_eq!(p.headers_before(0), 1);
        let first_group = p.groups[0].1;
        assert_eq!(p.headers_before(first_group), 2);
    }

    #[test]
    fn v_cycles_saved_views_and_survives_refresh() {
        let mut p = pane();
        let sh = shared();
        // No views: v explains instead of cycling.
        let ev = p.execute(&sh, BoardAction::NextView);
        assert!(matches!(&ev[0], PaneEvent::StatusMessage(m) if m.contains("no saved views")));

        let mut b = board();
        b.views = vec![view(1, "All"), view(2, "Sprint"), view(3, "Roadmap")];
        p.set_board(b.clone());
        assert_eq!(p.view_label(), Some(("All", 1, 3)));
        p.execute(&sh, BoardAction::NextView);
        assert_eq!(p.view_label(), Some(("Sprint", 2, 3)));
        p.execute(&sh, BoardAction::PrevView);
        p.execute(&sh, BoardAction::PrevView);
        assert_eq!(p.view_label(), Some(("Roadmap", 3, 3)));

        // A refresh of the same project keeps the shown view…
        p.execute(&sh, BoardAction::NextView); // wraps to All
        p.execute(&sh, BoardAction::NextView); // Sprint
        p.set_board(b.clone());
        assert_eq!(p.view_label(), Some(("Sprint", 2, 3)));
        // …another project starts over on its first view.
        let mut other = board();
        other.number = 99;
        other.views = vec![view(1, "Only")];
        p.set_board(other);
        assert_eq!(p.view_label(), Some(("Only", 1, 1)));
    }

    /// A local `default` view opens first; fields it names that the board
    /// lacks are reported, and a saved view carries neither mark.
    #[test]
    fn a_local_default_view_opens_first_and_missing_fields_are_noticed() {
        let mut p = pane();
        let sh = shared();
        let mut b = board();
        b.number = 5; // another project than the helper's: opens fresh
        let mut mine = view(2, "Mine");
        mine.local = true;
        mine.initial = true;
        mine.layout = crate::projects::domain::types::ViewLayout::Board;
        mine.group_by = vec!["Ghost".into()];
        mine.sort_by = vec![crate::projects::domain::types::ViewSort {
            field: "Title".into(),
            desc: false,
        }];
        // "Ghost" twice, apart: listed once.
        mine.visible_fields = vec!["Status".into(), "Phantom".into(), "Ghost".into()];
        b.views = vec![view(1, "All"), mine];
        p.set_board(b);
        assert_eq!(p.view_label(), Some(("Mine", 2, 2)));
        assert!(p.current_view_is_local());
        assert_eq!(p.mode, BoardMode::Board);
        assert_eq!(
            p.view_notice().as_deref(),
            Some("⚠ view \"Mine\": no field \"Ghost\", \"Phantom\"")
        );
        p.execute(&sh, BoardAction::PrevView);
        assert_eq!(p.view_label(), Some(("All", 1, 2)));
        assert!(!p.current_view_is_local());
        assert_eq!(p.view_notice(), None);
    }

    #[test]
    fn columns_and_cards_navigate_with_hjkl() {
        let mut p = pane();
        let sh = shared();
        assert_eq!(p.column_count(), 5);
        assert_eq!(p.item_count(), 5);
        assert!(p.truncated());
        // Starts on the first column (Todo) and its first card.
        assert_eq!(selected_id(&p), "I4");
        let ev = p.execute(&sh, BoardAction::NextColumn);
        assert!(matches!(ev.as_slice(), [PaneEvent::SelectionChanged]));
        assert_eq!(selected_id(&p), "I2", "In Progress");
        p.execute(&sh, BoardAction::NextColumn);
        assert_eq!(selected_id(&p), "I1", "Done");
        // Moving down inside a one-card column is a no-op.
        assert!(p
            .execute(&sh, BoardAction::Nav(NavAction::MoveDown))
            .is_empty());
        // Right stops at the last column.
        p.execute(&sh, BoardAction::NextColumn);
        p.execute(&sh, BoardAction::NextColumn);
        assert_eq!(selected_id(&p), "I3", "No status");
        assert!(p.execute(&sh, BoardAction::NextColumn).is_empty());
        p.execute(&sh, BoardAction::PrevColumn);
        assert_eq!(selected_id(&p), "I5", "Blocked");
        // Enter focuses the detail, Esc goes back to the project list.
        let ev = p.execute(&sh, BoardAction::OpenDetail);
        assert!(matches!(ev.as_slice(), [PaneEvent::SetFocus(2)]));
        let ev = p.execute(&sh, BoardAction::Esc);
        assert!(matches!(ev.as_slice(), [PaneEvent::SetFocus(0)]));
        // Without a placed list pane Esc does nothing (search still clears).
        let mut p = BoardPane::new(1, 2, None);
        p.set_board(board());
        assert!(p.execute(&sh, BoardAction::Esc).is_empty());
        let mut searching = shared();
        searching.search.query = Some("x".into());
        let ev = p.execute(&searching, BoardAction::Esc);
        assert!(matches!(ev.as_slice(), [PaneEvent::ClearSearch]));
    }

    #[test]
    fn table_mode_keeps_the_selected_item_and_sorts() {
        let mut p = pane();
        let sh = shared();
        p.execute(&sh, BoardAction::NextColumn);
        assert_eq!(selected_id(&p), "I2");
        assert!(p.execute(&sh, BoardAction::ToggleTable).is_empty());
        assert_eq!(p.mode, BoardMode::Table);
        assert_eq!(selected_id(&p), "I2", "same item after the toggle");
        assert_eq!(p.sort_label(), Some("#"));
        // Sorted by number: #114, #119, #124, then the items without one.
        assert_eq!(p.sorted, [0, 3, 1, 2, 4]);
        assert_eq!(p.table_row, 2);
        p.execute(&sh, BoardAction::CycleSort);
        assert_eq!(p.sort_label(), Some("Title"));
        assert_eq!(selected_id(&p), "I2", "sort keeps the selection");
        // `l` also advances the sort column in table mode; `h` goes back.
        p.execute(&sh, BoardAction::NextColumn);
        assert_eq!(p.sort_label(), Some("Assignees"));
        p.execute(&sh, BoardAction::PrevColumn);
        assert_eq!(p.sort_label(), Some("Title"));
        // j / k walk the sorted rows.
        p.execute(&sh, BoardAction::Nav(NavAction::JumpTop));
        assert_eq!(selected_id(&p), "I1", "Config… first by title");
        let ev = p.execute(&sh, BoardAction::Nav(NavAction::MoveDown));
        assert!(matches!(ev.as_slice(), [PaneEvent::SelectionChanged]));
        assert_eq!(selected_id(&p), "I2");
        // Back to the board: the column / card of that item is selected.
        p.execute(&sh, BoardAction::ToggleTable);
        assert_eq!(p.mode, BoardMode::Board);
        assert_eq!((p.col, p.row), (1, 0));
        // `s` outside table mode is a no-op.
        assert!(p.execute(&sh, BoardAction::CycleSort).is_empty());
        assert_eq!(p.sort_label(), Some("Title"));
    }

    #[test]
    fn open_browser_uses_the_item_url_or_the_project_url() {
        let mut p = pane();
        let sh = shared();
        p.set_project_url(Some("https://github.com/users/td72/projects/2".into()));
        let ev = p.execute(&sh, BoardAction::OpenBrowser);
        assert!(matches!(ev.as_slice(), [PaneEvent::OpenUrl(u)] if u.ends_with("/issues/119")));
        // The draft has no URL of its own: open the project.
        p.jump_to_match(&sh, &SearchMatch::ListEntry(2));
        assert_eq!(selected_id(&p), "I3");
        let ev = p.execute(&sh, BoardAction::OpenBrowser);
        assert!(matches!(ev.as_slice(), [PaneEvent::OpenUrl(u)] if u.ends_with("/projects/2")));
    }

    #[test]
    fn search_matches_titles_and_numbers_across_columns() {
        let p = pane();
        let sh = shared();
        assert_eq!(p.collect_search_matches(&sh, "projects").len(), 2);
        let m = p.collect_search_matches(&sh, "#124");
        assert!(matches!(m.as_slice(), [SearchMatch::ListEntry(1)]));
        assert!(p.collect_search_matches(&sh, "nothing").is_empty());
    }

    #[test]
    fn refresh_keeps_the_selection_by_item_id() {
        let mut p = pane();
        let sh = shared();
        p.execute(&sh, BoardAction::NextColumn);
        p.execute(&sh, BoardAction::NextColumn);
        assert_eq!(selected_id(&p), "I1");
        // The item moved from Done back to Todo.
        let mut b = board();
        b.items[0].status = Some("Todo".into());
        p.set_board(b);
        assert_eq!(selected_id(&p), "I1");
        assert_eq!(p.col, 0);
        // A board without that item clamps instead.
        let mut b = board();
        b.items.clear();
        p.set_board(b);
        assert!(p.selected_item().is_none());
        assert_eq!(p.column_count(), 3, "status options stay as columns");
    }

    fn text_of(lines: &[Line<'static>]) -> Vec<String> {
        lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.to_string()).collect())
            .collect()
    }

    #[test]
    fn cards_truncate_and_pad_to_the_column_width() {
        let b = board();
        let lines = card_lines(&b.items[0], Some("td72/vig"), 20, false, None);
        let text = text_of(&lines);
        assert_eq!(text[0].width(), 20);
        assert!(text[0].starts_with("● #114 Config"));
        assert!(text[0].ends_with('…'));
        assert_eq!(text[1].trim_end(), "  @td72  enhancement");
        // A draft shows no number and has an empty second line.
        let lines = card_lines(&b.items[2], Some("td72/vig"), 40, true, None);
        let first = &text_of(&lines)[0];
        assert!(first.starts_with("✎ Record the Projects demo tape"));
        assert_eq!(lines[1].spans[0].content.trim(), "");
        assert_eq!(truncate_to_width("日本語テキスト", 7), "日本語…");
        assert_eq!(truncate_to_width("short", 10), "short");
        assert_eq!(truncate_to_width("x", 0), "");
    }

    #[test]
    fn cards_of_other_repositories_carry_a_dimmed_prefix() {
        let b = board();
        // Same repository (any case): the bare number.
        let lines = card_lines(&b.items[0], Some("TD72/Vig"), 40, false, None);
        assert!(text_of(&lines)[0].starts_with("● #114 Config"));
        assert_eq!(lines[0].spans[1].content, "");
        // Another repository: `owner/repo#n`, the prefix dimmed.
        let lines = card_lines(&b.items[0], Some("td72/other"), 40, false, None);
        let text = text_of(&lines);
        assert!(text[0].starts_with("● td72/vig#114 Config"), "{}", text[0]);
        assert_eq!(text[0].width(), 40);
        assert_eq!(lines[0].spans[1].content, "td72/vig");
        assert_eq!(lines[0].spans[1].style.fg, Some(Color::DarkGray));
        assert_eq!(lines[0].spans[2].content, "#114 ");
        // Unknown current repository or a draft: no prefix.
        let lines = card_lines(&b.items[0], None, 40, false, None);
        assert!(text_of(&lines)[0].starts_with("● #114 Config"));
        let lines = card_lines(&b.items[2], Some("td72/other"), 40, false, None);
        assert!(text_of(&lines)[0].starts_with("✎ Record"));
        assert_eq!(
            cross_repo_prefix(&b.items[0], Some("acme/x")),
            Some("td72/vig")
        );
        assert_eq!(cross_repo_prefix(&b.items[0], Some("td72/vig")), None);
        assert_eq!(cross_repo_prefix(&b.items[2], Some("acme/x")), None);
    }

    #[test]
    fn a_notice_replaces_the_missing_board() {
        let mut p = BoardPane::new(1, 2, None);
        assert!(p.notice().is_none());
        p.set_notice(Some("nothing linked".into()));
        assert_eq!(p.notice().map(String::as_str), Some("nothing linked"));
        // A board wins over the notice once it is there.
        p.set_board(board());
        assert_eq!(p.item_count(), 5);
        p.clear();
        assert!(p.board.is_none());
        assert!(p.notice().is_some(), "clear() keeps the notice");
    }
}

#[cfg(test)]
mod roadmap_render_tests {
    use super::tests::{shared, view};
    use super::*;
    use crate::projects::domain::types::tests::board;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn selected_roadmap_row_keeps_its_bar() {
        let mut p = BoardPane::new(1, 2, Some(0));
        let mut b = board();
        let mut v = view(1, "Timeline");
        v.layout = ViewLayout::Roadmap;
        b.views = vec![v];
        // Give every item a span around today so bars land in view.
        let t = roadmap::today();
        let (y, m, d) = roadmap::civil_from_days(t);
        let date = format!("{y:04}-{m:02}-{d:02}");
        let (y2, m2, d2) = roadmap::civil_from_days(t + 5);
        let date2 = format!("{y2:04}-{m2:02}-{d2:02}");
        for item in &mut b.items {
            item.fields
                .insert("start date".into(), serde_json::Value::String(date.clone()));
            item.fields.insert(
                "target date".into(),
                serde_json::Value::String(date2.clone()),
            );
        }
        b.fields.push(ProjectField {
            id: "FT".into(),
            name: "Target date".into(),
            kind: "ProjectV2Field".into(),
            options: vec![],
        });
        p.set_board(b);
        assert_eq!(p.mode, BoardMode::Roadmap);
        let sh = shared();

        let mut term = Terminal::new(TestBackend::new(100, 20)).unwrap();
        term.draw(|f| {
            let area = f.area();
            let ctx = crate::core::app::AppContext {
                should_quit: false,
                active_page: 0,
                page_labels: vec![],
                page_keys: vec![],
                show_help: false,
                status_message: None,
                error_dialog: None,
                workdir: std::path::PathBuf::new(),
                needs_full_redraw: false,
                last_input: std::time::Instant::now(),
                auto_refresh: true,
            };
            p.render(f, &ctx, &sh, area);
        })
        .unwrap();
        let buf = term.backend().buffer().clone();
        let row_text = |row: u16| -> String {
            (0..buf.area.width)
                .map(|x| buf[(x, row)].symbol().to_string())
                .collect()
        };
        // Row 0/1 headers (inside the border), first item row selected.
        let all: Vec<String> = (0..20).map(row_text).collect();
        let selected_row = all
            .iter()
            .find(|l| l.contains(&board().items[0].title()[..10]))
            .expect("selected item row rendered");
        assert!(
            selected_row.contains('█'),
            "selected row lost its bar: {selected_row:?}"
        );
    }
}
