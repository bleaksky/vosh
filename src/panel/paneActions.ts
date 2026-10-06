import { createContext, useContext } from 'react';
import {
  addPane,
  closePane,
  isLeaf,
  leafIdFor,
  paneRef,
  replacePane,
  sanitize,
  splitPane,
  type PaneLeaf,
  type PaneNode,
  type PaneRef,
  type PaneSplit,
  type PaneType,
  type SplitDir,
} from './paneLayout';
import { getPanelLayout, setPaneTree, updatePanelLayout } from './panelLayoutStore';
import { paneTypesToAdd } from './paneTypes';

// What a pane's header and menu, the palette and the title band can do
// to the panel. Each action reads the store's current tree when it
// runs, so a menu that stayed open across a profile switch or a drag
// never writes back a stale copy.

/** The leaf a pane renders, handed down by PanelHost. */
export const PaneLeafContext = createContext<PaneLeaf | null>(null);

export function usePaneLeaf(): PaneLeaf | null {
  return useContext(PaneLeafContext);
}

/** The pane a split adds: the first one the panel does not show. */
export function paneToSplitIn(): PaneRef | null {
  const pane = paneTypesToAdd(getPanelLayout()?.root ?? null)[0];
  return pane === undefined ? null : paneRef(pane);
}

export function splitHere(id: string, dir: SplitDir): void {
  const root = getPanelLayout()?.root;
  const pane = paneToSplitIn();
  if (!root || !pane) return;
  setPaneTree(splitPane(root, id, dir, pane));
}

export function showHereInstead(id: string, ref: PaneRef): void {
  const root = getPanelLayout()?.root;
  if (root) setPaneTree(replacePane(root, id, ref));
}

export function closeHere(id: string): void {
  const root = getPanelLayout()?.root;
  if (root) setPaneTree(closePane(root, id));
}

/** Show or hide one pane type from the palette. Showing opens the
 *  panel too, and a pane already there under a hidden panel just
 *  comes back with it. */
export function togglePane(pane: PaneType): void {
  updatePanelLayout((l) => {
    const ref = paneRef(pane);
    const leaf = leafIdFor(l.root, ref);
    if (leaf !== null && l.panel_open) return { ...l, root: closePane(l.root, leaf) };
    return { ...l, panel_open: true, root: leaf !== null ? l.root : addPane(l.root, ref) };
  });
}

/** Add a pane from the title band, at the bottom of the panel. */
export function addPaneType(pane: PaneType): void {
  updatePanelLayout((l) => ({ ...l, panel_open: true, root: addPane(l.root, paneRef(pane)) }));
}

/** Merge `patch` into the props of leaf `id`. An empty string value
 *  removes that key. Returns the input tree when nothing changes. */
export function setLeafProps(
  tree: PaneSplit,
  id: string,
  patch: Record<string, string>,
): PaneSplit {
  let changed = false;
  const visit = (node: PaneNode): PaneNode => {
    if (!isLeaf(node)) {
      const children = node.children.map(visit);
      return children.some((c, i) => c !== node.children[i]) ? { ...node, children } : node;
    }
    if (node.id !== id) return node;
    const props = { ...node.props };
    for (const [key, value] of Object.entries(patch)) {
      if (value === '') delete props[key];
      else props[key] = value;
    }
    const keys = new Set([...Object.keys(props), ...Object.keys(node.props)]);
    if ([...keys].every((k) => props[k] === node.props[k])) return node;
    changed = true;
    return { ...node, props };
  };
  const next = visit(tree);
  return changed && !isLeaf(next) ? sanitize(next) : tree;
}

export function updateLeafProps(id: string, patch: Record<string, string>): void {
  const root = getPanelLayout()?.root;
  if (root) setPaneTree(setLeafProps(root, id, patch));
}

/** Hand the caret back to the command line, the way the title band's
 *  menus do when they close. MainWindow listens for this event. */
export function returnToCommandLine(): void {
  window.dispatchEvent(new CustomEvent('vosh:focus-input'));
}
