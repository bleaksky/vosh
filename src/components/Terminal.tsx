import { useCallback, useEffect, useLayoutEffect, useRef } from 'react';
import { Terminal as XTerm } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import { WebLinksAddon } from '@xterm/addon-web-links';
import { Unicode11Addon } from '@xterm/addon-unicode11';
import {
  SearchAddon,
  type ISearchOptions,
  type ISearchResultChangeEvent,
} from '@xterm/addon-search';
import { WebglAddon } from '@xterm/addon-webgl';

import '@xterm/xterm/css/xterm.css';
import { subscribeBaseAnsi } from '../lib/baseAnsi';
import {
  nativeSurfacePointer,
  nativeSurfaceSetBounds,
  nativeSurfaceSetCellMetrics,
  nativeSurfaceSetFont,
  nativeSurfaceSetTheme,
  nativeSurfaceWheel,
  onNativeGridSize,
} from '../ipc/nativeSurface';
import { setWindowSize } from '../ipc/session';
import {
  loadScrollback,
  onOutput,
  terminalCursor,
  terminalScreenRows,
  terminalLocalWrite,
} from '../ipc/terminal';
import { useTauriEvent } from '../ipc/useTauriEvent';
import { findTheme, onCustomThemesChanged } from '../lib/themes';
import { getFitGameColors, subscribeFitGameColors } from '../lib/fitGameColors';
import { setHighlightGround } from '../lib/highlightGround';
import { nativeThemeOf, xtermThemeFor } from '../lib/terminalTheme';
import { getCurrentThemeId, subscribeThemeChanges } from '../lib/theme';
import { OutputShaper } from '../lib/outputShaper';
import { RegionWriter } from '../lib/terminalRegion';
import { remeasureWhenLoaded } from '../lib/terminalFont';
import {
  cellInGrid,
  regionFromCursor,
  regionFromXterm,
  type RegionOnScreen,
  type ScreenCell,
} from '../lib/promptPointer';
import { BandLayer, LiftTracker, markLifted } from '../lib/promptBands';
import {
  GameSizeReport,
  gameSize,
  keepTail,
  keptRows,
  nativeBottomBounds,
  spareAbove,
} from '../lib/terminalRows';
import { noteReader } from '../lib/readerBusy';
import { ingestRecentNames } from '../lib/recentNames';
import { underlayShows, XtermMirror } from '../lib/xtermMirror';
import { XtermBlink } from '../lib/xtermBlink';
import type { BufferView, LineMark } from '../lib/splitDrag';

/** Session flag set when the native surface never came up, so the page
 *  falls back to xterm instead of leaving a transparent hole. */
export const NATIVE_FAILED_KEY = 'vosh.nativesurface.failed';

// Tier 3: the native wgpu terminal surface. Default ON on macOS, where it
// reached visual parity with xterm (docs/native-renderer.md, M4) and had
// its hardware pass. There vosh.nativesurface '0' falls back to xterm.
// Windows and Linux always draw with xterm and never read the flag. Vosh
// keeps no native surface for them, and in native mode the page waits for
// the surface to size the grid, so a forced flag there would leave the
// terminal without a size.
//
// The surface sits BELOW the webview (the underlay). The page leaves the
// terminal pane unpainted so the grid shows through, DOM overlays draw
// over it with no renderer swap, and pointer input over the pane is
// forwarded to the surface.
export function nativeSurfaceEnabled(): boolean {
  if (typeof localStorage === 'undefined') return false;
  // The surface failed to come up earlier in this session. Stay on xterm
  // until the next launch.
  try {
    if (sessionStorage.getItem(NATIVE_FAILED_KEY) === '1') return false;
  } catch {
    // storage unavailable; fall through to the platform
  }
  const mac =
    typeof navigator !== 'undefined' &&
    (navigator.platform.startsWith('Mac') || navigator.userAgent.includes('Mac OS'));
  if (!mac) return false;
  // '1' and no flag both leave the macOS default on.
  return localStorage.getItem('vosh.nativesurface') !== '0';
}

