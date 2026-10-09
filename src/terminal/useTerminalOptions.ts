import { useEffect, useLayoutEffect, type MutableRefObject } from 'react';
import type { Terminal as XTerm } from '@xterm/xterm';

import { subscribeBaseAnsi } from '../theme/baseAnsi';
import { nativeSurfaceSetFont } from '../ipc/nativeSurface';
import { useTauriEvent } from '../ipc/useTauriEvent';
import { onCustomThemesChanged } from '../theme/themes';
import { subscribeColorVision, subscribeFitGameColors } from '../theme/fitGameColors';
import { reportTheme } from './paneTheme';
import { getCurrentThemeId, subscribeThemeChanges } from '../theme/theme';
import { remeasureWhenLoaded } from './terminalFont';
import { nativeSurfaceEnabled } from './terminalRenderer';
import type { XtermBlink } from './xterm/xtermBlink';
import type { PaneSizer } from './paneSizer';

/** What the live option effects of a Terminal read: the props they
 *  follow, the refs the setup effect fills and the pane's theme and lift
 *  steps. */
interface TerminalOptions {
  shown: boolean;
  lentRows: number;
  anchorBottom: boolean;
  blinkText: boolean;
  fontFamily: string;
  fontSize: number;
  scrollback: number;
  lineHeight: number;
  themeTerminalColors: boolean;
  lifted: boolean;
  termRef: MutableRefObject<XTerm | null>;
  paneSizerRef: MutableRefObject<PaneSizer | null>;
  blinkRef: MutableRefObject<XtermBlink | null>;
  showingRef: MutableRefObject<{ show(): void; hide(): void } | null>;
  appliedShownRef: MutableRefObject<boolean>;
  lentRef: MutableRefObject<number>;
  anchorRef: MutableRefObject<boolean>;
  themeTerminalColorsRef: MutableRefObject<boolean>;
  quietRef: MutableRefObject<boolean>;
  liftsHere: () => boolean;
  applyTheme: (term: XTerm, themeId?: string) => void;
  applyLift: (term: XTerm) => void;
}

/** The effects that apply a Terminal's options live, without rebuilding
 *  the terminal. Called right after the setup effect, so React runs them
 *  after it, in this order. */
export function useTerminalOptions({
  shown,
  lentRows,
  anchorBottom,
  blinkText,
  fontFamily,
  fontSize,
  scrollback,
  lineHeight,
  themeTerminalColors,
  lifted,
  termRef,
  paneSizerRef,
  blinkRef,
  showingRef,
  appliedShownRef,
  lentRef,
  anchorRef,
  themeTerminalColorsRef,
  quietRef,
  liftsHere,
  applyTheme,
  applyLift,
}: TerminalOptions): void {
  // The pane shows or hides as a selection moves, before the page
  // paints, so it never shows a frame at the size it had. The setup
  // effect applied the state the pane mounted with.
  useLayoutEffect(() => {
    if (appliedShownRef.current === shown) return;
    appliedShownRef.current = shown;
    if (shown) showingRef.current?.show();
    else showingRef.current?.hide();
  }, [shown, showingRef, appliedShownRef]);

  // A new count of rows lent to the pinned band applies before the page
  // paints, in the same commit that grows or shrinks the band, so the
  // newest line moves with the band's top and never sits under it. So
  // does the grid starting or stopping keeping to the bottom.
  useLayoutEffect(() => {
    if (lentRef.current === lentRows && anchorRef.current === anchorBottom) return;
    lentRef.current = lentRows;
    anchorRef.current = anchorBottom;
    paneSizerRef.current?.relayout();
  }, [lentRows, anchorBottom, lentRef, anchorRef, paneSizerRef]);

  // Blinking text turns on or off live.
  useEffect(() => {
    blinkRef.current?.setOn(blinkText);
  }, [blinkText, blinkRef]);

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
  }, [fontFamily, fontSize, termRef, paneSizerRef]);

  // Scrollback size changes without rebuilding the terminal. A smaller
  // size drops the oldest lines.
  useEffect(() => {
    const term = termRef.current;
    if (term && term.options.scrollback !== scrollback) term.options.scrollback = scrollback;
  }, [scrollback, termRef]);

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
  }, [lineHeight, termRef, paneSizerRef, quietRef]);

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
}
