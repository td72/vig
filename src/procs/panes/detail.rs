//! Bottom-right pane of the Procs page: everything known about the selected
//! process. Environment variables are never part of it.

use crate::core::app::AppContext;
use crate::core::keymap::{half_page_step, nav_bindings, ActionHelp, Keymap, NavAction};
use crate::core::pane::{Pane, PaneEvent, PaneShared};
use crate::core::theme;
use crate::files::domain::fs::human_size;
use crate::procs::domain::types::{format_elapsed, PortEntry, ProcessInfo};
use crate::procs::panes::{dim, spark_string, NO_ACCESS};
use crossterm::event::KeyCode;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::{layout::Rect, Frame};

#[derive(Debug, Clone)]
pub enum DetailAction {
    Nav(NavAction),
    /// Return focus to the process list.
    Back,
    Esc,
}

crate::impl_pane_action_from_str!(DetailAction, nav: Nav, Back, Esc);

impl ActionHelp for DetailAction {
    fn label(&self) -> Option<&'static str> {
        match self {
            DetailAction::Nav(NavAction::MoveDown) => Some("Scroll down"),
            DetailAction::Nav(NavAction::MoveUp) => Some("Scroll up"),
            DetailAction::Nav(nav) => nav.label(),
            DetailAction::Back => Some("Back to process list"),
            DetailAction::Esc => Some("Back to process list"),
        }
    }
}

pub fn default_keymap() -> Keymap<DetailAction> {
    Keymap::new()
        .bindings(nav_bindings(DetailAction::Nav))
        .key(KeyCode::Char('h'), DetailAction::Back)
        .key(KeyCode::Left, DetailAction::Back)
        .key(KeyCode::Esc, DetailAction::Esc)
}

/// What the detail pane shows: the process plus what the other panes know
/// about it.
#[derive(Debug, Clone)]
pub struct DetailData {
    pub info: ProcessInfo,
    pub parent_name: Option<String>,
    pub children: Vec<(u32, String)>,
    pub ports: Vec<PortEntry>,
    /// CPU% samples of this pid, oldest first (may be empty).
    pub cpu_history: Vec<f32>,
    /// RSS samples of this pid in bytes, oldest first (may be empty).
    pub rss_history: Vec<u64>,
}

const LABEL_W: usize = 9;
/// Widest history sparkline drawn under the CPU / MEM fields.
const SPARK_W: usize = 48;

pub struct DetailPane {
    pane_id: usize,
    list_pane_id: usize,
    keymap: Keymap<DetailAction>,
    data: Option<DetailData>,
    scroll: usize,
    /// Rows produced by the last render (wrapping depends on the width).
    line_count: usize,
    view_height: u16,
}

impl DetailPane {
    pub fn new(pane_id: usize, list_pane_id: usize) -> Self {
        Self {
            pane_id,
            list_pane_id,
            keymap: default_keymap(),
            data: None,
            scroll: 0,
            line_count: 0,
            view_height: 20,
        }
    }

    pub fn set_keymap(&mut self, km: Keymap<DetailAction>) {
        self.keymap = km;
    }

    pub fn keymap(&self) -> &Keymap<DetailAction> {
        &self.keymap
    }

    /// Show `data`. The scroll position survives a refresh of the same
    /// process and resets when a different one is selected.
    pub fn load(&mut self, data: Option<DetailData>) {
        let same = match (&self.data, &data) {
            (Some(a), Some(b)) => a.info.pid == b.info.pid,
            (None, None) => true,
            _ => false,
        };
        if !same {
            self.scroll = 0;
        }
        self.data = data;
    }

    #[cfg(test)]
    pub fn pid(&self) -> Option<u32> {
        self.data.as_ref().map(|d| d.info.pid)
    }

    /// Sample counts of the loaded CPU / RSS histories (tests only).
    #[cfg(test)]
    pub fn history_lens(&self) -> Option<(usize, usize)> {
        self.data
            .as_ref()
            .map(|d| (d.cpu_history.len(), d.rss_history.len()))
    }

    /// Build the text for `width` columns. Public for tests.
    pub fn lines(data: &DetailData, width: usize) -> Vec<Line<'static>> {
        let info = &data.info;
        let value_w = width.saturating_sub(LABEL_W + 1).max(8);
        let mut out = Vec::new();
        let plain = |s: String| vec![Span::raw(s)];
        let access = |v: &Option<String>| match v {
            Some(s) => vec![Span::raw(s.clone())],
            None => vec![Span::styled(NO_ACCESS.to_string(), dim())],
        };
        let count = |n: usize| {
            if n == 0 {
                vec![Span::styled("-".to_string(), dim())]
            } else {
                plain(n.to_string())
            }
        };

