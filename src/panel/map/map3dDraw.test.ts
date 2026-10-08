import { describe, expect, it } from 'vitest';
import { aabahranMapPacket } from '../../test/aabahranGmcp';
import { drawMap3D } from './map3dDraw';
import { TILE, cameraFor, project, roofAt, sceneOf, square, stairsOf } from './map3dScene';
import { DEFAULT_MAP_3D_VIEW, type Map3dView } from './map3dView';
import { MAP_COLORS, type MapInks } from './mapPalette';
import type { MapTilesPayload } from './mapTiles';
import { offerOf, planWalk, type WalkMark } from './mapWalk';

/** You stand in The Central Square of Val Miran, radius 10, with a
 *  floor below and two above. */
const VAL_MIRAN = aabahranMapPacket('val-miran-central-square.gmcp').data as MapTilesPayload;

const INKS: MapInks = {
  light: false,
  ground: '#2e3440',
  text: '#d8dee9',
  secondary: '#9aa3b5',
  tertiary: '#6b7385',
  sep: '#3b4252',
  accent: '#88c0d0',
  accentSoft: 'rgba(136, 192, 208, 0.13)',
  font: 'sans-serif',
  labelPx: 10,
};

type XY = { x: number; y: number };

/** A canvas that keeps each fill and stroke in order, with its color,
 *  width, dash, the points of its path and its place among all of
 *  them, and does nothing else. */
function recorder() {
  const fills: { style: string; pts: XY[]; n: number }[] = [];
  const strokes: { style: string; width: number; dash: number[]; pts: XY[]; n: number }[] = [];
  let n = 0;
  let pts: XY[] = [];
  let dash: number[] = [];
  const state: Record<string | symbol, unknown> = {};
  const ctx = new Proxy(state, {
    get(target, key) {
      if (key === 'beginPath') return () => (pts = []);
      if (key === 'moveTo' || key === 'lineTo') return (x: number, y: number) => pts.push({ x, y });
      if (key === 'setLineDash') return (d: number[]) => (dash = d);
      if (key === 'fill') return () => fills.push({ style: String(target.fillStyle), pts, n: n++ });
      if (key === 'stroke')
        return () =>
          strokes.push({
            style: String(target.strokeStyle),
            width: Number(target.lineWidth),
            dash,
            pts,
            n: n++,
          });
      return key in target ? target[key] : () => undefined;
    },
  });
  return { ctx: ctx as unknown as CanvasRenderingContext2D, fills, strokes };
}

const near = (a: XY, b: XY, d: number) => Math.abs(a.x - b.x) <= d && Math.abs(a.y - b.y) <= d;

