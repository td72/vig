//! Top pane of the Procs page: system-wide CPU and memory history graphs.
//! Everything drawn here is a machine total — numbers only, never a
//! process name, user or path — so recordings stay clean even without the
//! `VIG_PROCS_ROOT_PID` filter (which deliberately does not apply here).

use crate::core::app::AppContext;
use crate::core::keymap::{ActionHelp, Keymap};
use crate::core::pane::{Pane, PaneEvent, PaneShared};
use crate::core::theme;
use crate::files::domain::fs::human_size;
use crate::procs::domain::history::{Ring, SystemSample};
use crate::procs::panes::dim;
use crossterm::event::KeyCode;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, RenderDirection, Sparkline};
use ratatui::{layout::Rect, Frame};

#[derive(Debug, Clone)]
pub enum GraphsAction {
    /// Switch the CPU graph between history and per-core bars.
    TogglePerCore,
    Esc,
}

impl std::str::FromStr for GraphsAction {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "TogglePerCore" => Ok(GraphsAction::TogglePerCore),
            "Esc" => Ok(GraphsAction::Esc),
            _ => Err(format!("Unknown action: {s}")),
        }
    }
}

impl ActionHelp for GraphsAction {
    fn label(&self) -> Option<&'static str> {
        match self {
            GraphsAction::TogglePerCore => Some("Toggle per-core CPU bars"),
            GraphsAction::Esc => Some("Back to process list"),
        }
    }
}

pub fn default_keymap() -> Keymap<GraphsAction> {
    Keymap::new()
        .key(KeyCode::Char('c'), GraphsAction::TogglePerCore)
        .key(KeyCode::Esc, GraphsAction::Esc)
}

/// `3.2G / 16.0G (20%)` — used / total with a rounded percentage.
pub fn usage_label(used: u64, total: u64) -> String {
    let pct = if total == 0 {
        0
    } else {
        (used as f64 / total as f64 * 100.0).round() as u64
    };
    format!("{} / {} ({pct}%)", human_size(used), human_size(total))
}

/// Style for a CPU percentage (calm, busy, saturated).
fn load_style(pct: f32) -> Style {
    if pct >= 80.0 {
        Style::default().fg(Color::Red)
    } else if pct >= 50.0 {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::Green)
    }
}

pub struct GraphsPane {
    pane_id: usize,
    processes_pane_id: usize,
    keymap: Keymap<GraphsAction>,
    /// System totals, oldest first; capacity is `procs-history`.
    history: Ring<SystemSample>,
    /// `true` shows one bar per core instead of the CPU history.
    pub per_core: bool,
}

impl GraphsPane {
    pub fn new(pane_id: usize, processes_pane_id: usize, capacity: usize) -> Self {
        Self {
            pane_id,
            processes_pane_id,
            keymap: default_keymap(),
            history: Ring::new(capacity),
            per_core: false,
        }
    }

    pub fn set_keymap(&mut self, km: Keymap<GraphsAction>) {
        self.keymap = km;
    }

    pub fn keymap(&self) -> &Keymap<GraphsAction> {
        &self.keymap
    }

    /// Append one snapshot's machine totals.
    pub fn record(&mut self, sample: SystemSample) {
        self.history.push(sample);
    }

    pub fn toggle_per_core(&mut self) {
        self.per_core = !self.per_core;
    }

    #[cfg(test)]
    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    /// Highest global CPU% in the buffer.
    fn cpu_peak(&self) -> f32 {
        self.history.iter().map(|s| s.cpu).fold(0.0, f32::max)
    }

    fn execute(&mut self, _shared: &PaneShared, action: GraphsAction) -> Vec<PaneEvent> {
        match action {
            GraphsAction::TogglePerCore => vec![PaneEvent::ToggleCpuCores],
            GraphsAction::Esc => vec![PaneEvent::SetFocus(self.processes_pane_id)],
        }
    }

    /// Newest-first series for a right-to-left sparkline: the latest sample
    /// sits at the right edge and history grows leftward, so a buffer that
    /// is not full yet is right-aligned.
    fn spark_data(&self, width: usize, value: impl Fn(&SystemSample) -> u64) -> Vec<u64> {
        self.history.iter().rev().map(value).take(width).collect()
    }

