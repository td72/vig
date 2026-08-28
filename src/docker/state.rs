//! The Docker page: containers (grouped by compose project), images, an
//! inspect summary and a log tail. Read-only: only `docker version / ps /
//! images / inspect / logs` are ever run.

use crate::core::app::{AppContext, PageState};
use crate::core::config::{Config, LoadedPageConfig};
use crate::core::keymap::{Keymap, ViewAction};
use crate::core::layout::{split_page_frame, PageLayoutConfig};
use crate::core::page::PageAction;
use crate::core::pane::{self, Pane, PaneEvent, PaneSet, PaneShared};
use crate::core::search::SearchState;
use crate::core::ui::status_bar;
use crate::docker::domain::client;
use crate::docker::domain::types::{Container, Image, InspectSummary};
use crate::docker::panes::containers::{ContainerRow, ContainersAction, ContainersPane};
use crate::docker::panes::detail::{DetailAction, DetailPane, DetailTarget};
use crate::docker::panes::images::{ImagesAction, ImagesPane};
use crate::docker::panes::logs::{LogsAction, LogsPane};
use anyhow::Result;
use crossterm::event::KeyEvent;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// How often the container / image lists are re-fetched while the page is active.
pub const LIST_REFRESH_INTERVAL: Duration = Duration::from_secs(5);

/// Pane IDs resolved from the KDL config at construction time.
#[derive(Debug, Clone, Copy)]
pub struct DockerPaneIds {
    pub containers: usize,
    pub images: usize,
    pub detail: usize,
    pub logs: usize,
}

impl DockerPaneIds {
    fn from_config(cfg: &LoadedPageConfig) -> Self {
        Self {
            containers: cfg.resolve_id_expect("containers"),
            images: cfg.resolve_id_expect("images"),
            detail: cfg.resolve_id_expect("detail"),
            logs: cfg.resolve_id_expect("logs"),
        }
    }
}

pub enum DockerBgMessage {
    Version(Result<(), String>),
    ContainerList(Result<Vec<Container>, String>),
    ImageList(Result<Vec<Image>, String>),
    Inspect {
        key: String,
        result: Result<InspectSummary, String>,
    },
    Logs {
        request_id: u64,
        append: bool,
        result: Result<Vec<String>, String>,
    },
}

pub struct DockerPanes {
    pub containers: ContainersPane,
    pub images: ImagesPane,
    pub detail: DetailPane,
    pub logs: LogsPane,
    pub ids: DockerPaneIds,
}

impl PaneSet for DockerPanes {
    fn get_mut(&mut self, idx: usize) -> Option<&mut dyn Pane<PaneEvent>> {
        if idx == self.ids.containers {
            Some(&mut self.containers)
        } else if idx == self.ids.images {
            Some(&mut self.images)
        } else if idx == self.ids.detail {
            Some(&mut self.detail)
        } else if idx == self.ids.logs {
            Some(&mut self.logs)
        } else {
            None
        }
    }
}

/// Which list the detail pane follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DetailSource {
    Containers,
    Images,
}

pub struct DockerState {
    pub pane: PaneShared,
    pub panes: DockerPanes,
    /// `None` until `docker version` has answered.
    pub docker_available: Option<bool>,
    pub docker_error: Option<String>,
    bg_rx: Option<mpsc::Receiver<DockerBgMessage>>,
    bg_tx: Option<mpsc::Sender<DockerBgMessage>>,
    initialized: bool,
    last_list_refresh: Option<Instant>,
    detail_source: DetailSource,
    layout_config: PageLayoutConfig,
    view_keymap: Keymap<ViewAction>,
}

impl pane::PageLayout for DockerState {
    type Panes = DockerPanes;
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

impl DockerState {
    pub fn new(cfg: &Config) -> Result<Self> {
        let page_cfg = cfg.docker_page()?;
        let ids = DockerPaneIds::from_config(&page_cfg);
        // Validates the bind declarations (containers / images → detail).
        let _ = page_cfg.resolve_select_bindings();

        let containers_km = page_cfg.keymap::<ContainersAction>("containers")?;
        let images_km = page_cfg.keymap::<ImagesAction>("images")?;
        let detail_km = page_cfg.keymap::<DetailAction>("detail")?;
        let logs_km = page_cfg.keymap::<LogsAction>("logs")?;
        let view_km = page_cfg.keymap::<ViewAction>("view")?;

        let mut containers = ContainersPane::new(ids.containers, ids.detail, ids.logs);
        containers.set_keymap(containers_km);
        let mut images = ImagesPane::new(ids.images, ids.detail);
        images.set_keymap(images_km);
        let mut detail = DetailPane::new(ids.detail);
        detail.set_keymap(detail_km);
        let mut logs = LogsPane::new(ids.logs);
        logs.set_keymap(logs_km);

        Ok(Self {
            pane: PaneShared {
                focused_pane: ids.containers,
                previous_pane: ids.containers,
                search: SearchState::new(),
            },
            panes: DockerPanes {
                containers,
                images,
                detail,
                logs,
                ids,
            },
            docker_available: None,
            docker_error: None,
            bg_rx: None,
            bg_tx: None,
            initialized: false,
            last_list_refresh: None,
            detail_source: DetailSource::Containers,
            layout_config: page_cfg.layout,
            view_keymap: view_km,
        })
    }

