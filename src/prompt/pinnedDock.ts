// The geometry of the pinned prompt band, measured on the prompt boards
// P4 to P10: the band reaches 4 px past the text on each side and 2 px
// above and below its rows, and its bottom sits 9.5 px above the input
// band. The dock above the command line is as tall as the rows the band
// shows now, plus a gap under the terminal's text of one blank line and
// 6 px, as the game leaves a blank line before each prompt. It takes one
// row and that gap of room under the terminal and borrows the rows past
// the first from the terminal's bottom (src/terminal/terminalRows.ts).

import { parseSgrCells, shownColumns, type Cell } from '../terminal/sgrCells';

/** The space the boards keep between the terminal's text and a band. */
export const DOCK_GAP = 6;
/** The band's outset above and below its rows. */
export const BAND_OUTSET_Y = 2;
/** The band's outset left and right of its text. */
export const BAND_OUTSET_X = 4;
/** How far the band's bottom sits above the dock's bottom, which is the
 *  terminal area's 6 px above the input band, so the band ends 9.5 px
 *  above the input band as the boards draw it. */
export const BAND_LIFT = 3.5;

/** The gap between the terminal's text and the band: one blank line,
 *  the one the game leaves before each prompt, and DOCK_GAP. It stays the
 *  same in a fight and out of one, so the prompt never sits right under
 *  the last line of text. */
export function dockGap(cellH: number): number {
  return cellH + DOCK_GAP;
}

/** The dock's height for `rows` rows `cellH` tall. */
export function dockHeight(rows: number, cellH: number): number {
  return dockGap(cellH) + BAND_OUTSET_Y + rows * cellH + BAND_OUTSET_Y + BAND_LIFT;
}

/** The rows the dock shows: the rows of the band, the last `zone` of
 *  them, and one while prompts are off, when the row holds the sentence
 *  that says so. None while nothing is pinned, before your first prompt
 *  and after you disconnect, so a login menu or a farewell sits right
 *  over the command line instead of over an empty band and its gap. */
export function dockRows(pin: string | null, zone: number, promptsOff: boolean): number {
  if (promptsOff) return 1;
  if (!pin) return 0;
  return Math.max(1, bandRows(pin, Math.max(1, zone)).length);
}

/** The rows the dock borrows from the bottom of the terminal while it
 *  shows `rows`: every row past the first. */
export function lentRows(rows: number): number {
  return Math.max(0, rows - 1);
}

/** The cell the terminal draws at, in CSS px, and its column count. */
export interface CellSize {
  width: number;
  height: number;
  /** The terminal's columns, the widest a band row can be. */
  cols: number;
}

/** What the band lays out: at most `zone` rows, the last ones, each cut
 *  to `cols` columns. */
export function bandRows(pin: string, zone: number): Cell[][] {
  return bandCut(pin, zone).rows;
}

/** The rows the band lays out, and which row of the pinned prompt is the
 *  first of them, so the pieces of the design are cut the same way: rows
 *  that show nothing at the end go, and of the rest only the last `zone`
 *  stay. */
export function bandCut(pin: string, zone: number): { rows: Cell[][]; first: number } {
  const rows = parseSgrCells(pin);
  while (rows.length > 0 && shownColumns(rows[rows.length - 1]) === 0) rows.pop();
  const first = Math.max(0, rows.length - zone);
  return { rows: rows.slice(first), first };
}

/** The top of the band's first row, from the dock's top, while it shows
 *  `rows` rows in a dock `zone` rows tall: the band's bottom sits
 *  BAND_LIFT above the dock's, and its rows BAND_OUTSET_Y inside it. */
export function bandRowsTop(zone: number, rows: number, cellH: number): number {
  return dockHeight(zone, cellH) - BAND_LIFT - BAND_OUTSET_Y - rows * cellH;
}
