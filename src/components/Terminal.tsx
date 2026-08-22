import { useEffect, useRef } from 'react';
import { Terminal as XTerm, type ITheme } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import { WebLinksAddon } from '@xterm/addon-web-links';
import { Unicode11Addon } from '@xterm/addon-unicode11';
import {
  SearchAddon,
  type ISearchOptions,
  type ISearchResultChangeEvent,
} from '@xterm/addon-search';
import { WebglAddon } from '@xterm/addon-webgl';

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

import '@xterm/xterm/css/xterm.css';
import { baseAnsiRecord, subscribeBaseAnsi } from '../lib/baseAnsi';
import { loadScrollback, onOutput, setWindowSize } from '../lib/session';
import { findTheme, type AppTheme } from '../lib/themes';
import { getCurrentThemeId, subscribeThemeChanges } from '../lib/theme';
import { WordWrapper } from '../lib/wordWrap';
import { ingestRecentNames } from '../lib/recentNames';
import { hexToRgba } from '../lib/mapPalette';

// Tier 3: the native wgpu terminal surface. Default ON on macOS, where it
// reached visual parity with xterm (docs/native-renderer.md, M4) and had
// its hardware pass. The Windows and Linux surfaces ship compile-verified
// but untested, so they default OFF there — setting vosh.nativesurface to
// '1' explicitly forces the surface on for testing on any platform, and
// '0' falls back to xterm anywhere.
export function nativeSurfaceEnabled(): boolean {
  if (typeof localStorage === 'undefined') return false;
  const flag = localStorage.getItem('vosh.nativesurface');
  if (flag === '1') return true;
  if (flag === '0') return false;
  return (
    typeof navigator !== 'undefined' &&
    (navigator.platform.startsWith('Mac') || navigator.userAgent.includes('Mac OS'))
  );
}

// Report the active theme's surface colors and resolved ANSI palette to
// the native renderer so its background/foreground/selection and the
// 16-color palette match xterm (including the themeTerminalColors tint),
// live-updating on theme or toggle change.
function reportNativeTheme(themeId: string, themeTerminalColors: boolean): void {
  if (!nativeSurfaceEnabled()) return;
  const resolved = xtermThemeFor(findTheme(themeId), themeTerminalColors);
  const ansi = [
    resolved.black,
    resolved.red,
    resolved.green,
    resolved.yellow,
    resolved.blue,
    resolved.magenta,
    resolved.cyan,
    resolved.white,
    resolved.brightBlack,
    resolved.brightRed,
    resolved.brightGreen,
    resolved.brightYellow,
    resolved.brightBlue,
    resolved.brightMagenta,
    resolved.brightCyan,
    resolved.brightWhite,
  ].map((c) => c ?? '#000000');
  void invoke('native_surface_set_theme', {
    background: resolved.background ?? '#101218',
    foreground: resolved.foreground ?? '#cccccc',
    // The raw theme selection hex (resolved.selectionBackground is an
    // rgba() string after the translucency pass, which the backend hex
    // parser cannot read).
    selection: findTheme(themeId).xterm.selectionBackground ?? '#2a3b5e',
    ansi,
  }).catch(() => {});
}

export interface FindOptions {
  /** Treat the term as a regex. Default false (plain substring). */
  regex?: boolean;
  /** Whole-word match. Default false. */
  wholeWord?: boolean;
  /** Case-sensitive search. Default false. */
  caseSensitive?: boolean;
}

export interface TerminalHandle {
  write: (data: Uint8Array | string) => void;
  fit: () => void;
  focus: () => void;
  clear: () => void;
  /** Scroll the xterm scrollback by N pages. Negative N scrolls up. */
  scrollPages: (n: number) => void;
  /** Scroll the xterm scrollback by N lines. Negative N scrolls up. */
  scrollLines: (n: number) => void;
  /** Jump the viewport to the live tail. */
  scrollToBottom: () => void;
  /** True when the viewport is anchored at the live tail (no scrollback offset). */
  isAtBottom: () => boolean;
  /** Current cols × rows. Used by the host to push the size to the
   *  backend via NAWS after a (re)connect. */
  getSize: () => { cols: number; rows: number };
  /** Force the renderer to redraw the visible rows. The split-scrollback
   *  history pane can mount sized and positioned on real content yet the
   *  DOM renderer leaves it blank until the next scroll triggers a draw;
   *  calling this after it settles paints it immediately. */
  refresh: () => void;
  /** TEMPORARY diagnostic snapshot of the xterm internals, for tracing
   *  the intermittent blank split-scrollback pane. */
  debug: () => {
    rows: number;
    cols: number;
    viewportY: number;
    baseY: number;
    bufferLength: number;
    hostW: number;
    hostH: number;
    webgl: boolean;
    instances: number;
  };
  /** Height of one terminal cell in CSS pixels, derived from the
   *  host's pixel height divided by the current row count. Used
   *  by the split-scrollback Resizable to snap the divider to
   *  row boundaries so it never lands mid-row and clips a half
   *  line of content. */
  cellHeight: () => number;
  /** Search forward from the current selection (or top of buffer). Returns
   *  true when a match was found and scrolled into view. Highlights every
   *  match across the entire scrollback as a side effect. */
  findNext: (term: string, options?: FindOptions) => boolean;
  /** Search backward. Same return + decoration semantics as findNext. */
  findPrevious: (term: string, options?: FindOptions) => boolean;
  /** Clear search decorations (called when the find toolbar closes). */
  clearSearch: () => void;
  /** Drop any current selection. Used to enforce "one selection across
   *  panes" when the split is open — when one pane gets a selection
   *  the other pane's selection is cleared. */
  clearSelection: () => void;
  /** True when this pane currently has a non-empty selection. */
  hasSelection: () => boolean;
  /** Subscribe to selection-change events on this terminal. Returns
   *  an unsubscribe function. */
  onSelectionChange: (cb: () => void) => () => void;
  /** Current selected text, or empty string when no selection. */
  getSelection: () => string;
}

