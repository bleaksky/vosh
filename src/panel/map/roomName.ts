import type { AnsiSlot } from '../../theme/baseAnsi';
import { STATUS_TEXT_CONTRAST, type ChromeTokens } from '../../theme/chrome';
import { liftAtHue, parseHex, toHex } from '../../theme/color';
import { SECTORS } from './mapPalette';
import type { XtermPalette } from '../../theme/themes';

// How the Map pane shows the room you stand in: the name in the color
// the terminal draws it in, and the terrain in the game's own words.
//
// Every named room in the area files but The Gem Vault in gems.are
// starts its name with a 16 color code, which format_room_name in db.c
// wrote from room_color_table in tables.c by the room's sector. do_look
// in act_info.c prints a 256 color tint for the sector before the name,
// but the name's own code follows it at once, and process_color in
// comm.c opens every 16 color code with ESC[0, a full reset. The tint
// never shows, and the terminal draws the name in one of the theme's 16
// ANSI colors. A bold code (`8 `^ `@ `# `&) draws in its bright slot,
// the way the terminal and the chat pane read bold.
//
// Room.Info sends the name with its codes stripped, so the pane draws
// the slot the sector gives. That is the name's own code for 12,737 of
// the 12,893 named rooms the game loads from area.lst. The other 156
// carry a code their builder picked, like the 39 inside rooms in white,
// or none at all. Swamps follow the area files rather than
// room_color_table. The table gives them `@, bold green, but all 119
// swamp rooms carry `8, so the terminal draws them gray. Drawing from
// the theme's slots also follows a theme switch the way the terminal
// does.
//
// The pane draws on the panel, not on the terminal ground, so a slot
// that reads in the terminal can fade there. A dark theme's bright
// black sits close to its panel, and a light theme's white washes out.
// Each color moves in OKLCH lightness at its own hue until it reads on
// the panel, the way the chat pane lifts its channel colors. The name
// is the words of its row, with the terrain row under it in the quiet
// tier at 3.1:1, so it holds the 4.5:1 the chrome gives words in a
// status color rather than the 3:1 of a chat line. Only the pane's ink
// moves.

/** The ANSI slot the terminal draws a room's name in, by sector index,
 *  from the code room_color_table in tables.c gives the sector. Index 0
 *  is inside and 12 is snow, the order Room.Info's `sector` and
 *  Map.Tiles' `s` use. */
export const SECTOR_NAME_SLOTS: readonly AnsiSlot[] = [
  'brightBlack', // inside, `8 bold black
  'white', // city, `7 white
  'yellow', // field, `3 yellow
  'green', // forest, `2 green
  'yellow', // hills, `3 yellow
  'yellow', // mountain, `3 yellow
  'brightCyan', // water_swim, `^ bold cyan
  'blue', // water_noswim, `4 blue
  'brightBlack', // swamp, `8 in every area file, where the table gives `@
  'cyan', // air, `6 cyan
  'brightYellow', // desert, `# bold yellow
  'red', // lava, `1 red
  'brightWhite', // snow, `& bold white
];

/** The contrast the room's name holds on the panel, the chrome's floor
 *  for words drawn in a status color. */
export const ROOM_NAME_CONTRAST = STATUS_TEXT_CONTRAST;

/** The ground the pane draws on, from the theme's chrome tokens. */
export type RoomNameGround = Pick<ChromeTokens, 'panel' | 'appearance'>;

/** The slot for a sector, or null for a sector past the table, which
 *  keeps the pane's text color. Room.Info sends -1 for a room with no
 *  sector. */
export function sectorNameSlot(sector: number | null): AnsiSlot | null {
  if (sector === null || !Number.isInteger(sector)) return null;
  return SECTOR_NAME_SLOTS[sector] ?? null;
}

/** The color the pane draws a room's name in: the theme's color for the
 *  sector's slot, lighter on a dark panel or darker on a light one where
 *  it would read under 4.5:1, at its own hue. Null for a sector past the
 *  table. A slot or panel that does not parse as hex draws as given. */
export function roomNameColor(
  sector: number | null,
  palette: XtermPalette,
  ground: RoomNameGround,
): string | null {
  const slot = sectorNameSlot(sector);
  if (!slot) return null;
  const color = palette[slot];
  const rgb = parseHex(color);
  const panel = parseHex(ground.panel);
  if (!rgb || !panel) return color;
  const dir = ground.appearance === 'dark' ? 1 : -1;
  return toHex(liftAtHue(rgb, panel, ROOM_NAME_CONTRAST, dir));
}

/** The room's terrain as the game's map legend names it, like `Inside`
 *  or `Deep Water`. A terrain past the known sectors shows in its own
 *  words, and `unknown`, which the game sends for a room with no
 *  sector, shows nothing. */
export function terrainLabel(sector: number | null, terrain: string | null): string | null {
  if (sector !== null && Object.hasOwn(SECTORS, sector)) return SECTORS[sector].name;
  const words = terrain?.replace(/_/g, ' ').trim();
  if (!words || words.toLowerCase() === 'unknown') return null;
  return words.charAt(0).toUpperCase() + words.slice(1);
}
