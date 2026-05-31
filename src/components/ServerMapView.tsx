import { memo, useCallback, useEffect, useRef, useState, type ChangeEvent } from 'react';
import { getAreaSnapshot, onGmcp, onMap, onState, type AreaSnapshot } from '../lib/session';
import {
  MAP_COLORS,
  SECTORS,
  UNKNOWN_GLYPH,
  hexToRgba,
  sectorForCode,
  sectorGlyphColor,
} from '../lib/mapPalette';
import { drawTerrainDecorations } from '../lib/terrainDecor';
import { subscribeThemeChanges } from '../lib/theme';

/// One cell of the server-side map grid (player's floor only).
/// Per the Aabahran GMCP wiki:
/// `g[y][x]` carries `{s, e, l, h, ar, f, d, ex}`. Multi-floor rooms
/// live in a separate top-level `zr` array (see `MultiZEntry`), not
/// in the `g` grid.
interface ServerCell {
  /// Exit string like `"nesw"`. Uppercase letter means an exit that
  /// leads off-grid.
  e?: string;
  /// Area vnum this cell belongs to.
  ar?: number | string;
  /// Light level 0..4.
  l?: number | string;
  /// `1` only on the player's own cell, omitted otherwise.
  h?: number;
  /// Flag chars (e.g. `"$bthsq"` for safe/bank/trainer/healer/shop/quest).
  f?: string;
  /// Sector index. Aabahran sends digit codes (`0..9`) as a JSON
  /// **number** and letter codes (`a..c`) as a string. We normalize
  /// to a string at read time via sectorCodeOf().
  s?: string | number;
  /// Door state dict keyed by direction.
  d?: Record<string, unknown>;
  /// Exit destinations keyed by direction.
  ex?: Record<string, number | string>;
}

/// Off-floor room entry. Aabahran ships ±1-floor rooms in
/// `payload.a` (above) and `payload.b` (below); deeper floors come
/// through `payload.zr` per the GMCP wiki, with an explicit `z`
/// floor-delta. `x` and `y` share the same 0-indexed `[y][x]`
/// coordinate space as `g`.
interface OffFloorEntry {
  x: number;
  y: number;
  /// Present in `zr` entries; absent in `a` / `b` entries (where the
  /// array name implies +1 / -1).
  z?: number;
  /// Sector index — number for digit codes, string for letter codes.
  /// See ServerCell.s for the rationale.
  s?: string | number;
  e?: string;
  l?: number | string;
  ar?: number | string;
  f?: string;
  d?: Record<string, unknown>;
  ex?: Record<string, number | string>;
}

interface AreaInfo {
  name?: string;
  color?: string;
}

interface MapTilesPayload {
  r?: number;
  t?: string;
  g?: Record<string, Record<string, ServerCell | null | string>>;
  /// Rooms one floor above the player's current floor, in the same
  /// `[y][x]` coordinate space as `g`. Empty/absent when no above-
  /// floor rooms are within radius.
  a?: OffFloorEntry[];
  /// Rooms one floor below.
  b?: OffFloorEntry[];
  /// Per the GMCP wiki, the multi-Z array carries rooms at any
  /// floor delta (positive=above, negative=below) via an explicit
  /// `z` field on each entry. Aabahran's current production server
  /// uses `a`/`b` for ±1; `zr` should still be honored if it shows
  /// up so deeper-floor rooms render too.
  zr?: OffFloorEntry[];
  areas?: Record<string, AreaInfo>;
}

type Style = 'squares' | 'glyphs' | 'tileset';

const STYLE_KEY = 'vosh.layout.serverMapStyle';
const TILESET_KEY = 'vosh.layout.serverMapTileset';
const ZOOM_KEY = 'vosh.layout.serverMapZoom';

// Zoom multiplier applied to the base 20-pixel pitch. 1.0 = default
// (20px cells), 2.0 = 40px, 0.5 = 10px. Stepping at 0.25 increments
// keeps cell sizes on whole-pixel boundaries.
const ZOOM_MIN = 0.5;
const ZOOM_MAX = 3.0;
const ZOOM_STEP = 0.25;

function loadStyle(): Style {
  try {
    const value = localStorage.getItem(STYLE_KEY);
    if (value === 'glyphs' || value === 'tileset') return value;
    return 'squares';
  } catch {
    return 'squares';
  }
}

function loadTileset(): string | null {
  try {
    return localStorage.getItem(TILESET_KEY);
  } catch {
    return null;
  }
}

function loadZoom(): number {
  try {
    const v = Number(localStorage.getItem(ZOOM_KEY));
    if (!Number.isFinite(v) || v <= 0) return 1.0;
    return Math.max(ZOOM_MIN, Math.min(ZOOM_MAX, v));
  } catch {
    return 1.0;
  }
}

function clampZoom(z: number): number {
  // Snap to the nearest step to avoid drift from arithmetic on
  // wheel-delta increments accumulating sub-step fractions.
  const snapped = Math.round(z / ZOOM_STEP) * ZOOM_STEP;
  return Math.max(ZOOM_MIN, Math.min(ZOOM_MAX, snapped));
}

