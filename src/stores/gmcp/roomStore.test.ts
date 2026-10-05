import { describe, expect, it } from 'vitest';
import {
  groupPeople,
  parseExits,
  parseMapAreas,
  parsePeople,
  parseRegion,
  parseRoomInfo,
  resolveArea,
} from './roomStore';

describe('parseRoomInfo', () => {
  it('reads the Aabahran payload', () => {
    expect(
      parseRoomInfo({
        num: 5279,
        name: 'The Bank of Aabahran',
        area: 'Blackwatch Village',
        terrain: 'inside',
        sector: 0,
        region: 0,
        climate: 'temperate',
        exits: { south: 5271 },
      }),
    ).toEqual({
      name: 'The Bank of Aabahran',
      vnum: 5279,
      area: 'Blackwatch Village',
      exits: ['south'],
      sector: 0,
      terrain: 'inside',
      region: 'temperate',
    });
  });

  it('reads no region from a build that sends none', () => {
    // The made up room under rhapsody of delusion, and the older builds.
    expect(
      parseRoomInfo({ num: 0, name: 'A Wondrous Place', terrain: 'unknown', sector: -1 })?.region,
    ).toBeNull();
    expect(parseRoomInfo({ name: 'Between Ice Bars', terrain: 'inside' })?.region).toBeNull();
  });

  it('falls back to the terrain name when the sector is missing or unknown', () => {
    expect(parseRoomInfo({ name: 'Road', terrain: 'forest' })?.sector).toBe(3);
    expect(parseRoomInfo({ name: 'Road', terrain: 'FOREST', sector: -1 })?.sector).toBe(3);
    expect(parseRoomInfo({ name: 'Road', terrain: 'unknown', sector: -1 })?.sector).toBeNull();
  });

  it('needs a room name', () => {
    expect(parseRoomInfo({ num: 1 })).toBeNull();
    expect(parseRoomInfo(null)).toBeNull();
  });
});

describe('parseRegion', () => {
  it('takes the climate name, or else names the region index as tables.c does', () => {
    expect(parseRegion('Coastal North', 1)).toBe('Coastal North');
    expect(parseRegion(undefined, 0)).toBe('Temperate');
    expect(parseRegion(null, 4)).toBe('Tundra');
    expect(parseRegion('', '7')).toBe('Mountain East');
  });

  it('names nothing for an index past the table', () => {
    expect(parseRegion(undefined, 8)).toBeNull();
    expect(parseRegion(undefined, -1)).toBeNull();
    expect(parseRegion(undefined, 1.5)).toBeNull();
    expect(parseRegion(undefined, undefined)).toBeNull();
  });
});

describe('parseExits', () => {
  it('lists open exits in compass order, then the rest', () => {
    expect(parseExits({ down: 9, northeast: 7, west: 3, north: 1, up: 0, south: null })).toEqual([
      'north',
      'west',
      'down',
      'northeast',
    ]);
    expect(parseExits(['north'])).toEqual([]);
  });
});

describe('people', () => {
  it('reads Room.Chars and the npc flag in its several spellings', () => {
    expect(
      parsePeople([
        { name: 'A Blackwatch villager', npc: true },
        { name: 'Orla', npc: false },
        { name: 'a rat', npc: '1' },
        { name: '' },
        'junk',
      ]),
    ).toEqual([
      { name: 'A Blackwatch villager', npc: true },
      { name: 'Orla', npc: false },
      { name: 'a rat', npc: true },
    ]);
    expect(parsePeople({})).toEqual([]);
  });

  it('folds duplicates and keeps their Room.Chars positions', () => {
    const groups = groupPeople([
      { name: 'a rat', npc: true },
      { name: 'Orla', npc: false },
      { name: 'a rat', npc: true },
    ]);
    expect(groups).toEqual([
      { name: 'a rat', count: 2, npc: true, positions: [1, 3] },
      { name: 'Orla', count: 1, npc: false, positions: [2] },
    ]);
  });
});

describe('area lookup', () => {
  const tiles = parseMapAreas({
    r: 1,
    g: [
      [null, { s: 1, e: 's', ar: 52 }],
      [{ s: 0, e: 'n', h: 1, ar: 52 }, null],
    ],
    areas: {
      '52': { name: 'Blackwatch Village', color: '#a3be8c' },
      '7': { name: 'Northern Road', color: 'not a color' },
    },
  });

  it('keys the areas dict by vnum and finds the cell you stand on', () => {
    expect(tiles).toEqual({
      areas: {
        '52': { name: 'Blackwatch Village', color: '#a3be8c' },
        '7': { name: 'Northern Road', color: null },
      },
      here: 52,
    });
  });

  it('resolves the Room.Info area name through the vnum keyed dict', () => {
    expect(resolveArea('Blackwatch Village', tiles)).toEqual({ vnum: 52, color: '#a3be8c' });
    expect(resolveArea('northern road', tiles)).toEqual({ vnum: 7, color: null });
    expect(resolveArea(null, tiles)).toEqual({ vnum: 52, color: '#a3be8c' });
  });

  it('finds nothing for an unknown area or before Map.Tiles arrives', () => {
    expect(resolveArea('Temple of Aabahran', tiles)).toBeNull();
    expect(resolveArea('Blackwatch Village', null)).toBeNull();
  });
});
