import { isTrackedRow, type AffectRow } from './affects/affectsView';
import { isChipsStyle, type AffectsStyle } from '../ipc/affects';
import { affectsColumns, affectsRestMinRows, affectsRulePx } from './affects/affectsGrid';
import {
  chipGroups,
  chipLabelMode,
  chipsMinBody,
  FIXED_MEASURE,
  type ChipMeasure,
} from './affects/chipsGrid';
import { countdownMinRows, countdownRowPx } from './affects/countdownGrid';
import { affectHours } from './paneText';
import { PANE_TEXT_PX, paneText, textPx } from './paneTextSize';
import {
  PANE_HEADER_PX,
  PANE_MIN_H,
  PANE_ROW_PX,
  isLeaf,
  type PaneKind,
  type PaneLeaf,
  type PaneNode,
  type PaneSplit,
  type PaneType,
  type SplitDir,
} from './paneLayout';

export { PANE_MIN_H };

// Pixel boxes for the panel's pane tree. PanelHost renders every pane
// as a flat, absolutely placed sibling keyed by its pane, so editing
// the tree moves boxes instead of re-parenting React subtrees. A
// re-parented subtree remounts, and a remount drops the map canvas
// and any scroll position. Pure so the rounding, the minimum sizes,
// and the drag clamps are unit tested.
//
// Every pane type has a minimum height it can be read at. Affects and
// Group raise theirs to hold the rows you most need, your tracked
// affects and anything harmful on you, or a member in danger, so those
// never fall below the fold. While the panel has room for every
// minimum, panes share the space by weight and none drops below its
// own. On a panel too short for that, every pane keeps its header and
// one row, the heaviest panes then get their minimum in turn, and the
// lightest ones come up short and scroll inside their own box. No two
// boxes ever overlap.
//
// Every pane draws at your panel size (paneTextSize.ts), so every
// minimum, and the floor on a short panel, counts the header and the
// rows at that size.

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface LeafBox {
  leaf: PaneLeaf;
  rect: Rect;
}

/** The 1 px line between two siblings, which you drag to resize them. */
export interface HandleBox {
  /** The split whose children the handle sits between. */
  parentId: string;
  /** The handle sits after child `index` and before child `index + 1`. */
  index: number;
  /** The parent's direction. A `column` handle is a horizontal line
   *  you drag up and down, a `row` handle a vertical one. */
  dir: SplitDir;
  rect: Rect;
  /** Pixel size of every child of the parent along its axis, in
   *  order. A drag rewrites two of them and hands all to setWeights. */
  sizes: number[];
  /** The minimum of every child of the parent along its axis, in
   *  order. A drag leaves each neighbour at least this much. */
  mins: number[];
}

export interface PaneGeometry {
  leaves: LeafBox[];
  handles: HandleBox[];
}

/** Handle thickness. The visible line and the space it takes. */
export const HANDLE_PX = 1;

/** The least a pane gets on a panel too short for every minimum: its
 *  header and one dense row, with the rest scrolling inside. At 12 px,
 *  and paneFloorH at your size. */
export const PANE_FLOOR_H = PANE_HEADER_PX + PANE_ROW_PX;

/** The floor at panel size `size` px. */
export function paneFloorH(size: number = PANE_TEXT_PX): number {
  const { header, row } = paneText(size);
  return header + row;
}
/** Narrowest a side by side pane gets. */
export const MIN_PANE_W = 120;

/** Minimum heights that follow what a pane shows right now, in place
 *  of its PANE_MIN_H entry. */
export type PaneMins = Partial<Record<PaneKind, number>>;

/** A pane type's stock minimum height at panel size `size` px:
 *  Affects its header and six rows, Group, Staff queues and a Lua pane
 *  their header and three rows, and the Map and Chat their header and
 *  a body as much taller as their text. Each is its PANE_MIN_H entry
 *  at 12 px. The pinned writing card keeps its entry at every size. */
export function paneMinH(pane: PaneKind, size: number = PANE_TEXT_PX): number {
  const text = paneText(size);
  if (pane === 'affects') return text.header + 6 * text.affectsRow;
  if (pane === 'group' || pane === 'imm' || pane === 'lua') return text.header + 3 * text.row;
  // The writing card draws at your terminal size, not the panel's.
  if (pane === 'writing') return PANE_MIN_H.writing;
  return text.header + textPx(PANE_MIN_H[pane] - PANE_HEADER_PX, size);
}

