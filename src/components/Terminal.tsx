import { useEffect, useRef } from 'react';
import { Terminal as XTerm, type ITheme } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import { WebLinksAddon } from '@xterm/addon-web-links';
import { Unicode11Addon } from '@xterm/addon-unicode11';

import '@xterm/xterm/css/xterm.css';
import { loadScrollback, onOutput } from '../lib/session';
import { findTheme, type AppTheme } from '../lib/themes';
import { getCurrentThemeId, subscribeThemeChanges } from '../lib/theme';

export interface TerminalHandle {
  write: (data: Uint8Array | string) => void;
  fit: () => void;
  focus: () => void;
  clear: () => void;
  /** Scroll the xterm scrollback by N pages. Negative N scrolls up. */
  scrollPages: (n: number) => void;
  /** Scroll the xterm scrollback by N lines. Negative N scrolls up. */
  scrollLines: (n: number) => void;
}

interface Props {
  onReady?: (handle: TerminalHandle) => void;
  fontFamily: string;
  fontSize: number;
  /// When true the chrome theme tints server output too. When false
  /// (the default) server output uses the canonical xterm-256
  /// palette regardless of theme.
  themeTerminalColors: boolean;
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

export function Terminal({ onReady, fontFamily, fontSize, themeTerminalColors }: Props) {
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
    loadScrollback()
      .then((bytes) => {
        if (bytes.length > 0) {
          term.write(bytes);
          term.write('\r\n\x1b[2m[scrollback restored]\x1b[0m\r\n');
        }
      })
      .catch(() => {
        // No scrollback yet, or backend not ready; ignore.
      });
    onOutput((bytes) => {
      term.write(bytes);
    }).then((unlisten) => {
      unsubOutput = unlisten;
    });

    const handle: TerminalHandle = {
      write: (data) => term.write(data),
      fit: () => fit.fit(),
      focus: () => term.focus(),
      clear: () => term.clear(),
      scrollPages: (n) => term.scrollPages(n),
      scrollLines: (n) => term.scrollLines(n),
    };
    onReadyRef.current?.(handle);

    // Cmd+C / Ctrl+Shift+C copies the xterm selection. The keystroke
    // almost always lands while focus is in the Input box (the user
    // drag-selects xterm output, then hits the shortcut without
    // clicking back into the terminal), so a keydown listener
    // attached to xterm alone never fires. Listen at the window
    // instead, and defer to the focused element's native selection
    // when it actually has one of its own.
    const onCopyKey = (event: KeyboardEvent) => {
      const key = event.key.toLowerCase();
      const isMacCopy = event.metaKey && !event.ctrlKey && !event.altKey && key === 'c';
      const isNonMacCopy = event.ctrlKey && event.shiftKey && key === 'c';
      if (!isMacCopy && !isNonMacCopy) return;
      const selection = term.getSelection();
      if (!selection) return;
      const active = document.activeElement as HTMLInputElement | HTMLTextAreaElement | null;
      const activeHasSelection =
        active &&
        'selectionStart' in active &&
        active.selectionStart !== active.selectionEnd;
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
      unsubOutput?.();
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
