import { describe, expect, it } from 'vitest';
import { aabahranMapFixtureNames, aabahranMapPacket } from '../../test/aabahranGmcp';
import {
  DOOR_COLORS,
  corridors,
  getCell,
  glyphGrid,
  gridDims,
  gridRooms,
  hasExit,
  offFloorLayers,
  playerCellOf,
  REACH,
  type Corridor,
  type Dir,
  type GlyphCell,
  type GlyphGrid,
  type MapTilesPayload,
  type StrokeKind,
} from './mapTiles';

// The packets come from fixtures/gmcp/aabahran/map, built by the game's
// own generate_map and gmcp_send_map over its area files. Its README
// says how.

function tiles(name: string): MapTilesPayload {
  const { package: pkg, data } = aabahranMapPacket(name);
  expect(pkg, name).toBe('Map.Tiles');
  return data as MapTilesPayload;
}

/** You stand West of the City Fountain in Caranduin, radius 7. */
const CARANDUIN = 'caranduin-west-of-the-fountain.gmcp';
/** You stand in The Central Square of Val Miran, radius 10. */
const VAL_MIRAN = 'val-miran-central-square.gmcp';
/** You stand at An Enormous Gate in Mahn-Tor's Dungeon, radius 7. */
const MAHN_TOR = 'mahn-tor-an-enormous-gate.gmcp';
/** An immortal stands in the Maw of Malfeascances in The Dark Castle,
 *  radius 10, and sees secret exits. */
const DARK_CASTLE = 'dark-castle-maw-immortal.gmcp';

const DIRS: Record<string, Dir> = { '0,-1': 'n', '1,0': 'e', '0,1': 's', '-1,0': 'w' };

/** The directions the strokes of one kind from one room head, in
 *  stroke order. */
function strokesFrom(
  payload: MapTilesPayload,
  row: number,
  col: number,
  kind: StrokeKind = 'join',
): Dir[] {
  const { rows, cols } = gridDims(payload);
  return corridors(payload, rows, cols)
    .filter((c) => c.row === row && c.col === col && c.kind === kind)
    .map((c) => DIRS[`${c.dx},${c.dy}`]);
}

/** The corridor from one room toward dir, if one is drawn. */
function corridorFrom(
  payload: MapTilesPayload,
  row: number,
  col: number,
  dir: Dir,
): Corridor | undefined {
  const { rows, cols } = gridDims(payload);
  return corridors(payload, rows, cols).find(
    (c) => c.row === row && c.col === col && DIRS[`${c.dx},${c.dy}`] === dir,
  );
}

/** What glyphAt reads for a slot the glyph grid does not have. */
const NOWHERE: GlyphCell = { glyph: '', color: '', isPlayer: false, floor: 'same' };

/** The glyph at a room of the packet's grid, or at the connector slot
 *  dx, dy from it. Found from where the player's @ sits, so it reads
 *  the layout the grid has, wherever its first row starts. */
function glyphAt(grid: GlyphGrid, row: number, col: number, dx = 0, dy = 0): GlyphCell {
  const pr = grid.cells.findIndex((r) => r.some((c) => c.isPlayer));
  const pc = grid.cells[pr].findIndex((c) => c.isPlayer);
  const r = pr + 2 * (row - grid.centerR) + dy;
  const c = pc + 2 * (col - grid.centerC) + dx;
  return grid.cells[r]?.[c] ?? NOWHERE;
}

describe('the Map.Tiles fixtures', () => {
  it('are Map.Tiles packets with a square grid twice the radius plus one wide', () => {
    const names = aabahranMapFixtureNames();
    expect(names.length).toBeGreaterThan(0);
    for (const name of names) {
      const { package: pkg, data } = aabahranMapPacket(name);
      expect(pkg, name).toBe('Map.Tiles');
      const p = data as { r: number; g: unknown[][] };
      expect(p.g.length, name).toBe(2 * p.r + 1);
      for (const row of p.g) expect(row.length, name).toBe(2 * p.r + 1);
    }
  });
});

describe('hasExit', () => {
  it('counts a lowercase letter, which the game sends when the exit lands on the next cell', () => {
    // The Common Road, one step west of you in Caranduin.
    const road = getCell(tiles(CARANDUIN), 7, 6);
    expect(road?.e).toBe('NeSw');
    expect(hasExit(road!, 'e')).toBe(true);
    expect(hasExit(road!, 'w')).toBe(true);
    expect(hasExit(road!, 'n')).toBe(false);
    expect(hasExit(road!, 's')).toBe(false);
  });
});

