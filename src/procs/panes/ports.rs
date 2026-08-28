//! Top-right pane of the Procs page: listening TCP / UDP sockets and the
//! process that owns each one.

use crate::core::app::AppContext;
use crate::core::keymap::{
    nav_bindings, search_bindings, ActionHelp, Keymap, NavAction, SearchAction,
};
use crate::core::pane::{self, Pane, PaneEvent, PaneShared};
use crate::core::search::SearchMatch;
use crate::procs::domain::types::{PortEntry, Proto};
use crate::procs::panes::{dim, render_table_pane, NO_ACCESS};
use crossterm::event::KeyCode;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::ListItem;
use ratatui::{layout::Rect, Frame};

#[derive(Debug, Clone)]
pub enum PortsAction {
    Nav(NavAction),
    /// Select the owning process in the processes pane.
    JumpToProcess,
    Search(SearchAction),
    Esc,
}

crate::impl_pane_action_from_str!(
    PortsAction, nav: Nav, search: Search, esc: Esc,
    JumpToProcess
);

impl ActionHelp for PortsAction {
    fn label(&self) -> Option<&'static str> {
        match self {
            PortsAction::Nav(nav) => nav.label(),
            PortsAction::JumpToProcess => Some("Jump to owning process"),
            PortsAction::Search(sa) => sa.label(),
            PortsAction::Esc => Some("Clear search"),
        }
    }
}

pub fn default_keymap() -> Keymap<PortsAction> {
    Keymap::new()
        .bindings(nav_bindings(PortsAction::Nav))
        .bindings(search_bindings(PortsAction::Search))
        .key(KeyCode::Enter, PortsAction::JumpToProcess)
        .key(KeyCode::Esc, PortsAction::Esc)
}

const HEADER: &str = "PROTO ADDRESS                    PID  NAME";

pub struct PortsPane {
    pub entries: Vec<PortEntry>,
    pub selected_idx: usize,
    /// Why there is no list (tool missing, unsupported platform).
    pub notice: Option<String>,
    pub loading: bool,
    keymap: Keymap<PortsAction>,
    pane_id: usize,
    view_height: u16,
}

impl PortsPane {
    pub fn new(pane_id: usize) -> Self {
        Self {
            entries: Vec::new(),
            selected_idx: 0,
            notice: None,
            loading: true,
            keymap: default_keymap(),
            pane_id,
            view_height: 20,
        }
    }

    pub fn set_keymap(&mut self, km: Keymap<PortsAction>) {
        self.keymap = km;
    }

    pub fn keymap(&self) -> &Keymap<PortsAction> {
        &self.keymap
    }

    pub fn selected(&self) -> Option<&PortEntry> {
        self.entries.get(self.selected_idx)
    }

    /// Ports owned by `pid`.
    pub fn ports_of(&self, pid: u32) -> Vec<PortEntry> {
        self.entries
            .iter()
            .filter(|e| e.pid == Some(pid))
            .cloned()
            .collect()
    }

    /// Apply a fetch result, keeping the selection on the same socket.
    pub fn apply(&mut self, result: Result<Vec<PortEntry>, String>) {
        self.loading = false;
        match result {
            Ok(entries) => {
                let keep = self.selected().map(|e| (e.proto, e.address()));
                self.entries = entries;
                self.notice = None;
                self.selected_idx = keep
                    .and_then(|(proto, addr)| {
                        self.entries
                            .iter()
                            .position(|e| e.proto == proto && e.address() == addr)
                    })
                    .unwrap_or(self.selected_idx)
                    .min(self.entries.len().saturating_sub(1));
            }
            Err(notice) => {
                self.entries.clear();
                self.selected_idx = 0;
                self.notice = Some(notice);
            }
        }
    }

    fn execute(&mut self, shared: &PaneShared, action: PortsAction) -> Vec<PaneEvent> {
        if let Some(events) = pane::try_dispatch_search_esc(&action, shared, self.pane_id, vec![]) {
            return events;
        }
        match action {
            PortsAction::Nav(nav) => pane::execute_list_nav(
                nav,
                &mut self.selected_idx,
                self.entries.len(),
                Some(self.view_height),
            ),
            PortsAction::JumpToProcess => match self.selected() {
                Some(PortEntry { pid: Some(pid), .. }) => vec![PaneEvent::JumpToProcess(*pid)],
                Some(_) => vec![PaneEvent::StatusMessage(
                    "Owner of this port is not visible (no access)".to_string(),
                )],
                None => vec![],
            },
            _ => vec![],
        }
    }

