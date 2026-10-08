import { isTrackedRow, type AffectRow } from './affectsView';
import { affectsColumns, pageCells, type AffectsCell } from './affectsGrid';
import { PANE_TEXT_BASE, PANE_TEXT_PX, paneText } from '../paneTextSize';

// Where each affect sits in the Countdown style. One
// run sorted by the hours left: the tracked affects you are missing
// first, in your order, then every other affect, fewest hours first,
// permanent after every timed one and unknown last. It fills down the
// left column, then down the right, on 23 px rows, two columns from
// 360 px, one below, at 12 px. The rows and the width follow your
// panel size (paneTextSize.ts). What does not fit is the end of the
// countdown, counted in the last cell of the page, and a click on the
// count scrolls one page on. Pure so the fit is unit tested.

/** One countdown row at 12 px: the text line and the 2 px meter under
 *  it. */
export const COUNTDOWN_ROW_PX = PANE_TEXT_BASE.countdownRow;

/** One countdown row at text `size` px. */
export function countdownRowPx(size: number = PANE_TEXT_PX): number {
  return paneText(size).countdownRow;
}

/** A row's place in the countdown by its hours. */
function rank(ticks: number | null): number {
  if (ticks === null) return Number.MAX_SAFE_INTEGER;
  if (ticks < 0) return Number.MAX_SAFE_INTEGER - 1;
  return ticks;
}

/** Board B's one run. Tracked affects you miss first, in your order,
 *  then every other row by hours left, permanent after timed, unknown
 *  last. At equal hours your tracked affect comes before the rest,
 *  then by name. Harmful rows sort in by their hours. */
export function countdownOrder(rows: readonly AffectRow[]): AffectRow[] {
  const missing = rows.filter((r) => r.state === 'missing');
  const rest = rows
    .filter((r) => r.state !== 'missing')
    .sort(
      (a, b) =>
        rank(a.ticks) - rank(b.ticks) ||
        Number(!isTrackedRow(a)) - Number(!isTrackedRow(b)) ||
        a.key.localeCompare(b.key),
    );
  return [...missing, ...rest];
}

export interface CountdownGrid {
  columns: number;
  /** Rows on one page, the height of the window. */
  pageRows: number;
  pages: number;
  cells: AffectsCell[];
}

/** Place `rows`, in the order affectsView gives them, in a pane body of
 *  `box`, at panel size `size` px. A short list balances
 *  across both columns (fifteen make eight over seven), a long one
 *  fills what fits, and a page holds at least two cells while two or
 *  more affects wait. Before the body is measured (`null`) every affect
 *  goes on one page. */
export function countdownGrid(
  rows: readonly AffectRow[],
  box: { width: number; height: number } | null,
  size: number = PANE_TEXT_PX,
): CountdownGrid {
  const columns = box ? affectsColumns(box.width, size) : 2;
  const ordered = countdownOrder(rows);
  const all = Math.max(1, Math.ceil(ordered.length / columns));
  const fit = box ? Math.floor(box.height / countdownRowPx(size)) : all;
  const least = ordered.length > 1 ? Math.ceil(2 / columns) : 1;
  const pageRows = Math.max(least, Math.min(all, fit));
  const { cells, pages } = pageCells(ordered, columns, pageRows);
  return { columns, pageRows, pages, cells };
}

/** Rows the Countdown keeps in view at its least: down to the last
 *  affect that asks something of you (missing, running out, harmful),
 *  and the count after it when more follow. None when nothing does. */
export function countdownMinRows(rows: readonly AffectRow[], columns: number): number {
  const ordered = countdownOrder(rows);
  let last = -1;
  ordered.forEach((r, i) => {
    if (r.state === 'missing' || r.state === 'expiring' || r.state === 'harmful') last = i;
  });
  if (last < 0) return 0;
  const cells = last + 1 + (ordered.length > last + 1 ? 1 : 0);
  return Math.min(Math.ceil(ordered.length / columns), Math.ceil(cells / columns));
}