    /// First switch to the page: detect the CLI and fetch both lists.
    fn initialize(&mut self) {
        if self.initialized {
            return;
        }
        self.initialized = true;
        let (tx, rx) = mpsc::channel();
        self.bg_tx = Some(tx);
        self.bg_rx = Some(rx);
        self.check_version();
        self.spawn_lists();
    }

    fn check_version(&mut self) {
        let Some(tx) = self.bg_tx.clone() else {
            return;
        };
        std::thread::spawn(move || {
            let _ = tx.send(DockerBgMessage::Version(client::check_docker_available()));
        });
    }

    fn spawn_lists(&mut self) {
        let Some(tx) = self.bg_tx.clone() else {
            return;
        };
        self.last_list_refresh = Some(Instant::now());
        self.panes.containers.set_loading(true);
        self.panes.images.set_loading(true);
        let tx_c = tx.clone();
        std::thread::spawn(move || {
            let _ = tx_c.send(DockerBgMessage::ContainerList(client::list_containers()));
        });
        std::thread::spawn(move || {
            let _ = tx.send(DockerBgMessage::ImageList(client::list_images()));
        });
    }

    fn is_loading(&self) -> bool {
        self.panes.containers.is_loading() || self.panes.images.is_loading()
    }

    /// `r`: re-check the CLI, re-fetch everything, drop the inspect cache.
    fn refresh(&mut self) {
        self.docker_error = None;
        if self.docker_available == Some(false) {
            self.docker_available = None;
        }
        self.check_version();
        self.spawn_lists();
        self.panes.detail.clear_cache();
        if let Some(tx) = self.bg_tx.clone() {
            self.panes.detail.reload_current(&tx);
            self.panes.logs.refresh(&tx);
        }
    }

    /// Point the detail pane at the selection of the list it follows.
    fn sync_detail(&mut self) {
        let Some(tx) = self.bg_tx.clone() else {
            return;
        };
        match self.detail_source {
            DetailSource::Containers => match self.panes.containers.selected() {
                Some(ContainerRow::Container(c)) => {
                    let target = DetailTarget::container(c);
                    self.panes.detail.load(target, &tx);
                }
                Some(ContainerRow::Project { name, .. }) => {
                    let name = name.clone();
                    let members = self.panes.containers.project_members(&name);
                    self.panes.detail.show_project(&name, &members);
                }
                None => self.panes.detail.show_none(),
            },
            DetailSource::Images => match self.panes.images.selected() {
                Some(img) => {
                    let target = DetailTarget::image(img);
                    self.panes.detail.load(target, &tx);
                }
                None => self.panes.detail.show_none(),
            },
        }
    }

    /// The logs pane always follows the containers list.
    fn sync_logs(&mut self) {
        if let Some(tx) = self.bg_tx.clone() {
            let selected = self.panes.containers.selected_container().cloned();
            self.panes.logs.load(selected.as_ref(), &tx);
        }
    }

    /// A list pane's selection is now the one the detail should follow.
    fn follow_list(&mut self, pane_id: usize) {
        let ids = self.panes.ids;
        if pane_id == ids.containers {
            self.detail_source = DetailSource::Containers;
            self.sync_detail();
            self.sync_logs();
        } else if pane_id == ids.images {
            self.detail_source = DetailSource::Images;
            self.sync_detail();
        }
    }

    fn drain_bg_messages(&mut self) {
        let messages: Vec<_> = match &self.bg_rx {
            Some(rx) => rx.try_iter().collect(),
            None => return,
        };
        let mut containers_arrived = false;
        let mut images_arrived = false;
        for msg in messages {
            match msg {
                DockerBgMessage::Version(result) => match result {
                    Ok(()) => {
                        self.docker_available = Some(true);
                    }
                    Err(e) => {
                        self.docker_available = Some(false);
                        self.docker_error = Some(e);
                        self.panes.containers.set_loading(false);
                        self.panes.images.set_loading(false);
                    }
                },
                DockerBgMessage::ContainerList(result) => {
                    self.panes.containers.set_loading(false);
                    match result {
                        Ok(list) => {
                            self.panes.containers.set_containers(list);
                            containers_arrived = true;
                        }
                        Err(e) => self.note_error(e),
                    }
                }
                DockerBgMessage::ImageList(result) => {
                    self.panes.images.set_loading(false);
                    match result {
                        Ok(list) => {
                            self.panes.images.set_images(list);
                            images_arrived = true;
                        }
                        Err(e) => self.note_error(e),
                    }
                }
                DockerBgMessage::Inspect { key, result } => {
                    self.panes.detail.apply(&key, result);
                }
                DockerBgMessage::Logs {
                    request_id,
                    append,
                    result,
                } => {
                    self.panes.logs.apply(request_id, append, result);
                }
            }
        }
        if containers_arrived || images_arrived {
            let on_containers = self.detail_source == DetailSource::Containers;
            if (on_containers && containers_arrived) || (!on_containers && images_arrived) {
                self.sync_detail();
                if let Some(tx) = self.bg_tx.clone() {
                    self.panes.detail.reload_current(&tx);
                }
            }
            if containers_arrived {
                self.sync_logs();
            }
        }
    }

