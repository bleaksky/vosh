// The terrain colors and the 16 px terrain sprites of the 3D map, ported
// from the game's own atlas (web/static/atlas.js in the Forsaken Lands
// source, GPL v3). The sprites paint from code at run time, so nothing
// is fetched from the game site. Each sector's fill, border and halo are
// the mapPalette table, which the atlas shares value for value.

import { WHITE, mix, parseHex, scaled, type Rgb } from '../theme/color';
import { SECTORS } from './mapPalette';

/** How one terrain paints on a box. */
export interface TerrainShade {
  /** The lit roof. The sector fill is a background tone and too dark
   *  for a face in the light, so the roof lifts from the border toward
   *  the halo, as the atlas does. */
  top: Rgb;
  /** The walls, before the light on each face. */
  side: Rgb;
  /** The sector border. */
  border: Rgb;
}

type SpriteKind =
  | 'wood'
  | 'flag'
  | 'grass'
  | 'forest'
  | 'rock'
  | 'water'
  | 'sand'
  | 'lava'
  | 'snow';

// The atlas sprite for each sector, by index.
const KINDS: SpriteKind[] = [
  'wood', // Inside
  'flag', // City
  'grass', // Field
  'forest', // Forest
  'grass', // Hills
  'rock', // Mountain
  'water', // Water
  'water', // Deep water
  'grass', // Swamp
  'flag', // Air
  'sand', // Desert
  'lava', // Lava
  'snow', // Snow
];

// Truncated, as the atlas rounds its colors.
function whole(c: Rgb): Rgb {
  const ch = (v: number) => Math.max(0, Math.min(255, v | 0));
  return { r: ch(c.r), g: ch(c.g), b: ch(c.b) };
}

// SECTORS holds literal hex colors, so each one parses.
function sector(i: number) {
  const s = SECTORS[i] ?? SECTORS[0];
  return { fill: parseHex(s.fill)!, border: parseHex(s.border)!, halo: parseHex(s.halo)! };
}

/** Each sector's shade, by the index mapPalette gives it. */
export const TERRAIN: TerrainShade[] = KINDS.map((_, i) => {
  const { fill, border, halo } = sector(i);
  return { top: whole(mix(border, halo, 0.3)), side: whole(mix(fill, border, 0.86)), border };
});

// The five step ramp a sprite paints with, dark to light.
function ramp(i: number): Rgb[] {
  const { fill: F, border: B, halo: H } = sector(i);
  return [
    whole(scaled(F, 0.55)),
    whole(mix(F, B, 0.18)),
    whole(mix(F, B, 0.55)),
    whole(mix(B, H, 0.45)),
    whole(mix(H, WHITE, 0.22)),
  ];
}

/** A sprite's width and height in pixels. */
export const SPRITE_SIZE = 16;
const TS = SPRITE_SIZE;

// A stable hash of a cell and a seed, in 0..1.
function h2(x: number, y: number, s: number): number {
  let n = (Math.imul(x, 374761393) + Math.imul(y, 668265263) + Math.imul(s, 1274126177)) | 0;
  n = Math.imul(n ^ (n >>> 13), 1274126177) | 0;
  return ((n ^ (n >>> 16)) >>> 0) / 4294967295;
}

// A 4 by 4 ordered dither.
const B4 = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];
function bay(x: number, y: number): number {
  return (B4[((y & 3) << 2) | (x & 3)] + 0.5) / 16;
}

// One pixel, packed the way ImageData lays out RGBA on a little endian
// machine.
function put(b: Uint32Array, x: number, y: number, c: Rgb) {
  if (x < 0 || y < 0 || x >= TS || y >= TS) return;
  b[y * TS + x] = ((255 << 24) | (c.b << 16) | (c.g << 8) | c.r) >>> 0;
}

type Painter = (b: Uint32Array, p: Rgb[], s: number) => void;

