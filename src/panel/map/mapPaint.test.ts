import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { aabahranMapPacket } from '../../test/aabahranGmcp';
import { hexToRgba, roomFill, sectorForCode } from './mapPalette';
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
} from './mapPaint';
import { GlyphsOverlay } from './MapView';

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
const MARK_FACE = 'Iosevka, monospace';
const VARS: Record<string, string> = {
  '--panel': GROUND,
  '--c-accent': ACCENT,
  '--c-accent-soft': ACCENT_SOFT,
  '--c-text-faint': FAINT,
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
  const anchor = computeAnchor(payload, rows, cols, centerR, centerC, W, H, zoom);
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

function squares(payload: MapTilesPayload, zoom = 1) {
  const s = scene(payload, zoom);
  const { ctx, calls } = recorder();
  drawSquares(
    ctx,
    W,
    H,
    payload,
    s.rows,
    s.cols,
    s.centerR,
    s.centerC,
    s.anchor,
    GROUND,
    MARK_FACE,
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

describe('the Tileset painter', () => {
  it('paints Squares in the mark face until a tileset loads', () => {
    for (const [name, payload] of Object.entries(ALL)) {
      const s = scene(payload);
      const { ctx, calls } = recorder();
      drawTileset(ctx, W, H, payload, s.rows, s.cols, s.centerR, s.centerC, null, s.anchor, GROUND);
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
      drawTileset(ctx, W, H, VAL_MIRAN, rows, cols, centerR, centerC, image, anchor, GROUND);
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
