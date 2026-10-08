// The 3D map's scene: the rooms of one Map.Tiles packet stacked by
// floor, the camera that looks at them, and the lines between them. Pure
// geometry, after the game's builder3d.js, so the projection and what
// draws where are unit tested without a canvas. map3dDraw.ts paints it.

import { DEFAULT_TILT, type Floors, type Map3dView } from './map3dView';
import { sectorIndex } from './mapPalette';
import {
  exitStrokes,
  gridDims,
  gridRooms,
  offFloorLayers,
  playerCellOf,
  sectorCodeOf,
  type DoorState,
  type MapTilesPayload,
  type ServerCell,
  type StrokeKind,
} from './mapTiles';

// Sizes in cells, the spacing of the game's grid.
/** A room's width. The rest of the cell is the gap a corridor crosses. */
export const TILE = 0.72;
/** A room's height. */
export const THICK = 0.3;
/** Floor to floor. */
export const LAYER = 1.6;
// The camera's distance in map widths. Far looks nearly flat.
const PERSPECTIVE = 2.6;
// Pixels kept clear at the drawing's edge when your reach fits.
const PAD = 8;
/** The smallest a roof draws at actual size, in pixels. A wide reach in
 *  a narrow pane runs past the edge, and zooming out shows it all. */
export const MIN_ROOF_PX = 12;
// A bent exit's tick runs from just inside its roof to a third of the
// way across the gap, so two ticks that face each other stay apart.
const TICK_IN = 0.06;
const TICK_OUT = 0.09;

export interface Room3d {
  x: number;
  y: number;
  /** Floors above yours, below when negative. */
  z: number;
  cell: ServerCell;
  /** The SECTORS index of its terrain. */
  sector: number;
  you: boolean;
}

export interface Scene {
  rooms: Room3d[];
  you: Room3d | null;
  /** Where the camera looks, your room or the middle of the grid. */
  center: { x: number; y: number };
  radius: number;
  at: (x: number, y: number, z: number) => Room3d | undefined;
}

/** Whether a floor z steps from yours draws under a floors choice. */
export function floorShown(z: number, floors: Floors): boolean {
  if (floors === 'yours') return z === 0;
  if (floors === 'adjacent') return Math.abs(z) <= 1;
  return true;
}

/** Every room of the packet on the floors shown. Your floor comes from
 *  the grid and the others from offFloorLayers, so 3D draws the same
 *  rooms Squares does. */
export function sceneOf(payload: MapTilesPayload, floors: Floors): Scene {
  const radius = typeof payload.r === 'number' && payload.r > 0 ? payload.r : 7;
  const { rows, cols } = gridDims(payload);
  const mark = playerCellOf(payload, rows, cols);
  const rooms: Room3d[] = [];
  const room = (x: number, y: number, z: number, cell: ServerCell, you: boolean) =>
    rooms.push({ x, y, z, cell, sector: sectorIndex(sectorCodeOf(cell.s)), you });
  for (const { row, col, cell } of gridRooms(payload, rows, cols)) {
    room(col, row, 0, cell, row === mark.row && col === mark.col);
  }
  for (const e of offFloorLayers(payload).flat()) {
    if (e.z !== 0 && floorShown(e.z, floors)) room(e.x, e.y, e.z, e, false);
  }
  const spots = new Map(rooms.map((r) => [`${r.x},${r.y},${r.z}`, r]));
  const you = rooms.find((r) => r.you) ?? null;
  return {
    rooms,
    you,
    center: you ?? { x: mark.col, y: mark.row },
    radius,
    at: (x, y, z) => spots.get(`${x},${y},${z}`),
  };
}

/** A point on screen, with its distance from the eye and the pixels a
 *  cell spans there. */
export interface Pt {
  x: number;
  y: number;
  depth: number;
  scale: number;
}

export interface Camera {
  w: number;
  h: number;
  tx: number;
  ty: number;
  tz: number;
  dist: number;
  focal: number;
  cosYaw: number;
  sinYaw: number;
  cosPitch: number;
  sinPitch: number;
  /** Pixels per cell where the camera looks. */
  ppc: number;
}

const RAD = Math.PI / 180;

/** The camera for a drawing w by h pixels. At zoom 1 your reach fits,
 *  a circle round it so the scale holds while you turn, unless a roof
 *  would draw under MIN_ROOF_PX. */
export function cameraFor(
  w: number,
  h: number,
  scene: Scene,
  view: Map3dView,
  zoom: number,
): Camera {
  const pitch = (Number.isFinite(view.tilt) ? view.tilt : DEFAULT_TILT) * RAD;
  const yaw = view.turn * RAD;
  const sinPitch = Math.sin(pitch);
  const cosPitch = Math.cos(pitch);
  const reach = scene.radius + 0.5;
  const fitW = (w - 2 * PAD) / (2 * reach);
  const fitH = (h - 2 * PAD) / (2 * reach * sinPitch + THICK * cosPitch);
  const ppc = Math.max(Math.min(fitW, fitH), MIN_ROOF_PX / TILE) * zoom;
  const dist = PERSPECTIVE * 2 * reach;
  return {
    w,
    h,
    tx: scene.center.x,
    ty: scene.center.y,
    tz: THICK / 2,
    dist,
    focal: ppc * dist,
    cosYaw: Math.cos(yaw),
    sinYaw: Math.sin(yaw),
    cosPitch,
    sinPitch,
    ppc,
  };
}

