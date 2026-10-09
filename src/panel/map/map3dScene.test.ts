import { describe, expect, it } from 'vitest';
import { aabahranMapPacket } from '../../test/aabahranGmcp';
import {
  MIN_ROOF_PX,
  TILE,
  cameraFor,
  exitLines,
  floorLabel,
  floorNumbers,
  northOnScreen,
  project,
  roofAt,
  roomAt,
  sceneOf,
  square,
  stairsOf,
  wallsFacing,
  type Scene,
} from './map3dScene';
import { DEFAULT_MAP_3D_VIEW, type Floors, type Map3dView } from './map3dView';
import { corridors, gridDims, gridRooms, offFloorLayers, type MapTilesPayload } from './mapTiles';

// The packets come from fixtures/gmcp/aabahran/map, built by the game's
// own generate_map and gmcp_send_map over its area files.

function tiles(name: string): MapTilesPayload {
  return aabahranMapPacket(name).data as MapTilesPayload;
}

/** You stand West of the City Fountain in Caranduin, radius 7, with a
 *  floor above you. */
const CARANDUIN = tiles('caranduin-west-of-the-fountain.gmcp');
/** You stand in The Central Square of Val Miran, radius 10, with a
 *  floor below and two above. */
const VAL_MIRAN = tiles('val-miran-central-square.gmcp');

const view = (patch: Partial<Map3dView> = {}): Map3dView => ({ ...DEFAULT_MAP_3D_VIEW, ...patch });

function floorsOf(scene: Scene): number[] {
  return [...new Set(scene.rooms.map((r) => r.z))].sort((a, b) => a - b);
}

describe('sceneOf', () => {
  it('holds your floor from the grid and the floors shown from offFloorLayers', () => {
    const p = VAL_MIRAN;
    const { rows, cols } = gridDims(p);
    const grid = gridRooms(p, rows, cols).length;
    const off = offFloorLayers(p).flat();
    const count = (floors: Floors) => sceneOf(p, floors).rooms.length;
    expect(count('yours')).toBe(grid);
    expect(count('adjacent')).toBe(grid + off.filter((e) => Math.abs(e.z) === 1).length);
    expect(count('all')).toBe(grid + off.length);
    expect(floorsOf(sceneOf(p, 'all'))).toEqual([-1, 0, 1, 2]);
    expect(floorsOf(sceneOf(p, 'adjacent'))).toEqual([-1, 0, 1]);
  });

  it('finds you at [r][r] and reads each terrain', () => {
    const scene = sceneOf(VAL_MIRAN, 'adjacent');
    expect(scene.you).toMatchObject({ x: 10, y: 10, z: 0, sector: 1 });
    expect(scene.rooms.filter((r) => r.you)).toHaveLength(1);
    expect(scene.center).toEqual(scene.you);
    expect(scene.radius).toBe(10);
    // Beneath the Suspension Bridge, deep water one floor down.
    expect(scene.at(10, 16, -1)).toMatchObject({ sector: 7 });
  });
});

