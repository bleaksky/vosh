# Native terminal renderer (Tier 3)

Goal: render the terminal pane on a native GPU surface so byte-to-pixel
matches a native terminal (Ghostty-class), while the rest of the app
stays in the Tauri webview. xterm.js inside WKWebView has a hard latency
and smoothness ceiling we cannot tune past; this removes it for the one
pane that matters.

## The simplification that makes this tractable

In Vosh the terminal pane is **output only**. You type into a separate
HTML input row, not into xterm. So the native surface never has to own
keyboard input or drive a PTY. It only needs to:

1. Render the server-output grid (text, colors, styles, cursor).
2. Handle mouse: wheel scroll through scrollback, drag to select, copy.

That cuts out the hardest parts of a native terminal (keymaps, IME, PTY
write path). Keystrokes stay where they are.

## Architecture: native overlay subview + webview chrome

The webview keeps rendering everything except the terminal grid: top bar,
input row, panels, map, settings. A **native child surface** is layered
into the same window, positioned over the terminal region, and draws the
grid with wgpu.

```
NSWindow / HWND / GtkWindow
├─ WKWebView (transparent)        chrome, panels, input, status — unchanged
└─ native child view (wgpu)       the terminal grid, on top, clipped to the pane
```

- **On top, not behind.** A child view positioned over the terminal
  region gets its own mouse events directly, which avoids the painful
  "transparent hole + input passthrough" routing. The frontend reports
  the pane's bounds (and dpr) over IPC; the backend keeps the surface
  aligned on every resize / layout change.
- **wgpu** is the renderer: Metal on macOS, D3D12 on Windows, Vulkan on
  Linux — one renderer, all three CI targets. No per-platform graphics
  code beyond creating the surface.

## Components

1. **Grid model + VTE parser.** Use `alacritty_terminal` (its `Term` +
   grid + scrollback + battle-tested VTE parser). Vosh's telnet layer
   already strips IAC negotiation; feed the resulting data stream to the
   `Term`, which maintains the grid. We keep `vosh-ansi` only for the
   trigger/highlight pipeline, which operates on lines, not the screen.

2. **wgpu cell renderer.** Instanced quads: one pass for cell
   backgrounds, one for glyphs from a rasterized atlas (`swash` or
   `fontdue` for rasterization, dynamic atlas). Cursor and selection are
   extra quads. This is the Alacritty/WezTerm renderer shape, not a
   general text layout engine.

3. **Native surface plumbing.** Create a child view on the Tauri window
   via `raw-window-handle` + a little `objc2`/`windows`/`gtk` glue, hand
   it to wgpu as a surface. Position + size + clip driven by pane bounds
   from the frontend.

4. **Mouse.** Wheel → scroll the grid viewport. Drag → cell selection →
   clipboard. Hover/URL detection later.

5. **Bridge.** Backend already owns the byte stream. It feeds the `Term`,
   then signals "frame dirty"; the renderer redraws. No new IPC for
   output — it never crosses into JS anymore, which is the whole point.

## Open decisions to confirm before coding

- **D1. Overlay-on-top vs transparent-hole.** Proposing on-top child
  view (simpler input). Risk: clipping/rounded-corner/z-order edge cases
  against the webview, and the panel that can overlap the terminal
  (none currently do, but the map/chat live beside it).
- **D2. Grid source.** Proposing `alacritty_terminal`. Alternative: a
  custom grid fed by `vosh-ansi`. Alacritty's is mature but pulls a dep
  and its own parser; we would run two ANSI parsers (its for the screen,
  ours for triggers). Acceptable, but worth a conscious yes.
- **D3. Split-scrollback.** The frozen-history overlay has to be redone
  natively (two viewports of one grid, or a second surface). Defer to a
  later milestone; native scroll-with-follow ships first.
- **D4. Fallback.** Keep xterm.js behind a flag during the transition so
  any platform where the native surface misbehaves still has a working
  terminal. Ship native as opt-in, then flip the default once it is
  proven on all three OSes.

## Milestones

- **M1 — surface proof of concept. DONE (macOS).** M1a: a child `NSView`
  on the window's contentView composites over the `WKWebView`. M1b: a
  wgpu Metal surface on a `CAMetalLayer`-backed view runs a real shader
  pipeline (teal triangle on dark blue) into it. The hardest integration
  — a native GPU surface compositing into the Tauri window — works.
  Still fixed-position; alignment/clipping to the real pane bounds and
  resize tracking come with M2. Code in `src-tauri/src/native_surface.rs`.
