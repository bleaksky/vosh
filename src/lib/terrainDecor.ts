// Terrain decorations ported from the Forsaken Lands web map
// (web/static/map.js). Paints biome-specific shapes (trees, peaks, water
// waves, etc.) on a hash-jittered grid in the void around the rooms so
// the empty area outside the corridor cluster reads as the area's
// landscape, not an indistinct black void.
//
// The renderer is intentionally framework-free: it takes a 2D context, a
// pixel viewport, and a list of "area boxes" with sector ids, and paints
// directly. Both the mapping view and the server view feed it different
// shapes: the mapping view supplies the area's room bbox, and the server
// view supplies the bbox of currently visible cells.

import { hexToRgba } from './mapPalette';

export interface AreaBox {
  /// Bounding box centre in canvas (CSS) pixels.
  cx: number;
  cy: number;
  /// Half-width and half-height of the room cluster in CSS pixels.
  hw: number;
  hh: number;
  /// Sector id 0..12 (matches the SECTORS table in mapPalette).
  sector: number;
  /// Halo color used to tint the decorations.
  haloColor: string;
}

export interface TerrainDecorOptions {
  /// Canvas size in CSS pixels.
  cssWidth: number;
  cssHeight: number;
  /// Approximate pixels-per-cell pitch. Drives feature size and grid
  /// step; matches the room spacing in the host view.
  pitch: number;
}

type Drawer = (
  ctx: CanvasRenderingContext2D,
  cx: number,
  cy: number,
  size: number,
  alpha: number,
  color: string,
) => void;

