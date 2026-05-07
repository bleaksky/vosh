import { useCallback, useEffect, useRef, useState, type ChangeEvent } from 'react';
import { getAreaSnapshot, onGmcp, onMap, onState, type AreaSnapshot } from '../lib/session';
import { MAP_COLORS, hexToRgba, sectorForCode } from '../lib/mapPalette';

/// One cell of the server-side map grid.
interface ServerCell {
  /// Exit string like `"nesw"`.
  e?: string;
  /// Area id this cell belongs to.
  ar?: number | string;
  /// Light level 0..4.
  l?: number | string;
  /// Flag chars (e.g. `"$bthsq"`).
  f?: string;
  /// Sector code (`"0"..."9"`, `"a"..."c"` on Aabahran).
  s?: string;
  /// Exit destinations keyed by direction.
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

  // Corridors under the squares, single thin gray pass like the FL web map.
  ctx.strokeStyle = MAP_COLORS.corridor;
  ctx.lineWidth = 1.5;
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

  // Squares, FL web map style: dim sector fill + 0.8-alpha sector border,
  // origin gets a yellow glow + bright yellow border.
  for (let r = 1; r <= rows; r++) {
    for (let c = 1; c <= cols; c++) {
      const cell = getCell(payload, r, c);
      if (!cell) continue;
      const cx = ox + c * pitch;
      const cy = oy + r * pitch;
      const isCenter = r === centerR && c === centerC;
      const sector = sectorForCode(cell.s);

      if (isCenter) {
        ctx.fillStyle = MAP_COLORS.originGlow;
        ctx.beginPath();
        ctx.arc(cx, cy, pitch * 0.85, 0, Math.PI * 2);
        ctx.fill();
      }

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

      const exits = (cell.e ?? '').toLowerCase();
      if (exits.includes('u') || exits.includes('d')) {
        ctx.fillStyle = MAP_COLORS.text;
        ctx.font = `${Math.max(7, Math.floor(size * 0.5))}px monospace`;
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        if (exits.includes('u')) {
          ctx.fillText('▲', cx, cy - size / 2 - size * 0.25);
        }
        if (exits.includes('d')) {
          ctx.fillText('▼', cx, cy + size / 2 + size * 0.25);
        }
      }
    }
  }
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
    drawSquares(ctx, cssWidth, cssHeight, payload, rows, cols, centerR, centerC, anchor);
    return;
  }
  const tileSize = image.naturalHeight;
  const tilesInImage = Math.max(1, Math.floor(image.naturalWidth / tileSize));
  const { pitch, playerX, playerY } = anchor;
  const ox = Math.floor(playerX - centerC * pitch);
  const oy = Math.floor(playerY - centerR * pitch);

  // Edges underneath the tiles so the connectivity still reads.
  ctx.strokeStyle = MAP_COLORS.corridor;
  ctx.lineWidth = 1.5;
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
