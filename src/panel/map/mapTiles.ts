import { SECTORS, UNKNOWN_GLYPH, sectorForCode, sectorGlyphColor } from './mapPalette';

// Reading a Map.Tiles packet. Aabahran builds the grid in generate_map
// (minimap.c) and writes it in gmcp_send_map (gmcp.c). The painters in
// mapPaint.ts and the glyph overlay in GlyphsOverlay.tsx draw what these
// functions read, so the rooms, the corridors and the glyph grid can be
// checked against real packets without a canvas.

/// One cell of the server-side map grid (player's floor only).
/// Per the Aabahran GMCP wiki:
/// `g[y][x]` carries `{s, e, l, h, ar, f, d, ex}`. Multi-floor rooms
/// live in a separate top-level `zr` array (see `MultiZEntry`), not
/// in the `g` grid.
export interface ServerCell {
  /// Exit string like `"nesw"`. An uppercase letter is an exit that
  /// does not land on the room in the next cell (exit_mismatch in
  /// minimap.c), whether that cell is empty, off the grid, or holds
  /// another room.
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
export interface OffFloorEntry {
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

export interface AreaInfo {
  name?: string;
  color?: string;
}

export interface MapTilesPayload {
  r?: number;
  t?: string;
  g?: Record<string, Record<string, ServerCell | null | string>>;
  /// Rooms one floor above the player's current floor, in the same
  /// `[y][x]` coordinate space as `g`. Empty/absent when no above-
  /// floor rooms are within radius. The game keeps this list for older
  /// clients. The map draws the rooms in it that `zr` does not already
  /// hold (offFloorLayers).
  a?: OffFloorEntry[];
  /// Rooms one floor below, kept the same way as `a`.
  b?: OffFloorEntry[];
  /// The multi-Z array carries rooms at any floor delta
  /// (positive=above, negative=below) via an explicit `z` field on
  /// each entry, out to your radius. It repeats most rooms in `a` and
  /// `b` at the same spot. The game leaves it out when its search finds
  /// no room.
  zr?: OffFloorEntry[];
  areas?: Record<string, AreaInfo>;
}

export function parseTextGrid(text: string | undefined): string[] {
  if (!text) return [];
  return text.split('|').filter((row) => row.length > 0);
}

export function getCell(payload: MapTilesPayload, row: number, col: number): ServerCell | null {
  if (!payload.g) return null;
  const r = payload.g[String(row)];
  if (!r) return null;
  const v = r[String(col)];
  if (!v || typeof v === 'string') return null;
  return v;
}

/// How many rows and columns the grid has. The game sends `g` as rows
/// indexed from 0, 2r + 1 of them with 2r + 1 cells each, and you stand
/// at [r][r]. Rows and columns run from 0 to one less than these counts.
export function gridDims(payload: MapTilesPayload): { rows: number; cols: number } {
  if (payload.g) {
    let rows = 0;
    let cols = 0;
    for (const [rKey, row] of Object.entries(payload.g)) {
      const r = Number(rKey);
      if (Number.isInteger(r) && r >= rows) rows = r + 1;
      for (const cKey of Object.keys(row)) {
        const c = Number(cKey);
        if (Number.isInteger(c) && c >= cols) cols = c + 1;
      }
    }
    if (rows > 0 && cols > 0) return { rows, cols };
  }
  const text = parseTextGrid(payload.t);
  if (text.length > 0) {
    return { rows: text.length, cols: Math.max(...text.map((r) => r.length)) };
  }
  return { rows: 0, cols: 0 };
}

export type Dir = 'n' | 's' | 'e' | 'w';

/// Whether the exit that way lands on the room in the next cell. The
/// game sends that letter in lowercase. An uppercase letter is an exit
/// that leads somewhere else, so it never joins this room to the room
/// beside it.
export function hasExit(cell: ServerCell, dir: Dir): boolean {
  return Boolean(cell.e?.includes(dir));
}

/// Possible door states reported by the server in `cell.d[dir]`.
/// Aabahran's MAP_DOOR constants (minimap.h): open / closed / locked /
/// hidden. The "hidden" value only ever reaches non-immortals if the
/// server gate at `map_compute_doors` is flipped; the client still
/// needs to recognize and render it for immortals (and for any future
/// game that exposes secret doors to mortals).
export type DoorState = 'open' | 'closed' | 'locked' | 'hidden';

export function doorStateAt(cell: ServerCell, dir: Dir): DoorState | null {
  const raw = cell.d?.[dir];
  if (typeof raw !== 'string') return null;
  if (raw === 'open' || raw === 'closed' || raw === 'locked' || raw === 'hidden') return raw;
  return null;
}

/// Resolve a connector's effective door state by checking both ends
/// of the link. "Worst" state wins — hidden > locked > closed > open
/// — because a connector is only as easy to traverse as its more-
/// restrictive door. Hidden is treated as the most-restrictive because
/// it implies you need to find it first. `null` means no door state
/// reported on either side.
export function combineDoorStates(a: DoorState | null, b: DoorState | null): DoorState | null {
  const rank: Record<DoorState, number> = { open: 0, closed: 1, locked: 2, hidden: 3 };
  if (a === null && b === null) return null;
  if (a === null) return b;
  if (b === null) return a;
  return rank[a] >= rank[b] ? a : b;
}

/// Pixel color for a door state in the canvas-based renderers
/// (squares / tileset). Open uses the existing corridor gray so
/// regular maps look unchanged. Closed/locked share solid lines in
/// their own colors; hidden pairs its pink with the dashed line
/// pattern set in the renderer.
export const DOOR_COLORS: Record<DoorState, string> = {
  open: '#474b55',
  closed: '#d4a14a',
  locked: '#c64545',
  hidden: '#e07fb8',
};

/// Glyph-mode connector char per state. Hidden uses the dashed
/// box-drawing pair (╌ / ╎); the others use solid (─ / │).
const DOOR_GLYPHS: Record<DoorState, { horizontal: string; vertical: string }> = {
  open: { horizontal: '─', vertical: '│' },
  closed: { horizontal: '─', vertical: '│' },
  locked: { horizontal: '─', vertical: '│' },
  hidden: { horizontal: '╌', vertical: '╎' },
};

// Aabahran's GMCP payload serializes digit sector codes (0..9) as
// JSON numbers and letter codes (a..c) as strings. Without this
// coercion the `0` cells (Inside rooms — the most common terrain
// indoors) would short-circuit the `!sectorCode` falsy check below
// and skip rendering entirely, leaving every indoor room invisible.
export function sectorCodeOf(s: unknown): string {
  if (s === null || s === undefined) return '';
  return String(s);
}

/// Find the player cell. Aabahran tags it with `h: 1`; falling back
/// to [r][r], where the game always puts you, when no cell carries the
/// flag. Old "midpoint of observed cells" math broke on sparse grids — the
/// player @ would not paint because the computed center missed the
/// player's row.
///
/// The h-check is permissive so a JSON quirk (`h: "1"`, `h: true`)
/// still resolves to the player. Without that, going up into an
/// indoor area sometimes lost the marker entirely.
export function playerCellOf(
  payload: MapTilesPayload,
  rows: number,
  cols: number,
): { row: number; col: number } {
  if (payload.g) {
    for (const [rKey, row] of Object.entries(payload.g)) {
      for (const [cKey, cell] of Object.entries(row)) {
        if (!cell || typeof cell === 'string') continue;
        const h = (cell as { h?: unknown }).h;
        if (h === 1 || h === '1' || h === true) {
          const r = Number(rKey);
          const c = Number(cKey);
          if (r >= 0 && c >= 0) return { row: r, col: c };
        }
      }
    }
  }
  if (typeof payload.r === 'number' && payload.r > 0) {
    return { row: payload.r, col: payload.r };
  }
  return { row: Math.floor(rows / 2), col: Math.floor(cols / 2) };
}

/** A room on your floor, at its row and column in the grid. */
export interface GridRoom {
  row: number;
  col: number;
  cell: ServerCell;
}

/** Every room on your floor, row by row, the order the squares and
 *  tileset painters draw them in. */
export function gridRooms(payload: MapTilesPayload, rows: number, cols: number): GridRoom[] {
  const out: GridRoom[] = [];
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      const cell = getCell(payload, r, c);
      if (cell) out.push({ row: r, col: c, cell });
    }
  }
  return out;
}

