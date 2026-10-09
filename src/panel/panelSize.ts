// The panel size (Settings, Appearance, Panel text, Size): the size in
// px every pane in the right panel and the status line under the
// terminal draw at. It saves as `panel_font_size` beside the panel
// font, in the font scope. 12, the default, is the size the panes were
// drawn at before you could pick one. 0 follows your terminal size.
// Anything else is a size the way Size saves one, 6 to 64 on half
// steps, such as 13.5.

import { normalizeTextSize } from '../lib/textSize';

/** What the row saves to follow your terminal size. */
export const PANEL_SIZE_TERMINAL = 0;
/** The size a profile starts at, the size the panes were drawn at. */
export const DEFAULT_PANEL_SIZE = 12;

/** A saved panel size as the page reads it, on the nearest half step
 *  and held to 6 to 64 as Rust holds it. Anything but a number is 12,
 *  so a config from before the row draws the way it always drew. */
export function normalizePanelSize(value: unknown): number {
  return normalizeTextSize(value, DEFAULT_PANEL_SIZE, true);
}

/** The size in px the panel draws at: your panel size, or your
 *  `terminal` size while the row follows it. 12 for a terminal size
 *  that is not one. */
export function resolvePanelSize(saved: unknown, terminal: number): number {
  const size = normalizePanelSize(saved);
  if (size !== PANEL_SIZE_TERMINAL) return size;
  return Number.isFinite(terminal) && terminal > 0 ? terminal : DEFAULT_PANEL_SIZE;
}
