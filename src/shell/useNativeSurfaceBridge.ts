// What the main window tells the macOS native surface under it, and what
// it hears back. Each effect does nothing while xterm draws the terminal.

import { useEffect, useRef } from 'react';
import {
  nativeSurfaceReady,
  nativeSurfaceSetBlinkText,
  nativeSurfaceSetPromptBands,
  nativeSurfaceSetPromptReach,
  nativeSurfaceSetTokens,
  onTerminalClicked,
  onTerminalCursor,
} from '../ipc/nativeSurface';
import { useTauriEvent } from '../ipc/useTauriEvent';
import { getColorVision } from '../theme/fitGameColors';
import { getCurrentThemeId } from '../theme/theme';
import { findTheme, themeTokens } from '../theme/themes';
import { parseHex, toRgba } from '../theme/color';
import { usePromptReach } from '../stores/session/promptReachStore';
import { NATIVE_FAILED_KEY, nativeSurfaceEnabled } from '../terminal/terminalRenderer';

// Hand the native surface the chrome colors the page derives with its
// theme tokens: the split divider, the selection and its text, find
// matches in ANSI yellow (28% for every match, so the text under them
// still reads, stronger for the current one), links in the accent, and the scrollbar
// in the tertiary tone. A lifted prompt's band takes the selected row
// fill, with its inset ring on a light theme. Runs on every theme apply,
// so light themes never get the renderer's dark defaults.
function pushNativeChromeTokens(): void {
  const theme = findTheme(getCurrentThemeId());
  const tokens = themeTokens(theme, getColorVision());
  const yellow = parseHex(theme.xterm.yellow);
  void nativeSurfaceSetTokens({
    divider: tokens.sep,
    selection: tokens.selection,
    selectionText: tokens.selectionText,
    findMatch: yellow ? toRgba(yellow, 0.28) : null,
    currentMatch: yellow ? toRgba(yellow, 0.6) : null,
    link: tokens.accent,
    scrollbar: tokens.tertiary,
    selrow: tokens.selrow,
    appearance: tokens.appearance,
  }).catch(() => {});
}

interface BridgeInputs {
  /** The selected session, whose grid shows. */
  session: number;
  /** Whether blinking text blinks now. */
  blinkText: boolean;
  /** Your prompt shows lifted. */
  promptLifted: boolean;
  /** The prompt card draws your design in the text. */
  cardBand: boolean;
  /** Puts the caret on the command line. The hook keeps the one it gets
   *  at mount. */
  focusInput: () => void;
}

export function useNativeSurfaceBridge({
  session,
  blinkText,
  promptLifted,
  cardBand,
  focusInput,
}: BridgeInputs): void {
  // The native grid blinks while Blinking text is on and draws steady
  // while it is off, from the next frame.
  useEffect(() => {
    if (!nativeSurfaceEnabled()) return;
    void nativeSurfaceSetBlinkText(blinkText).catch(() => {});
  }, [blinkText]);

  // The native surface sits under the page (macOS), which draws over it.
  // Mark the root so CSS leaves the terminal pane unpainted and hides the
  // xterm copy.
  useEffect(() => {
    if (!nativeSurfaceEnabled()) return;
    // Leave the pane transparent only once the backend confirms the
    // surface is up. It installs during setup, usually before this runs,
    // so poll briefly. If it never comes up, reload onto xterm for the
    // rest of the session instead of showing a see-through hole.
    let cancelled = false;
    let tries = 0;
    const check = () => {
      void nativeSurfaceReady()
        .catch(() => false)
        .then((ready) => {
          if (cancelled) return;
          if (ready) {
            document.documentElement.dataset.underlay = '1';
            return;
          }
          tries += 1;
          if (tries < 30) {
            window.setTimeout(check, 100);
            return;
          }
          try {
            sessionStorage.setItem(NATIVE_FAILED_KEY, '1');
          } catch {
            return;
          }
          window.location.reload();
        });
    };
    check();
    return () => {
      cancelled = true;
      delete document.documentElement.dataset.underlay;
    };
  }, []);

  // The native grid draws a band under each lifted prompt while your
  // prompt shows lifted, and under your design while the prompt card
  // draws it in the text. xterm keeps its own ground while the card is
  // open, so In the text stays as it is there and the card's marks
  // still show. Each session's grid keeps its own, and the grid a
  // selection brings to the front hears it. The card goes with the
  // selection, so the grid of the session left keeps only the bands of
  // its lifted prompts.
  const bandsTold = useRef<number | null>(null);
  useEffect(() => {
    if (!nativeSurfaceEnabled()) return;
    const left = bandsTold.current;
    if (left !== null && left !== session) {
      void nativeSurfaceSetPromptBands(promptLifted === true, left).catch(() => {});
    }
    bandsTold.current = session;
    void nativeSurfaceSetPromptBands(promptLifted === true || cardBand, session).catch(() => {});
  }, [promptLifted, cardBand, session]);

  // The band under the open row reaches past its last glyph for the
  // prompt card's line break mark and caret.
  const promptReach = usePromptReach();
  useEffect(() => {
    if (!nativeSurfaceEnabled()) return;
    void nativeSurfaceSetPromptReach(promptReach).catch(() => {});
  }, [promptReach]);

  // Keep the native surface's chrome colors on the theme. Every theme
  // apply, from this window, a broadcast, a profile switch, or a new
  // color vision, writes data-theme on the root, so one observer catches
  // them all.
  useEffect(() => {
    if (!nativeSurfaceEnabled()) return;
    pushNativeChromeTokens();
    const observer = new MutationObserver(pushNativeChromeTokens);
    observer.observe(document.documentElement, { attributeFilter: ['data-theme'] });
    return () => observer.disconnect();
  }, []);

  // Under the underlay the DOM owns the pointer over the terminal, but
  // only the native surface knows what sits under it: the split divider
  // wants a resize cursor and an armed link wants a hand. It reports the
  // cursor on each change and the sizer takes it through a variable.
  useEffect(() => {
    if (!nativeSurfaceEnabled()) return;
    const root = document.documentElement;
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void onTerminalCursor((cursor) => {
      if (cursor === 'row-resize' || cursor === 'pointer') {
        root.style.setProperty('--terminal-cursor', cursor);
      } else {
        root.style.removeProperty('--terminal-cursor');
      }
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
      root.style.removeProperty('--terminal-cursor');
    };
  }, []);

  // The page cancels each press it forwards to the native surface, so no
  // DOM mouseup follows. The backend emits an event on release instead,
  // and the input focuses here, matching the DOM mouseup handler that
  // covers the rest of the window.
  useTauriEvent(
    (cb) => (nativeSurfaceEnabled() ? onTerminalClicked(cb) : Promise.resolve(() => {})),
    () => focusInput(),
  );
}