/** Where a point of the map lands on screen. x runs east and y south in
 *  cells, as the grid does, and z rises in cells. */
export function project(c: Camera, x: number, y: number, z: number): Pt {
  const dx = x - c.tx;
  const dy = c.ty - y;
  const dz = z - c.tz;
  const x1 = dx * c.cosYaw - dy * c.sinYaw;
  const y1 = dx * c.sinYaw + dy * c.cosYaw;
  const y2 = y1 * c.cosPitch - dz * c.sinPitch;
  const z2 = y1 * c.sinPitch + dz * c.cosPitch;
  const depth = Math.max(0.05, y2 + c.dist);
  const scale = c.focal / depth;
  return { x: c.w / 2 + x1 * scale, y: c.h / 2 - z2 * scale, depth, scale };
}

/** The height of a floor's ground and of its roofs. */
export const floorAt = (z: number) => z * LAYER;
export const roofAt = (z: number) => z * LAYER + THICK;

/** The corners of a square `half` cells from x, y at height h, north
 *  west first and round clockwise. */
export function square(c: Camera, x: number, y: number, h: number, half: number): Pt[] {
  return [
    project(c, x - half, y - half, h),
    project(c, x + half, y - half, h),
    project(c, x + half, y + half, h),
    project(c, x - half, y + half, h),
  ];
}

/** The walls of a box that face the eye, by the index of their roof
 *  edge from square(): 0 north, 1 east, 2 south, 3 west. */
export function wallsFacing(c: Camera): number[] {
  // A wall seen exactly edge on stays hidden, past rounding.
  const e = 1e-9;
  const show = [c.cosYaw < -e, c.sinYaw < -e, c.cosYaw > e, c.sinYaw > e];
  return [0, 1, 2, 3].filter((f) => show[f]);
}

/** A line across the gap between rooms, at roof height. */
export interface ExitLine {
  z: number;
  from: [number, number, number];
  to: [number, number, number];
  state: DoorState;
  kind: StrokeKind;
}

/** The exits on each floor shown, from exitStrokes. A join runs across
 *  the gap from roof edge to roof edge, once for a pair that lead to
 *  each other, from the room west or north of the other. A stub runs to
 *  the middle of the gap, and a tick crosses its room's edge. */
export function exitLines(scene: Scene): ExitLine[] {
  const out: ExitLine[] = [];
  const hf = TILE / 2;
  for (const r of scene.rooms) {
    const next = (dx: number, dy: number) => scene.at(r.x + dx, r.y + dy, r.z)?.cell ?? null;
    for (const s of exitStrokes(r.cell, next)) {
      if (s.mutual && (s.dx < 0 || s.dy < 0)) continue;
      const [a, b] =
        s.kind === 'join'
          ? [hf, 1 - hf]
          : s.kind === 'stub'
            ? [hf, 0.5]
            : [hf - TICK_IN, hf + TICK_OUT];
      const h = roofAt(r.z);
      out.push({
        z: r.z,
        from: [r.x + s.dx * a, r.y + s.dy * a, h],
        to: [r.x + s.dx * b, r.y + s.dy * b, h],
        state: s.state,
        kind: s.kind,
      });
    }
  }
  return out;
}

/** A stair between two floors shown, at x, y from floor lo up to hi. */
export interface Shaft {
  x: number;
  y: number;
  lo: number;
  hi: number;
}

/** The stairs. Where both ends show, a shaft joins the roof of the lower
 *  room to the floor of the upper one, once per pair. A stair from your
 *  floor whose other end does not show leaves a mark on your roof. */
export function stairsOf(scene: Scene): {
  shafts: Shaft[];
  marks: { room: Room3d; up: boolean }[];
} {
  const shafts: Shaft[] = [];
  const marks: { room: Room3d; up: boolean }[] = [];
  for (const r of scene.rooms) {
    const exits = r.cell.e ?? '';
    for (const [letter, step] of [
      ['u', 1],
      ['d', -1],
    ] as const) {
      if (!exits.includes(letter)) continue;
      const other = scene.at(r.x, r.y, r.z + step);
      if (other) {
        // The room below draws the pair when it leads back up.
        if (step === -1 && (other.cell.e ?? '').includes('u')) continue;
        shafts.push({ x: r.x, y: r.y, lo: Math.min(r.z, other.z), hi: Math.max(r.z, other.z) });
      } else if (r.z === 0) {
        marks.push({ room: r, up: step === 1 });
      }
    }
  }
  return { shafts, marks };
}

