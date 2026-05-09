import { useEffect, useRef } from 'react';
import { Terminal as XTerm, type ITheme } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import { WebLinksAddon } from '@xterm/addon-web-links';
import { Unicode11Addon } from '@xterm/addon-unicode11';

import '@xterm/xterm/css/xterm.css';
import { loadScrollback, onGmcp, onOutput } from '../lib/session';
import { decorateCombat } from '../lib/combat';
import { combatOverlayStore } from '../lib/combatOverlay';

export interface TerminalHandle {
  write: (data: Uint8Array | string) => void;
  fit: () => void;
  focus: () => void;
  clear: () => void;
}

interface Props {
  onReady?: (handle: TerminalHandle) => void;
  fontFamily: string;
  fontSize: number;
}

// Kanso Zen palette, matching the user's Ghostty config so the terminal
// inside the app looks the same as the one outside it. ANSI 7 (white)
// in the published Kanso Zen palette is `#c8c093` — a warm tan that the
// MUD's plain "white" labels render as a yellow-gold. We keep every
// other slot faithful to the theme but pin 7 to the cool fg gray so
// "white" reads as the user expects.
const KANSO_ZEN_THEME: ITheme = {
  background: '#090e13',
  foreground: '#c5c9c7',
  cursor: '#c5c9c7',
  cursorAccent: '#090e13',
  selectionBackground: '#393b44',
  selectionForeground: '#c5c9c7',
  black: '#0d0c0c',
  red: '#c4746e',
  green: '#8a9a7b',
  yellow: '#c4b28a',
  blue: '#8ba4b0',
  magenta: '#a292a3',
  cyan: '#8ea4a2',
  white: '#c5c9c7',
  brightBlack: '#a4a7a4',
  brightRed: '#e46876',
  brightGreen: '#87a987',
  brightYellow: '#e6c384',
  brightBlue: '#7fb4ca',
  brightMagenta: '#938aa9',
  brightCyan: '#7aa89f',
  brightWhite: '#f0f3f1',
};

export function Terminal({ onReady, fontFamily, fontSize }: Props) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const termRef = useRef<XTerm | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const sizingRef = useRef<HTMLDivElement | null>(null);
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
      theme: KANSO_ZEN_THEME,
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
    let unsubGmcp: (() => void) | undefined;
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
      const opts = combatOverlayStore.effective();
      term.write(decorateCombat(bytes, opts));
    }).then((unlisten) => {
      unsubOutput = unlisten;
    });

    // Track player level so the auto-hide threshold can suppress
    // labels for veterans. Per the Aabahran GMCP wiki, level only
    // appears in Char.Status (name/level/race/class), not Char.Vitals.
    onGmcp((payload) => {
      if (
        payload.package === 'Char.Status' &&
        payload.data &&
        typeof payload.data === 'object'
      ) {
        const data = payload.data as Record<string, unknown>;
        const raw = data.level;
        const lvl =
          typeof raw === 'number' ? raw : raw !== undefined ? Number(raw) : NaN;
        if (Number.isFinite(lvl)) {
          combatOverlayStore.setLevel(lvl);
        }
      }
    }).then((unlisten) => {
      unsubGmcp = unlisten;
    });

    const handle: TerminalHandle = {
      write: (data) => term.write(data),
      fit: () => fit.fit(),
      focus: () => term.focus(),
      clear: () => term.clear(),
    };
    onReadyRef.current?.(handle);

    return () => {
      cancelAnimationFrame(rafId);
      observer.disconnect();
      window.removeEventListener('resize', handleWindowResize);
      unsubOutput?.();
      unsubGmcp?.();
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
