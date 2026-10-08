// Where you put the writing card and how big you make its box. You
// drag the card by its header anywhere in the window, its box taller by
// the grip on its free edge, and the box taller and wider by the grip at
// its corner. The profile's UI config keeps them (writing_card_left,
// writing_card_top, writing_card_rows and writing_card_cols). The card
// stays whole on screen however the window resizes, and the saved place
// stays as you left it, so a window that grows back puts the card back
// there too.

/** The fewest rows the box shows. Mirrors WRITING_CARD_ROWS_MIN in
 *  src-tauri/src/profile/ui.rs. */
export const BOX_ROWS_MIN = 6;
/** The most rows the config keeps. Mirrors WRITING_CARD_ROWS_MAX. */
export const BOX_ROWS_MAX = 500;
/** The columns of text the box shows until you widen it. */
export const BOX_COLS = 80;
/** The fewest columns, so the 75 column guide always shows. Mirrors
 *  WRITING_CARD_COLS_MIN in src-tauri/src/profile/ui.rs. */
export const BOX_COLS_MIN = 75;
/** The most columns the config keeps. Mirrors WRITING_CARD_COLS_MAX. */
export const BOX_COLS_MAX = 500;
/** What the box adds to its columns: the gutter's 22 px and two digits,
 *  and 10 px after the text. */
export const BOX_EDGE_PX = 32;
export const BOX_EDGE_COLS = 2;
/** How close a moved card comes to the window's edges. */
export const CARD_MARGIN = 8;
/** How far the pointer goes before a press on the header moves the
 *  card, so a click stays a click. */
export const DRAG_SLOP = 3;

export interface Point {
  left: number;
  top: number;
}

export interface Size {
  w: number;
  h: number;
}

/** The place you moved the card to, or null for the place over the
 *  terminal useWritingPlace works out. */
export function savedPlace(left: number | null, top: number | null): Point | null {
  return left === null || top === null ? null : { left, top };
}

/** Keep a card of `size` with its top left corner at `at` whole inside
 *  a window of `view`, `margin` in from each edge. A card wider or
 *  taller than the window keeps its left or top edge at the margin. */
export function clampPlace(at: Point, size: Size, view: Size, margin = CARD_MARGIN): Point {
  const hold = (v: number, room: number) =>
    Math.round(Math.max(margin, Math.min(v, room - margin)));
  return { left: hold(at.left, view.w - size.w), top: hold(at.top, view.h - size.h) };
}

/** Whether a moved card of width `wide` fits the window at its own
 *  width. A window narrower than that spans the card across it, as the
 *  card always did. */
export function fitsMoved(wide: number, viewW: number, margin = 12): boolean {
  return wide + 2 * margin <= viewW;
}

/** The whole rows of `lineH` that fit in `height` once the card's own
 *  `chrome` (header, rules, padding and footer) is taken out. */
export function fitRows(height: number, chrome: number, lineH: number): number {
  if (lineH <= 0) return BOX_ROWS_MIN;
  return Math.floor((height - chrome) / lineH);
}

/** The rows a moved card's box can reach: as many as the window holds
 *  between its margins. */
export function movedFit(viewH: number, chrome: number, lineH: number): number {
  return fitRows(viewH - 2 * CARD_MARGIN, chrome, lineH);
}

/** The rows the box shows. With rows you set (`saved`) it shows those,
 *  held to 6 and to what fits. Without, it grows with the text from 6
 *  rows up to what fits. Never fewer than 6, so a short window scrolls
 *  the card rather than the text. */
export function boxRowsFor(saved: number | null, lines: number, fit: number): number {
  const room = Math.max(BOX_ROWS_MIN, fit);
  if (saved !== null) return Math.max(BOX_ROWS_MIN, Math.min(saved, room));
  return Math.max(BOX_ROWS_MIN, Math.min(Math.max(lines, BOX_ROWS_MIN), room));
}

/** The rows a drag of the grip gives: the rows at the press, plus one
 *  for each row of pointer travel away from the card, held to 6 and to
 *  `fit`. `grows` is +1 for the grip on the foot, which grows as you
 *  pull down, and -1 for the one on the top edge, which grows as you
 *  pull up. */
export function dragRows(
  start: number,
  travel: number,
  lineH: number,
  fit: number,
  grows: 1 | -1 = 1,
): number {
  const step = lineH > 0 ? Math.round((grows * travel) / lineH) : 0;
  const room = Math.min(BOX_ROWS_MAX, Math.max(BOX_ROWS_MIN, fit));
  return Math.max(BOX_ROWS_MIN, Math.min(start + step, room));
}

/** Whether a press on `target` in the card's header may move the card:
 *  anywhere but a button, a field, a menu, or the switch. */
export function startsMove(target: EventTarget | null): boolean {
  const el = target as Partial<Pick<Element, 'closest'>> | null;
  if (typeof el?.closest !== 'function') return false;
  return el.closest(KEEPS_ITS_PRESS) === null;
}

/** What in the header keeps a press for itself. */
export const KEEPS_ITS_PRESS =
  'button, input, select, textarea, a, [role="menu"], [role="radiogroup"]';

/** The width of a box of `cols` columns of `colW` px. */
export function boxWidthFor(cols: number, colW: number): number {
  return BOX_EDGE_PX + (cols + BOX_EDGE_COLS) * colW;
}

/** The whole columns of `colW` a box fits in `room` px. */
export function fitCols(room: number, colW: number): number {
  if (colW <= 0) return BOX_COLS;
  return Math.floor((room - BOX_EDGE_PX) / colW) - BOX_EDGE_COLS;
}

/** The columns the box shows: 80 until you widen or narrow it, and the
 *  columns you set held to 75 and to what fits. */
export function boxColsFor(saved: number | null, fit: number): number {
  if (saved === null) return BOX_COLS;
  return Math.max(BOX_COLS_MIN, Math.min(saved, Math.max(BOX_COLS_MIN, fit)));
}

/** The columns a drag of the corner grip gives: the columns at the
 *  press, plus one for each column of pointer travel to the right, held
 *  to 75 and to `fit`. */
export function dragCols(start: number, travel: number, colW: number, fit: number): number {
  const step = colW > 0 ? Math.round(travel / colW) : 0;
  const room = Math.min(BOX_COLS_MAX, Math.max(BOX_COLS_MIN, fit));
  return Math.max(BOX_COLS_MIN, Math.min(start + step, room));
}