const DRAWERS: Record<number, Drawer> = {
  // Mountains: triangular peaks with snow caps
  5: (ctx, cx, cy, s, a, c) => {
    ctx.fillStyle = hexToRgba(c, a * 0.6);
    ctx.beginPath();
    ctx.moveTo(cx - s * 0.8, cy + s * 0.4);
    ctx.lineTo(cx - s * 0.1, cy - s * 0.7);
    ctx.lineTo(cx + s * 0.7, cy + s * 0.4);
    ctx.closePath();
    ctx.fill();
    ctx.fillStyle = hexToRgba(c, a * 0.4);
    ctx.beginPath();
    ctx.moveTo(cx + s * 0.1, cy + s * 0.4);
    ctx.lineTo(cx + s * 0.6, cy - s * 0.4);
    ctx.lineTo(cx + s * 1.0, cy + s * 0.4);
    ctx.closePath();
    ctx.fill();
    ctx.fillStyle = hexToRgba('#aaaabc', a * 0.5);
    ctx.beginPath();
    ctx.moveTo(cx - s * 0.25, cy - s * 0.35);
    ctx.lineTo(cx - s * 0.1, cy - s * 0.7);
    ctx.lineTo(cx + s * 0.15, cy - s * 0.35);
    ctx.closePath();
    ctx.fill();
  },
  // Hills: rounded ridgeline bumps
  4: (ctx, cx, cy, s, a, c) => {
    ctx.fillStyle = hexToRgba(c, a * 0.45);
    ctx.beginPath();
    ctx.moveTo(cx - s, cy + s * 0.15);
    ctx.quadraticCurveTo(cx - s * 0.5, cy - s * 0.4, cx, cy + s * 0.05);
    ctx.quadraticCurveTo(cx + s * 0.5, cy - s * 0.3, cx + s, cy + s * 0.15);
    ctx.lineTo(cx + s, cy + s * 0.25);
    ctx.lineTo(cx - s, cy + s * 0.25);
    ctx.closePath();
    ctx.fill();
  },
  // Forest: layered pine tree silhouettes
  3: (ctx, cx, cy, s, a, c) => {
    ctx.fillStyle = hexToRgba(c, a * 0.5);
    ctx.beginPath();
    ctx.moveTo(cx - s * 0.55, cy + s * 0.4);
    ctx.lineTo(cx - s * 0.3, cy - s * 0.3);
    ctx.lineTo(cx - s * 0.05, cy + s * 0.4);
    ctx.closePath();
    ctx.fill();
    ctx.fillStyle = hexToRgba(c, a * 0.65);
    ctx.beginPath();
    ctx.moveTo(cx - s * 0.3, cy + s * 0.4);
    ctx.lineTo(cx, cy - s * 0.65);
    ctx.lineTo(cx + s * 0.3, cy + s * 0.4);
    ctx.closePath();
    ctx.fill();
    ctx.fillStyle = hexToRgba(c, a * 0.45);
    ctx.beginPath();
    ctx.moveTo(cx + s * 0.1, cy + s * 0.4);
    ctx.lineTo(cx + s * 0.35, cy - s * 0.2);
    ctx.lineTo(cx + s * 0.6, cy + s * 0.4);
    ctx.closePath();
    ctx.fill();
  },
  // Water: wavy lines (river/stream)
  6: (ctx, cx, cy, s, a, c) => {
    ctx.strokeStyle = hexToRgba(c, a * 0.55);
    ctx.lineWidth = s * 0.15;
    ctx.lineCap = 'round';
    ctx.beginPath();
    ctx.moveTo(cx - s * 1.2, cy);
    ctx.quadraticCurveTo(cx - s * 0.6, cy - s * 0.3, cx, cy);
    ctx.quadraticCurveTo(cx + s * 0.6, cy + s * 0.3, cx + s * 1.2, cy);
    ctx.stroke();
    ctx.strokeStyle = hexToRgba(c, a * 0.35);
    ctx.lineWidth = s * 0.1;
    ctx.beginPath();
    ctx.moveTo(cx - s * 0.9, cy + s * 0.35);
    ctx.quadraticCurveTo(cx - s * 0.4, cy + s * 0.1, cx + s * 0.1, cy + s * 0.35);
    ctx.quadraticCurveTo(cx + s * 0.5, cy + s * 0.55, cx + s * 0.9, cy + s * 0.35);
    ctx.stroke();
    ctx.lineCap = 'butt';
  },
  // Deep water: triple wavy lines
  7: (ctx, cx, cy, s, a, c) => {
    ctx.strokeStyle = hexToRgba(c, a * 0.5);
    ctx.lineWidth = s * 0.12;
    ctx.lineCap = 'round';
    for (let row = -1; row <= 1; row++) {
      const y = cy + row * s * 0.3;
      ctx.beginPath();
      ctx.moveTo(cx - s, y);
      ctx.quadraticCurveTo(cx - s * 0.5, y - s * 0.2, cx, y);
      ctx.quadraticCurveTo(cx + s * 0.5, y + s * 0.2, cx + s, y);
      ctx.stroke();
    }
    ctx.lineCap = 'butt';
  },
  // Field: grass tufts / rolling lines
  2: (ctx, cx, cy, s, a, c) => {
    ctx.strokeStyle = hexToRgba(c, a * 0.4);
    ctx.lineWidth = s * 0.07;
    ctx.lineCap = 'round';
    for (let i = -1; i <= 1; i++) {
      const x = cx + i * s * 0.3;
      ctx.beginPath();
      ctx.moveTo(x, cy + s * 0.2);
      ctx.quadraticCurveTo(x + i * s * 0.15, cy - s * 0.15, x + i * s * 0.2, cy - s * 0.3);
      ctx.stroke();
    }
    ctx.lineCap = 'butt';
  },
  // Desert: sand dune curves
  10: (ctx, cx, cy, s, a, c) => {
    ctx.fillStyle = hexToRgba(c, a * 0.35);
    ctx.beginPath();
    ctx.moveTo(cx - s, cy + s * 0.15);
    ctx.quadraticCurveTo(cx - s * 0.3, cy - s * 0.35, cx + s * 0.2, cy);
    ctx.quadraticCurveTo(cx + s * 0.6, cy + s * 0.2, cx + s, cy + s * 0.1);
    ctx.lineTo(cx + s, cy + s * 0.2);
    ctx.lineTo(cx - s, cy + s * 0.2);
    ctx.closePath();
    ctx.fill();
  },
  // Snow: frosted peaks
  12: (ctx, cx, cy, s, a, c) => {
    ctx.fillStyle = hexToRgba(c, a * 0.5);
    ctx.beginPath();
    ctx.moveTo(cx - s * 0.6, cy + s * 0.3);
    ctx.lineTo(cx, cy - s * 0.45);
    ctx.lineTo(cx + s * 0.6, cy + s * 0.3);
    ctx.closePath();
    ctx.fill();
  },
  // Swamp: reeds with cattails
  8: (ctx, cx, cy, s, a, c) => {
    ctx.strokeStyle = hexToRgba(c, a * 0.45);
    ctx.lineWidth = s * 0.06;
    for (let i = -1; i <= 1; i++) {
      const x = cx + i * s * 0.25;
      ctx.beginPath();
      ctx.moveTo(x, cy + s * 0.3);
      ctx.lineTo(x + i * s * 0.08, cy - s * 0.3);
      ctx.stroke();
      ctx.fillStyle = hexToRgba(c, a * 0.55);
      ctx.beginPath();
      ctx.ellipse(x + i * s * 0.08, cy - s * 0.35, s * 0.05, s * 0.1, 0, 0, Math.PI * 2);
      ctx.fill();
    }
  },
  // Lava: glowing pools
  11: (ctx, cx, cy, s, a, c) => {
    ctx.fillStyle = hexToRgba(c, a * 0.35);
    ctx.beginPath();
    ctx.ellipse(cx, cy, s * 0.5, s * 0.25, 0, 0, Math.PI * 2);
    ctx.fill();
    ctx.fillStyle = hexToRgba('#ff8800', a * 0.25);
    ctx.beginPath();
    ctx.ellipse(cx, cy, s * 0.25, s * 0.12, 0, 0, Math.PI * 2);
    ctx.fill();
  },
  // City: small building outlines
  1: (ctx, cx, cy, s, a, c) => {
    ctx.strokeStyle = hexToRgba(c, a * 0.3);
    ctx.lineWidth = s * 0.06;
    const bw = s * 0.25;
    const bh = s * 0.35;
    ctx.strokeRect(cx - bw, cy - bh * 0.5, bw * 2, bh);
    ctx.beginPath();
    ctx.moveTo(cx - bw * 1.3, cy - bh * 0.5);
    ctx.lineTo(cx, cy - bh);
    ctx.lineTo(cx + bw * 1.3, cy - bh * 0.5);
    ctx.stroke();
  },
  // Inside: subtle stone dots
  0: (ctx, cx, cy, s, a, c) => {
    ctx.fillStyle = hexToRgba(c, a * 0.2);
    ctx.fillRect(cx - s * 0.1, cy - s * 0.1, s * 0.2, s * 0.2);
  },
  // Air: wispy cloud curves
  9: (ctx, cx, cy, s, a, c) => {
    ctx.strokeStyle = hexToRgba(c, a * 0.3);
    ctx.lineWidth = s * 0.08;
    ctx.lineCap = 'round';
    ctx.beginPath();
    ctx.moveTo(cx - s * 0.6, cy);
    ctx.bezierCurveTo(cx - s * 0.3, cy - s * 0.25, cx + s * 0.3, cy + s * 0.25, cx + s * 0.6, cy);
    ctx.stroke();
    ctx.lineCap = 'butt';
  },
};

