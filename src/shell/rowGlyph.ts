import type { RowLook } from '../stores/session/sessionRowStore';

/** The glyph a session's row shows in the meta's place, board 3 of the
 *  Sessions review, in the sidebar and the session popover alike. */
export type RowGlyph = 'triangle' | 'hand' | 'spinner' | 'dot';

/** The glyph of a row's look: its mark while something needs you, or
 *  the dot while something waits on a row behind. */
export function rowGlyph({ mark, count }: RowLook, current: boolean): RowGlyph | null {
  if (mark === 'triangle' || mark === 'hand' || mark === 'spinner') return mark;
  return count > 0 && !current ? 'dot' : null;
}