// Report the active theme's terminal background to the session, which
// keeps trigger colors readable on it, on either renderer. Then report the
// surface colors and resolved ANSI palette to the native renderer so its
// background/foreground/selection and the 16-color palette match xterm
// (including the themeTerminalColors tint and Fit game colors),
// live-updating on theme or toggle change.
function reportTheme(themeId: string, themeTerminalColors: boolean): void {
  const theme = findTheme(themeId);
  setHighlightGround(theme.xterm.background);
  if (!nativeSurfaceEnabled()) return;
  const native = nativeThemeOf(theme, themeTerminalColors, getFitGameColors());
  void nativeSurfaceSetTheme({
    background: native.background,
    foreground: native.foreground,
    selection: native.selection,
    ansi: native.ansi,
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
  /** The newest output of the prompt stage this terminal took, 0 before
   *  the first. Text written after it follows it, which the session reads
   *  to tell which prompt the text closes. */
  outputTaken: () => number;
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
  /** Current cols × rows the pane shows. */
  getSize: () => { cols: number; rows: number };
  /** The size the game is told through NAWS: the rows the pane shows
   *  plus the rows it lends to the pinned prompt band. The host pushes it
   *  after a (re)connect. */
  windowSize: () => { cols: number; rows: number };
  /** Force the renderer to redraw the visible rows. The split-scrollback
   *  history pane can mount sized and positioned on real content yet the
   *  DOM renderer leaves it blank until the next scroll triggers a draw;
   *  calling this after it settles paints it immediately. */
  refresh: () => void;
  /** A snapshot of the xterm internals. The history split reads
   *  `bufferLength` and `rows` from it to know when its pane holds real
   *  content. */
  debug: () => {
    rows: number;
    cols: number;
    viewportY: number;
    baseY: number;
    bufferLength: number;
    hostW: number;
    hostH: number;
    webgl: boolean;
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
  /** Select `length` cells from `column` of buffer row `row`, rows
   *  counted whole. A drag across the scrollback split selects through
   *  it (src/lib/splitDrag.ts). */
  select: (column: number, row: number, length: number) => void;
  /** The buffer as a drag reads it: the size, where the viewport and the
   *  bottom page start, and the cursor's row on the screen. */
  bufferView: () => BufferView;
  /** Keep track of buffer row `row` through new output and trimmed
   *  history, or null on the alternate screen. */
  markLine: (row: number) => LineMark | null;
  /** Buffer row `row` as text with its trailing blanks gone, or null past
   *  the buffer. A drag across the split finds its line by it. */
  lineText: (row: number) => string | null;
  /** Select the whole buffer, scrollback included. */
  selectAll: () => void;
  /** Where the open region starts on this pane's screen, as the renderer
   *  that draws it holds it: the native grid through terminal_cursor, or
   *  xterm from the line its mark came on. Null while no region is open.
   *  The prompt card lays your prompt out from it to map a pointer to a
   *  piece. */
  promptRegion: () => Promise<RegionOnScreen | null>;
  /** The screen's rows as text from its top, each with trailing blanks
   *  gone, as the renderer in use holds them, with its width and whether
   *  it shows the live tail. The prompt card finds the game's own line in
   *  them while the profile reads no prompt. Null before it has a size. */
  screenRows: () => Promise<{ rows: string[]; cols: number; atBottom: boolean } | null>;
  /** The screen cell under a point in client px, on the grid the
   *  renderer in use draws, or null outside it. */
  cellAt: (clientX: number, clientY: number) => ScreenCell | null;
  /** The client y of the top of screen row `row`, from the top of the
   *  visible screen, on the grid the renderer in use draws, or null
   *  before it has a size. The prompt card sits over your prompt by it. */
  rowTop: (row: number) => number | null;
  /** The cell grid the renderer in use draws: the top left of its first
   *  cell in client px and the size of a cell, or null before it has a
   *  size. The prompt card puts its marks on your prompt by it. */
  grid: () => { left: number; top: number; cellW: number; cellH: number } | null;
}

interface Props {
  onReady?: (handle: TerminalHandle) => void;
  fontFamily: string;
  fontSize: number;
  /// Row spacing as a multiple of the glyph height (xterm's lineHeight).
  /// The native grid follows through the cell size this pane reports.
  lineHeight: number;
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
  /// Fires when the cell the text sits in changes size, in CSS px, or
  /// the column count changes: the size the renderer in use draws a cell
  /// at, so a band outside the grid can put its characters on the same
  /// columns.
  onCellSize?: (size: { width: number; height: number; cols: number }) => void;
  /// Your prompt shows lifted: xterm draws each prompt over a band. The
  /// live pane only, and only while xterm draws the terminal. The native
  /// grid draws its own bands, and the split history pane draws none.
  lifted?: boolean;
  /// Rows at the bottom of the live pane the pinned prompt band borrows
  /// while your prompt takes more than one row (src/lib/terminalRows.ts).
  /// The pane keeps its size and the grid gives them up from its top, so
  /// the newest line stays right above the band. The game still hears of
  /// the rows the pane holds with them.
  lentRows?: number;
  /// Your prompt shows pinned: the grid keeps to the bottom of the live
  /// pane, so the pixels its whole rows leave over sit above its first row
  /// and the newest line sits the dock's gap over the band. Off, the grid
  /// keeps to the top, as in the text and Lifted.
  anchorBottom?: boolean;
  /// Blinking text is on, so xterm blinks SGR 5 on the shared clock
  /// (src/lib/xtermBlink.ts) while WebGL draws the pane. Off, or on the
  /// DOM renderer, it draws it steady.
  blinkText?: boolean;
}

// The terminal's palette lives in src/lib/terminalTheme.ts, which the
// pinned prompt band reads too.

/** `color`, a #rrggbb ground, fully clear. */
function clearGround(color: string | undefined): string {
  const m = /^#?([0-9a-f]{6})/i.exec(color ?? '');
  return m ? `#${m[1]}00` : 'rgba(0, 0, 0, 0)';
}

/** The theme xterm draws with: the terminal palette, its ground clear
 *  while the pane lifts your prompts (`clear`), since the bands draw under
 *  xterm's text and the terminal area's ground shows through. */
function themeFor(themeId: string, tinted: boolean, clear: boolean) {
  const theme = xtermThemeFor(findTheme(themeId), tinted, getFitGameColors());
  if (clear) theme.background = clearGround(theme.background);
  return theme;
}

export function Terminal({
  onReady,
  fontFamily,
  fontSize,
  lineHeight,
  themeTerminalColors,
  quiet = false,
  onScrollbackLoaded,
  onScrollPosition,
  onResultsChanged,
  onCellSize,
  lifted = false,
  lentRows = 0,
  anchorBottom = false,
  blinkText = false,
}: Props) {
  const quietRef = useRef(quiet);
  quietRef.current = quiet;
  const onScrollbackLoadedRef = useRef(onScrollbackLoaded);
  onScrollbackLoadedRef.current = onScrollbackLoaded;
  const onScrollPositionRef = useRef(onScrollPosition);
  onScrollPositionRef.current = onScrollPosition;
  const onResultsChangedRef = useRef(onResultsChanged);
  onResultsChangedRef.current = onResultsChanged;
  const onCellSizeRef = useRef(onCellSize);
  onCellSizeRef.current = onCellSize;
  const liftedRef = useRef(lifted);
  liftedRef.current = lifted;
  // The rows lent to the pinned band, and whether the grid keeps to the
  // bottom of its pane, which the layout effect below keeps and applies
  // before the page paints.
  const lentRef = useRef(lentRows);
  const anchorRef = useRef(anchorBottom);
  // Applies a new lent count or anchoring: refits and places xterm, or
  // reports the native bounds. Set by the setup effect.
  const relayoutRef = useRef<(() => void) | null>(null);
  // Fits xterm to its pane less the lent rows. Set by the setup effect.
  const fitKeptRef = useRef<(() => void) | null>(null);
  // Fits the pane to a cell measured again and reports it. Set by the
  // setup effect.
  const refitCellRef = useRef<(() => void) | null>(null);
  // The band layer, while this pane can draw bands.
  const bandsRef = useRef<BandLayer | null>(null);
  // Blinking text on xterm, and whether it is on.
  const blinkRef = useRef<XtermBlink | null>(null);
  const blinkTextRef = useRef(blinkText);
  blinkTextRef.current = blinkText;
  const containerRef = useRef<HTMLDivElement | null>(null);
  const termRef = useRef<XTerm | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const sizingRef = useRef<HTMLDivElement | null>(null);
  // Sends the current cell size to the native surface. Set by the setup
  // effect, so a line height change can report without waiting for the
  // next resize poll.
  const reportCellMetricsRef = useRef<(() => void) | null>(null);
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

  // Whether this pane draws bands now: your prompt shows lifted and
  // xterm draws the terminal.
  const liftsHere = useCallback(() => liftedRef.current && bandsRef.current !== null, []);

  // The lift state the pane last applied, so the default never touches
  // xterm's options.
  const appliedLiftRef = useRef(false);

  // Turn the bands on or off, with the clear ground they need.
  const applyLift = (term: XTerm) => {
    const on = liftsHere();
    if (on === appliedLiftRef.current) return;
    appliedLiftRef.current = on;
    term.options.allowTransparency = on;
    term.options.theme = themeFor(getCurrentThemeId(), themeTerminalColorsRef.current, on);
    const area = containerRef.current?.closest('.terminal-area');
    if (area) markLifted(area, on);
    bandsRef.current?.setEnabled(on);
  };

  useEffect(() => {
    if (!containerRef.current) return;

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
      lineHeight,
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
      theme: themeFor(getCurrentThemeId(), themeTerminalColorsRef.current, false),
    });

    // Every write to this xterm goes through one ordered writer, which
    // finds the regions the session marks and replaces them only while
    // nothing was written after them (src/lib/terminalRegion.ts). Every
    // resize goes through it too, so xterm takes a size only once it has
    // parsed what it holds. A copy that fills anew from the scrollback
    // gets a writer of its own (see the mirror below).
    let writer = new RegionWriter(term);
    const localDecoder = new TextDecoder('utf-8', { fatal: false });
    // The newest output of the prompt stage the writer took, in the order
    // the session sent them.
    let outputTaken = 0;
    // Lifted prompts. The tracker registers after the writer, so xterm
    // hands it the lift marks first and the region marks pass on.
    const lifts = !quietRef.current && !nativeSurfaceEnabled() ? new LiftTracker(term) : null;
    // A prompt that leaves the text takes its band with it.
    if (lifts) writer.onErase((row, col) => lifts.dropFrom(row, col));

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
    const blink = new XtermBlink(term);
    blink.setOn(blinkTextRef.current);
    blinkRef.current = blink;
    const area = containerRef.current.closest<HTMLElement>('.terminal-area');
    if (lifts && area) {
      bandsRef.current = new BandLayer(term, lifts, area);
      applyLift(term);
    }

    // Where xterm sits in its pane. While it keeps to the bottom, the host
    // moves down by the pixels its rows leave over, and the sizer clips
    // what the host then reaches past its bottom, which the band covers.
    // xterm maps the pointer from its screen's own box, so selections and
    // links follow. Under the native surface the bounds carry it instead.
    let placedTop = '0px';
    const placeGrid = () => {
      const pane = sizingRef.current;
      const box = containerRef.current;
      if (!pane || !box) return;
      let top = 0;
      const cell = term.dimensions?.css?.cell?.height;
      if (anchorRef.current && !quietRef.current && !nativeSurfaceEnabled() && cell) {
        const dpr = window.devicePixelRatio || 1;
        const height = pane.getBoundingClientRect().height;
        top = spareAbove(height, term.rows + lentRef.current, cell, dpr);
      }
      const next = `${top}px`;
      if (next === placedTop) return;
      placedTop = next;
      box.style.top = next;
    };

    // Fit xterm to its pane, less the rows the pinned band borrows. The
    // FitAddon only proposes the size, so the lent rows come off here.
    const fitKept = () => {
      const dims = fit.proposeDimensions();
      if (!dims || Number.isNaN(dims.cols) || Number.isNaN(dims.rows)) return;
      writer.resize(dims.cols, keptRows(dims.rows, lentRef.current));
      placeGrid();
    };
    fitKeptRef.current = fitKept;

    // GPU renderer. xterm's WebGL addon must run after term.open() and
    // it reads the host element's pixel size when it allocates the
    // glyph atlas — load it after one fit so the host has nonzero
    // dimensions. The earlier rAF-deferred attempt crashed in
    // syncScrollArea because the renderer swap and a scheduled fit()
    // ran in the same frame, leaving `_renderer.value` undefined; the
    // straightforward order (sync + fit, then swap) avoids that.
    const safeFit = () => {
      // When the native surface owns the pane it is the size authority and
      // resizes xterm via the native-grid-size event. The FitAddon sizes
      // xterm from its own cells, so letting it fit here would fight the
      // native grid.
      if (!quietRef.current && nativeSurfaceEnabled()) return;
      try {
        fitKept();
      } catch {
        // ignore resize before layout settles
      }
    };
    safeFit();
    // WebGL is on by default: the GPU renderer is far smoother for
    // scroll and burst output than xterm's DOM renderer. The webgl2
    // probe below still falls back to DOM when the WebView can't
    // allocate a context. Opt out with
    // `localStorage.setItem('vosh.webgl', '0')` (or the Settings
    // toggle), for the rare case where a context allocates but paints
    // nothing, which against Tauri's `transparent: true` window reads
    // as see-through desktop.
    let webglAddon: WebglAddon | null = null;
    const lsVal = typeof localStorage !== 'undefined' ? localStorage.getItem('vosh.webgl') : null;
    // On by default (opt-out). Only the live pane uses WebGL; the history
    // pane (quiet) always stays on the DOM renderer so the
    // split-scrollback overlay paints reliably.
    const enableWebgl = !quietRef.current && lsVal !== '0';
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
          blink.setWebgl(false);
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
        blink.setWebgl(true);
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
    // Live pane only, and only while the surface draws it.
    //
    // The rows the pinned band borrows go along, so the grid gives them
    // up in the same frame as the new bounds, and the game keeps its size.
    // While the grid keeps to the bottom of the pane under the underlay,
    // the bounds start lower by the pixels its rows leave over
    // (`nativeSpare`), and pointer positions count from there.
    const nativeSurfaceOn = !quietRef.current && nativeSurfaceEnabled();
    let lastNativeBounds = '';
    let nativeSpare = 0;
    const reportNativeBounds = () => {
      if (!nativeSurfaceOn || !sizer) return;
      const r = sizer.getBoundingClientRect();
      const dpr = window.devicePixelRatio || 1;
      const lent = lentRef.current;
      let { top, height } = r;
      nativeSpare = 0;
      if (anchorRef.current && nativeSurfaceEnabled()) {
        const cellPx = Math.round(term.dimensions?.device?.cell?.height ?? 0);
        const placed = nativeBottomBounds(r.top, r.height, dpr, cellPx);
        ({ top, height } = placed);
        nativeSpare = placed.spare;
      }
      const key = `${Math.round(r.left)},${top},${Math.round(r.width)},${height},${dpr},${lent}`;
      if (key === lastNativeBounds) return;
      lastNativeBounds = key;
      void nativeSurfaceSetBounds({
        x: r.left,
        y: top,
        width: r.width,
        height,
        dpr,
        lent,
      }).catch(() => {});
    };

    // Report xterm's exact device cell size so the surface grid matches the
    // webview's spacing instead of deriving it from font metrics. The cell
    // size is stable across pane resizes; it changes on font, dpr, and line
    // height changes. The line height is in the cell: xterm multiplies its
    // glyph box by it and centers the box in the cell, so the box height
    // rides along and the surface puts its baseline where xterm's is.
    let lastCellMetrics = '';
    const reportCellMetrics = () => {
      if (!nativeSurfaceOn) return;
      const device = term.dimensions?.device;
      const cell = device?.cell;
      if (!device || !cell?.width || !cell?.height) return;
      const width = Math.round(cell.width);
      const height = Math.round(cell.height);
      const charHeight = Math.round(device.char?.height ?? 0);
      const key = `${width},${height},${charHeight}`;
      if (key === lastCellMetrics) return;
      lastCellMetrics = key;
      void nativeSurfaceSetCellMetrics({
        width,
        height,
        charHeight: charHeight > 0 ? charHeight : null,
      }).catch(() => {});
    };
    reportCellMetricsRef.current = reportCellMetrics;

    // The cell size a band outside the grid lays its characters out on.
    // The native grid draws xterm's device cell rounded to whole pixels
    // (reportCellMetrics above), and xterm draws its own.
    let lastCellSize = '';
    const reportCellSize = () => {
      if (!onCellSizeRef.current) return;
      const cell = term.dimensions?.device?.cell;
      if (!cell?.width || !cell?.height) return;
      const dpr = window.devicePixelRatio || 1;
      const native = !quietRef.current && nativeSurfaceEnabled();
      const width = (native ? Math.round(cell.width) : cell.width) / dpr;
      const height = (native ? Math.round(cell.height) : cell.height) / dpr;
      const key = `${width},${height},${term.cols}`;
      if (key === lastCellSize) return;
      lastCellSize = key;
      onCellSizeRef.current({ width, height, cols: term.cols });
    };

    const sync = () => {
      if (!sizer || !host) return;
      reportNativeBounds();
      reportCellMetrics();
      reportCellSize();
      // A pane a fraction of a pixel taller fits the same rows and leaves
      // a different spare.
      placeGrid();
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
      reportCellSize();
    };

    // The pinned band borrows more or fewer rows, or the grid starts or
    // stops keeping to the bottom. The pane keeps its size, so nothing
    // above notices: xterm fits and places itself again, or the native
    // grid hears of it with its bounds.
    relayoutRef.current = () => {
      if (nativeSurfaceOn) reportNativeBounds();
      else safeFit();
      placeGrid();
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

    // Fits the pane to a cell xterm measured again once a face loaded, and
    // reports the cell. Under the native surface the fit waits for the
    // grid, which hears the new cell.
    refitCellRef.current = () => {
      safeFit();
      reportCellMetrics();
      reportCellSize();
    };

    // Underlay input. The webview sits above the surface and receives every
    // pointer event over the pane, so forward them to the native grid in
    // pane-local CSS px. Moves coalesce to one IPC per frame and read the
    // pane rect once per frame. A release flushes the last move first so a
    // drag ends where the pointer did. preventDefault on press keeps focus
    // in the command line and stops the page from starting a text
    // selection. It cannot shield an IME composition, which WebKit hands
    // every mouse event first. Right click and Control click fall through
    // to the terminal menu's contextmenu handler.
    const detachUnderlayInput = (() => {
      if (quietRef.current || !nativeSurfaceEnabled() || !sizer) return undefined;
      const send = (kind: string, x: number, y: number, open: boolean) => {
        void nativeSurfacePointer({ kind, x, y, open }).catch(() => {});
      };
      const toLocal = (clientX: number, clientY: number) => {
        const r = sizer.getBoundingClientRect();
        return { x: clientX - r.left, y: clientY - r.top - nativeSpare };
      };
      let dragging = false;
      let last = { clientX: 0, clientY: 0 };
      let pending = false;
      let raf = 0;
      const flush = () => {
        raf = 0;
        if (!pending) return;
        pending = false;
        const p = toLocal(last.clientX, last.clientY);
        send(dragging ? 'drag' : 'move', p.x, p.y, false);
      };
      // End a drag however it ends: a release, a cancel, lost capture, or a
      // move with the button already up because the release went elsewhere.
      const release = (pointerId?: number) => {
        if (!dragging) return;
        if (raf) cancelAnimationFrame(raf);
        flush();
        dragging = false;
        if (pointerId !== undefined && sizer.hasPointerCapture(pointerId)) {
          sizer.releasePointerCapture(pointerId);
        }
        const p = toLocal(last.clientX, last.clientY);
        send('up', p.x, p.y, false);
      };
      const onDown = (e: PointerEvent) => {
        last = { clientX: e.clientX, clientY: e.clientY };
        if (e.button === 1) {
          e.preventDefault();
          const p = toLocal(e.clientX, e.clientY);
          send('middle', p.x, p.y, false);
          return;
        }
        if (e.button === 2) {
          // Keep the caret in the command line. contextmenu still fires.
          e.preventDefault();
          return;
        }
        // Control click is the macOS context click. Leave it to the menu.
        if (e.button !== 0 || e.ctrlKey) return;
        e.preventDefault();
        try {
          sizer.setPointerCapture(e.pointerId);
        } catch {
          // No active pointer to capture (a synthetic event). The drag
          // still works while the pointer stays over the pane.
        }
        dragging = true;
        const p = toLocal(e.clientX, e.clientY);
        send('down', p.x, p.y, e.metaKey);
      };
      const onMove = (e: PointerEvent) => {
        last = { clientX: e.clientX, clientY: e.clientY };
        if (dragging && (e.buttons & 1) === 0) {
          release(e.pointerId);
          return;
        }
        pending = true;
        if (!raf) raf = requestAnimationFrame(flush);
      };
      const onUp = (e: PointerEvent) => {
        last = { clientX: e.clientX, clientY: e.clientY };
        if ((e.buttons & 1) === 0) release(e.pointerId);
      };
      const onCancel = (e: PointerEvent) => release(e.pointerId);
      const onLeave = () => {
        if (dragging) return;
        if (raf) cancelAnimationFrame(raf);
        raf = 0;
        pending = false;
        send('leave', 0, 0, false);
      };
      // WebKit reports wheel deltas with the opposite sign of AppKit's
      // scrollingDeltaY (positive deltaY scrolls toward newer output), and
      // the backend accumulator is tuned for AppKit pixels. WebKit on macOS
      // always sends pixel mode. Line and page modes scale up only in case
      // another engine sends them.
      const onWheel = (e: WheelEvent) => {
        const scale = e.deltaMode === 1 ? 8.5 : e.deltaMode === 2 ? 200 : 1;
        const delta = -e.deltaY * scale;
        if (delta === 0) return;
        void nativeSurfaceWheel(delta).catch(() => {});
      };
      sizer.addEventListener('pointerdown', onDown);
      sizer.addEventListener('pointermove', onMove);
      sizer.addEventListener('pointerup', onUp);
      sizer.addEventListener('pointercancel', onCancel);
      sizer.addEventListener('lostpointercapture', onCancel);
      sizer.addEventListener('pointerleave', onLeave);
      sizer.addEventListener('wheel', onWheel, { passive: true });
      return () => {
        if (raf) cancelAnimationFrame(raf);
        if (dragging) send('up', 0, 0, false);
        send('leave', 0, 0, false);
        sizer.removeEventListener('pointerdown', onDown);
        sizer.removeEventListener('pointermove', onMove);
        sizer.removeEventListener('pointerup', onUp);
        sizer.removeEventListener('pointercancel', onCancel);
        sizer.removeEventListener('lostpointercapture', onCancel);
        sizer.removeEventListener('pointerleave', onLeave);
        sizer.removeEventListener('wheel', onWheel);
      };
    })();

    let unsubOutput: (() => void) | undefined;
    // The native surface is the size authority while it owns the pane. It
    // emits its grid size; size hidden xterm to match so a dropdown swap
    // reveals identical content. Live pane only. A size equal to xterm's
    // still goes to the writer, since a size that waits there would land
    // over it. xterm does nothing for a size it already has.
    let unsubGridSize: (() => void) | undefined;
    if (!quietRef.current && nativeSurfaceEnabled()) {
      void onNativeGridSize(([cols, rows]) => {
        if (cols > 0 && rows > 0) writer.resize(cols, rows);
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
    //
    // While your prompt is the open region it pads nothing, so the region
    // stays open for the repaints the session still sends, and your echo
    // follows your prompt (RegionWriter.pad).
    const padToBottom = () => {
      const cursorY = term.buffer.active.cursorY;
      // Line ends held back for a pinned prompt go out with the padding,
      // so they count toward it.
      const rowsBelow = term.rows - 1 - cursorY - writer.pendingRows();
      if (rowsBelow > 0) {
        writer.pad('\r\n'.repeat(rowsBelow));
      }
    };

    // Decoded across outputs and word wrapped (src/lib/outputShaper.ts).
    // A copy that fills anew starts a shaper of its own.
    let shaper = new OutputShaper(term.cols);

    // While the native underlay draws the live terminal, the xterm copy
    // hides and takes no writes (src/lib/xtermMirror.ts). It keeps its
    // size, which the native grid sets, and the cell size it measures,
    // which the grid and the pinned band draw on. Find, selection, copy,
    // the prompt card and the scroll all go to the native grid then. If
    // the screen comes back to xterm, the copy fills anew from the
    // session's scrollback, as a reload onto xterm fills it.
    const mirror = new XtermMirror({
      owned: () =>
        !quietRef.current && nativeSurfaceEnabled() && underlayShows(document.documentElement),
      rebuild: (done) => {
        writer.dispose();
        term.reset();
        writer = new RegionWriter(term);
        if (lifts) writer.onErase((row, col) => lifts.dropFrom(row, col));
        shaper = new OutputShaper(term.cols);
        const settle = () => {
          padToBottom();
          keepTail(term);
          done();
        };
        loadScrollback(false)
          .then(({ bytes }) => {
            if (bytes.length === 0) return settle();
            writer.local(localDecoder.decode(bytes));
            writer.local('\r\n\x1b[38;5;244m[scrollback restored]\x1b[0m\r\n');
            // The pad reads where the cursor sits once xterm parsed it all.
            writer.whenParsed(settle);
          })
          .catch(settle);
      },
    });
    const underlayWatch = new MutationObserver(() => mirror.check());
    underlayWatch.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['data-underlay'],
    });

    // The live pane seeds the native grid with the persisted scrollback so
    // it has the same history as xterm; the quiet history pane must not.
    loadScrollback(!quietRef.current && nativeSurfaceEnabled())
      .then(({ bytes, seededNative }) => {
        // A copy the native grid hides takes none of it.
        const toXterm = mirror.mirrors();
        if (bytes.length > 0) {
          if (toXterm) writer.local(localDecoder.decode(bytes));
          if (!quietRef.current) {
            // Explicit 256-palette gray, not dim: xterm and the native
            // renderer dim differently, so dim would show two shades.
            const banner = '\r\n\x1b[38;5;244m[scrollback restored]\x1b[0m\r\n';
            if (toXterm) writer.local(banner);
            // Mirror the banner into the native grid so xterm and the surface
            // have the same line count; otherwise a dropdown swap to xterm
            // shifts the content up by these rows. Only when this load
            // seeded the grid: after a reload the grid already holds the
            // history and its first banner, and another would stack.
            if (seededNative) {
              void terminalLocalWrite(banner, null).catch(() => {});
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
          if (!quietRef.current) mirror.write(padToBottom);
          notifyPosition();
          onScrollbackLoadedRef.current?.();
        };
        if (bytes.length > 0 && toXterm) {
          writer.whenParsed(notify);
        } else {
          notify();
        }
      })
      .catch(() => {
        // No scrollback yet, or backend not ready; still notify so
        // the host can apply its initial scroll gesture (no-op on
        // an empty terminal, but does not lose the user intent).
        if (!quietRef.current) mirror.write(padToBottom);
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
    // already-complete prompt at the chunk tail. The shaper is made
    // above, with the mirror.
    term.onResize(({ cols }) => {
      shaper.setCols(cols);
      reportCellSize();
      placeGrid();
      // Same tail-anchor rationale as in onOutput below: a resize
      // shifts baseY without moving viewportY, which can land the
      // live pane above its tail. Snap on resize so the freeze
      // resolves even when no data is currently arriving (the user
      // dragged the divider, then waited — no write would have
      // otherwise unstuck the viewport until the next server line).
      // Only a pane that left its tail snaps: xterm's scrollbar still
      // has the old rows here, and a scroll asked of it now lands a
      // row short, the newest line under the screen
      // (src/lib/terminalRows.ts).
      if (!quietRef.current) {
        // When the viewport grows taller, the bottom-anchor pad
        // from mount no longer reaches the new last row, leaving
        // a gap below the cursor. Re-pad so the cursor lands on
        // the new bottom row before any subsequent content writes.
        mirror.write(() => {
          padToBottom();
          keepTail(term);
        });
      }
    });
    onOutput((out) => {
      if (out.id !== undefined && out.id > outputTaken) outputTaken = out.id;
      // A copy the native grid hides writes nothing. It only decodes
      // the text for the recent names cache below.
      if (!mirror.mirrors()) {
        if (!quietRef.current) {
          const { text, replace } = shaper.text(out);
          ingestRecentNames(text);
          if (replace !== null) ingestRecentNames(replace);
        }
        return;
      }
      const { output, text } = shaper.shape(out);
      if (output) {
        mirror.write(() => writer.output(output));
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
        // can read past output in the split. A pane on its tail asks
        // nothing, since a resize may have left xterm's scrollbar a
        // frame behind (src/lib/terminalRows.ts).
        if (!quietRef.current) {
          mirror.write(() => keepTail(term));
        }
      }
      // Feed the decoded text into the recent-names cache so Tab
      // completion can complete people the user has seen in
      // who-lists, comm-channel chatter, considers, etc. — not just
      // names they have typed before or chars currently in the room.
      // Cost is one regex pass per output chunk; sub-millisecond.
      if (!quietRef.current) {
        ingestRecentNames(text);
        if (output?.replace) ingestRecentNames(output.replace.text);
      }
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
    // A row the pinned band borrows or gives back sends nothing, since
    // the game is told the rows the pane holds with the lent ones
    // (src/lib/terminalRows.ts).
    let naws_timer: ReturnType<typeof setTimeout> | null = null;
    const gameSizes = new GameSizeReport();
    const pushSize = () => {
      if (quietRef.current) return;
      // When the native surface owns the terminal it advertises its own
      // (larger) grid size as NAWS; xterm must not fight it with the
      // webview-font column count.
      if (nativeSurfaceEnabled()) return;
      const size = gameSizes.next(term.cols, term.rows, lentRef.current);
      if (!size) return;
      void setWindowSize(size.cols, size.rows).catch(() => {
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
      write: (data) => {
        const text = typeof data === 'string' ? data : localDecoder.decode(data);
        mirror.write(() => writer.local(text));
      },
      outputTaken: () => outputTaken,
      fit: () => fitKept(),
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
      }),
      // viewportY tracks the top of the viewport in scrollback coords;
      // baseY tracks the top of the bottom page. Equal means the
      // viewport is anchored to the live tail.
      isAtBottom: () => term.buffer.active.viewportY === term.buffer.active.baseY,
      getSize: () => ({ cols: term.cols, rows: term.rows }),
      windowSize: () => gameSize(term.cols, term.rows, lentRef.current),
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
        // The rows lent to the pinned band are still the host's.
        const h = host && host.style.height ? parseFloat(host.style.height) : 0;
        const rows = term.rows + lentRef.current;
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
      select: (column, row, length) => term.select(column, row, length),
      bufferView: () => {
        const buffer = term.buffer.active;
        return {
          cols: term.cols,
          rows: term.rows,
          viewportY: buffer.viewportY,
          baseY: buffer.baseY,
          cursorY: buffer.cursorY,
        };
      },
      markLine: (row) => {
        const buffer = term.buffer.active;
        if (buffer.type !== 'normal') return null;
        return term.registerMarker(Math.max(0, row) - (buffer.baseY + buffer.cursorY)) ?? null;
      },
      lineText: (row) => term.buffer.active.getLine(row)?.translateToString(true) ?? null,
      selectAll: () => term.selectAll(),
      onSelectionChange: (cb) => {
        const disposable = term.onSelectionChange(cb);
        return () => disposable.dispose();
      },
      promptRegion: async () => {
        if (!quietRef.current && nativeSurfaceEnabled()) {
          return regionFromCursor(await terminalCursor().catch(() => null));
        }
        return regionFromXterm(writer.region(), term.buffer.active, term.cols);
      },
      screenRows: async () => {
        if (!quietRef.current && nativeSurfaceEnabled()) {
          const screen = await terminalScreenRows().catch(() => null);
          return screen
            ? { rows: screen.rows, cols: screen.cols, atBottom: screen.at_bottom }
            : null;
        }
        const buffer = term.buffer.active;
        const rows = Array.from(
          { length: term.rows },
          (_, row) => buffer.getLine(buffer.viewportY + row)?.translateToString(true) ?? '',
        );
        return { rows, cols: term.cols, atBottom: buffer.viewportY === buffer.baseY };
      },
      cellAt: (clientX, clientY) => {
        const dpr = window.devicePixelRatio || 1;
        if (!quietRef.current && nativeSurfaceEnabled()) {
          // The native grid draws from the pane's top left, each cell
          // xterm's device cell rounded to whole pixels.
          const device = term.dimensions?.device?.cell;
          if (!sizer || !device?.width || !device?.height) return null;
          const r = sizer.getBoundingClientRect();
          const grid = {
            left: r.left,
            top: r.top + nativeSpare,
            width: r.width,
            height: r.height - nativeSpare,
          };
          return cellInGrid(clientX, clientY, grid, {
            width: Math.round(device.width) / dpr,
            height: Math.round(device.height) / dpr,
          });
        }
        const screen = host?.querySelector('.xterm-screen');
        const cell = term.dimensions?.css?.cell;
        if (!screen || !cell?.width || !cell?.height) return null;
        return cellInGrid(clientX, clientY, screen.getBoundingClientRect(), cell);
      },
      rowTop: (row) => {
        if (!quietRef.current && nativeSurfaceEnabled()) {
          // The native grid, as cellAt reads it.
          const device = term.dimensions?.device?.cell;
          if (!sizer || !device?.height) return null;
          const dpr = window.devicePixelRatio || 1;
          return (
            sizer.getBoundingClientRect().top +
            nativeSpare +
            row * (Math.round(device.height) / dpr)
          );
        }
        const screen = host?.querySelector('.xterm-screen');
        const cell = term.dimensions?.css?.cell;
        if (!screen || !cell?.height) return null;
        return screen.getBoundingClientRect().top + row * cell.height;
      },
      grid: () => {
        if (!quietRef.current && nativeSurfaceEnabled()) {
          // The native grid, as cellAt reads it.
          const device = term.dimensions?.device?.cell;
          if (!sizer || !device?.width || !device?.height) return null;
          const dpr = window.devicePixelRatio || 1;
          const r = sizer.getBoundingClientRect();
          return {
            left: r.left,
            top: r.top + nativeSpare,
            cellW: Math.round(device.width) / dpr,
            cellH: Math.round(device.height) / dpr,
          };
        }
        const screen = host?.querySelector('.xterm-screen');
        const cell = term.dimensions?.css?.cell;
        if (!screen || !cell?.width || !cell?.height) return null;
        const r = screen.getBoundingClientRect();
        return { left: r.left, top: r.top, cellW: cell.width, cellH: cell.height };
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

    // A clock piece in your design leaves your prompt as it is while you
    // select text here, or while the live pane is off its newest rows.
    const selectionPart = quietRef.current ? 'historySelection' : 'liveSelection';
    const readerSelection = term.onSelectionChange(() =>
      noteReader(selectionPart, term.hasSelection()),
    );
    const readerBack = quietRef.current
      ? null
      : term.onScroll(() =>
          noteReader('liveBack', term.buffer.active.viewportY !== term.buffer.active.baseY),
        );

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
      // Return the caret to the command line so the user keeps typing
      // instead of leaving focus stranded on the terminal.
      window.dispatchEvent(new Event('vosh:focus-input'));
    };
    window.addEventListener('keydown', onCopyKey, true);

    return () => {
      if (rafPoll) cancelAnimationFrame(rafPoll);
      if (intervalPoll) clearInterval(intervalPoll);
      observer.disconnect();
      window.removeEventListener('resize', handleWindowResize);
      window.removeEventListener('vosh:resize-progress', onResizeProgress);
      window.removeEventListener('keydown', onCopyKey, true);
      detachUnderlayInput?.();
      if (naws_timer) clearTimeout(naws_timer);
      unsubOutput?.();
      unsubGridSize?.();
      underlayWatch.disconnect();
      writer.dispose();
      bandsRef.current?.dispose();
      bandsRef.current = null;
      blink.dispose();
      blinkRef.current = null;
      lifts?.dispose();
      resultsSub.dispose();
      scrollDisposable.dispose();
      readerSelection.dispose();
      readerBack?.dispose();
      // A pane that goes takes its selection and its place with it.
      noteReader(selectionPart, false);
      if (readerBack) noteReader('liveBack', false);
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
      fitKeptRef.current = null;
      refitCellRef.current = null;
      relayoutRef.current = null;
    };
    // Setup runs exactly once. Font is read from props on initial mount;
    // later font changes re-apply via the effect below without disposing
    // the xterm instance.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // A new count of rows lent to the pinned band applies before the page
  // paints, in the same commit that grows or shrinks the band, so the
  // newest line moves with the band's top and never sits under it. So
  // does the grid starting or stopping keeping to the bottom.
  useLayoutEffect(() => {
    if (lentRef.current === lentRows && anchorRef.current === anchorBottom) return;
    lentRef.current = lentRows;
    anchorRef.current = anchorBottom;
    relayoutRef.current?.();
  }, [lentRows, anchorBottom]);

  // Blinking text turns on or off live.
  useEffect(() => {
    blinkRef.current?.setOn(blinkText);
  }, [blinkText]);

  // Apply font changes without rebuilding the terminal so scrollback and
  // listeners survive. xterm reflows on the next fit() call.
  useEffect(() => {
    const term = termRef.current;
    const fit = fitRef.current;
    if (!term || !fit) return;
    term.options.fontFamily = fontFamily;
    term.options.fontSize = fontSize;
    try {
      fitKeptRef.current?.();
    } catch {
      // ignore
    }
    if (nativeSurfaceEnabled()) {
      // The whole list, so the atlas falls back through it the way
      // xterm does.
      void nativeSurfaceSetFont({
        family: fontFamily,
        size: Math.round(fontSize),
      }).catch(() => {});
    }
    // xterm just measured its cell on the faces that had loaded, and a
    // face the page mints for this list loads later. Measure again once
    // the face the list draws with has loaded (src/lib/terminalFont.ts).
    if (typeof document === 'undefined' || !document.fonts) return;
    return remeasureWhenLoaded(document.fonts, term, () => refitCellRef.current?.());
  }, [fontFamily, fontSize]);

  // Apply a line height change without rebuilding the terminal. xterm
  // resizes its cells on the option change. Under the native surface the
  // new cell goes out at once, the surface rebuilds its atlas to it, and
  // its grid size event resizes xterm to match. Otherwise fit reflows.
  useEffect(() => {
    const term = termRef.current;
    if (!term || term.options.lineHeight === lineHeight) return;
    term.options.lineHeight = lineHeight;
    if (!quietRef.current && nativeSurfaceEnabled()) {
      reportCellMetricsRef.current?.();
      return;
    }
    try {
      fitKeptRef.current?.();
    } catch {
      // ignore resize before layout settles
    }
  }, [lineHeight]);

  // Re-apply the palette when the canonical-vs-themed toggle flips
  // without needing to recreate the XTerm instance.
  useEffect(() => {
    const term = termRef.current;
    if (!term) return;
    term.options.theme = themeFor(getCurrentThemeId(), themeTerminalColors, liftsHere());
    reportTheme(getCurrentThemeId(), themeTerminalColors);
  }, [themeTerminalColors, liftsHere]);

  // Lift your prompts, or stop, when the choice changes.
  useEffect(() => {
    const term = termRef.current;
    if (term) applyLift(term);
    // applyLift reads the refs.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [lifted]);

  // Re-apply when the user edits the base ANSI palette (the colors
  // used while the tint toggle is off), turns Fit game colors on or off,
  // or a new custom theme list brings the theme on screen its fit.
  useEffect(() => {
    const reapply = () => {
      const term = termRef.current;
      if (!term) return;
      term.options.theme = themeFor(
        getCurrentThemeId(),
        themeTerminalColorsRef.current,
        liftsHere(),
      );
      reportTheme(getCurrentThemeId(), themeTerminalColorsRef.current);
    };
    const stopBase = subscribeBaseAnsi(reapply);
    const stopFit = subscribeFitGameColors(reapply);
    const stopList = onCustomThemesChanged(reapply);
    return () => {
      stopBase();
      stopFit();
      stopList();
    };
  }, [liftsHere]);

  // Live-refresh the xterm palette when the user switches themes from
  // the settings window. Listens on the cross-window theme event.
  useTauriEvent(subscribeThemeChanges, (themeId) => {
    const term = termRef.current;
    if (!term) return;
    term.options.theme = themeFor(themeId, themeTerminalColorsRef.current, liftsHere());
    reportTheme(themeId, themeTerminalColorsRef.current);
  });

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
