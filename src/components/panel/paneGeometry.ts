import {
  isLeaf,
  type PaneLeaf,
  type PaneNode,
  type PaneSplit,
  type PaneType,
  type SplitDir,
} from '../../lib/paneLayout';

// Pixel boxes for the panel's pane tree. PanelHost renders every pane
// as a flat, absolutely placed sibling keyed by its pane, so editing
// the tree moves boxes instead of re-parenting React subtrees. A
// re-parented subtree remounts, and a remount drops the map canvas
// and any scroll position. Pure so the rounding, the minimum sizes,
// and the drag clamps are unit tested.
//
// Every pane type has a minimum height it can be read at. While the
// panel has room for every minimum, panes share the space by weight
// and none drops below its own. On a panel too short for that, every
// pane keeps its header and one row, the heaviest panes then get
// their minimum in turn, and the lightest ones come up short and
// scroll inside their own box. No two boxes ever overlap.

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
/** The pane header, the --pane-header token. */
const HEADER_PX = 28;
/** One dense row, the --row token. */
const ROW_PX = 22;

/** The height each pane type needs to be read: Affects its header and
 *  six rows, Group and Staff queues their header and three rows, Chat
 *  a couple of messages, and the Map a drawing you can follow. */
export const PANE_MIN_H: Record<PaneType, number> = {
  map: 180,
  affects: HEADER_PX + 6 * ROW_PX,
  group: HEADER_PX + 3 * ROW_PX,
  chat: 120,
  imm: HEADER_PX + 3 * ROW_PX,
};
/** The least a pane gets on a panel too short for every minimum: its
 *  header and one dense row, with the rest scrolling inside. */
export const PANE_FLOOR_H = HEADER_PX + ROW_PX;
/** Narrowest a side by side pane gets. */
export const MIN_PANE_W = 120;

/** The least room `node` needs along `dir`'s axis, height for a
 *  column and width for a row. With `floor`, the least it gets on a
 *  panel too short for every minimum instead. */
export function minExtent(node: PaneNode, dir: SplitDir, floor = false): number {
  if (isLeaf(node)) {
    if (dir === 'row') return MIN_PANE_W;
    const min = PANE_MIN_H[node.pane];
    return floor ? Math.min(PANE_FLOOR_H, min) : min;
  }
  const parts = node.children.map((c) => minExtent(c, dir, floor));
  if (parts.length === 0) return 0;
  if (node.split !== dir) return Math.max(...parts);
  return parts.reduce((acc, p) => acc + p, 0) + HANDLE_PX * (parts.length - 1);
}

/** True when a `width` by `height` panel holds every pane of the tree
 *  at its minimum. */
export function fitsPanel(root: PaneSplit, width: number, height: number): boolean {
  return minExtent(root, 'row') <= width && minExtent(root, 'column') <= height;
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
 *  minimum or more while the box holds them all. */
export function layoutPanes(root: PaneSplit, width: number, height: number): PaneGeometry {
  const out: PaneGeometry = { leaves: [], handles: [] };
  place(root, { x: 0, y: 0, w: Math.max(0, width), h: Math.max(0, height) }, out);
  return out;
}

function place(node: PaneNode, rect: Rect, out: PaneGeometry): void {
  if (isLeaf(node)) {
    out.leaves.push({ leaf: node, rect });
    return;
  }
  const n = node.children.length;
  if (n === 0) return;
  const vertical = node.split === 'column';
  const axis = vertical ? rect.h : rect.w;
  const mins = node.children.map((c) => minExtent(c, node.split));
  const sizes = allocate(
    Math.max(0, axis - HANDLE_PX * (n - 1)),
    node.children.map((c) => c.weight),
    mins,
    node.children.map((c) => minExtent(c, node.split, true)),
  );
  let at = vertical ? rect.y : rect.x;
  node.children.forEach((child, i) => {
    const size = sizes[i];
    const box: Rect = vertical
      ? { x: rect.x, y: at, w: rect.w, h: size }
      : { x: at, y: rect.y, w: size, h: rect.h };
    place(child, box, out);
    at += size;
    if (i < n - 1) {
      out.handles.push({
        parentId: node.id,
        index: i,
        dir: node.split,
        rect: vertical
          ? { x: rect.x, y: at, w: rect.w, h: HANDLE_PX }
          : { x: at, y: rect.y, w: HANDLE_PX, h: rect.h },
        sizes,
        mins,
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
