//! Top pane of the Procs page: system-wide CPU and memory history graphs,
//! drawn btop-style — filled multi-row area charts whose sample columns are
//! colored by load. Everything drawn here is a machine total — numbers
//! only, never a process name, user or path — so recordings stay clean even
//! without the `VIG_PROCS_ROOT_PID` filter (which deliberately does not
//! apply here).

use crate::core::app::AppContext;
use crate::core::keymap::{ActionHelp, Keymap};
use crate::core::pane::{Pane, PaneEvent, PaneShared};
use crate::core::theme;
use crate::files::domain::fs::human_size;
use crate::procs::domain::history::{Ring, SystemSample};
use crate::procs::panes::{area_chart, dim, load_style};
use crossterm::event::KeyCode;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
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

/// `3.2G/16.0G` — used / total in human units.
pub fn ratio_label(used: u64, total: u64) -> String {
    format!("{}/{}", human_size(used), human_size(total))
}

/// `used` as a percentage of `total` (0 when there is no total).
pub fn used_pct(used: u64, total: u64) -> f32 {
    if total == 0 {
        0.0
    } else {
        (used as f64 / total as f64 * 100.0) as f32
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

    /// Global CPU% samples, oldest → newest.
    fn cpu_series(&self) -> Vec<f32> {
        self.history.iter().map(|s| s.cpu).collect()
    }

    /// Used memory as a percentage of the total, oldest → newest.
    fn mem_series(&self) -> Vec<f32> {
        self.history
            .iter()
            .map(|s| used_pct(s.mem_used, s.mem_total))
            .collect()
    }

    fn execute(&mut self, _shared: &PaneShared, action: GraphsAction) -> Vec<PaneEvent> {
        match action {
            GraphsAction::TogglePerCore => vec![PaneEvent::ToggleCpuCores],
            GraphsAction::Esc => vec![PaneEvent::SetFocus(self.processes_pane_id)],
        }
    }

    fn render_cpu(&self, f: &mut Frame, area: Rect, s: &SystemSample) {
        let mut spans = vec![
            Span::styled("CPU  ", Style::default().fg(Color::Cyan)),
            Span::styled(format!("{:.0}%", s.cpu), load_style(s.cpu)),
            Span::styled(format!("  peak {:.0}%", self.cpu_peak()), dim()),
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
        let lines = if self.per_core {
            core_grid_lines(&s.per_core, graph.height as usize, graph.width as usize)
        } else {
            area_chart(
                &self.cpu_series(),
                graph.height as usize,
                graph.width as usize,
                100.0,
            )
        };
        f.render_widget(Paragraph::new(lines), graph);
    }

    fn render_mem(&self, f: &mut Frame, area: Rect, s: &SystemSample) {
        let swap_line = s.swap_total > 0 && area.height >= 3;
        let pct = used_pct(s.mem_used, s.mem_total);
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("MEM  ", Style::default().fg(Color::Cyan)),
                Span::raw(ratio_label(s.mem_used, s.mem_total)),
                Span::styled(format!("  {pct:.0}%"), load_style(pct)),
            ])),
            Rect { height: 1, ..area },
        );
        let graph = Rect {
            y: area.y + 1,
            height: area.height - 1 - u16::from(swap_line),
            ..area
        };
        if graph.height > 0 {
            let lines = area_chart(
                &self.mem_series(),
                graph.height as usize,
                graph.width as usize,
                100.0,
            );
            f.render_widget(Paragraph::new(lines), graph);
        }
        if swap_line {
            let spct = used_pct(s.swap_used, s.swap_total);
            f.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled("Swp  ", Style::default().fg(Color::Cyan)),
                    Span::styled(ratio_label(s.swap_used, s.swap_total), dim()),
                    Span::styled(format!("  {spct:.0}%"), load_style(spct)),
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
/// column, then the next. Every cell is `<idx> <bar> <pct>` — numbers only —
/// with the bar and the percentage in the shared load gradient.
fn core_grid_lines(cores: &[f32], rows: usize, width: usize) -> Vec<Line<'static>> {
    if cores.is_empty() || rows == 0 || width == 0 {
        return vec![];
    }
    let mut cols = cores.len().div_ceil(rows);
    // Never let the fixed per-cell overhead push a row wider than the pane:
    // cap the column count to what fits and let the grid use more rows.
    let idx_w_est = (cores.len().saturating_sub(1)).to_string().len().max(2);
    let max_cols = (width / (idx_w_est + 1 + 6 + 3)).max(1);
    cols = cols.min(max_cols);
    let rows_used = cores.len().div_ceil(cols);
    // index + space + " 100% " — the index column grows with the core count.
    let idx_w = idx_w_est;
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
            spans.push(Span::styled(format!(" {:>3.0}% ", pct), load_style(pct)));
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
    fn labels_format_ratios_and_percentages() {
        let g = 1024 * 1024 * 1024;
        assert_eq!(ratio_label((32 * g) / 10, 16 * g), "3.2G/16.0G");
        assert_eq!(ratio_label(0, 0), "0B/0B");
        assert_eq!(ratio_label(g / 2, g), "512.0M/1.0G");
        assert!((used_pct((32 * g) / 10, 16 * g) - 20.0).abs() < 0.01);
        assert_eq!(used_pct(0, 0), 0.0);
        assert_eq!(used_pct(g / 2, g), 50.0);
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
        // Oldest → newest, so the chart's right edge is the latest sample.
        assert_eq!(pane.cpu_series(), vec![90.0, 20.0]);
        // 4G used of 16G → a flat 25% memory series for the MEM chart.
        assert_eq!(pane.mem_series(), vec![25.0, 25.0]);
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
        let lines = core_grid_lines(&[10.0, 20.0, 30.0, 90.0], 2, 40);
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
        assert!(text[1].contains("90%"), "{text:?}");
        // The percent text carries the same load color as its bar.
        let pct_span = lines[1].spans.last().unwrap();
        assert_eq!(pct_span.style.fg, Some(Color::Red)); // 90%
        assert!(core_grid_lines(&[], 2, 40).is_empty());
    }
}
