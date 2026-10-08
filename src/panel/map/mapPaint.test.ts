import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { aabahranMapPacket } from '../../test/aabahranGmcp';
import { hexToRgba } from '../../theme/color';
import { MAP_COLORS, roomFill, sectorForCode } from './mapPalette';
import { offerOf, planWalk, walkAhead, type WalkMark } from './mapWalk';
import {
  DOOR_COLORS,
  corridors,
  gridDims,
  gridRooms,
  offFloorLayers,
  playerCellOf,
  sectorCodeOf,
  type GlyphCell,
  type MapTilesPayload,
} from './mapTiles';
import {
  cellClass,
  computeAnchor,
  depthAlphaForRing,
  drawOffFloorOverlay,
  drawSquares,
  drawTileset,
  gridPlace,
  roomAt,
} from './mapPaint';
import { GlyphsOverlay } from './GlyphsOverlay';

// The packets come from fixtures/gmcp/aabahran/map, built by the game's
// own generate_map and gmcp_send_map over its area files.

function tiles(name: string): MapTilesPayload {
  return aabahranMapPacket(name).data as MapTilesPayload;
}

/** You stand West of the City Fountain in Caranduin, radius 7, with a
 *  floor above. */
const CARANDUIN = tiles('caranduin-west-of-the-fountain.gmcp');
/** You stand in The Central Square of Val Miran, radius 10, with a
 *  floor below and two above. */
const VAL_MIRAN = tiles('val-miran-central-square.gmcp');
/** You stand at An Enormous Gate in Mahn-Tor's Dungeon, radius 7. */
const MAHN_TOR = tiles('mahn-tor-an-enormous-gate.gmcp');
/** An immortal stands in the Maw of Malfeascances in The Dark Castle,
 *  radius 10, and sees secret exits. */
const DARK_CASTLE = tiles('dark-castle-maw-immortal.gmcp');

const ALL = { CARANDUIN, VAL_MIRAN, MAHN_TOR, DARK_CASTLE };

// MAP_COLORS and the mark face read the theme off the root as the map
// draws, so each test hands them a root with colors of its own.
const GROUND = '#1b1e24';
const ACCENT = '#5fb3a1';
const ACCENT_SOFT = 'rgba(95, 179, 161, 0.13)';
const FAINT = '#7c8394';
const DANGER = '#e0715f';
const SECONDARY = '#a3a9b8';
const MARK_FACE = 'Iosevka, monospace';
const VARS: Record<string, string> = {
  '--panel': GROUND,
  '--accent': ACCENT,
  '--accent-soft': ACCENT_SOFT,
  '--tertiary': FAINT,
  '--danger': DANGER,
  '--secondary': SECONDARY,
  '--font-panel-mark': MARK_FACE,
};

beforeEach(() => {
  vi.stubGlobal('document', { documentElement: { dataset: {} } });
  vi.stubGlobal('getComputedStyle', () => ({
    getPropertyValue: (name: string) => VARS[name] ?? '',
  }));
});

afterEach(() => {
  vi.unstubAllGlobals();
});

type Pt = [number, number];

interface Call {
  op: string;
  args: unknown[];
  /** What the canvas held as the call ran. */
  state: Record<string, unknown>;
  /** The lines a stroke draws, each from its moveTo or last lineTo. */
  lines?: [Pt, Pt][];
}

/** A canvas that keeps each call in order with the state it ran in,
 *  restores that state as the canvas does, and draws nothing. */
function recorder() {
  const calls: Call[] = [];
  let state: Record<string, unknown> = { globalAlpha: 1, lineDash: [] };
  const saved: Record<string, unknown>[] = [];
  let lines: [Pt, Pt][] = [];
  let at: Pt = [0, 0];
  const effects: Record<string, (args: unknown[]) => void> = {
    save: () => saved.push({ ...state }),
    restore: () => {
      state = saved.pop() ?? state;
    },
    setLineDash: ([dash]) => {
      state.lineDash = [...(dash as number[])];
    },
    beginPath: () => {
      lines = [];
    },
    moveTo: (args) => {
      at = args as Pt;
    },
    lineTo: (args) => {
      lines.push([at, args as Pt]);
      at = args as Pt;
    },
  };
  const ctx = new Proxy(
    {},
    {
      get: (_target, key) => {
        if (typeof key !== 'string') return undefined;
        if (key in state) return state[key];
        return (...args: unknown[]) => {
          effects[key]?.(args);
          calls.push({
            op: key,
            args,
            state: { ...state },
            ...(key === 'stroke' ? { lines: [...lines] } : {}),
          });
        };
      },
      set: (_target, key, value) => {
        state[String(key)] = value;
        return true;
      },
    },
  );
  return { ctx: ctx as unknown as CanvasRenderingContext2D, calls };
}