function hashXY(x: number, y: number): number {
  let h = (x * 374761393 + y * 668265263) | 0;
  h = ((h ^ (h >> 13)) * 1274126177) | 0;
  return h ^ (h >> 16);
}

/**
 * Paint terrain decorations across the canvas viewport. Decorations
 * fade as they approach a room cluster (so the rooms stay readable),
 * and stop entirely beyond a generous "world" pad around the bbox.
 */
export function drawTerrainDecorations(
  ctx: CanvasRenderingContext2D,
  areas: AreaBox[],
  opts: TerrainDecorOptions,
): void {
  if (areas.length === 0) return;
  const { cssWidth, cssHeight, pitch } = opts;
  if (pitch <= 0) return;

  // Spacing between potential features. Wider spacing keeps the void
  // from looking dense; we keep the same multiplier as the FL web map
  // at its default zoom (~SPACING * 1.8).
  const step = Math.max(24, pitch * 1.6);
  const featureSize = Math.max(10, Math.min(36, pitch * 0.9));
  const fadeZone = pitch * 3;

  // World bounds: stop drawing decorations further than `pad` outside
  // the area bbox so the map isn't surrounded by an infinite forest.
  const pad = pitch * 12;
  let bx0 = Infinity;
  let by0 = Infinity;
  let bx1 = -Infinity;
  let by1 = -Infinity;
  for (const a of areas) {
    if (a.cx - a.hw < bx0) bx0 = a.cx - a.hw;
    if (a.cy - a.hh < by0) by0 = a.cy - a.hh;
    if (a.cx + a.hw > bx1) bx1 = a.cx + a.hw;
    if (a.cy + a.hh > by1) by1 = a.cy + a.hh;
  }
  bx0 -= pad;
  by0 -= pad;
  bx1 += pad;
  by1 += pad;

  const x0 = Math.floor(Math.max(0, bx0) / step) * step;
  const y0 = Math.floor(Math.max(0, by0) / step) * step;
  const x1 = Math.min(cssWidth, bx1);
  const y1 = Math.min(cssHeight, by1);

  for (let gx = x0; gx < x1; gx += step) {
    for (let gy = y0; gy < y1; gy += step) {
      const h = hashXY(gx | 0, gy | 0);
      // Density: ~45% of grid points get a feature.
      if ((h & 0xff) > 115) continue;

      // Find the nearest area; gate on edge distance so we never paint
      // inside a room's bbox.
      let nearest: AreaBox | null = null;
      let bestDist2 = Infinity;
      let bestEdgeDist = 0;
      for (const a of areas) {
        const dx = gx - a.cx;
        const dy = gy - a.cy;
        const d2 = dx * dx + dy * dy;
        if (d2 < bestDist2) {
          bestDist2 = d2;
          nearest = a;
          const ox = Math.abs(dx) - a.hw;
          const oy = Math.abs(dy) - a.hh;
          bestEdgeDist = Math.max(ox, oy);
        }
      }
      if (!nearest || bestEdgeDist < 0) continue;

      let edgeFade = 1;
      if (bestEdgeDist < fadeZone) {
        edgeFade = bestEdgeDist / fadeZone;
        edgeFade *= edgeFade;
      }

      const dist = Math.sqrt(bestDist2);
      let alpha: number;
      if (dist < 120) alpha = 0.85;
      else if (dist < 400) alpha = 0.85 - ((dist - 120) / 280) * 0.35;
      else if (dist < 1000) alpha = 0.5 - ((dist - 400) / 600) * 0.2;
      else alpha = 0.3;
      alpha *= edgeFade;
      if (alpha < 0.02) continue;

      const jx = ((h >> 8) & 0xff) / 255 * step * 0.7 - step * 0.35;
      const jy = ((h >> 16) & 0xff) / 255 * step * 0.7 - step * 0.35;
      const sizeVar = 0.7 + ((h >> 24) & 0x7f) / 127 * 0.6;

      const drawer = DRAWERS[nearest.sector] ?? DRAWERS[2];
      drawer(ctx, gx + jx, gy + jy, featureSize * sizeVar, alpha, nearest.haloColor);
    }
  }
}
