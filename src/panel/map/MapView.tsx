import { useEffect, useLayoutEffect, useMemo, useRef, useState, type ChangeEvent } from 'react';
import { useTauriEvent } from '../../ipc/useTauriEvent';
import { drawMap3D } from './map3dDraw';
import { cameraFor, project, roofAt, roomAt as roomAt3d, sceneOf, type Camera } from './map3dScene';
import { DEFAULT_MAP_3D_VIEW, MAP_3D_VIEW_KEY, loadMap3dView, type Map3dView } from './map3dView';
import { MAP_COLORS, mapInks, mapThemeSignature } from './mapPalette';
import { MAP_STYLE_KEY, loadMapStyle, type MapStyle } from './mapStyle';
import { readPanelMarkFace, readPanelTextPx, subscribePanelFace } from '../panelFace';
import { ZOOM_MAX, ZOOM_MIN, ZOOM_STEP, clampZoom } from './mapZoom';
import { gridDims, playerCellOf } from './mapTiles';
import {
  computeAnchor,
  drawSquares,
  drawTileset,
  drawWalkPath,
  drawWalkTarget,
  gridPlace,
  roomAt,
  type GridPlace,
} from './mapPaint';
import {
  offerOf,
  planWalk,
  speedwalk,
  walkAhead,
  type GridSpot,
  type WalkMark,
  type WalkPlan,
} from './mapWalk';
import { WalkStatus } from './WalkStatus';
import { GlyphsOverlay } from './GlyphsOverlay';
import { subscribeThemeChanges } from '../../theme/theme';
import { pushToast } from '../../stores/toasts';
import { walkRoute } from '../../ipc/session';
import { useRoom } from '../../stores/gmcp/roomStore';
import { getSelected } from '../../stores/session/sessionsStore';
import { noteWalkRoute, useWalk } from '../../stores/session/walkStore';
import {
  getMapTiles,
  startMapTiles,
  subscribeMapTiles,
  type TilesSnap,
} from '../../stores/gmcp/mapTilesStore';
import { MapPaneControls } from './MapPaneControls';
import { textPx } from '../paneTextSize';
import { useMapGestures, type MapPoint } from './useMapGestures';

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
  const [tilesSnap, setTilesSnap] = useState<TilesSnap | null>(getMapTiles);
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
  // canvas picks up the new --panel / --accent CSS vars that
  // MAP_COLORS reads through its getters.
  const [themeVersion, setThemeVersion] = useState(0);
  // Where the pointer rests over the map, which shows the walk to the
  // room under it.
  const [pointer, setPointer] = useState<MapPoint | null>(null);
  const tipRef = useRef<HTMLDivElement | null>(null);

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
  // a double click on bare ground and the arrow keys turn and tilt it.
  const is3d = style === '3d';

  /** Where a flat style puts the grid in a drawing of width by height. */
  const placeIn = (width: number, height: number): GridPlace | null => {
    if (!tiles || style === '3d') return null;
    const { rows, cols } = gridDims(tiles);
    if (rows === 0 || cols === 0) return null;
    const { row, col } = playerCellOf(tiles, rows, cols);
    return gridPlace(style, width, height, zoom, row, col, tilesetImage !== null);
  };
  // The 3D scene, which finds the room under the pointer there.
  const scene3d = useMemo(
    () => (tiles && is3d ? sceneOf(tiles, view3d.floors) : null),
    [tiles, is3d, view3d.floors],
  );
  /** The 3D camera for a drawing of width by height. */
  const cameraIn = (width: number, height: number): Camera | null =>
    scene3d && cameraFor(width, height, scene3d, view3d, zoom);
  /** The grid cell at a point. In 3D only a room of your floor has one. */
  const spotAt = (at: MapPoint | null): GridSpot | null => {
    if (!at) return null;
    const cam = cameraIn(at.width, at.height);
    if (cam && scene3d) {
      const room = roomAt3d(cam, scene3d, at.x, at.y);
      return room && { row: room.y, col: room.x };
    }
    const place = placeIn(at.width, at.height);
    return place && roomAt(place, at.x, at.y);
  };
  /** Where the middle of a room draws, on its roof in 3D. */
  const pointOf = (spot: GridSpot, width: number, height: number) => {
    const cam = cameraIn(width, height);
    if (cam) return project(cam, spot.col, spot.row, roofAt(0));
    const place = placeIn(width, height);
    return place && { x: place.ox + spot.col * place.pitch, y: place.oy + spot.row * place.pitch };
  };
  // The walk to the room under the pointer, if one is on offer. Your
  // room, empty ground and rooms on other floors offer none. It plans
  // again only for a new room or packet, so a move inside one room
  // repaints nothing.
  const spot = spotAt(pointer);
  const hoverRow = spot?.row ?? null;
  const hoverCol = spot?.col ?? null;
  const hover = useMemo((): WalkOffer | null => {
    if (!tiles || hoverRow === null || hoverCol === null) return null;
    const plan = planWalk(tiles, hoverRow, hoverCol);
    return plan && { plan, target: { row: hoverRow, col: hoverCol } };
  }, [tiles, hoverRow, hoverCol]);
  // The walk a click sent, from where you stand to the room clicked.
  // The map shows it while you walk and after it stops, until you
  // arrive or stand off its route. The walk on offer under the pointer
  // shows over it.
  const walk = useWalk();
  const here = useRoom().info?.vnum ?? null;
  const ahead = useMemo(
    () => (tiles && walk.route ? walkAhead(tiles, walk.route, walk.progress, here) : null),
    [tiles, walk, here],
  );
  const mark = useMemo(
    (): WalkMark | null => (hover ? offerOf(hover.plan, hover.target) : ahead),
    [hover, ahead],
  );
  const status = walk.progress.kind === 'stopped' && !ahead ? null : walk.progress;

  useMapGestures(containerRef, {
    zoom,
    setZoom,
    view: is3d ? view3d : null,
    setView: setView3d,
    onPoint: setPointer,
    onRoom: (at) => spotAt(at) !== null,
    onPick: (at) => {
      // A click walks the plan for the room it lands on, from the room
      // the game last said you stand in, and the map keeps the route to
      // light it as the walk goes on. Rust drops a click planned from a
      // room you have since left, and the game has the final word on
      // every step.
      const target = spotAt(at);
      const plan = tiles && target && planWalk(tiles, target.row, target.col);
      const session = getSelected();
      const start = here;
      if (!tiles || !target || !plan || plan.steps.length === 0 || start === null) return;
      const { rows, cols } = gridDims(tiles);
      noteWalkRoute(session, {
        cells: [playerCellOf(tiles, rows, cols), ...plan.cells],
        rooms: [start, ...plan.rooms],
        target,
        kind: plan.kind,
      });
      void walkRoute(speedwalk(plan.steps), start, plan.rooms, session).catch(() => {});
    },
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
  // the draw effect. mapTilesStore drops a push whose JSON matches the
  // last one, so server re-sends with identical content cause no
  // repaint and no glyph DOM rebuild.
  useEffect(() => {
    startMapTiles();
    setTilesSnap(getMapTiles());
    return subscribeMapTiles(setTilesSnap);
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
      drawMap3D(ctx, cssWidth, cssHeight, tiles, view3d, zoom, mapInks(), mark);
      return;
    }

    const { row: centerR, col: centerC } = playerCellOf(tiles, rows, cols);

    const anchor = computeAnchor(cssWidth, cssHeight, zoom);

    if (style === 'tileset') {
      drawTileset(ctx, tiles, rows, cols, centerR, centerC, tilesetImage, anchor, ground, mark);
    } else if (style === 'squares') {
      drawSquares(
        ctx,
        tiles,
        rows,
        cols,
        centerR,
        centerC,
        anchor,
        ground,
        readPanelMarkFace(),
        mark,
      );
    } else {
      // Glyph mode: canvas paints just the background and the walk
      // it shows. The actual character grid is rendered via
      // <GlyphsOverlay /> in the JSX below so it tiles in em cells
      // using the glyph face, matching tintin's character-grid map.
      const place = placeIn(cssWidth, cssHeight);
      if (mark && place) {
        drawWalkPath(ctx, mark, { row: centerR, col: centerC }, place);
        drawWalkTarget(ctx, mark, place);
      }
    }
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
  }, [tiles, view3d, mark]);

  // The tip sits 16 px right of the room under the pointer, its roof in
  // 3D, and 50 px above it, or under it for a walk that stops at a door
  // or a shore.
  // Near the drawing's right edge it starts 200 px in from that edge,
  // and one that still runs past it flips to the left of the room.
  useLayoutEffect(() => {
    const tip = tipRef.current;
    const container = containerRef.current;
    if (!tip || !container || !hover) return;
    const width = container.clientWidth;
    const at = pointOf(hover.target, width, container.clientHeight);
    if (!at) return;
    const { x, y } = at;
    let left = Math.min(Math.max(x + 16, 8), width - 200);
    if (left + tip.offsetWidth > width - 8) left = Math.max(8, x - 16 - tip.offsetWidth);
    tip.style.left = `${left}px`;
    tip.style.top = `${hover.plan.kind === 'open' ? Math.max(y - 50, 8) : y + 18}px`;
  });

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
        {hover && hover.plan.steps.length > 0 && (
          <div ref={tipRef} className="walk-tip ov-toast" role="tooltip">
            <span className="ov-toast-msg">{walkTipText(hover)}</span>
            <span className="ov-toast-meta is-mono">{speedwalk(hover.plan.steps)}</span>
          </div>
        )}
        {status && <WalkStatus progress={status} />}
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

/** The walk on offer to the room under the pointer. */
interface WalkOffer {
  plan: WalkPlan;
  target: GridSpot;
}

/** What the tip over a room says. */
function walkTipText({ plan }: WalkOffer): string {
  if (plan.kind === 'door') return 'Walk to the door';
  if (plan.kind === 'shore') return 'Walk to the edge';
  return plan.steps.length === 1 ? 'Walk 1 step' : `Walk ${plan.steps.length} steps`;
}
