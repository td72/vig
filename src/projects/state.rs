//! The Projects page: the repository owner's GitHub Projects (v2) as a
//! kanban board with an item detail. Read-only: only `gh repo view`,
//! `gh project list / field-list / item-list`, `gh issue view`, `gh pr view`
//! and a GraphQL read are ever run.

use crate::core::app::{AppContext, PageState};
use crate::core::config::{Config, LoadedPageConfig};
use crate::core::keymap::{Keymap, ViewAction};
use crate::core::layout::{split_page_frame, PageLayoutConfig};
use crate::core::page::PageAction;
use crate::core::pane::{self, Pane, PaneEvent, PaneSet, PaneShared};
use crate::core::search::SearchState;
use crate::core::ui::status_bar;
use crate::projects::domain::types::{order_projects, Board, Project, ProjectListCache, RepoInfo};
use crate::projects::domain::{client, disk_cache};
use crate::projects::panes::board::{BoardAction, BoardPane};
use crate::projects::panes::detail::{DetailAction, DetailPane, ItemDetail};
use crate::projects::panes::projects::{ProjectsAction, ProjectsPane};
use anyhow::Result;
use crossterm::event::KeyEvent;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Coming back to the page after this long re-fetches the project list.
pub const LIST_REFRESH_AFTER: Duration = Duration::from_secs(300);

/// Notice shown instead of the panes when the token lacks the scope.
pub const SCOPE_NOTICE: &str = "gh needs the project scope: run `gh auth refresh -s project`";

/// Pane IDs resolved from the KDL config at construction time.
#[derive(Debug, Clone, Copy)]
pub struct ProjectsPaneIds {
    pub projects: usize,
    pub board: usize,
    pub detail: usize,
}

impl ProjectsPaneIds {
    fn from_config(cfg: &LoadedPageConfig) -> Self {
        Self {
            projects: cfg.resolve_id_expect("projects"),
            board: cfg.resolve_id_expect("board"),
            detail: cfg.resolve_id_expect("detail"),
        }
    }
}

pub enum ProjectsBgMessage {
    /// `gh repo view`: the owner to list and the linked project numbers.
    Repo(Result<RepoInfo, String>),
    ProjectList {
        owner: String,
        linked: Vec<u64>,
        result: Result<Vec<Project>, String>,
    },
    Board {
        number: u64,
        result: Result<Board, String>,
    },
    ItemDetail {
        key: String,
        result: Result<ItemDetail, String>,
    },
}

pub struct ProjectsPanes {
    pub projects: ProjectsPane,
    pub board: BoardPane,
    pub detail: DetailPane,
    pub ids: ProjectsPaneIds,
}

impl PaneSet for ProjectsPanes {
    fn get_mut(&mut self, idx: usize) -> Option<&mut dyn Pane<PaneEvent>> {
        if idx == self.ids.projects {
            Some(&mut self.projects)
        } else if idx == self.ids.board {
            Some(&mut self.board)
        } else if idx == self.ids.detail {
            Some(&mut self.detail)
        } else {
            None
        }
    }
}

pub struct ProjectsState {
    pub pane: PaneShared,
    pub panes: ProjectsPanes,
    /// `None` until `gh repo view` has answered.
    pub gh_available: Option<bool>,
    pub gh_error: Option<String>,
    /// The token lacks the `project` scope: the panes are replaced by a notice.
    pub scope_missing: bool,
    owner: Option<String>,
    linked: Vec<u64>,
    bg_rx: Option<mpsc::Receiver<ProjectsBgMessage>>,
    bg_tx: Option<mpsc::Sender<ProjectsBgMessage>>,
    initialized: bool,
    last_list_refresh: Option<Instant>,
    /// Boards by project number, filled from disk and fetches.
    board_cache: HashMap<u64, Board>,
    /// Project numbers with a board fetch in flight.
    board_inflight: HashSet<u64>,
    /// Read / write the disk cache (tests turn it off).
    use_disk_cache: bool,
    layout_config: PageLayoutConfig,
    view_keymap: Keymap<ViewAction>,
}