/** Most rows a list pane holds on to as its minimum, so a long list
 *  never crowds every other pane out of the panel. */
const LIST_MIN_ROWS = 12;
/** Group members a Group pane holds on to as its minimum. */
const GROUP_MIN_ROWS = 6;

// A minimum that shows `shows` px of a `total` px list, plus a peek at
// the next row when the list goes on past it: half a `row`, under the
// bottom fade, so you can tell it goes on.
function withPeek(shows: number, total: number, row: number): number {
  return total > shows ? Math.min(total, shows + Math.round(row / 2)) : shows;
}

/** The Affects pane's minimum for the rows it shows in `columns`
 *  columns, at panel size `size` px: every tracked slot, then
 *  under the hairline every harmful affect and the count cell after
 *  them. Never under the stock minimum, and never over a dozen rows.
 *  The pane shows whole rows only and counts the rest, so no peek
 *  follows. */
export function affectsMinH(
  rows: readonly AffectRow[],
  columns = 2,
  size: number = PANE_TEXT_PX,
): number {
  const tracked = rows.filter(isTrackedRow).length;
  const trackedRows = Math.ceil(tracked / columns);
  const restRows = affectsRestMinRows(rows, columns);
  const rule = tracked > 0 && restRows > 0 ? affectsRulePx(size) : 0;
  const { header, affectsRow: row } = paneText(size);
  const need = header + Math.min(LIST_MIN_ROWS, trackedRows + restRows) * row;
  return Math.max(paneMinH('affects', size), need + rule);
}

/** The Countdown pane's minimum for the rows it shows in `columns`
 *  columns, at panel size `size` px: every row down to the last
 *  one missing, running out, or harmful, and the count after it when
 *  more follow. Never under the stock minimum, and never over a dozen
 *  rows. */
export function countdownMinH(
  rows: readonly AffectRow[],
  columns = 2,
  size: number = PANE_TEXT_PX,
): number {
  const need = Math.min(LIST_MIN_ROWS, countdownMinRows(rows, columns)) * countdownRowPx(size);
  return Math.max(paneMinH('affects', size), paneText(size).header + need);
}

/** The Grouped chips pane's minimum `width` wide, at panel size
 *  `size` px: every Recast and Tracked chip and every harmful one on
 *  the first page, and the count after them when more follow, packed as
 *  the pane packs them with the same `measure`. Never under the stock
 *  minimum, and never over a dozen rows' worth. */
export function chipsMinH(
  rows: readonly AffectRow[],
  width: number,
  measure: ChipMeasure = FIXED_MEASURE,
  size: number = PANE_TEXT_PX,
): number {
  const body = chipsMinBody(
    chipGroups(rows),
    width,
    (r) => affectHours(r.state, r.ticks),
    measure,
    chipLabelMode(rows, width, size),
    LIST_MIN_ROWS * paneText(size).affectsRow,
    size,
  );
  return Math.max(paneMinH('affects', size), paneText(size).header + body);
}

/** The Affects pane's minimum in `root` laid out `width` wide, for the
 *  style it draws, at panel size `size` px. The pane draws one
 *  column or two by its own width, which a Split right halves, so the
 *  minimum counts the columns the pane draws, and the chips pack to
 *  it. A tree without the pane reads the panel's width. */
export function affectsMinIn(
  root: PaneSplit,
  width: number,
  rows: readonly AffectRow[],
  style: AffectsStyle = 'timers',
  measure: ChipMeasure = FIXED_MEASURE,
  size: number = PANE_TEXT_PX,
): number {
  const paneW = paneWidth(root, width, 'affects') ?? width;
  return affectsStyleMinH(rows, paneW, style, measure, size);
}

/** The Affects pane's minimum `paneW` wide for the style it draws,
 *  at panel size `size` px. */
export function affectsStyleMinH(
  rows: readonly AffectRow[],
  paneW: number,
  style: AffectsStyle,
  measure: ChipMeasure = FIXED_MEASURE,
  size: number = PANE_TEXT_PX,
): number {
  const columns = affectsColumns(paneW, size);
  if (style === 'countdown') return countdownMinH(rows, columns, size);
  if (isChipsStyle(style)) return chipsMinH(rows, paneW, measure, size);
  return affectsMinH(rows, columns, size);
}

/** The Group pane's minimum for `members` rows at panel size `size`
 *  px: every member up to six, so the one in danger is never the row
 *  cut off, then a peek at the seventh. */