    fn note_error(&mut self, e: String) {
        if self.docker_error.is_none() {
            self.docker_error = Some(e);
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
                PaneEvent::SetFocus(id) if id == ids.containers || id == ids.images => {
                    self.follow_list(id);
                }
                PaneEvent::SelectionChanged => {
                    self.follow_list(self.pane.focused_pane);
                }
                PaneEvent::JumpToMatch(forward) => {
                    if let Some(origin) =
                        self.pane
                            .jump_to_search_match(&mut self.panes, ctx, forward)
                    {
                        self.follow_list(origin);
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

    /// Summary for the status bar: `(containers, running, images)`.
    pub fn counts(&self) -> (usize, usize, usize) {
        let (total, running) = self.panes.containers.counts();
        (total, running, self.panes.images.items.len())
    }

    pub fn is_updating(&self) -> bool {
        self.is_loading()
    }

    fn render_unavailable(&self, f: &mut Frame, area: Rect) {
        let err = self.docker_error.as_deref().unwrap_or("docker not found");
        let lines = vec![
            Line::default(),
            Line::from(Span::styled(
                format!("  docker not available: {err}"),
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "  Install Docker and start the daemon, then press r.",
                Style::default().fg(Color::DarkGray),
            )),
        ];
        f.render_widget(Paragraph::new(lines), area);
    }
}

impl PageState for DockerState {
    fn id(&self) -> &'static str {
        "docker"
    }

    fn label(&self) -> &'static str {
        "Docker"
    }

    fn help_bindings(&self) -> Vec<(String, String)> {
        use crate::core::keymap::help_section;
        let s = |k: &str, v: &str| (k.to_string(), v.to_string());
        let mut entries = vec![s("1 / 2 / 3 / 4", "Switch view")];
        entries.extend(self.view_keymap.help_entries());
        entries.extend(help_section("Containers"));
        entries.extend(self.panes.containers.keymap().help_entries());
        entries.extend(help_section("Images"));
        entries.extend(self.panes.images.keymap().help_entries());
        entries.extend(help_section("Detail"));
        entries.extend(self.panes.detail.keymap().help_entries());
        entries.extend(help_section("Logs"));
        entries.extend(self.panes.logs.keymap().help_entries());
        entries
    }

    fn handle_key(&mut self, ctx: &mut AppContext, key: KeyEvent) -> Result<PageAction> {
        if self.pane.handle_search_input(&mut self.panes, ctx, key) {
            // A confirmed search moves the list selection without an event.
            let origin = self.pane.search.origin;
            if !self.pane.search.active {
                self.follow_list(origin);
            }
            return Ok(PageAction::None);
        }
        self.handle_view_key(ctx, key)
    }

    fn render(&mut self, f: &mut Frame, ctx: &AppContext, area: Rect) {
        let frame = split_page_frame(area);
        status_bar::render_docker_header(f, ctx, frame.header);
        if self.docker_available == Some(false) {
            self.render_unavailable(f, frame.content);
        } else {
            pane::render_page_content(self, f, ctx, frame.content);
        }
        status_bar::render_docker_status_bar(f, ctx, self, frame.status_bar);
    }

    fn intercepts_all_keys(&self) -> bool {
        self.pane.search.active
    }

    fn on_tick(&mut self, _ctx: &mut AppContext) {
        if !self.initialized || self.docker_available != Some(true) {
            return;
        }
        let due = self
            .last_list_refresh
            .is_none_or(|t| t.elapsed() >= LIST_REFRESH_INTERVAL);
        if due && !self.is_loading() {
            self.spawn_lists();
        }
        if let Some(tx) = self.bg_tx.clone() {
            self.panes.logs.on_tick(&tx);
        }
    }

    fn on_activate(&mut self, _ctx: &mut AppContext) {
        self.initialize();
    }

    fn drain_background(&mut self) {
        self.drain_bg_messages();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docker_page_builds_from_builtin_config() {
        let cfg = Config::builtin();
        let state = DockerState::new(&cfg).expect("docker page");
        assert_eq!(state.id(), "docker");
        assert_eq!(state.label(), "Docker");
        assert_eq!(state.pane.focused_pane, state.panes.ids.containers);
        assert_eq!(
            state.layout_config.tab_panes,
            vec![
                state.panes.ids.containers,
                state.panes.ids.images,
                state.panes.ids.detail,
                state.panes.ids.logs
            ]
        );
        let help = state.help_bindings();
        assert_eq!(help[0].0, "1 / 2 / 3 / 4");
        assert!(help.iter().any(|(_, d)| d.contains("Logs")));
    }
}
