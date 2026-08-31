# Docker View

![docker demo](../../../assets/demo-docker.gif)

A read-only view of the local Docker daemon, built on the `docker` CLI's JSON
output (`docker ps`, `docker images`, `docker inspect`, `docker logs`). If
`docker` is not installed or the daemon is not running, the view shows a
notice instead of the panes.

Containers are grouped under their compose project (running ones first), the
detail pane shows an inspect summary for the selected container or image, and
the logs pane tails the selected container (`--tail 200`, then `--since`
appends every second while following). The lists refresh every 5 seconds.

## Key bindings

| Key | Action |
|-----|--------|
| `j` / `k` | Move selection (detail and logs follow) |
| `i` / `Enter` | Focus the detail pane |
| `l` (containers) | Focus the logs pane |
| `Tab` / `Shift+Tab` | Cycle panes: Containers → Images → Detail → Logs |
| `j` / `k` / `Ctrl+d` / `Ctrl+u` (detail, logs) | Scroll (scrolling the logs pauses following) |
| `G` (logs) | Jump to the end and resume following |
| `/` `n` `N` | Search container / image names, or log lines |
| `h` / `Esc` (detail, logs) | Back to the list |
| `r` | Re-fetch containers, images, detail and logs |

## Constraints

- Requires the `docker` CLI and a running daemon; otherwise the view shows a
  notice (press `r` after starting the daemon).
- Environment variables are **never displayed** — the inspect summary omits
  them deliberately.
- Nothing in this view starts, stops, restarts, or removes containers or
  images. It only runs inspecting `docker` subcommands.
