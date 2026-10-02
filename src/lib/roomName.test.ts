import { describe, expect, it } from 'vitest';
import { contrast, parseHex, rgbToOklch, type Rgb } from './color';
import { logColorCss } from './logView';
import {
  ROOM_NAME_CONTRAST,
  SECTOR_NAME_COLORS,
  roomNameColor,
  sectorNameCode,
  sectorNameTint,
  terrainLabel,
} from './roomName';
import { parseRoomInfo } from './stores/roomStore';
import { BUILTIN_THEMES, findTheme, themeTokens } from './themes';

const hex = (h: string): Rgb => {
  const c = parseHex(h);
  if (!c) throw new Error(`not hex ${h}`);
  return c;
};

// The sector_colors table in do_look, act_info.c lines 2428 to 2442 of
// the forsaken_lands source, copied line for line with the leading tabs
// dropped. The tab inside the WATER_NOSWIM line stays.
const DO_LOOK_SECTOR_COLORS = [
  'static const int sector_colors[] = {',
  '  /* INSIDE */     255, /* white */',
  '  /* CITY */       229, /* light yellow */',
  '  /* FIELD */       82, /* bright green */',
  '  /* FOREST */      28, /* dark green */',
  '  /* HILLS */      143, /* olive/khaki */',
  '  /* MOUNTAIN */   245, /* grey */',
  '  /* WATER_SWIM */  39, /* light blue */',
  '  /* WATER_NOSWIM*/\t27, /* deep blue */',
  '  /* SWAMP */       58, /* dark olive */',
  '  /* AIR */        117, /* sky blue */',
  '  /* DESERT */     220, /* gold/sandy */',
  '  /* LAVA */       196, /* bright red */',
  '  /* SNOW */       231, /* bright white */',
  '};',
].join('\n');

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

describe('the sector tint table', () => {
  const rows = [...DO_LOOK_SECTOR_COLORS.matchAll(/\/\* ?([A-Z_]+) ?\*\/\s*(\d+),/g)].map(
    ([, sector, code]) => ({ sector: sector.toLowerCase(), code: Number(code) }),
  );

  it('holds the codes of do_look in act_info.c, in its order', () => {
    expect(rows).toHaveLength(13);
    expect(SECTOR_NAME_COLORS).toEqual(rows.map((r) => r.code));
  });

  it('lines up with the terrain Room.Info sends for each sector index', () => {
    expect(rows.map((r) => r.sector)).toEqual(ROOM_INFO_TERRAINS);
    ROOM_INFO_TERRAINS.forEach((terrain, i) => {
      const info = parseRoomInfo({ name: 'Between Ice Bars', terrain });
      expect(info?.sector, terrain).toBe(i);
      expect(sectorNameCode(info?.sector ?? null), terrain).toBe(rows[i].code);
    });
  });

  it('tints only the sectors do_look tints', () => {
    // do_look prints the name untinted outside 0 to 12, and Room.Info
    // sends -1 for a room with no sector.
    expect(sectorNameCode(-1)).toBeNull();
    expect(sectorNameCode(13)).toBeNull();
    expect(sectorNameCode(1.5)).toBeNull();
    expect(sectorNameCode(null)).toBeNull();
  });

  it('draws each tint from the 256 color cube the terminal draws it from', () => {
    expect(ROOM_INFO_TERRAINS.map((_, i) => sectorNameTint(i))).toEqual([
      '#eeeeee',
      '#ffffaf',
      '#5fff00',
      '#008700',
      '#afaf5f',
      '#8a8a8a',
      '#00afff',
      '#005fff',
      '#5f5f00',
      '#87d7ff',
      '#ffd700',
      '#ff0000',
      '#ffffff',
    ]);
    // The log view draws the same codes from the same cube.
    SECTOR_NAME_COLORS.forEach((code, i) => {
      const css = logColorCss(code, []);
      const [r, g, b] = (css.match(/\d+/g) ?? []).map(Number);
      expect(hex(sectorNameTint(i)!), css).toEqual({ r, g, b });
    });
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
        const color = roomNameColor(sector, tokens);
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
        const name = contrast(hex(roomNameColor(sector, tokens)!), panel);
        expect(name, `${theme.id} ${sector}`).toBeGreaterThan(quiet + 1);
      }
    }
  });

  it('keeps a tint that reads and moves only the lightness of one that does not', () => {
    for (const theme of BUILTIN_THEMES) {
      const tokens = themeTokens(theme);
      const panel = hex(tokens.panel);
      for (const sector of sectors) {
        const label = `${theme.id} ${sector}`;
        const was = hex(sectorNameTint(sector)!);
        const now = hex(roomNameColor(sector, tokens)!);
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

  it('darkens the pale tints on a light theme and keeps their hue', () => {
    const tokens = themeTokens(findTheme('vellum'));
    expect(tokens.appearance).toBe('light');
    const panel = hex(tokens.panel);
    // Inside is white, a city light yellow, the sky blue air, and snow
    // bright white. Each fades on the paper.
    for (const sector of [0, 1, 9, 12]) {
      const was = hex(sectorNameTint(sector)!);
      expect(contrast(was, panel), String(sector)).toBeLessThan(ROOM_NAME_CONTRAST);
      const now = hex(roomNameColor(sector, tokens)!);
      expect(rgbToOklch(now).L, String(sector)).toBeLessThan(rgbToOklch(was).L);
    }
    // The light yellow of a city stays yellow rather than turning olive
    // or orange.
    const city = rgbToOklch(hex(roomNameColor(1, tokens)!));
    expect(Math.abs(city.h - rgbToOklch(hex('#ffffaf')).h)).toBeLessThan(1);
  });

  it('lifts the deep tints on a dark theme', () => {
    const tokens = themeTokens(findTheme('kanso-zen'));
    expect(tokens.appearance).toBe('dark');
    // A swamp's dark olive fades into a dark panel, while a field's
    // bright green already reads and keeps the terminal's color.
    expect(roomNameColor(8, tokens)).not.toBe('#5f5f00');
    expect(roomNameColor(2, tokens)).toBe('#5fff00');
    expect(roomNameColor(0, tokens)).toBe('#eeeeee');
  });

  it('leaves a room the game prints untinted in the text color', () => {
    const tokens = themeTokens(findTheme('kanso-zen'));
    expect(roomNameColor(null, tokens)).toBeNull();
    expect(roomNameColor(-1, tokens)).toBeNull();
    expect(roomNameColor(13, tokens)).toBeNull();
  });

  it('takes the tint as it is on a panel that does not parse', () => {
    expect(roomNameColor(1, { panel: 'transparent', appearance: 'light' })).toBe('#ffffaf');
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
