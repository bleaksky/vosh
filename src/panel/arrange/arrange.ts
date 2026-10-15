import { useSyncExternalStore } from 'react';
import { createStore } from '../../stores/store';
import {
  WRITING_PANE,
  allPanes,
  bothTrees,
  liftLeaf,
  newLeaf,
  paneKey,
  paneRef,
  placeLeaf,
  treeIn,
  type PaneDrop,
  type PaneLayout,
  type PaneLeaf,
  type PaneTree,
} from '../paneLayout';
import { chatRefToAdd, readyForChat } from '../paneActions';
import { luaPaneRef, paneLabel, type LuaPaneOffer } from '../paneTypes';
import type { OfferedPaneType } from '../paneLayout';

// Arrange mode. The layout button in the title band, Arrange panes in a
// pane's menu, the View menu and the palette turn it on. While it is on,
// every pane is a tile you pick up anywhere and drop against another
// pane, on any of its four sides, or into an empty sidebar. The panes
// make room as you drag, so you see the layout you would get before you
// let go. The terminal's column turns into a card with the shelf, which
// holds every pane neither side shows: the ones you put away and the
// ones you could add. Nothing saves until Done, which saves the layout
// for the profile in front and keeps the one it replaced among the
// earlier layouts. Cancel and Esc put back the layout you started from.
//
// This file holds the state and every move as plain functions over it,
// so the tests run them without a window. ArrangeTile.tsx, ArrangeCard.tsx
// and arrangeDrag.ts draw it and feed it the pointer and the keys.

/** A pane on the shelf you put away while arranging, with the side it
 *  left, so putting it back takes it home. */
export interface PutAway {
  leaf: PaneLeaf;
  from: PaneTree;
}

/** Where a drag would land now: against a pane, or on the shelf. */
export type ArrangeTarget = { kind: 'pane'; drop: PaneDrop } | { kind: 'shelf' };

/** A tile on its way somewhere, lifted from where it was. Until
 *  `measured`, the window draws the layout without it for arrangeDrag.ts
 *  to measure the targets on. */
export interface ArrangeDrag {
  leaf: PaneLeaf;
  lifted: Lifted;
  measured: boolean;
  target: ArrangeTarget | null;
  x: number;
  y: number;
}

interface Step {
  draft: PaneLayout;
  putAway: PutAway[];
}

/** Arrange mode while it is on. */
export interface Arranging {
  /** The profile whose panes these are, which Done saves to. */
  profile: string;
  /** The layout as arrange mode found it, which Cancel puts back. */
  entered: PaneLayout;
  draft: PaneLayout;
  putAway: PutAway[];
  undo: Step[];
  drag: ArrangeDrag | null;
}

const store = createStore<Arranging | null>(null);

export function getArranging(): Arranging | null {
  return store.get();
}

export function useArranging(): Arranging | null {
  return useSyncExternalStore(store.subscribe, store.get, store.get);
}

export function subscribeArranging(cb: () => void): () => void {
  return store.subscribe(cb);
}

/** Turn arrange mode on over `layout`, the panes of `profile`. The panel
 *  shows while you arrange, whatever it did before. */
export function startArranging(layout: PaneLayout, profile: string): void {
  if (store.get()) return;
  store.set({
    profile,
    entered: layout,
    draft: { ...layout, panel_open: true },
    putAway: [],
    undo: [],
    drag: null,
  });
}

/** Turn arrange mode off and hand back what it held, for Done to save or
 *  Cancel to drop. */
export function stopArranging(): Arranging | null {
  const a = store.get();
  store.set(null);
  return a;
}

/** Make `next` the draft, with `putAway` the shelf's own panes, keeping
 *  the step before for Undo. Nothing happens when nothing changes. */
function commit(a: Arranging, next: PaneLayout, putAway: PutAway[]): void {
  if (sameLayout(next, a.draft) && sameAway(putAway, a.putAway)) {
    store.set({ ...a, drag: null });
    return;
  }
  store.set({
    ...a,
    draft: next,
    putAway,
    undo: [...a.undo, { draft: a.draft, putAway: a.putAway }],
    drag: null,
  });
}

function sameLayout(a: PaneLayout, b: PaneLayout): boolean {
  return JSON.stringify([a.root, a.sidebar]) === JSON.stringify([b.root, b.sidebar]);
}

function sameAway(a: PutAway[], b: PutAway[]): boolean {
  return a.length === b.length && a.every((p, i) => p.leaf.id === b[i].leaf.id);
}

/** Go back one step. */
export function undoArranging(): void {
  const a = store.get();
  const last = a?.undo.at(-1);
  if (!a || !last) return;
  store.set({ ...a, ...last, undo: a.undo.slice(0, -1), drag: null });
}

/** Start again from `layout`, such as the default or an earlier one,
 *  as one step Undo takes back. */
export function arrangeFrom(layout: PaneLayout): void {
  const a = store.get();
  if (!a) return;
  commit(a, { ...a.draft, root: layout.root, sidebar: layout.sidebar }, []);
}

