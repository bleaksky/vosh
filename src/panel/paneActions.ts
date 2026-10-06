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
import { luaPaneRef, luaPanesToAdd, paneTypesToAdd } from './paneTypes';
import { chatFilterOf, chatLeaves } from './chat/chatFilter';
import { pushToast } from '../stores/toasts';

// What a pane's header and menu, the palette and the title band can do
// to the panel. Each action reads the store's current tree when it
// runs, so a menu that stayed open across a profile switch or a drag
// never writes back a stale copy.

/** The leaf a pane renders, handed down by PanelHost. */
export const PaneLeafContext = createContext<PaneLeaf | null>(null);

export function usePaneLeaf(): PaneLeaf | null {
  return useContext(PaneLeafContext);
}

/** The pane a split adds: the first one Add a pane lists, a built-in
 *  pane first and then a Lua pane. */
export function paneToSplitIn(): PaneRef | null {
  const tree = getPanelLayout()?.root ?? null;
  const pane = paneTypesToAdd(tree)[0];
  if (pane !== undefined) return paneRef(pane);
  const lua = luaPanesToAdd(tree)[0];
  return lua === undefined ? null : luaPaneRef(lua);
}

/** What a new Chat pane starts on: tell while another Chat pane shows
 *  and none shows tell yet, so tells land in a pane of their own, and
 *  All otherwise. */
export function chatRefToAdd(tree: PaneNode | null): PaneRef {
  const chats = chatLeaves(tree);
  const onTell = chats.some((leaf) => {
    const filter = chatFilterOf(leaf);
    return filter.kind === 'channel' && filter.channel === 'tell';
  });
  return chats.length > 0 && !onTell
    ? { pane: 'chat', props: { channel: 'tell' } }
    : paneRef('chat');
}

// Place `ref` in `root` with `put`. A new Chat pane starts on what
// chatRefToAdd gives, and when that is tell, every Chat pane on All
// turns to Everything else in the same write, so each tell lands in
// one pane. A toast then says so and offers Undo, which puts those
// panes back on All.
function placePane(
  root: PaneSplit,
  ref: PaneRef,
  put: (tree: PaneSplit, ref: PaneRef) => PaneSplit,
): PaneSplit {
  if (ref.pane !== 'chat') return put(root, ref);
  const chat = chatRefToAdd(root);
  const turned =
    chat.props.channel === 'tell'
      ? chatLeaves(root)
          .filter((leaf) => chatFilterOf(leaf).kind === 'all')
          .map((leaf) => leaf.id)
      : [];
  const base = turned.reduce((tree, id) => setLeafProps(tree, id, { rest: '1' }), root);
  const next = put(base, chat);
  if (next === base) return root;
  if (turned.length > 0) {
    pushToast({
      kind: 'info',
      message:
        turned.length === 1
          ? 'Your other Chat pane now shows Everything else.'
          : 'Your other Chat panes now show Everything else.',
      action: { label: 'Undo', run: () => backToAll(turned) },
    });
  }
  return next;
}

// Undo for placePane: the panes it turned to Everything else go back
// on All, where they still show it.
function backToAll(ids: string[]): void {
  const root = getPanelLayout()?.root;
  if (!root) return;
  const onRest = chatLeaves(root)
    .filter((leaf) => ids.includes(leaf.id) && chatFilterOf(leaf).kind === 'rest')
    .map((leaf) => leaf.id);
  setPaneTree(onRest.reduce((tree, id) => setLeafProps(tree, id, { rest: '' }), root));
}

export function splitHere(id: string, dir: SplitDir): void {
  const root = getPanelLayout()?.root;
  const pane = paneToSplitIn();
  if (!root || !pane) return;
  setPaneTree(placePane(root, pane, (tree, ref) => splitPane(tree, id, dir, ref)));
}

export function showHereInstead(id: string, ref: PaneRef): void {
  const root = getPanelLayout()?.root;
  if (root) setPaneTree(placePane(root, ref, (tree, r) => replacePane(tree, id, r)));
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
export function addPaneAtBottom(ref: PaneRef): void {
  const root = getPanelLayout()?.root;
  if (!root) return;
  const next = placePane(root, ref, addPane);
  updatePanelLayout((l) => ({ ...l, panel_open: true, root: next }));
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
