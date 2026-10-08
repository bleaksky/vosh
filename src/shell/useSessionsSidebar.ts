import { useLayoutEffect, useState } from 'react';
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
// it, it stays hidden as sessions open and close, and Show sessions in
// the title band brings it back. The width
// you drag it to is kept in localStorage, as the split's height is,
// since it belongs to the install and to no profile.

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
  /** The sidebar shows. */
  shown: boolean;
  /** You keep it showing, which Show sessions in the palette and the
   *  View menu check, though a narrow window folds it. */
  wanted: boolean;
  /** Two or more sessions are open and the sidebar does not show them,
   *  so the session popover does. */
  folded: boolean;
  /** Two or more sessions are open and you hid the sidebar, so Show
   *  sessions sits in the title band. A session you open keeps it
   *  hidden. */
  hidden: boolean;
  /** Its rows' width, 180 to 320. */
  width: number;
  setWidth: (px: number) => void;
  /** Hide sessions, and Show sessions again. */
  hide: () => void;
  toggle: () => void;
}

export function useSessionsSidebar(count: number, panelOpen: boolean): SessionsSidebar {
  const [hidden, setHidden] = useState(false);
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

  const wanted = count >= 2 && !hidden;
  const shown = wanted && !narrow;
  return {
    shown,
    wanted,
    folded: count >= 2 && !shown,
    hidden: count >= 2 && hidden,
    width,
    setWidth: (px) => {
      setWidth(px);
      saveWidth(px);
    },
    hide: () => setHidden(true),
    toggle: () => setHidden((was) => !was),
  };
}
