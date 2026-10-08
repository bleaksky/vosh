// Paints the 3D map style in Canvas 2D: one box per room, stacked by
// floor, after the game's builder3d.js and atlas. Your floor draws lit
// on a faint plate the shape of your reach, the floors above as
// outlines you see through, and the floors below faded toward the
// ground. Your room takes the accent, and a walk draws its path across
// the roofs of your floor. map3dScene.ts holds the geometry, and this
// file decides only the paint and its order.

import { WHITE, mix, parseHex, scaled, toRgba, type Rgb } from '../../theme/color';
import { isNorthUp, type Map3dView } from './map3dView';
import {
  TILE,
  cameraFor,
  exitLines,
  floorAt,
  floorNumbers,
  northOnScreen,
  project,
  roofAt,
  sceneOf,
  square,
  stairsOf,
  wallsFacing,
  type Camera,
  type Pt,
  type Room3d,
  type Scene,
} from './map3dScene';
import { SPRITE_SIZE, TERRAIN, paintSprite, spriteMean, spriteVariant } from './mapAtlas';
import { inkWalkTarget, strokeWalkPath } from './mapPaint';
import type { MapInks } from './mapPalette';
import { DOOR_COLORS, type MapTilesPayload } from './mapTiles';
import type { WalkMark } from './mapWalk';

// How far a floor below fades toward the ground, by its steps from
// yours. Each step fades more, so the floors count by eye.
const FADE = [0, 0.6, 0.71, 0.8, 0.87];
// How strongly a floor above draws its outlines, by its steps.
const ABOVE = [1, 1, 0.68, 0.48, 0.34];
const fadeOf = (z: number) => FADE[Math.min(FADE.length - 1, Math.abs(z))];
const aboveOf = (z: number) => ABOVE[Math.min(ABOVE.length - 1, Math.abs(z))];
// The light falls from the north west. Each wall's share of it, in
// the order wallsFacing names them, north, east, south, west.
const WALL_LIGHT = [0.98, 0.58, 0.54, 0.86];
// A roof under this many pixels draws no edge, which would only blur it.
const EDGE_PX = 6;
// A roof under this many pixels keeps plain color with sprites on.
const SPRITE_PX = 11;

const FALLBACK: Rgb = { r: 128, g: 128, b: 128 };

/** Toward gray by t, at the same light. */
function desat(c: Rgb, t: number): Rgb {
  const l = 0.299 * c.r + 0.587 * c.g + 0.114 * c.b;
  return mix(c, { r: l, g: l, b: l }, t);
}

/** One thing to paint. Floors paint bottom up, and within a floor far
 *  to near. While the view looks down, a higher floor always lies
 *  nearer the eye than any lower thing it covers on screen, so this is
 *  a correct painter's order at every turn and tilt. */
interface Prim {
  /** The floor it paints with. The plate under yours is -0.5. */
  floor: number;
  depth: number;
  draw: () => void;
}

type BoxMode = 'lit' | 'faded' | 'outline';

/** Paint a Map.Tiles packet in 3D on a canvas w by h CSS pixels, its
 *  transform already set for the device pixel ratio, with the walk on
 *  offer or under way, if any. */
