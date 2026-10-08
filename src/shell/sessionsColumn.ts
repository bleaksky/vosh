// How wide the sessions sidebar is, and when the window is too narrow
// to hold it. You drag its line from 180 to 320, 220 at first, and the
// column adds the 1 px line. The window keeps a terminal at least 320
// wide. As it narrows, the panel gives way first, down to its floor
// (AppShell clamps it), and once the sidebar, a 320 terminal and the
// panel at its floor no longer fit, the sidebar folds and the session
// popover lists the sessions instead. Widening the window brings it
// back.

/** The terminal keeps at least this much width beside the sidebar and
 *  the panel. */
export const MIN_TERMINAL_WIDTH = 320;
/** The sidebar's rows, without its 1 px line. */
export const SESSIONS_WIDTH_MIN = 180;
export const SESSIONS_WIDTH_MAX = 320;
export const SESSIONS_WIDTH_STOCK = 220;

/** The sidebar's column, its rows and its line. */
export function sessionsColumn(width: number): number {
  return width + 1;
}

/** Whether a sidebar `width` wide folds in a window `windowWidth` wide,
 *  beside a panel that needs `panel`: its floor while it is open, and 0
 *  while it is hidden. */
export function sessionsFold(windowWidth: number, width: number, panel: number): boolean {
  return sessionsColumn(width) + MIN_TERMINAL_WIDTH + panel > windowWidth;
}

/** The width a drag or a key gives the sidebar: 180 to 320, and never so
 *  wide that it would fold in a window `windowWidth` wide beside a panel
 *  that needs `panel`. */
export function clampSessionsWidth(px: number, windowWidth: number, panel: number): number {
  const room = windowWidth - sessionsColumn(0) - MIN_TERMINAL_WIDTH - panel;
  return Math.round(Math.max(SESSIONS_WIDTH_MIN, Math.min(SESSIONS_WIDTH_MAX, room, px)));
}
