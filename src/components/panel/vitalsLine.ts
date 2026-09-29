// The One line vitals density (Settings, Layout, Vitals). Health, Mana,
// and Moves sit side by side on one row, each a label, the value, and
// a 2 px meter under it. On a panel under about 360 pt the labels drop
// and the values and meters stay. Longer values push that a little
// wider, since the labels show only once they fit (Erelei's four digit
// health needs 372). A panel too narrow for even the values, near the
// 200 minimum or with long numbers, stacks them in rows, so no value
// ever runs into the next.

/** The narrowest panel, in CSS pixels (points on macOS), that shows
 *  the Health, Mana, and Moves labels on one line. */
export const VITALS_LINE_LABEL_MIN_WIDTH = 360;

/** A vitals row's side padding, 18 on the left and 12 on the right. */
export const VITALS_ROW_PADDING = 30;
/** Space between two vitals on the line. */
export const VITALS_LINE_GAP = 16;
/** Space between a label and its value. */
export const VITALS_LINE_LABEL_GAP = 6;

/** One vital's text widths in CSS pixels, as drawn. */
export interface VitalsLineItem {
  label: number;
  value: number;
}

/** How One line draws on a panel: with the labels, with the values
 *  alone, or stacked in rows when even the values do not fit. */
export type VitalsLineFit = 'labels' | 'values' | 'rows';

export function vitalsLineFit(panelWidth: number, items: readonly VitalsLineItem[]): VitalsLineFit {
  const gaps = VITALS_LINE_GAP * Math.max(0, items.length - 1);
  const room = panelWidth - VITALS_ROW_PADDING - gaps;
  const values = items.reduce((sum, item) => sum + item.value, 0);
  const labels = items.reduce((sum, item) => sum + item.label + VITALS_LINE_LABEL_GAP, 0);
  if (panelWidth >= VITALS_LINE_LABEL_MIN_WIDTH && values + labels <= room) return 'labels';
  if (values <= room) return 'values';
  return 'rows';
}