impl pane::PageLayout for ProjectsState {
    type Panes = ProjectsPanes;
    fn page_parts_mut(
        &mut self,
    ) -> (
        &mut PaneShared,
        &mut Self::Panes,
        &Keymap<ViewAction>,
        &PageLayoutConfig,
    ) {
        (
            &mut self.pane,
            &mut self.panes,
            &self.view_keymap,
            &self.layout_config,
        )
    }
}

impl ProjectsState {
    pub fn new(cfg: &Config) -> Result<Self> {
        let page_cfg = cfg.projects_page()?;
        let ids = ProjectsPaneIds::from_config(&page_cfg);
        // Validates the bind declarations (projects → board, board → detail).
        let _ = page_cfg.resolve_select_bindings();

        let projects_km = page_cfg.keymap::<ProjectsAction>("projects")?;
        let board_km = page_cfg.keymap::<BoardAction>("board")?;
        let detail_km = page_cfg.keymap::<DetailAction>("detail")?;
        let view_km = page_cfg.keymap::<ViewAction>("view")?;

        let mut projects = ProjectsPane::new(ids.projects, ids.board);
        projects.set_keymap(projects_km);
        let mut board = BoardPane::new(ids.board, ids.detail, ids.projects);
        board.set_keymap(board_km);
        let mut detail = DetailPane::new(ids.detail);
        detail.set_keymap(detail_km);

        Ok(Self {
            pane: PaneShared {
                focused_pane: ids.projects,
                previous_pane: ids.projects,
                search: SearchState::new(),
            },
            panes: ProjectsPanes {
                projects,
                board,
                detail,
                ids,
            },
            gh_available: None,
            gh_error: None,
            scope_missing: false,
            owner: None,
            linked: Vec::new(),
            bg_rx: None,
            bg_tx: None,
            initialized: false,
            last_list_refresh: None,
            board_cache: HashMap::new(),
            board_inflight: HashSet::new(),
            use_disk_cache: true,
            layout_config: page_cfg.layout,
            view_keymap: view_km,
        })
    }

    /// First switch to the page: show the cached list, then resolve the
    /// owner and fetch the projects.
    fn initialize(&mut self) {
        if self.initialized {
            return;
        }
        self.initialized = true;
        let (tx, rx) = mpsc::channel();
        self.bg_tx = Some(tx);
        self.bg_rx = Some(rx);
        let cached = if self.use_disk_cache {
            disk_cache::load_project_list()
        } else {
            None
        };
        if let Some(cache) = cached {
            self.owner = Some(cache.owner);
            self.linked = cache.linked.clone();
            self.panes
                .projects
                .set_projects(order_projects(cache.projects, &cache.linked));
            self.sync_board();
        }
        self.spawn_list();
    }

    /// `gh repo view` then `gh project list`, on one worker thread.
    fn spawn_list(&mut self) {
        let Some(tx) = self.bg_tx.clone() else {
            return;
        };
        self.last_list_refresh = Some(Instant::now());
        self.panes.projects.set_loading(true);
        std::thread::spawn(move || {
            let info = client::repo_info();
            let ok = info
                .as_ref()
                .ok()
                .map(|i| (i.owner.login.clone(), i.linked_numbers()));
            let _ = tx.send(ProjectsBgMessage::Repo(info));
            if let Some((owner, linked)) = ok {
                let result = client::list_projects(&owner);
                let _ = tx.send(ProjectsBgMessage::ProjectList {
                    owner,
                    linked,
                    result,
                });
            }
        });
    }

    fn spawn_board(&mut self, number: u64) {
        let (Some(tx), Some(owner)) = (self.bg_tx.clone(), self.owner.clone()) else {
            return;
        };
        if !self.board_inflight.insert(number) {
            return;
        }
        self.panes.board.set_loading(true);
        std::thread::spawn(move || {
            let result = client::fetch_board(&owner, number);
            let _ = tx.send(ProjectsBgMessage::Board { number, result });
        });
    }

    pub fn is_loading(&self) -> bool {
        self.panes.projects.is_loading()
            || self.panes.board.is_loading()
            || self.panes.detail.is_loading()
    }

