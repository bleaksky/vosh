import { textPx } from './paneTextSize';

// How the styles of the More Vitals Styles review measure, for the
// height a footer holds while it waits for your vitals. Every length
// that sits with the text scales with your panel size, and the marks
// keep their px. Kept pure for the unit tests.

/** The footer's height for `rows` rows of Bands, the 1 px line on top
 *  included: each row its text line, 3 under it the 8 px graph, and 5
 *  between two rows, between the footer's pads, as panel.css draws
 *  them. */
export function bandsHeight(size: number, rows: number): number {
  const row = textPx(16, size) + textPx(3, size) + 8;
  return 1 + textPx(9, size) + rows * row + (rows - 1) * textPx(5, size) + textPx(11, size);
}
