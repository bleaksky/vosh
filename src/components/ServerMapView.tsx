import { useEffect, useRef, useState } from 'react';
import { onGmcp, onState } from '../lib/session';

interface MapTilesPayload {
  /// Radius in cells from the center; grid is (2r+1) x (2r+1).
  r?: number;
  /// Tile glyph grid as a single string with rows joined by `|`.
  /// Each character is a single sector code (0-9, a-c on Aabahran).
  t?: string;
  /// Optional area name on Aabahran is not present here; Room.Info has it.
  [key: string]: unknown;
}

// Sector code → label and an xterm-256 friendly color. Mirrors the palette
// from the Aabahran TinTin++ helper so the map reads the same way. Unknown
// codes fall back to a neutral gray.
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

const PLAYER_COLOR = '#f1c232';
const BG = '#0d1117';
const GRID_LINE = '#1c2229';

function parseGrid(text: string | undefined): string[] {
  if (!text) return [];
  return text.split('|').filter((row) => row.length > 0);
}

export function ServerMapView() {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [tiles, setTiles] = useState<MapTilesPayload | null>(null);

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

      const rows = parseGrid(tiles?.t);
      if (rows.length === 0) {
        ctx.fillStyle = '#6e7681';
        ctx.font = '12px monospace';
        ctx.fillText('waiting for Map.Tiles GMCP push', 10, 22);
        return;
      }

      const cols = Math.max(...rows.map((r) => r.length));
      // Two display columns per cell: glyph + spacer.
      const cellW = Math.max(8, Math.min(20, Math.floor(cssWidth / (cols * 1.6))));
      const cellH = Math.max(10, Math.min(22, Math.floor(cssHeight / rows.length)));
      const gridW = cols * cellW;
      const gridH = rows.length * cellH;
      const ox = Math.max(0, Math.floor((cssWidth - gridW) / 2));
      const oy = Math.max(0, Math.floor((cssHeight - gridH) / 2));

      ctx.strokeStyle = GRID_LINE;
      ctx.lineWidth = 1;
      ctx.font = `${Math.floor(cellH * 0.85)}px "JetBrains Mono", "Menlo", monospace`;
      ctx.textBaseline = 'middle';
      ctx.textAlign = 'center';

      const centerR = Math.floor(rows.length / 2);
      const centerC = Math.floor(cols / 2);

      for (let r = 0; r < rows.length; r++) {
        const row = rows[r] ?? '';
        for (let c = 0; c < cols; c++) {
          const ch = row[c] ?? ' ';
          const x = ox + c * cellW;
          const y = oy + r * cellH;
          if (ch !== ' ') {
            const isCenter = r === centerR && c === centerC;
            ctx.fillStyle = isCenter ? PLAYER_COLOR : (SECTOR_COLORS[ch] ?? '#7d8590');
            ctx.fillText(isCenter ? '@' : ch, x + cellW / 2, y + cellH / 2);
          }
        }
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
  }, [tiles]);

  const radius = typeof tiles?.r === 'number' ? tiles.r : null;

  return (
    <div className="server-view">
      <div className="map-subhead">
        {tiles ? `radius ${radius ?? '?'} · server tiles` : 'waiting for server map'}
      </div>
      <div ref={containerRef} className="map-canvas-host">
        <canvas ref={canvasRef} />
      </div>
    </div>
  );
}
