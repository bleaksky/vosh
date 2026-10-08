import { useSyncExternalStore } from 'react';
import {
  WRITING_PANE,
  addPane,
  closePane,
  leafIdFor,
  paneRef,
  type PaneLayout,
} from '../panel/paneLayout';
import { updatePanelLayout } from '../panel/panelLayoutStore';
import { createStore } from '../stores/store';

// The writing card pinned to the panel. The pin adds a Writing pane to
// the tree, the panel draws an empty slot in it, and the card moves its
// own box into the slot, so nothing it holds starts over. Unpinning or
// closing the card takes the pane out again, and a pane left in a saved
// tree with no card open goes at the next look. Add a pane, the View
// menu and the palette never offer it.

const WRITING_REF = paneRef(WRITING_PANE);

/** `layout` with the Writing pane in it while `want` holds, and out of
 *  it otherwise. A pane the pin adds goes at the foot of the panel with
 *  a share as tall as six rows of text and the card's own chrome, and
 *  shows the panel. Returns `layout` itself when nothing changes. */
export function withWritingPane(layout: PaneLayout, want: boolean): PaneLayout {
  const id = leafIdFor(layout.root, WRITING_REF);
  if (want && id === null) {
    return { ...layout, panel_open: true, root: addPane(layout.root, WRITING_REF) };
  }
  if (!want && id !== null) return { ...layout, root: closePane(layout.root, id) };
  return layout;
}

/** Put the Writing pane in the panel or take it out. Does nothing
 *  before the panel's layout loads. */
export function keepWritingPane(want: boolean): void {
  updatePanelLayout((l) => withWritingPane(l, want));
}

/** Pin the card: the Writing pane goes in and the panel shows, even
 *  when the pane is already there under a hidden panel. */
export function pinWritingPane(): void {
  updatePanelLayout((l) => {
    const next = withWritingPane(l, true);
    return next.panel_open ? next : { ...next, panel_open: true };
  });
}

// The slot the Writing pane draws, which the pinned card fills.
const slot = createStore<HTMLElement | null>(null);

/** The Writing pane hands over its slot as it mounts, and null as it
 *  goes. */
export function setWritingSlot(el: HTMLElement | null): void {
  slot.set(el);
}

/** The slot the pinned card fills, or null while no Writing pane shows. */
export function useWritingSlot(): HTMLElement | null {
  return useSyncExternalStore(slot.subscribe, slot.get);
}
