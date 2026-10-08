import type { Vital } from '../ipc/uiConfigVitals';
import type { VitalSample } from '../stores/gmcp/vitalsStore';
import { textPx } from './paneTextSize';
import { maxOf } from './vitalsView';
import { FOOTER_INSETS, LEDGER_GAP, type LedgerFigure, type MeasureText } from './vitalsLedgerFit';
import { markRoom } from './vitalsMarksFit';

// How the drawn styles, Bands to Orbs, fit the panel and
// measure, and the height a footer holds while it waits for your
// vitals. Every length that sits with the text scales with your panel
// size, and the marks keep their px. Every fit measures each vital at
// its max, digits as zeros, so a fight never moves a mark. Kept pure
// for the unit tests.

/** The footer's height for `rows` rows of Bands, the 1 px line on top
 *  included: each row its text line, 3 under it the 8 px graph, and 5
 *  between two rows, between the footer's pads, as panel.css draws
 *  them. */
export function bandsHeight(size: number, rows: number): number {
  const row = textPx(16, size) + textPx(3, size) + 8;
  return 1 + textPx(9, size) + rows * row + (rows - 1) * textPx(5, size) + textPx(11, size);
}

/** How a row style fits the panel: each mark beside its label and
 *  value, or under both. */
export type RowMarkFit = 'beside' | 'under';

/** Space between a row style's label, its mark and its value. */
export const ROW_MARK_GAP = 10;

/** The narrowest a row style's mark draws beside its label and value.
 *  Gauges drops its mark under once the mark would be under 40 pt,
 *  but a ladder needs 2 px for each of its 24 segments and 1 px for
 *  each gap, 71 in all, so a ladder drops under at the 200 pt floor
 *  where Gauges stays beside. So the row styles keep 72 and drop under
 *  below it. */
export const ROW_MARK_MIN = 72;

/** A row style keeps each mark beside the longest of `labels` and the
 *  widest of `values` while it has ROW_MARK_MIN, else drops it under. */
export function rowMarkFit(
  width: number,
  size: number,
  labels: readonly string[],
  values: readonly string[],
  measure: MeasureText,
): RowMarkFit {
  if (labels.length === 0) return 'beside';
  return markRoom(width, size, labels, values, measure) - 2 * ROW_MARK_GAP < ROW_MARK_MIN
    ? 'under'
    : 'beside';
}

/** The segments of a ladder, and of your opponent's across the footer. */
export const LADDER = 24;
export const FOE_LADDER = 48;

/** How many of `count` segments light for a vital at `pct` percent:
 *  each lights once the vital reaches its share, so 78 percent lights
 *  19 of 24. A value the game hides lights none. */
export function litSegments(pct: number | null, count: number): number {
  return pct === null ? 0 : Math.round((pct * count) / 100);
}

/** The footer's height for `rows` rows of Traces, the 1 px line on top
 *  included: each row 26 tall, between the footer's pads. */
export function tracesHeight(size: number, rows: number): number {
  return 1 + textPx(9, size) + rows * textPx(26, size) + textPx(11, size);
}

/** How many of your last Char.Vitals a trace spans. */
export const TRACE_POINTS = 40;

/** A vital's trace: each of the last TRACE_POINTS of `history` as a
 *  share of its max, oldest first, or `now` percent alone while the
 *  history holds none. */
export function traceSeries(history: readonly VitalSample[], vital: Vital, now: number): number[] {
  const series = history.slice(-TRACE_POINTS).map(({ values }) => {
    const max = values[maxOf(vital)];
    return max > 0 ? Math.max(0, Math.min(1, values[vital] / max)) : 0;
  });
  return series.length === 0 ? [now / 100] : series;
}

/** How a column style fits the panel. It keeps its columns at every
 *  width: in full, without the max, or with a smaller instrument and no
 *  max. */
export type ColumnFit = 'full' | 'bare' | 'narrow';

/** The width of each of `count` columns a footer `width` px wide holds,
 *  16 apart, as Ledger lays them. */
export function columnWidth(width: number, count: number): number {
  return (width - FOOTER_INSETS - LEDGER_GAP * (count - 1)) / Math.max(1, count);
}

/** A dial's size in full and on a narrow panel. */
export const DIAL = 60;
export const DIAL_NARROW = 44;

/** Dials draws each dial at 60 while its column holds one, else at 44
 *  without the max, which sits in the dial's opening. */
export function dialsFit(width: number, count: number): ColumnFit {
  return count === 0 || columnWidth(width, count) >= DIAL ? 'full' : 'narrow';
}

/** The footer's height for Dials, the 1 px line on top included: the
 *  caps, 5 under them the dial, between the column pads. */
export function dialsHeight(size: number, fit: ColumnFit): number {
  const dial = fit === 'narrow' ? DIAL_NARROW : DIAL;
  return 1 + textPx(10, size) + textPx(12, size) + textPx(5, size) + dial + textPx(12, size);
}

/** A vial's size, and the space between it and its figure. */
export const VIAL_WIDTH = 18;
export const VIAL_HEIGHT = 44;
const VIAL_GAP = 9;

