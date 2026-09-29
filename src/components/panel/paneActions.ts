import { createContext, useContext } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import {
  addPane,
  closePane,
  isLeaf,
  replacePane,
  sanitize,
  splitPane,
  type PaneLeaf,
  type PaneNode,
  type PaneSplit,
  type PaneType,
  type SplitDir,
} from '../../lib/paneLayout';
import { getPanelLayout, setPaneTree, setPanelOpen } from './panelLayoutStore';
import { paneTypesToAdd } from './paneTypes';

// What a pane's header and menu can do to the tree. Each action reads
// the store's current tree when it runs, so a menu that stayed open
// across a profile switch or a drag never writes back a stale copy.

/** The leaf a pane renders, handed down by PanelHost. */
export const PaneLeafContext = createContext<PaneLeaf | null>(null);

export function usePaneLeaf(): PaneLeaf | null {
  return useContext(PaneLeafContext);
}

/** The pane a split adds: the first one the panel does not show. */
export function paneToSplitIn(): PaneType | null {
  return paneTypesToAdd(getPanelLayout()?.root ?? null)[0] ?? null;
}

export function splitHere(id: string, dir: SplitDir): void {
  const root = getPanelLayout()?.root;
  const pane = paneToSplitIn();
  if (!root || !pane) return;
  setPaneTree(splitPane(root, id, dir, pane));
}

export function showHereInstead(id: string, pane: PaneType): void {
  const root = getPanelLayout()?.root;
  if (root) setPaneTree(replacePane(root, id, pane));
}

/** Add `pane` at the bottom of the panel (the title band's Add a
 *  pane), and open the panel if it is hidden. */
export function addPaneToPanel(pane: PaneType): void {
  const layout = getPanelLayout();
  if (!layout) return;
  setPaneTree(addPane(layout.root, pane));
  if (!layout.panel_open) setPanelOpen(true);
}

export function closeHere(id: string): void {
  const root = getPanelLayout()?.root;
  if (root) setPaneTree(closePane(root, id));
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

/** Open Settings on `tab`. The window may not exist yet, so the tab
 *  travels twice, the way the command palette sends it: through
 *  localStorage for a cold open and an event for a window already up. */
export function openSettingsTab(tab: string): void {
  try {
    localStorage.setItem('vosh.settings.pendingTab', tab);
  } catch {
    // Storage unavailable. The event still reaches an open window.
  }
  void emit('vosh://settings-goto-tab', tab);
  invoke('open_settings_window').catch((e: unknown) => {
    console.error('[panel] open_settings_window failed', e);
  });
}
