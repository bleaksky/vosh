// How every find marks a match: the theme's ANSI yellow at 28%, and the
// match you are on at 60%. The terminal, Help, the session logs page and
// the native grid all take their marks from here.

import { parseHex, toRgba } from './color';
import type { XtermPalette } from './themes';

export interface FindMarks {
  /** Every match. */
  match: string;
  /** The match you are on. */
  current: string;
}

/** The marks for a theme palette, or null when its yellow is not a color. */
export function findMarks(xterm: Pick<XtermPalette, 'yellow'>): FindMarks | null {
  const yellow = parseHex(xterm.yellow);
  return yellow ? { match: toRgba(yellow, 0.28), current: toRgba(yellow, 0.6) } : null;
}
