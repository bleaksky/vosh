import { useSyncExternalStore, type PointerEvent } from 'react';
import { createStore } from '../stores/store';
import { dropPane } from './paneActions';
import type { PaneDrop, PaneLeaf, PaneTree } from './paneLayout';
import { WRITING_PANE } from './paneLayout';
import { paneLabel } from './paneTypes';

// Moving a pane by dragging its header, within the panel or the
// sidebar or from one to the other. A press on a header that moves 4 px
// starts the drag. Under the pointer, the half of a pane it would land
// against washes in the accent with a 2 px line along that edge, and an
// empty sidebar washes whole. Letting go there moves the pane, keeping
// its id and props. Esc, a press that never moved, or letting go
// anywhere else leaves it where it was. The writing card stays put.

/** A box in window pixels. */
export interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** Where a drag would land now, and what draws it. */
export interface DropView {
  drop: PaneDrop;
  /** The part of the target washed in the accent. */
  wash: Box;
  /** The 2 px line along the edge the pane joins. */
  line: Box;
}

/** A drag under way. */
export interface PaneDrag {
  id: string;
  /** The pane's name, which the ghost under the pointer shows. */
  label: string;
  x: number;
  y: number;
  target: DropView | null;
}

const store = createStore<PaneDrag | null>(null);

export function usePaneDrag(): PaneDrag | null {
  return useSyncExternalStore(store.subscribe, store.get);
}

/** How far a press moves before it becomes a drag. */
const THRESHOLD = 4;

/** The edge of `box` a pane dropped at height `y` joins. */
export function edgeAt(y: number, box: Box): 'before' | 'after' {
  return y < box.y + box.h / 2 ? 'before' : 'after';
}

/** What draws a drop against `box` at `edge`: its half on that side
 *  washed, and a 2 px line along that edge. The line above sits on the
 *  seam with the pane before it, and the line below stays inside the
 *  pane, so it shows at the window's bottom edge too. */
export function dropBoxes(box: Box, edge: 'before' | 'after'): Pick<DropView, 'wash' | 'line'> {
  const half = box.h / 2;
  const top = edge === 'before' ? box.y : box.y + half;
  return {
    wash: { x: box.x, y: top, w: box.w, h: half },
    line: { x: box.x, y: edge === 'before' ? box.y - 1 : box.y + box.h - 2, w: box.w, h: 2 },
  };
}

function boxOf(el: Element): Box {
  const r = el.getBoundingClientRect();
  return { x: r.left, y: r.top, w: r.width, h: r.height };
}

/** Where a pane dragged from `dragging` lands at `x`, `y`: against the
 *  pane under the pointer, or alone in an empty tree, or nowhere. */
export function dropAt(x: number, y: number, dragging: string): DropView | null {
  const el = typeof document === 'undefined' ? null : document.elementFromPoint(x, y);
  const area = el?.closest<HTMLElement>('[data-tree]');
  if (!el || !area) return null;
  const tree = area.dataset.tree as PaneTree;
  const pane = el.closest<HTMLElement>('[data-leaf]');
  if (pane && area.contains(pane)) {
    const at = pane.dataset.leaf ?? '';
    if (at === dragging) return null;
    const box = boxOf(pane);
    const edge = edgeAt(y, box);
    return { drop: { tree, at, edge }, ...dropBoxes(box, edge) };
  }
  if (area.querySelector('[data-leaf]') !== null) return null;
  const box = boxOf(area);
  return {
    drop: { tree, at: null, edge: 'after' },
    wash: box,
    line: { x: box.x, y: box.y - 1, w: box.w, h: 2 },
  };
}

// Controls inside a header keep their own presses.
const OWN_PRESS = 'button, select, input, textarea, a, [role="menu"]';

/** The pointer handlers that let a pane's header drag `leaf`. */
export function headerDrag(leaf: PaneLeaf | null): {
  onPointerDown?: (e: PointerEvent<HTMLElement>) => void;
} {
  if (!leaf || leaf.pane === WRITING_PANE) return {};
  return {
    onPointerDown: (e) => {
      if (e.button !== 0 || (e.target as Element).closest(OWN_PRESS)) return;
      const header = e.currentTarget;
      const start = { x: e.clientX, y: e.clientY };
      let dragging = false;
      const move = (ev: globalThis.PointerEvent) => {
        if (!dragging) {
          if (Math.hypot(ev.clientX - start.x, ev.clientY - start.y) < THRESHOLD) return;
          dragging = true;
          header.setPointerCapture(ev.pointerId);
          document.body.classList.add('is-dragging-pane');
          markDragged(leaf.id, true);
        }
        store.set({
          id: leaf.id,
          label: paneLabel(leaf),
          x: ev.clientX,
          y: ev.clientY,
          target: dropAt(ev.clientX, ev.clientY, leaf.id),
        });
      };
      const end = (drop: boolean) => {
        window.removeEventListener('pointermove', move);
        window.removeEventListener('pointerup', up);
        window.removeEventListener('pointercancel', cancel);
        window.removeEventListener('keydown', key, true);
        const target = store.get()?.target ?? null;
        store.set(null);
        if (!dragging) return;
        document.body.classList.remove('is-dragging-pane');
        markDragged(leaf.id, false);
        if (drop && target) dropPane(leaf.id, target.drop);
      };
      const up = () => end(true);
      const cancel = () => end(false);
      const key = (ev: KeyboardEvent) => {
        if (ev.key !== 'Escape' || !dragging) return;
        ev.preventDefault();
        ev.stopPropagation();
        end(false);
      };
      window.addEventListener('pointermove', move);
      window.addEventListener('pointerup', up);
      window.addEventListener('pointercancel', cancel);
      window.addEventListener('keydown', key, true);
    },
  };
}

// The pane on its way fades where it was.
function markDragged(id: string, on: boolean): void {
  const section = document.querySelector(`[data-leaf="${CSS.escape(id)}"]`);
  section?.classList.toggle('is-dragged', on);
}