const W = 301;
const H = 349;

/** Everything a 2D style needs to paint one packet at one zoom. */
function scene(payload: MapTilesPayload, zoom = 1) {
  const { rows, cols } = gridDims(payload);
  const { row: centerR, col: centerC } = playerCellOf(payload, rows, cols);
  const anchor = computeAnchor(W, H, zoom);
  const { pitch, playerX, playerY } = anchor;
  return {
    rows,
    cols,
    centerR,
    centerC,
    anchor,
    pitch,
    size: Math.max(8, Math.floor(pitch * 0.55)),
    ox: Math.floor(playerX - centerC * pitch),
    oy: Math.floor(playerY - centerR * pitch),
  };
}

function squares(payload: MapTilesPayload, zoom = 1, walk: WalkMark | null = null) {
  const s = scene(payload, zoom);
  const { ctx, calls } = recorder();
  drawSquares(
    ctx,
    payload,
    s.rows,
    s.cols,
    s.centerR,
    s.centerC,
    s.anchor,
    GROUND,
    MARK_FACE,
    walk,
  );
  return { ...s, calls };
}

/** The square of side size centered on (cx, cy), as fillRect and
 *  strokeRect take it. */
const box = (cx: number, cy: number, size: number) => [cx - size / 2, cy - size / 2, size, size];

const sameBox = (call: Call, rect: number[]) =>
  call.args.length === 4 && call.args.every((v, i) => v === rect[i]);