describe('cameraFor and project', () => {
  const scene = sceneOf(CARANDUIN, 'adjacent');

  it('holds north at the top and east to the right when the map is not turned', () => {
    const c = cameraFor(300, 240, scene, view(), 1);
    const you = project(c, 7, 7, 0);
    const north = project(c, 7, 6, 0);
    const east = project(c, 8, 7, 0);
    expect(north.x).toBeCloseTo(you.x);
    expect(north.y).toBeLessThan(you.y);
    expect(east.x).toBeGreaterThan(you.x);
    expect(northOnScreen(c).x).toBeCloseTo(0);
    expect(northOnScreen(c).y).toBeCloseTo(-1);
  });

  it('swings north to the left as the turn grows to 90 degrees', () => {
    const c = cameraFor(300, 240, scene, view({ turn: 90 }), 1);
    const n = northOnScreen(c);
    expect(n.x).toBeCloseTo(-1);
    expect(n.y).toBeCloseTo(0);
  });

  it('draws the floor above higher on screen than the same spot on yours', () => {
    const c = cameraFor(300, 240, scene, view(), 1);
    expect(project(c, 7, 7, roofAt(1)).y).toBeLessThan(project(c, 7, 7, roofAt(0)).y);
  });

  it('fits your whole floor at actual size at any turn and tilt', () => {
    for (const turn of [0, 30, 45, 90, 135, 200, 315]) {
      for (const tilt of [20, 35.5, 60, 86]) {
        const c = cameraFor(300, 300, scene, view({ turn, tilt }), 1);
        for (const r of scene.rooms.filter((room) => room.z === 0)) {
          for (const p of square(c, r.x, r.y, roofAt(0), TILE / 2)) {
            expect(p.x, `${turn} ${tilt}`).toBeGreaterThanOrEqual(0);
            expect(p.x, `${turn} ${tilt}`).toBeLessThanOrEqual(300);
            expect(p.y, `${turn} ${tilt}`).toBeGreaterThanOrEqual(0);
            expect(p.y, `${turn} ${tilt}`).toBeLessThanOrEqual(300);
          }
        }
      }
    }
  });

  it('keeps a roof near 12 px at radius 10 in a 300 px pane, and zoom scales it', () => {
    const wide = sceneOf(VAL_MIRAN, 'adjacent');
    const c = cameraFor(300, 260, wide, view(), 1);
    expect(c.ppc * TILE).toBeCloseTo(MIN_ROOF_PX);
    expect(cameraFor(300, 260, wide, view(), 0.5).ppc).toBeCloseTo(c.ppc / 2);
    // Radius 7 fits with roofs larger than that.
    expect(cameraFor(300, 260, scene, view(), 1).ppc * TILE).toBeGreaterThan(MIN_ROOF_PX);
  });

  it('shows the walls that face the eye', () => {
    expect(wallsFacing(cameraFor(300, 240, scene, view(), 1))).toEqual([2]);
    expect(wallsFacing(cameraFor(300, 240, scene, view({ turn: 45 }), 1))).toEqual([2, 3]);
    expect(wallsFacing(cameraFor(300, 240, scene, view({ turn: 180 }), 1))).toEqual([0]);
  });
});

describe('exitLines', () => {
  it('joins each pair of rooms once and ticks each bent exit on your floor', () => {
    const p = CARANDUIN;
    const { rows, cols } = gridDims(p);
    const strokes = corridors(p, rows, cols);
    const lines = exitLines(sceneOf(p, 'yours'));
    const kinds = (k: string) => lines.filter((l) => l.kind === k).length;
    const pairs = strokes.filter((s) => s.kind === 'join');
    const oneWay = pairs.filter(
      (s) => !pairs.some((t) => t.row === s.row + s.dy && t.col === s.col + s.dx && t.dx === -s.dx),
    );
    expect(kinds('join')).toBe((pairs.length - oneWay.length) / 2 + oneWay.length);
    expect(kinds('tick')).toBe(strokes.filter((s) => s.kind === 'tick').length);
    expect(kinds('tick')).toBeGreaterThan(10);
  });

  it('runs a join across the gap and a tick across its own roof edge', () => {
    const lines = exitLines(sceneOf(CARANDUIN, 'yours'));
    const hf = TILE / 2;
    // You and The Common Road west of you lead to each other, and the
    // road draws the one line between you.
    const joins = lines.filter((l) => l.kind === 'join' && l.from[1] === 7 && l.to[1] === 7);
    const between = joins.filter(
      (l) => Math.min(l.from[0], l.to[0]) > 6 && Math.max(l.from[0], l.to[0]) < 7,
    );
    expect(between).toHaveLength(1);
    expect(between[0].from[0]).toBeCloseTo(6 + hf);
    expect(between[0].to[0]).toBeCloseTo(7 - hf);
    // The Common Road leads north past the Pill Shop.
    const tick = lines.find(
      (l) => l.kind === 'tick' && l.from[0] === 6 && l.from[1] < 7 && l.from[1] > 6,
    );
    expect(tick).toBeDefined();
    expect(tick!.from[1]).toBeGreaterThan(7 - hf);
    expect(tick!.to[1]).toBeLessThan(7 - hf);
    expect(7 - tick!.to[1]).toBeLessThan(0.5);
    expect(tick!.from[2]).toBe(roofAt(0));
  });
});

