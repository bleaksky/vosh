import { useCallback, useEffect, useLayoutEffect, useRef } from 'react';
import { Terminal as XTerm } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import { WebLinksAddon } from '@xterm/addon-web-links';
import { Unicode11Addon } from '@xterm/addon-unicode11';
import { SearchAddon, type ISearchResultChangeEvent } from '@xterm/addon-search';

import '@xterm/xterm/css/xterm.css';
import { subscribeBaseAnsi } from '../theme/baseAnsi';
import {
  nativeSurfaceSetFont,
  nativeSurfaceSetTheme,
  onNativeGridSize,
} from '../ipc/nativeSurface';
import { setWindowSize } from '../ipc/session';
import { othersOnProfile } from '../stores/session/sessionsStore';
import { loadScrollback, onOutput, terminalLocalWrite } from '../ipc/terminal';
import { useTauriEvent } from '../ipc/useTauriEvent';
import { findTheme, onCustomThemesChanged } from '../theme/themes';
import {
  getColorVision,
  getFitGameColors,
  subscribeColorVision,
  subscribeFitGameColors,
} from '../theme/fitGameColors';
import { setHighlightGround } from './highlightGround';
import { nativeThemeOf, xtermThemeFor } from './terminalTheme';
import { getCurrentThemeId, subscribeThemeChanges } from '../theme/theme';
import { OutputShaper } from './outputShaper';
import { RegionWriter } from './terminalRegion';
import { remeasureWhenLoaded } from './terminalFont';
import { nativeSurfaceEnabled } from './terminalRenderer';
import { BandLayer, LiftTracker, markLifted } from './xterm/liftBands';
import { GameSizeReport, gameSize, keepTail, type WindowSize } from './terminalRows';
import { noteReader } from './readerBusy';
import { ingestRecentNames } from '../input/recentNames';
import { underlayShows, XtermMirror } from './xterm/xtermMirror';
import {
  refillsWashes,
  WashPainter,
  washFields,
  WashWidth,
  type WashFields,
} from './xterm/xtermWash';
import { XtermBlink } from './xterm/xtermBlink';
import { xtermWebgl } from './xterm/xtermWebgl';
import { forwardUnderlayPointer } from './native/underlayPointer';
import { PaneSizer } from './paneSizer';
import { terminalHandle, type TerminalHandle } from './terminalHandle';

// Report the active theme's terminal background to the session, which
// keeps trigger colors readable on it, on either renderer. Then report the
// surface colors and resolved ANSI palette to the native renderer so its
// background/foreground/selection and the 16-color palette match xterm
// (including the themeTerminalColors tint, Fit game colors and the
// color vision it fits for),
// live-updating on theme or toggle change.
function reportTheme(themeId: string, themeTerminalColors: boolean): void {
  const theme = findTheme(themeId);
  setHighlightGround(theme.xterm.background);
  if (!nativeSurfaceEnabled()) return;
  const native = nativeThemeOf(theme, themeTerminalColors, getFitGameColors(), getColorVision());
  void nativeSurfaceSetTheme({
    background: native.background,
    foreground: native.foreground,
    selection: native.selection,
    ansi: native.ansi,
  }).catch(() => {});
}

interface Props {
  /** The session whose terminal this is. It hears that session's output
   *  only and names that session on every call. The host keys each pane
   *  by its session, so a pane keeps the one it mounted with. */
  session: number;
  /** Whether the pane shows, as the selected session's live pane does.
   *  The host keeps the live pane of each session it opened mounted and
   *  hides the others with display none, where xterm stops drawing. A
   *  hidden pane goes on taking its session's output, holds no WebGL
   *  context, and sizes, reports and copies nothing. */
  shown?: boolean;
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
  onScrollbackLoaded?: (() => void) | undefined;
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
  /// while your prompt takes more than one row (src/terminal/terminalRows.ts).
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
  /// (src/terminal/xterm/xtermBlink.ts) while WebGL draws the pane. Off, or on the
  /// DOM renderer, it draws it steady.
  blinkText?: boolean;
  /// Scrollback size, the lines xterm keeps above the screen (D40).
  scrollback?: number;
}

