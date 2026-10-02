import { indexedRgb } from './bandCells';
import { STATUS_TEXT_CONTRAST, type ChromeTokens } from './chrome';
import { liftAtHue, parseHex, toHex } from './color';
import { SECTORS } from './mapPalette';

// How the Map pane shows the room you stand in: the name in the color
// the game tints it with, and the terrain in the game's own words.
//
// do_look in act_info.c prints a room's name after a 256 color code its
// sector picks from the sector_colors table, written `(NNN) in the
// source. The server sends it as ESC[38;5;NNNm to a client that asks
// for 256 colors, as Vosh does, and the terminal draws it from xterm's
// fixed color cube, whatever the theme. An area file can color a name
// itself with a code after the tint, and the terminal then shows that
// code's color instead. Every named room in the area files carries one
// but The Gem Vault in gems.are. Room.Info sends the name with every
// color code stripped, so the pane cannot see the area file's code and
// always shows the sector's tint.
//
// The pane draws on the panel, not on the terminal ground, so a tint
// that reads in the terminal can fade there. A light theme's panel
// washes out the white of an inside room and the light yellow of a
// city, and a dark one swallows the olive of a swamp. Each tint moves
// in OKLCH lightness at its own hue until it reads on the panel, the
// way the chat pane lifts its channel colors. The name is the words of
// its row, with the terrain row under it in the quiet tier at 3.1:1, so
// it holds the 4.5:1 the chrome gives words in a status color rather
// than the 3:1 of a chat line. At 3:1 an inside room on Vellum drew its
// name in the same gray as its terrain. Only the pane's ink moves.

/** The 256 color code do_look tints a room's name with, by sector
 *  index, copied from act_info.c. Index 0 is inside and 12 is snow,
 *  the order Room.Info's `sector` and Map.Tiles' `s` use. */
export const SECTOR_NAME_COLORS: readonly number[] = [
  255, // inside, white
  229, // city, light yellow
  82, // field, bright green
  28, // forest, dark green
  143, // hills, olive/khaki
  245, // mountain, grey
  39, // water_swim, light blue
  27, // water_noswim, deep blue
  58, // swamp, dark olive
  117, // air, sky blue
  220, // desert, gold/sandy
  196, // lava, bright red
  231, // snow, bright white
];

/** The contrast the room's name holds on the panel, the chrome's floor
 *  for words drawn in a status color. */
export const ROOM_NAME_CONTRAST = STATUS_TEXT_CONTRAST;

/** The ground the pane draws on, from the theme's chrome tokens. */
export type RoomNameGround = Pick<ChromeTokens, 'panel' | 'appearance'>;

/** The 256 color code for a sector, or null for a sector the game
 *  prints untinted. do_look tints sectors 0 to 12 only. */
export function sectorNameCode(sector: number | null): number | null {
  if (sector === null || !Number.isInteger(sector)) return null;
  return SECTOR_NAME_COLORS[sector] ?? null;
}

/** The sector's tint as the terminal draws it, `#rrggbb`, or null. */
export function sectorNameTint(sector: number | null): string | null {
  const code = sectorNameCode(sector);
  if (code === null) return null;
  const [r, g, b] = indexedRgb(code, []);
  return toHex({ r, g, b });
}

/** The color the pane draws a room's name in: the sector's tint,
 *  lighter on a dark panel or darker on a light one where it would read
 *  under 4.5:1, at its own hue. Null for a room the game prints untinted,
 *  which keeps the pane's text color. A panel that does not parse as
 *  hex takes the tint as it is. */
export function roomNameColor(sector: number | null, ground: RoomNameGround): string | null {
  const tint = sectorNameTint(sector);
  const rgb = tint ? parseHex(tint) : null;
  if (!tint || !rgb) return null;
  const panel = parseHex(ground.panel);
  if (!panel) return tint;
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
