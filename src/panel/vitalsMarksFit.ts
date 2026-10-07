import { textPx } from './paneTextSize';
import { FOOTER_INSETS, type MeasureText } from './vitalsLedgerFit';

// The Gauges and Pips vitals styles (Vitals Styles, boards 1 and 6).
// Each vital is a 22 px pane row of its label, a mark and its value,
// the marks starting together after the longest label and ending
// together before the widest value. Gauges draws a 6 px pill that fills
// in the vital's tone, Pips ten 6 px discs lit as the status line's
// moons are. On a narrow panel Pips first draws five discs, and either
// style then drops its marks under the label and value, so every label
// stays whole. Every fit measures at the vitals' max, digits as zeros,
// so a fight never moves a mark. Kept pure for the unit tests.

/** Space between a Gauges label, its pill and its value. */
export const GAUGE_GAP = 10;
/** The narrowest a pill draws beside its label and value. */
export const GAUGE_MIN = 40;
/** Space between a Pips label, its discs and its value. */
export const PIPS_GAP = 8;
/** A disc's size, and the space between two discs. */
export const PIP = 6;
export const PIP_GAP = 3;

/** How Gauges fits the panel: each pill beside its label and value, or
 *  under both. */
export type GaugesFit = 'beside' | 'under';

/** How Pips fits the panel: ten discs beside the label and value, five,
 *  or ten under both. */
export type PipsFit = 'ten' | 'five' | 'under';

/** The widest of `texts` at `px` px and `weight`. */
function widest(texts: readonly string[], px: number, weight: number, measure: MeasureText) {
  return texts.reduce((most, text) => Math.max(most, measure(text, px, weight)), 0);
}

/** The room a footer `width` px wide leaves beside the longest of your
 *  `labels` and the widest of `values`, each value at its max. */
export function markRoom(
  width: number,
  size: number,
  labels: readonly string[],
  values: readonly string[],
  measure: MeasureText,
): number {
  return (
    width - FOOTER_INSETS - widest(labels, size, 400, measure) - widest(values, size, 500, measure)
  );
}

/** Gauges keeps a pill of 40 at least between the longest label and the
 *  widest value, else drops each pill under them. */
export function gaugesFit(
  width: number,
  size: number,
  labels: readonly string[],
  values: readonly string[],
  measure: MeasureText,
): GaugesFit {
  if (labels.length === 0) return 'beside';
  return markRoom(width, size, labels, values, measure) - 2 * GAUGE_GAP < GAUGE_MIN
    ? 'under'
    : 'beside';
}

/** The width of `count` discs in a row. */
export function pipsWidth(count: number): number {
  return count * PIP + (count - 1) * PIP_GAP;
}

/** Pips keeps every label whole: ten discs, then five, then ten under
 *  the label and value. */
export function pipsFit(
  width: number,
  size: number,
  labels: readonly string[],
  values: readonly string[],
  measure: MeasureText,
): PipsFit {
  if (labels.length === 0) return 'ten';
  const spare = markRoom(width, size, labels, values, measure) - 2 * PIPS_GAP;
  if (pipsWidth(10) <= spare) return 'ten';
  if (pipsWidth(5) <= spare) return 'five';
  return 'under';
}

/** A disc lit whole, lit on its left half, or dark. */
export type PipLight = 'full' | 'half' | 'off';

/** How `count` discs light for a vital at `pct` percent, in halves, as
 *  a moon lights on the status line. Null, a value the game hides,
 *  lights none. */
export function pipLights(pct: number | null, count: number): PipLight[] {
  const halves = pct === null ? 0 : Math.round((pct * count * 2) / 100);
  return Array.from({ length: count }, (_, i) =>
    halves >= 2 * (i + 1) ? 'full' : halves === 2 * i + 1 ? 'half' : 'off',
  );
}

/** The footer's height for `rows` rows beside their marks, the 1 px
 *  line on top included, as panel.css draws them. It holds this while
 *  it waits for your vitals. */
export function marksHeight(size: number, rows: number): number {
  return 1 + textPx(9, size) + rows * textPx(22, size) + textPx(11, size);
}
