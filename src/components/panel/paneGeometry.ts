import {
  isLeaf,
  type PaneLeaf,
  type PaneNode,
  type PaneSplit,
  type SplitDir,
} from '../../lib/paneLayout';

// Pixel boxes for the panel's pane tree. PanelHost renders every pane
// as a flat, absolutely placed sibling keyed by its pane, so editing
// the tree moves boxes instead of re-parenting React subtrees. A
// re-parented subtree remounts, and a remount drops the map canvas
// and any scroll position. Pure so the rounding and the drag clamps
// are unit tested.

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
}

export interface PaneGeometry {
  leaves: LeafBox[];
  handles: HandleBox[];
}

/** Handle thickness. The visible line and the space it takes. */
export const HANDLE_PX = 1;
/** Smallest pane a drag leaves: the 28 px header and one dense row. */
export const MIN_PANE_H = 50;
/** Smallest side by side pane a drag leaves. */
export const MIN_PANE_W = 120;

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

/** Lay the tree out in a `width` by `height` box. */
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
  const sizes = distribute(
    Math.max(0, axis - HANDLE_PX * (n - 1)),
    node.children.map((c) => c.weight),
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
      });
      at += HANDLE_PX;
    }
  });
}

/** New child sizes after dragging the handle after child `index` by
 *  `delta` pixels. The two neighbours trade space, each keeping at
 *  least `min`, and every other child keeps its size. Returns the
 *  input when the pair has no room to give. */
export function dragSizes(
  sizes: readonly number[],
  index: number,
  delta: number,
  min: number,
): number[] {
  const a = sizes[index];
  const b = sizes[index + 1];
  if (a === undefined || b === undefined) return sizes.slice();
  const pair = a + b;
  if (pair < min * 2) return sizes.slice();
  const nextA = Math.min(pair - min, Math.max(min, Math.round(a + delta)));
  const out = sizes.slice();
  out[index] = nextA;
  out[index + 1] = pair - nextA;
  return out;
}
