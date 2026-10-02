import { SECTORS, UNKNOWN_GLYPH, sectorForCode, sectorGlyphColor } from './mapPalette';

// Reading a Map.Tiles packet. Aabahran builds the grid in generate_map
// (minimap.c) and writes it in gmcp_send_map (gmcp.c). The painters in
// ServerMapView draw what these functions read, so the rooms, the
// corridors and the glyph grid can be checked against real packets
// without a canvas.

/// One cell of the server-side map grid (player's floor only).
/// Per the Aabahran GMCP wiki:
/// `g[y][x]` carries `{s, e, l, h, ar, f, d, ex}`. Multi-floor rooms
/// live in a separate top-level `zr` array (see `MultiZEntry`), not
/// in the `g` grid.
export interface ServerCell {
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

export function gridDims(payload: MapTilesPayload): { rows: number; cols: number } {
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

export type Dir = 'n' | 's' | 'e' | 'w';

export function hasExit(cell: ServerCell, dir: Dir): boolean {
  return Boolean(cell.e && cell.e.toLowerCase().includes(dir));
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
/// to (radius+1) per the GMCP spec when no cell carries the flag. Old
/// "midpoint of observed cells" math broke on sparse grids — the
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
          if (r > 0 && c > 0) return { row: r, col: c };
        }
      }
    }
  }
  if (typeof payload.r === 'number' && payload.r > 0) {
    return { row: payload.r + 1, col: payload.r + 1 };
  }
  return { row: Math.floor((rows + 1) / 2), col: Math.floor((cols + 1) / 2) };
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
  for (let r = 1; r <= rows; r++) {
    for (let c = 1; c <= cols; c++) {
      const cell = getCell(payload, r, c);
      if (cell) out.push({ row: r, col: c, cell });
    }
  }
  return out;
}

/** One corridor stroke from a room's center toward the next cell, in
 *  grid units. `reach` is 1 when it meets the room there and 0.5 for
 *  the stub a hidden door draws toward a cell with no room. */
export interface Corridor {
  row: number;
  col: number;
  dx: number;
  dy: number;
  reach: number;
  state: DoorState;
}

const DIR_OFFSETS: Array<[Dir, Dir, number, number]> = [
  ['n', 's', 0, -1],
  ['e', 'w', 1, 0],
  ['s', 'n', 0, 1],
  ['w', 'e', -1, 0],
];

/** The corridors the squares and tileset styles stroke under the rooms.
 *
 *  Hidden doors get special handling: the server omits the direction
 *  from `cell.e` AND the hidden room beyond may be absent from the grid
 *  (the immortal-only `d[dir] = "hidden"` flag is the ONLY signal we
 *  have). So the hidden path (a) ignores `hasExit`, (b) accepts a null
 *  neighbor and draws a half-pitch stub so the line says "secret exit"
 *  without claiming a room that is not actually rendered. */
export function corridors(payload: MapTilesPayload, rows: number, cols: number): Corridor[] {
  const out: Corridor[] = [];
  for (const { row, col, cell } of gridRooms(payload, rows, cols)) {
    for (const [dir, opp, dx, dy] of DIR_OFFSETS) {
      const neighbor = getCell(payload, row + dy, col + dx);
      const here = doorStateAt(cell, dir);
      const there = neighbor ? doorStateAt(neighbor, opp) : null;
      const state = combineDoorStates(here, there);
      const isHidden = state === 'hidden';
      if (!hasExit(cell, dir) && !isHidden) continue;
      if (!isHidden && !neighbor) continue;
      out.push({ row, col, dx, dy, reach: neighbor ? 1 : 0.5, state: state ?? 'open' });
    }
  }
  return out;
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
//   1. Off-floor rooms (a / b / zr) — dim sector glyph as a
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
 *  grid. */
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
  // travel both ways). Door state (cell.d) picks the connector
  // color: open → gray, closed → amber, locked → red, hidden →
  // pink + dashed glyph (see DOOR_COLORS / DOOR_GLYPHS).
  const OPEN_COLOR = 'var(--c-border-strong, var(--c-border))';
  const connectorColor = (state: DoorState | null): string =>
    state && state !== 'open' ? DOOR_COLORS[state] : OPEN_COLOR;
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
      if (r < rows) {
        const south = hasGrid ? getCell(payload, r + 1, c) : null;
        if (south && hasExit(here, 's') && hasExit(south, 'n')) {
          const or = 2 * (r - 1) + 1;
          const oc = 2 * (c - 1);
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

  // Pass 3b: hidden doors. The server omits the direction from
  // `cell.e` and the hidden room beyond is normally absent from the
  // grid — `cell.d[dir] === 'hidden'` is the only signal we have.
  // Iterate all four directions of every cell and place a dashed
  // connector in the slot pointing toward the secret exit. Only
  // writes if the slot is still empty so a regular connector from
  // pass 3 (when both sides happen to be visible AND flagged
  // hidden) wins.
  const hiddenDirOffsets: Array<[Dir, number, number, string]> = [
    ['n', 0, -1, '╎'],
    ['e', 1, 0, '╌'],
    ['s', 0, 1, '╎'],
    ['w', -1, 0, '╌'],
  ];
  for (let r = 1; r <= rows; r++) {
    for (let c = 1; c <= cols; c++) {
      const here = hasGrid ? getCell(payload, r, c) : null;
      if (!here) continue;
      for (const [dir, dx, dy, glyph] of hiddenDirOffsets) {
        if (doorStateAt(here, dir) !== 'hidden') continue;
        const or = 2 * (r - 1) + dy;
        const oc = 2 * (c - 1) + dx;
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