// ---------------------------------------------------------------
// The shelf.
// ---------------------------------------------------------------

/** A tile on the shelf: a pane you put away, or one you could add. */
export interface ShelfItem {
  leaf: PaneLeaf;
  label: string;
  /** Put away while arranging, rather than one Vosh offers. */
  away: boolean;
}

/** What the session offers to add, which the shelf lists after the
 *  panes you put away. */
export interface Offers {
  builtIns: OfferedPaneType[];
  lua: LuaPaneOffer[];
}

/** Every pane on the shelf: the ones you put away, then each pane the
 *  session offers that neither side shows nor the shelf holds, and one
 *  more Chat while there is room for it. */
export function shelfItems(layout: PaneLayout, putAway: PutAway[], offers: Offers): ShelfItem[] {
  const items: ShelfItem[] = putAway.map(({ leaf }) => ({
    leaf,
    label: paneLabel(leaf),
    away: true,
  }));
  const held = new Set([...allPanes(bothTrees(layout)), ...putAway.map((p) => paneKey(p.leaf))]);
  // Ids for the new leaves, unique across both trees and the shelf.
  let room: PaneLayout = {
    ...layout,
    sidebar: {
      ...layout.sidebar,
      children: [...layout.sidebar.children, ...putAway.map((p) => p.leaf)],
    },
  };
  const add = (leaf: PaneLeaf, label: string) => {
    items.push({ leaf, label, away: false });
    room = { ...room, sidebar: { ...room.sidebar, children: [...room.sidebar.children, leaf] } };
  };
  const shownChats = allPanes(bothTrees(layout)).filter((k) => k === 'chat').length;
  for (const t of offers.builtIns) {
    if (t === 'chat') {
      // A Chat you put away waits on the shelf already, so the shelf
      // offers no new one beside it.
      if (putAway.some((p) => p.leaf.pane === 'chat')) continue;
      add(newLeaf(room, chatRefToAdd(bothTrees(layout))), shownChats > 0 ? 'Another Chat' : 'Chat');
      continue;
    }
    if (held.has(t)) continue;
    add(newLeaf(room, paneRef(t)), paneLabel(paneRef(t)));
  }
  for (const offer of offers.lua) {
    const ref = luaPaneRef(offer);
    if (held.has(paneKey(ref))) continue;
    add(newLeaf(room, ref), paneLabel(ref));
  }
  return items;
}

// ---------------------------------------------------------------
// Moves.
// ---------------------------------------------------------------

/** Whether a tile can move: every pane but the writing card, which its
 *  pin keeps in the panel. */
export function movable(leaf: PaneLeaf): boolean {
  return leaf.pane !== WRITING_PANE;
}

/** A tile lifted from where it was: the layout and the shelf without
 *  it, the side it left or the shelf, and on the shelf the entry it had
 *  if you put it away, which a pane the session offers has not. */
interface Lifted {
  /** The leaf as the draft or the shelf holds it, its fold with it. */
  leaf: PaneLeaf;
  base: PaneLayout;
  away: PutAway[];
  from: PaneTree | 'shelf';
  home: PutAway | null;
}

/** Take `leaf` out of the draft or off the shelf, ready to land. */
function lift(a: Arranging, leaf: PaneLeaf): Lifted {
  const lifted = liftLeaf(a.draft, leaf.id);
  if (lifted) {
    return {
      leaf: lifted.leaf,
      base: lifted.layout,
      away: a.putAway,
      from: lifted.from,
      home: null,
    };
  }
  const home = a.putAway.find((p) => p.leaf.id === leaf.id) ?? null;
  return {
    leaf: home?.leaf ?? leaf,
    base: a.draft,
    away: a.putAway.filter((p) => p.leaf.id !== leaf.id),
    from: 'shelf',
    home,
  };
}

/** Where the leaf `l` lifted would be with `target`: the layout and the
 *  shelf. */
export function landed(
  l: Lifted,
  target: ArrangeTarget,
): { layout: PaneLayout; putAway: PutAway[] } {
  const { leaf } = l;
  if (target.kind === 'shelf') {
    // A pane the session offered and you never placed needs no tile of
    // its own, since the shelf offers it again.
    if (l.from === 'shelf')
      return { layout: l.base, putAway: l.home ? [...l.away, l.home] : l.away };
    return { layout: l.base, putAway: [...l.away, { leaf, from: l.from }] };
  }
  // A new Chat pane joins as one from the palette does, so the pane on
  // All turns to Everything else.
  const fresh = l.from === 'shelf' && !l.home && leaf.pane === 'chat';
  const base = fresh ? readyForChat(l.base).layout : l.base;
  return { layout: placeLeaf(base, { ...leaf, weight: 1 }, target.drop), putAway: l.away };
}

/** Move `leaf` to `target` as one step. */
export function moveTo(leaf: PaneLeaf, target: ArrangeTarget): void {
  const a = store.get();
  if (!a || !movable(leaf)) return;
  const lifted = lift(a, leaf);
  if (lifted.from === 'shelf' && target.kind === 'shelf') return;
  const next = landed(lifted, target);
  commit(a, next.layout, next.putAway);
}

