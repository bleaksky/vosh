// The Panel font (Settings, Appearance, Terminal text): the face every
// pane in the right panel and the status line under the terminal draw
// their text in. It saves as `panel_font` beside the terminal font, in
// the font scope. Empty, the default, is the terminal font. `system` is
// the system font the menus and Settings use. Anything else is a font
// list the way Font saves one. The main window writes the face as
// --panel-font-family, and tokens.css turns it into --font-panel, the
// one name every pane rule reads and every pane measure measures in
// (panelFace.ts).

import { renderFontStack } from './fontLoader';

/** What the row saves for the terminal font, the default. */
export const PANEL_FONT_TERMINAL = '';
/** What the row saves for the system font. */
export const PANEL_FONT_SYSTEM = 'system';

/** A saved panel font as the page reads it: trimmed, with the system
 *  font spelled one way. Anything but a string is the terminal font,
 *  so a config from before the row reads the way it always drew. */
export function normalizePanelFont(value: unknown): string {
  if (typeof value !== 'string') return PANEL_FONT_TERMINAL;
  const trimmed = value.trim();
  return trimmed.toLowerCase() === PANEL_FONT_SYSTEM ? PANEL_FONT_SYSTEM : trimmed;
}

/** The font list a saved choice names, as the page renders it, or null
 *  for the terminal font and the system font, which the page has. */
export function panelFontList(choice: unknown): string | null {
  const c = normalizePanelFont(choice);
  return c === PANEL_FONT_TERMINAL || c === PANEL_FONT_SYSTEM ? null : renderFontStack(c);
}

/** The value a window writes as --panel-font-family for a saved choice:
 *  the terminal face, the system face, or the font list. */
export function panelFontFamily(choice: unknown): string {
  const c = normalizePanelFont(choice);
  if (c === PANEL_FONT_TERMINAL) return 'var(--font-mud)';
  if (c === PANEL_FONT_SYSTEM) return 'var(--font-ui)';
  return renderFontStack(c);
}
