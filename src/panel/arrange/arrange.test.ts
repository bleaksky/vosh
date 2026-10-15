import { afterEach, describe, expect, it, vi } from 'vitest';
import { defaultLayout, type PaneLayout, type PaneLeaf, type PaneNode } from '../paneLayout';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

const {
  arrangeView,
  dragTo,
  endDrag,
  getArranging,
  keyTarget,
  moveTo,
  putAway,
  putBack,
  shelfItems,
  startArranging,
  startDrag,
  stopArranging,
  undoArranging,
} = await import('./arrange');

const leaf = (pane: string, id = pane): PaneLeaf => ({
  id,
  pane: pane as PaneLeaf['pane'],
  weight: 1,
  props: {},
});

/** Map, Affects and Chat in the panel, Stats and Group in the sidebar. */
function laidOut(): PaneLayout {
  return {
    ...defaultLayout(),
    root: {
      id: 'root',
      split: 'column',
      weight: 1,
      children: [leaf('map'), leaf('affects'), leaf('chat')],
    },
    sidebar: {
      id: 'sidebar',
      split: 'column',
      weight: 1,
      children: [leaf('stats'), leaf('group')],
    },
  };
}

/** A tree as text: each pane by id, each split by its direction. */
function shape(node: PaneNode): string {
  if ('pane' in node) return node.id;
  return `${node.split}(${node.children.map(shape).join(' ')})`;
}

const panes = () => {
  const a = getArranging();
  if (!a) throw new Error('not arranging');
  const view = arrangeView(a).layout;
  return { panel: shape(view.root), sidebar: shape(view.sidebar) };
};

const offers = { builtIns: [], lua: [] };

afterEach(() => {
  stopArranging();
});

describe('arrange mode', () => {
  it('shows the panel while you arrange and starts with nothing to undo', () => {
    startArranging({ ...laidOut(), panel_open: false }, 'Tolliver');
    const a = getArranging();
    expect(a?.draft.panel_open).toBe(true);
    expect(a?.entered.panel_open).toBe(false);
    expect(a?.undo).toEqual([]);
  });

  it('moves a pane above, below or beside another, across the two sides', () => {
    startArranging(laidOut(), 'Tolliver');
    moveTo(leaf('chat'), { kind: 'pane', drop: { tree: 'sidebar', at: 'stats', edge: 'before' } });
    expect(panes()).toEqual({
      panel: 'column(map affects)',
      sidebar: 'column(chat stats group)',
    });
    moveTo(leaf('group'), { kind: 'pane', drop: { tree: 'panel', at: 'affects', edge: 'right' } });
    expect(panes()).toEqual({
      panel: 'column(map row(affects group))',
      sidebar: 'column(chat stats)',
    });
    moveTo(leaf('map'), { kind: 'pane', drop: { tree: 'panel', at: 'group', edge: 'left' } });
    expect(panes().panel).toBe('row(affects map group)');
  });

  it('puts a pane away on the shelf and back home, and Undo takes each step back', () => {
    startArranging(laidOut(), 'Tolliver');
    putAway(leaf('stats'));
    expect(panes().sidebar).toBe('column(group)');
    const a = getArranging();
    if (!a) throw new Error('not arranging');
    const shelf = shelfItems(arrangeView(a).layout, a.putAway, offers);
    expect(shelf.map((i) => [i.leaf.id, i.away])).toEqual([['stats', true]]);
    putBack(shelf[0]);
    expect(panes().sidebar).toBe('column(group stats)');
    undoArranging();
    expect(panes().sidebar).toBe('column(group)');
    undoArranging();
    expect(panes().sidebar).toBe('column(stats group)');
  });

  it('offers on the shelf every pane neither side shows, and another Chat', () => {
    const layout = laidOut();
    const items = shelfItems(layout, [], { builtIns: ['map', 'imm', 'chat'], lua: [] });
    // Map shows already, so only the staff queues and a second Chat.
    expect(items.map((i) => i.label)).toEqual(['Staff queues', 'Another Chat']);
    expect(items[1].leaf.id).not.toBe('chat');
    expect(items[1].leaf.props.channel).toBe('tell');
  });

  it('lets a drag show the layout it would make, and lands it on release', () => {
    startArranging(laidOut(), 'Tolliver');
    startDrag(leaf('map'), 10, 10);
    // Before the targets are measured, the panes close over the gap.
    expect(panes().panel).toBe('column(affects chat)');
    expect(arrangeView(getArranging()!).slot).toBeNull();
    dragTo(20, 20, { kind: 'pane', drop: { tree: 'sidebar', at: 'group', edge: 'after' } });
    expect(panes()).toEqual({
      panel: 'column(affects chat)',
      sidebar: 'column(stats group map)',
    });
    expect(arrangeView(getArranging()!).slot).toBe('map');
    endDrag(true);
    expect(getArranging()?.drag).toBeNull();
    expect(panes().sidebar).toBe('column(stats group map)');
    expect(getArranging()?.undo).toHaveLength(1);
  });

  it('puts the tile back where it was when a drag ends with no target or on Esc', () => {
    startArranging(laidOut(), 'Tolliver');
    startDrag(leaf('map'), 10, 10);
    dragTo(20, 20, null);
    expect(panes().panel).toBe('column(map affects chat)');
    endDrag(true);
    expect(getArranging()?.undo).toEqual([]);
    startDrag(leaf('map'), 10, 10);
    dragTo(20, 20, { kind: 'shelf' });
    endDrag(false);
    expect(panes().panel).toBe('column(map affects chat)');
  });

  it('keeps the writing card where its pin put it', () => {
    const layout = laidOut();
    layout.root.children.push(leaf('writing'));
    startArranging(layout, 'Tolliver');
    moveTo(leaf('writing'), { kind: 'shelf' });
    startDrag(leaf('writing'), 0, 0);
    expect(getArranging()?.drag).toBeNull();
    expect(panes().panel).toBe('column(map affects chat writing)');
  });
});