describe('corridors', () => {
  it('ticks an exit that leads past the room next door and joins no room along it', () => {
    const p = tiles(CARANDUIN);
    // The Common Road leads north to the Potion Shop and south to the
    // Trading Post, but the cells beside it hold the Pill Shop and The
    // Meat Store. Only its east and west exits reach the next cell, and
    // the other two draw ticks.
    expect(getCell(p, 7, 6)?.ex).toEqual({ n: 4508, e: 4406, s: 4514, w: 4404 });
    expect(strokesFrom(p, 7, 6)).toEqual(['e', 'w']);
    expect(strokesFrom(p, 7, 6, 'tick')).toEqual(['n', 's']);
    expect(corridorFrom(p, 7, 6, 'n')).toMatchObject({ kind: 'tick', state: 'open' });
    // Under a Battlement Ladder leads east to The South Road, and the
    // Edge of the Pond beside it leads west to Outside the Monastery.
    // No corridor joins the two, and a tick from each faces the other.
    expect(getCell(p, 9, 2)?.e).toBe('nESu');
    expect(getCell(p, 9, 3)?.e).toBe('esW');
    expect(strokesFrom(p, 9, 2)).toEqual(['n']);
    expect(strokesFrom(p, 9, 3)).toEqual(['e', 's']);
    expect(strokesFrom(p, 9, 2, 'tick')).toEqual(['e', 's']);
    expect(strokesFrom(p, 9, 3, 'tick')).toEqual(['w']);
    // Each tick stops short of the middle of the gap.
    expect(2 * REACH.tick).toBeLessThan(1);
  });

  it('joins every lowercase exit toward a room and ticks every uppercase one', () => {
    for (const name of aabahranMapFixtureNames()) {
      const p = tiles(name);
      const { rows, cols } = gridDims(p);
      const drawn = new Map(
        corridors(p, rows, cols).map((c) => [`${c.row},${c.col},${DIRS[`${c.dx},${c.dy}`]}`, c]),
      );
      let lower = 0;
      for (const { row, col, cell } of gridRooms(p, rows, cols)) {
        for (const letter of cell.e ?? '') {
          const dir = letter.toLowerCase() as Dir;
          if (!'nesw'.includes(dir)) continue;
          const [dx, dy] = Object.entries(DIRS)
            .find(([, d]) => d === dir)![0]
            .split(',')
            .map(Number);
          const next = getCell(p, row + dy, col + dx);
          const key = `${row},${col},${dir}`;
          const stroke = drawn.get(key);
          if (letter === dir && next) {
            lower++;
            expect(stroke?.kind, `${name} ${key}`).toBe('join');
          } else if (cell.d?.[dir] === 'hidden') {
            // A secret exit that leads elsewhere keeps its half stub.
            expect(stroke, `${name} ${key}`).toMatchObject({ kind: 'stub', state: 'hidden' });
          } else if (letter !== dir) {
            expect(stroke, `${name} ${key}`).toMatchObject({ kind: 'tick' });
          } else {
            expect(stroke, `${name} ${key}`).toBeUndefined();
          }
        }
      }
      expect(lower, name).toBeGreaterThan(100);
    }
  });

  it('takes a door from the room beside it only along its exit back', () => {
    const p = tiles(MAHN_TOR);
    // You stand at An Enormous Gate. Its locked door south opens on A
    // Long Tunnel, whose locked door north leads back.
    expect(getCell(p, 7, 7)).toMatchObject({ e: 'su', d: { s: 'locked' } });
    expect(getCell(p, 8, 7)).toMatchObject({ e: 'ns', d: { n: 'locked' } });
    expect(corridorFrom(p, 7, 7, 's')).toMatchObject({
      kind: 'join',
      state: 'locked',
      mutual: true,
    });
    // The next Long Tunnel leads south to A Four-Way Intersection with no
    // door. The closed door north of that intersection leads to another
    // one, so the game sends its letter as N, and the tunnel stays open.
    expect(getCell(p, 9, 7)).toEqual({ s: 0, e: 'ns', l: 1, ar: 45, ex: { n: 18363, s: 18366 } });
    expect(getCell(p, 10, 7)).toMatchObject({
      e: 'Nesw',
      d: { n: 'closed' },
      ex: { n: 18372, s: 18369 },
    });
    expect(corridorFrom(p, 9, 7, 's')).toMatchObject({
      kind: 'join',
      state: 'open',
      mutual: false,
    });
    // That closed door draws a tick of its own color instead.
    expect(corridorFrom(p, 10, 7, 'n')).toMatchObject({ kind: 'tick', state: 'closed' });
  });

  it('joins two rooms at a hidden door only when its exit lands on the other', () => {
    const p = tiles(DARK_CASTLE);
    // Barnok Boulevard's hidden door south opens on A Secret Hideout,
    // whose hidden door north leads back.
    expect(getCell(p, 12, 16)).toMatchObject({
      e: 'Nesw',
      d: { s: 'hidden' },
      ex: { s: 20705 },
    });
    expect(getCell(p, 13, 16)).toMatchObject({ e: 'ns', d: { n: 'hidden' }, ex: { n: 20635 } });
    expect(corridorFrom(p, 12, 16, 's')).toMatchObject({ kind: 'join', state: 'hidden' });
    expect(corridorFrom(p, 13, 16, 'n')).toMatchObject({ kind: 'join', state: 'hidden' });
    // The hidden door south of the Mausoleum of Innocence leads to
    // Ingress of Shadows, past Lord Corim Street in the cell below. The
    // closed door north of the street leads elsewhere too. The mausoleum
    // keeps a stub, the street a closed tick, and no corridor joins the
    // two.
    expect(getCell(p, 12, 9)).toMatchObject({ e: 'nS', d: { s: 'hidden' }, ex: { s: 27062 } });
    expect(getCell(p, 13, 9)).toMatchObject({ e: 'New', d: { n: 'closed' }, ex: { n: 20759 } });
    expect(corridorFrom(p, 12, 9, 's')).toMatchObject({ kind: 'stub', state: 'hidden' });
    expect(corridorFrom(p, 13, 9, 'n')).toMatchObject({ kind: 'tick', state: 'closed' });
    // A cobbled road into the forest has a hidden door south to A
    // Peaceful Clearing, and the cell below it is empty. It keeps a stub.
    expect(getCell(p, 16, 14)).toMatchObject({ e: 'nSW', d: { s: 'hidden' }, ex: { s: 2524 } });
    expect(getCell(p, 17, 14)).toBeNull();
    expect(corridorFrom(p, 16, 14, 's')).toMatchObject({ kind: 'stub', state: 'hidden' });
  });
});