export function drawMap3D(
  ctx: CanvasRenderingContext2D,
  w: number,
  h: number,
  payload: MapTilesPayload,
  view: Map3dView,
  zoom: number,
  inks: MapInks,
  walk: WalkMark | null,
): void {
  const scene = sceneOf(payload, view.floors);
  const cam = cameraFor(w, h, scene, view, zoom);
  const ground = parseHex(inks.ground) ?? FALLBACK;
  const text = parseHex(inks.text) ?? FALLBACK;
  const second = parseHex(inks.secondary) ?? FALLBACK;
  // A color on a floor below, faded toward the ground by its steps.
  const below = (c: Rgb, z: number) => mix(desat(c, 0.45), ground, fadeOf(z));
  // A corridor is a light line over a dark casing in every theme, as
  // builder3d draws its exits in white. The gaps it crosses are mostly
  // the dark walls of the rooms beside it, and on a light theme the
  // casing outlines it where it crosses the paper.
  const lightInk = inks.light ? mix(ground, WHITE, 0.55) : text;
  const darkInk = inks.light ? text : ground;
  const prims: Prim[] = [];

  // The plate under your floor: the diamond your reach covers, a shade
  // off the ground with a hairline edge, so your floor reads as a plane
  // and the floors below sit under it.
  if (scene.you) {
    const R = scene.radius + 0.5;
    const { x, y } = scene.you;
    const pts = [
      project(cam, x, y - R, 0),
      project(cam, x + R, y, 0),
      project(cam, x, y + R, 0),
      project(cam, x - R, y, 0),
    ];
    prims.push({
      floor: -0.5,
      depth: 0,
      draw: () => {
        path(ctx, pts);
        ctx.fillStyle = toRgba(text, inks.light ? 0.035 : 0.03);
        ctx.fill();
        ctx.lineWidth = 1;
        ctx.strokeStyle = toRgba(text, 0.16);
        ctx.stroke();
      },
    });
  }

  // The middle of a room's box. The box sorts at its depth.
  const boxMid = (r: Room3d) => project(cam, r.x, r.y, (floorAt(r.z) + roofAt(r.z)) / 2);
  const walls = wallsFacing(cam);
  const hf = TILE / 2;
  for (const r of scene.rooms) {
    const top = square(cam, r.x, r.y, roofAt(r.z), hf);
    const bot = square(cam, r.x, r.y, floorAt(r.z), hf);
    const faces = walls.map((f) => ({
      q: [top[f], top[(f + 1) % 4], bot[(f + 1) % 4], bot[f]],
      light: WALL_LIGHT[f],
    }));
    const mid = boxMid(r);
    const mode: BoxMode = r.z === 0 ? 'lit' : r.z > 0 ? 'outline' : 'faded';
    const box = { room: r, top, faces, mode, topPx: TILE * mid.scale };
    prims.push({
      floor: r.z,
      depth: mid.depth,
      draw: () => drawBox(ctx, box, view.sprites, below, ground, text),
    });
  }

  for (const line of exitLines(scene)) {
    const a = project(cam, ...line.from);
    const b = project(cam, ...line.to);
    const door = line.state !== 'open' ? DOOR_COLORS[line.state] : null;
    let core: string;
    let casing: string;
    let width: number;
    if (line.z === 0) {
      core = door ?? toRgba(lightInk, 0.9);
      casing = toRgba(darkInk, inks.light ? 0.8 : 1);
      width = door ? 2 : 1.6;
    } else if (line.z > 0) {
      core = toRgba(mix(text, ground, 0.25), 0.85 * aboveOf(line.z));
      casing = toRgba(ground, 0.75 * aboveOf(line.z));
      width = 1;
    } else {
      core = toRgba(below(lightInk, line.z), 1);
      casing = toRgba(below(darkInk, line.z), inks.light ? 0.8 : 1);
      width = 1.25;
    }
    const dash = line.state === 'hidden' ? [3, 3] : undefined;
    prims.push({
      floor: line.z,
      depth: Math.min(a.depth, b.depth) - 0.004,
      draw: () => casedLine(ctx, a, b, core, width, casing, dash),
    });
  }

  // Stairs. A dashed shaft in the secondary ink, a stair and never a
  // corridor, as strong as the nearer of its two floors. A stair from
  // your floor whose other end does not show leaves a small mark on its
  // roof. It sorts just nearer than its own box, since its own point
  // toward the far edge of the roof lies deeper than the box's middle.
  // Your roof keeps only the pin, as your cell in Squares keeps no arrow.
  const { shafts, marks } = stairsOf(scene);
  for (const s of shafts) {
    const a = project(cam, s.x, s.y, roofAt(s.lo));
    const b = project(cam, s.x, s.y, floorAt(s.hi));
    const near = Math.abs(s.lo) < Math.abs(s.hi) ? s.lo : s.hi;
    const ink =
      near === 0
        ? toRgba(second, 0.95)
        : near > 0
          ? toRgba(second, 0.8 * aboveOf(near))
          : toRgba(below(second, near), 1);
    prims.push({
      floor: s.lo,
      depth: Math.min(a.depth, b.depth) - 0.006,
      draw: () => casedLine(ctx, a, b, ink, 1.4, toRgba(ground, 0.9), [2.5, 2]),
    });
  }
  for (const { room, up } of marks) {
    if (room.you) continue;
    const p = project(cam, room.x, room.y + (up ? -0.2 : 0.2), roofAt(0));
    const size = Math.max(3, Math.min(5, TILE * p.scale * 0.2));
    prims.push({
      floor: 0,
      depth: boxMid(room).depth - 0.005,
      draw: () => drawStairMark(ctx, p, up, toRgba(second, 0.95), size, inks.ground),
    });
  }

  ctx.fillStyle = inks.ground;
  ctx.fillRect(0, 0, w, h);
  prims.sort((p, q) => p.floor - q.floor || q.depth - p.depth);
  for (const p of prims) p.draw();
  if (scene.you && walk) drawWalk(ctx, cam, scene.you, walk, inks);
  if (scene.you) drawYou(ctx, cam, scene.you, inks);
  if (view.floors === 'all') drawFloorNumbers(ctx, cam, scene, inks, ground);
  if (!isNorthUp(view)) drawCompass(ctx, cam, inks);
}

