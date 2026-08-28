//! The Procs page: a process tree with CPU / memory, the listening ports and
//! their owners, and a per-process detail. Read-only by design — nothing
//! here sends a signal or touches a process, and environment variables are
//! never read.

use crate::core::app::{AppContext, PageState};
use crate::core::config::{Config, LoadedPageConfig};
use crate::core::keymap::{Keymap, ViewAction};
use crate::core::layout::{split_page_frame, PageLayoutConfig};
use crate::core::page::PageAction;
use crate::core::pane::{self, Pane, PaneEvent, PaneSet, PaneShared};
use crate::core::search::SearchState;
use crate::core::tab::Tab;
use crate::core::ui::status_bar;
use crate::procs::domain::ports;
use crate::procs::domain::snapshot;
use crate::procs::domain::types::{PortEntry, ProcessInfo};
use crate::procs::panes::detail::{DetailAction, DetailData, DetailPane};
use crate::procs::panes::ports::{PortsAction, PortsPane};
use crate::procs::panes::processes::{ProcessesAction, ProcessesPane};
use anyhow::Result;
use crossterm::event::KeyEvent;
use ratatui::{layout::Rect, Frame};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Pane IDs resolved from the KDL config at construction time.
#[derive(Debug, Clone, Copy)]
pub struct ProcsPaneIds {
    pub processes: usize,
    pub ports: usize,
    pub detail: usize,
}

impl ProcsPaneIds {
    fn from_config(cfg: &LoadedPageConfig) -> Self {
        Self {
            processes: cfg.resolve_id_expect("processes"),
            ports: cfg.resolve_id_expect("ports"),
            detail: cfg.resolve_id_expect("detail"),
        }
    }
}

pub type ProcTab = Tab<ProcessesPane, DetailPane>;

pub struct ProcsPanes {
    pub tab: ProcTab,
    pub ports: PortsPane,
    pub ids: ProcsPaneIds,
}

impl ProcsPanes {
    /// Rebuild the detail from the selected process, its children and the
    /// ports it owns.
    pub fn sync_detail(&mut self) {
        let data = self.tab.list.selected().map(|info| DetailData {
            parent_name: info.ppid.and_then(|pp| self.tab.list.name_of(pp)),
            children: self.tab.list.children_of(info.pid),
            ports: self.ports.ports_of(info.pid),
            info: info.clone(),
        });
        self.tab.detail.load(data);
    }
}

impl PaneSet for ProcsPanes {
    fn get_mut(&mut self, idx: usize) -> Option<&mut dyn Pane<PaneEvent>> {
        if idx == self.ids.ports {
            Some(&mut self.ports)
        } else {
            self.tab
                .get_pane_mut(self.ids.processes, self.ids.detail, idx)
        }
    }
}

pub enum ProcsBgMessage {
    Snapshot(Vec<ProcessInfo>),
    Ports(Result<Vec<PortEntry>, String>),
}

pub struct ProcsState {
    pub pane: PaneShared,
    pub panes: ProcsPanes,
    /// Shown in the header.
    pub host: String,
    layout_config: PageLayoutConfig,
    view_keymap: Keymap<ViewAction>,
    bg_rx: Option<mpsc::Receiver<ProcsBgMessage>>,
    bg_tx: Option<mpsc::Sender<ProcsBgMessage>>,
    /// One `()` per snapshot request to the sampler thread.
    snapshot_req: Option<mpsc::Sender<()>>,
    snapshot_pending: bool,
    ports_pending: bool,
    interval: Duration,
    last_request: Instant,
    initialized: bool,
}