/** Vials sets the pane label caps, the figure and the max beside each
 *  vial while the widest of each fits its column, else moves the
 *  figure under the vial and drops the max. */
export function vialsFit(
  width: number,
  size: number,
  widest: readonly { label: string; figure: LedgerFigure }[],
  measure: MeasureText,
): ColumnFit {
  if (widest.length === 0) return 'full';
  const column = columnWidth(width, widest.length) - VIAL_WIDTH - VIAL_GAP;
  const fits = widest.every(
    ({ label, figure }) =>
      measure(label.toUpperCase(), textPx(10, size), 600) <= column &&
      measure(figure.current, textPx(16, size), 500) <= column &&
      (figure.max === null || measure(figure.max, textPx(10, size), 500) <= column),
  );
  return fits ? 'full' : 'narrow';
}

/** The footer's height for Vials, the 1 px line on top included: the
 *  vial beside its text, or the caps, the vial and the figure under it,
 *  between the column pads. */
export function vialsHeight(size: number, fit: ColumnFit): number {
  const cell =
    fit === 'narrow'
      ? textPx(12, size) + textPx(5, size) + VIAL_HEIGHT + textPx(5, size) + textPx(20, size)
      : Math.max(VIAL_HEIGHT, textPx(12, size) + textPx(20, size) + textPx(12, size));
  return 1 + textPx(10, size) + cell + textPx(12, size);
}

/** An orb's size in full, on a narrow panel, and before your
 *  opponent's name. */
export const ORB = 44;
export const ORB_NARROW = 40;
export const ORB_FOE = 14;

/** Orbs writes each value with its max under its orb while the widest
 *  fits its column, then drops the max, then draws the orbs at 40 where
 *  a column has no room round a 44. */
export function orbsFit(
  width: number,
  size: number,
  widest: readonly { figure: LedgerFigure }[],
  measure: MeasureText,
): ColumnFit {
  if (widest.length === 0) return 'full';
  const column = columnWidth(width, widest.length);
  const fits = widest.every(
    ({ figure }) =>
      measure(
        figure.max === null ? figure.current : `${figure.current} ${figure.max}`,
        size,
        500,
      ) <= column,
  );
  if (fits && column >= ORB) return 'full';
  return column >= ORB + 4 ? 'bare' : 'narrow';
}

/** The footer's height for Orbs, the 1 px line on top included: the
 *  caps, the orb and the value under it, between the column pads. */
export function orbsHeight(size: number, fit: ColumnFit): number {
  const orb = fit === 'narrow' ? ORB_NARROW : ORB;
  const cell = textPx(12, size) + textPx(5, size) + orb + textPx(5, size) + textPx(16, size);
  return 1 + textPx(10, size) + cell + textPx(12, size);
}

/** The Rings glyph's size, and the radius of each ring, outer to inner
 *  in your order. */
export const RINGS = 56;
export const RING_RADII = [25, 19, 13] as const;

/** How Rings fits the panel: its legend with each label, or with only
 *  the key in each vital's tone, the label left to a screen reader. */
export type RingsFit = 'labels' | 'keys';

/** The room a legend row keeps beside the glyph for its key, the space
 *  after it, and the space between its label and its value. */
const RING_ROW_EXTRAS = 6 + 8 + 8;

/** Rings keeps every label whole beside the widest value, else draws
 *  only the keys. */
export function ringsFit(
  width: number,
  size: number,
  labels: readonly string[],
  values: readonly string[],
  measure: MeasureText,
): RingsFit {
  const room = markRoom(width, size, labels, values, measure) - RINGS - 16 - RING_ROW_EXTRAS;
  return labels.length === 0 || room >= 0 ? 'labels' : 'keys';
}

/** The footer's height for Rings, the 1 px line on top included: the
 *  glyph or the legend's `rows` rows of 19, whichever is taller,
 *  between the column pads. */
export function ringsHeight(size: number, rows: number): number {
  return 1 + textPx(10, size) + Math.max(RINGS, rows * textPx(19, size)) + textPx(12, size);
}

/** The full block, the cell Blocks draws for each whole share. */
export const FULL_BLOCK = '█';

/** The left eighths of a block, one to seven, for Blocks' last cell. */
const EIGHTHS = ['', '▏', '▎', '▍', '▌', '▋', '▊', '▉'];

/** How many cells of `cell` px fit a Blocks bar `width` px wide, at
 *  least one, or none before either is measured. */
export function blockCells(width: number, cell: number): number {
  return width > 0 && cell > 0 ? Math.max(1, Math.floor(width / cell)) : 0;
}

/** A Blocks bar at `pct` percent of `cells` cells: a full block for
 *  each whole cell and an eighth for the last, as btop draws a meter. */
export function blockRun(pct: number, cells: number): string {
  const exact = (pct * cells) / 100;
  const whole = Math.floor(exact);
  return FULL_BLOCK.repeat(whole) + EIGHTHS[Math.floor((exact - whole) * 8)];
}

/** The segment of `count` a Ladders peak at `peak` percent holds lit,
 *  the one the vital reached into, or -1 for none. */
export function ladderPeak(peak: number | null, count: number): number {
  return peak === null ? -1 : Math.ceil((peak * count) / 100) - 1;
}