    fn render_cpu(&self, f: &mut Frame, area: Rect, s: &SystemSample) {
        let mut spans = vec![
            Span::styled("CPU  ", Style::default().fg(Color::Cyan)),
            Span::styled(format!("{:.1}%", s.cpu), load_style(s.cpu)),
            Span::styled(format!("  peak {:.1}%", self.cpu_peak()), dim()),
        ];
        if self.per_core {
            spans.push(Span::styled(format!("  {} cores", s.per_core.len()), dim()));
        }
        f.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect { height: 1, ..area },
        );
        let graph = Rect {
            y: area.y + 1,
            height: area.height - 1,
            ..area
        };
        if graph.height == 0 {
            return;
        }
        if self.per_core {
            let lines = core_grid_lines(&s.per_core, graph.height as usize, graph.width as usize);
            f.render_widget(Paragraph::new(lines), graph);
        } else {
            let data = self.spark_data(graph.width as usize, |s| s.cpu.round() as u64);
            f.render_widget(
                Sparkline::default()
                    .data(data)
                    .max(100)
                    .direction(RenderDirection::RightToLeft)
                    .style(Style::default().fg(Color::Green)),
                graph,
            );
        }
    }

    fn render_mem(&self, f: &mut Frame, area: Rect, s: &SystemSample) {
        let swap_line = s.swap_total > 0 && area.height >= 3;
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("MEM  ", Style::default().fg(Color::Cyan)),
                Span::raw(usage_label(s.mem_used, s.mem_total)),
            ])),
            Rect { height: 1, ..area },
        );
        let graph = Rect {
            y: area.y + 1,
            height: area.height - 1 - u16::from(swap_line),
            ..area
        };
        if graph.height > 0 {
            let data = self.spark_data(graph.width as usize, |s| s.mem_used);
            f.render_widget(
                Sparkline::default()
                    .data(data)
                    .max(s.mem_total.max(1))
                    .direction(RenderDirection::RightToLeft)
                    .style(Style::default().fg(Color::Cyan)),
                graph,
            );
        }
        if swap_line {
            f.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled("Swap ", Style::default().fg(Color::Cyan)),
                    Span::styled(usage_label(s.swap_used, s.swap_total), dim()),
                ])),
                Rect {
                    y: area.y + area.height - 1,
                    height: 1,
                    ..area
                },
            );
        }
    }
}

/// Lay the per-core bars out in columns: cores fill the rows of the first
/// column, then the next. Every cell is `<idx> <bar> <pct>` — numbers only.
fn core_grid_lines(cores: &[f32], rows: usize, width: usize) -> Vec<Line<'static>> {
    if cores.is_empty() || rows == 0 || width == 0 {
        return vec![];
    }
    let cols = cores.len().div_ceil(rows);
    let rows_used = cores.len().div_ceil(cols);
    // index + space + " 100% " — the index column grows with the core count.
    let idx_w = (cores.len().saturating_sub(1)).to_string().len().max(2);
    let fixed = idx_w + 1 + 6;
    let cell_w = (width / cols).max(fixed + 3);
    let bar_w = cell_w.saturating_sub(fixed).max(3);
    let mut lines = Vec::new();
    for row in 0..rows_used {
        let mut spans = Vec::new();
        for col in 0..cols {
            let Some(&pct) = cores.get(col * rows_used + row) else {
                continue;
            };
            let filled = ((f64::from(pct) / 100.0 * bar_w as f64).round() as usize).min(bar_w);
            spans.push(Span::styled(
                format!("{:>idx_w$} ", col * rows_used + row),
                Style::default().fg(Color::Cyan),
            ));
            spans.push(Span::styled("█".repeat(filled), load_style(pct)));
            spans.push(Span::styled("░".repeat(bar_w - filled), dim()));
            spans.push(Span::raw(format!(" {:>3.0}% ", pct)));
        }
        lines.push(Line::from(spans));
    }
    lines
}