impl pane::PageLayout for ProcsState {
    type Panes = ProcsPanes;
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

impl ProcsState {
    pub fn new(cfg: &Config) -> Result<Self> {
        let page_cfg = cfg.procs_page()?;
        let ids = ProcsPaneIds::from_config(&page_cfg);
        // Validates the bind declarations (processes → detail).
        let _ = page_cfg.resolve_select_bindings();
        let interval = cfg.procs_refresh_interval()?;

        let processes_km = page_cfg.keymap::<ProcessesAction>("processes")?;
        let ports_km = page_cfg.keymap::<PortsAction>("ports")?;
        let detail_km = page_cfg.keymap::<DetailAction>("detail")?;
        let view_km = page_cfg.keymap::<ViewAction>("view")?;

        let mut list = ProcessesPane::new(ids.processes, ids.detail);
        list.set_keymap(processes_km);
        let mut ports = PortsPane::new(ids.ports);
        ports.set_keymap(ports_km);
        let mut detail = DetailPane::new(ids.detail, ids.processes);
        detail.set_keymap(detail_km);

        Ok(Self {
            pane: PaneShared {
                focused_pane: ids.processes,
                previous_pane: ids.processes,
                search: SearchState::new(),
            },
            panes: ProcsPanes {
                tab: Tab { list, detail },
                ports,
                ids,
            },
            host: sysinfo::System::host_name().unwrap_or_else(|| "localhost".to_string()),
            layout_config: page_cfg.layout,
            view_keymap: view_km,
            bg_rx: None,
            bg_tx: None,
            snapshot_req: None,
            snapshot_pending: false,
            ports_pending: false,
            interval,
            last_request: Instant::now(),
            initialized: false,
        })
    }

    /// Start the background workers on the first visit and take the first
    /// snapshot.
    fn initialize(&mut self) {
        if self.initialized {
            return;
        }
        self.initialized = true;
        let (tx, rx) = mpsc::channel();
        self.snapshot_req = Some(snapshot::spawn_worker(tx.clone(), ProcsBgMessage::Snapshot));
        self.bg_tx = Some(tx);
        self.bg_rx = Some(rx);
        self.request_refresh();
    }

