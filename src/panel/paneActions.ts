import { createContext, useContext } from 'react';
import {
  addPane,
  bothTrees,
  cleanTrees,
  closePane,
  countPanes,
  findNode,
  isLeaf,
  leafIdFor,
  moveToOtherTree,
  otherTree,
  paneCap,
  paneRef,
  sanitize,
  setFolded,
  treeIn,
  treeOf,
  withTree,
  type PaneLayout,
  type PaneLeaf,
  type PaneNode,
  type PaneRef,
  type PaneSplit,
  type PaneTree,
  type PaneType,
} from './paneLayout';
import { getPanelLayout, updatePanelLayout } from './panelLayoutStore';
import { chatFilterOf, chatFilterProps, chatLeaves, checkedChannels } from './chat/chatFilter';
import { pushToast } from '../stores/toasts';
import { profileInFront } from '../stores/session/sessionsStore';

// What a pane's menu and the palette can do to the panel and the
// sidebar, outside arrange mode. Each action reads the store's current
// layout when it runs, so a menu that stayed open across a profile
// switch or a drag never writes back a stale copy. An action on a pane
// finds the tree that holds it by its id, which is unique across both.

/** The leaf a pane renders, handed down by PanelHost. */
export const PaneLeafContext = createContext<PaneLeaf | null>(null);

export function usePaneLeaf(): PaneLeaf | null {
  return useContext(PaneLeafContext);
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
// and a Chat pane needs room across both. A new Chat pane joins as
// readyForChat says, and when the other pane was alone on All, a toast
// says it now shows Everything else.
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
  const chat = chatRefToAdd(bothTrees(base));
  const { layout: ready, turned } = readyForChat(base);
  const tree = put(treeIn(ready, which), chat);
  if (tree === treeIn(ready, which)) return layout;
  if (turned) {
    pushToast({ kind: 'info', message: 'Your other Chat pane now shows Everything else.' });
  }
  return cleanTrees(withTree(ready, which, tree));
}

/** `layout` ready for a new Chat pane to join, which starts on what
 *  chatRefToAdd gives. The pane that shows Everything else from then on
 *  says rest, so the new pane never takes it. `turned` says that pane
 *  was alone on All, which is not offered beside another Chat pane. */
export function readyForChat(layout: PaneLayout): { layout: PaneLayout; turned: boolean } {
  const all = bothTrees(layout);
  const chats = chatLeaves(all);
  const rest = chats.length > 0 ? restCandidate(all) : undefined;
  if (!rest) return { layout, turned: false };
  const ready = setLayoutLeafProps(layout, rest.id, chatFilterProps({ kind: 'rest' }));
  return { layout: ready, turned: ready !== layout && chats.length === 1 };
}

/** `next` with a Chat pane left alone showing All, whatever it showed
 *  beside the others in `before`. */
export function loneChatToAll(before: PaneLayout, next: PaneLayout): PaneLayout {
  const chats = chatLeaves(bothTrees(next));
  if (chats.length !== 1 || chatLeaves(bothTrees(before)).length < 2) return next;
  return setLayoutLeafProps(next, chats[0].id, chatFilterProps({ kind: 'all' }));
}

/** Close the pane `id`, and offer to bring it back where it was in a
 *  toast that names it `label`. */
export function closeWithUndo(id: string, label: string): void {
  const before = getPanelLayout();
  const profile = profileInFront();
  closeHere(id);
  if (!before) return;
  pushToast({
    kind: 'info',
    message: `Closed ${label}`,
    action: {
      label: 'Undo',
      // Only while the same profile is in front, so its panes never land
      // in another's.
      run: () => {
        if (profileInFront() !== profile) return;
        updatePanelLayout((l) => ({ ...l, root: before.root, sidebar: before.sidebar }));
      },
    },
  });
}

/** Fold the pane `id` down to its header, or open it again. */
export function toggleFold(id: string): void {
  updatePanelLayout((l) => {
    const which = treeOf(l, id);
    if (which === null) return l;
    const leaf = findNode(treeIn(l, which), id);
    if (leaf === null || !isLeaf(leaf)) return l;
    return withTree(l, which, setFolded(treeIn(l, which), id, leaf.folded !== true));
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
