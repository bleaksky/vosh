import { isTrackedRow, type AffectRow } from '../../lib/affectsView';
import { PANE_ROW_PX } from '../../lib/paneLayout';

// Where each affect sits in the Affects pane (board Affects A, timers
// first). Two columns of 22 px rows: your tracked affects fill the top
// rows in your order, row by row, so each keeps its slot. The rest sit
// under a hairline and fill down the left column, then down the right,
// so the hours rise down each column.
//
// The pane shows only whole rows. When the rest do not fit, the last
// cell counts the ones that do not, the ones that last longest, and
// the ones past it wait on the next page. A page is as many rows as
// fit under the tracked slots, so scrolling one page at a time always
// stops on whole rows. Pure so the fit is unit tested.

/** The hairline between your tracked slots and the rest, with 4 px
 *  above and below. */
export const AFFECTS_RULE_PX = 9;

/** Narrowest pane that draws two columns. Each column then keeps room
 *  for the hours and a name of about 16 characters in the terminal
 *  face at 12 px. A narrower pane draws one column. */
export const AFFECTS_TWO_COLUMNS_W = 360;

export function affectsColumns(width: number): number {
  return width >= AFFECTS_TWO_COLUMNS_W ? 2 : 1;
}

interface CellPlace {
  /** 1 based grid line of the row, counting every page. */
  gridRow: number;
  gridColumn: number;
  /** The first cell of a page, where a scroll stops. */
  pageStart: boolean;
}

export type AffectsCell =
  | (CellPlace & { kind: 'affect'; row: AffectRow })
  /** The last cell of a page that does not hold everything left:
   *  how many come after it. A click scrolls to the next page. */
  | (CellPlace & { kind: 'more'; count: number; page: number });

export interface AffectsGrid {
  columns: number;
  /** Your tracked affects in your order, filling row by row. */
  tracked: AffectRow[];
  /** A hairline between the tracked slots and the rest. */
  rule: boolean;
  /** The rest, placed page after page. */
  rest: AffectsCell[];
  /** Rows of the rest on one page, the height of their window. 0 when
   *  nothing else affects you. */
  pageRows: number;
  pages: number;
}

/** Place `rows`, in the order affectsView gives them, in a pane body
 *  of `box`. Before the body is measured (`null`) every affect goes on
 *  one page. */
export function affectsGrid(
  rows: readonly AffectRow[],
  box: { width: number; height: number } | null,
): AffectsGrid {
  const columns = box ? affectsColumns(box.width) : 2;
  const tracked = rows.filter(isTrackedRow);
  const others = rows.filter((r) => !isTrackedRow(r));
  const rule = tracked.length > 0 && others.length > 0;
  if (others.length === 0) return { columns, tracked, rule, rest: [], pageRows: 0, pages: 0 };

  const allRows = Math.ceil(others.length / columns);
  const trackedPx = Math.ceil(tracked.length / columns) * PANE_ROW_PX;
  const fit = box
    ? Math.floor((box.height - trackedPx - (rule ? AFFECTS_RULE_PX : 0)) / PANE_ROW_PX)
    : allRows;
  // A page holds at least two cells while two or more affects wait, so
  // the count always has an affect beside it. In a pane too short even
  // for that, the body scrolls.
  const leastRows = others.length > 1 ? Math.ceil(2 / columns) : 1;
  const pageRows = Math.max(leastRows, Math.min(allRows, fit));
  const slots = pageRows * columns;

  const rest: AffectsCell[] = [];
  let next = 0;
  let page = 0;
  while (next < others.length) {
    const left = others.length - next;
    // A page that cannot hold everything left gives its last cell to
    // the count, unless the page is a single cell.
    const counts = left > slots && slots > 1;
    const take = counts ? slots - 1 : Math.min(left, slots);
    const place = (i: number): CellPlace => ({
      gridRow: page * pageRows + (i % pageRows) + 1,
      gridColumn: Math.floor(i / pageRows) + 1,
      pageStart: i === 0,
    });
    for (let i = 0; i < take; i += 1) {
      rest.push({ kind: 'affect', row: others[next + i], ...place(i) });
    }
    next += take;
    if (counts) rest.push({ kind: 'more', count: others.length - next, page, ...place(slots - 1) });
    page += 1;
  }
  return { columns, tracked, rule, rest, pageRows, pages: page };
}

/** Rows of the rest the pane keeps in view at its least: every
 *  harmful affect, which sort first, or at least one affect, and the
 *  count after them when more follow. */
export function affectsRestMinRows(rows: readonly AffectRow[], columns: number): number {
  const others = rows.filter((r) => !isTrackedRow(r)).length;
  if (others === 0) return 0;
  const harmful = rows.filter((r) => r.state === 'harmful').length;
  const cells = Math.max(harmful, 1) + 1;
  return Math.min(Math.ceil(others / columns), Math.ceil(cells / columns));
}

/** What holdsPage reads of the window the rest scroll in. */
export interface PageWindow {
  matches(selector: string): boolean;
  querySelector(selector: string): unknown;
}

/** True while the rest keep the page you scrolled them to: while you
 *  point at them, or while you tab through the counts. A click on a
 *  count leaves focus on it in Chromium, as on Windows, but no focus
 *  ring, so pointing away after a click scrolls back to the first
 *  page there too. */
export function holdsPage(el: PageWindow): boolean {
  return el.matches(':hover') || el.querySelector(':focus-visible') !== null;
}
