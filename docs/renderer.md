# The native terminal renderer

On macOS Vosh draws the terminal pane on a native GPU surface, so text
reaches the screen as fast as it does in a native terminal. The rest of
the window stays in the Tauri webview. xterm.js inside WKWebView has a
latency and smoothness ceiling no setting tunes past, and the surface
removes it for the one pane where it matters.

Windows and Linux draw the terminal with xterm.js. The surface code for
them never ran on real hardware, so it left before 1.0. The platform seam
stays, so a port can come later.
docs/history/native-renderer-milestones.md keeps the build diary.

## Why it stays small

The terminal pane only shows output. You type into the input row below
it, which is HTML. So the surface never owns keyboard input, IME or a
PTY. It has two jobs.

1. Draw the grid of the game's output, with its text, colors and styles.
2. Answer the pointer. The wheel scrolls the history, a drag selects, a
   release copies, and Cmd+click opens a link.

## How it sits in the window

```
NSWindow
├─ WKWebView                     chrome, panels, input, status line, overlays
└─ NSView + CAMetalLayer (wgpu)  the grid, under the webview
```

- The surface sits under the webview. It spans the whole window and
  never moves. The grid draws at the pane's offset, and the page leaves
  the pane unpainted so the grid shows through.
- Menus, dialogs and the find bar are plain DOM, so they draw over live
  terminal pixels with no swap between renderers.
- The page takes every pointer event. Over the pane it forwards each one
  to the surface, which tells the page which cursor to show.
- wgpu drives a Metal surface. The glyphs rasterize through CoreGraphics
  with font smoothing off, so they match the webview's text.

## The parts

The renderer lives in `src-tauri/src/native/`, in three folders.
docs/architecture.md maps each file and gives the lock order.

1. `grid`. The cell grid the game's output builds, one for each session.
   It wraps the `Term` of `alacritty_terminal`, with its VT parser, cells,
   scrollback, selection and resize. Around it sit the prompt regions
   Vosh may redraw (`regions`), find (`find`), web links (`links`) and
   the SGR 5 blink alacritty drops (`blink`). The grid builds on every
   platform for the tests, which read it, and ships only on macOS.
2. `gpu`. What turns the grid that shows into pixels. `CellRenderer` draws
   each frame with two shaders, `cell.wgsl` for the cells and `band.wgsl`
   for the band under a lifted prompt. `frame` lays out the quads of a
   frame without the GPU, `style` colors each cell, `atlas` holds the
   glyph atlas and its fonts, `decor` the underlines and the strike, and
   `bands` the prompt bands.
3. `surface`. The view under the webview and the frame loop. `macos.rs`
   creates the view, places the grid in the pane, writes the clipboard
   and opens links. `device` holds the GPU device and loads fonts off the
   main thread. `pointer` takes the pointer input the page forwards,
   `split_drag` maps a selection drag across the scrollback split, and
   `report` tells the page and the game what a frame changed.

## How output reaches it

The game's output never crosses into JavaScript to reach the surface.
`emit_counted` in `src-tauri/src/output.rs` hands every write to the
session's grid and then to the page, in the same order for both. The
session asks for a frame only while its grid shows. Triggers, highlights
and gags need nothing of their own, since the trigger engine bakes them
into the bytes both renderers take.

The page keeps an xterm for each session too, hidden while the surface
draws. It stays at the grid's size, so both hold the same lines.

## What the surface draws

- Every cell style xterm draws. That means true bold from the bundled
  Bold face, sheared italic that overhangs its cell, underline in five
  shapes, strikethrough, dim, inverse and background color.
- 16 color, 256 color and true color text, with the theme palette and
  the terminal tint. Bright bold draws bright ANSI colors in the bold
  weight when you turn it on.
- Blinking text (SGR 5), shown and hidden in 600 ms halves on the clock
  xterm and the pinned band share. The hidden half keeps the ground and
  drops the glyph, the underline and the strike. SGR 6 draws steady, as
  in xterm. With Blinking text off, or reduce motion on, it draws steady.
- The cell spacing xterm reports, so both renderers line up to the pixel.
- The wheel and the keyboard scroll the history. Scrolled back, the pane
  splits into history above a draggable divider and the live tail below.
- A scrollbar thumb you can drag, and the scroll depth the page shows.
- Drag to select, copy on release and on Cmd+C, and Select all.
- Find, with every match marked and the current one stepped to.
- Your sent text, echoed in its own color.
- The prompt Vosh redraws, the band under a lifted prompt and the
  repeated lines it collapses, matching what the page draws on xterm.
- Web links, underlined on hover and opened with Cmd+click.
- Scrollback from your last session, written in before live output.

There is no block cursor, since the pane takes no typing. Wide characters
take two columns, and find counts columns the same way. A search for two
wide characters side by side needs a space between them to match.

## Talking to the page

The page calls the surface through `src/ipc/nativeSurface.ts`, and Rust
answers in `src-tauri/src/ipc/native_surface.rs`. On Windows and Linux
those commands do nothing, so the page calls them on every platform.

- Geometry. `native_surface_set_bounds` sends the pane's rectangle, the
  device pixel ratio and the rows the pinned prompt band borrows.
  `native_surface_set_cell_metrics` sends xterm's device cell, and
  `native_surface_set_font` the font list and size.
- Look. `native_surface_set_theme`, `native_surface_set_tokens`,
  `native_surface_set_divider_color`, `native_surface_set_bright_bold`,
  `native_surface_set_blink_text`, `native_surface_set_prompt_bands` and
  `native_surface_set_prompt_reach`.
- Input. `native_surface_pointer`, `native_surface_wheel`,
  `native_surface_scroll`, `native_surface_copy` and
  `native_surface_select_all`.
- Find. `native_surface_find` and `native_surface_find_clear`.
- Start up. `native_surface_ready` says whether the surface installed and
  its GPU came up.
- `terminal_local_write` writes text the page makes itself, such as your
  echo, to the grid. The page writes the same text to xterm.

The surface answers on five events. `vosh://native-grid-size` carries the
grid's columns and rows, which size the hidden xterm and the window size
the game hears. `vosh://native-scroll` carries the scroll offset,
`vosh://native-copied` the count of characters copied,
`vosh://terminal-clicked` the end of a click and `vosh://terminal-cursor`
the pointer's cursor.

## Turning it off

`nativeSurfaceEnabled` in `src/terminal/terminalRenderer.ts` decides. On
macOS the surface is on unless the `vosh.nativesurface` browser storage
key reads `0`. If the surface fails to come up, the page sets
`vosh.nativesurface.failed` for the rest of the run and draws with xterm.
Windows and Linux never read either key.
