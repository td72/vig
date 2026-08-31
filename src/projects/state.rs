//! The Projects page: the GitHub Projects (v2) linked to the repository vig
//! runs in, as a kanban board with an item detail. Read-only: only
//! `gh repo view`, `gh project field-list / item-list`, `gh issue view` and
//! `gh pr view` are ever run.
//!
//! The board fills the page; with several linked projects `p` / `P` switch
//! between them (a `projects-board` config node pins the page to one board
//! instead). The `projects` list pane exists but is not placed by the
//! built-in layout — a user layout that places it gets the list back and
//! the list's selection drives the board.

use crate::core::app::{AppContext, PageState};
use crate::core::config::{Config, LoadedPageConfig, ProjectsBoard};
use crate::core::keymap::{Keymap, ViewAction};
use crate::core::layout::{split_page_frame, PageLayoutConfig};
use crate::core::page::PageAction;
use crate::core::pane::{self, Pane, PaneEvent, PaneSet, PaneShared};
use crate::core::search::SearchState;
use crate::core::ui::status_bar;
use crate::projects::domain::types::{Board, Project, ProjectListCache, RepoInfo};
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

/// Coming back to the page after this long re-reads the linked projects
/// and re-fetches the shown board.
pub const STALE_AFTER: Duration = Duration::from_secs(300);

/// Notice shown instead of the panes when the token lacks the scope.
pub const SCOPE_NOTICE: &str = "gh needs the project scope: run `gh auth refresh -s project`";

/// Notice shown in the board pane when the repository has no linked project.
pub const NO_LINKED_NOTICE: &str = "No projects are linked to this repository \
    (link one from the repository's Projects tab or `gh project link`)";

/// Status message when `p` / `P` is pressed while `projects-board` pins the board.
pub const PINNED_MESSAGE: &str = "board pinned by config (projects-board)";

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
    /// `gh repo view`: the repository and its linked projects.
    Repo(Result<RepoInfo, String>),
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

/// A board in memory together with when it was fetched. `None` means the
/// fetch time is unknown (a disk cache file older than the clock can
/// express): such a board is always considered stale.
struct CachedBoard {
    board: Board,
    fetched_at: Option<Instant>,
}

pub struct ProjectsState {
    pub pane: PaneShared,
    /// The list pane holds the linked projects and the current one, whether
    /// or not the layout places it.
    pub panes: ProjectsPanes,
    /// `None` until `gh repo view` has answered.
    pub gh_available: Option<bool>,
    pub gh_error: Option<String>,
    /// The token lacks the `project` scope: the panes are replaced by a notice.
    pub scope_missing: bool,
    /// `owner/repo` of the repository vig runs in (cards of other
    /// repositories are prefixed with theirs).
    repo: Option<String>,
    /// The layout places the `projects` list pane (it can take focus then).
    list_placed: bool,
    /// The linked projects have been read (from `gh repo view` or the disk
    /// cache): an empty list then really means nothing is linked.
    links_known: bool,
    /// The `projects-board` config pin: only the matching project is shown
    /// and `p` / `P` do not cycle.
    pinned: Option<ProjectsBoard>,
    bg_rx: Option<mpsc::Receiver<ProjectsBgMessage>>,
    bg_tx: Option<mpsc::Sender<ProjectsBgMessage>>,
    initialized: bool,
    last_list_refresh: Option<Instant>,
    /// Boards by project number with their fetch time, filled from the
    /// disk cache (as old as its file) and from fetches.
    board_cache: HashMap<u64, CachedBoard>,
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
        let list_placed = page_cfg.is_placed("projects");
        let pinned = cfg.projects_board()?;
        // Validates the bind declarations (projects → board while the list
        // is placed, board → detail).
        let _ = page_cfg.resolve_select_bindings();

        let projects_km = page_cfg.keymap::<ProjectsAction>("projects")?;
        let board_km = page_cfg.keymap::<BoardAction>("board")?;
        let detail_km = page_cfg.keymap::<DetailAction>("detail")?;
        let view_km = page_cfg.keymap::<ViewAction>("view")?;

        let mut projects = ProjectsPane::new(ids.projects, ids.board);
        projects.set_keymap(projects_km);
        // Esc on the board goes back to the list only when there is one.
        let mut board = BoardPane::new(ids.board, ids.detail, list_placed.then_some(ids.projects));
        board.set_keymap(board_km);
        let mut detail = DetailPane::new(ids.detail);
        detail.set_keymap(detail_km);