/** What a stroke out of a room says. A `join` runs to the room in the
 *  next cell. A `stub` is the half pitch a hidden door draws when its
 *  exit leads elsewhere. A `tick` is the short mark of a bent exit, one
 *  the game sends in uppercase because it leads past the next cell. */
export type StrokeKind = 'join' | 'stub' | 'tick';

/** How far a stroke of each kind reaches from its room's center in
 *  Squares and Tileset, in cells. A join meets the room in the next cell
 *  and a stub stops in the middle of the gap. A tick clears a room in
 *  Squares and stops short of the middle of the gap, so two ticks that
 *  face each other never read as a join. */
export const REACH: Record<StrokeKind, number> = { join: 1, stub: 0.5, tick: 0.42 };

/** One stroke from a room's center toward the next cell, in grid
 *  units. */
export interface Stroke {
  dx: number;
  dy: number;
  state: DoorState;
  kind: StrokeKind;
  /** A join the room in the next cell leads back along, so the two
   *  rooms share one line. */
  mutual: boolean;
}

/** A stroke out of the room at a row and column of your floor. */
export interface Corridor extends Stroke {
  row: number;
  col: number;
}

const DIR_OFFSETS: Array<[Dir, Dir, number, number]> = [
  ['n', 's', 0, -1],
  ['e', 'w', 1, 0],
  ['s', 'n', 0, 1],
  ['w', 'e', -1, 0],
];

