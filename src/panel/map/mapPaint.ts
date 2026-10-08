// How the map's 2D styles paint. Squares and Tileset draw on the canvas
// MapView sizes and clears, with your room at its middle, and the Glyphs
// grid takes each cell's classes from cellClass.

import { hexToRgba } from '../../theme/color';
import { MAP_COLORS, lightAppearance, roomFill, sectorForCode } from './mapPalette';
import { readPanelMarkFace } from '../panelFace';
import {
  DOOR_COLORS,
  REACH,
  corridors,
  getCell,
  gridRooms,
  offFloorLayers,
  sectorCodeOf,
  type DoorState,
  type GlyphCell,
  type MapTilesPayload,
  type OffFloorEntry,
} from './mapTiles';
import type { MapStyle } from './mapStyle';
import type { GridSpot, WalkPlan } from './mapWalk';

// Default sector code order in a horizontal sprite strip. A tileset PNG
// supplied by the user is assumed to lay tiles out left-to-right in this
// order.
const SECTOR_ORDER: string[] = ['0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'c'];

interface Anchor {
  /// Pitch in pixels per cell.
  pitch: number;
  /// Canvas pixel where the player cell (centerR, centerC) sits.
  playerX: number;
  playerY: number;
}

/// Pick the pitch and the anchor point for the player cell. The camera
/// follows the player: the player cell always sits at the canvas center,
/// floored to integer pixels so cells render on the same pixel grid every
/// frame. The pitch is also integer; multiplying integer cell offsets by
/// integer pitch lands every neighbor on a clean grid line.
export function computeAnchor(cssWidth: number, cssHeight: number, zoom: number = 1): Anchor {
  // 20 pixels a cell at zoom 1, which your zoom scales up or down.
  // Floored to integer pixels so cells stay on a clean grid.
  const pitch = Math.max(4, Math.floor(20 * zoom));
  return {
    pitch,
    playerX: Math.floor(cssWidth / 2),
    playerY: Math.floor(cssHeight / 2),
  };
}

/** Where a flat style puts the grid on the canvas. The room at
 *  [row][col] centers on (ox + col * pitch, oy + row * pitch), and its
 *  square has side `size`. */
export interface GridPlace {
  ox: number;
  oy: number;
  pitch: number;
  size: number;
}

/** The side of a room's square in Squares at a pitch. */
export function squareSide(pitch: number): number {
  return Math.max(8, Math.floor(pitch * 0.55));
}

/** The Glyphs style's room box and the bridge between two rooms, in
 *  em of its font. */
export const GLYPH_ROOM_EM = 1;
export const GLYPH_BRIDGE_EM = 0.35;

/** The Glyphs style's font size at a zoom, 14 px at 1. */
export function glyphFontPx(zoom: number): number {
  return Math.round(14 * zoom);
}

/** Where a flat style puts the grid on a canvas of width by height,
 *  with your room at [centerR][centerC]. Squares and Tileset place it
 *  as drawSquares does, and a loaded tileset fills each pitch. Glyphs
 *  places it as GlyphsOverlay does, your room's box centered on the
 *  middle of the drawing and each room 1 em plus a bridge from the
 *  next. */
export function gridPlace(
  style: Exclude<MapStyle, '3d'>,
  width: number,
  height: number,
  zoom: number,
  centerR: number,
  centerC: number,
  tileset: boolean,
): GridPlace {
  if (style === 'glyphs') {
    const font = glyphFontPx(zoom);
    const pitch = font * (GLYPH_ROOM_EM + GLYPH_BRIDGE_EM);
    return {
      ox: width / 2 - centerC * pitch,
      oy: height / 2 - centerR * pitch,
      pitch,
      size: font * GLYPH_ROOM_EM,
    };
  }
  const { pitch, playerX, playerY } = computeAnchor(width, height, zoom);
  return {
    ox: Math.floor(playerX - centerC * pitch),
    oy: Math.floor(playerY - centerR * pitch),
    pitch,
    size: style === 'tileset' && tileset ? pitch : squareSide(pitch),
  };
}

/** The grid cell nearest a point on the canvas. */
export function roomAt(place: GridPlace, x: number, y: number): GridSpot {
  return {
    row: Math.round((y - place.oy) / place.pitch),
    col: Math.round((x - place.ox) / place.pitch),
  };
}

/** A walk the map shows, to the room at `target`, under the pointer or
 *  clicked. */
export interface WalkMark {
  plan: WalkPlan;
  target: GridSpot;
}

const spotKey = (spot: GridSpot) => `${spot.row},${spot.col}`;

/** The rooms a walk passes through, as spotKey gives them. */
function walkRooms(walk: WalkMark | null): Set<string> {
  return new Set(walk ? walk.plan.cells.map(spotKey) : []);
}

/** The path of a walk from your room at `from`, through the corridor
 *  of each step, in the path color at 2 px. It draws under the rooms,
 *  so the route reads between them. */
export function drawWalkPath(
  ctx: CanvasRenderingContext2D,
  plan: WalkPlan,
  from: GridSpot,
  place: GridPlace,
) {
  if (plan.cells.length === 0) return;
  const { ox, oy, pitch } = place;
  ctx.save();
  ctx.strokeStyle = MAP_COLORS.pathLine;
  ctx.lineWidth = 2;
  ctx.lineJoin = 'round';
  ctx.lineCap = 'round';
  ctx.beginPath();
  ctx.moveTo(ox + from.col * pitch, oy + from.row * pitch);
  for (const { row, col } of plan.cells) ctx.lineTo(ox + col * pitch, oy + row * pitch);
  ctx.stroke();
  ctx.restore();
}

/** The ring around the room a walk goes to, 1.5 px and 3.5 px outside
 *  its square. The accent marks a room the walk reaches, and a dashed
 *  danger ring one past a door or a shore. */
export function drawWalkTarget(ctx: CanvasRenderingContext2D, walk: WalkMark, place: GridPlace) {
  const { ox, oy, pitch, size } = place;
  const open = walk.plan.kind === 'open';
  const half = size / 2 + 3.5;
  ctx.save();
  ctx.strokeStyle = open ? MAP_COLORS.origin : MAP_COLORS.danger;
  ctx.lineWidth = 1.5;
  ctx.setLineDash(open ? [] : [3, 2]);
  ctx.beginPath();
  ctx.roundRect(
    ox + walk.target.col * pitch - half,
    oy + walk.target.row * pitch - half,
    half * 2,
    half * 2,
    3,
  );
  ctx.stroke();
  ctx.restore();
}

export function drawSquares(
  ctx: CanvasRenderingContext2D,
  payload: MapTilesPayload,
  rows: number,
  cols: number,
  centerR: number,
  centerC: number,
  anchor: Anchor,
  ground: string,
  /** The mark face, which the up and down marks draw in. */
  face: string,
  /** The walk the map shows, if any. */
  walk: WalkMark | null = null,
) {
  const { pitch, playerX, playerY } = anchor;
  const size = squareSide(pitch);
  // Place each grid cell relative to the player's canvas position so the
  // ROOM at world coord (x, y) keeps its on-screen position across pushes.
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

  const place = { ox, oy, pitch, size };
  if (walk) drawWalkPath(ctx, walk.plan, { row: centerR, col: centerC }, place);
  const onPath = walkRooms(walk);

  // Squares, FL web map style: dim sector fill + 0.8-alpha sector border,
  // and your room in the accent over its soft fill. A light theme fills
  // each room from its sector over the paper (roomFill), so your room
  // stays the only accent square. Each cell's alpha tracks Manhattan
  // distance from the player so the player sits in a bright pool that
  // fades outward. A room on the walk shown takes the path color at
  // full alpha.
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
      const lit = onPath.has(spotKey({ row: r, col: c }));
      ctx.globalAlpha = lit ? 1 : depth;
      ctx.fillStyle = roomFill(sector, ground, light);
      ctx.fillRect(cx - size / 2, cy - size / 2, size, size);
      ctx.strokeStyle = lit ? MAP_COLORS.pathLine : hexToRgba(sector.border, 0.8);
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

  if (walk) drawWalkTarget(ctx, walk, place);
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
export function drawOffFloorOverlay(
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

// How far a room fades by its ring around yours. The ring stands in for
// the distance along exits, since the payload carries no room graph.
export function depthAlphaForRing(d: number): number {
  if (d === 0) return 1;
  if (d <= 2) return 0.9;
  if (d <= 4) return 0.72;
  if (d <= 6) return 0.55;
  if (d <= 9) return 0.4;
  return 0.28;
}

export function drawTileset(
  ctx: CanvasRenderingContext2D,
  payload: MapTilesPayload,
  rows: number,
  cols: number,
  centerR: number,
  centerC: number,
  image: HTMLImageElement | null,
  anchor: Anchor,
  ground: string,
  /** The walk the map shows, if any. */
  walk: WalkMark | null = null,
) {
  if (!image) {
    // Fallback when no tileset is loaded — render with the standard
    // squares style and the line-based off-floor glyphs.
    drawSquares(
      ctx,
      payload,
      rows,
      cols,
      centerR,
      centerC,
      anchor,
      ground,
      readPanelMarkFace(),
      walk,
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

  const place = { ox, oy, pitch, size: pitch };
  if (walk) drawWalkPath(ctx, walk.plan, { row: centerR, col: centerC }, place);
  const onPath = walkRooms(walk);

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
    } else if (onPath.has(spotKey({ row: r, col: c }))) {
      ctx.strokeStyle = MAP_COLORS.pathLine;
      ctx.lineWidth = 1;
      ctx.strokeRect(cx - pitch / 2, cy - pitch / 2, pitch, pitch);
    }
  }

  if (walk) drawWalkTarget(ctx, walk, place);
}

function line(ctx: CanvasRenderingContext2D, x1: number, y1: number, x2: number, y2: number) {
  ctx.beginPath();
  ctx.moveTo(x1, y1);
  ctx.lineTo(x2, y2);
  ctx.stroke();
}

// Cell kind is encoded in its output position:
//   even row + even col = room (1em × 1em)
//   even row + odd col  = horizontal bridge (BRIDGE_EM × 1em)
//   odd row + even col  = vertical bridge (1em × BRIDGE_EM)
//   odd row + odd col   = corner (BRIDGE_EM × BRIDGE_EM, always blank)
export function cellClass(cell: GlyphCell, r: number, c: number): string {
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
