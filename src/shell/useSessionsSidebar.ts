import { useEffect, useLayoutEffect, useState } from 'react';
import { useEscape } from '../lib/escapeStack';
import { isMacPlatform } from '../lib/shortcuts';
import { panelWidthFloor } from '../panel/paneLayout';
import {
  SESSIONS_WIDTH_MAX,
  SESSIONS_WIDTH_MIN,
  SESSIONS_WIDTH_STOCK,
  sessionsFold,
} from './sessionsColumn';

// Whether the main window shows the sessions sidebar, and how wide. It
// shows while two or more sessions are open (Q17), unless you hid it in
// this window or the window is too narrow to hold it (board 8). Either
// way it folds and the session popover lists the sessions. Once you hide
// it, it stays hidden as sessions open and close. One toggle in the
// window's top left corner hides it and shows it again, as do Ctrl Cmd
// S, the View menu and the palette (Sessions toggle T1 to T3). In a
// window too narrow for it, the toggle slides it over the terminal
// instead, until you pick a row, press the toggle again or press Esc
// (T5), and widening the window or dropping to one session puts it
// away. The caret goes back to the command line whenever the sidebar
// over the terminal goes, and when a toggle press would leave it on the
// toggle or on a sidebar that hides. The width you drag it to is kept in localStorage, as the
// split's height is, since it belongs to the install and to no profile.

const WIDTH_KEY = 'vosh.layout.sessionsWidth';

/** The width you left the sidebar at, or the stock width. */
function loadWidth(): number {
  try {
    const saved = Number(localStorage.getItem(WIDTH_KEY) ?? NaN);
    if (!Number.isFinite(saved)) return SESSIONS_WIDTH_STOCK;
    return Math.max(SESSIONS_WIDTH_MIN, Math.min(SESSIONS_WIDTH_MAX, Math.round(saved)));
  } catch {
    return SESSIONS_WIDTH_STOCK;
  }
}

function saveWidth(px: number): void {
  try {
    localStorage.setItem(WIDTH_KEY, String(px));
  } catch {
    // The width still holds until the window closes.
  }
}

export interface SessionsSidebar {
  /** The sidebar shows in its column. */
  shown: boolean;
  /** The sidebar slides over the terminal in a window too narrow for
   *  its column. */
  overlay: boolean;
  /** The sidebar shows, in its column or over the terminal, so the
   *  toggle reads pressed and Hide sessions is what it offers. */
  pressed: boolean;
  /** Two or more sessions are open, so the toggle shows. */
  toggleable: boolean;
  /** Two or more sessions are open and the sidebar does not show them
   *  in its column, so the session popover does. */
  folded: boolean;
  /** Its rows' width, 180 to 320. */
  width: number;
  setWidth: (px: number) => void;
  /** Hide the sidebar, or show it again. In a narrow window, slide it
   *  over the terminal or put it away. */
  toggle: () => void;
  /** Put the sidebar over the terminal away, as picking a row does,
   *  and leave the caret where it is. */
  closeOverlay: () => void;
}

/** Where the caret sits on the toggle or in the sidebar, which a press
 *  that hides the sidebar would strand. */
const STRANDED = '.shell-lead, .shell-slot-sessions, .shell-sessions-overlay';

function caretStranded(): boolean {
  const active = typeof document === 'undefined' ? null : document.activeElement;
  return active instanceof Element && active.closest(STRANDED) !== null;
}

/** The sidebar for `count` sessions beside a panel open or hidden.
 *  `focusInput` puts the caret back on the command line. */
export function useSessionsSidebar(
  count: number,
  panelOpen: boolean,
  focusInput: () => void = () => {},
): SessionsSidebar {
  const [hidden, setHidden] = useState(false);
  const [overlayOpen, setOverlayOpen] = useState(false);
  const [width, setWidth] = useState(loadWidth);
  // Read on each resize, so a window drag renders only when the sidebar
  // folds or comes back.
  const [narrow, setNarrow] = useState(false);
  const panel = panelOpen ? panelWidthFloor(isMacPlatform()) : 0;
  useLayoutEffect(() => {
    const check = () => setNarrow(sessionsFold(window.innerWidth, width, panel));
    check();
    window.addEventListener('resize', check);
    return () => window.removeEventListener('resize', check);
  }, [width, panel]);

  const toggleable = count >= 2;
  // The overlay lives only in a narrow window with two or more sessions.
  const overlay = overlayOpen && narrow && toggleable;
  useEffect(() => {
    if (overlayOpen && !(narrow && toggleable)) setOverlayOpen(false);
  }, [overlayOpen, narrow, toggleable]);

  // Esc puts the sidebar over the terminal away, as the topmost surface.
  useEscape(overlay, () => {
    setOverlayOpen(false);
    focusInput();
  });

  const shown = toggleable && !hidden && !narrow;
  return {
    shown,
    overlay,
    pressed: shown || overlay,
    toggleable,
    folded: toggleable && !shown,
    width,
    setWidth: (px) => {
      setWidth(px);
      saveWidth(px);
    },
    toggle: () => {
      const stranded = caretStranded();
      if (narrow) setOverlayOpen((was) => !was);
      else setHidden((was) => !was);
      if (stranded || overlay) focusInput();
    },
    closeOverlay: () => setOverlayOpen(false),
  };
}
