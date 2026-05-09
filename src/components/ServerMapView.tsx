import { useCallback, useEffect, useRef, useState, type ChangeEvent } from 'react';
import { getAreaSnapshot, onGmcp, onMap, onState, type AreaSnapshot } from '../lib/session';
import { MAP_COLORS, SECTORS, hexToRgba, sectorForCode } from '../lib/mapPalette';
import { drawTerrainDecorations } from '../lib/terrainDecor';

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
  /// Sector index (`"0".."9"`, `"a".."c"` on Aabahran).
  s?: string;
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
  s?: string;
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

const STYLE_KEY = 'mudclient.layout.serverMapStyle';
const TILESET_KEY = 'mudclient.layout.serverMapTileset';

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

export function ServerMapView() {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const fileInputRef = useRef<HTMLInputElement | null>(null);
  const [tiles, setTiles] = useState<MapTilesPayload | null>(null);
  const [style, setStyle] = useState<Style>(loadStyle);
  const [tilesetUrl, setTilesetUrl] = useState<string | null>(loadTileset);
  const [tilesetImage, setTilesetImage] = useState<HTMLImageElement | null>(null);
  const [tilesetError, setTilesetError] = useState<string | null>(null);
  // Snapshot of the persistent mapping store. We use it to translate the
  // player-centric Map.Tiles grid into stable world coordinates so cells
  // do not shift on canvas as the player walks.
  const [snapshot, setSnapshot] = useState<AreaSnapshot | null>(null);
  const lastRefreshRef = useRef<number>(0);

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
      if (canvas.width !== cssWidth * dpr || canvas.height !== cssHeight * dpr) {
        canvas.width = Math.max(1, cssWidth * dpr);
        canvas.height = Math.max(1, cssHeight * dpr);
        canvas.style.width = `${cssWidth}px`;
        canvas.style.height = `${cssHeight}px`;
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

      // Area name watermark behind the cells, mirroring the mapping
      // view so both modes share the same context cue.
      if (snapshot?.area) {
        const charW = 0.6;
        const fitSize = Math.min(
          cssHeight * 0.45,
          (cssWidth * 0.85) / (snapshot.area.length * charW),
        );
        ctx.font = `bold ${Math.max(14, Math.floor(fitSize))}px monospace`;
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillStyle = hexToRgba(halo, 0.18);
        ctx.fillText(snapshot.area.toUpperCase(), cssWidth / 2, cssHeight / 2);
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
      );

      if (style === 'glyphs') {
        drawGlyphs(ctx, cssWidth, cssHeight, tiles, rows, cols, centerR, centerC, anchor);
      } else if (style === 'tileset') {
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
      } else {
        drawSquares(ctx, cssWidth, cssHeight, tiles, rows, cols, centerR, centerC, anchor);
      }
    };

    draw();
    const observer = new ResizeObserver(draw);
    observer.observe(container);
    window.addEventListener('resize', draw);
    return () => {
      observer.disconnect();
      window.removeEventListener('resize', draw);
    };
  }, [tiles, style, tilesetImage, snapshot]);

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
): Anchor {
  // Same pitch as the mapping view so both modes render at the same scale.
  const pitch = 20;
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
  drawOffFloorCells(ctx, payload.a, ox, oy, pitch, size, centerR, centerC);
  drawOffFloorCells(ctx, payload.b, ox, oy, pitch, size, centerR, centerC);
  if (Array.isArray(payload.zr)) {
    drawOffFloorCells(ctx, payload.zr, ox, oy, pitch, size, centerR, centerC);
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
      const sector = sectorForCode(cell.s);
      const dist = Math.abs(r - centerR) + Math.abs(c - centerC);
      const depth = depthAlphaForRing(dist);

      if (isCenter) {
        ctx.fillStyle = MAP_COLORS.originGlow;
        ctx.beginPath();
        ctx.arc(cx, cy, pitch * 0.85, 0, Math.PI * 2);
        ctx.fill();
      }

      ctx.save();
      // Wipe the background under the cell first so corridor lines
      // drawn underneath don't bleed through the (less-than-fully-
      // opaque) sector fill. Without this, every distance-faded cell
      // shows a faint corridor stripe across it.
      ctx.fillStyle = MAP_COLORS.bg;
      ctx.fillRect(cx - size / 2, cy - size / 2, size, size);

      ctx.globalAlpha = depth;
      ctx.fillStyle = sector.fill;
      ctx.fillRect(cx - size / 2, cy - size / 2, size, size);

      if (isCenter) {
        ctx.strokeStyle = MAP_COLORS.origin;
        ctx.lineWidth = 2;
      } else {
        ctx.strokeStyle = hexToRgba(sector.border, 0.8);
        ctx.lineWidth = 1;
      }
      ctx.strokeRect(cx - size / 2, cy - size / 2, size, size);
      ctx.restore();

      const exits = (cell.e ?? '').toLowerCase();
      if (exits.includes('u') || exits.includes('d')) {
        ctx.fillStyle = MAP_COLORS.text;
        ctx.font = `${Math.max(7, Math.floor(size * 0.55))}px monospace`;
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        // Up/down arrows render INSIDE the cell so they don't bleed
        // into adjacent rooms or the corridor lines. ~30% offset
        // above/below center keeps them readable at small pitch.
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

// Three-tier distance fade for off-floor cells, mirroring the
// TinTin map_panel.tin convention (close <= 2, medium <= 5, far).
// Each tier knocks the alpha down a step so distant off-floor
// chains darken visibly without becoming invisible.
function offFloorDistanceFade(d: number): number {
  if (d <= 2) return 1.0;
  if (d <= 5) return 0.75;
  return 0.55;
}

// Pass A: only the dim cell fills. Drawn under everything;
// same-floor cells will paint over off-floor cells at overlapping
// positions. No border outline — the corridor lines drawn over the
// top in pass B carry the connectivity signal.
function drawOffFloorCells(
  ctx: CanvasRenderingContext2D,
  entries: OffFloorEntry[] | undefined,
  ox: number,
  oy: number,
  pitch: number,
  size: number,
  centerR: number,
  centerC: number,
) {
  if (!Array.isArray(entries) || entries.length === 0) return;
  const half = size / 2;
  for (const entry of entries) {
    const cx = ox + entry.x * pitch;
    const cy = oy + entry.y * pitch;
    const sector = sectorForCode(entry.s);
    const dist = Math.abs(entry.y - centerR) + Math.abs(entry.x - centerC);
    const fade = offFloorDistanceFade(dist);
    ctx.save();
    // Off-floor cell uses sector.border (medium-bright) at 0.4 alpha.
    // Visible enough to read against the dark map background but
    // dim enough that the corridor lines drawn in pass B remain the
    // dominant connectivity signal.
    ctx.globalAlpha = 0.4 * fade;
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

function drawGlyphs(
  ctx: CanvasRenderingContext2D,
  cssWidth: number,
  cssHeight: number,
  payload: MapTilesPayload,
  rows: number,
  cols: number,
  centerR: number,
  centerC: number,
  anchor: Anchor,
) {
  const text = parseTextGrid(payload.t);
  if (text.length === 0) {
    ctx.fillStyle = '#6e7681';
    ctx.font = '12px monospace';
    ctx.fillText('no glyph data in payload', 10, 22);
    return;
  }
  const pitchX = Math.max(8, Math.min(20, Math.floor(cssWidth / (cols * 1.6))));
  const pitchY = Math.max(10, Math.min(22, Math.floor(cssHeight / rows)));
  // Anchor on the player cell whose canvas position holds steady across
  // walks (locked to world coords when the mapping store knows the
  // player's room).
  const ox = Math.floor(anchor.playerX - (centerC - 0.5) * pitchX);
  const oy = Math.floor(anchor.playerY - (centerR - 0.5) * pitchY);
  ctx.font = `${Math.floor(pitchY * 0.85)}px "JetBrains Mono", "Menlo", monospace`;
  ctx.textBaseline = 'middle';
  ctx.textAlign = 'center';

  for (let r = 0; r < rows; r++) {
    const row = text[r] ?? '';
    for (let c = 0; c < cols; c++) {
      const ch = row[c] ?? ' ';
      if (ch === ' ') continue;
      const isCenter = r + 1 === centerR && c + 1 === centerC;
      const glyphSector = sectorForCode(ch);
      ctx.fillStyle = isCenter ? MAP_COLORS.origin : glyphSector.halo;
      ctx.fillText(isCenter ? '@' : ch, ox + (c + 0.5) * pitchX, oy + (r + 0.5) * pitchY);
    }
  }
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
      const idx = cell.s ? SECTOR_ORDER.indexOf(cell.s) : -1;
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
        ctx.strokeStyle = MAP_COLORS.origin;
        ctx.lineWidth = 2;
        ctx.beginPath();
        ctx.rect(cx - pitch / 2, cy - pitch / 2, pitch, pitch);
        ctx.stroke();
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
