import { useEffect, useRef, useState } from 'react';
import { onGmcp, onState } from '../lib/session';

/// One cell of the server-side map grid.
interface ServerCell {
  /// Exit string like `"nesw"`. Each character is a direction the cell
  /// has an exit in.
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

type Style = 'squares' | 'glyphs';

const STYLE_KEY = 'mudclient.layout.serverMapStyle';

function loadStyle(): Style {
  try {
    const value = localStorage.getItem(STYLE_KEY);
    return value === 'glyphs' ? 'glyphs' : 'squares';
  } catch {
    return 'squares';
  }
}

// Sector code → label and a base color drawn in glyph mode and as a fill
// fallback when no per-area color is present.
const SECTOR_COLORS: Record<string, string> = {
  '0': '#c9d1d9', // inside / road
  '1': '#f0c674', // city
  '2': '#3fb950', // field
  '3': '#2ea043', // forest
  '4': '#a39080', // hill
  '5': '#8b95a0', // mountain
  '6': '#58a6ff', // water swim
  '7': '#1f6feb', // water no-swim
  '8': '#7d8590', // air
  '9': '#bb8d3a', // desert
  a: '#3fb950', // pasture
  b: '#d2a8ff', // ice
  c: '#a371f7', // underwater
};

const PLAYER_COLOR = '#1f6feb';
const PLAYER_BORDER = '#c9d1d9';
const ROOM_BORDER = '#444c56';
const EDGE_COLOR = '#3a444f';
const BG = '#0d1117';
const GRID_LINE = '#1c2229';

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
  // Fall back to the t field.
  const text = parseTextGrid(payload.t);
  if (text.length > 0) {
    return { rows: text.length, cols: Math.max(...text.map((r) => r.length)) };
  }
  return { rows: 0, cols: 0 };
}

function fillFor(payload: MapTilesPayload, cell: ServerCell): string {
  const aid = cell.ar !== undefined ? String(cell.ar) : null;
  if (aid && payload.areas) {
    const area = payload.areas[aid];
    if (area?.color) {
      const color = area.color.startsWith('#') ? area.color : `#${area.color}`;
      return color;
    }
  }
  if (cell.s && SECTOR_COLORS[cell.s]) return SECTOR_COLORS[cell.s];
  return '#21262d';
}

function hasExit(cell: ServerCell, dir: 'n' | 's' | 'e' | 'w'): boolean {
  return Boolean(cell.e && cell.e.toLowerCase().includes(dir));
}

export function ServerMapView() {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [tiles, setTiles] = useState<MapTilesPayload | null>(null);
  const [style, setStyle] = useState<Style>(loadStyle);

  useEffect(() => {
    try {
      localStorage.setItem(STYLE_KEY, style);
    } catch {
      // ignore
    }
  }, [style]);

  useEffect(() => {
    let unsubGmcp: (() => void) | undefined;
    let unsubState: (() => void) | undefined;

    onGmcp((payload) => {
      if (payload.package === 'Map.Tiles') {
        setTiles((payload.data ?? {}) as MapTilesPayload);
      }
    }).then((fn) => {
      unsubGmcp = fn;
    });

    onState((payload) => {
      if (payload.kind === 'disconnected') {
        setTiles(null);
      }
    }).then((fn) => {
      unsubState = fn;
    });

    return () => {
      unsubGmcp?.();
      unsubState?.();
    };
  }, []);

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
      ctx.fillStyle = BG;
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

      if (style === 'glyphs') {
        drawGlyphs(ctx, cssWidth, cssHeight, tiles, rows, cols, centerR, centerC);
      } else {
        drawSquares(ctx, cssWidth, cssHeight, tiles, rows, cols, centerR, centerC);
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
  }, [tiles, style]);

  return (
    <div className="server-view">
      <div className="map-subhead">
        <span>
          {tiles
            ? `radius ${tiles.r ?? '?'} · server tiles`
            : 'waiting for server map'}
        </span>
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
        </div>
      </div>
      <div ref={containerRef} className="map-canvas-host">
        <canvas ref={canvasRef} />
      </div>
    </div>
  );
}