/** The strokes one room draws toward the cells around it, north, east,
 *  south, then west. `next` finds the room dx, dy away on its floor.
 *
 *  A join reaches the next cell only along a lowercase exit, the one
 *  that lands on the room there. Its door color comes from this room's
 *  door that way and from the neighbor's door back, but only when the
 *  neighbor's letter back is lowercase too. An uppercase letter there is
 *  an exit to somewhere else, and its door belongs to that exit.
 *
 *  An uppercase letter draws a tick in its own door's color, so you see
 *  an exit leave that way without a line that joins the room beside it.
 *
 *  An immortal sees secret exits. The game sends them in `e` like any
 *  other exit, uppercase when they lead elsewhere, and marks their door
 *  `d[dir] = "hidden"`. A hidden door whose exit does not land on the
 *  next room draws a half pitch stub instead, so the line says "secret
 *  exit" without joining this room to the room beside it. */
export function exitStrokes(
  cell: ServerCell,
  next: (dx: number, dy: number) => ServerCell | null,
): Stroke[] {
  const out: Stroke[] = [];
  for (const [dir, opp, dx, dy] of DIR_OFFSETS) {
    const neighbor = next(dx, dy);
    const here = doorStateAt(cell, dir);
    if (neighbor && hasExit(cell, dir)) {
      const mutual = hasExit(neighbor, opp);
      const there = mutual ? doorStateAt(neighbor, opp) : null;
      const state = combineDoorStates(here, there) ?? 'open';
      out.push({ dx, dy, state, kind: 'join', mutual });
    } else if (here === 'hidden') {
      out.push({ dx, dy, state: 'hidden', kind: 'stub', mutual: false });
    } else if (cell.e?.includes(dir.toUpperCase())) {
      out.push({ dx, dy, state: here ?? 'open', kind: 'tick', mutual: false });
    }
  }
  return out;
}

/** The strokes the squares and tileset styles draw under the rooms of
 *  your floor, each room's in exitStrokes order. */
export function corridors(payload: MapTilesPayload, rows: number, cols: number): Corridor[] {
  const out: Corridor[] = [];
  for (const { row, col, cell } of gridRooms(payload, rows, cols)) {
    const next = (dx: number, dy: number) => getCell(payload, row + dy, col + dx);
    for (const stroke of exitStrokes(cell, next)) out.push({ row, col, ...stroke });
  }
  return out;
}

