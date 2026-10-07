import type { Vital } from '../ipc/uiConfig';
import type { VitalSample } from '../stores/gmcp/vitalsStore';
import { textPx } from './paneTextSize';
import { maxOf } from './vitalsView';
import type { MeasureText } from './vitalsLedgerFit';
import { markRoom } from './vitalsMarksFit';

// How the styles of the More Vitals Styles review fit the panel and
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
 *  Q24 words the rule as Gauges keeps it, under once the mark would be
 *  under 40 pt, but a ladder needs 2 px for each of its 24 segments and
 *  1 px for each gap, 71 in all, and board 5 drops the ladder under at
 *  the 200 pt floor where Gauges stays beside. So the row styles keep
 *  72, which agrees with the board, and drop under below it. */
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