interface Props {
  onReady?: (handle: TerminalHandle) => void;
  fontFamily: string;
  fontSize: number;
  /// When true the chrome theme tints server output too. When false
  /// (the default) server output uses the canonical xterm-256
  /// palette regardless of theme.
  themeTerminalColors: boolean;
  /// Suppress the dim "[scrollback restored]" banner. Useful for
  /// secondary terminals (e.g. the split-scrollback history pane)
  /// that load scrollback on every open and would otherwise show
  /// the banner repeatedly.
  quiet?: boolean;
  /// Fires once after the initial scrollback has been written into the
  /// terminal (or when the backend reports none). Use this to apply an
  /// initial viewport position; doing the same work inside onReady is
  /// too early — the terminal has no content at that point and any
  /// scrollPages call is a no-op that the next write would override.
  onScrollbackLoaded?: () => void;
  /// Fires on every viewport change. `back` is the number of lines
  /// above the live tail the viewport is currently showing (0 when
  /// anchored to the tail). `max` is the total scrollback above (the
  /// largest possible `back`). Use to drive a scroll-depth indicator.
  onScrollPosition?: (back: number, max: number) => void;
  /// Fires whenever SearchAddon's match list changes (after every
  /// findNext / findPrevious call). The event carries the index of
  /// the active match plus the total result count. Use to drive a
  /// "3 / 12" badge in a find toolbar.
  onResultsChanged?: (event: ISearchResultChangeEvent) => void;
}

// Canonical xterm-256 palette for ANSI codes 0-15. Used when the
// terminal renders in "independent palette" mode (the default) so
// the server's 16-color and 256-color output looks the same
// regardless of which chrome theme the user picked. The 6x6x6
// cube (codes 16-231) and 24-step grayscale ramp (232-255) are
// already theme-independent inside xterm.js; this fixes the 0-15
// slice that the theme used to tint.
function xtermThemeFor(theme: AppTheme, themeTerminalColors: boolean): ITheme {
  // Tinted mode lets the chrome theme color server output; otherwise
  // the BASE palette applies — the canonical xterm-256 chart unless
  // the user replaced slots in the themes tab (lib/baseAnsi). Either
  // way the chrome theme owns the surfaces (background, foreground,
  // cursor, selection).
  const base: ITheme = themeTerminalColors
    ? { ...theme.xterm }
    : { ...theme.xterm, ...baseAnsiRecord() };
  // Make the selection translucent. Some themes ship a solid (and light)
  // selectionBackground; the search addon selects every active match, so
  // a washed-out selection means an unreadable search hit. Blending over
  // the dark terminal surface keeps the cell dark enough that the line's
  // own text stays legible, while still marking the selection.
  if (theme.xterm.selectionBackground) {
    base.selectionBackground = hexToRgba(theme.xterm.selectionBackground, 0.4);
  }
  return base;
}

// TEMPORARY: count live xterm instances to catch a mount/dispose leak
// across split-scrollback open/close cycles (the history pane mounts and
// unmounts each time the split toggles).
let liveTerminalInstances = 0;