// The atlas painters, line for line.
const PAINT: Record<SpriteKind, Painter> = {
  grass(b, p, s) {
    for (let y = 0; y < TS; y++)
      for (let x = 0; x < TS; x++) put(b, x, y, h2(x, y, s) > 0.88 ? p[2] : p[3]);
    for (let gy = 0; gy < 4; gy++)
      for (let gx = 0; gx < 4; gx++) {
        if (h2(gx, gy, s + 3) < 0.5) continue;
        const cx = gx * 4 + ((h2(gx, gy, s + 5) * 2) | 0);
        const cy = gy * 4 + ((h2(gx, gy, s + 7) * 2) | 0);
        put(b, cx, cy, p[4]);
        put(b, cx - 1, cy + 1, p[4]);
        put(b, cx + 1, cy + 1, p[4]);
      }
  },
  forest(b, p, s) {
    for (let y = 0; y < TS; y++) for (let x = 0; x < TS; x++) put(b, x, y, p[1]);
    for (let i = 0; i < 6; i++) {
      const cx = (h2(i, 0, s) * TS) | 0;
      const cy = (h2(i, 1, s) * TS) | 0;
      for (let dy = -2; dy <= 2; dy++)
        for (let dx = -2; dx <= 2; dx++) {
          if (dx * dx + dy * dy > 4) continue;
          put(b, cx + dx, cy + dy, dx + dy < -1 ? p[3] : p[2]);
        }
      put(b, cx, cy - 2, p[4]);
    }
  },
  flag(b, p, s) {
    for (let y = 0; y < TS; y++)
      for (let x = 0; x < TS; x++) {
        const q = (x >> 3) + (y >> 3) * 2;
        let c = h2(q, 0, s) > 0.5 ? p[3] : p[2];
        if (h2(x, y, s + 4) > 0.93) c = p[4];
        if (x % 8 === 0 || y % 8 === 0) c = p[1];
        put(b, x, y, c);
      }
  },
  wood(b, p, s) {
    for (let y = 0; y < TS; y++) {
      const q = y >> 2;
      const j = (h2(q, 1, s + 4) * TS) | 0;
      for (let x = 0; x < TS; x++) {
        let c = h2(q, 0, s) > 0.5 ? p[3] : p[2];
        if (h2(x, q, s + 2) > 0.9 || y % 4 === 0 || x === j) c = p[1];
        put(b, x, y, c);
      }
    }
  },
  rock(b, p, s) {
    for (let y = 0; y < TS; y++) for (let x = 0; x < TS; x++) put(b, x, y, p[1]);
    for (let i = 0; i < 3; i++) {
      const cx = 2 + ((h2(i, 0, s) * 11) | 0);
      const cy = 2 + ((h2(i, 1, s) * 10) | 0);
      const r = 3 + ((h2(i, 2, s) * 2) | 0);
      for (let dy = -r; dy <= r; dy++)
        for (let dx = -r; dx <= r; dx++) {
          if (Math.abs(dx) + Math.abs(dy) > r) continue;
          put(b, cx + dx, cy + dy, dx + dy < -r / 2 ? p[4] : dx + dy < r / 2 ? p[2] : p[0]);
        }
    }
  },
  water(b, p, s) {
    for (let y = 0; y < TS; y++)
      for (let x = 0; x < TS; x++) put(b, x, y, bay(x, y) > 0.55 ? p[2] : p[3]);
    for (let i = 0; i < 2; i++) {
      const cy = 3 + i * 8 + ((h2(i, 0, s) * 3) | 0);
      const x0 = (h2(i, 1, s) * 10) | 0;
      const ln = 6 + ((h2(i, 2, s) * 6) | 0);
      for (let d = 0; d < ln; d++) put(b, (x0 + d) % TS, cy, p[4]);
    }
  },
  sand(b, p, s) {
    for (let y = 0; y < TS; y++)
      for (let x = 0; x < TS; x++) put(b, x, y, bay(x, y) > 0.6 ? p[3] : p[2]);
    for (let i = 0; i < 2; i++) {
      const cy = 4 + i * 7;
      for (let x = 0; x < TS; x++)
        put(b, x, cy + ((Math.sin(x * 0.4 + h2(i, 1, s) * 6) * 2) | 0), p[4]);
    }
  },
  snow(b, p, s) {
    for (let y = 0; y < TS; y++)
      for (let x = 0; x < TS; x++) put(b, x, y, bay(x, y) > 0.45 ? p[3] : p[4]);
    for (let i = 0; i < 3; i++) {
      const cx = (h2(i, 0, s) * TS) | 0;
      const cy = (h2(i, 1, s) * TS) | 0;
      for (let dy = -2; dy <= 2; dy++)
        for (let dx = -2; dx <= 2; dx++) {
          if (dx * dx + dy * dy > 4) continue;
          put(b, (cx + dx + TS) % TS, (cy + dy + TS) % TS, p[2]);
        }
    }
  },
  lava(b, p, s) {
    const pts: [number, number][] = [];
    for (let i = 0; i < 5; i++) pts.push([h2(i, 0, s) * TS, h2(i, 1, s) * TS]);
    for (let y = 0; y < TS; y++)
      for (let x = 0; x < TS; x++) {
        let d0 = 1e9;
        let d1 = 1e9;
        for (const [px, py] of pts) {
          const dd = Math.hypot(x - px, y - py);
          if (dd < d0) {
            d1 = d0;
            d0 = dd;
          } else if (dd < d1) d1 = dd;
        }
        const sv = (1 - Math.min(1, (d1 - d0) / 7)) * 4;
        let ii = sv | 0;
        const f = sv - ii;
        if (ii < 4 && Math.abs(f - 0.5) < 0.3) {
          if (f > bay(x, y)) ii++;
        } else if (f > 0.5 && ii < 4) ii++;
        put(b, x, y, p[Math.min(4, Math.max(1, ii))]);
      }
  },
};

/** The pixels of one sector's sprite, variant 0, 1 or 2. */
export function paintSprite(sectorIndex: number, variant: number): Uint32Array {
  const i = KINDS[sectorIndex] ? sectorIndex : 0;
  const b = new Uint32Array(TS * TS);
  PAINT[KINDS[i]](b, ramp(i), i * 31 + variant * 7 + 3);
  return b;
}

/** Which of the three variants a room paints, the same at every visit.
 *  The atlas hashes where a room stands in the world. A packet's grid
 *  centers on you and moves with each step, so the rooms its exits lead
 *  to, `ex` in the packet, stand in for that. A room with no exit there
 *  paints variant 0. */
export function spriteVariant(ex: Record<string, number | string> | undefined): number {
  const vnums = Object.values(ex ?? {})
    .map(Number)
    .filter(Number.isInteger)
    .sort((a, b) => a - b);
  if (vnums.length === 0) return 0;
  const seed = vnums.reduce((n, v) => (Math.imul(n, 31) + v) | 0, 0);
  return (h2(seed, vnums.length, 77) * 3) | 0;
}

/** The average color of a sprite. Its walls paint in it, so a room
 *  reads as one block of material. */
export function spriteMean(pixels: Uint32Array): Rgb {
  let r = 0;
  let g = 0;
  let b = 0;
  for (const px of pixels) {
    r += px & 255;
    g += (px >>> 8) & 255;
    b += (px >>> 16) & 255;
  }
  const n = pixels.length || 1;
  return { r: r / n, g: g / n, b: b / n };
}
