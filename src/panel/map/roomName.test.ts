import { describe, expect, it } from 'vitest';
import { ANSI_SLOTS, type AnsiSlot } from '../../theme/baseAnsi';
import { contrast, parseHex, rgbToOklch, type Rgb } from '../../theme/color';
import { logSpanCss, parseLogLine } from '../../settings/general/logView';
import {
  ROOM_NAME_CONTRAST,
  SECTOR_NAME_SLOTS,
  roomNameColor,
  sectorNameSlot,
  terrainLabel,
} from './roomName';
import { parseRoomInfo } from '../../stores/gmcp/roomStore';
import { BUILTIN_THEMES, findTheme, themeTokens } from '../../theme/themes';

const hex = (h: string): Rgb => {
  const c = parseHex(h);
  if (!c) throw new Error(`not hex ${h}`);
  return c;
};

// room_color_table in tables.c of the forsaken_lands source, lines 727
// to 739, one entry per sector in sector order.
const ROOM_COLOR_TABLE = [
  "  {'8'}, /* SECT_INSIDE  dark-gray*/",
  "  {'7'}, /* SECT_CITY  bright-green/blue*/",
  "  {'3'}, /* SECT_FIELD  dark-white*/",
  "  {'2'}, /* SECT_FOREST dark-green*/",
  "  {'3'}, /* SECT_HILLS dark-yellow*/",
  "  {'3'}, /* SECT_MOUNTAIN dark-yellow*/",
  "  {'^'}, /* SECT_WATER_SWIM  dark-blue*/",
  "  {'4'}, /* SECT_WATER_NOSWIM dark-blue*/",
  "  {'@'}, /* SECT_SWAMP bright-green*/",
  "  {'6'}, /* SECT_AIR dark-cyan?*/",
  "  {'#'}, /* SECT_DESERT  bright-yellow*/",
  "  {'1'}, /* SECT_LAVA  bright-red */",
  "  {'&'}, /* SECT_SNOW snow */",
].map((line) => {
  const [, code, sector] = /\{'(.)'\}, \/\* SECT_([A-Z_]+)/.exec(line) ?? [];
  return { code, sector: sector.toLowerCase() };
});

// process_color in comm.c, lines 1969 to 1987 with the indent dropped:
// the 16 color codes, each to its index in color_table.
const PROCESS_COLOR = new Map(
  [
    "case '`': c=0; break;",
    "case '1': c=1; break;",
    "case '2': c=2; break;",
    "case '3': c=3; break;",
    "case '4': c=4; break;",
    "case '5': c=5; break;",
    "case '6': c=6; break;",
    "case '7': c=7; break;",
    "case '8': c=16; break;",
    "case '9': c=13; break;",
    "case '0': c=12; break;",
    "case '!': c=9; break;",
    "case '@': c=10; break;",
    "case '#': c=11; break;",
    "case '$': c=12; break;",
    "case ')': c=12; break;",
    "case '%': c=13; break;",
    "case '^': c=14; break;",
    "case '&': c=15; break;",
  ].map((line) => {
    const [, code, index] = /case '(.)': c=(\d+);/.exec(line) ?? [];
    return [code, Number(index)];
  }),
);

// color_table in ansi.h, entries 0 to 16, and the SGR each one names.
// ANSI_ESCAPE opens every one of them with ESC[0, a full reset.
const COLOR_TABLE = [
  ';0', // ANSI_NORMAL
  ';31', // ANSI_RED
  ';32', // ANSI_GREEN
  ';33', // ANSI_YELLOW
  ';34', // ANSI_BLUE
  ';35', // ANSI_PURPLE
  ';36', // ANSI_CYAN
  ';37', // ANSI_WHITE
  ';30', // ANSI_BLACK
  ';1;31', // ANSI_BOLD_RED
  ';1;32', // ANSI_BOLD_GREEN
  ';1;33', // ANSI_BOLD_YELLOW
  ';1;34', // ANSI_BOLD_BLUE
  ';1;35', // ANSI_BOLD_PURPLE
  ';1;36', // ANSI_BOLD_CYAN
  ';1;37', // ANSI_BOLD_WHITE
  ';1;30', // ANSI_BOLD_BLACK
];
const ANSI_ESCAPE = '\x1b[0';

