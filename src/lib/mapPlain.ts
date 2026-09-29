// The One Window map drawing (SPEC 10 G7), laid out as plain data so
// the canvas painter in ServerMapView stays a thin loop and the
// geometry can be tested without a canvas.
//
// Rooms are filled squares on a 22 px grid, the lines between them
// run only through the gaps (so a translucent line never doubles up
// over a translucent room), and the room you stand in carries a ring
// 3 px outside it. Exits that lead somewhere the map does not show
// end in a short stub. Place labels come only from what the server
// names in the map data: the room flags for a bank, a healer, a
// trainer, or a shop.
//
// Every length below is the approved mockup's at 100% zoom. Zoom
// scales the grid, the rooms, the ring gap, and the stubs. Line
// widths and the 10 px label type stay put.

export type Dir = 'n' | 'e' | 's' | 'w';

/** One room on your floor, in the server grid's row and column. */
export interface PlainCell {
  row: number;
  col: number;
  /** Exit letters. Lowercase leads to the neighbor cell, uppercase
   *  leads off the grid. */
  exits: string;
  /** Flag letters from the server (s safe, $ shop, b bank, t trainer,
   *  h healer). */
  flags: string;
  /** Door state per direction, `hidden` for a secret exit. */
  doors?: Record<string, unknown>;
}

export interface PlainInput {
  cells: PlainCell[];
  /** The room you stand in, or null when the push does not say. */
  current: { row: number; col: number } | null;
  /** The drawing box in CSS pixels. */
  width: number;
  height: number;
  zoom: number;
  /** Width in CSS pixels of a label at the label font. */
  measure: (text: string) => number;
}

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface Segment {
  x1: number;
  y1: number;
  x2: number;
  y2: number;
}

export interface PlainLabel {
  text: string;
  /** Left edge and alphabetic baseline. */
  x: number;
  y: number;
}

export interface PlainScene {
  /** Room size and corner radius. */
  size: number;
  radius: number;
  /** Top left corners of every room but yours. */
  rooms: { x: number; y: number }[];
  /** Top left corner of your room. */
  current: { x: number; y: number } | null;
  /** The ring around your room, as the rect its stroke centers on. */
  ring: (Rect & { radius: number }) | null;
  /** Lines between rooms and exit stubs. */
  lines: Segment[];
  /** Secret exits, drawn dashed. */
  hidden: Segment[];
  labels: PlainLabel[];
}

// The mockup's lengths at 100% zoom.
export const PLAIN = {
  pitch: 22,
  room: 12,
  radius: 3,
  ringGap: 3,
  ringWidth: 2,
  lineWidth: 1.5,
  stub: 6,
  /** Label left edge past the room's right edge. */
  labelGap: 9,
  /** The 10 px label's box around its baseline. */
  labelAscent: 7.5,
  labelDescent: 2.5,
  /** Baseline below a room's center for a label beside it. */
  labelMid: 3.5,
  /** Space between a room and a label under or over it. */
  labelClear: 3.5,
  /** Space kept between the drawing and the box edge when the drawing
   *  is larger than the box. */
  edge: 8,
  font: 10,
} as const;

// Place names the map data carries, in the order one wins when a room
// has several.
const FLAG_LABELS: ReadonlyArray<[string, string]> = [
  ['b', 'Bank'],
  ['h', 'Healer'],
  ['t', 'Trainer'],
  ['$', 'Shop'],
];

/** The place name for a room's flags, or null when the flags name
 *  none. Safe rooms carry a flag but no name. */
export function placeLabel(flags: string | undefined): string | null {
  if (!flags) return null;
  for (const [flag, label] of FLAG_LABELS) {
    if (flags.includes(flag)) return label;
  }
  return null;
}

const STEPS: ReadonlyArray<[Dir, Dir, number, number]> = [
  ['n', 's', 0, -1],
  ['e', 'w', 1, 0],
  ['s', 'n', 0, 1],
  ['w', 'e', -1, 0],
];

interface Metrics {
  pitch: number;
  size: number;
  radius: number;
  ringGap: number;
  stub: number;
}

export function plainMetrics(zoom: number): Metrics {
  const z = Number.isFinite(zoom) && zoom > 0 ? zoom : 1;
  // An even room size keeps every room edge on a whole pixel.
  const size = Math.max(4, 2 * Math.round((PLAIN.room / 2) * z));
  const pitch = Math.max(size + 4, Math.round(PLAIN.pitch * z));
  const radius = Math.min(size / 2, Math.max(1, Math.round(PLAIN.radius * z * 2) / 2));
  const ringGap = Math.max(2, Math.round(PLAIN.ringGap * z));
  const stub = Math.max(3, Math.round(PLAIN.stub * z));
  return { pitch, size, radius, ringGap, stub };
}