export function Terminal({
  onReady,
  fontFamily,
  fontSize,
  themeTerminalColors,
  quiet = false,
  onScrollbackLoaded,
  onScrollPosition,
  onResultsChanged,
}: Props) {
  const quietRef = useRef(quiet);
  quietRef.current = quiet;
  const onScrollbackLoadedRef = useRef(onScrollbackLoaded);
  onScrollbackLoadedRef.current = onScrollbackLoaded;
  const onScrollPositionRef = useRef(onScrollPosition);
  onScrollPositionRef.current = onScrollPosition;
  const onResultsChangedRef = useRef(onResultsChanged);
  onResultsChangedRef.current = onResultsChanged;
  const containerRef = useRef<HTMLDivElement | null>(null);
  const termRef = useRef<XTerm | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const sizingRef = useRef<HTMLDivElement | null>(null);
  // Mirror the flag in a ref so the long-lived effect (which creates
  // the XTerm instance once) doesn't re-create the terminal every
  // time the user toggles the setting.
  const themeTerminalColorsRef = useRef(themeTerminalColors);
  themeTerminalColorsRef.current = themeTerminalColors;
  // Hold the latest onReady in a ref so the setup effect can call it without
  // listing it as a dependency. Without this, every parent re-render passes
  // a fresh arrow function, the effect re-runs, and the xterm instance is
  // disposed and recreated, wiping all output.
  const onReadyRef = useRef(onReady);
  onReadyRef.current = onReady;

  useEffect(() => {
    if (!containerRef.current) return;

    liveTerminalInstances += 1;
    const term = new XTerm({
      // The user types into the bottom input box, not into xterm, so a
      // cursor block in the output pane is just noise (and a confusing
      // leftover after disconnect). Disable blink and pick the bar style
      // before we send the DECTCEM hide below to keep behavior even if
      // some path turns the cursor back on.
      cursorBlink: false,
      cursorStyle: 'bar',
      cursorInactiveStyle: 'none',
      fontFamily,
      fontSize,
      lineHeight: 1.2,
      scrollback: 10000,
      allowProposedApi: true,
      convertEol: false,
      // High-precision touchpads emit many small deltaY events per
      // gesture; xterm's default scrollSensitivity (1) compounds
      // these into a runaway scroll on macOS. 0.5 halves the per-
      // event step so trackpad scrolling feels controllable. Smooth-
      // scroll animation was tried (smoothScrollDuration) but stacking
      // consecutive scrollLines animations broke the scrolling feel —
      // discrete instant steps via the App.tsx accumulator works
      // better in practice.
      scrollSensitivity: 0.75,
      theme: xtermThemeFor(findTheme(getCurrentThemeId()), themeTerminalColorsRef.current),
    });

    const fit = new FitAddon();
    term.loadAddon(fit);
    term.loadAddon(new WebLinksAddon());
    const unicode11 = new Unicode11Addon();
    term.loadAddon(unicode11);
    term.unicode.activeVersion = '11';
    // Scrollback search. Highlights every match across the full
    // 10k-line buffer via decorations; the host's find toolbar drives
    // it through the findNext / findPrevious handle methods.
    const searchAddon = new SearchAddon();
    term.loadAddon(searchAddon);
    const resultsSub = searchAddon.onDidChangeResults((event) => {
      onResultsChangedRef.current?.(event);
    });

    term.open(containerRef.current);

    // GPU renderer. xterm's WebGL addon must run after term.open() and
    // it reads the host element's pixel size when it allocates the
    // glyph atlas — load it after one fit so the host has nonzero
    // dimensions. The earlier rAF-deferred attempt crashed in
    // syncScrollArea because the renderer swap and a scheduled fit()
    // ran in the same frame, leaving `_renderer.value` undefined; the
    // straightforward order (sync + fit, then swap) avoids that.
    // Escape hatch: set `window.__voshDisableWebGL = true` in the
    // console before reload to force the DOM renderer.
    const safeFit = () => {
      // When the native surface owns the pane it is the size authority and
      // resizes xterm via the native-grid-size event. xterm is hidden behind
      // the opaque surface and mismeasures itself there, so do not let the
      // FitAddon fight the native grid (it would wrap the MUD too narrow).
      if (!quietRef.current && nativeSurfaceEnabled()) return;
      try {
        fit.fit();
      } catch {
        // ignore resize before layout settles
      }
    };
    safeFit();
    // WebGL is on by default: the GPU renderer is far smoother for
    // scroll and burst output than xterm's DOM renderer. The webgl2
    // probe below still falls back to DOM when the WebView can't
    // allocate a context. Opt out persistently with
    // `localStorage.setItem('vosh.webgl', '0')` (or the Settings
    // toggle), or for the current page only with
    // `window.__voshDisableWebGL = true` before reload — the escape
    // hatch for the rare case where a context allocates but paints
    // nothing, which against Tauri's `transparent: true` window reads
    // as see-through desktop.
    let webglAddon: WebglAddon | null = null;
    const lsVal = typeof localStorage !== 'undefined' ? localStorage.getItem('vosh.webgl') : null;
    const winEnable =
      typeof window !== 'undefined' &&
      (window as { __voshEnableWebGL?: boolean }).__voshEnableWebGL === true;
    const winDisable =
      typeof window !== 'undefined' &&
      (window as { __voshDisableWebGL?: boolean }).__voshDisableWebGL === true;
    // On by default (opt-out). Only the live pane uses WebGL; the history
    // pane (quiet) always stays on the DOM renderer so the
    // split-scrollback overlay paints reliably. `__voshDisableWebGL`
    // forces it off; `__voshEnableWebGL` forces it on even when
    // localStorage says '0'.
    const enableWebgl = !winDisable && !quietRef.current && (winEnable || lsVal !== '0');
    if (enableWebgl) {
      // Probe webgl2 in a throwaway canvas first. If the WebView
      // can't allocate a context, the addon would load, immediately
      // fire onContextLoss, and (per xterm 5.5.0 bug) leave the
      // terminal renderer in an unrenderable state. Cheaper to
      // detect now and stay on DOM.
      let probeOk = false;
      try {
        const probe = document.createElement('canvas');
        const ctx = probe.getContext('webgl2') as WebGL2RenderingContext | null;
        if (ctx) {
          probeOk = true;
          try {
            ctx.getExtension('WEBGL_lose_context')?.loseContext();
          } catch {
            // ignore probe cleanup failure
          }
        }
      } catch {
        probeOk = false;
      }
      if (!probeOk) {
        console.log('[vosh] webgl2 unavailable in this WebView, staying on DOM');
      }
      try {
        if (!probeOk) throw new Error('webgl2 probe failed');
        // xterm 6.1.0-beta's WebglAddon takes an options object and
        // also ships PR #5529 — synchronous redraw on resize — which
        // is what fixes the 1px-canvas-width oscillation that made
        // the divider drag wobble on the previous (5.5.0) line.
        // No options needed; the defaults match what we want.
        const addon = new WebglAddon();
        // On context loss (GPU reset, sleep/wake, too many live GL
        // contexts) dispose the addon so xterm hands rendering back to
        // its DOM renderer. Without this the pane keeps a dead GL
        // canvas, which against Tauri's transparent window reads as a
        // blank see-through hole until the user reloads — worse now
        // that WebGL is the default. Disposing is xterm's documented
        // context-loss fallback. The crash that blocked this on the
        // 5.5.0 line was a fit() racing a half-disposed renderer
        // (unguarded `_renderer.value.dimensions`); we sidestep that
        // regardless of version by deferring the repaint to the next
        // frame, so nothing touches the renderer in the same tick as
        // the swap inside dispose().
        addon.onContextLoss(() => {
          console.warn('[vosh] webgl context lost — falling back to DOM renderer');
          try {
            addon.dispose();
          } catch (err) {
            console.warn('[vosh] webgl dispose after context loss failed', err);
          }
          webglAddon = null;
          // Force the DOM renderer to paint the visible rows once the
          // swap settles. The buffer is untouched by the renderer
          // change, so this just makes the content reappear immediately
          // instead of on the next server write.
          requestAnimationFrame(() => {
            try {
              term.refresh(0, term.rows - 1);
            } catch {
              // ignore — the DOM renderer repaints on the next write
            }
          });
        });
        term.loadAddon(addon);
        webglAddon = addon;
        console.log('[vosh] webgl renderer active');
      } catch (err) {
        console.log('[vosh] webgl renderer failed, staying on DOM', err);
        webglAddon = null;
      }
    } else {
      console.log('[vosh] webgl off — re-enable with localStorage.removeItem("vosh.webgl")');
    }
    requestAnimationFrame(safeFit);
    setTimeout(safeFit, 50);
    setTimeout(safeFit, 200);
    setTimeout(safeFit, 800);
    termRef.current = term;
    fitRef.current = fit;

    // The actual fix. Read the sizing wrapper's bounding rect every
    // frame and explicitly write width/height in pixels onto the
    // terminal-host element. xterm-addon-fit reads the host's
    // computed `height` style (not its clientHeight); without an
    // explicit pixel height, computed height comes back wrong in some
    // Tauri/WebKit layout passes — opening DevTools forces a layout
    // and the value goes right, but otherwise it stays stale.
    let lastW = 0;
    let lastH = 0;
    const sizer = sizingRef.current;
    const host = containerRef.current;

    // Tier 3 (docs/native-renderer.md): report this pane's screen
    // rectangle to the native wgpu surface so it tracks the terminal.
    // Live pane only, and only when opted in via the localStorage flag,
    // because the surface is opaque and would otherwise occlude xterm.
    // Set vosh.nativesurface to "1" and reload to see it.
    const nativeSurfaceOn = !quietRef.current && nativeSurfaceEnabled();
    let lastNativeBounds = '';
    const reportNativeBounds = () => {
      if (!nativeSurfaceOn || !sizer) return;
      const r = sizer.getBoundingClientRect();
      const dpr = window.devicePixelRatio || 1;
      const key = `${Math.round(r.left)},${Math.round(r.top)},${Math.round(
        r.width,
      )},${Math.round(r.height)},${dpr}`;
      if (key === lastNativeBounds) return;
      lastNativeBounds = key;
      void invoke('native_surface_set_bounds', {
        x: r.left,
        y: r.top,
        width: r.width,
        height: r.height,
        dpr,
      }).catch(() => {});
    };

    // Report xterm's exact device cell size so the surface grid matches the
    // webview's spacing instead of deriving it from font metrics. The cell
    // size is stable across pane resizes; it changes on font/dpr changes.
    let lastCellMetrics = '';
    const reportCellMetrics = () => {
      if (!nativeSurfaceOn) return;
      const cell = term.dimensions?.device?.cell;
      if (!cell?.width || !cell?.height) return;
      const width = Math.round(cell.width);
      const height = Math.round(cell.height);
      const key = `${width},${height}`;
      if (key === lastCellMetrics) return;
      lastCellMetrics = key;
      void invoke('native_surface_set_cell_metrics', { width, height }).catch(() => {});
    };

    const sync = () => {
      if (!sizer || !host) return;
      reportNativeBounds();
      reportCellMetrics();
      const rect = sizer.getBoundingClientRect();
      const w = Math.floor(rect.width);
      const h = Math.floor(rect.height);
      if (w === lastW && h === lastH) return;
      lastW = w;
      lastH = h;
      host.style.width = `${w}px`;
      host.style.height = `${h}px`;
      safeFit();
      // Fit may have just established or changed the cell dimensions.
      reportCellMetrics();
    };

    // Resizable broadcasts `vosh:resize-progress` { size } from
    // its pointermove handler — fires synchronously inside the
    // same JS task that just set the wrapper's CSS height. We
    // run sync + fit + anchor restore in that same task so
    // wrapper, xterm, and scroll all update before the browser
    // paints. Anything async (React state, ResizeObserver) would
    // land in a separate paint and the user would see a brief
    // mismatched intermediate frame — the "jitter" that every
    // previous attempt produced. For quiet panes (split-scrollback
    // history) the viewport top is saved before fit and restored
    // after so the larger viewport exposes new rows BELOW the old
    // bottom instead of pushing old content down. That's what
    // makes the drag look like a continuous curtain: the new
    // rows xterm exposes match the rows that were just at the
    // top of the live pane (both buffers are in sync because
    // they both consume the same session://output stream).
    const onResizeProgress = (_event: Event) => {
      if (!sizer || !host) return;
      const rect = sizer.getBoundingClientRect();
      const w = Math.floor(rect.width);
      const h = Math.floor(rect.height);
      if (w === lastW && h === lastH) return;
      lastW = w;
      lastH = h;
      host.style.width = `${w}px`;
      host.style.height = `${h}px`;
      safeFit();
      // No explicit refresh or viewport restore. xterm's resize
      // adjusts the buffer dimensions; the WebGL renderer's own
      // debounced redraw paints once after the drag settles, which
      // matches what the DOM renderer does — both panes look
      // stationary during the drag and the divider slides cleanly
      // between them. Forcing a per-frame refresh produced the
      // curtain effect (content shifted per drag frame); doing
      // scrollLines or scrollToLine produced oscillation. Leaving
      // it alone gives the right visual.
    };
    window.addEventListener('vosh:resize-progress', onResizeProgress);

    const handleWindowResize = sync;
    window.addEventListener('resize', handleWindowResize);
    const observer = new ResizeObserver(sync);
    if (sizer) observer.observe(sizer);
    observer.observe(document.body);

    // Keep a resize sync alive after mount as a backup for the rare
    // Tauri/WebKit case where ResizeObserver misses a one-shot chrome
    // change. The split-scrollback history pane (quiet) needs a
    // per-frame sync: it mounts transiently when the split opens and
    // must be fully fit by the time onScrollbackLoaded positions its
    // viewport, otherwise that first scroll lands on blank rows and only
    // a second scroll re-renders it. The live pane uses a low-frequency
    // interval instead, because a per-frame getBoundingClientRect there
    // stacked a layout reflow onto every combat-round write and stole
    // frames from the renderer.
    let rafPoll = 0;
    let intervalPoll: ReturnType<typeof setInterval> | undefined;
    if (quietRef.current) {
      const pollLoop = () => {
        sync();
        rafPoll = requestAnimationFrame(pollLoop);
      };
      rafPoll = requestAnimationFrame(pollLoop);
    } else {
      intervalPoll = setInterval(sync, 250);
    }

    let unsubOutput: (() => void) | undefined;
    // The native surface is the size authority while it owns the pane. It
    // emits its grid size; size hidden xterm to match so a dropdown swap
    // reveals identical content. Live pane only.
    let unsubGridSize: (() => void) | undefined;
    if (!quietRef.current && nativeSurfaceEnabled()) {
      void listen<[number, number]>('vosh://native-grid-size', (event) => {
        const [cols, rows] = event.payload;
        if (cols > 0 && rows > 0 && (cols !== term.cols || rows !== term.rows)) {
          try {
            term.resize(cols, rows);
          } catch {
            // resize before the renderer is ready; the next emit retries
          }
        }
      }).then((un) => {
        unsubGridSize = un;
      });
    }
    // Replay persisted scrollback before any live output lands so the
    // user opens the app to the tail of their last session.
    const notifyPosition = () => {
      const buf = term.buffer.active;
      onScrollPositionRef.current?.(buf.baseY - buf.viewportY, buf.baseY);
    };
    term.onScroll(notifyPosition);

    // Pre-pad the buffer with blank lines so the first content write
    // appears at the bottom of the viewport instead of the top.
    // Without this, a fresh session shows MUD output anchored to the
    // top with empty rows below — which reads as a giant gap between
    // the latest prompt and the room strip / vitals chip at the
    // bottom of the window. Padding pushes the cursor to the last
    // viewport row; subsequent writes then scroll content up from
    // the bottom like every other MUD client.
    //
    // Runs after EVERY initial-mount path — empty scrollback, short
    // scrollback that did not fill the viewport, or a scrollback
    // restore failure — because the bug surfaces whenever the
    // post-restore cursor position lands above the viewport bottom.
    // No-op when the cursor is already at the bottom (large
    // scrollback that overfilled).
    const padToBottom = () => {
      const cursorY = term.buffer.active.cursorY;
      const rowsBelow = term.rows - 1 - cursorY;
      if (rowsBelow > 0) {
        term.write('\r\n'.repeat(rowsBelow));
      }
    };

    // The live pane seeds the native grid with the persisted scrollback so
    // it has the same history as xterm; the quiet history pane must not.
    loadScrollback(!quietRef.current && nativeSurfaceEnabled())
      .then((bytes) => {
        if (bytes.length > 0) {
          term.write(bytes);
          if (!quietRef.current) {
            // Explicit 256-palette gray, not dim: xterm and the native
            // renderer dim differently, so dim would show two shades.
            const banner = '\r\n\x1b[38;5;244m[scrollback restored]\x1b[0m\r\n';
            term.write(banner);
            // Mirror the banner into the native grid so xterm and the surface
            // have the same line count; otherwise a dropdown swap to xterm
            // shifts the content up by these rows.
            if (nativeSurfaceEnabled()) {
              void invoke('native_surface_echo', { text: banner }).catch(() => {});
            }
          }
        }
        // xterm.write batches into an internal queue; flush before
        // notifying so any onScrollbackLoaded handler that adjusts
        // the viewport sees the final row count, not the count
        // before the buffered bytes were rendered. The flush also
        // matters for padToBottom: cursorY only reflects the
        // post-restore position after the queued bytes are drained.
        const notify = () => {
          if (!quietRef.current) padToBottom();
          notifyPosition();
          onScrollbackLoadedRef.current?.();
        };
        if (bytes.length > 0) {
          term.write('', notify);
        } else {
          notify();
        }
      })
      .catch(() => {
        // No scrollback yet, or backend not ready; still notify so
        // the host can apply its initial scroll gesture (no-op on
        // an empty terminal, but does not lose the user intent).
        if (!quietRef.current) padToBottom();
        notifyPosition();
        onScrollbackLoadedRef.current?.();
      });
    // Client-side word wrap. NAWS handles most lines server-side, but
    // some content paths (tells, comm channels) ignore it on certain
    // ROM derivatives. We line-buffer here so a complete line word-
    // wraps cleanly before hitting xterm.
    //
    // We flush the trailing partial (prompt) at the end of every chunk
    // instead of waiting on an idle timer. The old 20ms idle flush
    // made GMCP-driven UI (room strip, vitals, map) visibly land
    // before the text — you would see the new room's info pop up a
    // frame before walking into it. With backend per-read batching
    // (v0.2.10) each TCP read arrives as one output event, so the
    // line-buffer's wrap math still has the whole line for any line
    // ending in \n; the only thing that flushes "early" is the
    // already-complete prompt at the chunk tail.
    const wrapper = new WordWrapper(term.cols);
    const decoder = new TextDecoder('utf-8', { fatal: false });
    term.onResize(({ cols }) => {
      wrapper.setCols(cols);
      // Same tail-anchor rationale as in onOutput below: a resize
      // shifts baseY without moving viewportY, which can land the
      // live pane above its tail. Snap on resize so the freeze
      // resolves even when no data is currently arriving (the user
      // dragged the divider, then waited — no write would have
      // otherwise unstuck the viewport until the next server line).
      if (!quietRef.current) {
        // When the viewport grows taller, the bottom-anchor pad
        // from mount no longer reaches the new last row, leaving
        // a gap below the cursor. Re-pad so the cursor lands on
        // the new bottom row before any subsequent content writes.
        padToBottom();
        term.scrollToBottom();
      }
    });
    onOutput((bytes) => {
      const text = decoder.decode(bytes, { stream: true });
      const wrapped = wrapper.process(text);
      const tail = wrapper.flush();
      if (wrapped.length > 0 || tail.length > 0) {
        term.write(wrapped + tail);
        // Live pane is a strict tail of server output. Without this
        // snap, dragging the split-scrollback divider can leave the
        // live pane's viewport above its baseY — xterm preserves
        // absolute viewportY across the resize, but baseY shifts as
        // rows are added/removed, so the viewport ends up "stuck"
        // showing the line you were on when the drag started. xterm's
        // default auto-scroll-on-write only kicks in if viewport ==
        // baseY pre-write, so subsequent server lines pile up below
        // the visible region instead of advancing the tail. Forcing a
        // snap on every write makes the live pane behave like a true
        // live tail. The history pane (quiet=true) opts out so users
        // can read past output in the split.
        if (!quietRef.current) {
          term.scrollToBottom();
        }
      }
      // Feed the decoded text into the recent-names cache so Tab
      // completion can complete people the user has seen in
      // who-lists, comm-channel chatter, considers, etc. — not just
      // names they have typed before or chars currently in the room.
      // Cost is one regex pass per output chunk; sub-millisecond.
      if (!quietRef.current) ingestRecentNames(text);
    }).then((unlisten) => {
      unsubOutput = unlisten;
    });

    // Push the live terminal size to the backend so the telnet
    // negotiator can advertise it via NAWS. The MUD wraps server-side
    // at the advertised column count, which is what well-behaved
    // word wrap looks like — no client preprocessing, no latency.
    // Debounce so a rapid resize animation only fires one IPC at the
    // settled size. Only the primary (non-quiet) terminal pushes,
    // since the history pane in the split view shares the same width.
    let naws_timer: ReturnType<typeof setTimeout> | null = null;
    const pushSize = () => {
      if (quietRef.current) return;
      // When the native surface owns the terminal it advertises its own
      // (larger) grid size as NAWS; xterm must not fight it with the
      // webview-font column count.
      if (nativeSurfaceEnabled()) return;
      void setWindowSize(term.cols, term.rows).catch(() => {
        // Not connected, or session torn down. Either is fine; the
        // initial NAWS handshake on the next connect will send the
        // current size anyway.
      });
    };
    const scheduleSizePush = () => {
      if (naws_timer) clearTimeout(naws_timer);
      naws_timer = setTimeout(pushSize, 120);
    };
    term.onResize(scheduleSizePush);
    // First push after the deferred fits settle so the backend gets
    // the real size rather than the 80x24 default xterm starts with.
    setTimeout(pushSize, 900);

    // Match-highlight colors. Read from CSS vars at search time so a
    // theme switch picks up the new accent on the next find call.
    // Hard-coded fallbacks keep matches visible if the var lookup
    // returns empty (early-mount race in WKWebView).
    const searchDecorations = (): NonNullable<ISearchOptions['decorations']> => {
      const rootStyle = getComputedStyle(document.documentElement);
      const accent = rootStyle.getPropertyValue('--c-accent').trim() || '#7aa2f7';
      const toRgba = (hex: string, alpha: number): string => {
        const m = /^#?([0-9a-fA-F]{6})$/.exec(hex);
        if (!m) return hex;
        const n = parseInt(m[1], 16);
        return `rgba(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}, ${alpha})`;
      };
      // The SearchAddon draws non-active matches BELOW the text and the
      // active match ABOVE it. So a non-active match can carry an accent
      // tint (the glyphs paint on top and stay legible), but the active
      // match must have NO top fill — any fill there sits over the
      // glyphs and washes them out, which is the unreadable highlight
      // bug. The active match is marked instead by a solid accent
      // outline (and its own tint shows through from the below-text
      // highlight layer the addon also draws for it).
      return {
        matchBackground: toRgba(accent, 0.28),
        matchBorder: toRgba(accent, 0.5),
        matchOverviewRuler: accent,
        activeMatchBackground: 'transparent',
        activeMatchBorder: accent,
        activeMatchColorOverviewRuler: accent,
      };
    };

    const handle: TerminalHandle = {
      write: (data) => term.write(data),
      fit: () => fit.fit(),
      focus: () => term.focus(),
      clear: () => term.clear(),
      scrollPages: (n) => term.scrollPages(n),
      scrollLines: (n) => term.scrollLines(n),
      scrollToBottom: () => term.scrollToBottom(),
      refresh: () => {
        if (term.rows > 0) term.refresh(0, term.rows - 1);
      },
      debug: () => ({
        rows: term.rows,
        cols: term.cols,
        viewportY: term.buffer.active.viewportY,
        baseY: term.buffer.active.baseY,
        bufferLength: term.buffer.active.length,
        hostW: host && host.style.width ? parseFloat(host.style.width) : 0,
        hostH: host && host.style.height ? parseFloat(host.style.height) : 0,
        webgl: webglAddon !== null,
        instances: liveTerminalInstances,
      }),
      // viewportY tracks the top of the viewport in scrollback coords;
      // baseY tracks the top of the bottom page. Equal means the
      // viewport is anchored to the live tail.
      isAtBottom: () => term.buffer.active.viewportY === term.buffer.active.baseY,
      getSize: () => ({ cols: term.cols, rows: term.rows }),
      cellHeight: () => {
        // Read the host's pixel height (set by sync), divide by
        // xterm's current row count, and round UP to whole pixels.
        // xterm does not expose actual cell height in its public
        // API; this derivation matches FitAddon's own row math.
        // Ceiling matters: the snap pitch is the wrapper-height
        // delta per row. With fractional cellHeight the wrapper
        // height between two snap points is `N * cellHeight`, but
        // the DOM rounds it to whole pixels — drag near a snap
        // boundary then oscillates between two pixel rows of
        // remainder space at the bottom of xterm and the line near
        // the divider looks like its height is changing. Ceiling
        // guarantees `N * snap > N * actualCellHeight`, so xterm
        // always fits N rows comfortably with a constant tiny
        // remainder, and that remainder doesn't move as snaps
        // change. A few unused pixels at the bottom is cheaper
        // than a visible jitter.
        const h = host && host.style.height ? parseFloat(host.style.height) : 0;
        const rows = term.rows;
        return rows > 0 ? Math.ceil(h / rows) : 0;
      },
      findNext: (query, opts) =>
        searchAddon.findNext(query, {
          regex: opts?.regex ?? false,
          wholeWord: opts?.wholeWord ?? false,
          caseSensitive: opts?.caseSensitive ?? false,
          decorations: searchDecorations(),
        }),
      findPrevious: (query, opts) =>
        searchAddon.findPrevious(query, {
          regex: opts?.regex ?? false,
          wholeWord: opts?.wholeWord ?? false,
          caseSensitive: opts?.caseSensitive ?? false,
          decorations: searchDecorations(),
        }),
      clearSearch: () => searchAddon.clearDecorations(),
      clearSelection: () => term.clearSelection(),
      hasSelection: () => term.hasSelection(),
      getSelection: () => term.getSelection(),
      onSelectionChange: (cb) => {
        const disposable = term.onSelectionChange(cb);
        return () => disposable.dispose();
      },
    };
    onReadyRef.current?.(handle);

    // Auto-clear selection when it scrolls off the viewport.
    // xterm's canvas renderer paints the selection overlay at the
    // selection's current row in the viewport, but the underlying
    // selection POSITION stays anchored to its original buffer rows
    // even when new data scrolls the buffer up — the paint then
    // happens at a stale screen position ("ghost"). Subscribing to
    // onScroll lets us notice when the selection range has fallen
    // outside [viewportY, viewportY + rows] and clear it before the
    // ghost can render.
    const onScrollClearStaleSelection = () => {
      const sel = term.getSelectionPosition();
      if (!sel) return;
      const top = term.buffer.active.viewportY;
      const bottom = top + term.rows - 1;
      const offTop = sel.end.y < top;
      const offBottom = sel.start.y > bottom;
      if (offTop || offBottom) {
        term.clearSelection();
      }
    };
    const scrollDisposable = term.onScroll(onScrollClearStaleSelection);

    // Ctrl/Cmd + C or X copies the xterm selection. The keystroke
    // almost always lands while focus is in the Input box (the user
    // drag-selects xterm output, then hits the shortcut without
    // clicking back into the terminal), so a keydown listener
    // attached to xterm alone never fires. Listen at the window
    // instead, and defer to the focused element's native copy/cut
    // when it actually has its own selection.
    const onCopyKey = (event: KeyboardEvent) => {
      const key = event.key.toLowerCase();
      if (key !== 'c' && key !== 'x') return;
      // Accept any combination of Ctrl or Cmd (without Alt), with or
      // without Shift. Plain Ctrl+C is the convention most MUD clients
      // use; the older Ctrl+Shift+C variant still works.
      const primary = event.ctrlKey || event.metaKey;
      if (!primary || event.altKey) return;
      const selection = term.getSelection();
      if (!selection) return;
      const active = document.activeElement as HTMLInputElement | HTMLTextAreaElement | null;
      const activeHasSelection =
        active && 'selectionStart' in active && active.selectionStart !== active.selectionEnd;
      const domSelection = window.getSelection();
      const domHasSelection = domSelection !== null && domSelection.toString().length > 0;
      if (activeHasSelection || domHasSelection) return;
      void navigator.clipboard.writeText(selection).catch(() => {
        /* clipboard may be unavailable in some webviews */
      });
      event.preventDefault();
      event.stopPropagation();
    };
    window.addEventListener('keydown', onCopyKey, true);

    return () => {
      if (rafPoll) cancelAnimationFrame(rafPoll);
      if (intervalPoll) clearInterval(intervalPoll);
      observer.disconnect();
      window.removeEventListener('resize', handleWindowResize);
      window.removeEventListener('vosh:resize-progress', onResizeProgress);
      window.removeEventListener('keydown', onCopyKey, true);
      if (naws_timer) clearTimeout(naws_timer);
      unsubOutput?.();
      unsubGridSize?.();
      resultsSub.dispose();
      scrollDisposable.dispose();
      searchAddon.dispose();
      // WebglAddon's dispose reads `_terminal._core._store._isDisposed`
      // and throws when xterm has already torn down its core. The
      // history pane mounts/unmounts on split open/close so this fires
      // routinely. The renderer still releases its GL resources before
      // the throw, so the silent swallow is safe — there's nothing
      // useful for us to do here and printing pollutes the console
      // every time the split toggles.
      try {
        webglAddon?.dispose();
      } catch {
        // intentional swallow — see comment above
      }
      webglAddon = null;
      term.dispose();
      termRef.current = null;
      fitRef.current = null;
      liveTerminalInstances -= 1;
    };
    // Setup runs exactly once. Font is read from props on initial mount;
    // later font changes re-apply via the effect below without disposing
    // the xterm instance.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Apply font changes without rebuilding the terminal so scrollback and
  // listeners survive. xterm reflows on the next fit() call.
  useEffect(() => {
    const term = termRef.current;
    const fit = fitRef.current;
    if (!term || !fit) return;
    term.options.fontFamily = fontFamily;
    term.options.fontSize = fontSize;
    try {
      fit.fit();
    } catch {
      // ignore
    }
    if (nativeSurfaceEnabled()) {
      const primary = fontFamily
        .split(',')[0]
        .trim()
        .replace(/^["']|["']$/g, '');
      void invoke('native_surface_set_font', {
        family: primary,
        size: Math.round(fontSize),
      }).catch(() => {});
    }
  }, [fontFamily, fontSize]);

  // Re-apply the palette when the canonical-vs-themed toggle flips
  // without needing to recreate the XTerm instance.
  useEffect(() => {
    const term = termRef.current;
    if (!term) return;
    term.options.theme = xtermThemeFor(findTheme(getCurrentThemeId()), themeTerminalColors);
    reportNativeTheme(getCurrentThemeId(), themeTerminalColors);
  }, [themeTerminalColors]);

  // Re-apply when the user edits the base ANSI palette (the colors
  // used while the tint toggle is off).
  useEffect(() => {
    return subscribeBaseAnsi(() => {
      const term = termRef.current;
      if (!term) return;
      term.options.theme = xtermThemeFor(
        findTheme(getCurrentThemeId()),
        themeTerminalColorsRef.current,
      );
      reportNativeTheme(getCurrentThemeId(), themeTerminalColorsRef.current);
    });
  }, []);

  // Live-refresh the xterm palette when the user switches themes from
  // the settings window. Listens on the cross-window theme event.
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    subscribeThemeChanges((themeId) => {
      const term = termRef.current;
      if (!term) return;
      term.options.theme = xtermThemeFor(findTheme(themeId), themeTerminalColorsRef.current);
      reportNativeTheme(themeId, themeTerminalColorsRef.current);
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  return (
    <div ref={sizingRef} className="terminal-sizer">
      <div
        ref={containerRef}
        className="terminal-host"
        role="log"
        aria-live="polite"
        aria-label="MUD output"
      />
    </div>
  );
}