describe('the Squares painter', () => {
  it('puts your room at the middle of the canvas in the accent, sized by the zoom', () => {
    // zoom, then the pitch floor(20 * zoom) and the side floor(pitch *
    // 0.55), never under 8.
    const steps: [number, number, number][] = [
      [0.5, 10, 8],
      [1, 20, 11],
      [1.25, 25, 13],
      [2, 40, 22],
      [3, 60, 33],
    ];
    for (const [zoom, pitch, size] of steps) {
      const { anchor, calls } = squares(VAL_MIRAN, zoom);
      expect(anchor.pitch, `zoom ${zoom}`).toBe(pitch);
      const rect = box(150, 174, size);
      const fills = calls.filter((c) => c.op === 'fillRect' && c.state.fillStyle === ACCENT_SOFT);
      expect(
        fills.map((c) => c.args),
        `zoom ${zoom}`,
      ).toEqual([rect]);
      expect(fills[0].state.globalAlpha).toBe(1);
      const strokes = calls.filter((c) => c.op === 'strokeRect' && c.state.strokeStyle === ACCENT);
      expect(
        strokes.map((c) => c.args),
        `zoom ${zoom}`,
      ).toEqual([rect]);
      expect(strokes[0].state.lineWidth).toBe(1);
    }
  });

  it('fades each room by its ring, the steps from your room', () => {
    expect([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 14].map(depthAlphaForRing)).toEqual([
      1, 0.9, 0.9, 0.72, 0.72, 0.55, 0.55, 0.4, 0.4, 0.4, 0.28, 0.28,
    ]);
    for (const [name, payload] of Object.entries(ALL)) {
      const { rows, cols, centerR, centerC, ox, oy, pitch, size, calls } = squares(payload);
      const seen = new Set<number>();
      for (const { row, col, cell } of gridRooms(payload, rows, cols)) {
        if (row === centerR && col === centerC) continue;
        const where = `${name} at ${row}, ${col}`;
        const rect = box(ox + col * pitch, oy + row * pitch, size);
        const sector = sectorForCode(sectorCodeOf(cell.s));
        const alpha = depthAlphaForRing(Math.abs(row - centerR) + Math.abs(col - centerC));
        seen.add(alpha);
        const fill = calls.filter((c) => c.op === 'fillRect' && sameBox(c, rect)).at(-1);
        expect(fill?.state.fillStyle, where).toBe(roomFill(sector, GROUND, false));
        expect(fill?.state.globalAlpha, where).toBe(alpha);
        const edge = calls.filter((c) => c.op === 'strokeRect' && sameBox(c, rect)).at(-1);
        expect(edge?.state.strokeStyle, where).toBe(hexToRgba(sector.border, 0.8));
        expect(edge?.state.globalAlpha, where).toBe(alpha);
      }
      expect(seen.size, name).toBeGreaterThan(3);
    }
  });

  it('dashes the corridors of hidden doors 3 on 3, and no other', () => {
    const { rows, cols, calls } = squares(DARK_CASTLE);
    const hidden = corridors(DARK_CASTLE, rows, cols).filter((c) => c.state === 'hidden');
    expect(hidden.length).toBeGreaterThan(0);
    const strokes = calls.filter((c) => c.op === 'stroke');
    const dashed = strokes.filter((c) => c.state.strokeStyle === DOOR_COLORS.hidden);
    expect(dashed).toHaveLength(1);
    expect(dashed[0].state.lineDash).toEqual([3, 3]);
    expect(dashed[0].lines).toHaveLength(hidden.length);
    for (const c of strokes) {
      if (c.state.strokeStyle !== DOOR_COLORS.hidden) expect(c.state.lineDash).toEqual([]);
    }
  });

  it('fills the rooms on other floors in their sector border at 0.22', () => {
    for (const [name, payload] of Object.entries({ CARANDUIN, VAL_MIRAN, DARK_CASTLE })) {
      const { ox, oy, pitch, size, calls } = squares(payload);
      const rooms = offFloorLayers(payload).flat();
      expect(rooms.length, name).toBeGreaterThan(0);
      const faint = calls.filter((c) => c.op === 'fillRect' && c.state.globalAlpha === 0.22);
      expect(faint, name).toHaveLength(rooms.length);
      for (const room of rooms) {
        const rect = box(ox + room.x * pitch, oy + room.y * pitch, size);
        const border = sectorForCode(sectorCodeOf(room.s)).border;
        const where = `${name} at ${room.x}, ${room.y}, ${room.z}`;
        expect(
          faint.some((c) => sameBox(c, rect) && c.state.fillStyle === border),
          where,
        ).toBe(true);
      }
    }
  });

  it('joins the rooms on another floor a whole step, and stubs an exit half a step', () => {
    // At a pitch of 25 the stub floors to 12.
    const { ox, oy, pitch } = scene(VAL_MIRAN, 1.25);
    for (const layer of offFloorLayers(VAL_MIRAN)) {
      const { ctx, calls } = recorder();
      drawOffFloorOverlay(ctx, layer, ox, oy, pitch);
      if (layer.length === 0) {
        expect(calls).toEqual([]);
        continue;
      }
      const strokes = calls.filter((c) => c.op === 'stroke');
      expect(strokes).toHaveLength(1);
      expect(strokes[0].state).toMatchObject({
        strokeStyle: '#474b55',
        globalAlpha: 1,
        lineWidth: 1.25,
      });
      const spots = new Set(layer.map((e) => `${e.x},${e.y}`));
      const expected: [Pt, Pt][] = [];
      for (const e of layer) {
        const from: Pt = [ox + e.x * pitch, oy + e.y * pitch];
        const exits = (e.e ?? '').toLowerCase();
        for (const [dir, dx, dy] of [
          ['n', 0, -1],
          ['s', 0, 1],
          ['e', 1, 0],
          ['w', -1, 0],
        ] as const) {
          if (!exits.includes(dir)) continue;
          const reach = spots.has(`${e.x + dx},${e.y + dy}`) ? pitch : Math.floor(pitch / 2);
          expected.push([from, [from[0] + dx * reach, from[1] + dy * reach]]);
        }
      }
      expect(strokes[0].lines).toEqual(expected);
    }
  });

  it('marks up and down exits with ▲ and ▼ in the mark face, but not in your room', () => {
    let drawn = 0;
    for (const [name, payload] of Object.entries(ALL)) {
      const { rows, cols, centerR, centerC, ox, oy, pitch, size, calls } = squares(payload);
      const marks = calls.filter((c) => c.op === 'fillText');
      const expected: unknown[][] = [];
      for (const { row, col, cell } of gridRooms(payload, rows, cols)) {
        if (row === centerR && col === centerC) continue;
        const exits = (cell.e ?? '').toLowerCase();
        const cx = ox + col * pitch;
        const cy = oy + row * pitch;
        if (exits.includes('u')) expected.push(['▲', cx, cy - size * 0.25]);
        if (exits.includes('d')) expected.push(['▼', cx, cy + size * 0.25]);
      }
      expect(
        marks.map((c) => c.args),
        name,
      ).toEqual(expected);
      for (const c of marks) {
        expect(c.state).toMatchObject({
          fillStyle: FAINT,
          font: `${Math.max(7, Math.floor(size * 0.55))}px ${MARK_FACE}`,
          textAlign: 'center',
          textBaseline: 'middle',
        });
      }
      drawn += marks.length;
    }
    expect(drawn).toBeGreaterThan(0);
  });
});

