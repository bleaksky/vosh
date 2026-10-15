import { createContext, useContext } from 'react';
import {
  addPane,
  bothTrees,
  cleanTrees,
  closePane,
  countPanes,
  isLeaf,
  leafIdFor,
  moveLeaf,
  moveToOtherTree,
  otherTree,
  paneCap,
  paneRef,
  replacePane,
  sanitize,
  splitPane,
  treeIn,
  treeOf,
  withTree,
  type PaneDrop,
  type PaneLayout,
  type PaneLeaf,
  type PaneNode,
  type PaneRef,
  type PaneSplit,
  type PaneTree,
  type PaneType,
  type SplitDir,
} from './paneLayout';
import { getPanelLayout, updatePanelLayout } from './panelLayoutStore';
import { luaPaneRef, luaPanesToAdd, paneTypesToAdd } from './paneTypes';
import { chatFilterOf, chatFilterProps, chatLeaves, checkedChannels } from './chat/chatFilter';
import { pushToast } from '../stores/toasts';

// What a pane's header and menu, the palette and the title band can do
// to the panel and the sidebar. Each action reads the store's current
// layout when it runs, so a menu that stayed open across a profile
// switch or a drag never writes back a stale copy. An action on a pane
// finds the tree that holds it by its id, which is unique across both.

/** The leaf a pane renders, handed down by PanelHost. */
export const PaneLeafContext = createContext<PaneLeaf | null>(null);

export function usePaneLeaf(): PaneLeaf | null {
  return useContext(PaneLeafContext);
}

/** The pane a split of leaf `id` adds. On a Chat pane it is another
 *  Chat while there is room. Elsewhere it is the first pane neither
 *  the panel nor the sidebar shows, a built-in pane first and then a
 *  Lua pane, and another Chat once they show them all. */
export function paneToSplitIn(id: string): PaneRef | null {
  const layout = getPanelLayout();
  const tree = layout ? bothTrees(layout) : null;
  const types = paneTypesToAdd(tree);
  const chat = types.includes('chat');
  if (chat && chatLeaves(tree).some((leaf) => leaf.id === id)) return paneRef('chat');
  const fresh = types.find((t) => tree === null || countPanes(tree, paneRef(t)) === 0);
  if (fresh !== undefined) return paneRef(fresh);
  const lua = luaPanesToAdd(tree)[0];
  if (lua !== undefined) return luaPaneRef(lua);
  return chat ? paneRef('chat') : null;
}

/** The Chat pane that turns to Everything else as another Chat pane
 *  joins: the first that says rest, else the first with no channels
 *  checked, which shows All while it is alone. */
function restCandidate(tree: PaneNode | null): PaneLeaf | undefined {
  const chats = chatLeaves(tree);
  return (
    chats.find((leaf) => chatFilterOf(leaf).kind === 'rest') ??
    chats.find((leaf) => chatFilterOf(leaf).kind === 'all')
  );
}

/** What a new Chat pane starts on. The first shows All. Another starts
 *  on tell while no Chat pane checks tell, so tells land in a pane of
 *  their own, then on Everything else while no pane shows it, and with
 *  no channels otherwise, for you to pick. */
export function chatRefToAdd(tree: PaneNode | null): PaneRef {
  const chats = chatLeaves(tree);
  if (chats.length === 0) return paneRef('chat');
  const onTell = chats.some((leaf) => checkedChannels(leaf).includes('tell'));
  if (!onTell) return { pane: 'chat', props: { channel: 'tell' } };
  return restCandidate(tree) ? paneRef('chat') : { pane: 'chat', props: { rest: '1' } };
}

/** Merge `patch` into the props of leaf `id`, in whichever tree holds
 *  it. */
function setLayoutLeafProps(
  layout: PaneLayout,
  id: string,
  patch: Record<string, string>,
): PaneLayout {
  const which = treeOf(layout, id);
  if (which === null) return layout;
  return withTree(layout, which, setLeafProps(treeIn(layout, which), id, patch));
}