- **M2 — real pane + grid + static render.** Three steps:
  - **M2a — align to the pane. DONE (needs visual check).** The frontend
    (`Terminal.tsx` `sync`) reports the live pane's bounds and dpr through
    the `native_surface_set_bounds` command; the backend (`set_bounds`)
    flips the y to AppKit's bottom-left origin, moves and sizes the
    `NSView`, reconfigures the wgpu surface, and redraws. The surface
    installs hidden and is opaque, so it is gated behind the
    `vosh.nativesurface` localStorage flag (default off, xterm untouched).
    VISUAL CHECK: in DevTools run
    `localStorage.setItem('vosh.nativesurface','1')` and reload. The
    dark-blue rectangle with the teal triangle should cover exactly the
    terminal pane and track it on window resize. If it is offset or the
    wrong size, the y-flip or the dpr scaling in `set_bounds` is the
    suspect. Set the flag back to `0` (or remove it) to restore xterm.
  - **M2b — the grid. DONE (unit-tested).** `term_grid.rs` wraps
    `alacritty_terminal`'s `Term` + VTE parser; `session.rs` feeds it the
    same `display_batch` ANSI bytes it hands xterm, so a real cell grid
    (chars, colors, styles, scrollback) builds alongside the live session.
    Verified by unit tests (plain text, CRLF, SGR color, wrap, shared
    feed) rather than by eye. Two dependency-skew fixes were needed:
    `rustix` gets its `std` feature forced on (alacritty's unused unix tty
    module otherwise fails to compile), and `vosh-ansi` drops vte's
    default `no_std` feature (which had unified on across the workspace and
    broke `alacritty_terminal`'s `vte::ansi::Processor`); vte is pinned to
    0.13.0. The grid's read API (cell accessors) is marked allow-dead until
    M2c consumes it.
  - **M2c — the cell renderer. DONE in code (needs visual check).** Built
    in `cell_render.rs` over four commits: (1) `color_to_rgba` maps
    alacritty named/256/truecolor to rgba; (2) `GlyphAtlas` rasterizes the
    system monospace font (font-kit + fontdue) into an A8 slot texture;
    (3a) `build_instances` turns grid cells into per-cell quad instances;
    (3b) `CellRenderer` uploads the atlas as an R8Unorm texture and runs an
    instanced pipeline whose WGSL composites fg over bg by glyph coverage,
    wired into `native_surface::render` via `term_grid::with_grid`, the M1
    triangle gone; (3c) the session loop calls `request_redraw` on new
    output so text updates live (coalesced, dispatched to the main thread).
    Color, atlas packing/rasterization, and instance layout are all
    unit-tested. NOT yet verified: WGSL compiles only at runtime in
    `CellRenderer::new`; glyph placement (baseline math), color fidelity,
    and orientation (the y-flip) need eyes. No styles/scroll yet.
- **M3 — scroll + selection.** Wheel scrollback, drag-select, copy.
- **M4 — parity.** Themes, fonts, cursor styles, link clicks, the
  per-line trigger/highlight effects, resize/NAWS, opaque vs transparent.
- **M5 — split-scrollback, native.** Reimplement the frozen-history view.
- **M6 — cross-platform.** Windows (D3D12) and Linux (Vulkan) surfaces,
  CI bundles, flip the default.

M1 is the go/no-go gate: if a native surface cannot composite cleanly
into the Tauri window across the three platforms, the hybrid approach
is dead and we stay in Tier 1/2. Everything after M1 is "normal" work.

## Resume here

Branch `native-renderer`. **M1 and M2 are done and VERIFIED on screen.**
The native wgpu surface renders the full live terminal (banner, login,
char list, room text) with accurate ANSI colors, composited into the
Tauri window beside the webview chrome. The M2c verification pass
(`910aff2`) fixed sRGB color (linearize the palette for the sRGB surface)
and sized the grid to the pane (`resize_grid` / `grid_size_for`). Enable
with `localStorage.setItem('vosh.nativesurface','1')` + reload; `'0'`
falls back to xterm. Default is off (xterm) until the renderer reaches
parity.

Known gaps before native can be the default:

- **NAWS width.** Text wraps at the xterm column count, so it fills the
  left of the pane, not the full width. The MUD needs the native column
  count (send NAWS sized to `grid_size_for`) so lines wrap to fill. This
  is the most visible next fix.
- **Font.** Uses the best system monospace, not Vosh's configured font.
  Wire the renderer to the same font + size as xterm (M4 parity).
- **Dynamic atlas.** Only printable ASCII is pre-uploaded; non-ASCII
  renders blank until the atlas re-uploads on new glyphs.
- **Styles.** Bold/italic/underline/inverse from the cell flags (M4).

**Then M3** — scroll (wheel through `term_grid` scrollback) + selection
(drag to select cells, copy). After that M4 styles/themes/fonts, M5 native
split-scrollback, M6 cross-platform. Check `git log --oneline` first.