/** The index of the last call that matches, or -1. */
function lastIndex(calls: Call[], match: (call: Call) => boolean): number {
  for (let i = calls.length - 1; i >= 0; i--) if (match(calls[i])) return i;
  return -1;
}

describe('the walk under the pointer', () => {
  /** The walk to the room at row, col of the Val Miran packet. */
  function walkTo(row: number, col: number): WalkMark {
    const plan = planWalk(VAL_MIRAN, row, col);
    if (!plan) throw new Error(`no walk to ${row},${col}`);
    return offerOf(plan, { row, col });
  }

  it('strokes the path in pathLine at 2 px from your room before the room fills', () => {
    // 4n2e, from The Central Square at [10][10] to [6][12].
    const walk = walkTo(6, 12);
    const { ox, oy, pitch, calls } = squares(VAL_MIRAN, 1, walk);
    const at = (row: number, col: number): Pt => [ox + col * pitch, oy + row * pitch];
    const path = calls.findIndex(
      (c) => c.op === 'stroke' && c.state.strokeStyle === MAP_COLORS.pathLine,
    );
    expect(path).toBeGreaterThan(-1);
    expect(calls[path].state.lineWidth).toBe(2);
    expect(calls[path].lines).toEqual([
      [at(10, 10), at(9, 10)],
      [at(9, 10), at(8, 10)],
      [at(8, 10), at(7, 10)],
      [at(7, 10), at(6, 10)],
      [at(6, 10), at(6, 11)],
      [at(6, 11), at(6, 12)],
    ]);
    const firstRoom = calls.findIndex((c) => c.op === 'fillRect' && c.state.fillStyle === GROUND);
    expect(path).toBeLessThan(firstRoom);
    const lastCorridor = lastIndex(
      calls,
      (c) => c.op === 'stroke' && c.state.strokeStyle === DOOR_COLORS.open,
    );
    expect(path).toBeGreaterThan(lastCorridor);
  });

  it('outlines each room on the path in pathLine at full alpha', () => {
    const walk = walkTo(6, 12);
    const { ox, oy, pitch, size, calls } = squares(VAL_MIRAN, 1, walk);
    const lit = calls.filter(
      (c) => c.op === 'strokeRect' && c.state.strokeStyle === MAP_COLORS.pathLine,
    );
    const byPlace = (x: unknown[], y: unknown[]) => String(x).localeCompare(String(y));
    expect(lit.map((c) => c.args).sort(byPlace)).toEqual(
      walk.cells.map(({ row, col }) => box(ox + col * pitch, oy + row * pitch, size)).sort(byPlace),
    );
    expect(lit.every((c) => c.state.globalAlpha === 1 && c.state.lineWidth === 1)).toBe(true);
  });

  it('rings the room in the accent 3.5 px outside its square, after the rooms', () => {
    const { ox, oy, pitch, size, calls } = squares(VAL_MIRAN, 1, walkTo(6, 12));
    const ring = calls.findIndex((c) => c.op === 'roundRect');
    const half = size / 2 + 3.5;
    expect(calls[ring].args).toEqual([
      ox + 12 * pitch - half,
      oy + 6 * pitch - half,
      size + 7,
      size + 7,
      3,
    ]);
    const stroke = calls[ring + 1];
    expect(stroke.op).toBe('stroke');
    expect(stroke.state).toMatchObject({ strokeStyle: ACCENT, lineWidth: 1.5, lineDash: [] });
    expect(ring).toBeGreaterThan(lastIndex(calls, (c) => c.op === 'strokeRect'));
  });

  it('dashes a danger ring 3 on 2 around a room past a locked door', () => {
    // The only way into [7][15] is the locked door south of [6][15].
    const walk = walkTo(7, 15);
    expect(walk.kind).toBe('door');
    const { calls } = squares(VAL_MIRAN, 1, walk);
    const ring = calls.findIndex((c) => c.op === 'roundRect');
    expect(calls[ring + 1].state).toMatchObject({
      strokeStyle: DANGER,
      lineWidth: 1.5,
      lineDash: [3, 2],
    });
  });

  it('draws nothing of a walk with none to show', () => {
    const { calls } = squares(VAL_MIRAN);
    expect(calls.some((c) => c.op === 'roundRect')).toBe(false);
    expect(calls.some((c) => c.state.strokeStyle === MAP_COLORS.pathLine)).toBe(false);
  });

  it('finds the room under the pointer where each flat style draws it', () => {
    const { centerR, centerC, ox, oy, pitch } = scene(VAL_MIRAN);
    const squaresAt = gridPlace('squares', W, H, 1, centerR, centerC, false);
    expect(squaresAt).toEqual({ ox, oy, pitch, size: 11 });
    expect(roomAt(squaresAt, ox + 12 * pitch + 4, oy + 6 * pitch - 6)).toEqual({ row: 6, col: 12 });
    expect(gridPlace('tileset', W, H, 1, centerR, centerC, true).size).toBe(pitch);
    // Glyphs centers your room's 1 em box on the drawing, 14 px a room
    // and 4.9 px a bridge at zoom 1.
    const glyphs = gridPlace('glyphs', W, H, 1, centerR, centerC, false);
    expect(glyphs.pitch).toBeCloseTo(18.9);
    expect(glyphs.size).toBe(14);
    expect(roomAt(glyphs, W / 2, H / 2)).toEqual({ row: centerR, col: centerC });
    expect(roomAt(glyphs, W / 2 + 2 * 18.9 + 5, H / 2 - 4 * 18.9)).toEqual({
      row: centerR - 4,
      col: centerC + 2,
    });
  });
});