function key(row: number, col: number): string {
  return `${row},${col}`;
}

/** Lay out the drawing for one Map.Tiles push. */
export function layoutPlain(input: PlainInput): PlainScene {
  const { cells, current, width, height } = input;
  const m = plainMetrics(input.zoom);
  const { pitch, size, radius, ringGap, stub } = m;
  const half = size / 2;
  const cap = PLAIN.lineWidth / 2;
  // How far the ring reaches past your room's edge.
  const ringReach = ringGap + PLAIN.ringWidth;

  const byKey = new Map<string, PlainCell>();
  for (const c of cells) byKey.set(key(c.row, c.col), c);
  const isCurrent = (row: number, col: number) =>
    current !== null && current.row === row && current.col === col;

  // World space: room centers on the grid, before the view offset.
  const cx = (col: number) => col * pitch;
  const cy = (row: number) => row * pitch;

  // Lines run from edge to edge through the gap. Round caps reach half
  // the line width past each end, so the ends sit that far inside, and
  // an end at your room starts past the ring instead.
  const lines: Segment[] = [];
  const hidden: Segment[] = [];
  const seen = new Set<string>();
  for (const cell of cells) {
    for (const [dir, opp, dx, dy] of STEPS) {
      const lower = cell.exits.includes(dir);
      const upper = cell.exits.includes(dir.toUpperCase());
      const nRow = cell.row + dy;
      const nCol = cell.col + dx;
      const neighbor = byKey.get(key(nRow, nCol)) ?? null;
      const secret = cell.doors?.[dir] === 'hidden' || neighbor?.doors?.[opp] === 'hidden';
      if (!lower && !upper && !secret) continue;
      const startAt = (isCurrent(cell.row, cell.col) ? half + ringReach : half) + cap;
      const x0 = cx(cell.col);
      const y0 = cy(cell.row);
      let seg: Segment | null = null;
      if (neighbor && !upper) {
        const pair =
          dx + dy > 0
            ? `${key(cell.row, cell.col)}>${key(nRow, nCol)}`
            : `${key(nRow, nCol)}>${key(cell.row, cell.col)}`;
        if (seen.has(pair)) continue;
        seen.add(pair);
        const endAt = pitch - (isCurrent(nRow, nCol) ? half + ringReach : half) - cap;
        if (endAt - startAt < 0.5) continue;
        seg = {
          x1: x0 + dx * startAt,
          y1: y0 + dy * startAt,
          x2: x0 + dx * endAt,
          y2: y0 + dy * endAt,
        };
      } else {
        const base = isCurrent(cell.row, cell.col) ? half + ringReach : half;
        const endAt = base + stub - cap;
        seg = {
          x1: x0 + dx * startAt,
          y1: y0 + dy * startAt,
          x2: x0 + dx * endAt,
          y2: y0 + dy * endAt,
        };
      }
      (secret ? hidden : lines).push(seg);
    }
  }

  // The drawing's extent is its rooms and the ring. Stubs may run
  // past it, the way the approved mockup centers the village.
  let lo = { x: Infinity, y: Infinity };
  let hi = { x: -Infinity, y: -Infinity };
  const grow = (col: number, row: number, r: number) => {
    lo = { x: Math.min(lo.x, cx(col) - r), y: Math.min(lo.y, cy(row) - r) };
    hi = { x: Math.max(hi.x, cx(col) + r), y: Math.max(hi.y, cy(row) + r) };
  };
  for (const c of cells) grow(c.col, c.row, half);
  if (current) grow(current.col, current.row, half + ringReach);
  if (!Number.isFinite(lo.x)) {
    return {
      size,
      radius,
      rooms: [],
      current: null,
      ring: null,
      lines: [],
      hidden: [],
      labels: [],
    };
  }

  // Center the drawing. When it is larger than the box, follow your
  // room instead, but never past the drawing's own edge, so the box
  // has no empty band while there is map to show.
  const fit = (min: number, max: number, view: number, focus: number | null) => {
    const span = max - min;
    if (span + 2 * PLAIN.edge <= view) return Math.round((view - (min + max)) / 2);
    const want = view / 2 - (focus ?? (min + max) / 2);
    const least = view - PLAIN.edge - max;
    const most = PLAIN.edge - min;
    return Math.round(Math.min(most, Math.max(least, want)));
  };
  const ox = fit(lo.x, hi.x, width, current ? cx(current.col) : null);
  const oy = fit(lo.y, hi.y, height, current ? cy(current.row) : null);
  const move = (s: Segment): Segment => ({
    x1: s.x1 + ox,
    y1: s.y1 + oy,
    x2: s.x2 + ox,
    y2: s.y2 + oy,
  });

  const rooms: { x: number; y: number }[] = [];
  for (const c of cells) {
    if (isCurrent(c.row, c.col)) continue;
    rooms.push({ x: cx(c.col) - half + ox, y: cy(c.row) - half + oy });
  }
  const here = current ? { x: cx(current.col) - half + ox, y: cy(current.row) - half + oy } : null;
  const ringInset = ringGap + PLAIN.ringWidth / 2;
  const ring = here
    ? {
        x: here.x - ringInset,
        y: here.y - ringInset,
        w: size + 2 * ringInset,
        h: size + 2 * ringInset,
        radius: radius + ringInset,
      }
    : null;
  const movedLines = lines.map(move);
  const movedHidden = hidden.map(move);

  const labels = placeLabels(input, m, ox, oy, rooms, here, ring, [...movedLines, ...movedHidden]);

  return {
    size,
    radius,
    rooms,
    current: here,
    ring,
    lines: movedLines,
    hidden: movedHidden,
    labels,
  };
}

