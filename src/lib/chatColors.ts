// Per channel colors for the chat pane, from the active theme's ANSI
// slots. Each Aabahran channel takes the slot the game prints its
// messages in (comm.c color codes, act_comm.c and languages.c), so a
// line in the pane matches the same message in the terminal. The game
// sends bold in the bright slot, so a bold code maps to its bright slot.
// Pane names the game does not send (trigger routes, other games'
// channels) hash onto the colored slots, so every channel still gets a
// stable color of its own.

import type { AnsiSlot } from './baseAnsi';
import type { XtermPalette } from './themes';

// A Map rather than an object literal: pane names come straight from
// server data and user-defined routes, and keys like "constructor"
// must not walk the prototype chain.
const CHANNEL_SLOTS = new Map<string, AnsiSlot>([
  ['say', 'brightYellow'], // `# bold yellow
  ['tell', 'green'], // `2 green
  ['gtell', 'brightMagenta'], // `9 bold magenta
  ['yell', 'cyan'], // `6 cyan
  ['pray', 'brightWhite'], // `& bold white
  ['cabal', 'brightBlue'], // `0 bold blue
  ['clan', 'brightCyan'], // `^ bold cyan
  ['faction', 'yellow'], // `3 yellow
  ['newbie', 'brightGreen'], // `@ bold green
  ['immortal', 'brightRed'], // `! bold red
  ['imp', 'brightCyan'], // the message in `^ bold cyan
  // Other games name the same channels in their own words.
  ['tells', 'green'],
  ['says', 'brightYellow'],
  ['group', 'brightMagenta'],
  ['shout', 'cyan'],
]);

// The slots a pane the game does not send can land on. The grays read
// as plain text and red reads as a warning, so neither is offered.
const HASHED_SLOTS: readonly AnsiSlot[] = [
  'green',
  'yellow',
  'blue',
  'magenta',
  'cyan',
  'brightGreen',
  'brightYellow',
  'brightBlue',
  'brightMagenta',
  'brightCyan',
];

/** The ANSI slot a channel's line takes in the chat pane. */
export function chatChannelSlot(pane: string): AnsiSlot {
  const key = pane.trim().toLowerCase();
  const fixed = CHANNEL_SLOTS.get(key);
  if (fixed) return fixed;
  // FNV-1a over the pane name; stable across sessions and windows.
  let hash = 0x811c9dc5;
  for (let i = 0; i < key.length; i++) {
    hash ^= key.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193);
  }
  return HASHED_SLOTS[(hash >>> 0) % HASHED_SLOTS.length];
}

/** A channel's color on the theme whose terminal palette is given. */
export function chatChannelColor(pane: string, palette: XtermPalette): string {
  return palette[chatChannelSlot(pane)];
}
