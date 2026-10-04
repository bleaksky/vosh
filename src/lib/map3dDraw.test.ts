import { describe, expect, it } from 'vitest';
import { aabahranMapPacket } from '../test/aabahranGmcp';
import { drawMap3D } from './map3dDraw';
import { TILE, cameraFor, project, roofAt, sceneOf, square, stairsOf } from './map3dScene';
import { DEFAULT_MAP_3D_VIEW, type Map3dView } from './map3dView';
import type { MapInks } from './mapPalette';
import type { MapTilesPayload } from './mapTiles';

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

/** A canvas that keeps each fill in order, with its color and the
 *  points of its path, and does nothing else. */
function recorder() {
  const fills: { style: string; pts: XY[] }[] = [];
  let pts: XY[] = [];
  const state: Record<string | symbol, unknown> = {};
  const ctx = new Proxy(state, {
    get(target, key) {
      if (key === 'beginPath') return () => (pts = []);
      if (key === 'moveTo' || key === 'lineTo') return (x: number, y: number) => pts.push({ x, y });
      if (key === 'fill') return () => fills.push({ style: String(target.fillStyle), pts });
      return key in target ? target[key] : () => undefined;
    },
  });
  return { ctx: ctx as unknown as CanvasRenderingContext2D, fills };
}

const near = (a: XY, b: XY, d: number) => Math.abs(a.x - b.x) <= d && Math.abs(a.y - b.y) <= d;

describe('drawMap3D', () => {
  it('paints each stair mark over its own roof, with north at the top or the bottom', () => {
    for (const turn of [0, 180]) {
      const view: Map3dView = { ...DEFAULT_MAP_3D_VIEW, floors: 'yours', turn };
      const { ctx, fills } = recorder();
      drawMap3D(ctx, 300, 348, VAL_MIRAN, view, 2, INKS);
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
});
