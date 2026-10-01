// The geometry of the pinned prompt band, measured on the prompt boards
// P4 to P10: the band reaches 4 px past the text on each side and 2 px
// above and below its rows, and its bottom sits 9.5 px above the input
// band. The dock above the command line keeps room for the most rows the
// capture's prompts can take, plus a 6 px gap under the terminal's text.

import { parseSgrCells, shownColumns, type Cell } from './sgrCells';

/** The gap between the terminal's text and the band's rows. */
export const DOCK_GAP = 6;
/** The band's outset above and below its rows. */
export const BAND_OUTSET_Y = 2;
/** The band's outset left and right of its text. */
export const BAND_OUTSET_X = 4;
/** How far the band's bottom sits above the dock's bottom, which is the
 *  terminal area's 6 px above the input band, so the band ends 9.5 px
 *  above the input band as the boards draw it. */
export const BAND_LIFT = 3.5;

/** The dock's height for `zone` rows `cellH` tall. */
export function dockHeight(zone: number, cellH: number): number {
  return DOCK_GAP + BAND_OUTSET_Y + zone * cellH + BAND_OUTSET_Y + BAND_LIFT;
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
