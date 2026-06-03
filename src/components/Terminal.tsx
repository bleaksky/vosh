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

import '@xterm/xterm/css/xterm.css';
import { loadScrollback, onOutput, setWindowSize } from '../lib/session';
import { findTheme, type AppTheme } from '../lib/themes';
import { getCurrentThemeId, subscribeThemeChanges } from '../lib/theme';
import { WordWrapper } from '../lib/wordWrap';
import { ingestRecentNames } from '../lib/recentNames';

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
  /** Search forward from the current selection (or top of buffer). Returns
   *  true when a match was found and scrolled into view. Highlights every
   *  match across the entire scrollback as a side effect. */
  findNext: (term: string, options?: FindOptions) => boolean;
  /** Search backward. Same return + decoration semantics as findNext. */
  findPrevious: (term: string, options?: FindOptions) => boolean;
  /** Clear search decorations (called when the find toolbar closes). */
  clearSearch: () => void;
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
//
// Values match the SVG chart at
// https://upload.wikimedia.org/wikipedia/commons/1/15/Xterm_256color_chart.svg
const CANONICAL_ANSI_16: Pick<
  ITheme,
  | 'black'
  | 'red'
  | 'green'
  | 'yellow'
  | 'blue'
  | 'magenta'
  | 'cyan'
  | 'white'
  | 'brightBlack'
  | 'brightRed'
  | 'brightGreen'
  | 'brightYellow'
  | 'brightBlue'
  | 'brightMagenta'
  | 'brightCyan'
  | 'brightWhite'
> = {
  black: '#000000',
  red: '#800000',
  green: '#008000',
  yellow: '#808000',
  blue: '#000080',
  magenta: '#800080',
  cyan: '#008080',
  white: '#c0c0c0',
  brightBlack: '#808080',
  brightRed: '#ff0000',
  brightGreen: '#00ff00',
  brightYellow: '#ffff00',
  brightBlue: '#0000ff',
  brightMagenta: '#ff00ff',
  brightCyan: '#00ffff',
  brightWhite: '#ffffff',
};

function xtermThemeFor(theme: AppTheme, themeTerminalColors: boolean): ITheme {
  if (themeTerminalColors) {
    // Legacy behavior: the chrome theme also colors server output.
    return { ...theme.xterm };
  }
  // Default: chrome theme controls the surfaces (background, default
  // foreground, cursor, selection) but the 16 ANSI palette stays at
  // the canonical xterm-256 values so server output reads identically
  // to a stock xterm.
  return {
    ...theme.xterm,
    ...CANONICAL_ANSI_16,
  };
}

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
    // DECTCEM: hide the cursor entirely. Belt-and-suspenders alongside
    // the cursor* options above; some terminals re-show it on certain
    // sequences and we want the read-only pane to stay clean.
    term.write('\x1b[?25l');
    // Initial fit is a moving target — the container may not have
    // its final dimensions until after one or two layout/paint
    // cycles (viewport units, fonts, etc). Schedule fit() across
    // several deadlines so at least one lands on the settled layout.
    const safeFit = () => {
      try {
        fit.fit();
      } catch {
        // ignore resize before layout settles
      }
    };
    safeFit();
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
    const sync = () => {
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
    };

    const handleWindowResize = sync;
    window.addEventListener('resize', handleWindowResize);
    const observer = new ResizeObserver(sync);
    if (sizer) observer.observe(sizer);
    observer.observe(document.body);

    // Per-frame sync as belt-and-suspenders for Tauri/WebKit cases
    // where ResizeObserver doesn't fire on sibling chrome growth.
    let rafId = 0;
    const pollLoop = () => {
      sync();
      rafId = requestAnimationFrame(pollLoop);
    };
    rafId = requestAnimationFrame(pollLoop);

    let unsubOutput: (() => void) | undefined;
    // Replay persisted scrollback before any live output lands so the
    // user opens the app to the tail of their last session.
    const notifyPosition = () => {
      const buf = term.buffer.active;
      onScrollPositionRef.current?.(buf.baseY - buf.viewportY, buf.baseY);
    };
    term.onScroll(notifyPosition);

    loadScrollback()
      .then((bytes) => {
        if (bytes.length > 0) {
          term.write(bytes);
          if (!quietRef.current) {
            term.write('\r\n\x1b[2m[scrollback restored]\x1b[0m\r\n');
          }
        }
        // xterm.write batches into an internal queue; flush before
        // notifying so any onScrollbackLoaded handler that adjusts
        // the viewport sees the final row count, not the count
        // before the buffered bytes were rendered.
        const notify = () => {
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
      const accent = rootStyle.getPropertyValue('--c-accent').trim() || '#ff3399';
      const accentSoft =
        rootStyle.getPropertyValue('--c-accent-soft').trim() || 'rgba(255, 51, 153, 0.25)';
      return {
        matchBackground: accentSoft,
        matchBorder: accent,
        matchOverviewRuler: accent,
        activeMatchBackground: accent,
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
      // viewportY tracks the top of the viewport in scrollback coords;
      // baseY tracks the top of the bottom page. Equal means the
      // viewport is anchored to the live tail.
      isAtBottom: () => term.buffer.active.viewportY === term.buffer.active.baseY,
      getSize: () => ({ cols: term.cols, rows: term.rows }),
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
    };
    onReadyRef.current?.(handle);

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
      cancelAnimationFrame(rafId);
      observer.disconnect();
      window.removeEventListener('resize', handleWindowResize);
      window.removeEventListener('keydown', onCopyKey, true);
      if (naws_timer) clearTimeout(naws_timer);
      unsubOutput?.();
      resultsSub.dispose();
      searchAddon.dispose();
      term.dispose();
      termRef.current = null;
      fitRef.current = null;
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
  }, [fontFamily, fontSize]);

  // Re-apply the palette when the canonical-vs-themed toggle flips
  // without needing to recreate the XTerm instance.
  useEffect(() => {
    const term = termRef.current;
    if (!term) return;
    term.options.theme = xtermThemeFor(findTheme(getCurrentThemeId()), themeTerminalColors);
  }, [themeTerminalColors]);

  // Live-refresh the xterm palette when the user switches themes from
  // the settings window. Listens on the cross-window theme event.
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    subscribeThemeChanges((themeId) => {
      const term = termRef.current;
      if (!term) return;
      term.options.theme = xtermThemeFor(findTheme(themeId), themeTerminalColorsRef.current);
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