/** The label of a floor z steps from yours, +1 above and −2 two below. */
export function floorLabel(z: number): string {
  return z > 0 ? `+${z}` : `−${-z}`;
}

/** Where a floor's number goes. */
export interface FloorNumber {
  label: string;
  x: number;
  y: number;
  align: 'left' | 'right';
}

/** With every floor shown, each floor besides yours takes its number
 *  just left of its leftmost roof that nothing above covers, or else of
 *  the leftmost one with its middle in view. Your floor and the floors
 *  below draw solid boxes, and the floors above, drawn as outlines,
 *  cover nothing. `measure` gives a label's width in pixels. A label
 *  that would run off the left edge starts at the edge instead. */
export function floorNumbers(
  c: Camera,
  scene: Scene,
  measure: (label: string) => number,
): FloorNumber[] {
  const hf = TILE / 2;
  const roofOf = (r: Room3d) => square(c, r.x, r.y, roofAt(r.z), hf);
  const solid = scene.rooms
    .filter((r) => r.z <= 0)
    .map((r) => ({
      z: r.z,
      poly: hull([...square(c, r.x, r.y, floorAt(r.z), hf), ...roofOf(r)]),
    }));
  const byFloor = new Map<number, Room3d[]>();
  for (const r of scene.rooms) {
    if (r.z !== 0) byFloor.set(r.z, [...(byFloor.get(r.z) ?? []), r]);
  }
  const out: FloorNumber[] = [];
  for (const [z, rooms] of [...byFloor].sort((p, q) => q[0] - p[0])) {
    const placed = rooms.map((r) => {
      const q = roofOf(r);
      const mid = {
        x: (q[0].x + q[1].x + q[2].x + q[3].x) / 4,
        y: (q[0].y + q[1].y + q[2].y + q[3].y) / 4,
      };
      // The middle, then each corner pulled a little toward it.
      const probes = [
        mid,
        ...q.map((p) => ({ x: p.x + (mid.x - p.x) * 0.15, y: p.y + (mid.y - p.y) * 0.15 })),
      ];
      return { left: Math.min(...q.map((p) => p.x)), mid, probes };
    });
    placed.sort((p, q) => p.left - q.left);
    const covered = (pts: { x: number; y: number }[]) =>
      pts.some((p) => solid.some((s) => s.z > z && inside(p, s.poly)));
    const pick = placed.find((p) => !covered(p.probes)) ?? placed.find((p) => !covered([p.mid]));
    if (!pick) continue;
    const label = floorLabel(z);
    const x = pick.left - 4;
    out.push(
      x - measure(label) < 3
        ? { label, x: 3, y: pick.mid.y, align: 'left' }
        : { label, x, y: pick.mid.y, align: 'right' },
    );
  }
  return out;
}

/** The room of your floor whose roof lies under x, y on screen, the
 *  nearest to the eye first where two overlap, or null. A point on a
 *  room of another floor only, or on bare ground, gives null. */
export function roomAt(c: Camera, scene: Scene, x: number, y: number): Room3d | null {
  const hf = TILE / 2;
  let hit: Room3d | null = null;
  let nearest = Infinity;
  for (const r of scene.rooms) {
    if (r.z !== 0) continue;
    const roof = square(c, r.x, r.y, roofAt(0), hf);
    if (!inside({ x, y }, roof)) continue;
    const { depth } = project(c, r.x, r.y, roofAt(0));
    if (depth < nearest) {
      nearest = depth;
      hit = r;
    }
  }
  return hit;
}

/** The unit vector of north on screen, for the compass. */
export function northOnScreen(c: Camera): { x: number; y: number } {
  const o = project(c, c.tx, c.ty, c.tz);
  const n = project(c, c.tx, c.ty - 1, c.tz);
  const l = Math.hypot(n.x - o.x, n.y - o.y) || 1;
  return { x: (n.x - o.x) / l, y: (n.y - o.y) / l };
}

type XY = { x: number; y: number };

function hull<T extends XY>(pts: T[]): T[] {
  const p = [...pts].sort((a, b) => a.x - b.x || a.y - b.y);
  const cross = (o: XY, a: XY, b: XY) => (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x);
  const half = (list: T[]) => {
    const out: T[] = [];
    for (const q of list) {
      while (out.length >= 2 && cross(out[out.length - 2], out[out.length - 1], q) <= 0) out.pop();
      out.push(q);
    }
    return out.slice(0, -1);
  };
  return [...half(p), ...half([...p].reverse())];
}

function inside(pt: XY, poly: XY[]): boolean {
  let hit = false;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const a = poly[i];
    const b = poly[j];
    if (a.y > pt.y !== b.y > pt.y && pt.x < ((b.x - a.x) * (pt.y - a.y)) / (b.y - a.y) + a.x)
      hit = !hit;
  }
  return hit;
}