    /// `r`: re-fetch the list, the shown board and the shown item.
    fn refresh(&mut self) {
        self.gh_error = None;
        self.scope_missing = false;
        if self.gh_available == Some(false) {
            self.gh_available = None;
        }
        self.board_cache.clear();
        self.panes.detail.clear_cache();
        self.spawn_list();
        if let Some(number) = self.panes.projects.selected_number() {
            self.spawn_board(number);
        }
        if let Some(tx) = self.bg_tx.clone() {
            self.panes.detail.reload_current(&tx);
        }
    }

    /// Point the board at the selected project: from memory, else from
    /// disk while a fetch runs.
    fn sync_board(&mut self) {
        let Some(project) = self.panes.projects.selected().cloned() else {
            self.panes.board.clear();
            self.panes.detail.show_none();
            return;
        };
        let number = project.number;
        self.panes.board.set_project_url(Some(project.url.clone()));
        let shown = self.panes.board.board.as_ref().map(|b| b.number);
        if shown != Some(number) {
            let cached = self.board_cache.get(&number).cloned().or_else(|| {
                self.use_disk_cache
                    .then(|| disk_cache::load_board(number))
                    .flatten()
            });
            match cached {
                Some(board) => self.panes.board.set_board(board),
                None => self.panes.board.clear(),
            }
        }
        if !self.board_cache.contains_key(&number) {
            self.spawn_board(number);
        }
        self.sync_detail();
    }

    /// The detail pane follows the board's selection.
    fn sync_detail(&mut self) {
        let Some(tx) = self.bg_tx.clone() else {
            return;
        };
        let fields = self
            .panes
            .board
            .board
            .as_ref()
            .map(|b| b.fields.clone())
            .unwrap_or_default();
        match self.panes.board.selected_item().cloned() {
            Some(item) => self.panes.detail.load(&item, &fields, &tx),
            None => self.panes.detail.show_none(),
        }
    }

    /// A selection moved in `pane_id`: refresh what follows it.
    fn follow(&mut self, pane_id: usize) {
        let ids = self.panes.ids;
        if pane_id == ids.projects {
            self.sync_board();
        } else if pane_id == ids.board {
            self.sync_detail();
        }
    }

    fn note_error(&mut self, e: String) {
        if client::is_scope_error(&e) {
            self.scope_missing = true;
        } else if self.gh_error.is_none() {
            self.gh_error = Some(e);
        }
    }

    fn drain_bg_messages(&mut self) {
        let messages: Vec<_> = match &self.bg_rx {
            Some(rx) => rx.try_iter().collect(),
            None => return,
        };
        for msg in messages {
            match msg {
                ProjectsBgMessage::Repo(result) => match result {
                    Ok(info) => {
                        self.gh_available = Some(true);
                        self.owner = Some(info.owner.login.clone());
                        self.linked = info.linked_numbers();
                    }
                    Err(e) => {
                        self.panes.projects.set_loading(false);
                        if client::is_gh_missing(&e) {
                            self.gh_available = Some(false);
                            self.gh_error = Some(e);
                        } else {
                            self.gh_available = Some(true);
                            self.note_error(e);
                        }
                    }
                },
                ProjectsBgMessage::ProjectList {
                    owner,
                    linked,
                    result,
                } => {
                    self.panes.projects.set_loading(false);
                    match result {
                        Ok(projects) => {
                            if self.use_disk_cache {
                                disk_cache::save_project_list(&ProjectListCache {
                                    owner: owner.clone(),
                                    linked: linked.clone(),
                                    projects: projects.clone(),
                                });
                            }
                            self.owner = Some(owner);
                            self.linked = linked;
                            let ordered = order_projects(projects, &self.linked);
                            self.panes.projects.set_projects(ordered);
                            self.sync_board();
                        }
                        Err(e) => self.note_error(e),
                    }
                }
                ProjectsBgMessage::Board { number, result } => {
                    self.board_inflight.remove(&number);
                    self.panes.board.set_loading(false);
                    let current = self.panes.projects.selected_number() == Some(number);
                    match result {
                        Ok(board) => {
                            if self.use_disk_cache {
                                disk_cache::save_board(&board);
                            }
                            self.board_cache.insert(number, board.clone());
                            if current {
                                self.panes.board.set_board(board);
                                self.sync_detail();
                            }
                        }
                        Err(e) => {
                            if client::is_scope_error(&e) {
                                self.scope_missing = true;
                            } else if current && self.panes.board.board.is_none() {
                                self.panes.board.set_error(Some(e));
                            } else {
                                self.note_error(e);
                            }
                        }
                    }
                }
                ProjectsBgMessage::ItemDetail { key, result } => {
                    self.panes.detail.apply(&key, result);
                }
            }
        }
    }