describe('glyphGrid', () => {
  it('joins two rooms only when each exit lands on the other', () => {
    const grid = glyphGrid(tiles(CARANDUIN))!;
    // You and The Common Road west of you lead to each other.
    expect(glyphAt(grid, 7, 6, 1, 0).glyph).toBe('─');
    // Under a Battlement Ladder and the Edge of the Pond both point
    // past each other.
    expect(glyphAt(grid, 9, 2, 1, 0).glyph).toBe(' ');
  });

  it('dashes a hidden door between two rooms only when its exit lands on the other', () => {
    const grid = glyphGrid(tiles(DARK_CASTLE))!;
    // Barnok Boulevard and A Secret Hideout, joined by hidden doors.
    expect(glyphAt(grid, 12, 16, 0, 1)).toMatchObject({ glyph: '╎', color: DOOR_COLORS.hidden });
    // The Mausoleum of Innocence and Lord Corim Street lead past each
    // other.
    expect(glyphAt(grid, 12, 9, 0, 1).glyph).toBe(' ');
    // A cobbled road into the forest, its hidden door south toward an
    // empty cell.
    expect(glyphAt(grid, 16, 14, 0, 1)).toMatchObject({ glyph: '╎', color: DOOR_COLORS.hidden });
  });
});

describe('the top row and the left column', () => {
  it('count every row and column the packet sends', () => {
    // The game sends 2r + 1 rows and columns, indexed from 0, with you
    // at [r][r].
    expect(gridDims(tiles(VAL_MIRAN))).toEqual({ rows: 21, cols: 21 });
    expect(gridDims(tiles(CARANDUIN))).toEqual({ rows: 15, cols: 15 });
  });

  it('find you at [r][r], with or without your h flag', () => {
    expect(playerCellOf(tiles(VAL_MIRAN), 21, 21)).toEqual({ row: 10, col: 10 });
    const unmarked = structuredClone(aabahranMapPacket(VAL_MIRAN).data) as {
      g: Array<Array<{ h?: number } | null>>;
    };
    delete unmarked.g[10][10]!.h;
    const payload = unmarked as unknown as MapTilesPayload;
    expect(playerCellOf(payload, 21, 21)).toEqual({ row: 10, col: 10 });
  });

  it('draw every room the packet sends', () => {
    for (const name of aabahranMapFixtureNames()) {
      const p = tiles(name);
      const { rows, cols } = gridDims(p);
      const sent = (p.g as unknown as Array<Array<unknown>>).flatMap((row, r) =>
        row.flatMap((cell, c) => (cell ? [`${r},${c}`] : [])),
      );
      const drawn = gridRooms(p, rows, cols).map(({ row, col }) => `${row},${col}`);
      expect(drawn, name).toEqual(sent);
      const grid = glyphGrid(p)!;
      for (const key of sent) {
        const [r, c] = key.split(',').map(Number);
        expect(grid.cells[2 * r]?.[2 * c]?.glyph ?? '', `${name} ${key}`).toMatch(/\S/);
      }
    }
  });

  it('draw the room ten steps north and the room ten steps west of Val Miran', () => {
    const p = tiles(VAL_MIRAN);
    const { rows, cols } = gridDims(p);
    // The Forest's Edge, in Elium Forest, tops the grid. Its south exit
    // reaches the room below it.
    expect(getCell(p, 0, 10)?.e).toBe('Ns');
    expect(strokesFrom(p, 0, 10)).toEqual(['s']);
    // A trail through the light forest, in Haon Dor, starts the grid on
    // the left. Its east exit reaches the room beside it.
    expect(getCell(p, 10, 0)?.e).toBe('eW');
    expect(strokesFrom(p, 10, 0)).toEqual(['e']);
    expect(gridRooms(p, rows, cols)[0]).toMatchObject({ row: 0, col: 10 });
    const grid = glyphGrid(p)!;
    expect(grid.cells[20][20].isPlayer).toBe(true);
    // Both are forest, joined to their neighbors.
    expect(glyphAt(grid, 0, 10).glyph).toBe('*');
    expect(glyphAt(grid, 0, 10, 0, 1).glyph).toBe('│');
    expect(glyphAt(grid, 10, 0).glyph).toBe('*');
    expect(glyphAt(grid, 10, 0, 1, 0).glyph).toBe('─');
  });
});

