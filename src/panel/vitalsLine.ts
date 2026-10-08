// The One line vitals density (Settings, Vitals). Health, Mana,
// and Moves sit side by side on one row, each a label, the value, and
// a meter under it. The labels drop only when the row cannot fit every
// label beside its value. With Ilsabet's 1020 / 1020 that happens under
// 372 pt, about the 360 you asked for, while Current and Percent keep
// the labels even at 300. A panel too narrow
// for even the values, near the 200 minimum or with long numbers,
// stacks them in rows, so no value ever runs into the next.

/** A vitals row's side padding, 18 on the left and 12 on the right. */
export const VITALS_ROW_PADDING = 30;
/** Space between two vitals on the line. */
export const VITALS_LINE_GAP = 16;
/** The least space between a label and its value. */
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
  if (values + labels <= room) return 'labels';
  if (values <= room) return 'values';
  return 'rows';
}