function drawSquares(
  ctx: CanvasRenderingContext2D,
  cssWidth: number,
  cssHeight: number,
  payload: MapTilesPayload,
  rows: number,
  cols: number,
  centerR: number,
  centerC: number,
) {
  // Match the mapping view's square + spacing aesthetic. The cell pitch
  // adapts to the available pane size so the whole grid fits.
  const targetPitch = 28;
  const pitch = Math.max(
    14,
    Math.min(targetPitch, Math.floor(Math.min(cssWidth / cols, cssHeight / rows))),
  );
  const size = Math.max(8, Math.floor(pitch * 0.65));
  const ox = Math.floor(cssWidth / 2 - (centerC - 0.5) * pitch);
  const oy = Math.floor(cssHeight / 2 - (centerR - 0.5) * pitch);

  // Grid lines for orientation.
  ctx.strokeStyle = GRID_LINE;
  ctx.lineWidth = 1;
  for (let c = 1; c <= cols + 1; c++) {
    const x = ox + (c - 0.5) * pitch;
    ctx.beginPath();
    ctx.moveTo(x, 0);
    ctx.lineTo(x, cssHeight);
    ctx.stroke();
  }
  for (let r = 1; r <= rows + 1; r++) {
    const y = oy + (r - 0.5) * pitch;
    ctx.beginPath();
    ctx.moveTo(0, y);
    ctx.lineTo(cssWidth, y);
    ctx.stroke();
  }

  // Edges first (under squares).
  ctx.strokeStyle = EDGE_COLOR;
  ctx.lineWidth = 1.5;
  for (let r = 1; r <= rows; r++) {
    for (let c = 1; c <= cols; c++) {
      const cell = getCell(payload, r, c);
      if (!cell) continue;
      const cx = ox + c * pitch;
      const cy = oy + r * pitch;
      if (hasExit(cell, 'n') && getCell(payload, r - 1, c)) {
        ctx.beginPath();
        ctx.moveTo(cx, cy);
        ctx.lineTo(cx, cy - pitch);
        ctx.stroke();
      }
      if (hasExit(cell, 'e') && getCell(payload, r, c + 1)) {
        ctx.beginPath();
        ctx.moveTo(cx, cy);
        ctx.lineTo(cx + pitch, cy);
        ctx.stroke();
      }
      if (hasExit(cell, 's') && getCell(payload, r + 1, c)) {
        ctx.beginPath();
        ctx.moveTo(cx, cy);
        ctx.lineTo(cx, cy + pitch);
        ctx.stroke();
      }
      if (hasExit(cell, 'w') && getCell(payload, r, c - 1)) {
        ctx.beginPath();
        ctx.moveTo(cx, cy);
        ctx.lineTo(cx - pitch, cy);
        ctx.stroke();
      }
    }
  }

  // Squares.
  for (let r = 1; r <= rows; r++) {
    for (let c = 1; c <= cols; c++) {
      const cell = getCell(payload, r, c);
      if (!cell) continue;
      const cx = ox + c * pitch;
      const cy = oy + r * pitch;
      const isCenter = r === centerR && c === centerC;
      ctx.fillStyle = isCenter ? PLAYER_COLOR : fillFor(payload, cell);
      ctx.strokeStyle = isCenter ? PLAYER_BORDER : ROOM_BORDER;
      ctx.lineWidth = isCenter ? 2 : 1;
      ctx.beginPath();
      ctx.rect(cx - size / 2, cy - size / 2, size, size);
      ctx.fill();
      ctx.stroke();

      // Up/down hints in the corner.
      const exits = (cell.e ?? '').toLowerCase();
      if (exits.includes('u') || exits.includes('d')) {
        ctx.fillStyle = '#f1c232';
        ctx.font = `${Math.max(8, Math.floor(size * 0.55))}px monospace`;
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        const marks = [];
        if (exits.includes('u')) marks.push('↑');
        if (exits.includes('d')) marks.push('↓');
        ctx.fillText(marks.join(''), cx, cy);
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
) {
  const text = parseTextGrid(payload.t);
  if (text.length === 0) {
    ctx.fillStyle = '#6e7681';
    ctx.font = '12px monospace';
    ctx.fillText('no glyph data in payload', 10, 22);
    return;
  }
  const cellW = Math.max(8, Math.min(20, Math.floor(cssWidth / (cols * 1.6))));
  const cellH = Math.max(10, Math.min(22, Math.floor(cssHeight / rows)));
  const ox = Math.max(0, Math.floor((cssWidth - cols * cellW) / 2));
  const oy = Math.max(0, Math.floor((cssHeight - rows * cellH) / 2));
  ctx.font = `${Math.floor(cellH * 0.85)}px "JetBrains Mono", "Menlo", monospace`;
  ctx.textBaseline = 'middle';
  ctx.textAlign = 'center';

  for (let r = 0; r < rows; r++) {
    const row = text[r] ?? '';
    for (let c = 0; c < cols; c++) {
      const ch = row[c] ?? ' ';
      if (ch === ' ') continue;
      const isCenter = r + 1 === centerR && c + 1 === centerC;
      ctx.fillStyle = isCenter ? '#f1c232' : (SECTOR_COLORS[ch] ?? '#7d8590');
      ctx.fillText(isCenter ? '@' : ch, ox + c * cellW + cellW / 2, oy + r * cellH + cellH / 2);
    }
  }
}
