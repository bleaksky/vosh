import { useMemo, useSyncExternalStore } from 'react';
import {
  PANEL_WIDTH_MAX,
  PANEL_WIDTH_MIN,
  allPanes,
  defaultLayout,
  getPaneLayout,
  setPaneLayout,
  subscribePaneLayout,
  type PaneLayout,
  type PaneSplit,
  type PaneType,
} from './paneLayout';

// The one copy of the active profile's pane layout this window edits.
// The panel changes the tree and the title band opens, closes, and
// sizes the panel, and both write the same saved object. Each keeping
// its own copy would let one write a stale half over the other, so
// every edit goes through updatePanelLayout here.
//
// The store loads once, on first use, and follows layouts that change
// elsewhere (a profile switch). Local edits show at once and reach the
// backend through setPaneLayout, which debounces splitter drags.

/** Stock panel width in CSS pixels, the --panel-w token. */
export const PANEL_WIDTH_DEFAULT = 300;

let layout: PaneLayout | null = null;
let started = false;
const listeners = new Set<() => void>();

function publish(next: PaneLayout): void {
  if (next === layout) return;
  layout = next;
  for (const cb of listeners) cb();
}

export function startPanelLayoutStore(): void {
  if (started) return;
  started = true;
  // A tree from a profile switch can land before the first read
  // answers. The switch is newer, so the read then stands down.
  let heard = false;
  subscribePaneLayout((next) => {
    heard = true;
    publish(next);
  }).catch((e: unknown) => console.error('[panel] layout subscribe failed', e));
  getPaneLayout()
    .then((first) => {
      if (!heard && layout === null) publish(first);
    })
    .catch((e: unknown) => {
      console.error('[panel] layout load failed', e);
      if (layout === null) publish(defaultLayout());
    });
}

/** The layout, or null until the first read answers. */
export function getPanelLayout(): PaneLayout | null {
  return layout;
}

export function subscribePanelLayout(cb: () => void): () => void {
  startPanelLayoutStore();
  listeners.add(cb);
  return () => {
    listeners.delete(cb);
  };
}

export function usePanelLayout(): PaneLayout | null {
  return useSyncExternalStore(subscribePanelLayout, getPanelLayout);
}

/** Apply `fn` to the current layout, show the result, and save it.
 *  Does nothing before the first read answers, so an early click
 *  cannot overwrite the saved tree with the default one. */
export function updatePanelLayout(fn: (current: PaneLayout) => PaneLayout): void {
  if (layout === null) return;
  const next = fn(layout);
  if (next === layout) return;
  publish(next);
  setPaneLayout(next);
}

/** Replace the pane tree, keeping the panel's open state and width. */
export function setPaneTree(root: PaneSplit): void {
  updatePanelLayout((l) => (l.root === root ? l : { ...l, root }));
}

/** Whether the panel shows. True until the layout loads, which is
 *  the stock state, so the first paint does not jump. */
export function usePanelOpen(): boolean {
  return usePanelLayout()?.panel_open ?? true;
}

/** Pane types the panel shows, in reading order. Empty until the
 *  layout loads. */
export function useShownPanes(): PaneType[] {
  const root = usePanelLayout()?.root;
  return useMemo(() => (root ? allPanes(root) : []), [root]);
}

export function setPanelOpen(open: boolean): void {
  updatePanelLayout((l) => (l.panel_open === open ? l : { ...l, panel_open: open }));
}

export function togglePanelOpen(): void {
  updatePanelLayout((l) => ({ ...l, panel_open: !l.panel_open }));
}

/** The panel width in CSS pixels, the stock 300 when none is saved. */
export function panelWidthOf(l: PaneLayout | null): number {
  return l?.panel_width ?? PANEL_WIDTH_DEFAULT;
}

/** Save a panel width, clamped to the panel bounds. Null, or the
 *  stock width, goes back to following the default. */
export function setPanelWidth(px: number | null): void {
  const width =
    px === null ? null : Math.round(Math.min(PANEL_WIDTH_MAX, Math.max(PANEL_WIDTH_MIN, px)));
  const saved = width === PANEL_WIDTH_DEFAULT ? null : width;
  updatePanelLayout((l) => (l.panel_width === saved ? l : { ...l, panel_width: saved }));
}