/** Put `leaf` away on the shelf. */
export function putAway(leaf: PaneLeaf): void {
  moveTo(leaf, { kind: 'shelf' });
}

/** Take `item` off the shelf to the foot of the side it left, or of the
 *  panel for one it never showed on. */
export function putBack(item: ShelfItem): void {
  const a = store.get();
  if (!a) return;
  const home = a.putAway.find((p) => p.leaf.id === item.leaf.id)?.from ?? 'panel';
  moveTo(item.leaf, { kind: 'pane', drop: { tree: home, at: null, edge: 'after' } });
}

/** The side a key moves a pane to, for the panel on `panelSide`: the
 *  arrow toward the other column, never to a sidebar the window has no
 *  room to show. */
function across(
  from: PaneTree,
  key: string,
  panelSide: 'left' | 'right',
  sidebarShows: boolean,
): PaneTree | null {
  const leftTree: PaneTree = panelSide === 'left' ? 'panel' : 'sidebar';
  const rightTree: PaneTree = leftTree === 'panel' ? 'sidebar' : 'panel';
  const to =
    key === 'ArrowLeft' && from !== leftTree
      ? leftTree
      : key === 'ArrowRight' && from === leftTree
        ? rightTree
        : null;
  return to === 'sidebar' && !sidebarShows ? null : to;
}

/** What a key does to the tile of `leaf`: Up and Down move it past the
 *  pane before or after it on its side, the arrow toward the other
 *  column moves it to that side's foot, and Delete or Backspace puts it
 *  away. A tile on the shelf goes back with Enter or Space. Null for a
 *  key that moves nothing. */
export function keyTarget(
  a: Pick<Arranging, 'draft'>,
  leaf: PaneLeaf,
  key: string,
  panelSide: 'left' | 'right',
  sidebarShows = true,
): ArrangeTarget | 'back' | null {
  const lifted = liftLeaf(a.draft, leaf.id);
  if (!lifted) return key === 'Enter' || key === ' ' ? 'back' : null;
  if (key === 'Delete' || key === 'Backspace') return { kind: 'shelf' };
  const { from } = lifted;
  const order = allLeafIds(treeIn(a.draft, from));
  const i = order.indexOf(leaf.id);
  if (key === 'ArrowUp' && i > 0) {
    return { kind: 'pane', drop: { tree: from, at: order[i - 1], edge: 'before' } };
  }
  if (key === 'ArrowDown' && i < order.length - 1) {
    return { kind: 'pane', drop: { tree: from, at: order[i + 1], edge: 'after' } };
  }
  const to = across(from, key, panelSide, sidebarShows);
  if (to) return { kind: 'pane', drop: { tree: to, at: null, edge: 'after' } };
  return null;
}

function allLeafIds(tree: PaneLayout['root']): string[] {
  const out: string[] = [];
  const walk = (node: PaneLayout['root'] | PaneLeaf) => {
    if ('pane' in node) out.push(node.id);
    else node.children.forEach(walk);
  };
  walk(tree);
  return out;
}

// ---------------------------------------------------------------
// Dragging.
// ---------------------------------------------------------------

/** Pick up `leaf` at `x`, `y`. The panes close over the space it left,
 *  for arrangeDrag.ts to measure the targets on. */
export function startDrag(leaf: PaneLeaf, x: number, y: number): void {
  const a = store.get();
  if (!a || a.drag || !movable(leaf)) return;
  store.set({ ...a, drag: { leaf, lifted: lift(a, leaf), measured: false, target: null, x, y } });
}

/** The pointer is at `x`, `y` over `target`, measured on the layout
 *  without the tile. */
export function dragTo(x: number, y: number, target: ArrangeTarget | null): void {
  const a = store.get();
  if (!a?.drag) return;
  store.set({ ...a, drag: { ...a.drag, measured: true, x, y, target } });
}

/** Let go: land where the drag points, or put the tile back with no
 *  target. */
export function endDrag(drop: boolean): void {
  const a = store.get();
  if (!a?.drag) return;
  const { lifted, target } = a.drag;
  if (!drop || !target || (lifted.from === 'shelf' && target.kind === 'shelf')) {
    store.set({ ...a, drag: null });
    return;
  }
  const next = landed(lifted, target);
  commit(a, next.layout, next.putAway);
}

/** What the window shows now: the draft and the shelf's own panes, or
 *  during a drag where the tile would land, with `slot` the tile the
 *  drag holds. Before the pointer finds a target, the tile stays where
 *  it was. */
export function arrangeView(a: Arranging): {
  layout: PaneLayout;
  putAway: PutAway[];
  slot: string | null;
} {
  const d = a.drag;
  if (!d) return { layout: a.draft, putAway: a.putAway, slot: null };
  if (!d.measured) return { layout: d.lifted.base, putAway: d.lifted.away, slot: null };
  if (!d.target) return { layout: a.draft, putAway: a.putAway, slot: d.leaf.id };
  const next = landed(d.lifted, d.target);
  return { ...next, slot: d.leaf.id };
}