function path(ctx: CanvasRenderingContext2D, q: Pt[]) {
  ctx.beginPath();
  ctx.moveTo(q[0].x, q[0].y);
  for (let i = 1; i < q.length; i++) ctx.lineTo(q[i].x, q[i].y);
  ctx.closePath();
}

interface Box {
  room: Room3d;
  top: Pt[];
  faces: { q: Pt[]; light: number }[];
  mode: BoxMode;
  /** The roof's width on screen. */
  topPx: number;
}

function drawBox(
  ctx: CanvasRenderingContext2D,
  { room, top, faces, mode, topPx }: Box,
  sprites: boolean,
  below: (c: Rgb, z: number) => Rgb,
  ground: Rgb,
  text: Rgb,
) {
  const shade = TERRAIN[room.sector] ?? TERRAIN[0];
  if (mode === 'outline') {
    // A floor above is a ceiling you look through. Each roof is a
    // whisper of its terrain, outlined in a light ink over a casing of
    // the ground, so the outline holds over any roof below it.
    const k = aboveOf(room.z);
    path(ctx, top);
    ctx.fillStyle = toRgba(shade.top, 0.12 * k);
    ctx.fill();
    ctx.lineJoin = 'round';
    ctx.lineWidth = 3;
    ctx.strokeStyle = toRgba(ground, 0.7 * k);
    ctx.stroke();
    ctx.lineWidth = 1;
    ctx.strokeStyle = toRgba(mix(mix(shade.border, shade.top, 0.5), text, 0.55), 0.95 * k);
    ctx.stroke();
    return;
  }
  const lit = mode === 'lit';
  const sprite = lit && sprites && topPx > SPRITE_PX ? spriteOf(room) : null;
  // With a sprite on the roof the walls take its average color, so the
  // room reads as one block of material.
  const wall = sprite ? sprite.mean : shade.side;
  const edge = lit && topPx > EDGE_PX ? 'rgba(0, 0, 0, 0.42)' : null;
  for (const f of faces) {
    path(ctx, f.q);
    const c = scaled(wall, f.light);
    ctx.fillStyle = toRgba(lit ? c : below(c, room.z), 1);
    ctx.fill();
    if (edge) {
      ctx.strokeStyle = edge;
      ctx.lineWidth = 1;
      ctx.stroke();
    }
  }
  if (sprite) {
    // The roof is a quad under perspective. Average its opposing edges
    // and hang the sprite off its middle, as the atlas does.
    const q = top;
    const n = SPRITE_SIZE * 2;
    const ax = (q[1].x - q[0].x + (q[2].x - q[3].x)) / n;
    const ay = (q[1].y - q[0].y + (q[2].y - q[3].y)) / n;
    const bx = (q[3].x - q[0].x + (q[2].x - q[1].x)) / n;
    const by = (q[3].y - q[0].y + (q[2].y - q[1].y)) / n;
    const mx = (q[0].x + q[1].x + q[2].x + q[3].x) / 4;
    const my = (q[0].y + q[1].y + q[2].y + q[3].y) / 4;
    const half = SPRITE_SIZE / 2;
    ctx.save();
    ctx.imageSmoothingEnabled = false;
    ctx.transform(ax, ay, bx, by, mx - half * (ax + bx), my - half * (ay + by));
    ctx.drawImage(sprite.canvas, 0, 0);
    ctx.restore();
  } else {
    path(ctx, top);
    ctx.fillStyle = toRgba(lit ? shade.top : below(shade.top, room.z), 1);
    ctx.fill();
  }
  if (edge) {
    path(ctx, top);
    ctx.strokeStyle = edge;
    ctx.lineWidth = 1;
    ctx.stroke();
  } else if (!lit && topPx > EDGE_PX) {
    // A faded roof keeps a hairline of its own border, faded the same,
    // so two rooms side by side stay two.
    path(ctx, top);
    ctx.strokeStyle = toRgba(below(shade.border, room.z), 0.9);
    ctx.lineWidth = 1;
    ctx.stroke();
  }
}

