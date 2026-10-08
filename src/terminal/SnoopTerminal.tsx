import { useEffect, useRef } from 'react';
import { Terminal as XTerm } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import { Unicode11Addon } from '@xterm/addon-unicode11';
import '@xterm/xterm/css/xterm.css';
import { useTauriEvent } from '../ipc/useTauriEvent';
import { SNOOP_LINES, snoopText, subscribeSnoopOutput } from '../stores/session/snoopStore';
import { subscribeBaseAnsi } from '../theme/baseAnsi';
import {
  getColorVision,
  getFitGameColors,
  subscribeColorVision,
  subscribeFitGameColors,
} from '../theme/fitGameColors';
import { getCurrentThemeId, subscribeThemeChanges } from '../theme/theme';
import { findTheme, onCustomThemesChanged } from '../theme/themes';
import { remeasureWhenLoaded } from './terminalFont';
import { xtermThemeFor } from './terminalTheme';
import { WordWrapper } from './wordWrap';

// One snooped player's screen (Snoop SN3), a small xterm for each tab of
// the snoop split. It draws in your terminal's face, size and line
// height and in the theme's colors, keeps the game's ANSI, and word
// wraps at its width as your terminal does. It keeps 5,000 lines.
//
// Nothing of your automation reaches it. It has no region writer, no
// lifted prompt bands, no highlight ground and no pane sizer, and the
// snoop text never passes the session's triggers, highlights, gags or
// sounds. Lua still hears the Snoop packets in the backend.
//
// It is xterm on both renderers. While the native surface draws your
// terminal under the page, the snoop draws over its own strip of the
// column on an opaque ground, and the native grid takes the rest.
//
// It starts from every line its tab holds in the snoop store, then
// writes each new piece. A snapshot the store takes in place of what it
// held writes the tab afresh.

interface Props {
  session: number;
  /** The snooped player, whose text this terminal shows. */
  name: string;
  /** Whether its tab is in front. A terminal behind keeps writing, and
   *  fits once it shows. */
  shown: boolean;
  fontFamily: string;
  fontSize: number;
  lineHeight: number;
  themeTerminalColors: boolean;
}

/** The xterm theme for the theme in front, as your terminal draws it. */
function themeOf(themeId: string, tinted: boolean) {
  return xtermThemeFor(findTheme(themeId), tinted, getFitGameColors(), getColorVision());
}

export function SnoopTerminal({
  session,
  name,
  shown,
  fontFamily,
  fontSize,
  lineHeight,
  themeTerminalColors,
}: Props) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const termRef = useRef<XTerm | null>(null);
  const fitRef = useRef<() => void>(() => {});
  const shownRef = useRef(shown);
  shownRef.current = shown;
  const tintedRef = useRef(themeTerminalColors);
  tintedRef.current = themeTerminalColors;

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    const term = new XTerm({
      cursorBlink: false,
      cursorInactiveStyle: 'none',
      disableStdin: true,
      fontFamily,
      fontSize,
      lineHeight,
      scrollback: SNOOP_LINES,
      allowProposedApi: true,
      convertEol: false,
      scrollSensitivity: 0.75,
      theme: themeOf(getCurrentThemeId(), tintedRef.current),
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    const unicode11 = new Unicode11Addon();
    term.loadAddon(unicode11);
    term.unicode.activeVersion = '11';
    term.open(host);
    // Hide the cursor, since you never type here.
    term.write('\x1b[?25l');
    termRef.current = term;

    const wrapper = new WordWrapper(term.cols);
    const write = (text: string) => term.write(wrapper.process(text) + wrapper.flush());
    const fitNow = () => {
      if (!shownRef.current) return;
      try {
        fit.fit();
      } catch {
        // ignore a fit before layout settles
      }
    };
    fitRef.current = fitNow;
    fitNow();
    const resized = term.onResize(({ cols }) => wrapper.setCols(cols));
    const observer = new ResizeObserver(fitNow);
    observer.observe(host);

    write(snoopText(session, name));
    const unsubscribe = subscribeSnoopOutput((from, who, text, whole) => {
      if (from !== session || who !== name) return;
      if (whole) {
        term.reset();
        term.write('\x1b[?25l');
        wrapper.reset();
      }
      write(text);
    });

    // Cmd C or Ctrl C copies what you selected here, and the caret goes
    // back to the command line.
    const onCopyKey = (event: KeyboardEvent) => {
      if (!shownRef.current || event.altKey || !(event.metaKey || event.ctrlKey)) return;
      if (event.key.toLowerCase() !== 'c') return;
      const selection = term.getSelection();
      if (!selection) return;
      void navigator.clipboard.writeText(selection).catch(() => {});
      event.preventDefault();
      window.dispatchEvent(new Event('vosh:focus-input'));
    };
    window.addEventListener('keydown', onCopyKey, true);

    return () => {
      window.removeEventListener('keydown', onCopyKey, true);
      unsubscribe();
      observer.disconnect();
      resized.dispose();
      term.dispose();
      termRef.current = null;
      fitRef.current = () => {};
    };
    // Setup runs once for the tab. The face, the size and the colors
    // apply in the effects below.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // A tab that comes to the front fits the well as it is now.
  useEffect(() => {
    if (shown) fitRef.current();
  }, [shown]);

  useEffect(() => {
    const term = termRef.current;
    if (!term) return;
    term.options.fontFamily = fontFamily;
    term.options.fontSize = fontSize;
    term.options.lineHeight = lineHeight;
    fitRef.current();
    // xterm measured its cell on the faces that had loaded, so it
    // measures again once the face the list draws with has loaded.
    if (typeof document === 'undefined' || !document.fonts) return;
    return remeasureWhenLoaded(document.fonts, term, () => fitRef.current());
  }, [fontFamily, fontSize, lineHeight]);

  // The colors follow the theme, the tint, your base ANSI, Fit game
  // colors and your color vision, as your terminal's do.
  useEffect(() => {
    const paint = () => {
      const term = termRef.current;
      if (term) term.options.theme = themeOf(getCurrentThemeId(), tintedRef.current);
    };
    paint();
    const stops = [
      subscribeBaseAnsi(paint),
      subscribeFitGameColors(paint),
      subscribeColorVision(paint),
      onCustomThemesChanged(paint),
    ];
    return () => stops.forEach((stop) => stop());
  }, [themeTerminalColors]);

  useTauriEvent(subscribeThemeChanges, (themeId) => {
    const term = termRef.current;
    if (term) term.options.theme = themeOf(themeId, tintedRef.current);
  });

  return (
    <div
      ref={hostRef}
      className="snoop-term"
      hidden={!shown}
      role="log"
      aria-label={`${name}’s screen`}
    />
  );
}
