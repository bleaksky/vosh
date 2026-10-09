// How tall the snoop split stands in the terminal column. It opens at
// the profile's share of the column, in whole rows of the snoop
// terminal. It never goes under its strip and four rows, and it always
// leaves your terminal six. Drag its line to the top and it folds to
// the strip alone. When the column cannot hold both, the snoop keeps
// its four rows and you can fold it.

/** The strip, var(--band). */
export const SNOOP_STRIP = 32;
/** The strip and the 1 px line under the split: all a folded split
 *  shows. */
export const SNOOP_FOLDED = SNOOP_STRIP + 1;
/** What the split holds besides its rows: the strip, the line and the
 *  well's 6 px foot. */
export const SNOOP_CHROME = SNOOP_FOLDED + 6;
/** The rows the split keeps. */
export const SNOOP_MIN_ROWS = 4;
/** The rows the split leaves your terminal. */
export const YOUR_MIN_ROWS = 6;
/** Your terminal's insets above and below its rows. */
export const YOUR_INSETS = 12;

/** The terminal column's height and the snoop terminal's row height, in
 *  CSS px. */
export interface SnoopRoom {
  column: number;
  row: number;
}

/** The height of a split that shows `rows` rows `row` px tall. */
export function heightForRows(rows: number, row: number): number {
  return SNOOP_CHROME + Math.ceil(rows * row - 1e-6);
}

/** The fewest and most rows the split may show in `room`. */
export function rowLimits({ column, row }: SnoopRoom): { min: number; max: number } {
  const spare = column - YOUR_INSETS - YOUR_MIN_ROWS * row - SNOOP_CHROME;
  const max = Math.floor(spare / row + 1e-6);
  return { min: SNOOP_MIN_ROWS, max: Math.max(SNOOP_MIN_ROWS, max) };
}

/** The whole rows nearest a split `height` px tall, held to the
 *  limits. */
function rowsNear(height: number, room: SnoopRoom): number {
  const { min, max } = rowLimits(room);
  const rows = Math.round((height - SNOOP_CHROME) / room.row);
  return Math.min(max, Math.max(min, rows));
}

/** The height of an open split with the saved `share` of the column. */
export function snoopHeight(share: number, room: SnoopRoom): number {
  return heightForRows(rowsNear(share * room.column, room), room.row);
}

/** The height under which a drag folds the split: halfway from the
 *  strip to the fewest rows. */
export function foldBelow(room: SnoopRoom): number {
  return (SNOOP_FOLDED + heightForRows(SNOOP_MIN_ROWS, room.row)) / 2;
}

/** Where a drag of the line to `height` px leaves the split: folded, or
 *  open at whole rows with the share of the column to save. */
export type SnoopDrag = { folded: true } | { folded: false; height: number; share: number };

export function dragTo(height: number, room: SnoopRoom): SnoopDrag {
  if (height < foldBelow(room)) return { folded: true };
  const snapped = heightForRows(rowsNear(height, room), room.row);
  return { folded: false, height: snapped, share: snapped / room.column };
}
