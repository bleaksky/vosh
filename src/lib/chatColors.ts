// Per channel colors for the chat pane, from the active theme's ANSI
// slots. Each Aabahran channel takes the slot the game prints its
// messages in (comm.c color codes, act_comm.c and languages.c), so a
// line in the pane matches the same message in the terminal. The game
// sends bold in the bright slot, so a bold code maps to its bright slot.
// Pane names the game does not send (trigger routes, other games'
// channels) hash onto the colored slots, so every channel still gets a
// stable color of its own. A color you pick in the chat pane's menu
// (Channel colors) wins over both, and stays one of the theme's slots,
// so it follows a theme switch too.

import { ANSI_SLOT_LABELS } from './appearanceSettings';
import { ANSI_SLOTS, type AnsiSlot } from './baseAnsi';
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

/** The channels Aabahran sends over Comm.Channel, in the order the
 *  chat pane's Channel colors menu lists them. */
export const CHAT_CHANNELS = [
  'say',
  'tell',
  'gtell',
  'yell',
  'pray',
  'cabal',
  'clan',
  'faction',
  'newbie',
  'immortal',
  'imp',
] as const;

/** The colors you picked for the active profile's channels, by channel
 *  name in lowercase. A channel left out takes its default. */
export type ChatColors = ReadonlyMap<string, AnsiSlot>;

export const NO_CHAT_COLORS: ChatColors = new Map();

const SLOT_SET: ReadonlySet<string> = new Set(ANSI_SLOTS);

/** The chat colors a backend table holds, keeping only the 16 slots. */
export function normalizeChatColors(raw: unknown): ChatColors {
  const colors = new Map<string, AnsiSlot>();
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) return colors;
  for (const [channel, slot] of Object.entries(raw as Record<string, unknown>)) {
    const key = channel.trim().toLowerCase();
    if (key && typeof slot === 'string' && SLOT_SET.has(slot)) {
      colors.set(key, slot as AnsiSlot);
    }
  }
  return colors;
}

/** Whether two tables hold the same picks. */
export function sameChatColors(a: ChatColors, b: ChatColors): boolean {
  if (a.size !== b.size) return false;
  for (const [channel, slot] of a) {
    if (b.get(channel) !== slot) return false;
  }
  return true;
}

/** The ANSI slot a channel's line takes in the chat pane: the one you
 *  picked, or else the one the game prints it in. */
export function chatChannelSlot(pane: string, colors: ChatColors = NO_CHAT_COLORS): AnsiSlot {
  const key = pane.trim().toLowerCase();
  const picked = colors.get(key);
  if (picked) return picked;
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
export function chatChannelColor(
  pane: string,
  palette: XtermPalette,
  colors: ChatColors = NO_CHAT_COLORS,
): string {
  return palette[chatChannelSlot(pane, colors)];
}

/** One row of a channel's color list in the chat pane menu. */
export interface ChatColorChoice {
  /** The slot, or null for Default. */
  value: AnsiSlot | null;
  label: string;
  /** The color the row's swatch shows on this theme. */
  swatch: string;
  checked: boolean;
}

/** Default, then the theme's 16 ANSI colors in slot order, with a check
 *  on what `channel` shows now. Default shows the color the game prints
 *  the channel in. */
export function chatColorChoices(
  channel: string,
  colors: ChatColors,
  palette: XtermPalette,
): ChatColorChoice[] {
  const picked = colors.get(channel.trim().toLowerCase()) ?? null;
  return [
    {
      value: null,
      label: 'Default',
      swatch: palette[chatChannelSlot(channel)],
      checked: picked === null,
    },
    ...ANSI_SLOTS.map((slot) => ({
      value: slot,
      label: ANSI_SLOT_LABELS[slot],
      swatch: palette[slot],
      checked: picked === slot,
    })),
  ];
}
