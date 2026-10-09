// How every find marks a match: the theme's ANSI yellow at 28%, and the
// match you are on at 60%. The terminal, Help, the session logs page and
// the native grid all take their marks from here.

import { composite, parseHex, toHex, toRgba } from './color';
import type { XtermPalette } from './themes';

const MATCH = 0.28;
const CURRENT = 0.6;

export interface FindMarks {
  /** Every match. */
  match: string;
  /** The match you are on. */
  current: string;
}

/** The marks for a theme palette, or null when its yellow is not a color. */
export function findMarks(xterm: Pick<XtermPalette, 'yellow'>): FindMarks | null {
  const yellow = parseHex(xterm.yellow);
  return yellow ? { match: toRgba(yellow, MATCH), current: toRgba(yellow, CURRENT) } : null;
}

/** The same marks laid over a ground as solid #rrggbb colors, for xterm's
 *  search addon, which takes no alpha. Null when either is not a color. */
export function solidFindMarks(
  xterm: Pick<XtermPalette, 'yellow'>,
  ground: string,
): FindMarks | null {
  const yellow = parseHex(xterm.yellow);
  const under = parseHex(ground);
  if (!yellow || !under) return null;
  return {
    match: toHex(composite(yellow, under, MATCH)),
    current: toHex(composite(yellow, under, CURRENT)),
  };
}
