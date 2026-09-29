// The One line vitals density (Settings, Layout, Vitals). Health, Mana,
// and Moves sit side by side on one row, each a label, the value, and
// a 2 px meter under it. On a narrow panel the labels drop and the
// values and meters stay.

/** The narrowest panel, in CSS pixels (points on macOS), that still
 *  shows the Health, Mana, and Moves labels on one line. */
export const VITALS_LINE_LABEL_MIN_WIDTH = 360;

/** Whether One line shows the labels on a panel this wide. */
export function vitalsLineShowsLabels(panelWidth: number): boolean {
  return panelWidth >= VITALS_LINE_LABEL_MIN_WIDTH;
}