    fn process_events(
        &mut self,
        ctx: &mut AppContext,
        events: Vec<PaneEvent>,
    ) -> Result<PageAction> {
        let ids = self.panes.ids;
        for event in events {
            if pane::process_common_event(&mut self.pane, ctx, &event) {
                continue;
            }
            match event {
                PaneEvent::SetFocus(id) if id == ids.board || id == ids.detail => {
                    // Entering the board / detail: make sure they show the
                    // current selection (the list may have moved silently).
                    self.follow(self.pane.previous_pane);
                }
                PaneEvent::SelectionChanged => {
                    self.follow(self.pane.focused_pane);
                }
                PaneEvent::JumpToMatch(forward) => {
                    if let Some(origin) =
                        self.pane
                            .jump_to_search_match(&mut self.panes, ctx, forward)
                    {
                        self.follow(origin);
                    }
                }
                _ => {}
            }
        }
        Ok(PageAction::None)
    }

    fn handle_view_key(&mut self, ctx: &mut AppContext, key: KeyEvent) -> Result<PageAction> {
        if let Some(action) = self.view_keymap.lookup(key) {
            if let Some(page_action) = pane::execute_common_view_action(ctx, *action) {
                return Ok(page_action);
            }
            if *action == ViewAction::Refresh {
                self.refresh();
                return Ok(PageAction::None);
            }
        }
        let events = pane::dispatch_page_key(self, key);
        self.process_events(ctx, events)
    }

    /// Summary for the status bar: `(projects, items, columns, truncated)`.
    pub fn counts(&self) -> (usize, usize, usize, bool) {
        (
            self.panes.projects.items.len(),
            self.panes.board.item_count(),
            self.panes.board.column_count(),
            self.panes.board.truncated(),
        )
    }

    fn render_notice(&self, f: &mut Frame, area: Rect, lines: Vec<String>) {
        let lines: Vec<Line> = std::iter::once(Line::default())
            .chain(lines.into_iter().map(|l| {
                Line::from(Span::styled(
                    format!("  {l}"),
                    Style::default().fg(Color::DarkGray),
                ))
            }))
            .collect();
        f.render_widget(Paragraph::new(lines), area);
    }
}