describe('drawMap3D', () => {
  it('paints each stair mark over its own roof, with north at the top or the bottom', () => {
    for (const turn of [0, 180]) {
      const view: Map3dView = { ...DEFAULT_MAP_3D_VIEW, floors: 'yours', turn };
      const { ctx, fills } = recorder();
      drawMap3D(ctx, 300, 348, VAL_MIRAN, view, 2, INKS, null);
      const scene = sceneOf(VAL_MIRAN, 'yours');
      const cam = cameraFor(300, 348, scene, view, 2);
      // Your roof keeps only the pin, as your cell in Squares keeps no
      // arrow.
      const marks = stairsOf(scene).marks.filter((m) => !m.room.you);
      expect(marks.length, `turn ${turn}`).toBeGreaterThan(10);
      for (const { room, up } of marks) {
        const roof = square(cam, room.x, room.y, roofAt(0), TILE / 2);
        const roofFill = fills
          .map((f) => f.pts.length === 4 && f.pts.every((p, i) => near(p, roof[i], 1e-9)))
          .lastIndexOf(true);
        const spot = project(cam, room.x, room.y + (up ? -0.2 : 0.2), roofAt(0));
        const markFill = fills.findIndex((f) => {
          if (f.pts.length !== 3) return false;
          const mid = {
            x: (f.pts[0].x + f.pts[1].x + f.pts[2].x) / 3,
            y: (f.pts[0].y + f.pts[1].y + f.pts[2].y) / 3,
          };
          return near(mid, spot, 1);
        });
        const where = `turn ${turn}, ${up ? 'up' : 'down'} at ${room.x}, ${room.y}`;
        expect(roofFill, where).toBeGreaterThanOrEqual(0);
        expect(markFill, where).toBeGreaterThan(roofFill);
      }
    }
  });

  describe('a walk', () => {
    const plan = planWalk(VAL_MIRAN, 6, 12)!;
    const offer = offerOf(plan, { row: 6, col: 12 });
    const scene = sceneOf(VAL_MIRAN, 'adjacent');
    const you = scene.you!;

    function draw(walk: WalkMark | null, turn: number) {
      const view: Map3dView = { ...DEFAULT_MAP_3D_VIEW, turn, tilt: 50 };
      const rec = recorder();
      drawMap3D(rec.ctx, 400, 360, VAL_MIRAN, view, 1.5, INKS, walk);
      const cam = cameraFor(400, 360, scene, view, 1.5);
      const at = ({ row, col }: { row: number; col: number }) => project(cam, col, row, roofAt(0));
      return { ...rec, cam, at };
    }

    it('strokes its path on the plane of your roofs in the path color at 2 px', () => {
      for (const turn of [0, 135]) {
        const { strokes, at } = draw(offer, turn);
        const lines = strokes.filter((s) => s.style === MAP_COLORS.pathLine);
        expect(lines, `turn ${turn}`).toHaveLength(1);
        const [line] = lines;
        expect(line.width).toBe(2);
        expect(line.dash).toEqual([]);
        const want = [{ row: you.y, col: you.x }, ...plan.cells].map(at);
        expect(line.pts).toHaveLength(want.length);
        line.pts.forEach((p, i) => expect(near(p, want[i], 1e-9), `point ${i}`).toBe(true));
      }
    });

    it('rings the roof it goes to in the accent, under your pin', () => {
      const { strokes, fills, cam } = draw(offer, 0);
      const ring = square(cam, 12, 6, roofAt(0), TILE / 2 + 3.5 / cam.ppc);
      const at = strokes.findIndex(
        (s) => s.pts.length === 4 && s.pts.every((p, i) => near(p, ring[i], 1e-9)),
      );
      expect(at).toBeGreaterThanOrEqual(0);
      expect(strokes[at]).toMatchObject({ style: MAP_COLORS.origin, width: 1.5, dash: [] });
      // Your roof and pin fill after the path and the ring.
      const pin = fills.filter((f) => f.style === INKS.accent).at(-1)!;
      const path = strokes.find((s) => s.style === MAP_COLORS.pathLine)!;
      expect(pin.n).toBeGreaterThan(strokes[at].n);
      expect(pin.n).toBeGreaterThan(path.n);
    });

    it('dashes the steps a stopped walk left in the secondary ink', () => {
      const { strokes, at } = draw({ ...offer, solid: 2 }, 0);
      const sent = strokes.find((s) => s.style === MAP_COLORS.pathLine)!;
      expect(sent.pts).toEqual(
        [{ row: you.y, col: you.x }, ...plan.cells.slice(0, 2)].map(at).map(xy),
      );
      const left = strokes.find((s) => s.style === MAP_COLORS.secondary && s.width === 2)!;
      expect(left.dash).toEqual([3, 3]);
      expect(left.pts).toHaveLength(plan.cells.length - 1);
      const ring = strokes.find((s) => s.style === MAP_COLORS.text && s.width === 1.5);
      expect(ring?.dash).toEqual([3, 2]);
    });

    it('draws no path with no walk', () => {
      const { strokes } = draw(null, 0);
      expect(strokes.some((s) => s.style === MAP_COLORS.pathLine)).toBe(false);
    });
  });
});

const xy = ({ x, y }: XY): XY => ({ x, y });