    fn row_line(entry: &PortEntry) -> Line<'static> {
        let proto_style = Style::default().fg(match entry.proto {
            Proto::Tcp => Color::Green,
            Proto::Udp => Color::Magenta,
        });
        let mut spans = vec![
            Span::styled(format!("{:<6}", entry.proto.label()), proto_style),
            Span::raw(format!("{:<22} ", entry.address())),
        ];
        match entry.pid {
            Some(pid) => {
                spans.push(Span::styled(
                    format!("{pid:>7}  "),
                    Style::default().fg(Color::Cyan),
                ));
                spans.push(Span::raw(entry.name.clone().unwrap_or_default()));
            }
            None => spans.push(Span::styled(format!("{NO_ACCESS:>11}"), dim())),
        }
        Line::from(spans)
    }
}

impl Pane<PaneEvent> for PortsPane {
    crate::impl_handle_key!(keymap);

    fn render(&mut self, f: &mut Frame, _ctx: &AppContext, shared: &PaneShared, area: Rect) {
        self.view_height = area.height.saturating_sub(3);
        let empty = match &self.notice {
            Some(n) => Some(n.clone()),
            None if self.entries.is_empty() => Some(
                if self.loading {
                    "Loading..."
                } else {
                    "(no listening ports)"
                }
                .to_string(),
            ),
            None => None,
        };
        let selected = (!self.entries.is_empty()).then_some(self.selected_idx);
        render_table_pane(
            f,
            area,
            shared,
            self.pane_id,
            "Ports",
            HEADER,
            selected,
            shared.focused_pane == self.pane_id,
            empty.as_deref(),
            |match_set, current_match_idx| {
                self.entries
                    .iter()
                    .enumerate()
                    .map(|(idx, e)| {
                        let mut li = ListItem::new(Self::row_line(e));
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
        pane::collect_list_search_matches(&self.entries, query, |e| {
            format!(
                "{} {} {}",
                e.proto.label(),
                e.address(),
                e.name.as_deref().unwrap_or("")
            )
        })
    }

    crate::impl_list_pane_selection!();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(port: u16, pid: Option<u32>) -> PortEntry {
        PortEntry {
            proto: Proto::Tcp,
            addr: "*".into(),
            port,
            pid,
            name: pid.map(|p| format!("p{p}")),
        }
    }

    #[test]
    fn apply_keeps_selection_and_reports_notice() {
        let mut pane = PortsPane::new(0);
        pane.apply(Ok(vec![
            entry(22, Some(1)),
            entry(80, Some(2)),
            entry(443, None),
        ]));
        pane.selected_idx = 1;
        pane.apply(Ok(vec![
            entry(22, Some(1)),
            entry(25, Some(9)),
            entry(80, Some(2)),
        ]));
        assert_eq!(pane.selected().map(|e| e.port), Some(80));
        assert_eq!(pane.ports_of(2).len(), 1);
        assert!(pane.ports_of(7).is_empty());

        pane.apply(Err("`lsof` not found".into()));
        assert!(pane.entries.is_empty());
        assert_eq!(pane.notice.as_deref(), Some("`lsof` not found"));
        assert!(!pane.loading);
    }

    #[test]
    fn jump_needs_a_visible_owner() {
        let shared = PaneShared {
            focused_pane: 0,
            previous_pane: 0,
            search: crate::core::search::SearchState::new(),
        };
        let mut pane = PortsPane::new(0);
        pane.apply(Ok(vec![entry(22, Some(7)), entry(80, None)]));
        let ev = pane.execute(&shared, PortsAction::JumpToProcess);
        assert!(matches!(ev.as_slice(), [PaneEvent::JumpToProcess(7)]));
        pane.selected_idx = 1;
        let ev = pane.execute(&shared, PortsAction::JumpToProcess);
        assert!(matches!(ev.as_slice(), [PaneEvent::StatusMessage(_)]));
    }
}