describe('a walk a click sent', () => {
  const plan = planWalk(VAL_MIRAN, 6, 12);
  if (!plan) throw new Error('no walk to 6,12');
  /** The room you stand in, the square, as Room.Info names it. */
  const HERE = 20605;
  /** The walk to [6][12], clicked two rooms south of the square, so on
   *  the grid it planned on the square sat two rows higher. */
  const route = {
    cells: [10, 9, 8, 7, 6, 5, 4]
      .map((row) => ({ row, col: 10 }))
      .concat([11, 12].map((col) => ({ row: 4, col }))),
    rooms: [20607, 20606, 20605, ...plan.rooms],
    target: { row: 4, col: 12 },
    kind: 'open' as const,
  };

  it('draws what is left ahead from the room you stand in', () => {
    const walking = { kind: 'walking', done: 2, total: 8, left: '4n2e', route: true } as const;
    expect(walkAhead(VAL_MIRAN, route, walking, HERE)).toEqual(offerOf(plan, { row: 6, col: 12 }));
    // Off the route, the map draws none of it.
    const elsewhere = { ...route, rooms: route.rooms.map((room) => room + 1000) };
    expect(walkAhead(VAL_MIRAN, elsewhere, walking, HERE)).toBeNull();
    expect(walkAhead(VAL_MIRAN, route, { kind: 'idle' }, HERE)).toBeNull();
    // Nor before the game names the room you stand in.
    expect(walkAhead(VAL_MIRAN, route, walking, null)).toBeNull();
  });

  it('keeps the steps Vosh sent but never saw land solid after a stop', () => {
    const stop = (done: number) =>
      walkAhead(VAL_MIRAN, route, { kind: 'stopped', done, total: 8, why: 'lost_sight' }, HERE);
    expect(stop(3)?.solid).toBe(1);
    expect(stop(2)?.solid).toBe(0);
  });

  /** The walk to [6][12] stopped with the first leg sent and unseen. */
  const stopped: WalkMark = { ...offerOf(plan, { row: 6, col: 12 }), solid: 1 };

  it('draws the leg Vosh sent solid and dashes the steps left 3 on 3 in the secondary ink', () => {
    const { ox, oy, pitch, calls } = squares(VAL_MIRAN, 1, stopped);
    const at = (row: number, col: number): Pt => [ox + col * pitch, oy + row * pitch];
    const solid = calls.filter(
      (c) => c.op === 'stroke' && c.state.strokeStyle === MAP_COLORS.pathLine,
    );
    expect(solid.map((c) => c.lines)).toEqual([[[at(10, 10), at(9, 10)]]]);
    expect(solid[0].state).toMatchObject({ lineWidth: 2, lineDash: [] });
    const dashed = calls.filter((c) => c.op === 'stroke' && c.state.strokeStyle === SECONDARY);
    expect(dashed).toHaveLength(1);
    expect(dashed[0].state).toMatchObject({ lineWidth: 2, lineDash: [3, 3] });
    expect(dashed[0].lines?.[0]).toEqual([at(9, 10), at(8, 10)]);
    expect(dashed[0].lines?.at(-1)).toEqual([at(6, 11), at(6, 12)]);
  });

  it('lets the rooms fall back to their depth fade and dashes a tertiary ring 3 on 2', () => {
    const { calls } = squares(VAL_MIRAN, 1, stopped);
    expect(
      calls.some((c) => c.op === 'strokeRect' && c.state.strokeStyle === MAP_COLORS.pathLine),
    ).toBe(false);
    const ring = calls.findIndex((c) => c.op === 'roundRect');
    expect(calls[ring + 1].state).toMatchObject({
      strokeStyle: FAINT,
      lineWidth: 1.5,
      lineDash: [3, 2],
    });
  });
});

