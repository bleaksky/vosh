import { memo, useEffect, useLayoutEffect, useRef, useState, type ChangeEvent } from 'react';
import { onGmcpPackage } from '../../ipc/session';
import { useTauriEvent } from '../../ipc/useTauriEvent';
import { drawMap3D } from './map3dDraw';
import { DEFAULT_MAP_3D_VIEW, MAP_3D_VIEW_KEY, loadMap3dView, type Map3dView } from './map3dView';
import {
  MAP_COLORS,
  hexToRgba,
  lightAppearance,
  mapInks,
  mapThemeSignature,
  roomFill,
  sectorForCode,
} from './mapPalette';
import { MAP_STYLE_KEY, loadMapStyle, type MapStyle } from './mapStyle';
import { readPanelMarkFace, readPanelTextPx, subscribePanelFace } from '../panelFace';
import { ZOOM_MAX, ZOOM_MIN, ZOOM_STEP, clampZoom } from './mapZoom';
import {
  DOOR_COLORS,
  REACH,
  corridors,
  getCell,
  glyphGrid,
  gridDims,
  gridRooms,
  offFloorLayers,
  playerCellOf,
  sectorCodeOf,
  type DoorState,
  type GlyphCell,
  type MapTilesPayload,
  type OffFloorEntry,
} from './mapTiles';
import { subscribeThemeChanges } from '../../theme/theme';
import { pushToast } from '../../stores/toasts';
import { MapPaneControls } from './MapPaneControls';
import { textPx } from '../paneTextSize';
import { useMapGestures } from './useMapGestures';

type Style = MapStyle;

const TILESET_KEY = 'vosh.layout.serverMapTileset';
const ZOOM_KEY = 'vosh.layout.serverMapZoom';

function loadStyle(): Style {
  try {
    return loadMapStyle(localStorage);
  } catch {
    return 'squares';
  }
}