        let first = if list_placed { ids.projects } else { ids.board };
        Ok(Self {
            pane: PaneShared {
                focused_pane: first,
                previous_pane: first,
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
            repo: None,
            list_placed,
            links_known: false,
            pinned,
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

    /// First switch to the page: show the cached projects, then read the
    /// linked projects of the repository.
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
            self.links_known = true;
            self.set_repo(Some(cache.repo));
            let projects = self.apply_pin(cache.projects);
            self.panes.projects.set_projects(projects);
            self.sync_board();
        }
        self.spawn_list();
    }

    fn set_repo(&mut self, repo: Option<String>) {
        self.repo = repo.filter(|r| !r.is_empty());
        self.panes.board.set_repo(self.repo.clone());
    }

    /// `gh repo view` on a worker thread.
    fn spawn_list(&mut self) {
        let Some(tx) = self.bg_tx.clone() else {
            return;
        };
        self.panes.projects.set_loading(true);
        self.update_notice();
        std::thread::spawn(move || {
            let _ = tx.send(ProjectsBgMessage::Repo(client::repo_info()));
        });
    }

    fn spawn_board(&mut self, number: u64) {
        let Some(tx) = self.bg_tx.clone() else {
            return;
        };
        let Some(owner) = self
            .panes
            .projects
            .items
            .iter()
            .find(|p| p.number == number)
            .map(|p| p.owner.login.clone())
        else {
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

    /// The cached board for `number` is missing or older than [`STALE_AFTER`].
    fn board_stale(&self, number: u64) -> bool {
        self.board_cache
            .get(&number)
            .is_none_or(|c| c.fetched_at.is_none_or(|t| t.elapsed() >= STALE_AFTER))
    }

    pub fn is_loading(&self) -> bool {
        self.panes.projects.is_loading()
            || self.panes.board.is_loading()
            || self.panes.detail.is_loading()
    }

    /// `r`: re-read the linked projects, the shown board and the shown item.
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

    /// `p` / `P`: show the next / previous linked project's board.
    fn cycle_project(&mut self, forward: bool) {
        let n = self.panes.projects.items.len();
        if n < 2 {
            return;
        }
        let idx = self.panes.projects.selected_idx;
        self.panes.projects.selected_idx = if forward {
            (idx + 1) % n
        } else {
            (idx + n - 1) % n
        };
        self.sync_board();
    }

    /// With `projects-board` set, drop every project but the pinned one.
    fn apply_pin(&self, projects: Vec<Project>) -> Vec<Project> {
        match &self.pinned {
            None => projects,
            Some(pin) => projects
                .into_iter()
                .filter(|p| pin.matches(p.number, &p.title))
                .collect(),
        }
    }

    /// What the board pane says while it has no board: loading, or that
    /// nothing is linked (or that the configured pin matches nothing).
    fn update_notice(&mut self) {
        let notice = if !self.panes.projects.items.is_empty() {
            None
        } else if self.panes.projects.is_loading() {
            Some("Loading...".to_string())
        } else if !self.links_known {
            None
        } else if let Some(pin) = &self.pinned {
            Some(format!(
                "projects-board {pin} does not match any project linked to this repository"
            ))
        } else {
            Some(NO_LINKED_NOTICE.to_string())
        };
        self.panes.board.set_notice(notice);
    }

    /// Point the board at the current project: from memory, else from
    /// disk while a fetch runs.
    fn sync_board(&mut self) {
        self.update_notice();
        let Some(project) = self.panes.projects.selected().cloned() else {
            self.panes.board.clear();
            self.panes.detail.show_none();
            return;
        };
        let number = project.number;
        self.panes.board.set_project_url(Some(project.url.clone()));
        if !self.board_cache.contains_key(&number) && self.use_disk_cache {
            if let Some((board, age)) = disk_cache::load_board_with_age(number) {
                // A board loaded from disk is as old as its file (the
                // last fetch wrote it).
                self.board_cache.insert(
                    number,
                    CachedBoard {
                        board,
                        fetched_at: Instant::now().checked_sub(age),
                    },
                );
            }
        }
        let shown = self.panes.board.board.as_ref().map(|b| b.number);
        if shown != Some(number) {
            match self.board_cache.get(&number) {
                Some(c) => self.panes.board.set_board(c.board.clone()),
                None => self.panes.board.clear(),
            }
        }
        if self.board_stale(number) {
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
                ProjectsBgMessage::Repo(result) => {
                    self.panes.projects.set_loading(false);
                    match result {
                        Ok(info) => {
                            self.last_list_refresh = Some(Instant::now());
                            self.gh_available = Some(true);
                            self.links_known = true;
                            self.set_repo(Some(info.name_with_owner.clone()));
                            let mut projects = info.linked_projects();
                            // Keep the item counts the boards have taught us.
                            for p in &mut projects {
                                if let Some(known) = self
                                    .panes
                                    .projects
                                    .items
                                    .iter()
                                    .find(|k| k.number == p.number)
                                {
                                    p.items = known.items.clone();
                                }
                            }
                            if self.use_disk_cache {
                                disk_cache::save_project_list(&ProjectListCache {
                                    repo: self.repo.clone().unwrap_or_default(),
                                    projects: projects.clone(),
                                });
                            }
                            let projects = self.apply_pin(projects);
                            self.panes.projects.set_projects(projects);
                            self.sync_board();
                        }
                        Err(e) => {
                            if client::is_gh_missing(&e) {
                                self.gh_available = Some(false);
                                self.gh_error = Some(e);
                            } else {
                                self.gh_available = Some(true);
                                self.note_error(e);
                            }
                            self.update_notice();
                        }
                    }
                }
                ProjectsBgMessage::Board { number, result } => {
                    self.board_inflight.remove(&number);
                    // Only the selected project's fetch may clear the pane's
                    // loading state: another board finishing (p/P cycling)
                    // must not hide the indicator while ours is in flight.
                    let selected = self.panes.projects.selected_number();
                    if selected.is_none_or(|n| !self.board_inflight.contains(&n)) {
                        self.panes.board.set_loading(false);
                    }
                    let current = self.panes.projects.selected_number() == Some(number);
                    match result {
                        Ok(board) => {
                            if self.use_disk_cache {
                                disk_cache::save_board(&board);
                            }
                            self.panes
                                .projects
                                .set_item_count(number, board.total_count);
                            self.board_cache.insert(
                                number,
                                CachedBoard {
                                    board: board.clone(),
                                    fetched_at: Some(Instant::now()),
                                },
                            );
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
            // An unplaced list pane has no area: it cannot take focus.
            if matches!(event, PaneEvent::SetFocus(id) if id == ids.projects && !self.list_placed) {
                continue;
            }
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
            match action {
                ViewAction::Refresh => {
                    self.refresh();
                    return Ok(PageAction::None);
                }
                ViewAction::NextProject | ViewAction::PrevProject => {
                    if self.pinned.is_some() {
                        ctx.status_message = Some(PINNED_MESSAGE.to_string());
                    } else {
                        self.cycle_project(*action == ViewAction::NextProject);
                    }
                    return Ok(PageAction::None);
                }
                _ => {}
            }
        }
        let events = pane::dispatch_page_key(self, key);
        self.process_events(ctx, events)
    }

    /// Summary for the status bar: `(linked projects, items, columns, truncated)`.
    pub fn counts(&self) -> (usize, usize, usize, bool) {
        (
            self.panes.projects.items.len(),
            self.panes.board.item_count(),
            self.panes.board.column_count(),
            self.panes.board.truncated(),
        )
    }

    /// The shown board's age for the status bar, e.g. `board 12m ago`.
    pub fn board_age(&self) -> Option<String> {
        let number = self.panes.board.board.as_ref()?.number;
        let fetched = self.board_cache.get(&number)?.fetched_at?;
        let secs = fetched.elapsed().as_secs() as i64;
        Some(format!(
            "board {}",
            crate::github::domain::actions::time::format_relative(secs)
        ))
    }

    /// The shown project for the header: `(title, position, linked count)`.
    pub fn board_label(&self) -> Option<(&str, usize, usize)> {
        let project = self.panes.projects.selected()?;
        Some((
            project.title.as_str(),
            self.panes.projects.selected_idx + 1,
            self.panes.projects.items.len(),
        ))
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
        if self.list_placed {
            entries.extend(help_section("Projects"));
            entries.extend(self.panes.projects.keymap().help_entries());
        }
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
        status_bar::render_projects_header(f, ctx, self, frame.header);
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
        if self.gh_available == Some(false) || self.scope_missing {
            return;
        }
        let list_stale = self
            .last_list_refresh
            .is_none_or(|t| t.elapsed() >= STALE_AFTER);
        if list_stale && !self.panes.projects.is_loading() {
            self.spawn_list();
        }
        if let Some(number) = self.panes.projects.selected_number() {
            if self.board_stale(number) {
                self.spawn_board(number);
            }
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
    use crate::projects::domain::types::tests::{board, repo_info};

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
    fn state_with(cfg: &Config) -> ProjectsState {
        let mut st = ProjectsState::new(cfg).expect("projects page");
        let (tx, rx) = mpsc::channel();
        st.bg_tx = Some(tx);
        st.bg_rx = Some(rx);
        st.initialized = true;
        st.use_disk_cache = false;
        st
    }

    fn state() -> ProjectsState {
        state_with(&Config::builtin())
    }

    /// A freshly-fetched cache entry.
    fn cached(board: Board) -> CachedBoard {
        CachedBoard {
            board,
            fetched_at: Some(Instant::now()),
        }
    }

    /// A user config that places the list pane on the left.
    fn list_config() -> Config {
        let doc: kdl::KdlDocument = r#"page "projects" {
            layout {
                split direction="horizontal" {
                    place "projects" size="22%"
                    split direction="vertical" size="min:30" {
                        place "board" size="60%"
                        place "detail" size="min:5"
                    }
                }
            }
            tabs "projects" "board" "detail"
        }"#
        .parse()
        .unwrap();
        Config::with_user(&doc, std::path::PathBuf::from("/u/config.kdl")).unwrap()
    }

    fn press(st: &mut ProjectsState, c: &mut AppContext, k: &str) {
        st.handle_key(c, key(k)).unwrap();
    }

    #[test]
    fn projects_page_builds_from_builtin_config() {
        let cfg = Config::builtin();
        let state = ProjectsState::new(&cfg).expect("projects page");
        assert_eq!(state.id(), "projects");
        assert_eq!(state.label(), "Projects");
        // The list pane is not placed: the board has focus and Tab cycles
        // between the board and the detail only.
        assert!(!state.list_placed);
        assert_eq!(state.pane.focused_pane, state.panes.ids.board);
        assert_eq!(
            state.layout_config.tab_panes,
            vec![state.panes.ids.board, state.panes.ids.detail]
        );
        let help = state.help_bindings();
        assert_eq!(help[0], ("q".to_string(), "Quit".to_string()));
        assert!(help.iter().all(|(_, d)| d != "Switch view"));
        assert!(help
            .iter()
            .any(|(k, d)| k == "p" && d == "Next linked project"));
        assert!(help
            .iter()
            .any(|(k, d)| k == "P" && d == "Prev linked project"));
        assert!(help
            .iter()
            .any(|(k, d)| k == "t" && d == "Toggle table mode"));
        assert!(help.iter().any(|(_, d)| d.contains("Board")));
        assert!(
            !help.iter().any(|(_, d)| d.contains("── Projects ──")),
            "no help section for the unplaced list pane"
        );
    }

    #[test]
    fn linked_projects_drive_the_board_and_p_cycles_them() {
        let mut st = state();
        let tx = st.bg_tx.clone().unwrap();
        assert!(st.board_label().is_none());
        tx.send(ProjectsBgMessage::Repo(Ok(repo_info()))).unwrap();
        st.drain_bg_messages();
        assert_eq!(st.gh_available, Some(true));
        assert_eq!(st.repo.as_deref(), Some("td72/vig"));
        // The first linked project is current; its board is requested.
        assert_eq!(st.panes.projects.selected_number(), Some(2));
        assert_eq!(st.board_label(), Some(("vig demo board", 1, 2)));
        assert!(st.board_inflight.contains(&2));
        assert!(st.panes.board.is_loading());
        assert!(st.panes.board.notice().is_none());
        // A board for the other project only warms the cache.
        let mut other = board();
        other.number = 7;
        tx.send(ProjectsBgMessage::Board {
            number: 7,
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
        // The board taught the list its item count.
        assert_eq!(st.panes.projects.items[0].items.total_count, 7);
        // The detail follows the board's first card (an issue: fetch starts).
        assert_eq!(st.panes.detail.item().map(|i| i.id.as_str()), Some("I4"));
        assert!(st.panes.detail.is_loading());
        // `p` shows the next project's cached board without a fetch; `P` goes back.
        let mut c = ctx();
        press(&mut st, &mut c, "p");
        assert_eq!(st.panes.projects.selected_number(), Some(7));
        assert_eq!(st.board_label(), Some(("Roadmap", 2, 2)));
        assert_eq!(st.panes.board.board.as_ref().map(|b| b.number), Some(7));
        assert!(!st.board_inflight.contains(&7));
        press(&mut st, &mut c, "p");
        assert_eq!(st.board_label(), Some(("vig demo board", 1, 2)));
        press(&mut st, &mut c, "P");
        assert_eq!(st.board_label(), Some(("Roadmap", 2, 2)));
        // A refresh keeps the current project and the known item counts.
        tx.send(ProjectsBgMessage::Repo(Ok(repo_info()))).unwrap();
        st.drain_bg_messages();
        assert_eq!(st.panes.projects.selected_number(), Some(7));
        assert_eq!(st.panes.projects.items[0].items.total_count, 7);
    }

    #[test]
    fn a_repository_without_linked_projects_shows_the_notice() {
        let mut st = state();
        let tx = st.bg_tx.clone().unwrap();
        st.spawn_list();
        assert_eq!(
            st.panes.board.notice().map(String::as_str),
            Some("Loading...")
        );
        let info: RepoInfo = serde_json::from_str(
            r#"{"nameWithOwner":"td72/empty","owner":{"login":"td72"},"projectsV2":{"Nodes":[]}}"#,
        )
        .unwrap();
        tx.send(ProjectsBgMessage::Repo(Ok(info))).unwrap();
        st.drain_bg_messages();
        assert_eq!(st.gh_available, Some(true));
        assert_eq!(st.counts(), (0, 0, 0, false));
        assert!(st.board_label().is_none());
        assert_eq!(
            st.panes.board.notice().map(String::as_str),
            Some(NO_LINKED_NOTICE)
        );
        assert!(st.gh_error.is_none());
        // `p` has nothing to cycle.
        let mut c = ctx();
        press(&mut st, &mut c, "p");
        assert!(st.board_label().is_none());
        // Once a project shows up the notice goes away.
        tx.send(ProjectsBgMessage::Repo(Ok(repo_info()))).unwrap();
        st.drain_bg_messages();
        assert!(st.panes.board.notice().is_none());
    }

    #[test]
    fn scope_errors_replace_the_panes_with_the_notice() {
        let mut st = state();
        let tx = st.bg_tx.clone().unwrap();
        tx.send(ProjectsBgMessage::Repo(Ok(repo_info()))).unwrap();
        tx.send(ProjectsBgMessage::Board {
            number: 2,
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
        assert!(st.panes.board.notice().is_none(), "not the no-links notice");
    }

    #[test]
    fn enter_and_esc_walk_board_and_detail_without_the_list() {
        let mut st = state();
        let ids = st.panes.ids;
        st.panes
            .projects
            .set_projects(repo_info().linked_projects());
        st.board_cache.insert(2, cached(board()));
        st.sync_board();
        let mut c = ctx();
        assert_eq!(st.pane.focused_pane, ids.board);
        // `l` moves to the next column and the detail follows.
        let events = pane::dispatch_page_key(&mut st, key("l"));
        st.process_events(&mut c, events).unwrap();
        assert_eq!(st.panes.detail.item().map(|i| i.id.as_str()), Some("I2"));
        let events = pane::dispatch_page_key(&mut st, key("Enter"));
        assert!(matches!(events.as_slice(), [PaneEvent::SetFocus(id)] if *id == ids.detail));
        st.process_events(&mut c, events).unwrap();
        assert_eq!(st.pane.focused_pane, ids.detail);
        let events = pane::dispatch_page_key(&mut st, key("Esc"));
        st.process_events(&mut c, events).unwrap();
        assert_eq!(st.pane.focused_pane, ids.board);
        // Esc on the board has no list to go back to.
        let events = pane::dispatch_page_key(&mut st, key("Esc"));
        assert!(events.is_empty());
        assert_eq!(st.pane.focused_pane, ids.board);
        // Tab cycles Board → Detail → Board.
        let events = pane::dispatch_page_key(&mut st, key("Tab"));
        assert!(matches!(events.as_slice(), [PaneEvent::SetFocus(id)] if *id == ids.detail));
        st.process_events(&mut c, events).unwrap();
        let events = pane::dispatch_page_key(&mut st, key("Tab"));
        assert!(matches!(events.as_slice(), [PaneEvent::SetFocus(id)] if *id == ids.board));
        st.process_events(&mut c, events).unwrap();
        assert_eq!(st.pane.focused_pane, ids.board);
        // A stray focus request for the unplaced list is dropped.
        st.process_events(&mut c, vec![PaneEvent::SetFocus(ids.projects)])
            .unwrap();
        assert_eq!(st.pane.focused_pane, ids.board);
    }

    #[test]
    fn a_placed_list_pane_takes_focus_and_drives_the_board() {
        let cfg = list_config();
        let mut st = state_with(&cfg);
        let ids = st.panes.ids;
        assert!(st.list_placed);
        assert_eq!(st.pane.focused_pane, ids.projects);
        assert_eq!(
            st.layout_config.tab_panes,
            vec![ids.projects, ids.board, ids.detail]
        );
        assert!(st
            .help_bindings()
            .iter()
            .any(|(_, d)| d.contains("── Projects ──")));
        st.panes
            .projects
            .set_projects(repo_info().linked_projects());
        st.board_cache.insert(2, cached(board()));
        st.board_cache.insert(7, {
            let mut b = board();
            b.number = 7;
            cached(b)
        });
        st.sync_board();
        let mut c = ctx();
        // `j` in the list moves to the other project's board.
        let events = pane::dispatch_page_key(&mut st, key("j"));
        st.process_events(&mut c, events).unwrap();
        assert_eq!(st.panes.board.board.as_ref().map(|b| b.number), Some(7));
        assert_eq!(st.board_label(), Some(("Roadmap", 2, 2)));
        // Enter focuses the board, Esc comes back to the list.
        let events = pane::dispatch_page_key(&mut st, key("Enter"));
        st.process_events(&mut c, events).unwrap();
        assert_eq!(st.pane.focused_pane, ids.board);
        let events = pane::dispatch_page_key(&mut st, key("Esc"));
        st.process_events(&mut c, events).unwrap();
        assert_eq!(st.pane.focused_pane, ids.projects);
        // `p` still cycles and the list follows.
        press(&mut st, &mut c, "p");
        assert_eq!(st.panes.projects.selected_number(), Some(2));
        assert_eq!(st.panes.board.board.as_ref().map(|b| b.number), Some(2));
    }

    /// A user config that pins the board.
    fn pinned_config(node: &str) -> Config {
        let doc: kdl::KdlDocument = node.parse().unwrap();
        Config::with_user(&doc, std::path::PathBuf::from("/u/config.kdl")).unwrap()
    }

    #[test]
    fn projects_board_pins_the_board_by_title_case_insensitively() {
        let cfg = pinned_config(r#"projects-board "VIG Demo Board""#);
        let mut st = state_with(&cfg);
        let tx = st.bg_tx.clone().unwrap();
        tx.send(ProjectsBgMessage::Repo(Ok(repo_info()))).unwrap();
        st.drain_bg_messages();
        // Only the pinned project remains: the header shows no `(i/n)`.
        assert_eq!(st.panes.projects.selected_number(), Some(2));
        assert_eq!(st.board_label(), Some(("vig demo board", 1, 1)));
        assert!(st.panes.board.notice().is_none());
        assert!(st.board_inflight.contains(&2));
        // `p` / `P` do not cycle; a status message says why.
        let mut c = ctx();
        press(&mut st, &mut c, "p");
        assert_eq!(st.panes.projects.selected_number(), Some(2));
        assert_eq!(c.status_message.as_deref(), Some(PINNED_MESSAGE));
        c.status_message = None;
        press(&mut st, &mut c, "P");
        assert_eq!(st.panes.projects.selected_number(), Some(2));
        assert_eq!(c.status_message.as_deref(), Some(PINNED_MESSAGE));
    }

    #[test]
    fn projects_board_pins_the_board_by_number() {
        let cfg = pinned_config("projects-board 7");
        let mut st = state_with(&cfg);
        let tx = st.bg_tx.clone().unwrap();
        tx.send(ProjectsBgMessage::Repo(Ok(repo_info()))).unwrap();
        st.drain_bg_messages();
        assert_eq!(st.panes.projects.selected_number(), Some(7));
        assert_eq!(st.board_label(), Some(("Roadmap", 1, 1)));
        assert!(st.panes.board.notice().is_none());
    }

    #[test]
    fn an_unmatched_projects_board_pin_names_itself_in_the_notice() {
        let cfg = pinned_config(r#"projects-board "foo""#);
        let mut st = state_with(&cfg);
        let tx = st.bg_tx.clone().unwrap();
        tx.send(ProjectsBgMessage::Repo(Ok(repo_info()))).unwrap();
        st.drain_bg_messages();
        assert!(st.board_label().is_none());
        assert_eq!(
            st.panes.board.notice().map(String::as_str),
            Some(r#"projects-board "foo" does not match any project linked to this repository"#)
        );
        // No stray error: the pin simply does not match.
        assert!(st.gh_error.is_none());
    }

    #[test]
    fn a_stale_board_is_refetched_on_activation_and_a_fresh_one_is_not() {
        let mut st = state();
        let tx = st.bg_tx.clone().unwrap();
        tx.send(ProjectsBgMessage::Repo(Ok(repo_info()))).unwrap();
        tx.send(ProjectsBgMessage::Board {
            number: 2,
            result: Ok(board()),
        })
        .unwrap();
        st.drain_bg_messages();
        assert!(!st.board_inflight.contains(&2));
        let mut c = ctx();
        // Fresh (< 5 min): activation fetches nothing.
        st.on_activate(&mut c);
        assert!(!st.board_inflight.contains(&2));
        assert!(!st.panes.projects.is_loading());
        // Stale: activation re-fetches while the old board stays shown.
        st.board_cache.get_mut(&2).unwrap().fetched_at = Instant::now().checked_sub(STALE_AFTER);
        st.on_activate(&mut c);
        assert!(st.board_inflight.contains(&2));
        assert_eq!(st.panes.board.board.as_ref().map(|b| b.number), Some(2));
    }

    #[test]
    fn a_failed_fetch_does_not_suppress_the_retry() {
        let mut st = state();
        let tx = st.bg_tx.clone().unwrap();
        st.spawn_list();
        assert!(
            st.last_list_refresh.is_none(),
            "spawning alone records no refresh"
        );
        tx.send(ProjectsBgMessage::Repo(Err("HTTP 502".into())))
            .unwrap();
        st.drain_bg_messages();
        assert!(
            st.last_list_refresh.is_none(),
            "a failed list read does not push the next retry out"
        );
        tx.send(ProjectsBgMessage::Repo(Ok(repo_info()))).unwrap();
        st.drain_bg_messages();
        assert!(
            st.last_list_refresh.is_some(),
            "success records the refresh"
        );
        // A failed board fetch leaves no cache entry: still stale, so the
        // next activation retries it.
        tx.send(ProjectsBgMessage::Board {
            number: 2,
            result: Err("HTTP 502".into()),
        })
        .unwrap();
        st.drain_bg_messages();
        assert!(!st.board_inflight.contains(&2));
        assert!(st.board_stale(2));
        st.gh_error = None;
        let mut c = ctx();
        st.on_activate(&mut c);
        assert!(st.board_inflight.contains(&2), "the board is retried");
    }

    #[test]
    fn the_status_bar_shows_the_board_age() {
        let mut st = state();
        let tx = st.bg_tx.clone().unwrap();
        assert!(st.board_age().is_none());
        tx.send(ProjectsBgMessage::Repo(Ok(repo_info()))).unwrap();
        tx.send(ProjectsBgMessage::Board {
            number: 2,
            result: Ok(board()),
        })
        .unwrap();
        st.drain_bg_messages();
        assert_eq!(st.board_age().as_deref(), Some("board just now"));
        let earlier = Instant::now().checked_sub(Duration::from_secs(12 * 60));
        st.board_cache.get_mut(&2).unwrap().fetched_at = earlier;
        if earlier.is_some() {
            // (`None` only on a machine up for less than 12 minutes.)
            assert_eq!(st.board_age().as_deref(), Some("board 12m ago"));
        }
        // An unknown fetch time (a very old disk file) shows no age and
        // counts as stale.
        st.board_cache.get_mut(&2).unwrap().fetched_at = None;
        assert!(st.board_age().is_none());
        assert!(st.board_stale(2));
    }
}