describe('the keys on a tile', () => {
  const a = { draft: laidOut() };

  it('move a pane past its neighbor with Up and Down', () => {
    expect(keyTarget(a, leaf('affects'), 'ArrowUp', 'right')).toEqual({
      kind: 'pane',
      drop: { tree: 'panel', at: 'map', edge: 'before' },
    });
    expect(keyTarget(a, leaf('affects'), 'ArrowDown', 'right')).toEqual({
      kind: 'pane',
      drop: { tree: 'panel', at: 'chat', edge: 'after' },
    });
    expect(keyTarget(a, leaf('map'), 'ArrowUp', 'right')).toBeNull();
  });

  it('move it to the other side with the arrow toward it, wherever the panel sits', () => {
    const toSidebar = { kind: 'pane', drop: { tree: 'sidebar', at: null, edge: 'after' } };
    expect(keyTarget(a, leaf('map'), 'ArrowLeft', 'right')).toEqual(toSidebar);
    expect(keyTarget(a, leaf('map'), 'ArrowRight', 'right')).toBeNull();
    expect(keyTarget(a, leaf('map'), 'ArrowRight', 'left')).toEqual(toSidebar);
    expect(keyTarget(a, leaf('stats'), 'ArrowRight', 'right')).toEqual({
      kind: 'pane',
      drop: { tree: 'panel', at: null, edge: 'after' },
    });
  });

  it('put it away with Delete or Backspace, and bring a shelf tile back with Enter', () => {
    expect(keyTarget(a, leaf('chat'), 'Delete', 'right')).toEqual({ kind: 'shelf' });
    expect(keyTarget(a, leaf('chat'), 'Backspace', 'right')).toEqual({ kind: 'shelf' });
    expect(keyTarget(a, leaf('imm'), 'Enter', 'right')).toBe('back');
  });
});

describe('a Chat pane from the shelf', () => {
  it('starts on tell and turns the pane on All to Everything else', () => {
    startArranging(laidOut(), 'Tolliver');
    const a = getArranging()!;
    const [chat] = shelfItems(a.draft, a.putAway, { builtIns: ['chat'], lua: [] });
    expect(chat.label).toBe('Another Chat');
    moveTo(chat.leaf, { kind: 'pane', drop: { tree: 'sidebar', at: null, edge: 'after' } });
    const view = arrangeView(getArranging()!).layout;
    expect(view.root.children.find((c) => c.id === 'chat')).toMatchObject({ props: { rest: '1' } });
    expect(view.sidebar.children.at(-1)).toMatchObject({
      pane: 'chat',
      props: { channel: 'tell' },
    });
  });

  it('is not offered again while one you put away waits there', () => {
    startArranging(laidOut(), 'Tolliver');
    putAway(leaf('chat'));
    const a = getArranging()!;
    const items = shelfItems(arrangeView(a).layout, a.putAway, { builtIns: ['chat'], lua: [] });
    expect(items.map((i) => i.label)).toEqual(['Chat']);
    expect(items[0].away).toBe(true);
  });

  it('keeps a pane you put away on the shelf while you drag it over the shelf', () => {
    startArranging(laidOut(), 'Tolliver');
    putAway(leaf('map'));
    startDrag(leaf('map'), 0, 0);
    dragTo(1, 1, { kind: 'shelf' });
    const view = arrangeView(getArranging()!);
    expect(view.putAway.map((p) => p.leaf.id)).toEqual(['map']);
    expect(view.slot).toBe('map');
  });
});

describe('a folded pane in arrange mode', () => {
  it('stays folded where you move it', () => {
    const layout = laidOut();
    layout.root.children[0] = { ...leaf('map'), folded: true };
    startArranging(layout, 'Tolliver');
    // The tile holds the leaf open, as arrange mode lays it out.
    moveTo(leaf('map'), { kind: 'pane', drop: { tree: 'sidebar', at: null, edge: 'after' } });
    expect(arrangeView(getArranging()!).layout.sidebar.children.at(-1)).toMatchObject({
      id: 'map',
      folded: true,
    });
  });
});

describe('the arrow toward the sidebar', () => {
  it('moves nothing while the window has no room to show the sidebar', () => {
    expect(keyTarget({ draft: laidOut() }, leaf('map'), 'ArrowLeft', 'right', false)).toBeNull();
  });
});
