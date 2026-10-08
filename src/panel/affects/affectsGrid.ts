import { isTrackedRow, type AffectRow } from './affectsView';
import { PANE_TEXT_BASE, PANE_TEXT_PX, paneText } from '../paneTextSize';

// Where each affect sits in the Affects pane, timers first. Two columns of 22 px rows at 12 px: your tracked affects
// fill the top rows in your order, row by row, so each keeps its slot.
// The rest sit under a hairline and fill down the left column, then
// down the right, so the hours rise down each column.
//
// The pane shows only whole rows. When the rest do not fit, the last
// cell counts the ones that do not, the ones that last longest, and
// the ones past it wait on the next page. A page is as many rows as
// fit under the tracked slots, so scrolling one page at a time always
// stops on whole rows. Pure so the fit is unit tested.
//
// The rows, the gaps round the hairline, and the width for two columns
// follow your panel size (paneTextSize.ts). The numbers here are at
// 12 px.

/** The hairline between your tracked slots and the rest, with 4 px
 *  above and below at 12 px. */
export const AFFECTS_RULE_PX = 1 + 2 * PANE_TEXT_BASE.affectsRuleGap;

/** The hairline and its gaps at text `size` px. */
export function affectsRulePx(size: number = PANE_TEXT_PX): number {
  return 1 + 2 * paneText(size).affectsRuleGap;
}

/** Narrowest pane that draws two columns at 12 px. Each column then
 *  keeps room for the hours and a name of about 16 characters in a
 *  monospace game face. A narrower pane draws one column. */
export const AFFECTS_TWO_COLUMNS_W = PANE_TEXT_BASE.twoColumns;

/** Narrowest pane that draws two columns at text `size` px. */
export function affectsTwoColumnsW(size: number = PANE_TEXT_PX): number {
  return paneText(size).twoColumns;
}

export function affectsColumns(width: number, size: number = PANE_TEXT_PX): number {
  return width >= affectsTwoColumnsW(size) ? 2 : 1;
}

export interface CellPlace {
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
 *  of `box`, at panel size `size` px. Before the body is
 *  measured (`null`) every affect goes on one page. */
export function affectsGrid(
  rows: readonly AffectRow[],
  box: { width: number; height: number } | null,
  size: number = PANE_TEXT_PX,
): AffectsGrid {
  const columns = box ? affectsColumns(box.width, size) : 2;
  const row = paneText(size).affectsRow;
  const tracked = rows.filter(isTrackedRow);
  const others = rows.filter((r) => !isTrackedRow(r));
  const rule = tracked.length > 0 && others.length > 0;
  if (others.length === 0) return { columns, tracked, rule, rest: [], pageRows: 0, pages: 0 };

  const allRows = Math.ceil(others.length / columns);
  const trackedPx = Math.ceil(tracked.length / columns) * row;
  const fit = box
    ? Math.floor((box.height - trackedPx - (rule ? affectsRulePx(size) : 0)) / row)
    : allRows;
  // A page holds at least two cells while two or more affects wait, so
  // the count always has an affect beside it. In a pane too short even
  // for that, the body scrolls.
  const leastRows = others.length > 1 ? Math.ceil(2 / columns) : 1;
  const pageRows = Math.max(leastRows, Math.min(allRows, fit));
  const { cells: rest, pages } = pageCells(others, columns, pageRows);
  return { columns, tracked, rule, rest, pageRows, pages };
}

/** Place `rows` in pages `pageRows` rows tall and `columns` wide. Each
 *  page fills down the left column, then down the right. A page that
 *  cannot hold everything left gives its last cell to the count of
 *  what follows, unless the page is a single cell. The rest of Timers
 *  first and the whole of Countdown page this way. */
export function pageCells(
  rows: readonly AffectRow[],
  columns: number,
  pageRows: number,
): { cells: AffectsCell[]; pages: number } {
  const slots = pageRows * columns;
  const cells: AffectsCell[] = [];
  let next = 0;
  let page = 0;
  while (next < rows.length) {
    const left = rows.length - next;
    const counts = left > slots && slots > 1;
    const take = counts ? slots - 1 : Math.min(left, slots);
    const place = (i: number): CellPlace => ({
      gridRow: page * pageRows + (i % pageRows) + 1,
      gridColumn: Math.floor(i / pageRows) + 1,
      pageStart: i === 0,
    });
    for (let i = 0; i < take; i += 1) {
      cells.push({ kind: 'affect', row: rows[next + i], ...place(i) });
    }
    next += take;
    if (counts) cells.push({ kind: 'more', count: rows.length - next, page, ...place(slots - 1) });
    page += 1;
  }
  return { cells, pages: page };
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