    /// Ask for a new process snapshot and port list. Requests already in
    /// flight are not duplicated.
    fn request_refresh(&mut self) {
        self.last_request = Instant::now();
        if !self.snapshot_pending {
            if let Some(req) = &self.snapshot_req {
                if req.send(()).is_ok() {
                    self.snapshot_pending = true;
                }
            }
        }
        if !self.ports_pending {
            if let Some(tx) = &self.bg_tx {
                self.ports_pending = true;
                let tx = tx.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(ProcsBgMessage::Ports(ports::fetch_ports()));
                });
            }
        }
    }

    /// A snapshot or port list is being taken right now.
    pub fn is_refreshing(&self) -> bool {
        self.snapshot_pending || self.ports_pending
    }

    fn drain(&mut self) {
        let messages: Vec<_> = match &self.bg_rx {
            Some(rx) => rx.try_iter().collect(),
            None => return,
        };
        if messages.is_empty() {
            return;
        }
        for msg in messages {
            match msg {
                ProcsBgMessage::Snapshot(procs) => {
                    self.snapshot_pending = false;
                    self.panes.tab.list.apply_snapshot(procs);
                }
                ProcsBgMessage::Ports(result) => {
                    self.ports_pending = false;
                    self.panes.ports.apply(result);
                }
            }
        }
        self.panes.sync_detail();
    }

    fn process_events(
        &mut self,
        ctx: &mut AppContext,
        events: Vec<PaneEvent>,
    ) -> Result<PageAction> {
        for event in events {
            if pane::process_common_event(&mut self.pane, ctx, &event) {
                continue;
            }
            match event {
                PaneEvent::SelectionChanged => self.panes.sync_detail(),
                PaneEvent::JumpToProcess(pid) => {
                    if self.panes.tab.list.select_pid(Some(pid)) {
                        self.pane.set_focus(self.panes.ids.processes);
                        self.panes.sync_detail();
                    } else {
                        ctx.status_message = Some(format!("Process {pid} is not in the list"));
                    }
                }
                PaneEvent::JumpToMatch(forward) => {
                    let jumped = self
                        .pane
                        .jump_to_search_match(&mut self.panes, ctx, forward)
                        .is_some();
                    if jumped {
                        self.panes.sync_detail();
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
                self.request_refresh();
                return Ok(PageAction::None);
            }
        }
        let events = pane::dispatch_page_key(self, key);
        self.process_events(ctx, events)
    }
}

impl PageState for ProcsState {
    fn id(&self) -> &'static str {
        "procs"
    }

    fn label(&self) -> &'static str {
        "Procs"
    }

    fn help_bindings(&self) -> Vec<(String, String)> {
        use crate::core::keymap::help_section;
        let s = |k: &str, v: &str| (k.to_string(), v.to_string());
        let mut entries = vec![s("1 / 2 / 3 / 4 / 5", "Switch view")];
        entries.extend(self.view_keymap.help_entries());
        entries.extend(help_section("Processes"));
        entries.extend(self.panes.tab.list.keymap().help_entries());
        entries.extend(help_section("Ports"));
        entries.extend(self.panes.ports.keymap().help_entries());
        entries.extend(help_section("Detail"));
        entries.extend(self.panes.tab.detail.keymap().help_entries());
        entries
    }

    fn handle_key(&mut self, ctx: &mut AppContext, key: KeyEvent) -> Result<PageAction> {
        if self.pane.handle_search_input(&mut self.panes, ctx, key) {
            // Incremental search moves the list selection without an event.
            if self.pane.search.origin == self.panes.ids.processes {
                self.panes.sync_detail();
            }
            return Ok(PageAction::None);
        }
        self.handle_view_key(ctx, key)
    }

    fn render(&mut self, f: &mut Frame, ctx: &AppContext, area: Rect) {
        let frame = split_page_frame(area);
        status_bar::render_procs_header(f, ctx, self, frame.header);
        pane::render_page_content(self, f, ctx, frame.content);
        status_bar::render_procs_status_bar(f, ctx, self, frame.status_bar);
    }

    fn intercepts_all_keys(&self) -> bool {
        self.pane.search.active
    }

    fn on_tick(&mut self, _ctx: &mut AppContext) {
        // Ticks arrive every 250 ms but only while this page is active, so
        // the page stops sampling as soon as the user switches away.
        if self.initialized && !self.is_refreshing() && self.last_request.elapsed() >= self.interval
        {
            self.request_refresh();
        }
    }

    fn on_activate(&mut self, _ctx: &mut AppContext) {
        self.initialize();
    }

    fn drain_background(&mut self) {
        self.drain();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::keymap::KeyInput;
    use crate::procs::domain::types::{proc, Proto};
    use crossterm::event::{KeyCode, KeyEvent};

    fn key(s: &str) -> KeyEvent {
        let ki: KeyInput = s.parse().unwrap();
        KeyEvent::new(ki.code, ki.modifiers)
    }

    fn ctx() -> AppContext {
        AppContext {
            should_quit: false,
            active_page: 0,
            page_labels: vec![],
            show_help: false,
            status_message: None,
            error_dialog: None,
            workdir: std::path::PathBuf::from("."),
            needs_full_redraw: false,
        }
    }

    /// A page fed with fixture data, no background threads.
    fn state() -> ProcsState {
        let mut s = ProcsState::new(&Config::builtin()).expect("procs page");
        s.panes.tab.list.apply_snapshot(vec![
            proc(1, None, 0.0, 10),
            proc(20, Some(1), 5.0, 100),
            proc(30, Some(1), 1.0, 900),
        ]);
        // Already in the order `fetch_ports` produces (by port number).
        s.panes.ports.apply(Ok(vec![
            PortEntry {
                proto: Proto::Udp,
                addr: "*".into(),
                port: 5353,
                pid: None,
                name: None,
            },
            PortEntry {
                proto: Proto::Tcp,
                addr: "*".into(),
                port: 8080,
                pid: Some(30),
                name: Some("p30".into()),
            },
        ]));
        s.panes.sync_detail();
        s
    }

    #[test]
    fn pane_ids_tabs_and_bindings_from_default_kdl() {
        let cfg = Config::builtin().procs_page().unwrap();
        let ids = ProcsPaneIds::from_config(&cfg);
        assert_eq!((ids.processes, ids.ports, ids.detail), (0, 1, 2));
        assert_eq!(cfg.layout.tab_panes, vec![0, 1, 2]);
        assert_eq!(
            cfg.resolve_select_bindings().get(&ids.processes),
            Some(&ids.detail)
        );
    }

    #[test]
    fn keys_from_default_kdl_match_hardcoded_defaults() {
        let s = state();
        for k in [
            "j", "k", "g", "G", "Ctrl+d", "Ctrl+u", "/", "n", "N", "Enter", "i", "s", "Esc",
        ] {
            let a = s.panes.tab.list.keymap().lookup(key(k));
            let b = crate::procs::panes::processes::default_keymap();
            assert_eq!(
                format!("{:?}", a),
                format!("{:?}", b.lookup(key(k))),
                "processes key {k}"
            );
        }
        for k in ["j", "k", "/", "Enter", "Esc"] {
            let a = s.panes.ports.keymap().lookup(key(k));
            let b = crate::procs::panes::ports::default_keymap();
            assert_eq!(
                format!("{:?}", a),
                format!("{:?}", b.lookup(key(k))),
                "ports key {k}"
            );
        }
        for k in ["j", "k", "h", "Left", "Esc"] {
            let a = s.panes.tab.detail.keymap().lookup(key(k));
            let b = crate::procs::panes::detail::default_keymap();
            assert_eq!(
                format!("{:?}", a),
                format!("{:?}", b.lookup(key(k))),
                "detail key {k}"
            );
        }
    }

    #[test]
    fn selection_drives_detail_with_children_and_ports() {
        let mut s = state();
        let mut c = ctx();
        // CPU order: 1 (root) → 20, 30 as children.
        assert_eq!(s.panes.tab.detail.pid(), Some(1));
        s.handle_key(&mut c, key("j")).unwrap();
        s.handle_key(&mut c, key("j")).unwrap();
        assert_eq!(s.panes.tab.list.selected_pid(), Some(30));
        assert_eq!(s.panes.tab.detail.pid(), Some(30));
        let lines = DetailPane::lines(
            &DetailData {
                info: s.panes.tab.list.selected().unwrap().clone(),
                parent_name: s.panes.tab.list.name_of(1),
                children: s.panes.tab.list.children_of(30),
                ports: s.panes.ports.ports_of(30),
            },
            60,
        );
        let text: Vec<String> = lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        // 10 fields + Children (none) + Ports (1) + one port row.
        assert_eq!(text.len(), 13, "{text:?}");
        assert_eq!(text[1], "PPID      1 (p1)");
        assert_eq!(text[11], "Ports     1");
        assert_eq!(text[12], "          tcp *:8080");
    }

    #[test]
    fn enter_on_port_jumps_to_owner_and_focuses_processes() {
        let mut s = state();
        let mut c = ctx();
        s.handle_key(&mut c, key("Tab")).unwrap();
        assert_eq!(s.pane.focused_pane, s.panes.ids.ports);
        // Ports sort by number: 5353 (no access) first, then 8080 (pid 30).
        s.handle_key(&mut c, key("Enter")).unwrap();
        assert_eq!(s.pane.focused_pane, s.panes.ids.ports);
        assert!(c.status_message.as_deref().unwrap().contains("no access"));
        s.handle_key(&mut c, key("j")).unwrap();
        s.handle_key(&mut c, key("Enter")).unwrap();
        assert_eq!(s.pane.focused_pane, s.panes.ids.processes);
        assert_eq!(s.panes.tab.list.selected_pid(), Some(30));
        assert_eq!(s.panes.tab.detail.pid(), Some(30));
    }

    #[test]
    fn sort_cycles_and_keeps_selection() {
        let mut s = state();
        let mut c = ctx();
        s.handle_key(&mut c, key("j")).unwrap(); // pid 20
        assert_eq!(s.panes.tab.list.selected_pid(), Some(20));
        s.handle_key(&mut c, key("s")).unwrap(); // MEM: 30 before 20
        assert_eq!(s.panes.tab.list.sort.label(), "MEM");
        assert_eq!(s.panes.tab.list.selected_pid(), Some(20));
        assert_eq!(s.panes.tab.list.selected_idx, 2);
        assert_eq!(s.panes.tab.detail.pid(), Some(20));
    }

    #[test]
    fn help_lists_every_pane_and_the_view_switch() {
        let s = state();
        let help = s.help_bindings();
        assert_eq!(help[0].0, "1 / 2 / 3 / 4 / 5");
        let text: Vec<String> = help.iter().map(|(k, v)| format!("{k} {v}")).collect();
        for needle in [
            "── Processes ──",
            "── Ports ──",
            "── Detail ──",
            "Cycle sort",
            "Jump to owning process",
        ] {
            assert!(text.iter().any(|l| l.contains(needle)), "{needle}");
        }
        let _ = KeyCode::Null;
    }
}