// Place `ref` in the tree `which` of `layout` with `put`. A pane the
// two trees hold once leaves the other tree, so placing it moves it,
// and a Chat pane needs room across both. A new Chat pane starts on
// what chatRefToAdd gives. The pane that shows Everything else from
// then on says rest in the same write, so the new pane never takes it,
// and when that pane was alone on All, a toast says it now shows
// Everything else, since All is not offered beside another Chat pane.
function placeIn(
  layout: PaneLayout,
  which: PaneTree,
  ref: PaneRef,
  put: (tree: PaneSplit, ref: PaneRef) => PaneSplit,
): PaneLayout {
  let base = layout;
  if (paneCap(ref) === 1) {
    const other = otherTree(which);
    const there = leafIdFor(treeIn(layout, other), ref);
    if (there !== null) base = withTree(layout, other, closePane(treeIn(layout, other), there));
  } else if (countPanes(bothTrees(layout), ref) >= paneCap(ref)) {
    return layout;
  }
  if (ref.pane !== 'chat') {
    const tree = put(treeIn(base, which), ref);
    return tree === treeIn(base, which) ? layout : cleanTrees(withTree(base, which, tree));
  }
  const all = bothTrees(base);
  const chat = chatRefToAdd(all);
  const rest = chatLeaves(all).length > 0 ? restCandidate(all) : undefined;
  const ready = rest ? setLayoutLeafProps(base, rest.id, chatFilterProps({ kind: 'rest' })) : base;
  const tree = put(treeIn(ready, which), chat);
  if (tree === treeIn(ready, which)) return layout;
  if (rest && chatLeaves(all).length === 1) {
    pushToast({ kind: 'info', message: 'Your other Chat pane now shows Everything else.' });
  }
  return cleanTrees(withTree(ready, which, tree));
}

// A Chat pane left alone shows All, whatever it showed beside the
// others.
function loneChatToAll(before: PaneLayout, next: PaneLayout): PaneLayout {
  const chats = chatLeaves(bothTrees(next));
  if (chats.length !== 1 || chatLeaves(bothTrees(before)).length < 2) return next;
  return setLayoutLeafProps(next, chats[0].id, chatFilterProps({ kind: 'all' }));
}

export function splitHere(id: string, dir: SplitDir): void {
  const pane = paneToSplitIn(id);
  if (!pane) return;
  updatePanelLayout((l) => {
    const which = treeOf(l, id);
    if (which === null) return l;
    return placeIn(l, which, pane, (tree, ref) => splitPane(tree, id, dir, ref));
  });
}

export function showHereInstead(id: string, ref: PaneRef): void {
  updatePanelLayout((l) => {
    const which = treeOf(l, id);
    if (which === null) return l;
    return loneChatToAll(
      l,
      placeIn(l, which, ref, (tree, r) => replacePane(tree, id, r)),
    );
  });
}

export function closeHere(id: string): void {
  updatePanelLayout((l) => {
    const which = treeOf(l, id);
    if (which === null) return l;
    const tree = closePane(treeIn(l, which), id);
    return tree === treeIn(l, which) ? l : loneChatToAll(l, withTree(l, which, tree));
  });
}

/** Show or hide one pane type from the palette. A pane in the sidebar
 *  closes there while `sidebarShows`, and otherwise moves to the panel
 *  to show. Showing opens the panel too, and a pane already there under
 *  a hidden panel just comes back with it. */
export function togglePane(pane: PaneType, sidebarShows = true): void {
  updatePanelLayout((l) => {
    const ref = paneRef(pane);
    const aside = leafIdFor(l.sidebar, ref);
    if (aside !== null && sidebarShows) {
      return loneChatToAll(l, withTree(l, 'sidebar', closePane(l.sidebar, aside)));
    }
    if (aside !== null) return { ...moveToOtherTree(l, aside), panel_open: true };
    const leaf = leafIdFor(l.root, ref);
    if (leaf !== null && l.panel_open) {
      return loneChatToAll(l, withTree(l, 'panel', closePane(l.root, leaf)));
    }
    const shown = leaf !== null ? l : placeIn(l, 'panel', ref, addPane);
    return { ...shown, panel_open: true };
  });
}

/** Add a pane from the title band, at the bottom of the panel. */
export function addPaneAtBottom(ref: PaneRef): void {
  updatePanelLayout((l) => ({ ...placeIn(l, 'panel', ref, addPane), panel_open: true }));
}

/** Add a pane from the title band's Add to the sidebar, at the bottom
 *  of the sidebar. */
export function addPaneToSidebar(ref: PaneRef): void {
  updatePanelLayout((l) => placeIn(l, 'sidebar', ref, addPane));
}

/** Move the pane `id` to the foot of the other tree, as Move to panel
 *  and Move to sidebar do. A pane moved to a hidden panel opens it. */
export function movePaneAcross(id: string): void {
  updatePanelLayout((l) => {
    const next = moveToOtherTree(l, id);
    return next !== l && treeOf(next, id) === 'panel' ? { ...next, panel_open: true } : next;
  });
}

/** Move the pane `id` where a drag dropped it. */
export function dropPane(id: string, drop: PaneDrop): void {
  updatePanelLayout((l) => {
    const next = moveLeaf(l, id, drop);
    return next !== l && drop.tree === 'panel' ? { ...next, panel_open: true } : next;
  });
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
  updatePanelLayout((l) => setLayoutLeafProps(l, id, patch));
}

/** Hand the caret back to the command line, the way the title band's
 *  menus do when they close. MainWindow listens for this event. */
export function returnToCommandLine(): void {
  window.dispatchEvent(new CustomEvent('vosh:focus-input'));
}