function loadView3d(): Map3dView {
  try {
    return loadMap3dView(localStorage);
  } catch {
    return DEFAULT_MAP_3D_VIEW;
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

// Default sector code order in a horizontal sprite strip. A tileset PNG
// supplied by the user is assumed to lay tiles out left-to-right in this
// order.
const SECTOR_ORDER: string[] = ['0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'c'];

type TilesSnap = { payload: MapTilesPayload; json: string };

// The last Map.Tiles push, kept at module scope. Map.Tiles arrives
// only when you move, so a map that remounts (moved in the panel, or
// shown again after you hide the panel) draws the last map at once
// instead of a blank box until your next step. It also outlives a
// disconnect, so the panel keeps showing where you logged out.
let lastTiles: TilesSnap | null = null;
const tilesListeners = new Set<(snap: TilesSnap) => void>();
let tilesStarted = false;

function startTilesCache(): void {
  if (tilesStarted) return;
  tilesStarted = true;
  void onGmcpPackage<MapTilesPayload>('Map.Tiles', (data) => {
    const payload = data ?? ({} as MapTilesPayload);
    const json = JSON.stringify(payload);
    if (lastTiles && lastTiles.json === json) return;
    lastTiles = { payload, json };
    for (const cb of tilesListeners) cb(lastTiles);
  });
}

interface MapViewProps {
  /** What the pane says until the first Map.Tiles arrives. */
  emptyText?: string;
}

/** The drawing in the Map pane. The pane header names the area, the
 *  map's own control sits in the drawing's corner, the canvas takes
 *  the panel's color, and the empty state is `emptyText` in the page
 *  instead of canvas text. */
export function MapView({ emptyText }: MapViewProps = {}) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const fileInputRef = useRef<HTMLInputElement | null>(null);
  // Tiles state carries the payload plus its serialized form. The
  // cached JSON string lets the Map.Tiles listener drop pushes whose
  // content matches the current payload without re-stringifying
  // current state, and gives GlyphsOverlay a cheap content-equality
  // key for its memo comparison.
  const [tilesSnap, setTilesSnap] = useState<TilesSnap | null>(() => lastTiles);
  const tiles = tilesSnap?.payload ?? null;
  const [style, setStyle] = useState<Style>(loadStyle);
  const [tilesetUrl, setTilesetUrl] = useState<string | null>(loadTileset);
  const [tilesetImage, setTilesetImage] = useState<HTMLImageElement | null>(null);
  // Set while a tileset you just picked decodes. A pane has no status
  // row, so a failure there says so in a toast.
  const pickedRef = useRef(false);
  const [zoom, setZoom] = useState<number>(loadZoom);
  // How you look at the 3D style: its turn, tilt, floors and sprites.
  const [view3d, setView3d] = useState<Map3dView>(loadView3d);
  // Snapshot of the persistent mapping store. We use it to translate the
  // player-centric Map.Tiles grid into stable world coordinates so cells
  // do not shift on canvas as the player walks.
  // Bumps when the theme changes so the draw effect re-runs and the
  // canvas picks up the new --c-surface / --c-accent CSS vars that
  // MAP_COLORS reads through its getters.
  const [themeVersion, setThemeVersion] = useState(0);

  useTauriEvent(subscribeThemeChanges, () => {
    setThemeVersion((v) => v + 1);
  });

  useEffect(() => {
    try {
      localStorage.setItem(MAP_STYLE_KEY, style);
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

  useEffect(() => {
    try {
      localStorage.setItem(MAP_3D_VIEW_KEY, JSON.stringify(view3d));
    } catch {
      // ignore
    }
  }, [view3d]);

  // Plain scroll and a pinch zoom the map in every style. In 3D a drag,
  // a double click and the arrow keys turn and tilt it.
  const is3d = style === '3d';
  useMapGestures(containerRef, {
    zoom,
    setZoom,
    view: is3d ? view3d : null,
    setView: setView3d,
  });

  useEffect(() => {
    if (!tilesetUrl) {
      setTilesetImage(null);
      return;
    }
    const img = new Image();
    img.onload = () => {
      pickedRef.current = false;
      setTilesetImage(img);
    };
    img.onerror = () => {
      setTilesetImage(null);
      if (pickedRef.current) {
        pushToast({ kind: 'error', message: 'Vosh could not read that tileset image.' });
      }
      pickedRef.current = false;
    };
    img.src = tilesetUrl;
  }, [tilesetUrl]);

  // Map.Tiles is the sole tile source; updating `tilesSnap` re-runs
  // the draw effect. The cache drops a push whose JSON matches the
  // last one, so server re-sends with identical content cause no
  // repaint and no glyph DOM rebuild.
  useEffect(() => {
    startTilesCache();
    setTilesSnap(lastTiles);
    tilesListeners.add(setTilesSnap);
    return () => {
      tilesListeners.delete(setTilesSnap);
    };
  }, []);

  // draw() closes over this render's tiles/style/tileset/zoom. The
  // effects and persistent observers below call it through drawRef so
  // callbacks that outlive a render (rAF, timers, ResizeObserver,
  // window resize) always hit the latest closure, never a stale one.
  const draw = () => {
    const canvas = canvasRef.current;
    const container = containerRef.current;
    if (!canvas || !container) return;
    const dpr = window.devicePixelRatio || 1;
    const cssWidth = container.clientWidth;
    const cssHeight = container.clientHeight;
    // Only resize the backing buffer. CSS keeps the display size
    // pinned to the container via width:100%/height:100% so the
    // canvas tracks layout reflows without needing the inline style
    // to be refreshed in lockstep.
    const targetW = Math.max(1, cssWidth * dpr);
    const targetH = Math.max(1, cssHeight * dpr);
    if (canvas.width !== targetW || canvas.height !== targetH) {
      canvas.width = targetW;
      canvas.height = targetH;
    }
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    // A pane's drawing sits on the panel's own color with no box
    // around it, as the approved Map pane shows: rooms, corridors, and
    // doors, nothing behind them.
    const ground = MAP_COLORS.panel;
    ctx.fillStyle = ground;
    ctx.fillRect(0, 0, cssWidth, cssHeight);

    // A pane shows its empty state as page text instead.
    if (!tiles) return;

    const { rows, cols } = gridDims(tiles);
    if (rows === 0 || cols === 0) {
      // The notice draws at the panel size, 12 px on a 12 px panel.
      const px = readPanelTextPx();
      ctx.fillStyle = '#6e7681';
      ctx.font = `${px}px ${readPanelMarkFace()}`;
      ctx.fillText('Map.Tiles payload has no grid yet', 10, textPx(22, px));
      return;
    }

    if (style === '3d') {
      drawMap3D(ctx, cssWidth, cssHeight, tiles, view3d, zoom, mapInks());
      return;
    }

    const { row: centerR, col: centerC } = playerCellOf(tiles, rows, cols);

    const anchor = computeAnchor(tiles, rows, cols, centerR, centerC, cssWidth, cssHeight, zoom);

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
        ground,
      );
    } else if (style === 'squares') {
      drawSquares(
        ctx,
        cssWidth,
        cssHeight,
        tiles,
        rows,
        cols,
        centerR,
        centerC,
        anchor,
        ground,
        readPanelMarkFace(),
      );
    }
    // Glyph mode: canvas paints just the background + terrain halo.
    // The actual character grid is rendered via <GlyphsOverlay /> in
    // the JSX below so it tiles in real terminal-cell pitch (1ch ×
    // 1em) using the app font, matching tintin's character-grid map.
  };
  const drawRef = useRef(draw);
  drawRef.current = draw;

  // Tiles-driven redraw: one repaint per push, in the frame that
  // commits it. Movement sends one Map.Tiles per step. A passive effect
  // runs after the browser paints, so waiting there and then on a
  // frame showed each step a frame or two after the terminal and the
  // room row had moved on. A layout effect draws before that paint.
  // A change to the 3D view repaints the same way, once per drag step.
  useLayoutEffect(() => {
    drawRef.current();
  }, [tiles, view3d]);

  // Style/layout-driven redraw keeps the settle sequence. Layout after
  // a mode toggle can take a frame or two to settle. Schedule a couple
  // of follow-up draws across short deadlines so at least one lands on
  // the post-reflow size.
  useEffect(() => {
    drawRef.current();
    const raf = requestAnimationFrame(() => drawRef.current());
    const t1 = window.setTimeout(() => drawRef.current(), 80);
    const t2 = window.setTimeout(() => drawRef.current(), 240);
    return () => {
      cancelAnimationFrame(raf);
      window.clearTimeout(t1);
      window.clearTimeout(t2);
    };
  }, [style, tilesetImage, themeVersion, zoom]);

  // Persistent size hooks, mounted once. Rebuilding the ResizeObserver
  // on every tiles push made its initial .observe() callback an extra
  // repaint per movement; a single long-lived observer (plus the window
  // resize listener) covers pane and window resizes instead, always
  // through the latest draw closure via drawRef.
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const redraw = () => drawRef.current();
    const observer = new ResizeObserver(redraw);
    observer.observe(container);
    window.addEventListener('resize', redraw);
    return () => {
      observer.disconnect();
      window.removeEventListener('resize', redraw);
    };
  }, []);

  // The labels and marks draw in the panel faces at the panel size, so
  // a new face or size, or a face that finishes loading after a paint,
  // paints the map again.
  useEffect(() => subscribePanelFace(() => drawRef.current()), []);

  // Every theme write lands on the root element's inline style, from
  // this window or a broadcast, and so does a change the theme event
  // never announces (the system contrast setting). Repaint when a
  // color the map paints with actually changed, at most once a frame.
  useEffect(() => {
    const root = document.documentElement;
    let last = mapThemeSignature();
    let frame = 0;
    const observer = new MutationObserver(() => {
      if (frame) return;
      frame = requestAnimationFrame(() => {
        frame = 0;
        const next = mapThemeSignature();
        if (next === last) return;
        last = next;
        drawRef.current();
      });
    });
    observer.observe(root, {
      attributes: true,
      attributeFilter: ['style', 'data-theme', 'data-appearance'],
    });
    return () => {
      observer.disconnect();
      cancelAnimationFrame(frame);
    };
  }, []);

  const handleLoadTileset = (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    // Clear the picker so choosing the same file again still loads it.
    event.target.value = '';
    if (!file) return;
    pickedRef.current = true;
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
    reader.onerror = () => {
      pickedRef.current = false;
      pushToast({ kind: 'error', message: 'Vosh could not read that tileset file.' });
    };
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
      <input
        ref={fileInputRef}
        type="file"
        accept="image/png,image/jpeg,image/webp"
        onChange={handleLoadTileset}
        hidden
      />
      <div
        ref={containerRef}
        className={`map-canvas-host${is3d ? ' is-3d' : ''}`}
        tabIndex={is3d ? 0 : undefined}
        role={is3d ? 'group' : undefined}
        aria-label={is3d ? 'Map. Drag or use the arrow keys to turn and tilt it.' : undefined}
      >
        <canvas ref={canvasRef} />
        {style === 'glyphs' && tilesSnap && (
          <GlyphsOverlay payload={tilesSnap.payload} payloadJson={tilesSnap.json} zoom={zoom} />
        )}
        {!tiles && emptyText && <p className="pane-map-empty">{emptyText}</p>}
        <MapPaneControls
          style={style}
          onStyle={setStyle}
          zoom={zoom}
          canZoomIn={zoom < ZOOM_MAX - 1e-6}
          canZoomOut={zoom > ZOOM_MIN + 1e-6}
          onZoomIn={() => setZoom((z) => clampZoom(z + ZOOM_STEP))}
          onZoomOut={() => setZoom((z) => clampZoom(z - ZOOM_STEP))}
          onZoomReset={() => setZoom(1)}
          tilesetLoaded={tilesetUrl !== null}
          onLoadTileset={() => fileInputRef.current?.click()}
          onClearTileset={clearTileset}
          view3d={view3d}
          onView3d={setView3d}
        />
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
  ground: string,
  /** The mark face, which the up and down marks draw in. */
  face: string,
) {
  const { pitch, playerX, playerY } = anchor;
  const size = Math.max(8, Math.floor(pitch * 0.55));
  // Place each grid cell relative to the player's canvas position so the
  // ROOM at world coord (x, y) keeps its on-screen position across pushes.
  // Cells outside the populated bounding box are still drawn but ignored
  // by hit-testing in this Phase 7 cut.
  const ox = Math.floor(playerX - centerC * pitch);
  const oy = Math.floor(playerY - centerR * pitch);

  // Corridors under the squares, bucketed by door state so we render
  // one stroke per color. corridors() says which ones, hidden door
  // stubs and the ticks of bent exits included. A tick clears its square
  // by a few pixels at any zoom. Toward a room it stops a pixel short of
  // the middle of the gap, so it never reads as a join, which leaves it
  // no room at the smallest zoom.
  type Segment = { cx: number; cy: number; nx: number; ny: number };
  const buckets: Record<DoorState, Segment[]> = {
    open: [],
    closed: [],
    locked: [],
    hidden: [],
  };
  for (const { row, col, dx, dy, kind, state } of corridors(payload, rows, cols)) {
    const cx = ox + col * pitch;
    const cy = oy + row * pitch;
    let reach = REACH[kind] * pitch;
    if (kind === 'tick') {
      reach = Math.max(reach, size / 2 + 3);
      if (getCell(payload, row + dy, col + dx)) reach = Math.min(reach, pitch / 2 - 1);
    }
    buckets[state].push({ cx, cy, nx: cx + dx * reach, ny: cy + dy * reach });
  }
  ctx.lineWidth = 1.25;
  const flushSolid = (state: 'open' | 'closed' | 'locked') => {
    const segs = buckets[state];
    if (segs.length === 0) return;
    ctx.strokeStyle = DOOR_COLORS[state];
    ctx.beginPath();
    for (const s of segs) {
      ctx.moveTo(s.cx, s.cy);
      ctx.lineTo(s.nx, s.ny);
    }
    ctx.stroke();
  };
  flushSolid('open');
  flushSolid('closed');
  flushSolid('locked');
  if (buckets.hidden.length > 0) {
    ctx.save();
    ctx.strokeStyle = DOOR_COLORS.hidden;
    ctx.setLineDash([3, 3]);
    ctx.beginPath();
    for (const s of buckets.hidden) {
      ctx.moveTo(s.cx, s.cy);
      ctx.lineTo(s.nx, s.ny);
    }
    ctx.stroke();
    ctx.restore();
  }

  // Off-floor: cells THEN lines, both drawn BEFORE same-floor cells.
  // Same-floor cells render last and wipe their cell area, so any
  // off-floor line crossing under a same-floor cell gets hidden —
  // the line "stops at" the same-floor cell visually. Off-floor
  // lines stay visible inside off-floor cells (translucent) and in
  // empty grid positions where no same-floor cell sits.
  const layers = offFloorLayers(payload);
  for (const layer of layers) drawOffFloorCells(ctx, layer, ox, oy, pitch, size);
  for (const layer of layers) drawOffFloorOverlay(ctx, layer, ox, oy, pitch);

  // Squares, FL web map style: dim sector fill + 0.8-alpha sector border,
  // and your room in the accent over its soft fill. A light theme fills
  // each room from its sector over the paper (roomFill), so your room
  // stays the only accent square. Each cell's alpha tracks Manhattan
  // distance from the player so the player sits in a bright pool that
  // fades outward.
  const light = lightAppearance();
  for (const { row: r, col: c, cell } of gridRooms(payload, rows, cols)) {
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
    ctx.fillStyle = ground;
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
      ctx.fillStyle = roomFill(sector, ground, light);
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
        ctx.font = `${Math.max(7, Math.floor(size * 0.55))}px ${face}`;
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

// HTML glyph overlay.
//
// glyphGrid() in panel/map/mapTiles lays out the rooms and the connectors
// between them. The player cell anchors to the parent's geometric
// center via a CSS calc() translate, so the map scrolls around the
// player as they walk. Each cell measures 1ch × 1em (line-height: 1)
// so the math is a clean character count.
//
// Wrapped in React.memo with a content-based equality. Props compare
// via the payload's cached JSON string (plus zoom), not the payload
// object reference, so a fresh-but-identical payload object cannot
// defeat the memo. A Char.Vitals / Room.Info / Room.Chars burst
// during a movement (which re-renders MapView when its snapshot
// or themeVersion shifts) does not re-run the 60×60 grid build +
// ~3500-span DOM diff. Only real payload content changes or the zoom
// value trigger a rebuild.
const GlyphsOverlay = memo(
  function GlyphsOverlay({
    payload,
    zoom,
  }: {
    payload: MapTilesPayload;
    /// Serialized form of `payload`, cached by the parent. The memo
    /// equality below compares this string instead of the payload
    /// reference.
    payloadJson: string;
    zoom: number;
  }) {
    const grid = glyphGrid(payload);
    if (!grid) {
      return <div className="map-glyph-empty">no glyph data in payload</div>;
    }
    const { cells: out, centerR, centerC } = grid;

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
    const playerColOffset = centerC * stepEm + ROOM_EM / 2;
    const playerRowOffset = centerR * stepEm + ROOM_EM / 2;

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
  },
  (prev, next) => prev.payloadJson === next.payloadJson && prev.zoom === next.zoom,
);

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
  ground: string,
) {
  if (!image) {
    // Fallback when no tileset is loaded — render with the standard
    // squares style and the line-based off-floor glyphs.
    drawSquares(
      ctx,
      cssWidth,
      cssHeight,
      payload,
      rows,
      cols,
      centerR,
      centerC,
      anchor,
      ground,
      readPanelMarkFace(),
    );
    return;
  }
  const tileSize = image.naturalHeight;
  const tilesInImage = Math.max(1, Math.floor(image.naturalWidth / tileSize));
  const { pitch, playerX, playerY } = anchor;
  const ox = Math.floor(playerX - centerC * pitch);
  const oy = Math.floor(playerY - centerR * pitch);

  // Edges underneath the tiles so the connectivity still reads.
  // Buckets per door state so the four colors share one render
  // pass each (see drawSquares for the same logic).
  ctx.lineWidth = 1.25;
  type EdgeSeg = { x1: number; y1: number; x2: number; y2: number };
  const buckets: Record<DoorState, EdgeSeg[]> = {
    open: [],
    closed: [],
    locked: [],
    hidden: [],
  };
  for (const { row, col, dx, dy, kind, state } of corridors(payload, rows, cols)) {
    const cx = ox + col * pitch;
    const cy = oy + row * pitch;
    const reach = REACH[kind] * pitch;
    buckets[state].push({ x1: cx, y1: cy, x2: cx + dx * reach, y2: cy + dy * reach });
  }
  const flushSolid = (state: 'open' | 'closed' | 'locked') => {
    const segs = buckets[state];
    if (segs.length === 0) return;
    ctx.strokeStyle = DOOR_COLORS[state];
    for (const s of segs) line(ctx, s.x1, s.y1, s.x2, s.y2);
  };
  flushSolid('open');
  flushSolid('closed');
  flushSolid('locked');
  if (buckets.hidden.length > 0) {
    ctx.save();
    ctx.strokeStyle = DOOR_COLORS.hidden;
    ctx.setLineDash([3, 3]);
    for (const s of buckets.hidden) line(ctx, s.x1, s.y1, s.x2, s.y2);
    ctx.restore();
  }

  for (const { row: r, col: c, cell } of gridRooms(payload, rows, cols)) {
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

function line(ctx: CanvasRenderingContext2D, x1: number, y1: number, x2: number, y2: number) {
  ctx.beginPath();
  ctx.moveTo(x1, y1);
  ctx.lineTo(x2, y2);
  ctx.stroke();
}
