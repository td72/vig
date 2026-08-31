# Procs View

![procs demo](../../../assets/demo-procs.gif)

A read-only view of what is running on your machine: the processes as a tree
by parent pid with CPU % and resident memory, the listening TCP / UDP ports
with the process that owns each one, and a detail of the selected process
(pid, ppid, user, state, uptime, CPU / memory, full command line, cwd,
executable, children, listening ports). Values that need privileges you do
not have are shown as `(no access)`.

Processes come from [sysinfo](https://crates.io/crates/sysinfo); ports from
`lsof` on macOS and `ss` on Linux. Both are re-read every 2 seconds while the
view is shown (`procs-refresh-interval` in the config) and on `r`.

## The System graphs

The System pane on top graphs the machine totals as btop-style filled area
charts: the global CPU % (with the recent peak) and the used memory over the
last `procs-history` samples (120 by default — 4 minutes at the 2 s interval),
plus a `Swp` line when swap is present. Every sample column is colored by its
load — green below 50 %, yellow from 50 %, red from 80 % — and the percentage
labels carry the same color.

`c` swaps the CPU chart for one small gauge per core in the same gradient.
The charts fill from the right until the buffer is full, sample only while
the view is shown, and always cover the whole machine — they draw numbers
only, never per-process data. The detail pane adds the same history for the
selected process: a colored CPU % area chart and a resident-memory chart under
the CPU / MEM fields.

## Key bindings

| Key | Action |
|-----|--------|
| `j` / `k` / `Ctrl+d` / `Ctrl+u` / `g` / `G` | Move in the process tree (detail follows) |
| `s` | Cycle the sort: CPU → MEM → PID (shown in the pane title) |
| `c` | Toggle the CPU graph: history ⇄ one bar per core |
| `Enter` / `i` / `l` | Focus the detail pane |
| `/` `n` `N` | Search command lines (processes) or address / port / name (ports) |
| `Tab` / `Shift+Tab` | Cycle panes: Processes → Ports → Detail → System |
| `Enter` (ports) | Jump to the process that owns the port |
| `j` / `k` / `Ctrl+d` / `Ctrl+u` (detail) | Scroll |
| `h` / `Esc` (detail) | Back to the process list |
| `r` | Refresh now |

## Constraints

- The view only inspects — **it never sends a signal**. There is no kill, no
  renice, no stop.
- Environment variables are never read or displayed.
- Values that require privileges you lack show as `(no access)`; port
  ownership may be incomplete without them.
- Sampling pauses while another view is shown, so the graphs only cover time
  spent on this view.
