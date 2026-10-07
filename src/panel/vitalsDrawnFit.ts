import { textPx } from './paneTextSize';
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

/** The segment of `count` a Ladders peak at `peak` percent holds lit,
 *  the one the vital reached into, or -1 for none. */
export function ladderPeak(peak: number | null, count: number): number {
  return peak === null ? -1 : Math.ceil((peak * count) / 100) - 1;
}
