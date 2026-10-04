// The Panel font (Settings, Appearance, Panel text): the face every
// pane in the right panel and the status line under the terminal draw
// their text in. It saves as `panel_font` beside the terminal font, in
// the font scope. Empty, the default, is As designed: each text keeps
// the face the panes were designed in, the system font for the headers,
// labels, counts, rows and the status line, and the terminal font for
// the game text. `terminal` is the terminal font for all of it, and
// `system` the system font the menus and Settings use. Anything else is
// a font list the way Font saves one. The main window writes any pick
// but As designed as --panel-font-family, and tokens.css turns it into
// the panel faces, the names every pane rule reads and every pane
// measure measures in (panelFace.ts). Under As designed it writes
// nothing, and each panel face keeps its own.

import { renderFontStack } from './fontLoader';

/** What the row saves for As designed, the default. */
export const PANEL_FONT_DESIGNED = '';
/** What the row saves for the terminal font. */
export const PANEL_FONT_TERMINAL = 'terminal';
/** What the row saves for the system font. */
export const PANEL_FONT_SYSTEM = 'system';

const NAMED = [PANEL_FONT_TERMINAL, PANEL_FONT_SYSTEM];

/** A saved panel font as the page reads it: trimmed, with the terminal
 *  font and the system font spelled one way each. Anything but a string
 *  is As designed, so a config from before the row reads the way it
 *  always drew. */
export function normalizePanelFont(value: unknown): string {
  if (typeof value !== 'string') return PANEL_FONT_DESIGNED;
  const trimmed = value.trim();
  return NAMED.find((named) => named === trimmed.toLowerCase()) ?? trimmed;
}

/** The font list a saved choice names, as the page renders it, or null
 *  for As designed, the terminal font and the system font, which the
 *  page has. */
export function panelFontList(choice: unknown): string | null {
  const c = normalizePanelFont(choice);
  return c === PANEL_FONT_DESIGNED || NAMED.includes(c) ? null : renderFontStack(c);
}

/** The value a window writes as --panel-font-family for a saved choice:
 *  the terminal face, the system face, or the font list. Null for As
 *  designed, where the window writes nothing and each panel face keeps
 *  the face it was designed in. */
export function panelFontFamily(choice: unknown): string | null {
  const c = normalizePanelFont(choice);
  if (c === PANEL_FONT_DESIGNED) return null;
  if (c === PANEL_FONT_TERMINAL) return 'var(--font-mud)';
  if (c === PANEL_FONT_SYSTEM) return 'var(--font-ui)';
  return renderFontStack(c);
}
