# Files View

![files demo](../../../assets/demo-files.gif)

A read-only file browser rooted at the repository, laid out like
[yazi](https://github.com/sxyazi/yazi): the left column shows the parent
directory, the middle the current one, and the right a preview of the selected
entry — syntax-highlighted text, or a listing for directories. `.git` internals
are hidden and symlinks are marked. Entries get Nerd Font icons by file type;
if your terminal font is not a [Nerd Font](https://www.nerdfonts.com/), put
`icons "none"` in your config.

## Image previews

Images (PNG / JPEG / GIF / WebP) are previewed in the pane, with their format,
dimensions, size and the renderer in use on the first line. In terminals with
a graphics protocol (Kitty, WezTerm, Ghostty, iTerm2, or Sixel-capable ones
such as foot) the image is drawn at full resolution; elsewhere it falls back
to unicode half-blocks. `image-preview "halfblocks"` in the config skips the
terminal detection and `"none"` shows only the metadata. Images over 20 MB
are not decoded.

## Markdown previews

Markdown files (`.md` / `.markdown`, by extension) are rendered in the
preview: headings, emphasis, lists, task lists, code and GFM tables, with
tables fitted to the pane width and reflowed on resize. A YAML front matter
block at the top is kept verbatim in a dim style. `m` toggles between the
rendered form and the raw highlighted text (the pane title shows `markdown` /
`raw`); the [`markdown-preview`](config-reference.md#markdown-preview) config
node picks the default.

## Opening files outside vig

Beyond previewing, the Files view can hand a file to another program — this is
the only view with an "open with" concept:

- `e` opens the selected file in your external editor (`$EDITOR`).
- `o` opens the selected file or directory with the OS default application
  (`open` / `xdg-open` / `explorer`).
- `O` prompts for an application name and opens the entry with it
  (`open -a <app>` on macOS).

## Key bindings

| Key | Action |
|-----|--------|
| `j` / `k` | Move selection (preview follows) |
| `l` / `→` / `Enter` | Enter directory / focus preview |
| `h` / `←` / `Backspace` | Parent directory |
| `i` | Focus preview |
| `j` / `k` / `Ctrl+d` / `Ctrl+u` (preview) | Scroll |
| `h` / `Esc` (preview) | Back to file list |
| `/` `n` `N` | Search file names |
| `e` | Open selected file in external editor |
| `o` | Open selected file or directory with the OS default app (`open` / `xdg-open` / `explorer`) |
| `O` | Open selected entry with an app you name (`open -a <app>` on macOS) |
| `m` | Toggle Markdown rendering in the preview |
| `r` | Re-read the current directory |

## Constraints

- The browser is rooted at the repository — it does not wander above the
  repository root.
- Strictly read-only: no create, rename, delete, copy, or move. Handing a file
  to `$EDITOR` or the OS opener is as far as it goes.