describe('stairsOf', () => {
  it('joins your room to the temple over it with a shaft, or marks your roof', () => {
    // Before the Temple of Neutrality sits right above you, its down exit
    // back to your room.
    expect(stairsOf(sceneOf(VAL_MIRAN, 'adjacent')).shafts).toContainEqual({
      x: 10,
      y: 10,
      lo: 0,
      hi: 1,
    });
    const alone = stairsOf(sceneOf(VAL_MIRAN, 'yours'));
    expect(alone.shafts).toEqual([]);
    expect(alone.marks).toContainEqual({
      room: expect.objectContaining({ x: 10, y: 10 }),
      up: true,
    });
  });
});

describe('floorNumbers', () => {
  it('numbers each floor besides yours with every floor shown', () => {
    const scene = sceneOf(VAL_MIRAN, 'all');
    const c = cameraFor(500, 480, scene, view(), 1);
    const labels = floorNumbers(c, scene, (l) => l.length * 6).map((n) => n.label);
    expect(labels.sort()).toEqual(['+1', '+2', '−1'].sort());
  });

  it('keeps each number inside the drawing', () => {
    const scene = sceneOf(VAL_MIRAN, 'all');
    const c = cameraFor(300, 260, scene, view({ turn: 45 }), 1);
    for (const n of floorNumbers(c, scene, (l) => l.length * 6)) {
      expect(n.align === 'left' ? n.x : n.x - n.label.length * 6).toBeGreaterThanOrEqual(3);
    }
  });

  it('writes a floor above with a plus and one below with a minus sign', () => {
    expect(floorLabel(1)).toBe('+1');
    expect(floorLabel(-2)).toBe('−2');
  });
});

describe('roomAt', () => {
  const scene = sceneOf(VAL_MIRAN, 'all');
  const ours = scene.rooms.filter((r) => r.z === 0);
  const turns = [
    { turn: 0, tilt: DEFAULT_MAP_3D_VIEW.tilt },
    { turn: 45, tilt: 60 },
    { turn: 200, tilt: 25 },
    { turn: 300, tilt: 86 },
  ];

  it('finds the room of your floor under the middle of each roof at any turn and tilt', () => {
    for (const t of turns) {
      const cam = cameraFor(420, 380, scene, view({ ...t, floors: 'all' }), 1.5);
      for (const r of ours) {
        const p = project(cam, r.x, r.y, roofAt(0));
        expect(roomAt(cam, scene, p.x, p.y), `turn ${t.turn} at ${r.x},${r.y}`).toBe(r);
      }
    }
  });

  it('gives null on bare ground and between two roofs', () => {
    for (const t of turns) {
      const cam = cameraFor(420, 380, scene, view({ ...t, floors: 'all' }), 1.5);
      const corner = project(cam, -3, -3, roofAt(0));
      expect(roomAt(cam, scene, corner.x, corner.y)).toBeNull();
      const a = ours.find((r) => scene.at(r.x + 1, r.y, 0))!;
      const gap = project(cam, a.x + 0.5, a.y, roofAt(0));
      expect(roomAt(cam, scene, gap.x, gap.y), `turn ${t.turn}`).toBeNull();
    }
  });

  it('gives null over a room of another floor with none of yours under it', () => {
    const cam = cameraFor(420, 380, scene, view({ floors: 'all' }), 1.5);
    const others = scene.rooms.filter((r) => r.z !== 0);
    const lone = others.filter((r) => {
      const p = project(cam, r.x, r.y, roofAt(r.z));
      return ours.every((o) => {
        const q = square(cam, o.x, o.y, roofAt(0), TILE / 2 + 0.1);
        const xs = q.map((v) => v.x);
        const ys = q.map((v) => v.y);
        const inBox =
          p.x >= Math.min(...xs) &&
          p.x <= Math.max(...xs) &&
          p.y >= Math.min(...ys) &&
          p.y <= Math.max(...ys);
        return !inBox;
      });
    });
    expect(lone.length).toBeGreaterThan(0);
    for (const r of lone) {
      const p = project(cam, r.x, r.y, roofAt(r.z));
      expect(roomAt(cam, scene, p.x, p.y)).toBeNull();
    }
  });
});