// Default sector code order in a horizontal sprite strip. A tileset PNG
// supplied by the user is assumed to lay tiles out left-to-right in this
// order.
const SECTOR_ORDER: string[] = ['0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'c'];

function parseTextGrid(text: string | undefined): string[] {
  if (!text) return [];
  return text.split('|').filter((row) => row.length > 0);
}

function getCell(payload: MapTilesPayload, row: number, col: number): ServerCell | null {
  if (!payload.g) return null;
  const r = payload.g[String(row)];
  if (!r) return null;
  const v = r[String(col)];
  if (!v || typeof v === 'string') return null;
  return v;
}

function gridDims(payload: MapTilesPayload): { rows: number; cols: number } {
  if (payload.g) {
    let maxRow = 0;
    let maxCol = 0;
    for (const [rKey, row] of Object.entries(payload.g)) {
      const r = Number(rKey);
      if (Number.isFinite(r) && r > maxRow) maxRow = r;
      for (const cKey of Object.keys(row)) {
        const c = Number(cKey);
        if (Number.isFinite(c) && c > maxCol) maxCol = c;
      }
    }
    if (maxRow > 0 && maxCol > 0) return { rows: maxRow, cols: maxCol };
  }
  const text = parseTextGrid(payload.t);
  if (text.length > 0) {
    return { rows: text.length, cols: Math.max(...text.map((r) => r.length)) };
  }
  return { rows: 0, cols: 0 };
}

function hasExit(cell: ServerCell, dir: 'n' | 's' | 'e' | 'w'): boolean {
  return Boolean(cell.e && cell.e.toLowerCase().includes(dir));
}

// Aabahran's GMCP payload serializes digit sector codes (0..9) as
// JSON numbers and letter codes (a..c) as strings. Without this
// coercion the `0` cells (Inside rooms — the most common terrain
// indoors) would short-circuit the `!sectorCode` falsy check below
// and skip rendering entirely, leaving every indoor room invisible.
function sectorCodeOf(s: unknown): string {
  if (s === null || s === undefined) return '';
  return String(s);
}

export function ServerMapView() {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const fileInputRef = useRef<HTMLInputElement | null>(null);
  const [tiles, setTiles] = useState<MapTilesPayload | null>(null);
  const [style, setStyle] = useState<Style>(loadStyle);
  const [tilesetUrl, setTilesetUrl] = useState<string | null>(loadTileset);
  const [tilesetImage, setTilesetImage] = useState<HTMLImageElement | null>(null);
  const [tilesetError, setTilesetError] = useState<string | null>(null);
  const [zoom, setZoom] = useState<number>(loadZoom);
  // Snapshot of the persistent mapping store. We use it to translate the
  // player-centric Map.Tiles grid into stable world coordinates so cells
  // do not shift on canvas as the player walks.
  const [snapshot, setSnapshot] = useState<AreaSnapshot | null>(null);
  // Bumps when the theme changes so the draw effect re-runs and the
  // canvas picks up the new --c-surface / --c-accent CSS vars that
  // MAP_COLORS reads through its getters.
  const [themeVersion, setThemeVersion] = useState(0);
  const lastRefreshRef = useRef<number>(0);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    subscribeThemeChanges(() => {
      setThemeVersion((v) => v + 1);
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  const refreshSnapshot = useCallback(async () => {
    try {
      const snap = await getAreaSnapshot();
      setSnapshot(snap);
    } catch {
      setSnapshot(null);
    }
  }, []);

  useEffect(() => {
    try {
      localStorage.setItem(STYLE_KEY, style);
    } catch {
      // ignore
    }
  }, [style]);

  useEffect(() => {
    try {
      localStorage.setItem(ZOOM_KEY, String(zoom));
    } catch {
      // ignore
    }
  }, [zoom]);

  // Ctrl/Cmd + wheel zooms in/out, mirroring the convention used by
  // map apps. Attached non-passively so we can preventDefault and stop
  // the browser from scrolling the surrounding pane in lieu of zooming.
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const handler = (e: WheelEvent) => {
      if (!e.ctrlKey && !e.metaKey) return;
      e.preventDefault();
      const direction = e.deltaY < 0 ? 1 : -1;
      setZoom((z) => clampZoom(z + direction * ZOOM_STEP));
    };
    el.addEventListener('wheel', handler, { passive: false });
    return () => el.removeEventListener('wheel', handler);
  }, []);

  useEffect(() => {
    if (!tilesetUrl) {
      setTilesetImage(null);
      return;
    }
    const img = new Image();
    img.onload = () => {
      setTilesetImage(img);
      setTilesetError(null);
    };
    img.onerror = () => {
      setTilesetImage(null);
      setTilesetError('failed to decode tileset image');
    };
    img.src = tilesetUrl;
  }, [tilesetUrl]);

  useEffect(() => {
    let unsubGmcp: (() => void) | undefined;
    let unsubMap: (() => void) | undefined;
    let unsubState: (() => void) | undefined;

    onGmcp((payload) => {
      if (payload.package === 'Map.Tiles') {
        setTiles((payload.data ?? {}) as MapTilesPayload);
      }
    }).then((fn) => {
      unsubGmcp = fn;
    });

    onMap(() => {
      const now = Date.now();
      if (now - lastRefreshRef.current < 80) return;
      lastRefreshRef.current = now;
      void refreshSnapshot();
    }).then((fn) => {
      unsubMap = fn;
    });

    onState((payload) => {
      if (payload.kind === 'disconnected') {
        setTiles(null);
        setSnapshot(null);
      } else if (payload.kind === 'connected') {
        void refreshSnapshot();
      }
    }).then((fn) => {
      unsubState = fn;
    });

    void refreshSnapshot();

    return () => {
      unsubGmcp?.();
      unsubMap?.();
      unsubState?.();
    };
  }, [refreshSnapshot]);

  useEffect(() => {
    const canvas = canvasRef.current;
    const container = containerRef.current;
    if (!canvas || !container) return;

    const draw = () => {
      const dpr = window.devicePixelRatio || 1;
      const cssWidth = container.clientWidth;
      const cssHeight = container.clientHeight;
      // Only resize the backing buffer. CSS keeps the display size
      // pinned to the container via width:100%/height:100% so the
      // canvas tracks layout reflows (e.g. the tileset-bar appearing
      // when style flips) without needing the inline style to be
      // refreshed in lockstep.
      const targetW = Math.max(1, cssWidth * dpr);
      const targetH = Math.max(1, cssHeight * dpr);
      if (canvas.width !== targetW || canvas.height !== targetH) {
        canvas.width = targetW;
        canvas.height = targetH;
      }
      const ctx = canvas.getContext('2d');
      if (!ctx) return;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.fillStyle = MAP_COLORS.bg;
      ctx.fillRect(0, 0, cssWidth, cssHeight);

      if (!tiles) {
        ctx.fillStyle = '#6e7681';
        ctx.font = '12px monospace';
        ctx.fillText('waiting for Map.Tiles GMCP push', 10, 22);
        return;
      }

      const { rows, cols } = gridDims(tiles);
      if (rows === 0 || cols === 0) {
        ctx.fillStyle = '#6e7681';
        ctx.font = '12px monospace';
        ctx.fillText('Map.Tiles payload has no grid yet', 10, 22);
        return;
      }
      const centerR = Math.floor((rows + 1) / 2);
      const centerC = Math.floor((cols + 1) / 2);

      // Pick the dominant sector from the visible cells. Drives both the
      // terrain decorations in the void and the watermark tint.
      const sectorCounts: Record<string, number> = {};
      for (const rowKey of Object.keys(tiles.g ?? {})) {
        const colMap = tiles.g?.[rowKey];
        if (!colMap) continue;
        for (const cellKey of Object.keys(colMap)) {
          const cell = colMap[cellKey];
          if (!cell || typeof cell === 'string') continue;
          const code = cell.s ?? '';
          if (!code) continue;
          sectorCounts[code] = (sectorCounts[code] ?? 0) + 1;
        }
      }
      let dominantCode = '0';
      let bestCount = 0;
      for (const [code, count] of Object.entries(sectorCounts)) {
        if (count > bestCount) {
          bestCount = count;
          dominantCode = code;
        }
      }
      const sector = sectorForCode(dominantCode) ?? sectorForCode('0');
      const halo = sector?.halo ?? '#7fb4ca';
      let dominantSectorId = 0;
      for (const [id, theme] of Object.entries(SECTORS)) {
        if (theme === sector) {
          dominantSectorId = Number(id);
          break;
        }
      }

      // Terrain decorations in the void around the visible cell cluster.
      // We don't have the area's true world bbox here; approximate it
      // from the rendered tile grid.
      {
        const anchorPitch = computeAnchor(
          snapshot,
          tiles,
          rows,
          cols,
          centerR,
          centerC,
          cssWidth,
          cssHeight,
          zoom,
        );
        const halfW = (cols / 2) * anchorPitch.pitch;
        const halfH = (rows / 2) * anchorPitch.pitch;
        drawTerrainDecorations(
          ctx,
          [
            {
              cx: anchorPitch.playerX,
              cy: anchorPitch.playerY,
              hw: halfW,
              hh: halfH,
              sector: dominantSectorId,
              haloColor: halo,
            },
          ],
          { cssWidth, cssHeight, pitch: anchorPitch.pitch },
        );
      }

      const anchor = computeAnchor(
        snapshot,
        tiles,
        rows,
        cols,
        centerR,
        centerC,
        cssWidth,
        cssHeight,
        zoom,
      );

      if (style === 'tileset') {
        drawTileset(
          ctx,
          cssWidth,
          cssHeight,
          tiles,
          rows,
          cols,
          centerR,
          centerC,
          tilesetImage,
          anchor,
        );
      } else if (style === 'squares') {
        drawSquares(ctx, cssWidth, cssHeight, tiles, rows, cols, centerR, centerC, anchor);
      }
      // Glyph mode: canvas paints just the background + terrain halo.
      // The actual character grid is rendered via <GlyphsOverlay /> in
      // the JSX below so it tiles in real terminal-cell pitch (1ch ×
      // 1em) using the app font, matching tintin's character-grid map.
    };

    draw();
    // Layout for the mode-toggle case (e.g. squares -> tileset adds
    // the tileset-bar above the canvas-host) can take a frame or two
    // to settle. Schedule a couple of follow-up draws across short
    // deadlines so at least one lands on the post-reflow size.
    const raf = requestAnimationFrame(draw);
    const t1 = window.setTimeout(draw, 80);
    const t2 = window.setTimeout(draw, 240);
    const observer = new ResizeObserver(draw);
    observer.observe(container);
    window.addEventListener('resize', draw);
    return () => {
      cancelAnimationFrame(raf);
      window.clearTimeout(t1);
      window.clearTimeout(t2);
      observer.disconnect();
      window.removeEventListener('resize', draw);
    };
  }, [tiles, style, tilesetImage, snapshot, themeVersion, zoom]);

  const handleLoadTileset = (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => {
      const url = String(reader.result ?? '');
      setTilesetUrl(url);
      try {
        localStorage.setItem(TILESET_KEY, url);
      } catch {
        // ignore (quota or private mode)
      }
    };
    reader.onerror = () => setTilesetError('file read failed');
    reader.readAsDataURL(file);
  };

  const clearTileset = () => {
    setTilesetUrl(null);
    setTilesetImage(null);
    try {
      localStorage.removeItem(TILESET_KEY);
    } catch {
      // ignore
    }
  };

  return (
    <div className="server-view">
      <div className="map-subhead">
        <span>{tiles ? `radius ${tiles.r ?? '?'}` : 'waiting for server map'}</span>
        <div className="map-mode-toggle">
          <button
            type="button"
            aria-pressed={style === 'squares'}
            onClick={() => setStyle('squares')}
          >
            squares
          </button>
          <button
            type="button"
            aria-pressed={style === 'glyphs'}
            onClick={() => setStyle('glyphs')}
          >
            glyphs
          </button>
          <button
            type="button"
            aria-pressed={style === 'tileset'}
            onClick={() => setStyle('tileset')}
          >
            tileset
          </button>
        </div>
        <div className="map-zoom" title="Ctrl+wheel over the map also zooms">
          <button
            type="button"
            aria-label="zoom out"
            disabled={zoom <= ZOOM_MIN + 1e-6}
            onClick={() => setZoom((z) => clampZoom(z - ZOOM_STEP))}
          >
            −
          </button>
          <span className="map-zoom-value">{Math.round(zoom * 100)}%</span>
          <button
            type="button"
            aria-label="zoom in"
            disabled={zoom >= ZOOM_MAX - 1e-6}
            onClick={() => setZoom((z) => clampZoom(z + ZOOM_STEP))}
          >
            +
          </button>
          <button
            type="button"
            aria-label="reset zoom"
            disabled={Math.abs(zoom - 1) < 1e-6}
            onClick={() => setZoom(1)}
            title="reset to 100%"
          >
            ⤺
          </button>
        </div>
      </div>
      {style === 'tileset' && (
        <div className="tileset-bar">
          <input
            ref={fileInputRef}
            type="file"
            accept="image/png,image/jpeg,image/webp"
            onChange={handleLoadTileset}
            hidden
          />
          <button type="button" onClick={() => fileInputRef.current?.click()}>
            load tileset
          </button>
          {tilesetUrl && (
            <button type="button" onClick={clearTileset}>
              clear
            </button>
          )}
          <span className="tileset-status">
            {tilesetError
              ? tilesetError
              : tilesetImage
                ? `tileset loaded (${tilesetImage.naturalWidth}x${tilesetImage.naturalHeight})`
                : tilesetUrl
                  ? 'loading...'
                  : 'horizontal strip of 13 tiles in sector order 0..9, a, b, c'}
          </span>
        </div>
      )}
      <div ref={containerRef} className="map-canvas-host">
        <canvas ref={canvasRef} />
        {style === 'glyphs' && tiles && <GlyphsOverlay payload={tiles} zoom={zoom} />}
      </div>
    </div>
  );
}

interface Anchor {
  /// Pitch in pixels per cell.
  pitch: number;
  /// Canvas pixel where the player cell (centerR, centerC) sits.
  playerX: number;
  playerY: number;
  /// True when the anchor is locked to the player's known absolute world
  /// coords (so cells stay put across walks). False means we fell back to
  /// player-centered because the world coord is not known.
  worldLocked: boolean;
}

/// Pick the pitch and the anchor point for the player cell. The camera
/// follows the player: the player cell always sits at the canvas center,
/// floored to integer pixels so cells render on the same pixel grid every
/// frame. The pitch is also integer; multiplying integer cell offsets by
/// integer pitch lands every neighbor on a clean grid line.
function computeAnchor(
  _snapshot: AreaSnapshot | null,
  _payload: MapTilesPayload,
  _rows: number,
  _cols: number,
  _centerR: number,
  _centerC: number,
  cssWidth: number,
  cssHeight: number,
  zoom: number = 1,
): Anchor {
  // Same base pitch as the mapping view so both modes render at the
  // same scale; the user's zoom multiplier scales it up or down.
  // Floored to integer pixels so cells stay on a clean grid.
  const pitch = Math.max(4, Math.floor(20 * zoom));
  return {
    pitch,
    playerX: Math.floor(cssWidth / 2),
    playerY: Math.floor(cssHeight / 2),
    worldLocked: false,
  };
}

function drawSquares(
  ctx: CanvasRenderingContext2D,
  _cssWidth: number,
  _cssHeight: number,
  payload: MapTilesPayload,
  rows: number,
  cols: number,
  centerR: number,
  centerC: number,
  anchor: Anchor,
) {
  const { pitch, playerX, playerY } = anchor;
  const size = Math.max(8, Math.floor(pitch * 0.55));
  // Place each grid cell relative to the player's canvas position so the
  // ROOM at world coord (x, y) keeps its on-screen position across pushes.
  // Cells outside the populated bounding box are still drawn but ignored
  // by hit-testing in this Phase 7 cut.
  const ox = Math.floor(playerX - centerC * pitch);
  const oy = Math.floor(playerY - centerR * pitch);

  // Corridors under the squares, single thin pass like the FL web map.
  ctx.strokeStyle = '#474b55';
  ctx.lineWidth = 1.25;
  ctx.beginPath();
  for (let r = 1; r <= rows; r++) {
    for (let c = 1; c <= cols; c++) {
      const cell = getCell(payload, r, c);
      if (!cell) continue;
      const cx = ox + c * pitch;
      const cy = oy + r * pitch;
      if (hasExit(cell, 'n') && getCell(payload, r - 1, c)) {
        ctx.moveTo(cx, cy);
        ctx.lineTo(cx, cy - pitch);
      }
      if (hasExit(cell, 'e') && getCell(payload, r, c + 1)) {
        ctx.moveTo(cx, cy);
        ctx.lineTo(cx + pitch, cy);
      }
      if (hasExit(cell, 's') && getCell(payload, r + 1, c)) {
        ctx.moveTo(cx, cy);
        ctx.lineTo(cx, cy + pitch);
      }
      if (hasExit(cell, 'w') && getCell(payload, r, c - 1)) {
        ctx.moveTo(cx, cy);
        ctx.lineTo(cx - pitch, cy);
      }
    }
  }
  ctx.stroke();

  // Off-floor: cells THEN lines, both drawn BEFORE same-floor cells.
  // Same-floor cells render last and wipe their cell area, so any
  // off-floor line crossing under a same-floor cell gets hidden —
  // the line "stops at" the same-floor cell visually. Off-floor
  // lines stay visible inside off-floor cells (translucent) and in
  // empty grid positions where no same-floor cell sits.
  drawOffFloorCells(ctx, payload.a, ox, oy, pitch, size);
  drawOffFloorCells(ctx, payload.b, ox, oy, pitch, size);
  if (Array.isArray(payload.zr)) {
    drawOffFloorCells(ctx, payload.zr, ox, oy, pitch, size);
  }
  drawOffFloorOverlay(ctx, payload.a, ox, oy, pitch);
  drawOffFloorOverlay(ctx, payload.b, ox, oy, pitch);
  if (Array.isArray(payload.zr)) {
    drawOffFloorOverlay(ctx, payload.zr, ox, oy, pitch);
  }

  // Squares, FL web map style: dim sector fill + 0.8-alpha sector border,
  // origin gets a yellow glow + bright yellow border. Each cell's alpha
  // tracks Manhattan distance from the player so the player sits in a
  // bright pool that fades outward.
  for (let r = 1; r <= rows; r++) {
    for (let c = 1; c <= cols; c++) {
      const cell = getCell(payload, r, c);
      if (!cell) continue;
      const cx = ox + c * pitch;
      const cy = oy + r * pitch;
      const isCenter = r === centerR && c === centerC;
      const sector = sectorForCode(sectorCodeOf(cell.s));
      const dist = Math.abs(r - centerR) + Math.abs(c - centerC);
      const depth = depthAlphaForRing(dist);

      ctx.save();
      // Wipe the background under the cell first so corridor lines
      // drawn underneath don't bleed through the (less-than-fully-
      // opaque) sector fill. Without this, every distance-faded cell
      // shows a faint corridor stripe across it.
      ctx.fillStyle = MAP_COLORS.bg;
      ctx.fillRect(cx - size / 2, cy - size / 2, size, size);

      if (isCenter) {
        // Player cell follows the same fill+border convention as a
        // sector tile, just in pink: dim pink interior with a bright
        // pink outline. Full alpha so it stays bright against the
        // depth-faded neighbors.
        ctx.fillStyle = MAP_COLORS.originFill;
        ctx.fillRect(cx - size / 2, cy - size / 2, size, size);
        ctx.strokeStyle = MAP_COLORS.origin;
        ctx.lineWidth = 1;
        ctx.strokeRect(cx - size / 2, cy - size / 2, size, size);
      } else {
        ctx.globalAlpha = depth;
        ctx.fillStyle = sector.fill;
        ctx.fillRect(cx - size / 2, cy - size / 2, size, size);
        ctx.strokeStyle = hexToRgba(sector.border, 0.8);
        ctx.lineWidth = 1;
        ctx.strokeRect(cx - size / 2, cy - size / 2, size, size);
      }
      ctx.restore();

      if (!isCenter) {
        const exits = (cell.e ?? '').toLowerCase();
        if (exits.includes('u') || exits.includes('d')) {
          ctx.fillStyle = MAP_COLORS.text;
          ctx.font = `${Math.max(7, Math.floor(size * 0.55))}px monospace`;
          ctx.textAlign = 'center';
          ctx.textBaseline = 'middle';
          if (exits.includes('u')) {
            ctx.fillText('▲', cx, cy - size * 0.25);
          }
          if (exits.includes('d')) {
            ctx.fillText('▼', cx, cy + size * 0.25);
          }
        }
      }
    }
  }
}

// Pass A: only the dim cell fills. Drawn under everything;
// same-floor cells will paint over off-floor cells at overlapping
// positions. No border outline — the corridor lines drawn over the
// top in pass B carry the connectivity signal.
//
// Off-floor cells render at a uniform alpha regardless of distance
// from the player. The same-floor distance ramp deliberately doesn't
// apply here: a room two floors above shouldn't get *more* visible
// just because it's near the player's projected coords on this
// floor.
function drawOffFloorCells(
  ctx: CanvasRenderingContext2D,
  entries: OffFloorEntry[] | undefined,
  ox: number,
  oy: number,
  pitch: number,
  size: number,
) {
  if (!Array.isArray(entries) || entries.length === 0) return;
  const half = size / 2;
  for (const entry of entries) {
    const cx = ox + entry.x * pitch;
    const cy = oy + entry.y * pitch;
    const sector = sectorForCode(sectorCodeOf(entry.s));
    ctx.save();
    // Faint enough that off-floor rooms read as background context
    // without competing with same-floor cells; corridor lines in
    // pass B carry the connectivity signal.
    ctx.globalAlpha = 0.22;
    ctx.fillStyle = sector.border;
    ctx.fillRect(cx - half, cy - half, size, size);
    ctx.restore();
  }
}

// Pass B: corridor lines (full pitch when both endpoints in the
// off-floor data, half-pitch stubs otherwise). Drawn AFTER same-
// floor cells so off-floor connectivity stays visible no matter
// what's underneath.
function drawOffFloorOverlay(
  ctx: CanvasRenderingContext2D,
  entries: OffFloorEntry[] | undefined,
  ox: number,
  oy: number,
  pitch: number,
) {
  if (!Array.isArray(entries) || entries.length === 0) return;
  const reach = Math.floor(pitch / 2);
  const byCoord = new Map<string, OffFloorEntry>();
  for (const e of entries) byCoord.set(`${e.x},${e.y}`, e);

  // Lines: full pitch between connected pairs, half-pitch stubs for
  // exits whose neighbor isn't in this push (so isolated off-floor
  // cells still announce their connections). Stroke at full alpha
  // so the connectivity signal stays loud — distance fade is for
  // the cell fill, not the line.
  ctx.save();
  ctx.lineWidth = 1.25;
  ctx.strokeStyle = '#474b55';
  ctx.globalAlpha = 1;
  ctx.beginPath();
  for (const entry of entries) {
    const cx = ox + entry.x * pitch;
    const cy = oy + entry.y * pitch;
    const exits = (entry.e ?? '').toLowerCase();
    if (exits.includes('n')) {
      const reachLen = byCoord.has(`${entry.x},${entry.y - 1}`) ? pitch : reach;
      ctx.moveTo(cx, cy);
      ctx.lineTo(cx, cy - reachLen);
    }
    if (exits.includes('s')) {
      const reachLen = byCoord.has(`${entry.x},${entry.y + 1}`) ? pitch : reach;
      ctx.moveTo(cx, cy);
      ctx.lineTo(cx, cy + reachLen);
    }
    if (exits.includes('e')) {
      const reachLen = byCoord.has(`${entry.x + 1},${entry.y}`) ? pitch : reach;
      ctx.moveTo(cx, cy);
      ctx.lineTo(cx + reachLen, cy);
    }
    if (exits.includes('w')) {
      const reachLen = byCoord.has(`${entry.x - 1},${entry.y}`) ? pitch : reach;
      ctx.moveTo(cx, cy);
      ctx.lineTo(cx - reachLen, cy);
    }
  }
  ctx.stroke();
  ctx.restore();
  // Up/down arrows are intentionally NOT rendered on off-floor
  // cells. They appear on the player's same-floor cell when needed
  // (the renderer for `g` cells handles that). Off-floor rooms are
  // already off-axis by definition; adding vertical arrows inside
  // them is redundant and visually noisy.
}

// Same fade table as the mapping view's BFS distance, but keyed to a
// ring index since the server payload doesn't ship full graph data.
function depthAlphaForRing(d: number): number {
  if (d === 0) return 1;
  if (d <= 2) return 0.9;
  if (d <= 4) return 0.72;
  if (d <= 6) return 0.55;
  if (d <= 9) return 0.4;
  return 0.28;
}

// Player marker color. xterm color 220 is the bright gold tintin
// uses in its `\e[1;38;5;220m@\e[0m` marker. Hardcoded here so the
// glyph overlay does not need a full xterm-256 lookup table.
const PLAYER_COLOR = '#ffd700';

// Connect-glyph override: rooms with vertical exits get a marker
// instead of the sector glyph. Matches tintin's ui_connect_glyph
// behavior for up/down only — N/S/E/W connectivity reads from the
// neighboring cells, not from a glyph swap.
function connectGlyph(exits: string | undefined): string | null {
  const e = (exits ?? '').toLowerCase();
  const hasU = e.includes('u');
  const hasD = e.includes('d');
  if (hasU && hasD) return '%';
  if (hasU) return '/';
  if (hasD) return 'v';
  return null;
}

// Tintin dim ladder: 0 (full) within 2 cells of player, 1 (mid)
// within 5, 2 (faint) beyond. Dark rooms (light ≤ 1) bump one tier
// to communicate "you can barely see in here."
function dimLevel(dr: number, dc: number, light: number | string | undefined): number {
  const dist = Math.abs(dr) + Math.abs(dc);
  let lvl = dist <= 2 ? 0 : dist <= 5 ? 1 : 2;
  const l = typeof light === 'number' ? light : Number(light);
  if (Number.isFinite(l) && l <= 1) lvl = Math.min(2, lvl + 1);
  return lvl;
}

// HTML glyph overlay.
//
// Render model: each room takes one glyph cell, and adjacent rooms
// are separated by a gap cell that either holds a box-drawing
// connection (─, │, ┼) if both rooms share that exit or stays blank.
// The output grid is therefore (2*rows - 1) × (2*cols - 1) so the
// connection cells have somewhere to live.
//
// The player cell anchors to the parent's geometric center via a
// CSS calc() translate, so the map scrolls around the player as
// they walk. Each cell measures 1ch × 1em (line-height: 1) so the
// math is a clean character count.
//
// Layers, painted in order:
//   1. Off-floor rooms (a / b / zr) — dim sector glyph as a
//      background hint so the player can see structure above and
//      below the current floor. Rendered at low alpha via CSS.
//   2. Same-floor rooms — full color sector glyph; overrides any
//      off-floor cell at the same coords.
//   3. Connection cells — drawn between two adjacent same-floor
//      rooms when both rooms list the corresponding exit.
//   4. Player marker (`@`) — bold yellow, always on top.
type FloorKind = 'same' | 'above' | 'below' | 'far';
interface GlyphCell {
  glyph: string;
  color: string;
  isPlayer: boolean;
  floor: FloorKind;
}
const EMPTY_CELL: GlyphCell = {
  glyph: ' ',
  color: 'transparent',
  isPlayer: false,
  floor: 'same',
};

// Wrapped in React.memo so a Char.Vitals / Room.Info / Room.Chars
// burst during a movement (which re-renders ServerMapView when its
// snapshot or themeVersion shifts) does not re-run the 60×60 grid
// build + ~3500-span DOM diff. Only changes to the tiles payload
// reference or the zoom value trigger a real rebuild.
const GlyphsOverlay = memo(function GlyphsOverlay({
  payload,
  zoom,
}: {
  payload: MapTilesPayload;
  zoom: number;
}) {
  const { rows, cols } = gridDims(payload);
  if (rows === 0 || cols === 0) {
    return <div className="map-glyph-empty">no glyph data in payload</div>;
  }

  // Find the player cell. Aabahran tags it with `h: 1`; falling
  // back to (radius+1) per the GMCP spec when no cell carries the
  // flag. Old "midpoint of observed cells" math broke on sparse
  // grids — the player @ would not paint because the computed center
  // missed the player's row.
  //
  // The h-check is permissive so a JSON quirk (`h: "1"`, `h: true`)
  // still resolves to the player. Without that, going up into an
  // indoor area sometimes lost the marker entirely.
  let centerR = 0;
  let centerC = 0;
  if (payload.g) {
    outer: for (const [rKey, row] of Object.entries(payload.g)) {
      for (const [cKey, cell] of Object.entries(row)) {
        if (!cell || typeof cell === 'string') continue;
        const h = (cell as { h?: unknown }).h;
        if (h === 1 || h === '1' || h === true) {
          centerR = Number(rKey);
          centerC = Number(cKey);
          break outer;
        }
      }
    }
  }
  if (centerR === 0 || centerC === 0) {
    if (typeof payload.r === 'number' && payload.r > 0) {
      centerR = payload.r + 1;
      centerC = payload.r + 1;
    } else {
      centerR = Math.floor((rows + 1) / 2);
      centerC = Math.floor((cols + 1) / 2);
    }
  }

  const textFallback = parseTextGrid(payload.t);
  const hasGrid = !!payload.g;

  // Output grid: rooms at even indices, connections at odd indices.
  const outRows = 2 * rows - 1;
  const outCols = 2 * cols - 1;
  const out: GlyphCell[][] = [];
  for (let r = 0; r < outRows; r++) {
    out.push(Array.from({ length: outCols }, () => EMPTY_CELL));
  }

  const placeRoom = (r: number, c: number, gc: GlyphCell) => {
    const or = 2 * (r - 1);
    const oc = 2 * (c - 1);
    if (or < 0 || or >= outRows || oc < 0 || oc >= outCols) return;
    out[or][oc] = gc;
  };

  // Pass 1: off-floor rooms. Each entry produces a sector glyph
  // tagged with its floor kind so the renderer can dim it via CSS.
  const placeOffEntries = (entries: OffFloorEntry[] | undefined, floor: FloorKind) => {
    if (!Array.isArray(entries)) return;
    for (const entry of entries) {
      const sectorCode = sectorCodeOf(entry.s);
      if (sectorCode === '') continue;
      const sector = sectorForCode(sectorCode);
      const isUnknown = sectorCode !== '0' && sector === SECTORS[0];
      const cg = connectGlyph(entry.e);
      const glyph = cg ?? (isUnknown ? UNKNOWN_GLYPH : sector.glyph);
      placeRoom(entry.y, entry.x, {
        glyph,
        color: sector.halo,
        isPlayer: false,
        floor,
      });
    }
  };
  placeOffEntries(payload.a, 'above');
  placeOffEntries(payload.b, 'below');
  if (Array.isArray(payload.zr)) {
    for (const entry of payload.zr) {
      const z = entry.z ?? 0;
      const floor: FloorKind = z > 0 ? 'above' : z < 0 ? 'below' : 'far';
      placeOffEntries([entry], floor);
    }
  }

  // Pass 2: same-floor rooms. Overrides off-floor at the same coords.
  // The player override fires BEFORE the sector check so an indoor
  // cell that arrives without an `s` field (which happens on the first
  // tick after walking up/down a Z transition) still gets the marker.
  for (let r = 1; r <= rows; r++) {
    for (let c = 1; c <= cols; c++) {
      if (r === centerR && c === centerC) {
        placeRoom(r, c, { glyph: '@', color: PLAYER_COLOR, isPlayer: true, floor: 'same' });
        continue;
      }
      const cell = hasGrid ? getCell(payload, r, c) : null;
      const sectorFromGrid = sectorCodeOf(cell?.s);
      // g and textFallback are parallel JS arrays. My loop reads
      // payload.g[String(r)] (array index r) so the text fallback
      // must read textFallback[r] too — not [r - 1]. The old
      // off-by-one made every null-cell SE of the player pick up
      // the player's "@" from text and render it as UNKNOWN_GLYPH.
      const sectorFromText = textFallback[r]?.[c] ?? '';
      const sectorCode = sectorFromGrid || sectorFromText;
      if (!sectorCode || sectorCode === ' ') continue;
      const dr = r - centerR;
      const dc = c - centerC;
      const lvl = dimLevel(dr, dc, cell?.l);
      const cg = cell ? connectGlyph(cell.e) : null;
      const sector = sectorForCode(sectorCode);
      const isUnknown = sectorCode !== '0' && sector === SECTORS[0];
      const glyph = cg ?? (isUnknown ? UNKNOWN_GLYPH : sector.glyph);
      placeRoom(r, c, {
        glyph,
        color: sectorGlyphColor(sectorCode, lvl),
        isPlayer: false,
        floor: 'same',
      });
    }
  }

  // Pass 3: connection chars between adjacent same-floor rooms.
  // Only same-floor connects to same-floor — connections to off-
  // floor would require diagonal up/down markers that the
  // single-cell grid cannot carry. The exit-string check is
  // bidirectional so a one-way exit does not light up the line
  // (otherwise the map would imply a connection that does not
  // travel both ways).
  for (let r = 1; r <= rows; r++) {
    for (let c = 1; c <= cols; c++) {
      const here = hasGrid ? getCell(payload, r, c) : null;
      if (!here) continue;
      // east-west connector
      if (c < cols) {
        const east = hasGrid ? getCell(payload, r, c + 1) : null;
        if (east && hasExit(here, 'e') && hasExit(east, 'w')) {
          const or = 2 * (r - 1);
          const oc = 2 * (c - 1) + 1;
          out[or][oc] = {
            glyph: '─',
            color: 'var(--c-border-strong, var(--c-border))',
            isPlayer: false,
            floor: 'same',
          };
        }
      }
      // north-south connector
      if (r < rows) {
        const south = hasGrid ? getCell(payload, r + 1, c) : null;
        if (south && hasExit(here, 's') && hasExit(south, 'n')) {
          const or = 2 * (r - 1) + 1;
          const oc = 2 * (c - 1);
          out[or][oc] = {
            glyph: '│',
            color: 'var(--c-border-strong, var(--c-border))',
            isPlayer: false,
            floor: 'same',
          };
        }
      }
    }
  }

  // Pass 4: unconditional player paint. Belt-and-suspenders for the
  // pass-2 override — if the player cell is outside the iteration
  // bounds (sparse grid, weird radius) the marker still lands at the
  // detected center. Without this, the @ silently disappeared on
  // some indoor floors.
  placeRoom(centerR, centerC, {
    glyph: '@',
    color: PLAYER_COLOR,
    isPlayer: true,
    floor: 'same',
  });

  // Font size scales with zoom; base 14 keeps glyph cells legible at
  // 1.0× and matches the terminal's default size.
  const fontSize = Math.round(14 * zoom);

  // Asymmetric cell sizing crunches the map toward squares-mode
  // density while keeping connection chars visible. Room cells stay
  // 1em × 1em so each sector glyph still has a clean box. Bridge
  // cells (the in-between row/column the doubled grid creates for
  // ─ │ connectors) shrink to BRIDGE_EM, so room-to-room pitch is
  // 1 + BRIDGE_EM em instead of 2em. CSS reads this constant via
  // a --vosh-glyph-bridge custom property so the same value drives
  // both cell widths and the player-center translate.
  const BRIDGE_EM = 0.35;
  const ROOM_EM = 1.0;
  const stepEm = ROOM_EM + BRIDGE_EM;
  const playerColOffset = (centerC - 1) * stepEm + ROOM_EM / 2;
  const playerRowOffset = (centerR - 1) * stepEm + ROOM_EM / 2;

  return (
    <div
      className="map-glyph-grid"
      style={{
        fontSize: `${fontSize}px`,
        transform: `translate(calc(-1em * ${playerColOffset}), calc(-1em * ${playerRowOffset}))`,
      }}
    >
      {out.map((row, r) => (
        <div
          key={r}
          className={`map-glyph-row ${r % 2 === 0 ? 'map-glyph-row-room' : 'map-glyph-row-bridge'}`}
        >
          {row.map((cell, c) => (
            <span key={c} className={cellClass(cell, r, c)} style={{ color: cell.color }}>
              {cell.glyph}
            </span>
          ))}
        </div>
      ))}
    </div>
  );
});

// Cell kind is encoded in its output position:
//   even row + even col = room (1em × 1em)
//   even row + odd col  = horizontal bridge (BRIDGE_EM × 1em)
//   odd row + even col  = vertical bridge (1em × BRIDGE_EM)
//   odd row + odd col   = corner (BRIDGE_EM × BRIDGE_EM, always blank)
function cellClass(cell: GlyphCell, r: number, c: number): string {
  const isRoomRow = r % 2 === 0;
  const isRoomCol = c % 2 === 0;
  const parts = ['map-glyph-cell'];
  if (isRoomRow && isRoomCol) parts.push('map-glyph-cell-room');
  else if (isRoomRow) parts.push('map-glyph-cell-hbridge');
  else if (isRoomCol) parts.push('map-glyph-cell-vbridge');
  else parts.push('map-glyph-cell-corner');
  if (cell.isPlayer) parts.push('map-glyph-player');
  if (cell.floor === 'above') parts.push('map-glyph-above');
  else if (cell.floor === 'below') parts.push('map-glyph-below');
  else if (cell.floor === 'far') parts.push('map-glyph-far');
  return parts.join(' ');
}

function drawTileset(
  ctx: CanvasRenderingContext2D,
  cssWidth: number,
  cssHeight: number,
  payload: MapTilesPayload,
  rows: number,
  cols: number,
  centerR: number,
  centerC: number,
  image: HTMLImageElement | null,
  anchor: Anchor,
) {
  if (!image) {
    // Fallback when no tileset is loaded — render with the standard
    // squares style and the line-based off-floor glyphs.
    drawSquares(ctx, cssWidth, cssHeight, payload, rows, cols, centerR, centerC, anchor);
    return;
  }
  const tileSize = image.naturalHeight;
  const tilesInImage = Math.max(1, Math.floor(image.naturalWidth / tileSize));
  const { pitch, playerX, playerY } = anchor;
  const ox = Math.floor(playerX - centerC * pitch);
  const oy = Math.floor(playerY - centerR * pitch);

  // Edges underneath the tiles so the connectivity still reads.
  ctx.strokeStyle = '#474b55';
  ctx.lineWidth = 1.25;
  for (let r = 1; r <= rows; r++) {
    for (let c = 1; c <= cols; c++) {
      const cell = getCell(payload, r, c);
      if (!cell) continue;
      const cx = ox + c * pitch;
      const cy = oy + r * pitch;
      if (hasExit(cell, 'n') && getCell(payload, r - 1, c)) {
        line(ctx, cx, cy, cx, cy - pitch);
      }
      if (hasExit(cell, 'e') && getCell(payload, r, c + 1)) {
        line(ctx, cx, cy, cx + pitch, cy);
      }
      if (hasExit(cell, 's') && getCell(payload, r + 1, c)) {
        line(ctx, cx, cy, cx, cy + pitch);
      }
      if (hasExit(cell, 'w') && getCell(payload, r, c - 1)) {
        line(ctx, cx, cy, cx - pitch, cy);
      }
    }
  }

  for (let r = 1; r <= rows; r++) {
    for (let c = 1; c <= cols; c++) {
      const cell = getCell(payload, r, c);
      if (!cell) continue;
      const cx = ox + c * pitch;
      const cy = oy + r * pitch;
      const cellSector = sectorCodeOf(cell.s);
      const idx = cellSector ? SECTOR_ORDER.indexOf(cellSector) : -1;
      const tileIndex = idx >= 0 && idx < tilesInImage ? idx : 0;
      ctx.drawImage(
        image,
        tileIndex * tileSize,
        0,
        tileSize,
        tileSize,
        cx - pitch / 2,
        cy - pitch / 2,
        pitch,
        pitch,
      );

      if (r === centerR && c === centerC) {
        // Player cell: dim pink overlay with a bright pink outline.
        ctx.fillStyle = MAP_COLORS.originFill;
        ctx.fillRect(cx - pitch / 2, cy - pitch / 2, pitch, pitch);
        ctx.strokeStyle = MAP_COLORS.origin;
        ctx.lineWidth = 1;
        ctx.strokeRect(cx - pitch / 2, cy - pitch / 2, pitch, pitch);
      }
    }
  }
}

function line(ctx: CanvasRenderingContext2D, x1: number, y1: number, x2: number, y2: number) {
  ctx.beginPath();
  ctx.moveTo(x1, y1);
  ctx.lineTo(x2, y2);
  ctx.stroke();
}
