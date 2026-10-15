import type { PointerEvent as ReactPointerEvent } from 'react';
import { flushSync } from 'react-dom';
import { pushEscape } from '../../lib/escapeStack';
import { MIN_PANE_W } from '../paneGeometry';
import type { DropEdge, PaneLeaf, PaneTree } from '../paneLayout';
import { dragTo, endDrag, movable, startDrag, type ArrangeTarget } from './arrange';

// Dragging a tile in arrange mode. A press that moves 4 px picks the
// tile up, and the panes close over the space it left. Those boxes are
// measured once, and every target comes from them, so the preview can
// reflow the panes under the pointer without moving the targets. Over a
// pane, its left or right quarter puts the tile beside it when both fit
// side by side, and its top or bottom half above or below it. Over an
// empty side, the tile goes there alone, and over the shelf, away. Esc
// or letting go anywhere else leaves it where it was.

/** How far a press moves before it becomes a drag. */
const THRESHOLD = 4;

/** The share of a pane's width, from each side, that puts a tile beside
 *  it rather than above or below it. */
const SIDE = 0.25;

/** A box in window pixels. */
export interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** The targets of one drag, measured on the layout without the tile. */
export interface Targets {
  leaves: (Box & { id: string; tree: PaneTree })[];
  /** A side that holds no pane. */
  empty: (Box & { tree: PaneTree })[];
  shelf: Box | null;
}

function boxOf(el: Element): Box {
  const r = el.getBoundingClientRect();
  return { x: r.left, y: r.top, w: r.width, h: r.height };
}

function inside(b: Box, x: number, y: number): boolean {
  return x >= b.x && x <= b.x + b.w && y >= b.y && y <= b.y + b.h;
}

/** Measure every pane, every empty side and the shelf in the window. */
export function measureTargets(doc: Document = document): Targets {
  const out: Targets = { leaves: [], empty: [], shelf: null };
  for (const area of doc.querySelectorAll<HTMLElement>('[data-tree]')) {
    const tree = area.dataset.tree as PaneTree;
    const panes = area.querySelectorAll<HTMLElement>(':scope > [data-leaf]');
    if (panes.length === 0) out.empty.push({ ...boxOf(area), tree });
    for (const pane of panes)
      out.leaves.push({ ...boxOf(pane), id: pane.dataset.leaf ?? '', tree });
  }
  const shelf = doc.querySelector('[data-arrange-shelf]');
  out.shelf = shelf ? boxOf(shelf) : null;
  return out;
}

/** The side of `box` a tile at `x`, `y` lands against. */
export function edgeIn(box: Box, x: number, y: number): DropEdge {
  // Two panes side by side each need the narrowest width a pane takes.
  if (box.w >= 2 * MIN_PANE_W + 1) {
    const fx = (x - box.x) / box.w;
    if (fx < SIDE) return 'left';
    if (fx > 1 - SIDE) return 'right';
  }
  return y < box.y + box.h / 2 ? 'before' : 'after';
}

/** Where a tile at `x`, `y` lands among `targets`, or null. */
export function targetAt(targets: Targets, x: number, y: number): ArrangeTarget | null {
  if (targets.shelf && inside(targets.shelf, x, y)) return { kind: 'shelf' };
  for (const pane of targets.leaves) {
    if (!inside(pane, x, y)) continue;
    return { kind: 'pane', drop: { tree: pane.tree, at: pane.id, edge: edgeIn(pane, x, y) } };
  }
  for (const side of targets.empty) {
    if (inside(side, x, y))
      return { kind: 'pane', drop: { tree: side.tree, at: null, edge: 'after' } };
  }
  return null;
}

// Controls on a tile keep their own presses.
const OWN_PRESS = 'button, select, input, textarea, a';

/** The press handler that lets a tile drag `leaf`. */
export function tileDrag(leaf: PaneLeaf): (e: ReactPointerEvent<HTMLElement>) => void {
  return (e) => {
    if (e.button !== 0 || !movable(leaf) || (e.target as Element).closest(OWN_PRESS)) return;
    e.preventDefault();
    const start = { x: e.clientX, y: e.clientY };
    let targets: Targets | null = null;
    let popEscape: (() => void) | null = null;
    const finish = (drop: boolean) => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
      window.removeEventListener('pointercancel', cancel);
      popEscape?.();
      if (!targets) return;
      document.body.classList.remove('is-arranging-drag');
      endDrag(drop);
    };
    const move = (ev: globalThis.PointerEvent) => {
      if (!targets) {
        if (Math.hypot(ev.clientX - start.x, ev.clientY - start.y) < THRESHOLD) return;
        // Draw the panes without the tile, then measure them.
        flushSync(() => startDrag(leaf, ev.clientX, ev.clientY));
        targets = measureTargets();
        document.body.classList.add('is-arranging-drag');
        popEscape = pushEscape(() => finish(false));
      }
      dragTo(ev.clientX, ev.clientY, targetAt(targets, ev.clientX, ev.clientY));
    };
    const up = () => finish(true);
    const cancel = () => finish(false);
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
    window.addEventListener('pointercancel', cancel);
  };
}
