import type { PanelSide } from '../../ipc/uiConfig';
import {
  WRITING_PANE,
  findNode,
  isLeaf,
  moveLeaf,
  moveToOtherTree,
  treeIn,
  treeOf,
  type PaneDrop,
  type PaneLayout,
  type PaneNode,
  type PaneTree,
} from '../../panel/paneLayout';
import type { PaneSchematic, SchematicRect } from './paneSchematic';

// Moving panes on the drawing under Panel layout, by the pointer or the
// keys, with the rules a move in the window keeps. A pane dragged over
// another lands above it in its top half and below it in its bottom
// half, which washes in the accent with a 2 px line along that edge, and
// a pane dragged over a box with no panes lands alone in it. Up and Down
// move a pane past the one before or after it in reading order, and the
// arrow toward the other box moves it to the foot of that box. Pure, so
// each rule is tested without a page.

/** One box of the drawing, at `x` in the drawing's units, whose panes
 *  fill the rows above `bottom`. */
export interface SchematicBox {
  tree: PaneTree;
  x: number;
  bottom: number;
  drawing: PaneSchematic;
}

/** Where a drag would land now, and what marks it, in the drawing's
 *  units. */
export interface SchematicDrop {
  drop: PaneDrop;
  wash: SchematicRect;
  line: SchematicRect;
}

const inside = (r: SchematicRect, x: number, y: number) =>
  x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height;

/** Where the pane `dragging` lands with the pointer at `x`, `y`: against
 *  the pane under it, alone in an empty box, or nowhere, as over its own
 *  pane, a line between panes or Vitals. */
export function dropAt(
  boxes: readonly SchematicBox[],
  x: number,
  y: number,
  dragging: string,
): SchematicDrop | null {
  const box = boxes.find((b) => x >= b.x && x < b.x + b.drawing.width && y >= 0 && y < b.bottom);
  if (!box) return null;
  const at = { x: x - box.x, y };
  const width = box.drawing.width;
  if (box.drawing.regions.length === 0) {
    return {
      drop: { tree: box.tree, at: null, edge: 'after' },
      wash: { x: box.x + 1, y: 1, width: width - 2, height: box.bottom - 1 },
      line: { x: box.x + 1, y: 1, width: width - 2, height: 2 },
    };
  }
  const region = box.drawing.regions.find((r) => inside(r.rect, at.x, at.y));
  if (!region || region.id === dragging) return null;
  const r = region.rect;
  const half = r.height / 2;
  const before = at.y < r.y + half;
  const lineY = before ? Math.max(0, r.y - 1) : Math.min(box.bottom - 2, r.y + r.height - 1);
  return {
    drop: { tree: box.tree, at: region.id, edge: before ? 'before' : 'after' },
    wash: { x: box.x + r.x, y: before ? r.y : r.y + half, width: r.width, height: half },
    line: { x: box.x + r.x, y: lineY, width: r.width, height: 2 },
  };
}

/** Whether the pane `id` moves at all. The writing card stays put. */
export function movable(layout: PaneLayout, id: string): boolean {
  const which = treeOf(layout, id);
  if (which === null) return false;
  const leaf = findNode(treeIn(layout, which), id);
  return leaf !== null && isLeaf(leaf) && leaf.pane !== WRITING_PANE;
}

function leafIds(node: PaneNode, out: string[] = []): string[] {
  if (isLeaf(node)) out.push(node.id);
  else node.children.forEach((child) => leafIds(child, out));
  return out;
}

/** The side of the drawing the box `tree` takes. The sidebar takes the
 *  side the panel does not. */
export function boxSide(tree: PaneTree, panelSide: PanelSide): 'left' | 'right' {
  if (tree === 'panel') return panelSide;
  return panelSide === 'right' ? 'left' : 'right';
}

/** The layout after `key` on the pane `id`, or null when the key moves
 *  nothing. */
export function keyMove(
  layout: PaneLayout,
  id: string,
  key: string,
  panelSide: PanelSide,
): PaneLayout | null {
  const which = treeOf(layout, id);
  if (which === null || !movable(layout, id)) return null;
  if (key === 'ArrowUp' || key === 'ArrowDown') {
    const ids = leafIds(treeIn(layout, which));
    const at = ids.indexOf(id);
    const next = key === 'ArrowUp' ? ids[at - 1] : ids[at + 1];
    if (next === undefined) return null;
    const edge = key === 'ArrowUp' ? 'before' : 'after';
    return moveLeaf(layout, id, { tree: which, at: next, edge });
  }
  if (key === 'ArrowLeft' || key === 'ArrowRight') {
    const toward = key === 'ArrowLeft' ? 'left' : 'right';
    if (boxSide(which, panelSide) === toward) return null;
    return moveToOtherTree(layout, id);
  }
  return null;
}
