import { createContext, useContext } from 'react';
import {
  addPane,
  closePane,
  countPanes,
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
import { chatFilterOf, chatFilterProps, chatLeaves, checkedChannels } from './chat/chatFilter';
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

/** The pane a split of leaf `id` adds. On a Chat pane it is another
 *  Chat while there is room. Elsewhere it is the first pane the panel
 *  does not show, a built-in pane first and then a Lua pane, and
 *  another Chat once the panel shows them all. */
export function paneToSplitIn(id: string): PaneRef | null {
  const tree = getPanelLayout()?.root ?? null;
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

// Place `ref` in `root` with `put`. A new Chat pane starts on what
// chatRefToAdd gives. The pane that shows Everything else from then
// on says rest in the same write, so the new pane never takes it, and
// when that pane was alone on All, a toast says it now shows
// Everything else, since All is not offered beside another Chat pane.
function placePane(
  root: PaneSplit,
  ref: PaneRef,
  put: (tree: PaneSplit, ref: PaneRef) => PaneSplit,
): PaneSplit {
  if (ref.pane !== 'chat') return put(root, ref);
  const chat = chatRefToAdd(root);
  const rest = chatLeaves(root).length > 0 ? restCandidate(root) : undefined;
  const base = rest ? setLeafProps(root, rest.id, chatFilterProps({ kind: 'rest' })) : root;
  const next = put(base, chat);
  if (next === base) return root;
  if (rest && chatLeaves(root).length === 1) {
    pushToast({ kind: 'info', message: 'Your other Chat pane now shows Everything else.' });
  }
  return next;
}

// A Chat pane left alone shows All, whatever it showed beside the
// others.
function loneChatToAll(before: PaneSplit, next: PaneSplit): PaneSplit {
  const chats = chatLeaves(next);
  if (chats.length !== 1 || chatLeaves(before).length < 2) return next;
  return setLeafProps(next, chats[0].id, chatFilterProps({ kind: 'all' }));
}

export function splitHere(id: string, dir: SplitDir): void {
  const root = getPanelLayout()?.root;
  const pane = paneToSplitIn(id);
  if (!root || !pane) return;
  setPaneTree(placePane(root, pane, (tree, ref) => splitPane(tree, id, dir, ref)));
}

export function showHereInstead(id: string, ref: PaneRef): void {
  const root = getPanelLayout()?.root;
  if (root) {
    setPaneTree(
      loneChatToAll(
        root,
        placePane(root, ref, (tree, r) => replacePane(tree, id, r)),
      ),
    );
  }
}

export function closeHere(id: string): void {
  const root = getPanelLayout()?.root;
  if (root) setPaneTree(loneChatToAll(root, closePane(root, id)));
}

/** Show or hide one pane type from the palette. Showing opens the
 *  panel too, and a pane already there under a hidden panel just
 *  comes back with it. */
export function togglePane(pane: PaneType): void {
  updatePanelLayout((l) => {
    const ref = paneRef(pane);
    const leaf = leafIdFor(l.root, ref);
    if (leaf !== null && l.panel_open) {
      return { ...l, root: loneChatToAll(l.root, closePane(l.root, leaf)) };
    }
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