impl PageState for ProjectsState {
    fn id(&self) -> &'static str {
        "projects"
    }

    fn label(&self) -> &'static str {
        "Projects"
    }

    fn help_bindings(&self) -> Vec<(String, String)> {
        use crate::core::keymap::help_section;
        let mut entries = self.view_keymap.help_entries();
        entries.extend(help_section("Projects"));
        entries.extend(self.panes.projects.keymap().help_entries());
        entries.extend(help_section("Board"));
        entries.extend(self.panes.board.keymap().help_entries());
        entries.extend(help_section("Detail"));
        entries.extend(self.panes.detail.keymap().help_entries());
        entries
    }

    fn handle_key(&mut self, ctx: &mut AppContext, key: KeyEvent) -> Result<PageAction> {
        if self.pane.handle_search_input(&mut self.panes, ctx, key) {
            // A confirmed search moves a selection without an event.
            let origin = self.pane.search.origin;
            if !self.pane.search.active {
                self.follow(origin);
            }
            return Ok(PageAction::None);
        }
        self.handle_view_key(ctx, key)
    }

    fn render(&mut self, f: &mut Frame, ctx: &AppContext, area: Rect) {
        let frame = split_page_frame(area);
        status_bar::render_projects_header(f, ctx, frame.header);
        if self.gh_available == Some(false) {
            let err = self.gh_error.as_deref().unwrap_or("gh not found");
            self.render_notice(
                f,
                frame.content,
                vec![
                    format!("gh not available: {err}"),
                    "Install the GitHub CLI and run `gh auth login`, then press r.".to_string(),
                ],
            );
        } else if self.scope_missing {
            self.render_notice(
                f,
                frame.content,
                vec![SCOPE_NOTICE.to_string(), "Then press r.".to_string()],
            );
        } else {
            pane::render_page_content(self, f, ctx, frame.content);
        }
        status_bar::render_projects_status_bar(f, ctx, self, frame.status_bar);
    }

    fn intercepts_all_keys(&self) -> bool {
        self.pane.search.active
    }

    fn on_activate(&mut self, _ctx: &mut AppContext) {
        if !self.initialized {
            self.initialize();
            return;
        }
        let stale = self
            .last_list_refresh
            .is_none_or(|t| t.elapsed() >= LIST_REFRESH_AFTER);
        if stale && self.gh_available != Some(false) && !self.scope_missing {
            self.spawn_list();
        }
    }

    fn drain_background(&mut self) {
        self.drain_bg_messages();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::keymap::KeyInput;
    use crate::projects::domain::types::tests::board;
    use crate::projects::domain::types::ProjectList;

    fn key(s: &str) -> KeyEvent {
        let ki: KeyInput = s.parse().unwrap();
        KeyEvent::new(ki.code, ki.modifiers)
    }

    fn ctx() -> AppContext {
        AppContext {
            should_quit: false,
            active_page: 0,
            page_labels: vec![],
            page_keys: vec![],
            show_help: false,
            status_message: None,
            error_dialog: None,
            workdir: std::path::PathBuf::new(),
            needs_full_redraw: false,
        }
    }

    /// A state with a live channel but no worker threads (no `gh` calls).
    fn state() -> ProjectsState {
        let mut st = ProjectsState::new(&Config::builtin()).expect("projects page");
        let (tx, rx) = mpsc::channel();
        st.bg_tx = Some(tx);
        st.bg_rx = Some(rx);
        st.initialized = true;
        st.use_disk_cache = false;
        st
    }

    fn projects() -> Vec<Project> {
        let list: ProjectList = serde_json::from_str(
            r#"{"projects":[{"number":1,"title":"life","items":{"totalCount":6}},{"number":2,"title":"vig demo board","items":{"totalCount":8},"url":"https://github.com/users/td72/projects/2"}],"totalCount":2}"#,
        )
        .unwrap();
        list.projects
    }

    #[test]
    fn projects_page_builds_from_builtin_config() {
        let cfg = Config::builtin();
        let state = ProjectsState::new(&cfg).expect("projects page");
        assert_eq!(state.id(), "projects");
        assert_eq!(state.label(), "Projects");
        assert_eq!(state.pane.focused_pane, state.panes.ids.projects);
        assert_eq!(
            state.layout_config.tab_panes,
            vec![
                state.panes.ids.projects,
                state.panes.ids.board,
                state.panes.ids.detail
            ]
        );
        let help = state.help_bindings();
        assert_eq!(help[0], ("q".to_string(), "Quit".to_string()));
        assert!(help.iter().all(|(_, d)| d != "Switch view"));
        assert!(help
            .iter()
            .any(|(k, d)| k == "t" && d == "Toggle table mode"));
        assert!(help.iter().any(|(_, d)| d.contains("Board")));
    }

    #[test]
    fn list_and_board_results_flow_into_the_panes() {
        let mut st = state();
        let tx = st.bg_tx.clone().unwrap();
        tx.send(ProjectsBgMessage::Repo(Ok(RepoInfo::default())))
            .unwrap();
        tx.send(ProjectsBgMessage::ProjectList {
            owner: "td72".into(),
            linked: vec![2],
            result: Ok(projects()),
        })
        .unwrap();
        st.drain_bg_messages();
        assert_eq!(st.gh_available, Some(true));
        assert_eq!(st.owner.as_deref(), Some("td72"));
        // The linked project is first and selected; its board is requested.
        assert_eq!(st.panes.projects.selected_number(), Some(2));
        assert!(st.board_inflight.contains(&2));
        assert!(st.panes.board.is_loading());
        // A board for another project only warms the cache.
        let mut other = board();
        other.number = 1;
        tx.send(ProjectsBgMessage::Board {
            number: 1,
            result: Ok(other),
        })
        .unwrap();
        st.drain_bg_messages();
        assert!(st.panes.board.board.is_none());
        tx.send(ProjectsBgMessage::Board {
            number: 2,
            result: Ok(board()),
        })
        .unwrap();
        st.drain_bg_messages();
        assert_eq!(st.panes.board.item_count(), 5);
        assert_eq!(st.counts(), (2, 5, 5, true));
        // The detail follows the board's first card (an issue: fetch starts).
        assert_eq!(st.panes.detail.item().map(|i| i.id.as_str()), Some("I4"));
        assert!(st.panes.detail.is_loading());
        // Moving to the other project shows its cached board without a fetch.
        let mut c = ctx();
        st.pane.focused_pane = st.panes.ids.projects;
        let events = pane::dispatch_page_key(&mut st, key("j"));
        st.process_events(&mut c, events).unwrap();
        assert_eq!(st.panes.projects.selected_number(), Some(1));
        assert_eq!(st.panes.board.board.as_ref().map(|b| b.number), Some(1));
        assert!(!st.board_inflight.contains(&1));
    }

    #[test]
    fn scope_errors_replace_the_panes_with_the_notice() {
        let mut st = state();
        let tx = st.bg_tx.clone().unwrap();
        tx.send(ProjectsBgMessage::Repo(Ok(RepoInfo::default())))
            .unwrap();
        tx.send(ProjectsBgMessage::ProjectList {
            owner: "td72".into(),
            linked: vec![],
            result: Err(
                "error: your authentication token is missing required scopes [project]".into(),
            ),
        })
        .unwrap();
        st.drain_bg_messages();
        assert!(st.scope_missing);
        assert!(st.gh_error.is_none());
        assert!(!st.panes.projects.is_loading());
        // A missing CLI is the other notice.
        let mut st = state();
        let tx = st.bg_tx.clone().unwrap();
        tx.send(ProjectsBgMessage::Repo(Err(
            "gh not found: No such file or directory (os error 2)".into(),
        )))
        .unwrap();
        st.drain_bg_messages();
        assert_eq!(st.gh_available, Some(false));
        assert!(st.gh_error.is_some());
        // Any other error lands in the status bar.
        let mut st = state();
        let tx = st.bg_tx.clone().unwrap();
        tx.send(ProjectsBgMessage::Repo(Err("HTTP 502".into())))
            .unwrap();
        st.drain_bg_messages();
        assert_eq!(st.gh_available, Some(true));
        assert_eq!(st.gh_error.as_deref(), Some("HTTP 502"));
    }

    #[test]
    fn enter_and_esc_walk_projects_board_and_detail() {
        let mut st = state();
        let ids = st.panes.ids;
        st.owner = Some("td72".into());
        st.panes.projects.set_projects(projects());
        st.board_cache.insert(1, {
            let mut b = board();
            b.number = 1;
            b
        });
        st.sync_board();
        let mut c = ctx();
        let events = pane::dispatch_page_key(&mut st, key("Enter"));
        assert!(matches!(events.as_slice(), [PaneEvent::SetFocus(id)] if *id == ids.board));
        st.process_events(&mut c, events).unwrap();
        assert_eq!(st.pane.focused_pane, ids.board);
        // `l` moves to the next column and the detail follows.
        let events = pane::dispatch_page_key(&mut st, key("l"));
        st.process_events(&mut c, events).unwrap();
        assert_eq!(st.panes.detail.item().map(|i| i.id.as_str()), Some("I2"));
        let events = pane::dispatch_page_key(&mut st, key("Enter"));
        st.process_events(&mut c, events).unwrap();
        assert_eq!(st.pane.focused_pane, ids.detail);
        let events = pane::dispatch_page_key(&mut st, key("Esc"));
        st.process_events(&mut c, events).unwrap();
        assert_eq!(st.pane.focused_pane, ids.board);
        let events = pane::dispatch_page_key(&mut st, key("Esc"));
        st.process_events(&mut c, events).unwrap();
        assert_eq!(st.pane.focused_pane, ids.projects);
        // Tab cycles Projects → Board → Detail.
        let events = pane::dispatch_page_key(&mut st, key("Tab"));
        assert!(matches!(events.as_slice(), [PaneEvent::SetFocus(id)] if *id == ids.board));
    }
}