interface Sprite {
  canvas: HTMLCanvasElement;
  mean: Rgb;
}

// Each sector's three sprites, painted once on first use.
const SPRITES = new Map<string, Sprite>();

function spriteOf(room: Room3d): Sprite | null {
  const variant = spriteVariant(room.cell.ex);
  const key = `${room.sector}_${variant}`;
  const hit = SPRITES.get(key);
  if (hit) return hit;
  const canvas = document.createElement('canvas');
  canvas.width = SPRITE_SIZE;
  canvas.height = SPRITE_SIZE;
  const c = canvas.getContext('2d');
  if (!c) return null;
  const pixels = paintSprite(room.sector, variant);
  const img = c.createImageData(SPRITE_SIZE, SPRITE_SIZE);
  new Uint32Array(img.data.buffer).set(pixels);
  c.putImageData(img, 0, 0);
  const sprite = { canvas, mean: spriteMean(pixels) };
  SPRITES.set(key, sprite);
  return sprite;
}

/** A line over a casing, so it reads over a wall, a roof or the bare
 *  ground in any theme. */
function casedLine(
  ctx: CanvasRenderingContext2D,
  a: Pt,
  b: Pt,
  core: string,
  width: number,
  casing: string,
  dash?: number[],
) {
  ctx.save();
  ctx.lineCap = 'butt';
  ctx.beginPath();
  ctx.moveTo(a.x, a.y);
  ctx.lineTo(b.x, b.y);
  ctx.strokeStyle = casing;
  ctx.lineWidth = width + 1.7;
  ctx.stroke();
  if (dash) ctx.setLineDash(dash);
  ctx.strokeStyle = core;
  ctx.lineWidth = width;
  ctx.stroke();
  ctx.restore();
}

/** A small triangle on your roof, up or down, for a stair whose other
 *  end the floors shown leave out. */
function drawStairMark(
  ctx: CanvasRenderingContext2D,
  at: Pt,
  up: boolean,
  color: string,
  s: number,
  ground: string,
) {
  const tip = up ? -1 : 1;
  ctx.beginPath();
  ctx.moveTo(at.x, at.y + tip * s * 0.6);
  ctx.lineTo(at.x + s * 0.6, at.y - tip * s * 0.4);
  ctx.lineTo(at.x - s * 0.6, at.y - tip * s * 0.4);
  ctx.closePath();
  ctx.lineJoin = 'round';
  ctx.lineWidth = 2;
  ctx.strokeStyle = ground;
  ctx.stroke();
  ctx.fillStyle = color;
  ctx.fill();
}

/** A walk on the plane of your roofs: the path from your room through
 *  each step, as the flat styles stroke it, and the ring round the
 *  roof it goes to. It draws over every floor and under your pin. */
function drawWalk(
  ctx: CanvasRenderingContext2D,
  cam: Camera,
  you: Room3d,
  walk: WalkMark,
  inks: MapInks,
) {
  const h = roofAt(0);
  const points = [{ row: you.y, col: you.x }, ...walk.cells].map(({ row, col }) =>
    project(cam, col, row, h),
  );
  // A casing of the ground under the whole path, as the corridors have,
  // so it reads over a roof of its own color.
  ctx.save();
  ctx.lineJoin = 'round';
  ctx.lineCap = 'round';
  ctx.lineWidth = 2 + 1.7;
  ctx.strokeStyle = inks.ground;
  ctx.beginPath();
  points.forEach(({ x, y }, i) => (i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y)));
  ctx.stroke();
  ctx.restore();
  strokeWalkPath(ctx, walk, points);
  ctx.save();
  inkWalkTarget(ctx, walk);
  ctx.lineJoin = 'round';
  path(ctx, square(cam, walk.target.col, walk.target.row, h, TILE / 2 + 3.5 / cam.ppc));
  ctx.stroke();
  ctx.restore();
}

