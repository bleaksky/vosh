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
//
// The pane draws on the panel, not on the terminal ground, so a slot
// that reads in the terminal can fade there (a light theme's panel sits
// a step darker than its paper). Each color moves in OKLCH lightness at
// a fixed hue until it reads at 3:1 on the panel. Where the new
// lightness leaves sRGB, the color gives up chroma rather than hue, so
// a deep yellow stays yellow. Only the pane's ink moves. The theme and
// the terminal keep the published color.

import { ANSI_SLOT_LABELS } from '../theme/appearanceSettings';
import { ANSI_SLOTS, type AnsiSlot } from '../theme/baseAnsi';
import type { ChromeTokens } from '../theme/chrome';
import { composite, contrast, liftAtHue, parseHex, toHex } from '../theme/color';
import type { XtermPalette } from '../theme/themes';

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

/** A channel's color on the theme whose terminal palette is given, as
 *  the theme holds it. The pane menu's swatches show this. */
export function chatChannelColor(
  pane: string,
  palette: XtermPalette,
  colors: ChatColors = NO_CHAT_COLORS,
): string {
  return palette[chatChannelSlot(pane, colors)];
}

/** The contrast every message holds on the panel, tag included. */
export const CHAT_CONTRAST = 3;

/** The tag's step back from its line, the opacity panel.css gives
 *  .pane-chat-tag. */
export const CHAT_TAG_OPACITY = 0.7;

/** How the pane draws the messages of one slot. */
export interface ChatInk {
  /** The line's color, the slot lifted to 3:1 on the panel. */
  color: string;
  /** Whether the tag keeps its step back. A tag that would read under
   *  3:1 at CHAT_TAG_OPACITY draws at full strength instead. */
  fadeTag: boolean;
}

/** The ground the pane draws on, from the theme's chrome tokens. */
export type ChatGround = Pick<ChromeTokens, 'panel' | 'appearance'>;

/** The ink for each of the 16 slots on the theme whose palette and
 *  panel are given. A slot that already reads at 3:1 keeps its color.
 *  One that falls short moves lighter on a dark theme and darker on a
 *  light one, at its own hue. A slot or panel that does not parse as
 *  hex draws as given, tag faded. */
export function chatInks(palette: XtermPalette, ground: ChatGround): Record<AnsiSlot, ChatInk> {
  const panel = parseHex(ground.panel);
  const dir = ground.appearance === 'dark' ? 1 : -1;
  const ink = (color: string): ChatInk => {
    const rgb = parseHex(color);
    if (!rgb || !panel) return { color, fadeTag: true };
    const lifted = liftAtHue(rgb, panel, CHAT_CONTRAST, dir);
    const tag = composite(lifted, panel, CHAT_TAG_OPACITY);
    return { color: toHex(lifted), fadeTag: contrast(tag, panel) >= CHAT_CONTRAST };
  };
  const inks = {} as Record<AnsiSlot, ChatInk>;
  for (const slot of ANSI_SLOTS) inks[slot] = ink(palette[slot]);
  return inks;
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