        field(&mut out, "PID", plain(info.pid.to_string()));
        field(
            &mut out,
            "PPID",
            match (info.ppid, &data.parent_name) {
                (Some(pp), Some(name)) => vec![
                    Span::raw(pp.to_string()),
                    Span::styled(format!(" ({name})"), dim()),
                ],
                (Some(pp), None) => plain(pp.to_string()),
                (None, _) => vec![Span::styled("-".to_string(), dim())],
            },
        );
        field(&mut out, "User", access(&info.user));
        field(&mut out, "State", plain(info.status.clone()));
        field(
            &mut out,
            "Started",
            match info.run_time {
                Some(secs) => plain(format!("{} ago", format_elapsed(secs))),
                None => vec![Span::styled("-".to_string(), dim())],
            },
        );
        field(&mut out, "CPU", plain(format!("{:.1}%", info.cpu)));
        if data.cpu_history.len() >= 2 {
            let vals: Vec<f64> = data.cpu_history.iter().map(|&v| f64::from(v)).collect();
            continuation(
                &mut out,
                vec![Span::styled(
                    spark_string(&vals, 100.0, value_w.min(SPARK_W)),
                    Style::default().fg(Color::Green),
                )],
            );
        }
        field(&mut out, "MEM", plain(human_size(info.rss)));
        if data.rss_history.len() >= 2 {
            let max = data.rss_history.iter().copied().max().unwrap_or(1) as f64;
            let vals: Vec<f64> = data.rss_history.iter().map(|&v| v as f64).collect();
            continuation(
                &mut out,
                vec![Span::styled(
                    spark_string(&vals, max, value_w.min(SPARK_W)),
                    Style::default().fg(Color::Cyan),
                )],
            );
        }
        let bold = Style::default().add_modifier(Modifier::BOLD);
        for (i, chunk) in wrap_chars(&info.cmd, value_w).into_iter().enumerate() {
            let spans = vec![Span::styled(chunk, bold)];
            if i == 0 {
                field(&mut out, "Command", spans);
            } else {
                continuation(&mut out, spans);
            }
        }
        field(&mut out, "CWD", access(&info.cwd));
        field(&mut out, "Exe", access(&info.exe));
        field(&mut out, "Children", count(data.children.len()));
        for (pid, name) in &data.children {
            continuation(
                &mut out,
                vec![
                    Span::styled(format!("{pid:>6} "), Style::default().fg(Color::Cyan)),
                    Span::raw(name.clone()),
                ],
            );
        }
        field(&mut out, "Ports", count(data.ports.len()));
        for p in &data.ports {
            continuation(
                &mut out,
                vec![
                    Span::styled(format!("{:<4}", p.proto.label()), dim()),
                    Span::raw(p.address()),
                ],
            );
        }
        out
    }

    fn execute(&mut self, _shared: &PaneShared, action: DetailAction) -> Vec<PaneEvent> {
        match action {
            DetailAction::Nav(nav) => {
                let max = self.line_count.saturating_sub(self.view_height as usize);
                let half = half_page_step(self.view_height) as usize;
                self.scroll = match nav {
                    NavAction::MoveDown => self.scroll + 1,
                    NavAction::MoveUp => self.scroll.saturating_sub(1),
                    NavAction::HalfPageDown => self.scroll + half,
                    NavAction::HalfPageUp => self.scroll.saturating_sub(half),
                    NavAction::JumpTop => 0,
                    NavAction::JumpBottom => max,
                }
                .min(max);
                vec![]
            }
            DetailAction::Back | DetailAction::Esc => vec![PaneEvent::SetFocus(self.list_pane_id)],
        }
    }
}

/// `Label     value…` row.
fn field(out: &mut Vec<Line<'static>>, label: &str, spans: Vec<Span<'static>>) {
    let mut line = vec![Span::styled(
        format!("{label:<LABEL_W$} "),
        Style::default().fg(Color::Cyan),
    )];
    line.extend(spans);
    out.push(Line::from(line));
}

/// Row under a field, indented past the label column.
fn continuation(out: &mut Vec<Line<'static>>, spans: Vec<Span<'static>>) {
    let mut line = vec![Span::raw(" ".repeat(LABEL_W + 1))];
    line.extend(spans);
    out.push(Line::from(line));
}

/// Greedy wrap at character boundaries (no word splitting logic: command
/// lines are mostly long paths without convenient spaces).
pub fn wrap_chars(s: &str, width: usize) -> Vec<String> {
    if s.is_empty() {
        return vec![String::new()];
    }
    let width = width.max(1);
    let chars: Vec<char> = s.chars().collect();
    chars.chunks(width).map(|c| c.iter().collect()).collect()
}

impl Pane<PaneEvent> for DetailPane {
    crate::impl_handle_key!(keymap);