impl Pane<PaneEvent> for GraphsPane {
    crate::impl_handle_key!(keymap);

    fn render(&mut self, f: &mut Frame, _ctx: &AppContext, shared: &PaneShared, area: Rect) {
        let title = if self.per_core {
            "System [per-core]"
        } else {
            "System"
        };
        let block = theme::pane_block(title, shared.focused_pane == self.pane_id);
        let inner = block.inner(area);
        f.render_widget(block, area);
        if inner.height == 0 || inner.width == 0 {
            return;
        }
        let Some(sample) = self.history.last().cloned() else {
            f.render_widget(
                Paragraph::new(Line::from(Span::styled("  Sampling...", dim()))),
                inner,
            );
            return;
        };
        let cpu_h = (inner.height / 2).max(1).min(inner.height);
        let cpu_area = Rect {
            height: cpu_h,
            ..inner
        };
        self.render_cpu(f, cpu_area, &sample);
        let mem_area = Rect {
            y: inner.y + cpu_h,
            height: inner.height - cpu_h,
            ..inner
        };
        if mem_area.height > 0 {
            self.render_mem(f, mem_area, &sample);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(cpu: f32) -> SystemSample {
        SystemSample {
            cpu,
            per_core: vec![cpu, cpu / 2.0],
            mem_used: 4 * 1024 * 1024 * 1024,
            mem_total: 16 * 1024 * 1024 * 1024,
            swap_used: 0,
            swap_total: 0,
        }
    }

    #[test]
    fn labels_format_percentages_and_sizes() {
        let g = 1024 * 1024 * 1024;
        assert_eq!(usage_label((32 * g) / 10, 16 * g), "3.2G / 16.0G (20%)");
        assert_eq!(usage_label(0, 0), "0B / 0B (0%)");
        assert_eq!(usage_label(g / 2, g), "512.0M / 1.0G (50%)");
    }

    #[test]
    fn record_caps_history_and_tracks_peak() {
        let mut pane = GraphsPane::new(3, 0, 2);
        assert_eq!(pane.history_len(), 0);
        pane.record(sample(10.0));
        pane.record(sample(90.0));
        pane.record(sample(20.0)); // evicts the 10% sample
        assert_eq!(pane.history_len(), 2);
        assert_eq!(pane.cpu_peak(), 90.0);
        // Newest first, so the sparkline's right edge is the latest sample.
        assert_eq!(pane.spark_data(10, |s| s.cpu as u64), vec![20, 90]);
        assert_eq!(pane.spark_data(1, |s| s.cpu as u64), vec![20]);
    }

    #[test]
    fn toggle_emits_event_and_esc_returns_to_processes() {
        let shared = PaneShared {
            focused_pane: 3,
            previous_pane: 0,
            search: crate::core::search::SearchState::new(),
        };
        let mut pane = GraphsPane::new(3, 0, 4);
        assert!(!pane.per_core);
        let ev = pane.execute(&shared, GraphsAction::TogglePerCore);
        assert!(matches!(ev.as_slice(), [PaneEvent::ToggleCpuCores]));
        let ev = pane.execute(&shared, GraphsAction::Esc);
        assert!(matches!(ev.as_slice(), [PaneEvent::SetFocus(0)]));
    }

    #[test]
    fn core_grid_lays_cores_out_in_columns() {
        // 4 cores in 2 rows → 2 columns; every index appears exactly once.
        let lines = core_grid_lines(&[10.0, 20.0, 30.0, 40.0], 2, 40);
        assert_eq!(lines.len(), 2);
        let text: Vec<String> = lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        assert!(text[0].starts_with(" 0 "), "{text:?}");
        assert!(text[1].starts_with(" 1 "), "{text:?}");
        assert!(text[0].contains(" 2 "), "{text:?}");
        assert!(text[1].contains(" 3 "), "{text:?}");
        assert!(text[0].contains("10%"), "{text:?}");
        assert!(text[1].contains("40%"), "{text:?}");
        assert!(core_grid_lines(&[], 2, 40).is_empty());
    }
}
