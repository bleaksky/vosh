// What a key pressed in a snoop terminal does. Esc or any key that
// types puts the caret back on the command line, so what you type
// always goes to your own character. A key that types lands there
// too: the caret moves while the key is down, and the browser types it
// into the command line. Keys with Cmd or Ctrl stay, so Cmd J, Cmd F
// and Cmd C still reach the snoop, and so do the keys that type
// nothing, like the arrows and Shift.

/** The parts of a key press the handoff reads. */
export interface SnoopKey {
  key: string;
  metaKey: boolean;
  ctrlKey: boolean;
  isComposing?: boolean;
}

/** What a key press in a snoop terminal does: `escape` puts the caret on
 *  the command line and takes the key, `type` puts the caret there and
 *  lets the key type, and `stay` leaves it to the snoop. */
export type SnoopHandoff = 'escape' | 'type' | 'stay';

export function snoopHandoff(event: SnoopKey): SnoopHandoff {
  if (event.isComposing) return 'stay';
  if (event.key === 'Escape') return 'escape';
  if (event.metaKey || event.ctrlKey) return 'stay';
  // One character, Option's own included. Named keys like Enter or
  // ArrowUp are longer, and a dead key reads `Dead`.
  return [...event.key].length === 1 ? 'type' : 'stay';
}