export function groupMinH(members: number, size: number = PANE_TEXT_PX): number {
  const { header, row } = paneText(size);
  const count = Math.max(0, Math.floor(members));
  const shows = Math.max(paneMinH('group', size), header + Math.min(GROUP_MIN_ROWS, count) * row);
  return withPeek(shows, header + count * row, row);
}

/** The least room `node` needs along `dir`'s axis, height for a
 *  column and width for a row. With `floor`, the least it gets on a
 *  panel too short for every minimum instead. `mins` overrides the
 *  stock minimum height of a pane type at panel size `size` px. */
export function minExtent(
  node: PaneNode,
  dir: SplitDir,
  floor = false,
  mins: PaneMins = {},
  size: number = PANE_TEXT_PX,
): number {
  if (isLeaf(node)) {
    if (dir === 'row') return MIN_PANE_W;
    const min = mins[node.pane] ?? paneMinH(node.pane, size);
    return floor ? Math.min(paneFloorH(size), min) : min;
  }
  const parts = node.children.map((c) => minExtent(c, dir, floor, mins, size));
  if (parts.length === 0) return 0;
  if (node.split !== dir) return Math.max(...parts);
  return parts.reduce((acc, p) => acc + p, 0) + HANDLE_PX * (parts.length - 1);
}

/** True when a `width` by `height` panel holds every pane of the tree
 *  at its minimum at panel size `size` px. */
export function fitsPanel(
  root: PaneSplit,
  width: number,
  height: number,
  size: number = PANE_TEXT_PX,
): boolean {
  return (
    minExtent(root, 'row', false, {}, size) <= width &&
    minExtent(root, 'column', false, {}, size) <= height
  );
}

/** Split `total` pixels by `weights`, whole pixels that sum to
 *  `total`. Rounds the running edge, so no share drifts by more than
 *  a pixel however many siblings there are. */
export function distribute(total: number, weights: readonly number[]): number[] {
  const size = Math.max(0, Math.floor(total));
  const sum = weights.reduce((acc, w) => acc + (w > 0 ? w : 0), 0);
  if (weights.length === 0) return [];
  const shares =
    sum > 0 ? weights.map((w) => (w > 0 ? w / sum : 0)) : weights.map(() => 1 / weights.length);
  const out: number[] = [];
  let acc = 0;
  let prevEdge = 0;
  shares.forEach((share, i) => {
    acc += share;
    const edge = i === shares.length - 1 ? size : Math.round(acc * size);
    out.push(Math.max(0, edge - prevEdge));
    prevEdge = edge;
  });
  return out;
}

const sum = (xs: readonly number[]) => xs.reduce((acc, x) => acc + x, 0);

/** Split `total` pixels by `weights` the way {@link distribute} does,
 *  but never give a child less than its entry in `mins`. A child whose
 *  share falls short gets its minimum and the rest share what is left
 *  by weight. When `total` cannot hold every minimum, each child first
 *  gets its entry in `floors`, then the heaviest children get their
 *  minimum in turn (ties go to the earlier child) until the space runs
 *  out. Whole pixels that sum to `total`. */
export function allocate(
  total: number,
  weights: readonly number[],
  mins: readonly number[],
  floors: readonly number[] = mins,
): number[] {
  const size = Math.max(0, Math.floor(total));
  if (weights.length === 0) return [];
  if (sum(mins) <= size) return shareAboveMins(size, weights, mins);
  const floorSum = sum(floors);
  if (floorSum >= size) return distribute(size, floors);
  const out = floors.slice();
  let spare = size - floorSum;
  const heaviestFirst = weights.map((_, i) => i).sort((a, b) => weights[b] - weights[a] || a - b);
  for (const i of heaviestFirst) {
    const add = Math.min(spare, Math.max(0, mins[i] - floors[i]));
    out[i] += add;
    spare -= add;
  }
  // Only reachable with a floor above its minimum, which no pane has.
  out[out.length - 1] += spare;
  return out;
}