function overlaps(a: Rect, b: Rect): boolean {
  return a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
}

function inflate(r: Rect, by: number): Rect {
  return { x: r.x - by, y: r.y - by, w: r.w + 2 * by, h: r.h + 2 * by };
}

// Each named room tries its label to the right, then under and over
// that spot, then to the left, and takes the first that stays inside
// the box and clear of every room, line, and label already placed.
// Your room goes first, then the rest by distance from you, so the
// names near you win a crowded spot.
function placeLabels(
  input: PlainInput,
  m: Metrics,
  ox: number,
  oy: number,
  rooms: { x: number; y: number }[],
  here: { x: number; y: number } | null,
  ring: Rect | null,
  lines: Segment[],
): PlainLabel[] {
  const { cells, current, width, height, measure } = input;
  const { pitch, size, ringGap } = m;
  const named = cells
    .map((c) => ({ cell: c, text: placeLabel(c.flags) }))
    .filter((n): n is { cell: PlainCell; text: string } => n.text !== null);
  if (named.length === 0) return [];

  const dist = (c: PlainCell) =>
    current ? Math.abs(c.row - current.row) + Math.abs(c.col - current.col) : 0;
  named.sort((a, b) => dist(a.cell) - dist(b.cell));

  const blocks: Rect[] = [];
  for (const r of rooms) blocks.push(inflate({ x: r.x, y: r.y, w: size, h: size }, 2));
  if (here) blocks.push(inflate({ x: here.x, y: here.y, w: size, h: size }, 2));
  if (ring) blocks.push(inflate(ring, 1 + PLAIN.ringWidth / 2));
  const reach = PLAIN.lineWidth / 2 + 1;
  for (const s of lines) {
    blocks.push({
      x: Math.min(s.x1, s.x2) - reach,
      y: Math.min(s.y1, s.y2) - reach,
      w: Math.abs(s.x2 - s.x1) + 2 * reach,
      h: Math.abs(s.y2 - s.y1) + 2 * reach,
    });
  }

  const gap = PLAIN.labelGap - PLAIN.ringGap + ringGap;
  const view: Rect = { x: 2, y: 2, w: width - 4, h: height - 4 };
  const placed: PlainLabel[] = [];
  const half = size / 2;
  for (const { cell, text } of named) {
    const x = cell.col * pitch - half + ox;
    const y = cell.row * pitch - half + oy;
    const w = measure(text);
    const mid = y + half + PLAIN.labelMid;
    const right = x + size + gap;
    const candidates: PlainLabel[] = [
      { text, x: right, y: mid },
      { text, x: right, y: y + size + PLAIN.labelClear + PLAIN.labelAscent },
      { text, x: right, y: y - PLAIN.labelClear - PLAIN.labelDescent },
      { text, x: x - gap - w, y: mid },
    ];
    for (const c of candidates) {
      const box: Rect = {
        x: c.x,
        y: c.y - PLAIN.labelAscent,
        w,
        h: PLAIN.labelAscent + PLAIN.labelDescent,
      };
      const inside =
        box.x >= view.x &&
        box.y >= view.y &&
        box.x + box.w <= view.x + view.w &&
        box.y + box.h <= view.y + view.h;
      if (!inside) continue;
      if (blocks.some((b) => overlaps(b, box))) continue;
      placed.push(c);
      blocks.push(inflate(box, 4));
      break;
    }
  }
  return placed;
}