    fn render(&mut self, f: &mut Frame, _ctx: &AppContext, shared: &PaneShared, area: Rect) {
        self.view_height = area.height.saturating_sub(2);
        let width = area.width.saturating_sub(2) as usize;
        let title = match &self.data {
            Some(d) => format!("{} ({})", d.info.name, d.info.pid),
            None => "Detail".to_string(),
        };
        let block = theme::pane_block(&title, shared.focused_pane == self.pane_id);
        let lines = match &self.data {
            Some(d) => Self::lines(d, width),
            None => vec![Line::from(Span::styled("  Select a process", dim()))],
        };
        self.line_count = lines.len();
        self.scroll = self
            .scroll
            .min(self.line_count.saturating_sub(self.view_height as usize));
        let para = Paragraph::new(lines)
            .block(block)
            .scroll((self.scroll as u16, 0));
        f.render_widget(para, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procs::domain::types::{proc, Proto};

    fn text(lines: &[Line<'_>]) -> Vec<String> {
        lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect()
    }

    #[test]
    fn wrap_splits_at_width() {
        assert_eq!(wrap_chars("", 4), vec![""]);
        assert_eq!(wrap_chars("abcdef", 4), vec!["abcd", "ef"]);
        assert_eq!(wrap_chars("abc", 0), vec!["a", "b", "c"]);
    }

    #[test]
    fn lines_cover_every_field_and_mark_no_access() {
        let mut info = proc(42, Some(1), 3.25, 2 * 1024 * 1024);
        info.cmd = "/usr/local/bin/server --port 8080 --verbose".into();
        info.run_time = Some(3600 * 3 + 60 * 12);
        info.user = Some("alice".into());
        let data = DetailData {
            info,
            parent_name: Some("launchd".into()),
            children: vec![(43, "worker".into())],
            ports: vec![PortEntry {
                proto: Proto::Tcp,
                addr: "127.0.0.1".into(),
                port: 8080,
                pid: Some(42),
                name: Some("server".into()),
            }],
            cpu_history: vec![],
            rss_history: vec![],
        };
        let t = text(&DetailPane::lines(&data, 40));
        assert_eq!(t[0], "PID       42");
        assert_eq!(t[1], "PPID      1 (launchd)");
        assert_eq!(t[2], "User      alice");
        assert_eq!(t[3], "State     Run");
        assert_eq!(t[4], "Started   3h 12m ago");
        assert_eq!(t[5], "CPU       3.2%");
        assert_eq!(t[6], "MEM       2.0M");
        // 40 cols − 10 label = 30 chars per command row, wrapped.
        assert_eq!(t[7], "Command   /usr/local/bin/server --port 8");
        assert_eq!(t[8], "          080 --verbose");
        assert_eq!(t[9], format!("CWD       {NO_ACCESS}"));
        assert_eq!(t[10], format!("Exe       {NO_ACCESS}"));
        assert_eq!(t[11], "Children  1");
        assert_eq!(t[12], "              43 worker");
        assert_eq!(t[13], "Ports     1");
        assert_eq!(t[14], "          tcp 127.0.0.1:8080");
        assert_eq!(t.len(), 15);
        // Nothing resembling an environment dump is ever rendered.
        assert!(t.iter().all(|l| !l.contains("PATH=")));
    }

    #[test]
    fn history_sparklines_sit_under_cpu_and_mem() {
        let data = DetailData {
            info: proc(42, None, 50.0, 1024),
            parent_name: None,
            children: vec![],
            ports: vec![],
            cpu_history: vec![0.0, 50.0, 100.0],
            rss_history: vec![512, 1024],
        };
        let t = text(&DetailPane::lines(&data, 40));
        // CPU row, then its sparkline; MEM row, then its sparkline. Both
        // right-aligned: latest sample at the right edge.
        assert_eq!(t[5], "CPU       50.0%");
        assert!(t[6].ends_with("▁▅█"), "{:?}", t[6]);
        assert!(t[6].starts_with(&" ".repeat(LABEL_W + 1)), "{:?}", t[6]);
        assert_eq!(t[7], "MEM       1.0K");
        assert!(t[8].ends_with("▅█"), "{:?}", t[8]);

        // A single sample draws no sparkline (a one-column graph is noise).
        let one = DetailData {
            cpu_history: vec![1.0],
            rss_history: vec![],
            ..data
        };
        let t = text(&DetailPane::lines(&one, 40));
        assert_eq!(t[6], "MEM       1.0K");
    }

    #[test]
    fn load_resets_scroll_only_on_a_different_pid() {
        let mut pane = DetailPane::new(1, 0);
        let d = |pid| DetailData {
            info: proc(pid, None, 0.0, 0),
            parent_name: None,
            children: vec![],
            ports: vec![],
            cpu_history: vec![],
            rss_history: vec![],
        };
        pane.load(Some(d(1)));
        pane.scroll = 5;
        pane.load(Some(d(1)));
        assert_eq!(pane.scroll, 5);
        pane.load(Some(d(2)));
        assert_eq!(pane.scroll, 0);
        assert_eq!(pane.pid(), Some(2));
    }
}