/** The bytes process_color sends for a 16 color code. */
function processColor(code: string): string {
  const index = PROCESS_COLOR.get(code);
  if (index === undefined) throw new Error(`no code ${code}`);
  return `${ANSI_ESCAPE}${COLOR_TABLE[index]}m`;
}

/** The bytes do_look sends for a room's name as the area file stores
 *  it, after the sector's 256 color tint, for a client that asks for
 *  256 colors as Vosh does. */
function doLookName(tint: number, stored: string): string {
  const [, code, words] = /^`(.)(.*)``$/.exec(stored) ?? [];
  return `\x1b[38;5;${tint}m${processColor(code)}${words}${processColor('`')}${processColor('`')}`;
}

/** The slot the terminal draws `words` in, out of a line's bytes. The
 *  slot names stand in for the palette, so the color reads back as the
 *  slot itself. */
function terminalSlot(bytes: string, words: string): string | undefined {
  const span = parseLogLine(bytes).find((s) => s.text === words);
  if (!span) throw new Error(`no ${words}`);
  return logSpanCss(span, ANSI_SLOTS, false).color;
}

// gmcp_send_room's sector_names in gmcp.c, the terrain Room.Info sends
// for each sector index.
const ROOM_INFO_TERRAINS = [
  'inside',
  'city',
  'field',
  'forest',
  'hills',
  'mountain',
  'water_swim',
  'water_noswim',
  'swamp',
  'air',
  'desert',
  'lava',
  'snow',
];

// One room of each sector from the area files the game loads, with its
// name as the file stores it and the sector on its flags line, and the
// tint do_look prints before it.
const AREA_ROOMS = [
  { file: 'winsteel.are', vnum: 10874, stored: '`8Between Ice Bars``', sector: 0, tint: 255 },
  {
    file: 'everwild.are',
    vnum: 210,
    stored: '`7Cloverton Square South-East``',
    sector: 1,
    tint: 229,
  },
  { file: 'everwild.are', vnum: 204, stored: '`3Treeline``', sector: 2, tint: 82 },
  { file: 'limbo.are', vnum: 8, stored: '`2An Empty Garden``', sector: 3, tint: 28 },
  { file: 'everwild.are', vnum: 201, stored: '`3Steep Hill``', sector: 4, tint: 143 },
  { file: 'plains.are', vnum: 346, stored: '`3The Steep Hills``', sector: 5, tint: 245 },
  { file: 'everwild.are', vnum: 248, stored: '`^A Hidden Brook``', sector: 6, tint: 39 },
  { file: 'avalon.are', vnum: 10720, stored: '`4A Misty Lake``', sector: 7, tint: 27 },
  { file: 'everwild.are', vnum: 217, stored: '`8Witch Wood``', sector: 8, tint: 58 },
  { file: 'mountrail.are', vnum: 748, stored: '`6Hanging on the Cliff``', sector: 9, tint: 117 },
  { file: 'hamlet.are', vnum: 1144, stored: '`#A White Beach``', sector: 10, tint: 220 },
  { file: 'artifact.are', vnum: 590, stored: '`1The Heart of the Nexus``', sector: 11, tint: 196 },
  {
    file: 'artifact.are',
    vnum: 550,
    stored: '`&Floating Among The Stars``',
    sector: 12,
    tint: 231,
  },
];

describe('the sector slot table', () => {
  it('draws each sector in the slot its room_color_table code prints in', () => {
    expect(ROOM_COLOR_TABLE.map((r) => r.sector)).toEqual(ROOM_INFO_TERRAINS);
    ROOM_COLOR_TABLE.forEach(({ code, sector }, i) => {
      // Swamps follow the area files, which store every swamp in `8.
      if (sector === 'swamp') return;
      const slot = terminalSlot(`${processColor(code)}Words${processColor('`')}`, 'Words');
      expect(SECTOR_NAME_SLOTS[i], sector).toBe(slot);
    });
    expect(SECTOR_NAME_SLOTS[8]).toBe('brightBlack');
  });

  it('reads a bold code in its bright slot, as the terminal draws it', () => {
    const slot = (code: string) =>
      terminalSlot(`${processColor(code)}Words${processColor('`')}`, 'Words');
    expect(['8', '^', '@', '#', '&'].map(slot)).toEqual([
      'brightBlack',
      'brightCyan',
      'brightGreen',
      'brightYellow',
      'brightWhite',
    ]);
    expect(['7', '3', '2', '4', '6', '1'].map(slot)).toEqual([
      'white',
      'yellow',
      'green',
      'blue',
      'cyan',
      'red',
    ]);
  });

  it('matches the slot the terminal draws a real room of each sector in', () => {
    for (const room of AREA_ROOMS) {
      const label = `${room.file} ${room.vnum}`;
      const words = room.stored.slice(2, -2);
      const info = parseRoomInfo({ name: words, terrain: ROOM_INFO_TERRAINS[room.sector] });
      expect(info?.sector, label).toBe(room.sector);
      // The name's own code resets the tint before the first letter.
      const drawn = terminalSlot(doLookName(room.tint, room.stored), words);
      expect(ANSI_SLOTS).toContain(drawn as AnsiSlot);
      expect(sectorNameSlot(info?.sector ?? null), label).toBe(drawn);
    }
  });

  it('lines up with the terrain Room.Info sends for each sector index', () => {
    ROOM_INFO_TERRAINS.forEach((terrain, i) => {
      const info = parseRoomInfo({ name: 'Between Ice Bars', terrain });
      expect(info?.sector, terrain).toBe(i);
      expect(sectorNameSlot(info?.sector ?? null), terrain).toBe(SECTOR_NAME_SLOTS[i]);
    });
  });

  it('leaves a sector past the table untinted', () => {
    // Room.Info sends -1 for a room with no sector.
    expect(sectorNameSlot(-1)).toBeNull();
    expect(sectorNameSlot(13)).toBeNull();
    expect(sectorNameSlot(1.5)).toBeNull();
    expect(sectorNameSlot(null)).toBeNull();
  });
});

describe('roomNameColor', () => {
  const sectors = ROOM_INFO_TERRAINS.map((_, i) => i);

  it('reads at 4.5:1 on the panel for every sector on every built in theme', () => {
    expect(ROOM_NAME_CONTRAST).toBe(4.5);
    for (const theme of BUILTIN_THEMES) {
      const tokens = themeTokens(theme);
      const panel = hex(tokens.panel);
      for (const sector of sectors) {
        const color = roomNameColor(sector, theme.xterm, tokens);
        expect(color, `${theme.id} ${sector}`).not.toBeNull();
        expect(contrast(hex(color!), panel), `${theme.id} ${sector}`).toBeGreaterThanOrEqual(
          ROOM_NAME_CONTRAST,
        );
      }
    }
  });

  it('reads stronger than the quiet terrain row under it', () => {
    for (const theme of BUILTIN_THEMES) {
      const tokens = themeTokens(theme);
      const panel = hex(tokens.panel);
      const quiet = contrast(hex(tokens.tertiary), panel);
      for (const sector of sectors) {
        const name = contrast(hex(roomNameColor(sector, theme.xterm, tokens)!), panel);
        // The one ground rule holds the quiet tier at 3.1:1 on menus too,
        // a step over the panel, so on the panel it reads up to 4.1:1 and
        // a name at 4.5:1 stands less than 1 above it. The High Contrast
        // pair pins every tier at 7:1 or better, and the name reads as
        // strong as its tier.
        if (quiet >= 7) {
          expect(name, `${theme.id} ${sector}`).toBeGreaterThanOrEqual(7);
          continue;
        }
        expect(name, `${theme.id} ${sector}`).toBeGreaterThan(quiet + 0.4);
      }
    }
  });

  it('keeps a theme color that reads and moves only the lightness of one that does not', () => {
    for (const theme of BUILTIN_THEMES) {
      const tokens = themeTokens(theme);
      const panel = hex(tokens.panel);
      for (const sector of sectors) {
        const label = `${theme.id} ${sector}`;
        const was = hex(theme.xterm[SECTOR_NAME_SLOTS[sector]]);
        const now = hex(roomNameColor(sector, theme.xterm, tokens)!);
        if (contrast(was, panel) >= ROOM_NAME_CONTRAST) {
          expect(now, label).toEqual(was);
          continue;
        }
        const a = rgbToOklch(was);
        const b = rgbToOklch(now);
        expect(b.C, label).toBeLessThan(a.C + 0.003);
        if (a.C >= 0.05) {
          expect(Math.abs(((b.h - a.h + 540) % 360) - 180), label).toBeLessThan(1);
        }
        expect(Math.sign(b.L - a.L), label).toBe(tokens.appearance === 'dark' ? 1 : -1);
      }
    }
  });

  it('lifts the gray of an inside room on a dark theme', () => {
    const theme = findTheme('kanso-zen');
    const tokens = themeTokens(theme);
    expect(tokens.appearance).toBe('dark');
    // Between Ice Bars draws in the theme's bright black in the
    // terminal, which sits too close to the panel to read there.
    const was = hex(theme.xterm.brightBlack);
    expect(contrast(was, hex(tokens.panel))).toBeLessThan(ROOM_NAME_CONTRAST);
    const now = hex(roomNameColor(0, theme.xterm, tokens)!);
    expect(rgbToOklch(now).L).toBeGreaterThan(rgbToOklch(was).L);
  });

  it('darkens the slots that fade on a light theme and keeps their hue', () => {
    const theme = findTheme('rubric');
    const tokens = themeTokens(theme);
    expect(tokens.appearance).toBe('light');
    const panel = hex(tokens.panel);
    // An inside room prints in bright black, a forest in green, and deep
    // water in blue. Each fades on the paper.
    for (const sector of [0, 3, 7]) {
      const was = hex(theme.xterm[SECTOR_NAME_SLOTS[sector]]);
      expect(contrast(was, panel), String(sector)).toBeLessThan(ROOM_NAME_CONTRAST);
      const now = hex(roomNameColor(sector, theme.xterm, tokens)!);
      expect(rgbToOklch(now).L, String(sector)).toBeLessThan(rgbToOklch(was).L);
    }
    // The forest's green stays green rather than turning gray or blue.
    const green = rgbToOklch(hex(theme.xterm.green));
    const forest = rgbToOklch(hex(roomNameColor(3, theme.xterm, tokens)!));
    expect(green.C).toBeGreaterThan(0.05);
    expect(Math.abs(forest.h - green.h)).toBeLessThan(1);
  });

  it('follows the theme, the way the terminal does', () => {
    const kanso = findTheme('kanso-zen');
    const rubric = findTheme('rubric');
    expect(roomNameColor(7, kanso.xterm, themeTokens(kanso))).not.toBe(
      roomNameColor(7, rubric.xterm, themeTokens(rubric)),
    );
    // A slot that reads on the panel draws as the theme gives it.
    const tokens = themeTokens(kanso);
    for (const sector of sectors) {
      const slot = kanso.xterm[SECTOR_NAME_SLOTS[sector]];
      if (contrast(hex(slot), hex(tokens.panel)) >= ROOM_NAME_CONTRAST) {
        expect(roomNameColor(sector, kanso.xterm, tokens)).toBe(slot);
      }
    }
  });

  it('leaves a room past the table in the text color', () => {
    const theme = findTheme('kanso-zen');
    const tokens = themeTokens(theme);
    expect(roomNameColor(null, theme.xterm, tokens)).toBeNull();
    expect(roomNameColor(-1, theme.xterm, tokens)).toBeNull();
    expect(roomNameColor(13, theme.xterm, tokens)).toBeNull();
  });

  it('takes the slot as it is on a panel that does not parse', () => {
    const theme = findTheme('rubric');
    expect(roomNameColor(1, theme.xterm, { panel: 'transparent', appearance: 'light' })).toBe(
      theme.xterm.white,
    );
  });
});

describe('terrainLabel', () => {
  it('names each sector as the game map legend does', () => {
    expect(ROOM_INFO_TERRAINS.map((terrain, i) => terrainLabel(i, terrain))).toEqual([
      'Inside',
      'City',
      'Field',
      'Forest',
      'Hills',
      'Mountain',
      'Water',
      'Deep Water',
      'Swamp',
      'Air',
      'Desert',
      'Lava',
      'Snow',
    ]);
  });

  it('reads an older build that sends only the terrain', () => {
    const info = parseRoomInfo({ name: 'A Misty Lake', terrain: 'water_noswim' });
    expect(terrainLabel(info!.sector, info!.terrain)).toBe('Deep Water');
  });

  it('shows a terrain past the table in its own words, and nothing for unknown', () => {
    expect(terrainLabel(null, 'deep_snow')).toBe('Deep snow');
    expect(terrainLabel(-1, 'unknown')).toBeNull();
    expect(terrainLabel(null, null)).toBeNull();
    expect(terrainLabel(null, ' ')).toBeNull();
  });
});