describe('offFloorLayers', () => {
  it('gives each room above or below you one spot', () => {
    for (const name of aabahranMapFixtureNames()) {
      const spots = offFloorLayers(tiles(name))
        .flat()
        .map((e) => `${e.x},${e.y},${e.z}`);
      expect(spots.length, name).toBeGreaterThan(0);
      expect(new Set(spots).size, name).toBe(spots.length);
    }
  });

  it('draws a room that a or b sends again in zr once, from zr', () => {
    const p = tiles(VAL_MIRAN);
    // Before the Temple of Neutrality sits right above you, its down
    // exit back to your room, and Beneath the Suspension Bridge sits six
    // steps south and one floor down. The game sends each in a or b and
    // again in zr, at the same spot.
    expect(p.a).toContainEqual({ x: 10, y: 10, s: 0, e: 'nd' });
    expect(p.zr).toContainEqual(
      expect.objectContaining({ x: 10, y: 10, z: 1, e: 'nd', ex: { n: 20784, d: 20605 } }),
    );
    expect(p.b).toContainEqual({ x: 10, y: 16, s: 7, e: 'ewu' });
    expect(p.zr).toContainEqual(
      expect.objectContaining({
        x: 10,
        y: 16,
        z: -1,
        e: 'ewu',
        ex: { e: 20817, w: 20770, u: 20741 },
      }),
    );
    // Every room of a and b comes again in zr there.
    expect(offFloorLayers(p)).toEqual([[], [], p.zr]);
  });

  it('keeps the rooms above or below you that only a or b carries', () => {
    const p = tiles(CARANDUIN);
    // The zr search counts the climb against your radius. It climbs at
    // Inside the West Gate, five steps west, and stops short of the two
    // Under a Battlement Ladder rooms seven steps away. The a list climbs
    // them too. It holds Atop a Battlement Ladder over each, and On the
    // Battlements beyond each, in a cell with no room on your floor.
    expect(getCell(p, 4, 2)).toBeNull();
    expect(getCell(p, 10, 2)).toBeNull();
    const [above, below, searched] = offFloorLayers(p);
    expect(above).toEqual([
      { x: 2, y: 5, s: 1, e: 'nsd', z: 1 },
      { x: 2, y: 4, s: 1, e: 'ns', z: 1 },
      { x: 2, y: 9, s: 1, e: 'nsd', z: 1 },
      { x: 2, y: 10, s: 1, e: 'ns', z: 1 },
    ]);
    expect(below).toEqual([]);
    expect(searched).toEqual(p.zr);
    // zr holds Atop the West Gate and the battlements beside it, so the
    // wall walk reads whole.
    const wall = [above, searched]
      .flat()
      .filter((e) => e.x === 2 && e.z === 1)
      .map((e) => e.y)
      .sort((a, b) => a - b);
    expect(wall).toEqual([4, 5, 6, 7, 8, 9, 10]);
  });

  it('draws a and b alone when the game sends no zr', () => {
    // The game leaves zr out when its search finds no room.
    const bare = structuredClone(tiles(VAL_MIRAN));
    delete bare.zr;
    expect(offFloorLayers(bare)).toEqual([
      bare.a!.map((e) => ({ ...e, z: 1 })),
      bare.b!.map((e) => ({ ...e, z: -1 })),
      [],
    ]);
  });
});