/** A room on another floor, `z` floors above you (below when negative). */
export interface OffFloorRoom extends OffFloorEntry {
  z: number;
}

/** The rooms on other floors, one list per layer the painters draw.
 *
 *  The game sends most of them twice. `zr` is its search over every
 *  floor out to your radius, and `a` and `b` are its older lists of the
 *  floor just above and just below. Most rooms in `a` and `b` come again
 *  in `zr` at the same spot, and drawing all three painted those rooms
 *  twice. Some do not. The `zr` search counts the climb against your
 *  radius and `a` and `b` do not, so the room over a staircase r steps
 *  away, and the rooms beside it, come only in `a` or `b`. The game also
 *  leaves `zr` out when its search finds no room. So every room of `zr`
 *  draws, and a room of `a` or `b` draws only when no room of `zr`, and
 *  no room listed before it, holds its spot, the same x, y and floor.
 *  `zr` comes last, so its room wins a glyph cell that `a` or `b`
 *  reaches on another floor. */
export function offFloorLayers(payload: MapTilesPayload): OffFloorRoom[][] {
  const spotOf = (e: OffFloorRoom) => `${e.x},${e.y},${e.z}`;
  const searched = Array.isArray(payload.zr) ? payload.zr.map((e) => ({ ...e, z: e.z ?? 0 })) : [];
  const taken = new Set(searched.map(spotOf));
  const older = (entries: OffFloorEntry[] | undefined, z: number): OffFloorRoom[] => {
    const out: OffFloorRoom[] = [];
    if (!Array.isArray(entries)) return out;
    for (const e of entries) {
      const room = { ...e, z };
      if (taken.has(spotOf(room))) continue;
      taken.add(spotOf(room));
      out.push(room);
    }
    return out;
  };
  return [older(payload.a, 1), older(payload.b, -1), searched];
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

// The glyph grid.
//
// Render model: each room takes one glyph cell, and adjacent rooms
// are separated by a gap cell that either holds a box-drawing
// connection (─, │, ┼) if both rooms share that exit or stays blank.
// The output grid is therefore (2*rows - 1) × (2*cols - 1) so the
// connection cells have somewhere to live.
//
// Layers, painted in order:
//   1. Off-floor rooms (offFloorLayers) — dim sector glyph as a
//      background hint so the player can see structure above and
//      below the current floor. Rendered at low alpha via CSS.
//   2. Same-floor rooms — full color sector glyph; overrides any
//      off-floor cell at the same coords.
//   3. Connection cells — drawn between two adjacent same-floor
//      rooms when both rooms list the corresponding exit.
//   4. Player marker (`@`) — bold yellow, always on top.
export type FloorKind = 'same' | 'above' | 'below' | 'far';
export interface GlyphCell {
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

/** The glyph grid, rooms at even rows and columns and connectors
 *  between them, with the player's room row and column in the packet's
 *  grid. The room at [r][c] of the packet sits at cells[2r][2c]. */
export interface GlyphGrid {
  cells: GlyphCell[][];
  centerR: number;
  centerC: number;
}

/** Lay out the glyph style's grid, or null when the packet has none. */
export function glyphGrid(payload: MapTilesPayload): GlyphGrid | null {
  const { rows, cols } = gridDims(payload);
  if (rows === 0 || cols === 0) return null;

  const { row: centerR, col: centerC } = playerCellOf(payload, rows, cols);

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
    const or = 2 * r;
    const oc = 2 * c;
    if (or < 0 || or >= outRows || oc < 0 || oc >= outCols) return;
    out[or][oc] = gc;
  };

  // Pass 1: off-floor rooms. Each entry produces a sector glyph
  // tagged with its floor kind so the renderer can dim it via CSS.
  for (const entry of offFloorLayers(payload).flat()) {
    const sectorCode = sectorCodeOf(entry.s);
    if (sectorCode === '') continue;
    const sector = sectorForCode(sectorCode);
    const isUnknown = sectorCode !== '0' && sector === SECTORS[0];
    const cg = connectGlyph(entry.e);
    const glyph = cg ?? (isUnknown ? UNKNOWN_GLYPH : sector.glyph);
    const floor: FloorKind = entry.z > 0 ? 'above' : entry.z < 0 ? 'below' : 'far';
    placeRoom(entry.y, entry.x, {
      glyph,
      color: sector.halo,
      isPlayer: false,
      floor,
    });
  }

  // Pass 2: same-floor rooms. Overrides off-floor at the same coords.
  // The player override fires BEFORE the sector check so an indoor
  // cell that arrives without an `s` field (which happens on the first
  // tick after walking up/down a Z transition) still gets the marker.
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
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
  // travel both ways). Door state (cell.d) picks the connector
  // color: open → gray, closed → amber, locked → red, hidden →
  // pink + dashed glyph (see DOOR_COLORS / DOOR_GLYPHS).
  const OPEN_COLOR = 'var(--c-border-strong, var(--c-border))';
  const connectorColor = (state: DoorState | null): string =>
    state && state !== 'open' ? DOOR_COLORS[state] : OPEN_COLOR;
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      const here = hasGrid ? getCell(payload, r, c) : null;
      if (!here) continue;
      // east-west connector
      if (c < cols - 1) {
        const east = hasGrid ? getCell(payload, r, c + 1) : null;
        if (east && hasExit(here, 'e') && hasExit(east, 'w')) {
          const or = 2 * r;
          const oc = 2 * c + 1;
          const state = combineDoorStates(doorStateAt(here, 'e'), doorStateAt(east, 'w'));
          const effective = state ?? 'open';
          out[or][oc] = {
            glyph: DOOR_GLYPHS[effective].horizontal,
            color: connectorColor(state),
            isPlayer: false,
            floor: 'same',
          };
        }
      }
      // north-south connector
      if (r < rows - 1) {
        const south = hasGrid ? getCell(payload, r + 1, c) : null;
        if (south && hasExit(here, 's') && hasExit(south, 'n')) {
          const or = 2 * r + 1;
          const oc = 2 * c;
          const state = combineDoorStates(doorStateAt(here, 's'), doorStateAt(south, 'n'));
          const effective = state ?? 'open';
          out[or][oc] = {
            glyph: DOOR_GLYPHS[effective].vertical,
            color: connectorColor(state),
            isPlayer: false,
            floor: 'same',
          };
        }
      }
    }
  }

  // Pass 3b: hidden doors. An immortal sees secret exits, which the
  // game sends in `cell.e` like any other exit, uppercase when they
  // lead elsewhere, with `cell.d[dir] === 'hidden'`. Place a dashed
  // connector in the slot pointing toward each one. A slot between two
  // rooms reads as a join, so it takes the connector only when the
  // exit lands on the room beside it. Toward an empty cell it marks the
  // secret exit, as the stub corridors() draws does. Only writes if
  // the slot is still empty so a regular connector from pass 3 (when
  // both sides lead to each other) wins.
  const hiddenDirOffsets: Array<[Dir, number, number, string]> = [
    ['n', 0, -1, '╎'],
    ['e', 1, 0, '╌'],
    ['s', 0, 1, '╎'],
    ['w', -1, 0, '╌'],
  ];
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      const here = hasGrid ? getCell(payload, r, c) : null;
      if (!here) continue;
      for (const [dir, dx, dy, glyph] of hiddenDirOffsets) {
        if (doorStateAt(here, dir) !== 'hidden') continue;
        if (getCell(payload, r + dy, c + dx) && !hasExit(here, dir)) continue;
        const or = 2 * r + dy;
        const oc = 2 * c + dx;
        if (or < 0 || or >= outRows || oc < 0 || oc >= outCols) continue;
        if (out[or][oc] !== EMPTY_CELL) continue;
        out[or][oc] = {
          glyph,
          color: DOOR_COLORS.hidden,
          isPlayer: false,
          floor: 'same',
        };
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

  return { cells: out, centerR, centerC };
}