// `size` holds every minimum. Pin each child whose share by weight
// falls short to its minimum and share the rest among the others,
// until no share falls short. Pinning a child only shrinks the other
// shares, so every child pinned in a pass stays pinned. Rounding the
// running edge keeps a pinned child at exactly its minimum and the
// others at or above theirs.
function shareAboveMins(
  size: number,
  weights: readonly number[],
  mins: readonly number[],
): number[] {
  const positive = weights.map((w) => (Number.isFinite(w) && w > 0 ? w : 0));
  const w = positive.some((x) => x > 0) ? positive : positive.map(() => 1);
  const pinned = w.map(() => false);
  let remaining = size;
  const freeWeight = () => w.reduce((acc, x, i) => (pinned[i] ? acc : acc + x), 0);
  const share = (i: number, free: number) => (free > 0 ? (remaining * w[i]) / free : 0);
  for (;;) {
    const free = freeWeight();
    const short = w.map((_, i) => i).filter((i) => !pinned[i] && share(i, free) < mins[i]);
    if (short.length === 0) break;
    for (const i of short) {
      pinned[i] = true;
      remaining -= mins[i];
    }
  }
  const free = freeWeight();
  const exact = w.map((_, i) => (pinned[i] ? mins[i] : share(i, free)));
  const out: number[] = [];
  let acc = 0;
  let prevEdge = 0;
  exact.forEach((part, i) => {
    acc += part;
    const edge = i === exact.length - 1 ? size : Math.round(acc);
    out.push(Math.max(0, edge - prevEdge));
    prevEdge = edge;
  });
  return out;
}

/** Lay the tree out in a `width` by `height` box, every pane at its
 *  minimum or more while the box holds them all. `mins` overrides the
 *  stock minimum height of a pane type at panel size `size` px. */
export function layoutPanes(
  root: PaneSplit,
  width: number,
  height: number,
  mins: PaneMins = {},
  size: number = PANE_TEXT_PX,
): PaneGeometry {
  const out: PaneGeometry = { leaves: [], handles: [] };
  place(root, { x: 0, y: 0, w: Math.max(0, width), h: Math.max(0, height) }, out, mins, size);
  return out;
}

/** The width `pane` gets in a `width` wide panel, or null when the
 *  tree does not hold it. Row splits share the width by weight and
 *  MIN_PANE_W alone, so no minimum height moves it, and PanelHost
 *  reads it before it knows those. */
export function paneWidth(root: PaneSplit, width: number, pane: PaneType): number | null {
  const box = layoutPanes(root, width, 0).leaves.find((l) => l.leaf.pane === pane);
  return box ? box.rect.w : null;
}

function place(node: PaneNode, rect: Rect, out: PaneGeometry, mins: PaneMins, size: number): void {
  if (isLeaf(node)) {
    out.leaves.push({ leaf: node, rect });
    return;
  }
  const n = node.children.length;
  if (n === 0) return;
  const vertical = node.split === 'column';
  const axis = vertical ? rect.h : rect.w;
  const childMins = node.children.map((c) => minExtent(c, node.split, false, mins, size));
  const sizes = allocate(
    Math.max(0, axis - HANDLE_PX * (n - 1)),
    node.children.map((c) => c.weight),
    childMins,
    node.children.map((c) => minExtent(c, node.split, true, mins, size)),
  );
  let at = vertical ? rect.y : rect.x;
  node.children.forEach((child, i) => {
    const extent = sizes[i];
    const box: Rect = vertical
      ? { x: rect.x, y: at, w: rect.w, h: extent }
      : { x: at, y: rect.y, w: extent, h: rect.h };
    place(child, box, out, mins, size);
    at += extent;
    if (i < n - 1) {
      out.handles.push({
        parentId: node.id,
        index: i,
        dir: node.split,
        rect: vertical
          ? { x: rect.x, y: at, w: rect.w, h: HANDLE_PX }
          : { x: at, y: rect.y, w: HANDLE_PX, h: rect.h },
        sizes,
        mins: childMins,
      });
      at += HANDLE_PX;
    }
  });
}

/** New child sizes after dragging the handle after child `index` by
 *  `delta` pixels. The two neighbours trade space, the one before the
 *  handle keeping at least `minBefore` and the one after at least
 *  `minAfter`, and every other child keeps its size. Returns the input
 *  when the pair has no room to give, as on a panel too short for
 *  every minimum. */
export function dragSizes(
  sizes: readonly number[],
  index: number,
  delta: number,
  minBefore: number,
  minAfter: number = minBefore,
): number[] {
  const a = sizes[index];
  const b = sizes[index + 1];
  if (a === undefined || b === undefined) return sizes.slice();
  const pair = a + b;
  if (pair < minBefore + minAfter) return sizes.slice();
  const nextA = Math.min(pair - minAfter, Math.max(minBefore, Math.round(a + delta)));
  const out = sizes.slice();
  out[index] = nextA;
  out[index + 1] = pair - nextA;
  return out;
}