/** Your room. Its roof clears to the ground and takes the accent tint,
 *  as Squares draws your cell, with a ring just outside its edge over a
 *  casing of the ground, and a pin standing on it. Drawn last, so no
 *  floor above hides it, and held apart from any terrain whatever the
 *  accent is. */
function drawYou(ctx: CanvasRenderingContext2D, cam: Camera, r: Room3d, inks: MapInks) {
  const h = roofAt(r.z);
  const hf = TILE / 2;
  path(ctx, square(cam, r.x, r.y, h, hf));
  ctx.fillStyle = inks.ground;
  ctx.fill();
  ctx.fillStyle = inks.accentSoft;
  ctx.fill();
  ctx.lineJoin = 'round';
  path(ctx, square(cam, r.x, r.y, h, hf + 2 / cam.ppc));
  ctx.lineWidth = 4.5;
  ctx.strokeStyle = inks.ground;
  ctx.stroke();
  ctx.lineWidth = 2;
  ctx.strokeStyle = inks.accent;
  ctx.stroke();
  const foot = project(cam, r.x, r.y, h);
  const headY = foot.y - Math.max(10, Math.min(18, TILE * foot.scale * 0.95));
  ctx.lineCap = 'round';
  ctx.beginPath();
  ctx.moveTo(foot.x, foot.y);
  ctx.lineTo(foot.x, headY);
  ctx.lineWidth = 4.5;
  ctx.strokeStyle = inks.ground;
  ctx.stroke();
  ctx.lineWidth = 1.75;
  ctx.strokeStyle = inks.accent;
  ctx.stroke();
  ctx.beginPath();
  ctx.arc(foot.x, headY, 3.8, 0, Math.PI * 2);
  ctx.lineWidth = 3;
  ctx.strokeStyle = inks.ground;
  ctx.stroke();
  ctx.fillStyle = inks.accent;
  ctx.fill();
  ctx.lineCap = 'butt';
}

/** With every floor shown, each floor besides yours carries its number
 *  beside a roof of it that nothing above covers. */
function drawFloorNumbers(
  ctx: CanvasRenderingContext2D,
  cam: Camera,
  scene: Scene,
  inks: MapInks,
  ground: Rgb,
) {
  ctx.save();
  ctx.font = `600 ${inks.labelPx}px ${inks.font}`;
  ctx.textBaseline = 'middle';
  ctx.lineJoin = 'round';
  ctx.lineWidth = 3;
  ctx.strokeStyle = toRgba(ground, 0.9);
  ctx.fillStyle = inks.secondary;
  for (const n of floorNumbers(cam, scene, (label) => ctx.measureText(label).width)) {
    ctx.textAlign = n.align;
    ctx.strokeText(n.label, n.x, n.y);
    ctx.fillText(n.label, n.x, n.y);
  }
  ctx.restore();
}

/** A small needle in the top right corner while the map is turned off
 *  north, its north half in the text color. */
function drawCompass(ctx: CanvasRenderingContext2D, cam: Camera, inks: MapInks) {
  const cx = cam.w - 14;
  const cy = 14;
  const R = 9;
  const n = northOnScreen(cam);
  const px = -n.y;
  const py = n.x;
  const w = 2.6;
  ctx.save();
  ctx.beginPath();
  ctx.arc(cx, cy, R + 1.5, 0, Math.PI * 2);
  ctx.fillStyle = inks.ground;
  ctx.fill();
  ctx.lineWidth = 1;
  ctx.strokeStyle = inks.sep;
  ctx.stroke();
  for (const [sign, color] of [
    [1, inks.text],
    [-1, inks.tertiary],
  ] as const) {
    ctx.beginPath();
    ctx.moveTo(cx + sign * n.x * (R - 2), cy + sign * n.y * (R - 2));
    ctx.lineTo(cx + px * w, cy + py * w);
    ctx.lineTo(cx - px * w, cy - py * w);
    ctx.closePath();
    ctx.fillStyle = color;
    ctx.fill();
  }
  ctx.restore();
}