// The terminal's palette lives in src/terminal/terminalTheme.ts, which the
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
  const theme = xtermThemeFor(findTheme(themeId), tinted, getFitGameColors(), getColorVision());
  if (clear) theme.background = clearGround(theme.background);
  return theme;
}

/** The fields xterm paints washed rows in, from the palette and the
 *  ground the native renderer draws, the ground before it goes clear. */
function washFieldsFor(themeId: string, tinted: boolean): WashFields {
  const native = nativeThemeOf(findTheme(themeId), tinted, getFitGameColors(), getColorVision());
  return washFields(native.ansi, native.background);
}

export function Terminal({
  session,
  shown = true,
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
  scrollback = 10_000,
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
  const shownRef = useRef(shown);
  shownRef.current = shown;
  // What the pane does as it shows or hides, which the setup effect
  // sets, and whether it shows as far as the effect below applied.
  const showingRef = useRef<{ show(): void; hide(): void } | null>(null);
  const appliedShownRef = useRef(shown);
  // The rows lent to the pinned band, and whether the grid keeps to the
  // bottom of its pane, which the layout effect below keeps and applies
  // before the page paints.
  const lentRef = useRef(lentRows);
  const anchorRef = useRef(anchorBottom);
  // Sizes the pane and places its grid, for the effects after setup.
  // Set by the setup effect.
  const paneSizerRef = useRef<PaneSizer | null>(null);
  // The band layer, while this pane can draw bands.
  const bandsRef = useRef<BandLayer | null>(null);
  // Blinking text on xterm, and whether it is on.
  const blinkRef = useRef<XtermBlink | null>(null);
  const blinkTextRef = useRef(blinkText);
  blinkTextRef.current = blinkText;
  const containerRef = useRef<HTMLDivElement | null>(null);
  const termRef = useRef<XTerm | null>(null);
  const sizingRef = useRef<HTMLDivElement | null>(null);
  // Mirror the flag in a ref so the long-lived effect (which creates
  // the XTerm instance once) doesn't re-create the terminal every
  // time the user toggles the setting.
  const themeTerminalColorsRef = useRef(themeTerminalColors);
  themeTerminalColorsRef.current = themeTerminalColors;
  // The wash fields of the theme the pane draws, which every write
  // paints washed rows in.
  const washRef = useRef<WashFields>(new Map());
  // How narrow the pane was for the washes it painted since it last
  // filled, and how it fills anew in the fields in force, which the
  // setup effect sets.
  const washWidthRef = useRef(new WashWidth());
  const refillRef = useRef<(() => void) | null>(null);
  // Hold the latest onReady in a ref so the setup effect can call it without
  // listing it as a dependency. Without this, every parent re-render passes
  // a fresh arrow function, the effect re-runs, and the xterm instance is
  // disposed and recreated, wiping all output.
  const onReadyRef = useRef(onReady);
  onReadyRef.current = onReady;

  // Whether this pane draws bands now: your prompt shows lifted, xterm
  // draws the terminal and the pane shows.
  const liftsHere = useCallback(
    () => liftedRef.current && bandsRef.current !== null && shownRef.current,
    [],
  );

  // The lift state the pane last applied, so the default never touches
  // xterm's options.
  const appliedLiftRef = useRef(false);

  // Draw with the theme `themeId`, its ground clear while the pane lifts
  // your prompts, and paint washes in its colors. xterm keeps the colors
  // a row was written in, so washes already on screen take the new
  // fields as the pane fills anew from the scrollback.
  const applyTheme = (term: XTerm, themeId: string = getCurrentThemeId()) => {
    term.options.theme = themeFor(themeId, themeTerminalColorsRef.current, liftsHere());
    const fields = washFieldsFor(themeId, themeTerminalColorsRef.current);
    const refill = refillsWashes(washRef.current, fields, washWidthRef.current.washed());
    washRef.current = fields;
    if (refill) refillRef.current?.();
  };

  // Turn the bands on or off, with the clear ground they need.
  const applyLift = (term: XTerm) => {
    const on = liftsHere();
    if (on === appliedLiftRef.current) return;
    appliedLiftRef.current = on;
    term.options.allowTransparency = on;
    applyTheme(term);
    // The area is every pane's, so the pane that shows marks it, and a
    // pane that hides leaves the mark to the one that shows next.
    const area = containerRef.current?.closest('.terminal-area');
    if (area && shownRef.current) markLifted(area, on);
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
      scrollback,
      allowProposedApi: true,
      convertEol: false,
      // High-precision touchpads emit many small deltaY events per
      // gesture; xterm's default scrollSensitivity (1) compounds
      // these into a runaway scroll on macOS. 0.75 shrinks the per-
      // event step so trackpad scrolling feels controllable.
      // smoothScrollDuration stays off, since consecutive scrollLines
      // animations stack and the scroll stops feeling direct. The
      // useScrollbackSplit.ts accumulator steps it at once instead.
      scrollSensitivity: 0.75,
      theme: themeFor(getCurrentThemeId(), themeTerminalColorsRef.current, false),
    });
    washRef.current = washFieldsFor(getCurrentThemeId(), themeTerminalColorsRef.current);
    const fields = () => washRef.current;
    const washed = () => washWidthRef.current.painted(term.cols);

    // Every write to this xterm goes through one ordered writer, which
    // finds the regions the session marks and replaces them only while
    // nothing was written after them (src/terminal/terminalRegion.ts). Every
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
    // The output is one Tab stop on its slot (Q22), so xterm's hidden
    // input leaves the Tab order and never eats a Tab.
    if (term.textarea) term.textarea.tabIndex = -1;
    term.attachCustomKeyEventHandler((event) => event.key !== 'Tab');
    const blink = new XtermBlink(term);
    blink.setOn(blinkTextRef.current);
    blinkRef.current = blink;
    const area = containerRef.current.closest<HTMLElement>('.terminal-area');
    if (lifts && area) {
      bandsRef.current = new BandLayer(term, lifts, area);
      applyLift(term);
    }

    // Sizes the pane and places the grid in it, for xterm and the native
    // grid alike (src/terminal/paneSizer.ts).
    const sizer = sizingRef.current;
    const host = containerRef.current;
    const paneSizer = new PaneSizer({
      term,
      fit,
      sizer,
      host,
      resize: (cols, rows) => writer.resize(cols, rows),
      lent: () => lentRef.current,
      anchor: () => anchorRef.current,
      quiet: () => quietRef.current,
      shown: () => shownRef.current,
      onCellSize: () => onCellSizeRef.current,
    });
    paneSizerRef.current = paneSizer;

    // GPU renderer. xterm's WebGL addon must run after term.open() and
    // it reads the host element's pixel size when it allocates the
    // glyph atlas — load it after one fit so the host has nonzero
    // dimensions. It loads here, not on a later frame, since a renderer
    // swap in the same frame as a scheduled fit() leaves
    // `_renderer.value` undefined and syncScrollArea throws. A pane that
    // mounts hidden loads it when it shows, after the fit there.
    paneSizer.safeFit();
    const webgl = xtermWebgl(term, blink, quietRef.current);
    if (shownRef.current) webgl.load();
    requestAnimationFrame(paneSizer.safeFit);
    setTimeout(paneSizer.safeFit, 50);
    setTimeout(paneSizer.safeFit, 200);
    setTimeout(paneSizer.safeFit, 800);
    termRef.current = term;
    paneSizer.start();

    const detachUnderlayInput = forwardUnderlayPointer(
      sizer,
      quietRef.current,
      () => paneSizer.nativeSpare,
    );

    // The game hears the size of the pane through NAWS, and wraps its
    // lines server-side at the column count it hears, which is what
    // well-behaved word wrap looks like, with no client preprocessing and
    // no latency. The pane that shows tells its own session, and the other
    // sessions on its profile hear the same size, since they share the
    // panel and the font. A session on another profile hears its size as
    // its pane shows. A row the pinned band borrows or gives back sends
    // nothing, since the game is told the rows the pane holds with the
    // lent ones (src/terminal/terminalRows.ts).
    const gameSizes = new GameSizeReport();
    const tellSize = (size: WindowSize, sessions: number[]) => {
      for (const to of sessions) {
        void setWindowSize(size.cols, size.rows, to).catch(() => {
          // Not connected, or session torn down. Either is fine; the
          // initial NAWS handshake on the next connect will send the
          // current size anyway.
        });
      }
    };

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
        // A frame tells the session whose grid shows its size. The other
        // sessions on its profile share the pane, so they hear it here.
        if (cols > 0 && rows > 0 && shownRef.current) {
          const size = gameSizes.next(cols, rows, lentRef.current);
          if (size) tellSize(size, othersOnProfile(session));
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

    // Decoded across outputs, word wrapped and its washes painted
    // (src/terminal/outputShaper.ts). A copy that fills anew starts a
    // shaper of its own.
    let shaper = new OutputShaper(term.cols, fields, washed);

    // Fill the copy anew from the session's scrollback, word wrapped as
    // live output is and in the wash fields in force, and keep the live
    // pane at its tail. The history pane keeps
    // the row it shows. Only a copy the screen gave back says the
    // scrollback was restored, since its writes stopped meanwhile. A fill
    // that a newer one overtook writes nothing and settles nothing, since
    // the writer and the screen are the newer fill's. The reset goes in
    // the stream (RIS), since xterm still parses what an earlier writer
    // handed it after a reset called from here, and the history would
    // show twice.
    let fills = 0;
    const fill = (banner: boolean, done: () => void) => {
      const gen = ++fills;
      const top = term.buffer.active.viewportY;
      writer.dispose();
      writer = new RegionWriter(term);
      if (lifts) writer.onErase((row, col) => lifts.dropFrom(row, col));
      writer.local('\x1bc');
      shaper = new OutputShaper(term.cols, fields, washed);
      washWidthRef.current.filled();
      const settle = () => {
        if (gen !== fills) return;
        if (quietRef.current) {
          term.scrollToLine(top);
        } else {
          padToBottom();
          keepTail(term);
        }
        done();
      };
      loadScrollback(false, session)
        .then(({ bytes }) => {
          if (gen !== fills) return;
          if (bytes.length === 0) return settle();
          writer.local(shaper.whole(localDecoder.decode(bytes)));
          if (banner) writer.local('\r\n\x1b[38;5;244m[scrollback restored]\x1b[0m\r\n');
          // The pad reads where the cursor sits once xterm parsed it all.
          writer.whenParsed(settle);
        })
        .catch(settle);
    };

    // While the native underlay draws the live terminal, the xterm copy
    // hides and takes no writes (src/terminal/xterm/xtermMirror.ts). It keeps its
    // size, which the native grid sets, and the cell size it measures,
    // which the grid and the pinned band draw on. Find, selection, copy,
    // the prompt card and the scroll all go to the native grid then. If
    // the screen comes back to xterm, the copy fills anew from the
    // session's scrollback, as a reload onto xterm fills it.
    const mirror = new XtermMirror({
      owned: () =>
        !quietRef.current && nativeSurfaceEnabled() && underlayShows(document.documentElement),
      rebuild: (done) => fill(true, done),
    });
    refillRef.current = () => mirror.refill((done) => fill(false, done));
    const underlayWatch = new MutationObserver(() => mirror.check());
    underlayWatch.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['data-underlay'],
    });

    // The live pane seeds the native grid with the persisted scrollback so
    // it has the same history as xterm; the quiet history pane must not.
    loadScrollback(!quietRef.current && nativeSurfaceEnabled(), session)
      .then(({ bytes, seededNative }) => {
        // A copy the native grid hides takes none of it, and neither does
        // a copy that began filling anew, since the fill writes it all.
        const toXterm = mirror.mirrors() && fills === 0;
        if (bytes.length > 0) {
          if (toXterm) {
            writer.local(WashPainter.whole(localDecoder.decode(bytes), washRef.current, washed));
          }
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
              void terminalLocalWrite(banner, null, session).catch(() => {});
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
    // A washed row keeps the width it was written at, so a pane that grows
    // past it fills anew once the size settles, and the field reaches the
    // new edge as it does natively.
    let widthRefill: ReturnType<typeof setTimeout> | null = null;
    const refillWhenSettled = () => {
      if (widthRefill) clearTimeout(widthRefill);
      widthRefill = setTimeout(() => {
        widthRefill = null;
        if (washWidthRef.current.outgrown(term.cols)) refillRef.current?.();
      }, 150);
    };

    // Client-side word wrap. NAWS handles most lines server-side, but
    // some content paths (tells, comm channels) ignore it on certain
    // ROM derivatives. We line-buffer here so a complete line word-
    // wraps cleanly before hitting xterm.
    //
    // The trailing partial (the prompt) flushes at the end of every
    // chunk rather than after an idle wait, since any wait lets
    // GMCP-driven UI (room strip, vitals, map) land a frame before the
    // text it belongs to. The backend sends each TCP read as one output
    // event, so the line-buffer's wrap math still has the whole line
    // for any line ending in \n, and only the prompt at the chunk's
    // tail flushes before its newline. The shaper is made above, with
    // the mirror.
    term.onResize(({ cols }) => {
      shaper.setCols(cols);
      if (washWidthRef.current.outgrown(cols)) refillWhenSettled();
      paneSizer.reportCellSize();
      paneSizer.placeGrid();
      // Same tail-anchor rationale as in onOutput below: a resize
      // shifts baseY without moving viewportY, which can land the
      // live pane above its tail. Snap on resize so the freeze
      // resolves even when no data is currently arriving (the user
      // dragged the divider, then waited — no write would have
      // otherwise unstuck the viewport until the next server line).
      // Only a pane that left its tail snaps: xterm's scrollbar still
      // has the old rows here, and a scroll asked of it now lands a
      // row short, the newest line under the screen
      // (src/terminal/terminalRows.ts).
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
    // Each pane writes its own session's output only, and a write to
    // another session's terminal decodes nothing here.
    onOutput(
      (from) => from === session,
      (out) => {
        if (out.id !== undefined && out.id > outputTaken) outputTaken = out.id;
        // A copy the native grid hides writes nothing. It only decodes
        // the text for the recent names cache below.
        if (!mirror.mirrors()) {
          if (!quietRef.current) {
            const { text, replace } = shaper.text(out);
            ingestRecentNames(text, session);
            if (replace !== null) ingestRecentNames(replace, session);
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
          // frame behind (src/terminal/terminalRows.ts).
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
          ingestRecentNames(text, session);
          if (output?.replace) ingestRecentNames(output.replace.text, session);
        }
      },
    ).then((unlisten) => {
      unsubOutput = unlisten;
    });

    // Push the live terminal size to the backend so the telnet
    // negotiator can advertise it via NAWS (see tellSize above). Debounce
    // so a rapid resize animation only fires one IPC at the settled
    // size. Only the primary (non-quiet) terminal that shows pushes,
    // since the history pane in the split view shares the same width.
    // When the native surface owns the terminal it advertises its own
    // (larger) grid size as NAWS, and the grid size listener above tells
    // the other sessions, so xterm must not fight it with the
    // webview-font column count.
    let naws_timer: ReturnType<typeof setTimeout> | null = null;
    // Set as the pane shows, so the next push tells its own session its
    // size even when it held. The pane of another session spoke for the
    // window meanwhile.
    let tellOwn = false;
    const pushSize = () => {
      if (quietRef.current || !shownRef.current || nativeSurfaceEnabled()) return;
      const lent = lentRef.current;
      const size = gameSizes.next(term.cols, term.rows, lent);
      if (size) tellSize(size, [session, ...othersOnProfile(session)]);
      else if (tellOwn) tellSize(gameSize(term.cols, term.rows, lent), [session]);
      tellOwn = false;
    };
    const scheduleSizePush = () => {
      if (naws_timer) clearTimeout(naws_timer);
      naws_timer = setTimeout(pushSize, 120);
    };
    term.onResize(scheduleSizePush);
    // First push after the deferred fits settle so the backend gets
    // the real size rather than the 80x24 default xterm starts with.
    setTimeout(pushSize, 900);

    // What the host reaches the pane by (src/terminal/terminalHandle.ts).
    const handle = terminalHandle({
      term,
      searchAddon,
      paneSizer,
      sizer,
      host,
      write: (data) => {
        const text = typeof data === 'string' ? data : localDecoder.decode(data);
        mirror.write(() => writer.local(text));
      },
      outputTaken: () => outputTaken,
      region: () => writer.region(),
      quiet: () => quietRef.current,
      lent: () => lentRef.current,
      session,
    });
    onReadyRef.current?.(handle);

    // A pane that shows again lifts your prompts again, fits the pane as
    // it is now, takes WebGL back after that fit, as it does as it
    // mounts, and tells its session its size. One that hides lets WebGL
    // go, so only the pane that shows holds a GL context, and draws no
    // bands.
    showingRef.current = {
      show: () => {
        applyLift(term);
        paneSizer.show();
        webgl.load();
        tellOwn = true;
        scheduleSizePush();
      },
      hide: () => {
        webgl.release();
        applyLift(term);
      },
    };

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
      noteReader(selectionPart, term.hasSelection(), session),
    );
    const readerBack = quietRef.current
      ? null
      : term.onScroll(() =>
          noteReader(
            'liveBack',
            term.buffer.active.viewportY !== term.buffer.active.baseY,
            session,
          ),
        );

    // Ctrl/Cmd + C or X copies the xterm selection. The keystroke
    // almost always lands while focus is in the Input box (the user
    // drag-selects xterm output, then hits the shortcut without
    // clicking back into the terminal), so a keydown listener
    // attached to xterm alone never fires. Listen at the window
    // instead, and defer to the focused element's native copy/cut
    // when it actually has its own selection. A hidden pane keeps its
    // selection for when it shows, and copies nothing meanwhile.
    const onCopyKey = (event: KeyboardEvent) => {
      if (!shownRef.current) return;
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
      paneSizer.stop();
      window.removeEventListener('keydown', onCopyKey, true);
      detachUnderlayInput?.();
      if (naws_timer) clearTimeout(naws_timer);
      if (widthRefill) clearTimeout(widthRefill);
      unsubOutput?.();
      unsubGridSize?.();
      underlayWatch.disconnect();
      refillRef.current = null;
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
      noteReader(selectionPart, false, session);
      if (readerBack) noteReader('liveBack', false, session);
      searchAddon.dispose();
      webgl.release();
      term.dispose();
      termRef.current = null;
      paneSizerRef.current = null;
      showingRef.current = null;
    };
    // Setup runs exactly once. Font is read from props on initial mount;
    // later font changes re-apply via the effect below without disposing
    // the xterm instance.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // The pane shows or hides as a selection moves, before the page
  // paints, so it never shows a frame at the size it had. The setup
  // effect applied the state the pane mounted with.
  useLayoutEffect(() => {
    if (appliedShownRef.current === shown) return;
    appliedShownRef.current = shown;
    if (shown) showingRef.current?.show();
    else showingRef.current?.hide();
  }, [shown]);

  // A new count of rows lent to the pinned band applies before the page
  // paints, in the same commit that grows or shrinks the band, so the
  // newest line moves with the band's top and never sits under it. So
  // does the grid starting or stopping keeping to the bottom.
  useLayoutEffect(() => {
    if (lentRef.current === lentRows && anchorRef.current === anchorBottom) return;
    lentRef.current = lentRows;
    anchorRef.current = anchorBottom;
    paneSizerRef.current?.relayout();
  }, [lentRows, anchorBottom]);

  // Blinking text turns on or off live.
  useEffect(() => {
    blinkRef.current?.setOn(blinkText);
  }, [blinkText]);

  // Apply font changes without rebuilding the terminal so scrollback and
  // listeners survive. xterm reflows on the next fit() call.
  useEffect(() => {
    const term = termRef.current;
    const paneSizer = paneSizerRef.current;
    if (!term || !paneSizer) return;
    term.options.fontFamily = fontFamily;
    term.options.fontSize = fontSize;
    try {
      paneSizer.fitKept();
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
    // the face the list draws with has loaded (src/terminal/terminalFont.ts).
    if (typeof document === 'undefined' || !document.fonts) return;
    return remeasureWhenLoaded(document.fonts, term, () => paneSizerRef.current?.refitCell());
  }, [fontFamily, fontSize]);

  // Scrollback size changes without rebuilding the terminal. A smaller
  // size drops the oldest lines.
  useEffect(() => {
    const term = termRef.current;
    if (term && term.options.scrollback !== scrollback) term.options.scrollback = scrollback;
  }, [scrollback]);

  // Apply a line height change without rebuilding the terminal. xterm
  // resizes its cells on the option change. Under the native surface the
  // new cell goes out at once, the surface rebuilds its atlas to it, and
  // its grid size event resizes xterm to match. Otherwise fit reflows.
  useEffect(() => {
    const term = termRef.current;
    if (!term || term.options.lineHeight === lineHeight) return;
    term.options.lineHeight = lineHeight;
    if (!quietRef.current && nativeSurfaceEnabled()) {
      paneSizerRef.current?.reportCellMetrics();
      return;
    }
    try {
      paneSizerRef.current?.fitKept();
    } catch {
      // ignore resize before layout settles
    }
  }, [lineHeight]);

  // Re-apply the palette when the canonical-vs-themed toggle flips
  // without needing to recreate the XTerm instance.
  useEffect(() => {
    const term = termRef.current;
    if (!term) return;
    applyTheme(term);
    reportTheme(getCurrentThemeId(), themeTerminalColors);
    // applyTheme reads the refs.
    // eslint-disable-next-line react-hooks/exhaustive-deps
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
  // picks another color vision, or a new custom theme list brings the
  // theme on screen its fit.
  useEffect(() => {
    const reapply = () => {
      const term = termRef.current;
      if (!term) return;
      applyTheme(term);
      reportTheme(getCurrentThemeId(), themeTerminalColorsRef.current);
    };
    const stopBase = subscribeBaseAnsi(reapply);
    const stopFit = subscribeFitGameColors(reapply);
    const stopVision = subscribeColorVision(reapply);
    const stopList = onCustomThemesChanged(reapply);
    return () => {
      stopBase();
      stopFit();
      stopVision();
      stopList();
    };
    // applyTheme reads the refs.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [liftsHere]);

  // Live-refresh the xterm palette when the user switches themes from
  // the settings window. Listens on the cross-window theme event.
  useTauriEvent(subscribeThemeChanges, (themeId) => {
    const term = termRef.current;
    if (!term) return;
    applyTheme(term, themeId);
    reportTheme(themeId, themeTerminalColorsRef.current);
  });

  return (
    <div ref={sizingRef} className="terminal-sizer" hidden={!shown}>
      {/* The host holds xterm's canvas and no text to read. The game
          reaches a screen reader through ScreenReaderFeed beside it. */}
      <div ref={containerRef} className="terminal-host" />
    </div>
  );
}