describe('the Tileset painter', () => {
  it('paints Squares in the mark face until a tileset loads', () => {
    for (const [name, payload] of Object.entries(ALL)) {
      const s = scene(payload);
      const { ctx, calls } = recorder();
      drawTileset(ctx, payload, s.rows, s.cols, s.centerR, s.centerC, null, s.anchor, GROUND);
      expect(calls, name).toEqual(squares(payload).calls);
    }
  });

  it('draws each room from its sector tile in the strip, and from tile 0 when the strip has none', () => {
    // A tileset lays its tiles left to right in this sector order.
    const strip = ['0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'c'];
    const { rows, cols, centerR, centerC, anchor, ox, oy, pitch } = scene(VAL_MIRAN);
    for (const tilesInImage of [13, 4]) {
      const image = { naturalWidth: tilesInImage * 16, naturalHeight: 16 } as HTMLImageElement;
      const { ctx, calls } = recorder();
      drawTileset(ctx, VAL_MIRAN, rows, cols, centerR, centerC, image, anchor, GROUND);
      const expected = gridRooms(VAL_MIRAN, rows, cols).map(({ row, col, cell }) => {
        const idx = strip.indexOf(sectorCodeOf(cell.s));
        const tile = idx >= 0 && idx < tilesInImage ? idx : 0;
        const x = ox + col * pitch - pitch / 2;
        const y = oy + row * pitch - pitch / 2;
        return [image, tile * 16, 0, 16, 16, x, y, pitch, pitch];
      });
      const drawn = calls.filter((c) => c.op === 'drawImage').map((c) => c.args);
      expect(drawn, `${tilesInImage} tiles`).toEqual(expected);
      expect(new Set(drawn.map((args) => args[1])).size).toBeGreaterThan(2);
    }
  });
});

describe('the Glyphs overlay', () => {
  const cell = (more: Partial<GlyphCell> = {}): GlyphCell => ({
    glyph: '+',
    color: '#c4a872',
    isPlayer: false,
    floor: 'same',
    ...more,
  });

  it('names each cell by where it sits in the doubled grid', () => {
    expect(cellClass(cell(), 0, 0)).toBe('map-glyph-cell map-glyph-cell-room');
    expect(cellClass(cell(), 2, 1)).toBe('map-glyph-cell map-glyph-cell-hbridge');
    expect(cellClass(cell(), 1, 2)).toBe('map-glyph-cell map-glyph-cell-vbridge');
    expect(cellClass(cell(), 3, 5)).toBe('map-glyph-cell map-glyph-cell-corner');
  });

  it('marks your room and the rooms on other floors', () => {
    expect(cellClass(cell({ glyph: '@', isPlayer: true }), 4, 4)).toBe(
      'map-glyph-cell map-glyph-cell-room map-glyph-player',
    );
    expect(cellClass(cell({ floor: 'above' }), 4, 6)).toBe(
      'map-glyph-cell map-glyph-cell-room map-glyph-above',
    );
    expect(cellClass(cell({ floor: 'below' }), 6, 4)).toBe(
      'map-glyph-cell map-glyph-cell-room map-glyph-below',
    );
    expect(cellClass(cell({ floor: 'far' }), 6, 6)).toBe(
      'map-glyph-cell map-glyph-cell-room map-glyph-far',
    );
  });

  it('says so when the packet holds no grid', () => {
    // A Map.Tiles with no data reaches the view as {}.
    const html = renderToStaticMarkup(
      createElement(GlyphsOverlay, { payload: {}, payloadJson: '{}', zoom: 1 }),
    );
    expect(html).toBe('<div class="map-glyph-empty">no glyph data in payload</div>');
  });
});
